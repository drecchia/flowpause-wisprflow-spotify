//! Cliente da Spotify Web API: autenticação, renovação de token e comandos de player.

mod auth;
mod token_store;

use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::{Duration, Instant};

use anyhow::{Context, Result, anyhow};
use reqwest::header::{CONTENT_LENGTH, RETRY_AFTER};
use reqwest::{Method, StatusCode};
use serde::Deserialize;
use serde::de::DeserializeOwned;
use serde_json::json;

use auth::AccessToken;

const API_BASE: &str = "https://api.spotify.com/v1";
const HTTP_TIMEOUT: Duration = Duration::from_secs(5);
/// Maior espera aceita num 429; acima disso a ação é descartada.
const MAX_RETRY_AFTER: Duration = Duration::from_secs(2);
/// A renovação em segundo plano acontece quando falta isso para o token expirar.
const REFRESH_AHEAD_SECS: u64 = 300;
/// Espera antes de tentar de novo uma renovação em segundo plano que falhou.
const REFRESH_RETRY: Duration = Duration::from_secs(30);
/// PING HTTP/2 periódico para a conexão com a API continuar aberta entre ditados.
const KEEP_ALIVE_INTERVAL: Duration = Duration::from_secs(30);

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
    /// Espera imposta por um 429 antes de repetir a chamada, se houve.
    pub retried_after_429: Option<Duration>,
}

impl ApiResponse {
    pub fn json<T: DeserializeOwned>(&self) -> Result<T> {
        serde_json::from_str(&self.body).with_context(|| format!("resposta inesperada de {}", self.request))
    }
}

/// Clonar é barato: os clones dividem a conexão HTTP e o token.
#[derive(Clone)]
pub struct Client {
    inner: Arc<Inner>,
}

struct Inner {
    http: reqwest::Client,
    client_id: String,
    token: Mutex<Option<AccessToken>>,
    /// Serializa renovações: duas em paralelo poderiam invalidar o refresh token.
    refresh_lock: tokio::sync::Mutex<()>,
}

struct Attempt {
    status: StatusCode,
    body: String,
    elapsed: Duration,
    retry_after: Option<Duration>,
}

impl Client {
    pub fn new(client_id: String) -> Result<Self> {
        Ok(Self {
            inner: Arc::new(Inner {
                http: http_client()?,
                client_id,
                token: Mutex::new(token_store::load_access_token()),
                refresh_lock: tokio::sync::Mutex::new(()),
            }),
        })
    }

    pub async fn login(&self) -> Result<()> {
        auth::login(&self.inner.http, &self.inner.client_id).await
    }

    pub async fn player(&self) -> Result<ApiResponse> {
        self.send(Method::GET, "/me/player", &[], None).await
    }

    pub async fn devices(&self) -> Result<ApiResponse> {
        self.send(Method::GET, "/me/player/devices", &[], None).await
    }

    pub async fn pause(&self) -> Result<ApiResponse> {
        self.send(Method::PUT, "/me/player/pause", &[], None).await
    }

    pub async fn play(&self, device_id: &str) -> Result<ApiResponse> {
        self.send(Method::PUT, "/me/player/play", &[("device_id", device_id)], None)
            .await
    }

    pub async fn transfer(&self, device_id: &str) -> Result<ApiResponse> {
        let body = json!({ "device_ids": [device_id], "play": true });
        self.send(Method::PUT, "/me/player", &[], Some(body)).await
    }

    /// Renova o access token em segundo plano antes de expirar, para nenhuma chamada esperar por isso.
    pub fn spawn_token_refresher(&self) {
        let client = self.clone();
        tokio::spawn(async move {
            loop {
                tokio::time::sleep(client.time_until_refresh()).await;
                let stale = client.token_slot().as_ref().map(|t| t.value.clone());
                match client.refresh_token(stale.as_deref()).await {
                    Ok((_, Some(elapsed))) => {
                        logln!("token renovado em segundo plano | {} ms", elapsed.as_millis());
                    }
                    Ok((_, None)) => {}
                    Err(err) => {
                        logln!("falha ao renovar o token em segundo plano: {err:#}");
                        tokio::time::sleep(REFRESH_RETRY).await;
                    }
                }
            }
        });
    }

