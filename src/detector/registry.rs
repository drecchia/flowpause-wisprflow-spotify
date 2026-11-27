//! Detector pelo registro: o Windows grava em `ConsentStore\microphone` quando cada app
//! abre e fecha o microfone. `LastUsedTimeStop` igual a 0 significa microfone em uso.

use std::io;
use std::ptr;

use anyhow::{Context, Result};
use tokio::sync::mpsc;
use windows_sys::Win32::Foundation::{CloseHandle, ERROR_SUCCESS, FALSE, HANDLE, TRUE, WAIT_OBJECT_0};
use windows_sys::Win32::System::Registry::{REG_NOTIFY_CHANGE_LAST_SET, RegNotifyChangeKeyValue};
use windows_sys::Win32::System::Threading::{CreateEventW, INFINITE, WaitForSingleObject};
use winreg::RegKey;
use winreg::enums::{HKEY_CURRENT_USER, KEY_READ};

use super::{DictationEvent, DictationSource};

const MICROPHONE_KEY: &str =
    r"Software\Microsoft\Windows\CurrentVersion\CapabilityAccessManager\ConsentStore\microphone";
/// Apps desktop ficam aqui, uma subchave por executável; apps empacotados ficam direto em `microphone`.
const NON_PACKAGED: &str = "NonPackaged";

pub struct RegistryDetector {
    /// Trecho do nome da subchave, em minúsculas.
    pattern: String,
}

struct Scan {
    matched: usize,
    active: bool,
}

impl RegistryDetector {
    pub fn new(pattern: &str) -> Self {
        Self {
            pattern: pattern.to_lowercase(),
        }
    }

    /// Reenumera as subchaves a cada leitura: uma atualização do app cria uma entrada nova.
    fn scan(&self, microphone: &RegKey) -> io::Result<Scan> {
        let mut scan = Scan {
            matched: 0,
            active: false,
        };
        for name in microphone.enum_keys() {
            let name = name?;
            if name.eq_ignore_ascii_case(NON_PACKAGED) {
                let non_packaged = microphone.open_subkey(&name)?;
                for app in non_packaged.enum_keys() {
                    self.check(&non_packaged, &app?, &mut scan);
                }
            } else {
                self.check(microphone, &name, &mut scan);
            }
        }
        Ok(scan)
    }

    fn check(&self, parent: &RegKey, name: &str, scan: &mut Scan) {
        if !name.to_lowercase().contains(&self.pattern) {
            return;
        }
        scan.matched += 1;
        // Subchave sem LastUsedTimeStop, ou que sumiu durante a leitura, conta como livre.
        let in_use = parent
            .open_subkey(name)
            .and_then(|key| key.get_value::<u64, _>("LastUsedTimeStop"))
            .is_ok_and(|stop| stop == 0);
        scan.active |= in_use;
    }

    fn watch(self, microphone: RegKey, events: mpsc::Sender<DictationEvent>) {
        let event = match Event::new() {
            Ok(event) => event,
            Err(err) => {
                logln!("detector: falha ao criar o evento de notificação: {err}");
                return;
            }
        };
        let mut active = false;
        loop {
            // Arma antes de ler: uma mudança entre a leitura e a espera não se perde.
            if let Err(err) = arm(&microphone, &event) {
                logln!("detector: falha ao registrar a notificação do registro: {err}");
                return;
            }
            match self.scan(&microphone) {
                Ok(scan) if scan.active != active => {
                    active = scan.active;
                    let dictation = if active {
                        DictationEvent::Started
                    } else {
                        DictationEvent::Stopped
                    };
                    if events.blocking_send(dictation).is_err() {
                        return;
                    }
                }
                Ok(_) => {}
                Err(err) => logln!("detector: falha ao ler o registro: {err}"),
            }
            if let Err(err) = event.wait() {
                logln!("detector: falha ao esperar a notificação: {err}");
                return;
            }
        }
    }
}

impl DictationSource for RegistryDetector {
    fn start(self, events: mpsc::Sender<DictationEvent>) -> Result<()> {
        let microphone = RegKey::predef(HKEY_CURRENT_USER)
            .open_subkey_with_flags(MICROPHONE_KEY, KEY_READ)
            .with_context(|| format!("não foi possível abrir HKCU\\{MICROPHONE_KEY}"))?;
        let scan = self
            .scan(&microphone)
            .context("não foi possível ler as subchaves do microfone")?;
        logln!(
            "detector: {} subchave(s) casam com '{}'; microfone {}",
            scan.matched,
            self.pattern,
            if scan.active { "em uso" } else { "livre" }
        );
        if scan.matched == 0 {
            logln!("detector: aviso: nenhuma subchave casa; nenhum ditado será detectado até o app usar o microfone");
        }
        // A notificação do registro é presa à thread que a armou, então a thread é dedicada.
        std::thread::Builder::new()
            .name("detector".into())
            .spawn(move || self.watch(microphone, events))
            .context("falha ao criar a thread do detector")?;
        Ok(())
    }
}

/// Notificação assíncrona de disparo único sobre a chave e toda a subárvore.
fn arm(key: &RegKey, event: &Event) -> io::Result<()> {
    // SAFETY: a chave e o evento são handles válidos enquanto `key` e `event` existirem.
    let status = unsafe {
        RegNotifyChangeKeyValue(key.raw_handle(), TRUE, REG_NOTIFY_CHANGE_LAST_SET, event.0, TRUE)
    };
    if status == ERROR_SUCCESS {
        Ok(())
    } else {
        Err(io::Error::from_raw_os_error(status as i32))
    }
}

/// Evento Win32 com reset automático.
struct Event(HANDLE);

impl Event {
    fn new() -> io::Result<Self> {
        // SAFETY: sem atributos de segurança nem nome; o handle é fechado no Drop.
        let handle = unsafe { CreateEventW(ptr::null(), FALSE, FALSE, ptr::null()) };
        if handle.is_null() {
            Err(io::Error::last_os_error())
        } else {
            Ok(Self(handle))
        }
    }

    fn wait(&self) -> io::Result<()> {
        // SAFETY: o handle é válido até o Drop.
        match unsafe { WaitForSingleObject(self.0, INFINITE) } {
            WAIT_OBJECT_0 => Ok(()),
            _ => Err(io::Error::last_os_error()),
        }
    }
}

impl Drop for Event {
    fn drop(&mut self) {
        // SAFETY: o handle veio de CreateEventW e é fechado uma vez só.
        unsafe { CloseHandle(self.0) };
    }
}
