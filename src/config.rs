use std::{env, error::Error, fmt, net::IpAddr};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Config {
    pub database_url: String,
    pub advisor_phone: String,
    pub transfer_payment_text: Option<String>,
    pub port: u16,
    /// Puerto del segundo listener, solo para `/internal/*`. Railway expone un
    /// único puerto público al edge (`port`, arriba) — este queda alcanzable
    /// solo por la red privada (`trabix-bot.railway.internal:<internal_port>`)
    /// porque nunca se le asigna dominio público. Antes `/internal/*` colgaba
    /// del mismo listener que `/webhook` y el `INTERNAL_API_TOKEN` era la
    /// única protección real contra acceso desde internet.
    pub internal_port: u16,
    pub bind_ip: IpAddr,
    pub whatsapp_token: String,
    pub whatsapp_phone_id: String,
    pub whatsapp_verify_token: String,
    pub whatsapp_app_secret: String,
    pub menu_image_media_id: String,
    pub anthropic_api_key: String,
    pub agent_daily_llm_call_limit: Option<u64>,
    pub waba_id: Option<String>,
    pub capi_dataset_id: Option<String>,
    pub capi_access_token: Option<String>,
    /// Secreto compartido con `crm-app` para `POST /internal/advisor/send`.
    /// Opcional a propósito: sin él el endpoint queda deshabilitado, nunca abierto.
    pub internal_api_token: Option<String>,
    /// Cuántas horas dura la pausa del bot tras un `sendText` desde `crm-app`
    /// (Fase 2, toma de control humana). Ventana deslizante: cada `sendText`
    /// nuevo la reemplaza por `now + N`, no la acumula. Default 6h si la
    /// variable falta o no es un número válido — un typo no debe tumbar el
    /// arranque.
    pub advisor_takeover_hours: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConfigError {
    MissingVar(&'static str),
    InvalidPort(String),
    InvalidInternalPort(String),
    InvalidLlmCallLimit(String),
}

impl fmt::Display for ConfigError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingVar(var) => write!(f, "missing required environment variable {var}"),
            Self::InvalidPort(value) => write!(f, "invalid PORT value: {value}"),
            Self::InvalidInternalPort(value) => {
                write!(f, "invalid INTERNAL_PORT value: {value}")
            }
            Self::InvalidLlmCallLimit(value) => {
                write!(f, "invalid AGENT_DAILY_LLM_CALL_LIMIT value: {value}")
            }
        }
    }
}

impl Error for ConfigError {}

