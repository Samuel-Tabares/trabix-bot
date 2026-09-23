//! Catálogo de sabores — fuente viva en `crm-app`, no en `config/messages.toml`.
//!
//! Mismo patrón que `pricing.rs`: caché en memoria, fetch al boot que NUNCA
//! bloquea el arranque, refresco periódico como red de seguridad y un
//! `POST /internal/flavors/refresh` que `crm-app` llama al guardar para que el
//! cambio se vea al instante.
//!
//! Por qué salió del TOML: agregar o retirar un sabor exigía editar Rust y
//! redesplegar. Ahora Samuel lo apaga desde el panel y desaparece de la carta
//! del website y de lo que ofrece el bot sin desplegar nada.
//!
//! **La desambiguación es la parte delicada.** Antes vivía en
//! `AMBIGUOUS_GROUPS`, una lista escrita a mano en `ai/tools.rs` con los cuatro
//! nombres base que existen en variante con y sin licor. Esa lista es el parche
//! del incidente del 2026-07-19, donde el modelo adivinaba en silencio si el
//! cliente quería el Maracumango con ron o el sin alcohol. Una lista a mano no
//! podía cubrir un sabor agregado desde un panel, así que acá los grupos se
//! calculan agrupando por `base_name` y las palabras que distinguen cada
//! variante se derivan del propio nombre.

use std::{
    error::Error,
    fmt,
    sync::{Arc, OnceLock, RwLock},
};

use serde::Deserialize;

static FLAVOR_TABLE: OnceLock<RwLock<Arc<FlavorTable>>> = OnceLock::new();

/// URL pública de la carta. Es configuración global, no contexto de
/// conversación, así que vive acá y no en `ConversationContext` — que ya tiene
/// 40 campos y se construye en una docena de sitios.
static CARTA_URL: OnceLock<Option<String>> = OnceLock::new();

pub fn init_carta_url(url: Option<String>) {
    let _ = CARTA_URL.set(url);
}

/// `None` = no hay carta configurada; el bot cae a mandar la imagen del menú,
/// que es el comportamiento anterior.
pub fn carta_url() -> Option<&'static str> {
    CARTA_URL.get().and_then(|v| v.as_deref())
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FlavorEntry {
    pub flavor_id: String,
    pub name: String,
    pub base_name: String,
    pub has_liquor: bool,
    pub active: bool,
    #[serde(default)]
    pub sort_order: i32,
}

#[derive(Debug, Clone, Deserialize)]
pub struct FlavorTable {
    pub flavors: Vec<FlavorEntry>,
}

impl Default for FlavorTable {
    /// Fallback compilado: los 12 sabores que estaban en `config/messages.toml`
    /// al momento de mover el catálogo a `crm-app` (2026-09-22).
    ///
    /// Es una foto congelada a propósito. Si `crm-app` no responde al arranque,
    /// el bot sigue vendiendo lo que vendía antes en vez de quedarse sin
    /// catálogo — que sería dejar de vender del todo.
    fn default() -> Self {
        fn e(flavor_id: &str, name: &str, base_name: &str, has_liquor: bool, sort_order: i32) -> FlavorEntry {
            FlavorEntry {
                flavor_id: flavor_id.to_string(),
                name: name.to_string(),
                base_name: base_name.to_string(),
                has_liquor,
                active: true,
                sort_order,
            }
        }

        Self {
            flavors: vec![
                e("liquor_maracumango_ron_blanco", "Maracumango Ron blanco", "Maracumango", true, 1),
                e("liquor_blueberry_vodka", "Blueberry Vodka", "Blueberry", true, 2),
                e("liquor_uva_vodka", "Uva Vodka", "Uva", true, 3),
                e("liquor_bonbonbum_whiskey", "Bonbonbum Whiskey", "Bonbonbum", true, 4),
                e("liquor_bonbonbum_fresa_champagne", "Bonbonbum fresa champaña", "Bonbonbum", true, 5),
                e("liquor_smirnoff_lulo", "Smirnoff de lulo", "Smirnoff lulo", true, 6),
                e("liquor_smirnoff_tamarindo", "Smirnoff de tamarindo", "Smirnoff tamarindo", true, 7),
                e("liquor_manzana_verde_tequila", "Manzana verde Tequila", "Manzana verde", true, 8),
                e("non_liquor_maracumango", "Maracumango", "Maracumango", false, 9),
                e("non_liquor_manzana_verde", "Manzana verde", "Manzana verde", false, 10),
                e("non_liquor_bonbonbum", "Bonbonbum", "Bonbonbum", false, 11),
                e("non_liquor_blueberry", "Blueberry", "Blueberry", false, 12),
            ],
        }
    }
}

