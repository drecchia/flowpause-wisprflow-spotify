mod spotify;

use std::process::ExitCode;

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use reqwest::StatusCode;

use spotify::{ApiResponse, Device, DeviceList, PlaybackState};

/// flowpause, fase 0: valida o controle do Spotify (Echo Dot e PC) pela Web API.
#[derive(Parser)]
#[command(version)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Autoriza no Spotify (PKCE) e grava o refresh token no Gerenciador de Credenciais
    Login,
    /// GET /v1/me/player: estado da reprodução e dispositivo ativo
    Status,
    /// GET /v1/me/player/devices: lista os dispositivos da conta
    Devices,
    /// PUT /v1/me/player/pause
    Pause,
    /// PUT /v1/me/player/play?device_id=<id>
    Play {
        #[arg(long)]
        device: String,
    },
    /// PUT /v1/me/player com {"device_ids":["<id>"],"play":true}
    Transfer {
        #[arg(long)]
        device: String,
    },
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> ExitCode {
    let cli = Cli::parse();
    match run(cli.command).await {
        Ok(code) => code,
        Err(err) => {
            eprintln!("erro: {err:#}");
            ExitCode::FAILURE
        }
    }
}

async fn run(command: Command) -> Result<ExitCode> {
    let client_id = std::env::var("FLOWPAUSE_CLIENT_ID")
        .ok()
        .filter(|id| !id.trim().is_empty())
        .context("FLOWPAUSE_CLIENT_ID não definida; veja o README")?;
    let mut client = spotify::Client::new(client_id)?;

    let response = match command {
        Command::Login => {
            client.login().await?;
            return Ok(ExitCode::SUCCESS);
        }
        Command::Status => {
            let response = client.player().await?;
            report(&response);
            match response.status {
                StatusCode::OK => print_playback(&response.json()?),
                StatusCode::NO_CONTENT => println!("nenhuma reprodução ativa"),
                _ => {}
            }
            response
        }
        Command::Devices => {
            let response = client.devices().await?;
            report(&response);
            if response.status == StatusCode::OK {
                let list: DeviceList = response.json()?;
                if list.devices.is_empty() {
                    println!("nenhum dispositivo na lista");
                }
                for device in &list.devices {
                    println!();
                    print_device(device);
                }
            }
            response
        }
        Command::Pause => report_only(client.pause().await?),
        Command::Play { device } => report_only(client.play(&device).await?),
        Command::Transfer { device } => report_only(client.transfer(&device).await?),
    };

    Ok(if response.status.is_success() {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    })
}

fn report(response: &ApiResponse) {
    println!("{}", response.request);
    if let Some(refresh) = response.token_refresh {
        println!("token renovado | {} ms", refresh.as_millis());
    }
    if response.retried_after_401 {
        println!("primeira tentativa: HTTP 401, repetida com token novo");
    }
    println!("HTTP {} | {} ms", response.status, response.elapsed.as_millis());
    if !response.status.is_success() && !response.body.is_empty() {
        println!("corpo do erro: {}", response.body);
    }
}

fn report_only(response: ApiResponse) -> ApiResponse {
    report(&response);
    response
}

fn print_playback(state: &PlaybackState) {
    println!();
    println!("is_playing: {}", state.is_playing);
    print_device(&state.device);
}

fn print_device(device: &Device) {
    println!("id: {}", device.id.as_deref().unwrap_or("(nulo)"));
    println!("name: {}", device.name);
    println!("type: {}", device.kind);
    println!("is_active: {}", device.is_active);
    println!("is_restricted: {}", device.is_restricted);
    println!("supports_volume: {}", device.supports_volume);
}
