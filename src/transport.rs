use std::path::PathBuf;

use crate::whatsapp::client::WhatsAppClient;

#[derive(Debug, Clone)]
pub struct SimulatorTransport {
    pub menu_image_path: PathBuf,
}

#[derive(Debug, Clone)]
pub enum OutboundTransport {
    Production(WhatsAppClient),
    Simulator(SimulatorTransport),
}

impl OutboundTransport {
    pub fn is_simulator(&self) -> bool {
        matches!(self, Self::Simulator(_))
    }

    pub fn simulator(&self) -> Option<&SimulatorTransport> {
        match self {
            Self::Simulator(transport) => Some(transport),
            Self::Production(_) => None,
        }
    }

    pub fn production(&self) -> Option<&WhatsAppClient> {
        match self {
            Self::Production(client) => Some(client),
            Self::Simulator(_) => None,
        }
    }
}