    /// Envia a chamada com token válido. Em 401, renova e repete uma vez.
    /// Em 429, repete uma vez se `Retry-After` for até 2 s; senão devolve o 429.
    async fn send(
        &self,
        method: Method,
        path: &str,
        query: &[(&str, &str)],
        body: Option<serde_json::Value>,
    ) -> Result<ApiResponse> {
        let request = format!("{method} /v1{path}");
        let mut token_refresh = None;

        let mut token = match self.valid_token() {
            Some(token) => token,
            None => {
                let (token, elapsed) = self.refresh_token(None).await?;
                add_refresh(&mut token_refresh, elapsed);
                token
            }
        };

        let mut attempt = self
            .attempt(&method, path, query, body.as_ref(), &token, &request)
            .await?;

        let mut retried_after_401 = false;
        if attempt.status == StatusCode::UNAUTHORIZED {
            let (new_token, elapsed) = self.refresh_token(Some(&token)).await?;
            add_refresh(&mut token_refresh, elapsed);
            token = new_token;
            attempt = self
                .attempt(&method, path, query, body.as_ref(), &token, &request)
                .await?;
            retried_after_401 = true;
        }

        let mut retried_after_429 = None;
        if attempt.status == StatusCode::TOO_MANY_REQUESTS
            && let Some(wait) = attempt.retry_after
            && wait <= MAX_RETRY_AFTER
        {
            tokio::time::sleep(wait).await;
            attempt = self
                .attempt(&method, path, query, body.as_ref(), &token, &request)
                .await?;
            retried_after_429 = Some(wait);
        }

        Ok(ApiResponse {
            request,
            status: attempt.status,
            body: attempt.body,
            elapsed: attempt.elapsed,
            token_refresh,
            retried_after_401,
            retried_after_429,
        })
    }

    async fn attempt(
        &self,
        method: &Method,
        path: &str,
        query: &[(&str, &str)],
        body: Option<&serde_json::Value>,
        token: &str,
        request: &str,
    ) -> Result<Attempt> {
        let mut builder = self
            .inner
            .http
            .request(method.clone(), format!("{API_BASE}{path}"))
            .bearer_auth(token)
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
        let retry_after = response
            .headers()
            .get(RETRY_AFTER)
            .and_then(|value| value.to_str().ok())
            .and_then(|value| value.trim().parse::<u64>().ok())
            .map(Duration::from_secs);
        let body = response
            .text()
            .await
            .map_err(|err| transport_error(err, request, start.elapsed()))?;
        Ok(Attempt {
            status,
            body,
            elapsed: start.elapsed(),
            retry_after,
        })
    }

    fn token_slot(&self) -> MutexGuard<'_, Option<AccessToken>> {
        self.inner.token.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn valid_token(&self) -> Option<String> {
        self.token_slot()
            .as_ref()
            .filter(|token| !token.expires_soon())
            .map(|token| token.value.clone())
    }

    fn time_until_refresh(&self) -> Duration {
        let secs_left = self.token_slot().as_ref().map_or(0, AccessToken::secs_left);
        Duration::from_secs(secs_left.saturating_sub(REFRESH_AHEAD_SECS))
    }

    /// Renova o token, a menos que outra tarefa já tenha trocado `stale` por um válido.
    /// Devolve o token e, se houve renovação aqui, quanto ela levou.
    async fn refresh_token(&self, stale: Option<&str>) -> Result<(String, Option<Duration>)> {
        let _guard = self.inner.refresh_lock.lock().await;
        if let Some(current) = self.valid_token()
            && Some(current.as_str()) != stale
        {
            return Ok((current, None));
        }
        let refresh_token = token_store::load_refresh_token()?
            .context("sem login salvo; rode `flowpause login`")?;
        let (token, elapsed) =
            auth::refresh(&self.inner.http, &self.inner.client_id, &refresh_token).await?;
        let value = token.value.clone();
        *self.token_slot() = Some(token);
        Ok((value, Some(elapsed)))
    }
}

fn add_refresh(total: &mut Option<Duration>, elapsed: Option<Duration>) {
    if let Some(elapsed) = elapsed {
        *total = Some(total.unwrap_or_default() + elapsed);
    }
}

fn http_client() -> Result<reqwest::Client> {
    reqwest::Client::builder()
        .timeout(HTTP_TIMEOUT)
        .pool_idle_timeout(None)
        .http2_keep_alive_interval(KEEP_ALIVE_INTERVAL)
        .http2_keep_alive_while_idle(true)
        .tcp_keepalive(KEEP_ALIVE_INTERVAL)
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
