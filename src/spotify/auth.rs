//! Authorization Code com PKCE e renovação de token.

use std::collections::HashMap;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result, anyhow, bail};
use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use reqwest::Url;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

use super::{token_store, transport_error};

const AUTHORIZE_URL: &str = "https://accounts.spotify.com/authorize";
const TOKEN_URL: &str = "https://accounts.spotify.com/api/token";
const CALLBACK_ADDR: &str = "127.0.0.1:8888";
const REDIRECT_URI: &str = "http://127.0.0.1:8888/callback";
const SCOPES: &str = "user-read-playback-state user-modify-playback-state";
const LOGIN_TIMEOUT: Duration = Duration::from_secs(300);
/// Renova o access token quando faltar menos que isso para expirar.
const EXPIRY_MARGIN_SECS: u64 = 60;

#[derive(Serialize, Deserialize)]
pub struct AccessToken {
    pub value: String,
    /// Segundos desde a época Unix.
    pub expires_at: u64,
}

impl AccessToken {
    pub fn expires_soon(&self) -> bool {
        now_unix() + EXPIRY_MARGIN_SECS >= self.expires_at
    }
}

#[derive(Deserialize)]
struct TokenResponse {
    access_token: String,
    expires_in: u64,
    refresh_token: Option<String>,
}

/// Fluxo completo de login: abre o navegador, recebe o callback, troca o código e grava os tokens.
pub async fn login(http: &reqwest::Client, client_id: &str) -> Result<()> {
    let verifier = random_urlsafe(32);
    let challenge = URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()).as_slice());
    let state = random_urlsafe(16);

    let listener = TcpListener::bind(CALLBACK_ADDR)
        .await
        .with_context(|| format!("não foi possível escutar em {CALLBACK_ADDR}; a porta está em uso?"))?;

    let url = Url::parse_with_params(
        AUTHORIZE_URL,
        &[
            ("client_id", client_id),
            ("response_type", "code"),
            ("redirect_uri", REDIRECT_URI),
            ("scope", SCOPES),
            ("code_challenge_method", "S256"),
            ("code_challenge", &challenge),
            ("state", &state),
        ],
    )?;
    println!("Abrindo o navegador para autorizar o Spotify. Se não abrir, acesse:\n{url}\n");
    open_browser(url.as_str());

    let code = tokio::time::timeout(LOGIN_TIMEOUT, wait_for_code(&listener, &state))
        .await
        .map_err(|_| anyhow!("nenhum callback recebido em {} s", LOGIN_TIMEOUT.as_secs()))??;

    let (response, elapsed) = token_request(
        http,
        &[
            ("grant_type", "authorization_code"),
            ("code", &code),
            ("redirect_uri", REDIRECT_URI),
            ("client_id", client_id),
            ("code_verifier", &verifier),
        ],
    )
    .await?;
    println!("POST /api/token (authorization_code)\nHTTP 200 OK | {} ms", elapsed.as_millis());

    if response.refresh_token.is_none() {
        bail!("o Spotify não devolveu refresh token");
    }
    store(response)?;
    println!("Login concluído. Refresh token gravado no Gerenciador de Credenciais.");
    Ok(())
}

/// Troca o refresh token por um access token novo e grava o resultado.
pub async fn refresh(
    http: &reqwest::Client,
    client_id: &str,
    refresh_token: &str,
) -> Result<(AccessToken, Duration)> {
    let (response, elapsed) = token_request(
        http,
        &[
            ("grant_type", "refresh_token"),
            ("refresh_token", refresh_token),
            ("client_id", client_id),
        ],
    )
    .await
    .context("falha ao renovar o token; se persistir, rode `flowpause login`")?;
    Ok((store(response)?, elapsed))
}

async fn token_request(
    http: &reqwest::Client,
    form: &[(&str, &str)],
) -> Result<(TokenResponse, Duration)> {
    let start = Instant::now();
    let response = http
        .post(TOKEN_URL)
        .form(form)
        .send()
        .await
        .map_err(|err| transport_error(err, "POST /api/token", start.elapsed()))?;
    let status = response.status();
    let body = response
        .text()
        .await
        .map_err(|err| transport_error(err, "POST /api/token", start.elapsed()))?;
    let elapsed = start.elapsed();
    if !status.is_success() {
        bail!("POST /api/token devolveu HTTP {status} ({} ms): {body}", elapsed.as_millis());
    }
    let parsed = serde_json::from_str(&body).context("resposta inesperada do /api/token")?;
    Ok((parsed, elapsed))
}

/// Grava o refresh token (se veio um novo) e o access token; devolve o access token.
fn store(response: TokenResponse) -> Result<AccessToken> {
    if let Some(refresh_token) = &response.refresh_token {
        token_store::save_refresh_token(refresh_token)?;
    }
    let token = AccessToken {
        value: response.access_token,
        expires_at: now_unix() + response.expires_in,
    };
    token_store::save_access_token(&token)?;
    Ok(token)
}

/// Aceita conexões até chegar `/callback`; devolve o `code` ou o erro informado pelo Spotify.
async fn wait_for_code(listener: &TcpListener, expected_state: &str) -> Result<String> {
    loop {
        let (mut stream, _) = listener.accept().await?;
        let mut buf = vec![0u8; 8192];
        let n = stream.read(&mut buf).await?;
        let request = String::from_utf8_lossy(&buf[..n]);
        let target = request
            .lines()
            .next()
            .and_then(|line| line.split_whitespace().nth(1))
            .unwrap_or("/");
        let Ok(url) = Url::parse(&format!("http://{CALLBACK_ADDR}{target}")) else {
            respond(&mut stream, "400 Bad Request", "Requisição inválida.").await;
            continue;
        };
        if url.path() != "/callback" {
            respond(&mut stream, "404 Not Found", "Não encontrado.").await;
            continue;
        }

        let params: HashMap<String, String> = url.query_pairs().into_owned().collect();
        let result = if let Some(error) = params.get("error") {
            Err(anyhow!("o Spotify recusou a autorização: {error}"))
        } else if params.get("state").map(String::as_str) != Some(expected_state) {
            Err(anyhow!("callback com state inválido; login abortado"))
        } else {
            params
                .get("code")
                .cloned()
                .ok_or_else(|| anyhow!("callback sem o parâmetro code"))
        };

        match &result {
            Ok(_) => respond(&mut stream, "200 OK", "flowpause autorizado. Pode fechar esta aba.").await,
            Err(err) => respond(&mut stream, "400 Bad Request", &format!("Falha no login: {err}")).await,
        }
        return result;
    }
}

async fn respond(stream: &mut TcpStream, status: &str, message: &str) {
    let body = format!("<!doctype html><meta charset=\"utf-8\"><title>flowpause</title><p>{message}</p>");
    let response = format!(
        "HTTP/1.1 {status}\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
    // Falha ao responder ao navegador não afeta o login.
    let _ = stream.write_all(response.as_bytes()).await;
}

fn open_browser(url: &str) {
    // rundll32 repassa a URL sem passar pelo cmd, então `&` na query não quebra.
    if let Err(err) = std::process::Command::new("rundll32")
        .args(["url.dll,FileProtocolHandler", url])
        .spawn()
    {
        eprintln!("aviso: não consegui abrir o navegador ({err}); copie a URL acima");
    }
}

fn random_urlsafe(bytes: usize) -> String {
    let mut buf = vec![0u8; bytes];
    rand::fill(buf.as_mut_slice());
    URL_SAFE_NO_PAD.encode(buf)
}

fn now_unix() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}
