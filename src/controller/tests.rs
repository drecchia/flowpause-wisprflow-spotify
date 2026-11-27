//! Testes da máquina de estados com player falso e relógio do tokio pausado.

use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Duration;

use anyhow::{Result, bail};
use reqwest::StatusCode;
use tokio::sync::{mpsc, oneshot};
use tokio::time::sleep;

use super::player::Playback;
use super::{Config, Controller, PauseStrategy, Player};
use crate::detector::DictationEvent::{self, Started, Stopped};
use crate::detector::DictationSource;

const ECHO: &str = "echo";
const PC: &str = "pc";
const STRATEGIES: [PauseStrategy; 2] = [PauseStrategy::PauseFirst, PauseStrategy::CheckFirst];

#[derive(Clone, Debug, PartialEq)]
enum Call {
    Pause,
    Playback,
    Play(String),
    Transfer(String),
}

/// Conta Spotify simulada: um dispositivo ativo que toca ou não.
#[derive(Default)]
struct World {
    playing: bool,
    active: Option<String>,
    /// Quantas chamadas de pause ainda falham com erro de transporte.
    pause_failures: usize,
    /// Status forçado para o play; `None` simula sucesso.
    play_status: Option<StatusCode>,
    transfer_status: Option<StatusCode>,
    calls: Vec<Call>,
}

#[derive(Clone)]
struct FakePlayer(Arc<Mutex<World>>);

impl FakePlayer {
    fn playing_on(device: &str) -> Self {
        Self(Arc::new(Mutex::new(World {
            playing: true,
            active: Some(device.into()),
            ..World::default()
        })))
    }

    fn world(&self) -> MutexGuard<'_, World> {
        self.0.lock().unwrap()
    }

    fn count(&self, call: &Call) -> usize {
        self.world().calls.iter().filter(|c| *c == call).count()
    }

    fn resumes(&self) -> Vec<Call> {
        self.world()
            .calls
            .iter()
            .filter(|c| matches!(c, Call::Play(_) | Call::Transfer(_)))
            .cloned()
            .collect()
    }
}

impl Player for FakePlayer {
    async fn pause(&self) -> Result<StatusCode> {
        let mut world = self.world();
        world.calls.push(Call::Pause);
        if world.pause_failures > 0 {
            world.pause_failures -= 1;
            bail!("timeout simulado");
        }
        if world.playing {
            world.playing = false;
            Ok(StatusCode::OK)
        } else {
            Ok(StatusCode::FORBIDDEN)
        }
    }

    async fn playback(&self) -> Result<Option<Playback>> {
        let mut world = self.world();
        world.calls.push(Call::Playback);
        Ok(world.active.clone().map(|id| Playback {
            is_playing: world.playing,
            device_name: id.clone(),
            device_id: Some(id),
            is_restricted: false,
        }))
    }

    async fn play(&self, device_id: &str) -> Result<StatusCode> {
        let mut world = self.world();
        world.calls.push(Call::Play(device_id.into()));
        if let Some(status) = world.play_status {
            return Ok(status);
        }
        world.active = Some(device_id.into());
        world.playing = true;
        Ok(StatusCode::OK)
    }

    async fn transfer(&self, device_id: &str) -> Result<StatusCode> {
        let mut world = self.world();
        world.calls.push(Call::Transfer(device_id.into()));
        Ok(world.transfer_status.unwrap_or(StatusCode::NO_CONTENT))
    }
}

/// Detector falso: envia eventos de um roteiro com esperas entre eles.
struct ScriptedSource(Vec<(Duration, DictationEvent)>);

impl DictationSource for ScriptedSource {
    fn start(self, events: mpsc::Sender<DictationEvent>) -> Result<()> {
        tokio::spawn(async move {
            for (wait, event) in self.0 {
                sleep(wait).await;
                if events.send(event).await.is_err() {
                    return;
                }
            }
            // Mantém o canal aberto: fechar encerraria o controlador.
            std::future::pending::<()>().await;
        });
        Ok(())
    }
}

