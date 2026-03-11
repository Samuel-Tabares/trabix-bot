use std::{env, error::Error, fmt};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Config {
    pub whatsapp_token: String,
    pub whatsapp_phone_id: String,
    pub whatsapp_verify_token: String,
    pub whatsapp_app_secret: String,
    pub database_url: String,
    pub advisor_phone: String,
    pub transfer_payment_text: String,
    pub menu_image_media_id: String,
    pub port: u16,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConfigError {
    MissingVar(&'static str),
    InvalidPort(String),
}

impl fmt::Display for ConfigError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingVar(var) => write!(f, "missing required environment variable {var}"),
            Self::InvalidPort(value) => write!(f, "invalid PORT value: {value}"),
        }
    }
}

impl Error for ConfigError {}

impl Config {
    pub fn from_env() -> Result<Self, ConfigError> {
        load_dotenv();

        Ok(Self {
            whatsapp_token: read_required("WHATSAPP_TOKEN")?,
            whatsapp_phone_id: read_required("WHATSAPP_PHONE_ID")?,
            whatsapp_verify_token: read_required("WHATSAPP_VERIFY_TOKEN")?,
            whatsapp_app_secret: read_required("WHATSAPP_APP_SECRET")?,
            database_url: read_required("DATABASE_URL")?,
            advisor_phone: read_required("ADVISOR_PHONE")?,
            transfer_payment_text: read_required("TRANSFER_PAYMENT_TEXT")?,
            menu_image_media_id: read_required("MENU_IMAGE_MEDIA_ID")?,
            port: read_port()?,
        })
    }
}

#[cfg(not(test))]
fn load_dotenv() {
    let _ = dotenvy::dotenv();
}

#[cfg(test)]
fn load_dotenv() {}

fn read_required(name: &'static str) -> Result<String, ConfigError> {
    env::var(name).map_err(|_| ConfigError::MissingVar(name))
}

fn read_port() -> Result<u16, ConfigError> {
    match env::var("PORT") {
        Ok(value) => value
            .parse::<u16>()
            .map_err(|_| ConfigError::InvalidPort(value)),
        Err(_) => Ok(8080),
    }
}