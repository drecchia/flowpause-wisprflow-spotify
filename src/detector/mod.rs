//! Detecção de início e fim do ditado (seção 3 da spec).

mod registry;

use std::fmt;

use anyhow::Result;
use tokio::sync::mpsc;

pub use registry::RegistryDetector;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DictationEvent {
    Started,
    Stopped,
}

impl fmt::Display for DictationEvent {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Started => "ditado iniciou",
            Self::Stopped => "ditado terminou",
        })
    }
}

/// Fonte de eventos de ditado.
pub trait DictationSource {
    /// Começa a observar e envia um evento a cada transição, nunca dois iguais seguidos.
    /// Se o ditado já estiver ativo na partida, o primeiro evento é `Started`.
    /// Retorna logo depois de iniciar; a observação continua em segundo plano até `events` fechar.
    fn start(self, events: mpsc::Sender<DictationEvent>) -> Result<()>;
}