/// Controlador rodando em segundo plano, com o canal de eventos na mão do teste.
struct Harness {
    events: mpsc::Sender<DictationEvent>,
    shutdown: Option<oneshot::Sender<()>>,
    done: tokio::task::JoinHandle<Result<()>>,
}

impl Harness {
    fn start(strategy: PauseStrategy, player: &FakePlayer) -> Self {
        let (events, receiver) = mpsc::channel(16);
        let (shutdown, shutdown_rx) = oneshot::channel();
        let config = Config {
            strategy,
            resume_delay: Duration::from_millis(2000),
            max_pause: Duration::from_secs(300),
        };
        let controller = Controller::new(player.clone(), config);
        let done = tokio::spawn(controller.run(receiver, async {
            let _ = shutdown_rx.await;
        }));
        Self {
            events,
            shutdown: Some(shutdown),
            done,
        }
    }

    async fn send(&self, event: DictationEvent) {
        self.events.send(event).await.unwrap();
        // Deixa o controlador processar antes de o teste seguir.
        sleep(Duration::from_millis(1)).await;
    }

    /// Um ditado completo que dura `length`.
    async fn dictate(&self, length: Duration) {
        self.send(Started).await;
        sleep(length).await;
        self.send(Stopped).await;
    }

    async fn stop(mut self) {
        let _ = self.shutdown.take().unwrap().send(());
        self.done.await.unwrap().unwrap();
    }
}

fn secs(n: u64) -> Duration {
    Duration::from_secs(n)
}

#[tokio::test(start_paused = true)]
async fn music_playing_pauses_and_resumes_on_same_device() {
    for strategy in STRATEGIES {
        let player = FakePlayer::playing_on(ECHO);
        let harness = Harness::start(strategy, &player);

        harness.dictate(secs(5)).await;
        assert!(!player.world().playing, "{strategy:?}: deveria estar pausado");
        assert!(player.resumes().is_empty(), "{strategy:?}: retomou antes da espera");

        sleep(secs(3)).await;
        assert_eq!(player.count(&Call::Pause), 1, "{strategy:?}");
        assert_eq!(player.resumes(), vec![Call::Play(ECHO.into())], "{strategy:?}");
        assert!(player.world().playing, "{strategy:?}");
        harness.stop().await;
    }
}

#[tokio::test(start_paused = true)]
async fn nothing_playing_never_plays() {
    for strategy in STRATEGIES {
        let player = FakePlayer::playing_on(ECHO);
        player.world().playing = false;
        let harness = Harness::start(strategy, &player);

        harness.dictate(secs(5)).await;
        sleep(secs(10)).await;
        assert!(player.resumes().is_empty(), "{strategy:?}");
        assert!(!player.world().playing, "{strategy:?}");
        if let PauseStrategy::CheckFirst = strategy {
            assert_eq!(player.count(&Call::Pause), 0, "consulta prévia não deve pausar");
        }
        harness.stop().await;
    }
}

#[tokio::test(start_paused = true)]
async fn three_quick_dictations_pause_once_and_resume_once() {
    for strategy in STRATEGIES {
        let player = FakePlayer::playing_on(ECHO);
        let harness = Harness::start(strategy, &player);

        for _ in 0..3 {
            harness.dictate(secs(3)).await;
            sleep(secs(1)).await; // menos que os 2 s de espera
        }
        assert!(player.resumes().is_empty(), "{strategy:?}: retomou entre ditados");

        sleep(secs(3)).await;
        assert_eq!(player.count(&Call::Pause), 1, "{strategy:?}");
        assert_eq!(player.resumes(), vec![Call::Play(ECHO.into())], "{strategy:?}");
        harness.stop().await;
    }
}

#[tokio::test(start_paused = true)]
async fn manual_play_during_pause_skips_resume() {
    for strategy in STRATEGIES {
        let player = FakePlayer::playing_on(ECHO);
        let harness = Harness::start(strategy, &player);

        harness.send(Started).await;
        player.world().playing = true; // usuário deu play
        harness.send(Stopped).await;
        sleep(secs(3)).await;
        assert!(player.resumes().is_empty(), "{strategy:?}");
        harness.stop().await;
    }
}