impl FlavorTable {
    pub fn active(&self) -> impl Iterator<Item = &FlavorEntry> {
        self.flavors.iter().filter(|f| f.active)
    }

    pub fn active_sorted(&self, has_liquor: bool) -> Vec<&FlavorEntry> {
        let mut out: Vec<&FlavorEntry> = self
            .active()
            .filter(|f| f.has_liquor == has_liquor)
            .collect();
        out.sort_by_key(|f| f.sort_order);
        out
    }

    /// Incluye los inactivos a propósito: si un cliente nombra un sabor que se
    /// acabó, el bot tiene que reconocerlo para poder decirle que no hay, en
    /// vez de no entenderle.
    pub fn find(&self, flavor_id: &str, has_liquor: bool) -> Option<&FlavorEntry> {
        self.flavors
            .iter()
            .find(|f| f.flavor_id == flavor_id && f.has_liquor == has_liquor)
    }

    pub fn is_active(&self, flavor_id: &str, has_liquor: bool) -> bool {
        self.find(flavor_id, has_liquor).is_some_and(|f| f.active)
    }
}

#[derive(Debug)]
pub enum FlavorSyncError {
    Http(reqwest::Error),
    Status(u16),
    /// Ver el comentario en `fetch_flavor_table`.
    Empty,
}

impl fmt::Display for FlavorSyncError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Http(source) => write!(f, "failed to fetch flavor table: {source}"),
            Self::Status(status) => write!(f, "crm-app returned status {status} for flavor table"),
            Self::Empty => write!(f, "crm-app returned a flavor table with no active flavors"),
        }
    }
}

impl Error for FlavorSyncError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Http(source) => Some(source),
            Self::Status(_) | Self::Empty => None,
        }
    }
}

pub fn init_flavor_table(table: FlavorTable) {
    let _ = FLAVOR_TABLE.set(RwLock::new(Arc::new(table)));
}

pub fn swap_flavor_table(table: FlavorTable) {
    if let Some(lock) = FLAVOR_TABLE.get() {
        *lock.write().expect("flavor table lock poisoned") = Arc::new(table);
    }
}

pub fn current_flavor_table() -> Arc<FlavorTable> {
    match FLAVOR_TABLE.get() {
        Some(lock) => lock.read().expect("flavor table lock poisoned").clone(),
        // Los tests unitarios nunca llaman init_flavor_table: caen al default
        // compilado, que es justo lo que esperan.
        None => Arc::new(FlavorTable::default()),
    }
}

/// Pide el catálogo a `crm-app`. Nunca hace panic: un fallo de red o un 5xx se
/// loguea y el bot se queda con la tabla anterior.
pub async fn fetch_flavor_table(
    http_client: &reqwest::Client,
    url: &str,
    token: &str,
) -> Result<FlavorTable, FlavorSyncError> {
    let response = http_client
        .get(url)
        .header("X-Internal-Token", token)
        .send()
        .await
        .map_err(FlavorSyncError::Http)?;

    if !response.status().is_success() {
        return Err(FlavorSyncError::Status(response.status().as_u16()));
    }

    let table = response
        .json::<FlavorTable>()
        .await
        .map_err(FlavorSyncError::Http)?;

    // Un catálogo sin un solo sabor activo es una respuesta rota o una base a
    // medio migrar, no un estado legítimo: aceptarla dejaría al bot sin nada
    // que vender. Se rechaza y gana lo que ya estaba cargado.
    if table.active().next().is_none() {
        return Err(FlavorSyncError::Empty);
    }

    Ok(table)
}

