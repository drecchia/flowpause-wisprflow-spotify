#[macro_use]
mod logging;
mod controller;
mod detector;
mod spotify;

use std::process::ExitCode;
use std::time::Duration;

use anyhow::{Context, Result, bail};
use clap::{Parser, Subcommand};
use reqwest::StatusCode;
use tokio::sync::mpsc;

use controller::{Config, Controller, PauseStrategy, Player};
use detector::{DictationSource, RegistryDetector};
use spotify::{ApiResponse, Device, DeviceList, PlaybackState};

/// `docs/FASE0-RESULTADOS.md` não registra o 403 ao pausar o que já está pausado (H2 não testada),
/// então o controlador consulta o estado antes de pausar.
const PAUSE_STRATEGY: PauseStrategy = PauseStrategy::CheckFirst;

/// flowpause: pausa o Spotify durante o ditado do Wispr Flow e retoma no mesmo dispositivo.
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
    /// Só o detector: imprime cada início e fim de ditado
    Watch {
        /// Trecho do nome da subchave do registro (sem diferenciar maiúsculas)
        #[arg(long = "match", default_value = "wispr")]
        pattern: String,
    },
    /// Detector + controlador: pausa durante o ditado e retoma depois. Ctrl+C para sair
    Run {
        /// Espera entre o fim do ditado e a retomada
        #[arg(long, default_value_t = 2000)]
        resume_delay_ms: u64,
        /// Acima disso a retomada automática é cancelada
        #[arg(long, default_value_t = 300)]
        max_pause_secs: u64,
        /// Trecho do nome da subchave do registro (sem diferenciar maiúsculas)
        #[arg(long = "match", default_value = "wispr")]
        pattern: String,
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
    let response = match command {
        Command::Watch { pattern } => {
            watch(&pattern).await?;
            return Ok(ExitCode::SUCCESS);
        }
        Command::Run {
            resume_delay_ms,
            max_pause_secs,
            pattern,
        } => {
            let config = Config {
                strategy: PAUSE_STRATEGY,
                resume_delay: Duration::from_millis(resume_delay_ms),
                max_pause: Duration::from_secs(max_pause_secs),
            };
            run_controller(spotify_client()?, config, &pattern).await?;
            return Ok(ExitCode::SUCCESS);
        }
        Command::Login => {
            spotify_client()?.login().await?;
            return Ok(ExitCode::SUCCESS);
        }
        Command::Status => {
            let response = spotify_client()?.player().await?;
            report(&response);
            match response.status {
                StatusCode::OK => print_playback(&response.json()?),
                StatusCode::NO_CONTENT => println!("nenhuma reprodução ativa"),
                _ => {}
            }
            response
        }
        Command::Devices => {
            let response = spotify_client()?.devices().await?;
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
        Command::Pause => report_only(spotify_client()?.pause().await?),
        Command::Play { device } => report_only(spotify_client()?.play(&device).await?),
        Command::Transfer { device } => report_only(spotify_client()?.transfer(&device).await?),
    };

    Ok(if response.status.is_success() {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    })
}

fn spotify_client() -> Result<spotify::Client> {
    let client_id = std::env::var("FLOWPAUSE_CLIENT_ID")
        .ok()
        .filter(|id| !id.trim().is_empty())
        .context("FLOWPAUSE_CLIENT_ID não definida; veja o README")?;
    spotify::Client::new(client_id)
}

async fn watch(pattern: &str) -> Result<()> {
    let (events, mut receiver) = mpsc::channel(16);
    RegistryDetector::new(pattern).start(events)?;
    logln!("observando o microfone. Ctrl+C para sair.");
    while let Some(event) = receiver.recv().await {
        logln!("{event}");
    }
    bail!("o detector parou")
}

async fn run_controller(client: spotify::Client, config: Config, pattern: &str) -> Result<()> {
    logln!(
        "flowpause: estratégia {:?}, espera {} ms, pausa máxima {} s",
        config.strategy,
        config.resume_delay.as_millis(),
        config.max_pause.as_secs()
    );
    // Valida o login e abre a conexão HTTPS antes do primeiro ditado.
    client
        .playback()
        .await
        .context("falha na consulta inicial ao Spotify")?;
    client.spawn_token_refresher();

    let (events, receiver) = mpsc::channel(16);
    RegistryDetector::new(pattern).start(events)?;
    logln!("pronto. Ctrl+C para sair.");

    let shutdown = async {
        if tokio::signal::ctrl_c().await.is_err() {
            std::future::pending::<()>().await;
        }
    };
    Controller::new(client, config).run(receiver, shutdown).await
}

fn report(response: &ApiResponse) {
    println!("{}", response.request);
    if let Some(refresh) = response.token_refresh {
        println!("token renovado | {} ms", refresh.as_millis());
    }
    if response.retried_after_401 {
        println!("primeira tentativa: HTTP 401, repetida com token novo");
    }
    if let Some(wait) = response.retried_after_429 {
        println!("primeira tentativa: HTTP 429, repetida após {} ms", wait.as_millis());
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
