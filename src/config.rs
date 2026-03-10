use std::{env, error::Error, fmt};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Config {
    pub whatsapp_token: String,
    pub whatsapp_phone_id: String,
    pub whatsapp_verify_token: String,
    pub whatsapp_app_secret: String,
    pub database_url: String,
    pub advisor_phone: String,
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

#[cfg(test)]
mod tests {
    use super::{Config, ConfigError};
    use std::sync::{Mutex, OnceLock};

    fn env_lock() -> &'static Mutex<()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(()))
    }

    fn clear_env() {
        for key in [
            "WHATSAPP_TOKEN",
            "WHATSAPP_PHONE_ID",
            "WHATSAPP_VERIFY_TOKEN",
            "WHATSAPP_APP_SECRET",
            "DATABASE_URL",
            "ADVISOR_PHONE",
            "PORT",
        ] {
            std::env::remove_var(key);
        }
    }

    #[test]
    fn loads_config_when_all_required_values_exist() {
        let _guard = env_lock().lock().expect("env mutex poisoned");
        clear_env();

        std::env::set_var("WHATSAPP_TOKEN", "token");
        std::env::set_var("WHATSAPP_PHONE_ID", "phone-id");
        std::env::set_var("WHATSAPP_VERIFY_TOKEN", "verify");
        std::env::set_var("WHATSAPP_APP_SECRET", "secret");
        std::env::set_var("DATABASE_URL", "postgres://db");
        std::env::set_var("ADVISOR_PHONE", "573001234567");

        let config = Config::from_env().expect("config should load");

        assert_eq!(config.port, 8080);
        assert_eq!(config.whatsapp_token, "token");
        assert_eq!(config.whatsapp_phone_id, "phone-id");
    }

    #[test]
    fn returns_descriptive_error_for_missing_var() {
        let _guard = env_lock().lock().expect("env mutex poisoned");
        clear_env();

        let err = Config::from_env().expect_err("config should fail");

        assert_eq!(err, ConfigError::MissingVar("WHATSAPP_TOKEN"));
    }

    #[test]
    fn returns_error_for_invalid_port() {
        let _guard = env_lock().lock().expect("env mutex poisoned");
        clear_env();

        std::env::set_var("WHATSAPP_TOKEN", "token");
        std::env::set_var("WHATSAPP_PHONE_ID", "phone-id");
        std::env::set_var("WHATSAPP_VERIFY_TOKEN", "verify");
        std::env::set_var("WHATSAPP_APP_SECRET", "secret");
        std::env::set_var("DATABASE_URL", "postgres://db");
        std::env::set_var("ADVISOR_PHONE", "573001234567");
        std::env::set_var("PORT", "not-a-port");

        let err = Config::from_env().expect_err("config should fail");

        assert_eq!(err, ConfigError::InvalidPort("not-a-port".to_string()));
    }
}
