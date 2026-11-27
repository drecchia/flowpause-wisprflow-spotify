//! O que o controlador precisa do Spotify, e a implementação sobre a Web API.

use anyhow::{Result, bail};
use reqwest::StatusCode;

use crate::spotify::{self, ApiResponse, PlaybackState};

pub struct Playback {
    pub is_playing: bool,
    pub device_id: Option<String>,
    pub device_name: String,
    pub is_restricted: bool,
}

/// Comandos de player. `Err` é falha de transporte (rede, timeout); status HTTP vem em `Ok`.
pub trait Player {
    async fn pause(&self) -> Result<StatusCode>;
    /// `None` quando não há reprodução ativa na conta (HTTP 204).
    async fn playback(&self) -> Result<Option<Playback>>;
    async fn play(&self, device_id: &str) -> Result<StatusCode>;
    async fn transfer(&self, device_id: &str) -> Result<StatusCode>;
}

impl Player for spotify::Client {
    async fn pause(&self) -> Result<StatusCode> {
        Ok(logged(spotify::Client::pause(self).await?).status)
    }

    async fn playback(&self) -> Result<Option<Playback>> {
        let response = logged(spotify::Client::player(self).await?);
        match response.status {
            StatusCode::OK => {
                let state: PlaybackState = response.json()?;
                Ok(Some(Playback {
                    is_playing: state.is_playing,
                    device_id: state.device.id,
                    device_name: state.device.name,
                    is_restricted: state.device.is_restricted,
                }))
            }
            StatusCode::NO_CONTENT => Ok(None),
            status => bail!("{} devolveu HTTP {status}", response.request),
        }
    }

    async fn play(&self, device_id: &str) -> Result<StatusCode> {
        Ok(logged(spotify::Client::play(self, device_id).await?).status)
    }

    async fn transfer(&self, device_id: &str) -> Result<StatusCode> {
        Ok(logged(spotify::Client::transfer(self, device_id).await?).status)
    }
}

fn logged(response: ApiResponse) -> ApiResponse {
    let mut notes = String::new();
    if let Some(refresh) = response.token_refresh {
        notes += &format!(" | token renovado em {} ms", refresh.as_millis());
    }
    if response.retried_after_401 {
        notes += " | repetida após 401";
    }
    if let Some(wait) = response.retried_after_429 {
        notes += &format!(" | repetida após 429 ({} ms)", wait.as_millis());
    }
    logln!(
        "{} -> HTTP {} | {} ms{notes}",
        response.request,
        response.status,
        response.elapsed.as_millis()
    );
    if !response.status.is_success() && !response.body.is_empty() {
        logln!("corpo do erro: {}", response.body);
    }
    response
}