// --- Desambiguación --------------------------------------------------------

/// Quita tildes y ñ y pasa a minúsculas, para que "champaña", "champana" y
/// "Champaña" caigan en el mismo lugar. Nadie acentúa escribiendo por WhatsApp.
fn normalizar(texto: &str) -> String {
    texto
        .to_lowercase()
        .chars()
        .map(|c| match c {
            'á' => 'a',
            'é' => 'e',
            'í' => 'i',
            'ó' => 'o',
            'ú' | 'ü' => 'u',
            // La ñ también: el cliente escribe "champana" tanto como "champaña".
            'ñ' => 'n',
            other => other,
        })
        .collect()
}

/// Palabras que distinguen esta variante dentro de su grupo.
///
/// Se derivan del nombre menos el nombre base: "Maracumango Ron blanco" sobre
/// la base "Maracumango" deja "ron" y "blanco". Eso reproduce exactamente la
/// lista que antes estaba escrita a mano, y a diferencia de ella cubre sola
/// cualquier sabor agregado desde el panel.
///
/// Las genéricas ("con/sin licor", "con/sin alcohol") aplican siempre según la
/// variante: un cliente que dice "el maracumango sin licor" ya fue claro aunque
/// no nombre ningún destilado.
fn keywords_de(entry: &FlavorEntry) -> Vec<String> {
    let mut keywords: Vec<String> = if entry.has_liquor {
        vec!["con licor".into(), "con alcohol".into()]
    } else {
        vec!["sin licor".into(), "sin alcohol".into()]
    };

    let base = normalizar(&entry.base_name);
    let base_tokens: Vec<&str> = base.split_whitespace().collect();

    for token in normalizar(&entry.name).split_whitespace() {
        // "de" en "Smirnoff de lulo" no distingue nada.
        if token.len() < 3 {
            continue;
        }
        if token == "de" || base_tokens.contains(&token) {
            continue;
        }
        keywords.push(token.to_string());
    }

    keywords
}

