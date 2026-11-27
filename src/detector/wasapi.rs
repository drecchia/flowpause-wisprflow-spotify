//! Detector por sessões de captura de áudio (WASAPI). O ditado está ativo enquanto algum
//! processo cujo caminho contém o padrão tem uma sessão de captura no estado ativo.
//!
//! Observa todos os microfones ativos. Os callbacks COM chegam em threads do sistema e só
//! repassam mensagens; uma thread dedicada é dona do estado e decide as transições.

use std::collections::HashMap;
use std::sync::mpsc as std_mpsc;

use anyhow::{Context, Result, anyhow};
use tokio::sync::mpsc;
use windows::Win32::Foundation::{CloseHandle, PROPERTYKEY};
use windows::Win32::Media::Audio::{
    AudioSessionDisconnectReason, AudioSessionState, AudioSessionStateActive,
    AudioSessionStateExpired, DEVICE_STATE, DEVICE_STATE_ACTIVE, EDataFlow, ERole,
    IAudioSessionControl, IAudioSessionControl2, IAudioSessionEvents, IAudioSessionEvents_Impl,
    IAudioSessionManager2, IAudioSessionNotification, IAudioSessionNotification_Impl,
    IMMDeviceEnumerator, IMMNotificationClient, IMMNotificationClient_Impl, MMDeviceEnumerator,
    eCapture,
};
use windows::Win32::System::Com::{
    CLSCTX_ALL, COINIT_MULTITHREADED, CoCreateInstance, CoInitializeEx, CoTaskMemFree,
};
use windows::Win32::System::Threading::{
    OpenProcess, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION, QueryFullProcessImageNameW,
};
use windows::core::{BOOL, GUID, Interface, PCWSTR, PWSTR, Ref, implement};

use super::{DictationEvent, DictationSource};

pub struct WasapiDetector {
    /// Trecho do caminho do executável, em minúsculas.
    pattern: String,
}

impl WasapiDetector {
    pub fn new(pattern: &str) -> Self {
        Self {
            pattern: pattern.to_lowercase(),
        }
    }
}

impl DictationSource for WasapiDetector {
    fn start(self, events: mpsc::Sender<DictationEvent>) -> Result<()> {
        let (ready_tx, ready_rx) = std_mpsc::channel();
        std::thread::Builder::new()
            .name("detector".into())
            .spawn(move || match Watcher::new(self.pattern, events) {
                Ok(mut watcher) => {
                    let _ = ready_tx.send(Ok(()));
                    watcher.run();
                }
                Err(err) => {
                    let _ = ready_tx.send(Err(err));
                }
            })
            .context("falha ao criar a thread do detector")?;
        ready_rx
            .recv()
            .map_err(|_| anyhow!("a thread do detector parou na partida"))?
    }
}

/// Mensagens dos callbacks COM para a thread do detector.
enum Message {
    SessionCreated(ComPtr<IAudioSessionControl>),
    StateChanged(String, AudioSessionState),
    Disconnected(String),
    DevicesChanged,
}

/// Ponteiro COM levado de uma thread para outra. Todas as threads envolvidas são MTA,
/// onde as interfaces de áudio podem ser usadas de qualquer thread.
struct ComPtr<T>(T);

// SAFETY: ver o comentário de `ComPtr`.
unsafe impl<T> Send for ComPtr<T> {}

struct Endpoint {
    manager: IAudioSessionManager2,
    notifier: IAudioSessionNotification,
}

struct Session {
    control: IAudioSessionControl,
    events: IAudioSessionEvents,
    active: bool,
}

struct Watcher {
    pattern: String,
    events: mpsc::Sender<DictationEvent>,
    messages: std_mpsc::Sender<Message>,
    inbox: std_mpsc::Receiver<Message>,
    enumerator: IMMDeviceEnumerator,
    _device_notifier: IMMNotificationClient,
    endpoints: Vec<Endpoint>,
    /// Sessões de processos que casam com o padrão, pela identificação da instância.
    sessions: HashMap<String, Session>,
    active: bool,
}