#[tokio::test(start_paused = true)]
async fn device_change_during_pause_skips_resume() {
    for strategy in STRATEGIES {
        let player = FakePlayer::playing_on(ECHO);
        let harness = Harness::start(strategy, &player);

        harness.send(Started).await;
        player.world().active = Some(PC.into()); // usuário trocou para o PC, ainda pausado
        harness.send(Stopped).await;
        sleep(secs(3)).await;
        assert!(player.resumes().is_empty(), "{strategy:?}");
        harness.stop().await;
    }
}

#[tokio::test(start_paused = true)]
async fn pause_longer_than_max_skips_resume() {
    for strategy in STRATEGIES {
        let player = FakePlayer::playing_on(ECHO);
        let harness = Harness::start(strategy, &player);

        harness.dictate(secs(301)).await;
        sleep(secs(3)).await;
        assert_eq!(player.count(&Call::Pause), 1, "{strategy:?}");
        assert!(player.resumes().is_empty(), "{strategy:?}");
        harness.stop().await;
    }
}

#[tokio::test(start_paused = true)]
async fn play_404_tries_one_transfer_to_same_device() {
    for strategy in STRATEGIES {
        let player = FakePlayer::playing_on(ECHO);
        player.world().play_status = Some(StatusCode::NOT_FOUND);
        player.world().transfer_status = Some(StatusCode::NOT_FOUND);
        let harness = Harness::start(strategy, &player);

        harness.dictate(secs(5)).await;
        sleep(secs(3)).await;
        assert_eq!(
            player.resumes(),
            vec![Call::Play(ECHO.into()), Call::Transfer(ECHO.into())],
            "{strategy:?}"
        );
        harness.stop().await;
    }
}

#[tokio::test(start_paused = true)]
async fn pause_transport_error_is_retried_once() {
    for strategy in STRATEGIES {
        let player = FakePlayer::playing_on(ECHO);
        player.world().pause_failures = 1;
        let harness = Harness::start(strategy, &player);

        harness.dictate(secs(5)).await;
        sleep(secs(3)).await;
        assert_eq!(player.count(&Call::Pause), 2, "{strategy:?}");
        assert_eq!(player.resumes(), vec![Call::Play(ECHO.into())], "{strategy:?}");
        harness.stop().await;
    }
}

#[tokio::test(start_paused = true)]
async fn pause_failing_twice_gives_up_without_resume() {
    for strategy in STRATEGIES {
        let player = FakePlayer::playing_on(ECHO);
        player.world().pause_failures = 2;
        let harness = Harness::start(strategy, &player);

        harness.dictate(secs(5)).await;
        sleep(secs(3)).await;
        assert_eq!(player.count(&Call::Pause), 2, "{strategy:?}");
        assert!(player.resumes().is_empty(), "{strategy:?}");
        harness.stop().await;
    }
}

#[tokio::test(start_paused = true)]
async fn shutdown_while_paused_resumes_before_exit() {
    for strategy in STRATEGIES {
        let player = FakePlayer::playing_on(ECHO);
        let harness = Harness::start(strategy, &player);

        harness.send(Started).await;
        harness.stop().await;
        assert_eq!(player.resumes(), vec![Call::Play(ECHO.into())], "{strategy:?}");
    }
}

#[tokio::test(start_paused = true)]
async fn scripted_source_drives_controller() {
    let player = FakePlayer::playing_on(ECHO);
    let (events, receiver) = mpsc::channel(16);
    ScriptedSource(vec![
        (secs(1), Started),
        (secs(4), Stopped),
        (secs(1), Started),
        (secs(4), Stopped),
    ])
    .start(events)
    .unwrap();
    let config = Config {
        strategy: PauseStrategy::CheckFirst,
        resume_delay: secs(2),
        max_pause: secs(300),
    };
    tokio::spawn(Controller::new(player.clone(), config).run(receiver, std::future::pending()));

    sleep(secs(20)).await;
    assert_eq!(player.count(&Call::Pause), 1);
    assert_eq!(player.resumes(), vec![Call::Play(ECHO.into())]);
}