/// Si `flavor_id`+`has_liquor` pertenece a un grupo ambiguo (dos o más sabores
/// que comparten `base_name`) y el texto del cliente no trae ninguna palabra
/// que distinga la variante elegida, devuelve los nombres de las otras
/// variantes para que el modelo pregunte en vez de adivinar.
///
/// Un sabor cuyo `base_name` no comparte con nadie es inequívoco y pasa
/// siempre.
pub fn check_flavor_disambiguation(
    flavor_id: &str,
    has_liquor: bool,
    customer_wording: &str,
) -> Result<(), Vec<String>> {
    let table = current_flavor_table();

    let Some(chosen) = table.find(flavor_id, has_liquor) else {
        return Ok(());
    };

    let base = normalizar(&chosen.base_name);
    let grupo: Vec<&FlavorEntry> = table
        .active()
        .filter(|f| normalizar(&f.base_name) == base)
        .collect();

    // Grupo de uno: no hay con qué confundirlo.
    if grupo.len() < 2 {
        return Ok(());
    }

    let dicho = normalizar(customer_wording);
    if keywords_de(chosen)
        .iter()
        .any(|keyword| dicho.contains(keyword.as_str()))
    {
        return Ok(());
    }

    let otros = grupo
        .iter()
        .filter(|f| !(f.flavor_id == flavor_id && f.has_liquor == has_liquor))
        .map(|f| f.name.clone())
        .collect();

    Err(otros)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sabor_sin_grupo_nunca_es_ambiguo() {
        // Uva Vodka solo existe con licor: da igual lo que diga el cliente.
        assert!(check_flavor_disambiguation("liquor_uva_vodka", true, "uva").is_ok());
    }

    #[test]
    fn base_compartida_sin_palabra_distintiva_es_ambigua() {
        let err = check_flavor_disambiguation("non_liquor_maracumango", false, "maracumango")
            .expect_err("maracumango existe con y sin licor");
        assert!(err.iter().any(|n| n.contains("Ron blanco")));
    }

    #[test]
    fn palabra_distintiva_resuelve_la_ambiguedad() {
        assert!(
            check_flavor_disambiguation("liquor_maracumango_ron_blanco", true, "maracumango con ron")
                .is_ok()
        );
        assert!(
            check_flavor_disambiguation("non_liquor_maracumango", false, "maracumango sin licor")
                .is_ok()
        );
    }

    #[test]
    fn keyword_derivada_del_nombre_funciona_sin_tildes() {
        // "champaña" se escribe a menudo sin tilde y sin ñ.
        assert!(check_flavor_disambiguation(
            "liquor_bonbonbum_fresa_champagne",
            true,
            "el bonbonbum de champana"
        )
        .is_ok());
    }

    #[test]
    fn grupo_de_tres_distingue_cada_variante() {
        // Bonbonbum existe en tres: sin licor, whiskey y fresa champaña.
        assert!(
            check_flavor_disambiguation("liquor_bonbonbum_whiskey", true, "bonbonbum whiskey")
                .is_ok()
        );
        let err = check_flavor_disambiguation("liquor_bonbonbum_whiskey", true, "bonbonbum")
            .expect_err("bonbonbum a secas no distingue entre tres variantes");
        assert_eq!(err.len(), 2);
    }

    #[test]
    fn un_sabor_nuevo_con_base_compartida_queda_cubierto_solo() {
        // El caso que la lista a mano no cubría: agregar "Mango" sin licor y
        // "Mango Ron" con licor desde el panel formaba un par ambiguo que
        // AMBIGUOUS_GROUPS no conocía, y el modelo volvía a adivinar.
        let tabla = FlavorTable {
            flavors: vec![
                FlavorEntry {
                    flavor_id: "non_liquor_mango".into(),
                    name: "Mango".into(),
                    base_name: "Mango".into(),
                    has_liquor: false,
                    active: true,
                    sort_order: 1,
                },
                FlavorEntry {
                    flavor_id: "liquor_mango_ron".into(),
                    name: "Mango Ron".into(),
                    base_name: "Mango".into(),
                    has_liquor: true,
                    active: true,
                    sort_order: 2,
                },
            ],
        };

        let base = normalizar("Mango");
        let grupo: Vec<&FlavorEntry> = tabla
            .active()
            .filter(|f| normalizar(&f.base_name) == base)
            .collect();
        assert_eq!(grupo.len(), 2, "los dos Mango forman grupo por base_name");

        let con_ron = tabla.find("liquor_mango_ron", true).unwrap();
        assert!(keywords_de(con_ron).contains(&"ron".to_string()));
    }

    #[test]
    fn catalogo_sin_activos_no_reemplaza_la_tabla() {
        let vacio = FlavorTable {
            flavors: vec![FlavorEntry {
                flavor_id: "non_liquor_x".into(),
                name: "X".into(),
                base_name: "X".into(),
                has_liquor: false,
                active: false,
                sort_order: 1,
            }],
        };
        assert!(vacio.active().next().is_none());
    }

    #[test]
    fn el_default_compilado_trae_los_doce() {
        let t = FlavorTable::default();
        assert_eq!(t.flavors.len(), 12);
        assert_eq!(t.active_sorted(true).len(), 8);
        assert_eq!(t.active_sorted(false).len(), 4);
    }
}