impl Watcher {
    fn new(pattern: String, events: mpsc::Sender<DictationEvent>) -> Result<Self> {
        // SAFETY: inicializa COM nesta thread, que vive até o fim do processo.
        unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) }
            .ok()
            .context("falha ao inicializar COM")?;
        // SAFETY: chamadas COM numa thread MTA inicializada.
        let enumerator: IMMDeviceEnumerator =
            unsafe { CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL) }
                .context("falha ao abrir o enumerador de dispositivos de áudio")?;
        let (messages, inbox) = std_mpsc::channel();
        let device_notifier: IMMNotificationClient = DeviceNotifier(messages.clone()).into();
        // SAFETY: o notificador fica guardado no Watcher enquanto estiver registrado.
        unsafe { enumerator.RegisterEndpointNotificationCallback(&device_notifier) }
            .context("falha ao registrar a notificação de dispositivos")?;

        let mut watcher = Self {
            pattern,
            events,
            messages,
            inbox,
            enumerator,
            _device_notifier: device_notifier,
            endpoints: Vec::new(),
            sessions: HashMap::new(),
            active: false,
        };
        watcher.attach_endpoints();
        logln!(
            "detector: {} microfone(s), {} sessão(ões) de captura casam com '{}'",
            watcher.endpoints.len(),
            watcher.sessions.len(),
            watcher.pattern
        );
        Ok(watcher)
    }

    fn run(&mut self) {
        // Na partida, ditado já em andamento vira `Started`.
        if !self.update() {
            return;
        }
        while let Ok(message) = self.inbox.recv() {
            match message {
                Message::SessionCreated(control) => self.track(control.0),
                Message::StateChanged(id, state) => {
                    if state == AudioSessionStateExpired {
                        self.untrack(&id);
                    } else if let Some(session) = self.sessions.get_mut(&id) {
                        session.active = state == AudioSessionStateActive;
                    }
                }
                Message::Disconnected(id) => self.untrack(&id),
                Message::DevicesChanged => {
                    self.detach_all();
                    self.attach_endpoints();
                    logln!(
                        "detector: dispositivos mudaram; {} microfone(s), {} sessão(ões) observadas",
                        self.endpoints.len(),
                        self.sessions.len()
                    );
                }
            }
            if !self.update() {
                return;
            }
        }
    }

    /// Emite evento se o estado agregado mudou. Devolve `false` se o receptor fechou.
    fn update(&mut self) -> bool {
        let active = self.sessions.values().any(|session| session.active);
        if active == self.active {
            return true;
        }
        self.active = active;
        let event = if active {
            DictationEvent::Started
        } else {
            DictationEvent::Stopped
        };
        self.events.blocking_send(event).is_ok()
    }

    fn attach_endpoints(&mut self) {
        // SAFETY: chamadas COM na thread MTA do detector.
        let devices = match unsafe { self.enumerator.EnumAudioEndpoints(eCapture, DEVICE_STATE_ACTIVE) } {
            Ok(devices) => devices,
            Err(err) => {
                logln!("detector: falha ao listar microfones: {err}");
                return;
            }
        };
        let count = unsafe { devices.GetCount() }.unwrap_or(0);
        for index in 0..count {
            if let Err(err) = self.attach_endpoint(&devices, index) {
                logln!("detector: falha ao observar o microfone {index}: {err}");
            }
        }
    }

    fn attach_endpoint(
        &mut self,
        devices: &windows::Win32::Media::Audio::IMMDeviceCollection,
        index: u32,
    ) -> windows::core::Result<()> {
        // SAFETY: chamadas COM na thread MTA do detector; os objetos registrados ficam
        // guardados em `self.endpoints` até o cancelamento do registro.
        unsafe {
            let device = devices.Item(index)?;
            let manager: IAudioSessionManager2 = device.Activate(CLSCTX_ALL, None)?;
            let notifier: IAudioSessionNotification = SessionNotifier(self.messages.clone()).into();
            manager.RegisterSessionNotification(&notifier)?;
            // A notificação de sessão nova só passa a valer depois de enumerar as sessões.
            let sessions = manager.GetSessionEnumerator()?;
            for i in 0..sessions.GetCount()? {
                self.track(sessions.GetSession(i)?);
            }
            self.endpoints.push(Endpoint { manager, notifier });
        }
        Ok(())
    }

    fn track(&mut self, control: IAudioSessionControl) {
        if let Err(err) = self.try_track(control) {
            logln!("detector: falha ao observar uma sessão de captura: {err}");
        }
    }

    fn try_track(&mut self, control: IAudioSessionControl) -> windows::core::Result<()> {
        let control2: IAudioSessionControl2 = control.cast()?;
        // SAFETY: chamadas COM na thread MTA do detector.
        let pid = unsafe { control2.GetProcessId() }.unwrap_or(0);
        if pid == 0 {
            return Ok(()); // sessão do sistema ou de vários processos
        }
        let Some(path) = process_path(pid) else {
            return Ok(());
        };
        if !path.to_lowercase().contains(&self.pattern) {
            return Ok(());
        }
        let id = session_id(&control2)?;
        if self.sessions.contains_key(&id) {
            return Ok(());
        }
        let events: IAudioSessionEvents = SessionEvents {
            id: id.clone(),
            messages: self.messages.clone(),
        }
        .into();
        // SAFETY: `events` fica guardado em `self.sessions` até o cancelamento do registro.
        // Registra antes de ler o estado: uma mudança no meio chega como mensagem.
        let state = unsafe {
            control.RegisterAudioSessionNotification(&events)?;
            control.GetState()?
        };
        let active = state == AudioSessionStateActive;
        logln!(
            "detector: sessão de captura de {path} (pid {pid}), {}",
            if active { "ativa" } else { "inativa" }
        );
        self.sessions.insert(
            id,
            Session {
                control,
                events,
                active,
            },
        );
        Ok(())
    }

    fn untrack(&mut self, id: &str) {
        if let Some(session) = self.sessions.remove(id) {
            // SAFETY: cancela o registro feito em `try_track` com o mesmo objeto.
            let _ = unsafe { session.control.UnregisterAudioSessionNotification(&session.events) };
        }
    }

    fn detach_all(&mut self) {
        let ids: Vec<String> = self.sessions.keys().cloned().collect();
        for id in ids {
            self.untrack(&id);
        }
        for endpoint in self.endpoints.drain(..) {
            // SAFETY: cancela o registro feito em `attach_endpoint` com o mesmo objeto.
            let _ = unsafe { endpoint.manager.UnregisterSessionNotification(&endpoint.notifier) };
        }
    }
}

