use crate::whatsapp::client::WhatsAppClient;
pub const SIMULATOR_MENU_ASSET_PATH: &str = "assets/trabix-menu.png";

#[derive(Debug, Clone)]
pub enum OutboundTransport {
    Production(WhatsAppClient),
    Simulator,
}

impl OutboundTransport {
    pub fn is_simulator(&self) -> bool {
        matches!(self, Self::Simulator)
    }

    pub fn production(&self) -> Option<&WhatsAppClient> {
        match self {
            Self::Production(client) => Some(client),
            Self::Simulator => None,
        }
    }
}
