//! Máquina de estados da seção 5 da spec: pausa no início do ditado e retoma no mesmo
//! dispositivo depois que ele termina. Casos de borda da seção 6.

mod player;
#[cfg(test)]
mod tests;

use std::future::Future;
use std::time::Duration;

use anyhow::{Result, bail};
use reqwest::StatusCode;
use tokio::sync::mpsc;
use tokio::time::Instant;

use crate::detector::DictationEvent;

pub use player::Player;

/// Como saber, no início do ditado, se havia música tocando.
#[derive(Clone, Copy, Debug)]
#[allow(dead_code)] // só uma estratégia fica ativa; ver PAUSE_STRATEGY em main.rs
pub enum PauseStrategy {
    /// `pause` sem consulta prévia: 2xx = havia música, 403 = não havia.
    /// Só vale se a fase 0 confirmou o 403 ao pausar o que já está pausado (H2).
    PauseFirst,
    /// `GET /v1/me/player` antes, e só pausa se `is_playing`. Custa uma chamada a mais.
    CheckFirst,
}

pub struct Config {
    pub strategy: PauseStrategy,
    /// Espera entre o fim do ditado e a retomada.
    pub resume_delay: Duration,
    /// Acima disso a retomada automática é cancelada.
    pub max_pause: Duration,
}

/// O que é preciso para retomar: só o dispositivo onde a música foi pausada.
struct Saved {
    device_id: String,
    device_name: String,
    paused_at: Instant,
}

enum State {
    Idle,
    /// Ditado em andamento, nada a retomar.
    NoResume,
    /// Ditado em andamento, música pausada pelo programa.
    Paused(Saved),
    /// Ditado terminou; retoma no prazo se nada mudar.
    Waiting(Saved, Instant),
}

impl State {
    fn name(&self) -> &'static str {
        match self {
            Self::Idle => "Ocioso",
            Self::NoResume => "SemRetomada",
            Self::Paused(_) => "Pausado",
            Self::Waiting(..) => "Aguardando",
        }
    }
}

pub struct Controller<P> {
    player: P,
    config: Config,
    state: State,
}

impl<P: Player> Controller<P> {
    pub fn new(player: P, config: Config) -> Self {
        Self {
            player,
            config,
            state: State::Idle,
        }
    }

    /// Processa eventos até `shutdown` terminar ou a fonte de eventos fechar.
    /// Nos dois casos, se a música estiver pausada pelo programa, tenta retomar antes de sair.
    pub async fn run(
        mut self,
        mut events: mpsc::Receiver<DictationEvent>,
        shutdown: impl Future<Output = ()>,
    ) -> Result<()> {
        tokio::pin!(shutdown);
        loop {
            let deadline = match &self.state {
                State::Waiting(_, deadline) => Some(*deadline),
                _ => None,
            };
            tokio::select! {
                () = &mut shutdown => {
                    logln!("encerrando");
                    self.shutdown().await;
                    return Ok(());
                }
                event = events.recv() => match event {
                    Some(event) => self.on_event(event).await,
                    None => {
                        self.shutdown().await;
                        bail!("a fonte de eventos de ditado parou");
                    }
                },
                () = sleep_until(deadline) => self.on_deadline().await,
            }
        }
    }

    async fn on_event(&mut self, event: DictationEvent) {
        logln!("{event}");
        let from = self.state.name();
        let next = match (std::mem::replace(&mut self.state, State::Idle), event) {
            (State::Idle, DictationEvent::Started) => self.begin_pause().await,
            (State::Waiting(saved, _), DictationEvent::Started) => {
                logln!("novo ditado durante a espera: retomada cancelada, música segue pausada");
                State::Paused(saved)
            }
            (State::Paused(saved), DictationEvent::Stopped) => {
                let deadline = Instant::now() + self.config.resume_delay;
                State::Waiting(saved, deadline)
            }
            (State::NoResume, DictationEvent::Stopped) => State::Idle,
            (state, _) => state,
        };
        self.set_state(from, next);
    }

    async fn on_deadline(&mut self) {
        let from = self.state.name();
        if let State::Waiting(saved, _) = std::mem::replace(&mut self.state, State::Idle) {
            self.resume(saved).await;
        }
        self.set_state(from, State::Idle);
    }

    async fn shutdown(&mut self) {
        if let State::Paused(saved) | State::Waiting(saved, _) =
            std::mem::replace(&mut self.state, State::Idle)
        {
            logln!("a música está pausada pelo programa: tentando retomar antes de sair");
            self.resume(saved).await;
        }
    }

    fn set_state(&mut self, from: &'static str, next: State) {
        if from != next.name() {
            logln!("estado: {from} -> {}", next.name());
        }
        self.state = next;
    }

    async fn begin_pause(&self) -> State {
        match self.config.strategy {
            PauseStrategy::PauseFirst => self.pause_first().await,
            PauseStrategy::CheckFirst => self.check_first().await,
        }
    }