fn session_id(control: &IAudioSessionControl2) -> windows::core::Result<String> {
    // SAFETY: a string vem alocada pelo COM e é liberada aqui depois de copiada.
    unsafe {
        let raw = control.GetSessionInstanceIdentifier()?;
        let id = raw.to_string().unwrap_or_default();
        CoTaskMemFree(Some(raw.as_ptr() as *const _));
        Ok(id)
    }
}

fn process_path(pid: u32) -> Option<String> {
    // SAFETY: o handle é fechado antes de sair; o buffer tem o tamanho informado.
    unsafe {
        let handle = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid).ok()?;
        let mut buffer = [0u16; 1024];
        let mut len = buffer.len() as u32;
        let result = QueryFullProcessImageNameW(
            handle,
            PROCESS_NAME_WIN32,
            PWSTR(buffer.as_mut_ptr()),
            &mut len,
        );
        let _ = CloseHandle(handle);
        result.ok()?;
        Some(String::from_utf16_lossy(&buffer[..len as usize]))
    }
}

#[implement(IAudioSessionNotification)]
struct SessionNotifier(std_mpsc::Sender<Message>);

impl IAudioSessionNotification_Impl for SessionNotifier_Impl {
    fn OnSessionCreated(&self, newsession: Ref<IAudioSessionControl>) -> windows::core::Result<()> {
        if let Ok(session) = newsession.ok() {
            let _ = self.0.send(Message::SessionCreated(ComPtr(session.clone())));
        }
        Ok(())
    }
}

#[implement(IAudioSessionEvents)]
struct SessionEvents {
    id: String,
    messages: std_mpsc::Sender<Message>,
}

impl IAudioSessionEvents_Impl for SessionEvents_Impl {
    fn OnDisplayNameChanged(&self, _: &PCWSTR, _: *const GUID) -> windows::core::Result<()> {
        Ok(())
    }

    fn OnIconPathChanged(&self, _: &PCWSTR, _: *const GUID) -> windows::core::Result<()> {
        Ok(())
    }

    fn OnSimpleVolumeChanged(&self, _: f32, _: BOOL, _: *const GUID) -> windows::core::Result<()> {
        Ok(())
    }

    fn OnChannelVolumeChanged(
        &self,
        _: u32,
        _: *const f32,
        _: u32,
        _: *const GUID,
    ) -> windows::core::Result<()> {
        Ok(())
    }

    fn OnGroupingParamChanged(&self, _: *const GUID, _: *const GUID) -> windows::core::Result<()> {
        Ok(())
    }

    fn OnStateChanged(&self, newstate: AudioSessionState) -> windows::core::Result<()> {
        let _ = self
            .messages
            .send(Message::StateChanged(self.id.clone(), newstate));
        Ok(())
    }

    fn OnSessionDisconnected(&self, _: AudioSessionDisconnectReason) -> windows::core::Result<()> {
        let _ = self.messages.send(Message::Disconnected(self.id.clone()));
        Ok(())
    }
}

#[implement(IMMNotificationClient)]
struct DeviceNotifier(std_mpsc::Sender<Message>);

impl IMMNotificationClient_Impl for DeviceNotifier_Impl {
    fn OnDeviceStateChanged(&self, _: &PCWSTR, _: DEVICE_STATE) -> windows::core::Result<()> {
        let _ = self.0.send(Message::DevicesChanged);
        Ok(())
    }

    fn OnDeviceAdded(&self, _: &PCWSTR) -> windows::core::Result<()> {
        let _ = self.0.send(Message::DevicesChanged);
        Ok(())
    }

    fn OnDeviceRemoved(&self, _: &PCWSTR) -> windows::core::Result<()> {
        let _ = self.0.send(Message::DevicesChanged);
        Ok(())
    }

    fn OnDefaultDeviceChanged(&self, _: EDataFlow, _: ERole, _: &PCWSTR) -> windows::core::Result<()> {
        Ok(())
    }

    fn OnPropertyValueChanged(&self, _: &PCWSTR, _: &PROPERTYKEY) -> windows::core::Result<()> {
        Ok(())
    }
}
