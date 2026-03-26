use std::{env, error::Error, fmt, net::IpAddr, path::PathBuf};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BotMode {
    Production,
    Simulator,
}

impl BotMode {
    fn from_env() -> Result<Self, ConfigError> {
        match env::var("BOT_MODE")
            .unwrap_or_else(|_| "production".to_string())
            .trim()
        {
            "production" => Ok(Self::Production),
            "simulator" => Ok(Self::Simulator),
            value => Err(ConfigError::InvalidMode(value.to_string())),
        }
    }

    pub fn is_simulator(&self) -> bool {
        matches!(self, Self::Simulator)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProductionConfig {
    pub whatsapp_token: String,
    pub whatsapp_phone_id: String,
    pub whatsapp_verify_token: String,
    pub whatsapp_app_secret: String,
    pub menu_image_media_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SimulatorConfig {
    pub upload_dir: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Config {
    pub mode: BotMode,
    pub database_url: String,
    pub advisor_phone: String,
    pub transfer_payment_text: Option<String>,
    pub port: u16,
    pub bind_ip: IpAddr,
    pub production: Option<ProductionConfig>,
    pub simulator: Option<SimulatorConfig>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConfigError {
    MissingVar(&'static str),
    InvalidPort(String),
    InvalidMode(String),
}

impl fmt::Display for ConfigError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingVar(var) => write!(f, "missing required environment variable {var}"),
            Self::InvalidPort(value) => write!(f, "invalid PORT value: {value}"),
            Self::InvalidMode(value) => write!(f, "invalid BOT_MODE value: {value}"),
        }
    }
}

impl Error for ConfigError {}

impl Config {
    pub fn from_env() -> Result<Self, ConfigError> {
        load_dotenv();
        let mode = BotMode::from_env()?;

        let production = match mode {
            BotMode::Production => Some(ProductionConfig {
                whatsapp_token: read_required("WHATSAPP_TOKEN")?,
                whatsapp_phone_id: read_required("WHATSAPP_PHONE_ID")?,
                whatsapp_verify_token: read_required("WHATSAPP_VERIFY_TOKEN")?,
                whatsapp_app_secret: read_required("WHATSAPP_APP_SECRET")?,
                menu_image_media_id: read_required("MENU_IMAGE_MEDIA_ID")?,
            }),
            BotMode::Simulator => None,
        };
        let simulator = match mode {
            BotMode::Production => None,
            BotMode::Simulator => Some(SimulatorConfig {
                upload_dir: PathBuf::from(
                    env::var("SIMULATOR_UPLOAD_DIR")
                        .unwrap_or_else(|_| ".simulator_uploads".to_string()),
                ),
            }),
        };

        Ok(Self {
            mode: mode.clone(),
            database_url: read_required("DATABASE_URL")?,
            advisor_phone: read_required("ADVISOR_PHONE")?,
            transfer_payment_text: read_optional("TRANSFER_PAYMENT_TEXT"),
            port: read_port()?,
            bind_ip: read_bind_ip(mode),
            production,
            simulator,
        })
    }

    pub fn production(&self) -> &ProductionConfig {
        self.production
            .as_ref()
            .expect("production config is only available in production mode")
    }

    pub fn simulator(&self) -> &SimulatorConfig {
        self.simulator
            .as_ref()
            .expect("simulator config is only available in simulator mode")
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

fn read_optional(name: &'static str) -> Option<String> {
    env::var(name).ok()
}

fn read_port() -> Result<u16, ConfigError> {
    match env::var("PORT") {
        Ok(value) => value
            .parse::<u16>()
            .map_err(|_| ConfigError::InvalidPort(value)),
        Err(_) => Ok(8080),
    }
}

fn read_bind_ip(mode: BotMode) -> IpAddr {
    match env::var("BIND_IP") {
        Ok(value) => value
            .parse::<IpAddr>()
            .unwrap_or_else(|_| default_bind_ip(mode)),
        Err(_) => default_bind_ip(mode),
    }
}

fn default_bind_ip(mode: BotMode) -> IpAddr {
    match mode {
        BotMode::Production => IpAddr::V4(std::net::Ipv4Addr::UNSPECIFIED),
        BotMode::Simulator => IpAddr::V4(std::net::Ipv4Addr::LOCALHOST),
    }
}

#[cfg(test)]
mod tests {
    use super::{BotMode, Config, ConfigError};

    fn clear_env() {
        for key in [
            "BOT_MODE",
            "DATABASE_URL",
            "ADVISOR_PHONE",
            "PORT",
            "TRANSFER_PAYMENT_TEXT",
            "WHATSAPP_TOKEN",
            "WHATSAPP_PHONE_ID",
            "WHATSAPP_VERIFY_TOKEN",
            "WHATSAPP_APP_SECRET",
            "MENU_IMAGE_MEDIA_ID",
            "SIMULATOR_UPLOAD_DIR",
            "BIND_IP",
        ] {
            std::env::remove_var(key);
        }
    }

    #[test]
    fn production_mode_requires_whatsapp_vars() {
        clear_env();
        std::env::set_var("DATABASE_URL", "postgres://local");
        std::env::set_var("ADVISOR_PHONE", "573001234567");

        let err = Config::from_env().expect_err("config should fail without whatsapp vars");
        assert!(matches!(err, ConfigError::MissingVar("WHATSAPP_TOKEN")));
    }

    #[test]
    fn simulator_mode_skips_whatsapp_vars() {
        clear_env();
        std::env::set_var("BOT_MODE", "simulator");
        std::env::set_var("DATABASE_URL", "postgres://local");
        std::env::set_var("ADVISOR_PHONE", "573001234567");

        let config = Config::from_env().expect("simulator config should load");
        assert_eq!(config.mode, BotMode::Simulator);
        assert!(config.production.is_none());
        assert_eq!(
            config.simulator().upload_dir,
            std::path::PathBuf::from(".simulator_uploads")
        );
    }
}