    async fn pause_first(&self) -> State {
        let Some(status) = retry_once("pause", || self.player.pause()).await else {
            return State::NoResume;
        };
        if status == StatusCode::FORBIDDEN {
            logln!("pause recusado (403): nada tocava");
            return State::NoResume;
        }
        if !status.is_success() {
            logln!("pause devolveu HTTP {status}: sem retomada");
            return State::NoResume;
        }
        let paused_at = Instant::now();
        // Havia música: descobrir em qual dispositivo, para retomar nele.
        match retry_once("consulta do estado", || self.player.playback()).await {
            Some(Some(playback)) => match playback.device_id {
                Some(device_id) => paused(device_id, playback.device_name, paused_at),
                None => {
                    logln!("aviso: dispositivo {} sem id; não será retomado", playback.device_name);
                    State::NoResume
                }
            },
            Some(None) => {
                logln!("aviso: pausado, mas o Spotify não informa dispositivo ativo; não será retomado");
                State::NoResume
            }
            None => State::NoResume,
        }
    }

    async fn check_first(&self) -> State {
        let Some(playback) = retry_once("consulta do estado", || self.player.playback()).await
        else {
            return State::NoResume;
        };
        let Some(playback) = playback.filter(|p| p.is_playing) else {
            logln!("nada tocando: sem pausa");
            return State::NoResume;
        };
        if playback.is_restricted {
            logln!("aviso: dispositivo {} é restrito; sem ação", playback.device_name);
            return State::NoResume;
        }
        let Some(device_id) = playback.device_id else {
            logln!("aviso: dispositivo {} sem id; sem ação", playback.device_name);
            return State::NoResume;
        };
        let Some(status) = retry_once("pause", || self.player.pause()).await else {
            return State::NoResume;
        };
        if status.is_success() {
            paused(device_id, playback.device_name, Instant::now())
        } else {
            logln!("pause devolveu HTTP {status}: sem retomada");
            State::NoResume
        }
    }

    /// Retoma no dispositivo guardado, salvo se a pausa foi longa demais ou o usuário já mexeu.
    async fn resume(&self, saved: Saved) {
        let paused_for = saved.paused_at.elapsed();
        if paused_for > self.config.max_pause {
            logln!(
                "pausa durou {} s, acima de {} s: sem retomada",
                paused_for.as_secs(),
                self.config.max_pause.as_secs()
            );
            return;
        }

        let playback = match self.player.playback().await {
            Ok(playback) => playback,
            Err(err) => {
                logln!("falha ao consultar o estado ({err:#}): sem retomada");
                return;
            }
        };
        if let Some(playback) = playback {
            if playback.is_playing {
                logln!("já está tocando em {}: sem retomada", playback.device_name);
                return;
            }
            if playback.device_id.as_deref() != Some(saved.device_id.as_str()) {
                logln!(
                    "dispositivo ativo mudou para {}: sem retomada",
                    playback.device_name
                );
                return;
            }
        }

        match self.player.play(&saved.device_id).await {
            Ok(status) if status.is_success() => logln!("retomado em {}", saved.device_name),
            Ok(StatusCode::NOT_FOUND) => {
                logln!("{} fora da lista (404): tentando transferência", saved.device_name);
                match self.player.transfer(&saved.device_id).await {
                    Ok(status) if status.is_success() => {
                        logln!("retomado em {} por transferência", saved.device_name);
                    }
                    Ok(status) => logln!("transferência devolveu HTTP {status}: desistindo"),
                    Err(err) => logln!("falha na transferência ({err:#}): desistindo"),
                }
            }
            Ok(status) => logln!("play devolveu HTTP {status}: desistindo"),
            Err(err) => logln!("falha no play ({err:#}): desistindo"),
        }
    }
}

fn paused(device_id: String, device_name: String, paused_at: Instant) -> State {
    logln!("pausado em {device_name}");
    State::Paused(Saved {
        device_id,
        device_name,
        paused_at,
    })
}

/// Falha de transporte: uma nova tentativa imediata; se falhar de novo, segue sem a ação.
async fn retry_once<T, F, Fut>(what: &str, call: F) -> Option<T>
where
    F: Fn() -> Fut,
    Fut: Future<Output = Result<T>>,
{
    match call().await {
        Ok(value) => return Some(value),
        Err(err) => logln!("{what} falhou ({err:#}); tentando de novo"),
    }
    match call().await {
        Ok(value) => Some(value),
        Err(err) => {
            logln!("{what} falhou de novo ({err:#}); seguindo sem ação");
            None
        }
    }
}

async fn sleep_until(deadline: Option<Instant>) {
    match deadline {
        Some(deadline) => tokio::time::sleep_until(deadline).await,
        None => std::future::pending().await,
    }
}
