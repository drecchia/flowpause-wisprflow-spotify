//! Cliente da Spotify Web API: autenticação, renovação de token e comandos de player.

mod auth;
mod token_store;

use std::time::{Duration, Instant};

use anyhow::{Context, Result, anyhow};
use reqwest::header::CONTENT_LENGTH;
use reqwest::{Method, StatusCode};
use serde::Deserialize;
use serde::de::DeserializeOwned;
use serde_json::json;

use auth::AccessToken;

const API_BASE: &str = "https://api.spotify.com/v1";
const HTTP_TIMEOUT: Duration = Duration::from_secs(5);

#[derive(Deserialize)]
pub struct Device {
    pub id: Option<String>,
    pub name: String,
    #[serde(rename = "type")]
    pub kind: String,
    pub is_active: bool,
    pub is_restricted: bool,
    pub supports_volume: bool,
}

#[derive(Deserialize)]
pub struct PlaybackState {
    pub is_playing: bool,
    pub device: Device,
}

#[derive(Deserialize)]
pub struct DeviceList {
    pub devices: Vec<Device>,
}

/// Resultado de uma chamada à Web API, com as medidas que a fase 0 registra.
pub struct ApiResponse {
    /// Método e caminho, por exemplo `PUT /v1/me/player/pause`.
    pub request: String,
    pub status: StatusCode,
    pub body: String,
    /// Duração da última tentativa, sem contar renovação de token.
    pub elapsed: Duration,
    /// Tempo gasto renovando o access token dentro desta chamada, se houve renovação.
    pub token_refresh: Option<Duration>,
    /// A primeira tentativa recebeu 401 e a chamada foi repetida com token novo.
    pub retried_after_401: bool,
}

impl ApiResponse {
    pub fn json<T: DeserializeOwned>(&self) -> Result<T> {
        serde_json::from_str(&self.body).with_context(|| format!("resposta inesperada de {}", self.request))
    }
}

pub struct Client {
    http: reqwest::Client,
    client_id: String,
    token: Option<AccessToken>,
}

impl Client {
    pub fn new(client_id: String) -> Result<Self> {
        Ok(Self {
            http: http_client()?,
            client_id,
            token: token_store::load_access_token(),
        })
    }

    pub async fn login(&self) -> Result<()> {
        auth::login(&self.http, &self.client_id).await
    }

    pub async fn player(&mut self) -> Result<ApiResponse> {
        self.send(Method::GET, "/me/player", &[], None).await
    }

    pub async fn devices(&mut self) -> Result<ApiResponse> {
        self.send(Method::GET, "/me/player/devices", &[], None).await
    }

    pub async fn pause(&mut self) -> Result<ApiResponse> {
        self.send(Method::PUT, "/me/player/pause", &[], None).await
    }

    pub async fn play(&mut self, device_id: &str) -> Result<ApiResponse> {
        self.send(Method::PUT, "/me/player/play", &[("device_id", device_id)], None)
            .await
    }

    pub async fn transfer(&mut self, device_id: &str) -> Result<ApiResponse> {
        let body = json!({ "device_ids": [device_id], "play": true });
        self.send(Method::PUT, "/me/player", &[], Some(body)).await
    }

    /// Envia a chamada com token válido; em 401, renova e repete uma vez.
    async fn send(
        &mut self,
        method: Method,
        path: &str,
        query: &[(&str, &str)],
        body: Option<serde_json::Value>,
    ) -> Result<ApiResponse> {
        let request = format!("{method} /v1{path}");
        let mut token_refresh = Duration::ZERO;
        let mut refreshed = false;

        if self.token.as_ref().is_none_or(AccessToken::expires_soon) {
            token_refresh += self.refresh().await?;
            refreshed = true;
        }

        let mut attempt = self.attempt(&method, path, query, body.as_ref(), &request).await?;
        let mut retried_after_401 = false;
        if attempt.0 == StatusCode::UNAUTHORIZED {
            token_refresh += self.refresh().await?;
            refreshed = true;
            attempt = self.attempt(&method, path, query, body.as_ref(), &request).await?;
            retried_after_401 = true;
        }

        let (status, body, elapsed) = attempt;
        Ok(ApiResponse {
            request,
            status,
            body,
            elapsed,
            token_refresh: refreshed.then_some(token_refresh),
            retried_after_401,
        })
    }

    async fn attempt(
        &self,
        method: &Method,
        path: &str,
        query: &[(&str, &str)],
        body: Option<&serde_json::Value>,
        request: &str,
    ) -> Result<(StatusCode, String, Duration)> {
        let token = self
            .token
            .as_ref()
            .ok_or_else(|| anyhow!("sem access token"))?;
        let mut builder = self
            .http
            .request(method.clone(), format!("{API_BASE}{path}"))
            .bearer_auth(&token.value)
            .query(query);
        builder = match body {
            Some(body) => builder.json(body),
            // O Spotify pode recusar PUT sem corpo e sem Content-Length (411).
            None if *method == Method::PUT => builder.header(CONTENT_LENGTH, "0"),
            None => builder,
        };

        let start = Instant::now();
        let response = builder
            .send()
            .await
            .map_err(|err| transport_error(err, request, start.elapsed()))?;
        let status = response.status();
        let text = response
            .text()
            .await
            .map_err(|err| transport_error(err, request, start.elapsed()))?;
        Ok((status, text, start.elapsed()))
    }

    async fn refresh(&mut self) -> Result<Duration> {
        let refresh_token = token_store::load_refresh_token()?
            .context("sem login salvo; rode `flowpause login`")?;
        let (token, elapsed) = auth::refresh(&self.http, &self.client_id, &refresh_token).await?;
        self.token = Some(token);
        Ok(elapsed)
    }
}

fn http_client() -> Result<reqwest::Client> {
    reqwest::Client::builder()
        .timeout(HTTP_TIMEOUT)
        .build()
        .context("falha ao criar o cliente HTTP")
}

fn transport_error(err: reqwest::Error, request: &str, elapsed: Duration) -> anyhow::Error {
    let ms = elapsed.as_millis();
    if err.is_timeout() {
        anyhow!("{request}: sem resposta em {} s ({ms} ms)", HTTP_TIMEOUT.as_secs())
    } else if err.is_connect() {
        anyhow!("{request}: falha de conexão após {ms} ms: {err}")
    } else {
        anyhow::Error::new(err).context(format!("{request}: falha após {ms} ms"))
    }
}