impl Config {
    pub fn from_env() -> Result<Self, ConfigError> {
        load_dotenv();

        Ok(Self {
            database_url: read_required("DATABASE_URL")?,
            advisor_phone: read_required("ADVISOR_PHONE")?,
            transfer_payment_text: read_optional("TRANSFER_PAYMENT_TEXT"),
            port: read_port()?,
            internal_port: read_internal_port()?,
            bind_ip: read_bind_ip(),
            whatsapp_token: read_required("WHATSAPP_TOKEN")?,
            whatsapp_phone_id: read_required("WHATSAPP_PHONE_ID")?,
            whatsapp_verify_token: read_required("WHATSAPP_VERIFY_TOKEN")?,
            whatsapp_app_secret: read_required("WHATSAPP_APP_SECRET")?,
            menu_image_media_id: read_required("MENU_IMAGE_MEDIA_ID")?,
            anthropic_api_key: read_required("ANTHROPIC_API_KEY")?,
            agent_daily_llm_call_limit: read_llm_call_limit()?,
            waba_id: read_optional("WABA_ID"),
            capi_dataset_id: read_optional("META_CAPI_DATASET_ID"),
            capi_access_token: read_optional("META_CAPI_ACCESS_TOKEN"),
            internal_api_token: read_optional("INTERNAL_API_TOKEN"),
            advisor_takeover_hours: read_u64("ADVISOR_TAKEOVER_HOURS", 6),
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

fn read_optional(name: &'static str) -> Option<String> {
    env::var(name).ok()
}

/// Un valor ausente o no parseable cae al default en vez de tumbar el
/// arranque: un typo en una variable no debería dejar el bot sin arrancar.
fn read_u64(name: &'static str, default: u64) -> u64 {
    match env::var(name) {
        Ok(value) => value.trim().parse::<u64>().unwrap_or(default),
        Err(_) => default,
    }
}

fn read_llm_call_limit() -> Result<Option<u64>, ConfigError> {
    match env::var("AGENT_DAILY_LLM_CALL_LIMIT") {
        Ok(value) => value
            .trim()
            .parse::<u64>()
            .map(Some)
            .map_err(|_| ConfigError::InvalidLlmCallLimit(value)),
        Err(_) => Ok(None),
    }
}

fn read_port() -> Result<u16, ConfigError> {
    match env::var("PORT") {
        Ok(value) => value
            .parse::<u16>()
            .map_err(|_| ConfigError::InvalidPort(value)),
        Err(_) => Ok(8080),
    }
}

fn read_internal_port() -> Result<u16, ConfigError> {
    match env::var("INTERNAL_PORT") {
        Ok(value) => value
            .parse::<u16>()
            .map_err(|_| ConfigError::InvalidInternalPort(value)),
        Err(_) => Ok(8081),
    }
}

fn read_bind_ip() -> IpAddr {
    match env::var("BIND_IP") {
        Ok(value) => value
            .parse::<IpAddr>()
            .unwrap_or(IpAddr::V4(std::net::Ipv4Addr::UNSPECIFIED)),
        Err(_) => IpAddr::V4(std::net::Ipv4Addr::UNSPECIFIED),
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{Mutex, OnceLock};

    use super::{Config, ConfigError};

    fn env_lock() -> &'static Mutex<()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(()))
    }

    fn clear_env() {
        for key in [
            "DATABASE_URL",
            "ADVISOR_PHONE",
            "PORT",
            "INTERNAL_PORT",
            "TRANSFER_PAYMENT_TEXT",
            "WHATSAPP_TOKEN",
            "WHATSAPP_PHONE_ID",
            "WHATSAPP_VERIFY_TOKEN",
            "WHATSAPP_APP_SECRET",
            "MENU_IMAGE_MEDIA_ID",
            "BIND_IP",
            "ANTHROPIC_API_KEY",
            "ADVISOR_TAKEOVER_HOURS",
        ] {
            std::env::remove_var(key);
        }
    }

    fn set_whatsapp_vars() {
        std::env::set_var("WHATSAPP_TOKEN", "token");
        std::env::set_var("WHATSAPP_PHONE_ID", "phone-id");
        std::env::set_var("WHATSAPP_VERIFY_TOKEN", "verify");
        std::env::set_var("WHATSAPP_APP_SECRET", "secret");
        std::env::set_var("MENU_IMAGE_MEDIA_ID", "media-id");
    }

    #[test]
    fn requires_whatsapp_vars() {
        let _guard = env_lock().lock().expect("env lock");
        clear_env();
        std::env::set_var("DATABASE_URL", "postgres://local");
        std::env::set_var("ADVISOR_PHONE", "573001234567");

        let err = Config::from_env().expect_err("config should fail without whatsapp vars");
        assert!(matches!(err, ConfigError::MissingVar("WHATSAPP_TOKEN")));
    }

    #[test]
    fn requires_anthropic_api_key() {
        let _guard = env_lock().lock().expect("env lock");
        clear_env();
        std::env::set_var("DATABASE_URL", "postgres://local");
        std::env::set_var("ADVISOR_PHONE", "573001234567");
        set_whatsapp_vars();

        let err = Config::from_env().expect_err("config should require ANTHROPIC_API_KEY");
        assert!(matches!(err, ConfigError::MissingVar("ANTHROPIC_API_KEY")));
    }

    /// Igual criterio que `advisor_whatsapp_defaults_to_enabled`: si la variable
    /// falta o llega con basura, la pausa cae al default en vez de tumbar el
    /// arranque o quedar en un estado indefinido.
    #[test]
    fn advisor_takeover_hours_defaults_and_falls_back() {
        let _guard = env_lock().lock().expect("env lock");
        clear_env();
        std::env::set_var("DATABASE_URL", "postgres://local");
        std::env::set_var("ADVISOR_PHONE", "573001234567");
        std::env::set_var("ANTHROPIC_API_KEY", "sk-test");
        set_whatsapp_vars();

        assert_eq!(Config::from_env().expect("config").advisor_takeover_hours, 6);

        std::env::set_var("ADVISOR_TAKEOVER_HOURS", "no-es-un-numero");
        assert_eq!(
            Config::from_env().expect("config").advisor_takeover_hours,
            6,
            "un valor no parseable debe caer al default"
        );
    }

    #[test]
    fn advisor_takeover_hours_can_be_overridden() {
        let _guard = env_lock().lock().expect("env lock");
        clear_env();
        std::env::set_var("DATABASE_URL", "postgres://local");
        std::env::set_var("ADVISOR_PHONE", "573001234567");
        std::env::set_var("ANTHROPIC_API_KEY", "sk-test");
        set_whatsapp_vars();

        std::env::set_var("ADVISOR_TAKEOVER_HOURS", "12");
        assert_eq!(Config::from_env().expect("config").advisor_takeover_hours, 12);
    }

    #[test]
    fn loads_with_api_key() {
        let _guard = env_lock().lock().expect("env lock");
        clear_env();
        std::env::set_var("DATABASE_URL", "postgres://local");
        std::env::set_var("ADVISOR_PHONE", "573001234567");
        std::env::set_var("ANTHROPIC_API_KEY", "sk-test");
        set_whatsapp_vars();

        let config = Config::from_env().expect("config should load");
        assert_eq!(config.anthropic_api_key.as_str(), "sk-test");
    }
}
