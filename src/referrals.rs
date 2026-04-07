use std::{collections::BTreeSet, error::Error, fmt, fs, path::Path, sync::OnceLock};

use serde::Deserialize;

pub const DEFAULT_REFERRALS_PATH: &str = "config/referrals.toml";

static REFERRAL_REGISTRY: OnceLock<ReferralRegistry> = OnceLock::new();

#[derive(Debug)]
pub enum ReferralRegistryError {
    Io {
        path: String,
        source: std::io::Error,
    },
    Parse(toml::de::Error),
    Validation(String),
    AlreadyInitialized,
}

impl fmt::Display for ReferralRegistryError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io { path, source } => {
                write!(f, "failed to read referral registry {path}: {source}")
            }
            Self::Parse(source) => write!(f, "failed to parse referral registry: {source}"),
            Self::Validation(message) => write!(f, "invalid referral registry: {message}"),
            Self::AlreadyInitialized => write!(f, "referral registry was already initialized"),
        }
    }
}

impl Error for ReferralRegistryError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Io { source, .. } => Some(source),
            Self::Parse(source) => Some(source),
            Self::Validation(_) | Self::AlreadyInitialized => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReferralRegistry {
    codes: BTreeSet<String>,
}

#[derive(Debug, Deserialize)]
struct ReferralRegistryFile {
    codes: Vec<String>,
}

impl ReferralRegistry {
    pub fn load_default() -> Result<Self, ReferralRegistryError> {
        Self::from_path(DEFAULT_REFERRALS_PATH)
    }

    pub fn from_path(path: impl AsRef<Path>) -> Result<Self, ReferralRegistryError> {
        let path = path.as_ref();
        let raw = fs::read_to_string(path).map_err(|source| ReferralRegistryError::Io {
            path: path.display().to_string(),
            source,
        })?;
        Self::from_toml_str(&raw)
    }

    pub fn contains(&self, code: &str) -> bool {
        self.codes.contains(code)
    }

    fn from_toml_str(raw: &str) -> Result<Self, ReferralRegistryError> {
        let parsed: ReferralRegistryFile =
            toml::from_str(raw).map_err(ReferralRegistryError::Parse)?;

        let mut codes = BTreeSet::new();
        for code in parsed.codes {
            let normalized = normalize_referral_code(&code);
            if normalized.is_empty() {
                return Err(ReferralRegistryError::Validation(
                    "codes cannot be empty".to_string(),
                ));
            }
            if normalized != code {
                return Err(ReferralRegistryError::Validation(format!(
                    "code `{code}` must already be trimmed lowercase"
                )));
            }
            if normalized.chars().any(char::is_whitespace) {
                return Err(ReferralRegistryError::Validation(format!(
                    "code `{code}` cannot contain whitespace"
                )));
            }
            if !codes.insert(normalized.clone()) {
                return Err(ReferralRegistryError::Validation(format!(
                    "duplicate code `{normalized}`"
                )));
            }
        }

        Ok(Self { codes })
    }

    #[cfg(test)]
    pub fn for_tests() -> Self {
        Self::from_toml_str(include_str!("../config/referrals.toml"))
            .expect("config/referrals.toml should stay valid")
    }
}

pub fn normalize_referral_code(input: &str) -> String {
    input.trim().to_lowercase()
}

pub fn set_referral_registry(registry: ReferralRegistry) -> Result<(), ReferralRegistryError> {
    REFERRAL_REGISTRY
        .set(registry)
        .map_err(|_| ReferralRegistryError::AlreadyInitialized)
}

pub fn referral_registry() -> &'static ReferralRegistry {
    #[cfg(test)]
    {
        return REFERRAL_REGISTRY.get_or_init(ReferralRegistry::for_tests);
    }

    #[cfg(not(test))]
    {
        REFERRAL_REGISTRY
            .get()
            .expect("referral registry must be initialized before use")
    }
}

#[cfg(test)]
mod tests {
    use super::{normalize_referral_code, ReferralRegistry, ReferralRegistryError};

    #[test]
    fn normalizes_input_to_trimmed_lowercase() {
        assert_eq!(normalize_referral_code("  CoDiGo-1 "), "codigo-1");
    }

    #[test]
    fn rejects_duplicate_codes() {
        let error = ReferralRegistry::from_toml_str(
            r#"
codes = ["codigo-1", "codigo-1"]
"#,
        )
        .expect_err("duplicate codes should fail");

        assert!(
            matches!(error, ReferralRegistryError::Validation(message) if message.contains("duplicate"))
        );
    }

    #[test]
    fn rejects_untrimmed_or_uppercase_codes() {
        let error = ReferralRegistry::from_toml_str(
            r#"
codes = [" Codigo-1 "]
"#,
        )
        .expect_err("non-normalized codes should fail");

        assert!(
            matches!(error, ReferralRegistryError::Validation(message) if message.contains("trimmed lowercase"))
        );
    }
}
