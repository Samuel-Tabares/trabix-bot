use std::{
    collections::{BTreeMap, BTreeSet},
    error::Error,
    fmt,
    sync::{Arc, OnceLock, RwLock},
};

use chrono::{DateTime, Utc};
use sqlx::PgPool;

use crate::db::queries::list_active_referral_codes;

pub const MAX_REFERRAL_CODE_LEN: usize = 15;

static REFERRAL_REGISTRY: OnceLock<RwLock<Arc<ReferralRegistry>>> = OnceLock::new();

#[derive(Debug)]
pub enum ReferralRegistryError {
    Db(sqlx::Error),
    Validation(String),
}

impl fmt::Display for ReferralRegistryError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Db(source) => write!(f, "failed to load referral registry from db: {source}"),
            Self::Validation(message) => write!(f, "invalid referral code: {message}"),
        }
    }
}

impl Error for ReferralRegistryError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Db(source) => Some(source),
            Self::Validation(_) => None,
        }
    }
}

impl From<sqlx::Error> for ReferralRegistryError {
    fn from(source: sqlx::Error) -> Self {
        Self::Db(source)
    }
}

/// Codigos de referido validos ahora mismo, cargados desde la tabla
/// `referral_codes` (Fase 6 — reemplaza `config/referrals.toml`). Solo
/// contiene codigos `active=true`; `boost_until` es una ventana temporal por
/// codigo, no un flag fijo: `has_boost` la compara contra la hora actual.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReferralRegistry {
    codes: BTreeSet<String>,
    boost_until: BTreeMap<String, DateTime<Utc>>,
}

impl ReferralRegistry {
    pub async fn load_from_db(pool: &PgPool) -> Result<Self, ReferralRegistryError> {
        let rows = list_active_referral_codes(pool).await?;

        let mut codes = BTreeSet::new();
        let mut boost_until = BTreeMap::new();
        for row in rows {
            codes.insert(row.code.clone());
            if let Some(until) = row.boost_until {
                boost_until.insert(row.code, until);
            }
        }

        Ok(Self { codes, boost_until })
    }

    pub fn contains(&self, code: &str) -> bool {
        self.codes.contains(code)
    }

    pub fn has_boost(&self, code: &str) -> bool {
        self.boost_until
            .get(code)
            .is_some_and(|until| *until > Utc::now())
    }

    /// Espejo de la semilla de la migración 016 (mismos 5 códigos legacy,
    /// `trabix-prueba15` boosteado) — varios tests en `checkout.rs` dependen
    /// de ese código puntual, y así siguen pasando sin tocarlos.
    #[cfg(test)]
    pub fn for_tests() -> Self {
        let mut codes = BTreeSet::new();
        for code in ["trabix-prueba15", "roma08", "jega1", "dani2303", "dg777"] {
            codes.insert(code.to_string());
        }

        let mut boost_until = BTreeMap::new();
        boost_until.insert(
            "trabix-prueba15".to_string(),
            Utc::now() + chrono::Duration::days(7),
        );

        Self { codes, boost_until }
    }
}

pub fn normalize_referral_code(input: &str) -> String {
    input.trim().to_lowercase()
}

/// Valida el formato de un codigo antes de insertarlo (usado por el endpoint
/// interno de creacion, `routes/internal.rs`): normaliza y rechaza vacio o
/// mas largo que `MAX_REFERRAL_CODE_LEN`. No exige que el caller ya lo haya
/// normalizado — a diferencia del validador original de TOML, esto ahora
/// recibe input de un formulario humano en `crm-app`.
pub fn validate_registry_code(code: &str) -> Result<String, ReferralRegistryError> {
    let normalized = normalize_referral_code(code);
    if normalized.is_empty() {
        return Err(ReferralRegistryError::Validation(
            "el codigo no puede estar vacio".to_string(),
        ));
    }
    if normalized.len() > MAX_REFERRAL_CODE_LEN {
        return Err(ReferralRegistryError::Validation(format!(
            "el codigo no puede tener mas de {MAX_REFERRAL_CODE_LEN} caracteres"
        )));
    }
    Ok(normalized)
}

/// Primera carga al boot, antes de aceptar trafico — ver `main.rs`.
pub fn init_referral_registry(registry: ReferralRegistry) {
    REFERRAL_REGISTRY
        .set(RwLock::new(Arc::new(registry)))
        .expect("referral registry must only be initialized once");
}

/// Reemplaza el registro cacheado. Lo llaman el refresco de background
/// (cada 30s, `main.rs`) y los 3 endpoints internos de escritura tras cada
/// mutacion, para que un cambio hecho desde `crm-app` aplique al instante
/// sin esperar el proximo tick.
pub fn swap_referral_registry(registry: ReferralRegistry) {
    if let Some(lock) = REFERRAL_REGISTRY.get() {
        *lock.write().expect("referral registry lock poisoned") = Arc::new(registry);
    }
}

/// Lectura sincrona y barata (clona un `Arc`, no toca la DB) para los call
/// sites de `ai/tools.rs` y `bot/states/checkout.rs`, que no son async.
pub fn referral_registry() -> Arc<ReferralRegistry> {
    #[cfg(test)]
    {
        return REFERRAL_REGISTRY
            .get_or_init(|| RwLock::new(Arc::new(ReferralRegistry::for_tests())))
            .read()
            .expect("referral registry lock poisoned")
            .clone();
    }

    #[cfg(not(test))]
    {
        REFERRAL_REGISTRY
            .get()
            .expect("referral registry must be initialized before use")
            .read()
            .expect("referral registry lock poisoned")
            .clone()
    }
}

#[cfg(test)]
mod tests {
    use chrono::Duration;

    use super::{normalize_referral_code, validate_registry_code, ReferralRegistry};

    #[test]
    fn normalizes_input_to_trimmed_lowercase() {
        assert_eq!(normalize_referral_code("  CoDiGo-1 "), "codigo-1");
    }

    #[test]
    fn validate_registry_code_normalizes_and_accepts_valid_input() {
        assert_eq!(validate_registry_code("  CoDiGo-1 ").unwrap(), "codigo-1");
    }

    #[test]
    fn validate_registry_code_rejects_empty() {
        assert!(validate_registry_code("   ").is_err());
    }

    #[test]
    fn validate_registry_code_rejects_too_long() {
        let too_long = "a".repeat(super::MAX_REFERRAL_CODE_LEN + 1);
        assert!(validate_registry_code(&too_long).is_err());
    }

    #[test]
    fn contains_only_active_codes_loaded_into_registry() {
        let registry = ReferralRegistry::for_tests();
        assert!(registry.contains("trabix-prueba15"));
        assert!(registry.contains("roma08"));
        assert!(!registry.contains("codigo-inexistente"));
    }

    #[test]
    fn has_boost_is_true_only_while_boost_until_is_in_the_future() {
        let mut registry = ReferralRegistry::for_tests();
        assert!(registry.has_boost("trabix-prueba15"));
        assert!(!registry.has_boost("roma08"));

        // Expira: mover boost_until al pasado debe apagar el boost.
        registry.boost_until.insert(
            "trabix-prueba15".to_string(),
            chrono::Utc::now() - Duration::days(1),
        );
        assert!(!registry.has_boost("trabix-prueba15"));
    }
}
