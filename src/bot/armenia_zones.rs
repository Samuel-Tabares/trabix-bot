//! Resuelve la zona de tarifa (norte/centro/sur) de una direccion de Armenia
//! a partir del barrio, de forma DETERMINISTA.
//!
//! Por que existe: `set_delivery_zone_armenia` recibia el sector como un
//! parametro que elegia el modelo, y no habia ninguna tabla detras. El modelo
//! leia "Barrio Granada" y adivinaba. Con el cliente Kall Diaz adivino "norte"
//! el 2026-08-30 ($6.000) y "centro" el 2026-09-18 ($8.000) sobre la MISMA
//! direccion, y ademas persistio la adivinanza en `customer_addresses`, asi
//! que el error se heredaba en cada recompra.
//!
//! La regla de este modulo es que nunca adivina: si el barrio no esta en la
//! tabla, o cae en dos comunas con zonas distintas, devuelve `Unknown` /
//! `Ambiguous` y el bot tiene que PREGUNTARLE al cliente. Eso es lo que
//! arregla el bug; la tabla solo ahorra la pregunta cuando se puede.

use std::collections::HashMap;
use std::sync::OnceLock;

use serde::Deserialize;

use super::delivery_zone::ArmeniaZone;

const ZONES_TOML: &str = include_str!("../../config/armenia_zones.toml");

/// Prefijos genericos que la gente omite al hablar: dicen "Granada", no
/// "Conjunto Residencial Granada". Se recortan de los dos lados (tabla y
/// direccion del cliente) para que los nombres se encuentren.
const GENERIC_PREFIXES: &[&str] = &[
    "conjunto residencial",
    "conjunto multifamiliar",
    "conjunto cerrado",
    "parque residencial",
    "asentamiento",
    "urbanizacion",
    "ciudadela",
    "condominio",
    "conjunto",
    "edificio",
    "bloques",
    "bloque",
    "barrio",
    "sector",
];

#[derive(Debug, Deserialize)]
struct ZonesFile {
    /// Nombres que existen en mas de una zona: nunca se resuelven solos.
    #[serde(default)]
    ambiguos: Vec<String>,
    barrios: HashMap<String, String>,
}

/// Que se pudo concluir de una direccion. `Ambiguous` y `Unknown` son
/// resultados de primera clase, no errores: los dos significan "preguntale al
/// cliente", que es exactamente lo que el bot dejo de hacer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ZoneLookup {
    Resolved { zone: ArmeniaZone, barrio: String },
    /// El nombre existe en comunas con zonas distintas (p. ej. "Los Andes"
    /// esta en la comuna 6 y en la 10). Un conjunto que se llama igual que un
    /// barrio de otro lado cae aca.
    Ambiguous { barrio: String },
    Unknown,
}

struct ZoneTable {
    /// barrio normalizado -> zonas posibles (vacio o >1 = ambiguo, preguntar).
    by_barrio: HashMap<String, Vec<ArmeniaZone>>,
    /// Mismo mapa pero con el articulo inicial recortado ("el paraiso" ->
    /// "paraiso"). La gente dicta "Barrio paraiso Mz D", no "El Paraiso", y sin
    /// esto una direccion real de la base quedaba sin reconocer. Se consulta
    /// solo si fallo la busqueda exacta, y se construye en tiempo de carga
    /// para que editar la zona de un barrio arrastre su alias sola.
    by_alias: HashMap<String, Vec<ArmeniaZone>>,
}

/// Articulos que la gente omite al nombrar un barrio.
const LEADING_ARTICLES: &[&str] = &["el ", "la ", "los ", "las "];

fn strip_leading_article(key: &str) -> Option<&str> {
    LEADING_ARTICLES
        .iter()
        .find_map(|article| key.strip_prefix(article))
        .filter(|rest| !rest.is_empty())
}

static TABLE: OnceLock<ZoneTable> = OnceLock::new();

/// Fuerza el parseo de la tabla al arrancar, no en la primera direccion de un
/// cliente. `[zona_por_comuna]` esta pensado para que Samuel lo edite a mano,
/// asi que un typo ahi es esperable — y sin esto el `panic!` del parser caia
/// en mitad de una conversacion real en vez de reventar el deploy, que es
/// donde un error de configuracion tiene que verse.
pub fn validate_at_startup() {
    let count = table().by_barrio.len();
    tracing::info!(barrios = count, "tabla de zonas de Armenia cargada");
}

fn table() -> &'static ZoneTable {
    TABLE.get_or_init(|| {
        let parsed: ZonesFile = toml::from_str(ZONES_TOML)
            .expect("config/armenia_zones.toml debe ser TOML valido");

        let mut by_barrio: HashMap<String, Vec<ArmeniaZone>> = HashMap::new();
        for (barrio, zone_text) in &parsed.barrios {
            let key = normalize_place(barrio);
            if key.is_empty() {
                continue;
            }
            let zone = ArmeniaZone::from_text(zone_text)
                .unwrap_or_else(|| panic!("zona invalida para '{barrio}': '{zone_text}'"));
            by_barrio.insert(key, vec![zone]);
        }

        // Los ambiguos se cargan DESPUES y pisan cualquier zona: listar un
        // nombre ahi es la forma de obligar al bot a preguntar por el.
        for barrio in &parsed.ambiguos {
            let key = normalize_place(barrio);
            if !key.is_empty() {
                by_barrio.insert(key, Vec::new());
            }
        }

        // Alias sin articulo. Si dos barrios distintos colapsan al mismo alias
        // con zonas distintas, el alias queda ambiguo y se pregunta: nunca se
        // elige uno. Un alias que choque con un nombre exacto no se crea —
        // gana siempre el nombre completo.
        let mut by_alias: HashMap<String, Vec<ArmeniaZone>> = HashMap::new();
        for (key, zones) in &by_barrio {
            let [zone] = zones.as_slice() else { continue };
            let Some(short) = strip_leading_article(key) else {
                continue;
            };
            if by_barrio.contains_key(short) {
                continue;
            }
            let entry = by_alias.entry(short.to_string()).or_default();
            if !entry.contains(zone) {
                entry.push(*zone);
            }
        }

        ZoneTable {
            by_barrio,
            by_alias,
        }
    })
}

/// Minusculas, sin tildes, sin puntuacion y sin los prefijos genericos.
pub fn normalize_place(input: &str) -> String {
    let lowered: String = input
        .to_lowercase()
        .chars()
        .map(|c| match c {
            'á' | 'à' | 'ä' | 'â' => 'a',
            'é' | 'è' | 'ë' | 'ê' => 'e',
            'í' | 'ì' | 'ï' | 'î' => 'i',
            'ó' | 'ò' | 'ö' | 'ô' => 'o',
            'ú' | 'ù' | 'ü' | 'û' => 'u',
            c if c.is_alphanumeric() || c.is_whitespace() => c,
            _ => ' ',
        })
        .collect();

    let mut cleaned = lowered.split_whitespace().collect::<Vec<_>>().join(" ");
    loop {
        let before = cleaned.clone();
        for prefix in GENERIC_PREFIXES {
            if let Some(rest) = cleaned.strip_prefix(&format!("{prefix} ")) {
                cleaned = rest.to_string();
            }
        }
        if cleaned == before {
            break;
        }
    }
    cleaned
}

/// Busca en el texto libre de una direccion el barrio mas especifico que
/// conozca la tabla.
///
/// Recorre las ventanas de palabras de mayor a menor longitud para que
/// "granada" no le gane a "bosques de pinares" dentro de la misma direccion:
/// el nombre mas largo que coincida es el mas informativo.
pub fn lookup_zone(address: &str) -> ZoneLookup {
    let table = table();
    let normalized = normalize_place(address);
    let words: Vec<&str> = normalized.split_whitespace().collect();
    if words.is_empty() {
        return ZoneLookup::Unknown;
    }

    // Dos pasadas: primero los nombres completos, y solo si ninguno pega se
    // reintenta contra los alias sin articulo. Asi un nombre exacto siempre le
    // gana a un alias.
    for map in [&table.by_barrio, &table.by_alias] {
        let max_window = words.len().min(6);
        for size in (1..=max_window).rev() {
            for start in 0..=words.len() - size {
                let candidate = words[start..start + size].join(" ");
                let Some(zones) = map.get(&candidate) else {
                    continue;
                };
                return match zones.as_slice() {
                    [zone] => ZoneLookup::Resolved {
                        zone: *zone,
                        barrio: candidate,
                    },
                    // Vacio = marcado como ambiguo en el TOML; mas de uno =
                    // el alias colapsa dos barrios de zonas distintas.
                    _ => ZoneLookup::Ambiguous { barrio: candidate },
                };
            }
        }
    }

    ZoneLookup::Unknown
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_table_parses_and_is_not_suspiciously_small() {
        let table = table();
        assert!(
            table.by_barrio.len() > 450,
            "se esperaban ~515 barrios, hay {}",
            table.by_barrio.len()
        );
    }

    /// Las tres zonas tienen que estar representadas: si una desaparece es que
    /// alguien rompió el TOML (o lo regeneró mal) y el bot cobraría de más o
    /// de menos en silencio.
    #[test]
    fn the_three_zones_are_all_present() {
        let mut seen = std::collections::HashSet::new();
        for zones in table().by_barrio.values() {
            for z in zones {
                seen.insert(z.label());
            }
        }
        assert_eq!(seen.len(), 3, "faltan zonas: {seen:?}");
    }

    /// Frontera sur declarada por Samuel: el Estadio, Puerto Espejo y Mercar
    /// están todos al sur.
    #[test]
    fn the_southern_landmarks_are_south() {
        for address in [
            "ciudadela puerto espejo etapa 2",
            "castilla grande casa 4",
            "bosques de pinares",
        ] {
            match lookup_zone(address) {
                ZoneLookup::Resolved { zone, .. } => {
                    assert_eq!(zone, ArmeniaZone::Sur, "{address} no dio sur")
                }
                other => panic!("{address}: {other:?}"),
            }
        }
    }

    /// Y los del norte, al norte. La Castellana y Laureles están arriba del
    /// Coliseo del Café.
    #[test]
    fn the_northern_landmarks_are_north() {
        for address in ["la castellana", "laureles", "la campiña"] {
            match lookup_zone(address) {
                ZoneLookup::Resolved { zone, .. } => {
                    assert_eq!(zone, ArmeniaZone::Norte, "{address} no dio norte")
                }
                other => panic!("{address}: {other:?}"),
            }
        }
    }

    /// Un nombre listado en `ambiguos` siempre pregunta, aunque el resto de la
    /// tabla pudiera resolverlo. "Fundadores" y "El Bosque" son justo las
    /// referencias que Samuel puso SOBRE las fronteras.
    #[test]
    fn names_marked_ambiguous_always_ask() {
        for address in ["fundadores", "el bosque casa 2", "san jose"] {
            assert!(
                matches!(lookup_zone(address), ZoneLookup::Ambiguous { .. }),
                "{address} debería preguntar"
            );
        }
    }

    /// El caso que origino el modulo: Granada (comuna 9) es centro, $8.000.
    /// El bot cobro $6.000 diciendo que era norte.
    #[test]
    fn granada_resolves_to_centro() {
        match lookup_zone("Calle 12 Cra 23-43 Barrio Granada KAL DISCOBAR enseguida del colegio zakurayima") {
            ZoneLookup::Resolved { zone, .. } => {
                assert_eq!(zone, ArmeniaZone::Centro);
                assert_eq!(zone.delivery_cost(), 8_000);
            }
            other => panic!("no resolvio: {other:?}"),
        }
    }

    #[test]
    fn a_barrio_written_without_the_word_barrio_still_resolves() {
        assert!(matches!(
            lookup_zone("granada casa 10"),
            ZoneLookup::Resolved { .. }
        ));
    }

    /// "Cibeles casa 10" era el ejemplo de Samuel de un nombre que podía ser
    /// barrio, conjunto o apartamentos. La division oficial por comunas NO lo
    /// tenia; OpenStreetMap si lo ubica, asi que hoy se resuelve solo. Es la
    /// razon por la que la tabla se genero de coordenadas y no de la lista
    /// administrativa.
    #[test]
    fn cibeles_resolves_now_that_the_table_comes_from_coordinates() {
        assert!(matches!(
            lookup_zone("cibeles casa 10"),
            ZoneLookup::Resolved { .. }
        ));
    }

    /// Dirección real de la base: "Barrio paraíso Mz D #153". La tabla tiene
    /// "el paraiso", así que sin el alias sin artículo quedaba sin reconocer.
    #[test]
    fn a_barrio_named_without_its_article_still_resolves() {
        match lookup_zone("Barrio paraíso Mz D #153") {
            ZoneLookup::Resolved { zone, .. } => assert_eq!(zone, ArmeniaZone::Centro),
            other => panic!("no resolvió: {other:?}"),
        }
    }

    /// El nombre completo le gana al alias: "las palmas" no puede resolverse
    /// como si fuera el alias de otra cosa.
    #[test]
    fn the_full_name_wins_over_an_alias() {
        match lookup_zone("las palmas apto 5") {
            ZoneLookup::Resolved { barrio, .. } => assert_eq!(barrio, "las palmas"),
            other => panic!("no resolvió: {other:?}"),
        }
    }

    /// Lo que de verdad no se reconoce sigue sin adivinarse.
    #[test]
    fn an_unknown_place_is_not_guessed() {
        assert_eq!(lookup_zone("apartamento 301"), ZoneLookup::Unknown);
        assert_eq!(lookup_zone("por la esquina de la tienda"), ZoneLookup::Unknown);
    }

    /// "Las Veraneras" aparece en dos puntos de la ciudad que caen en zonas
    /// distintas: preguntar, no elegir.
    #[test]
    fn a_name_in_two_zones_is_ambiguous() {
        assert!(matches!(
            lookup_zone("Parque Residencial Las Veraneras torre 2"),
            ZoneLookup::Ambiguous { .. }
        ));
    }

    #[test]
    fn the_longest_matching_name_wins() {
        match lookup_zone("bosques de pinares torre 3") {
            ZoneLookup::Resolved { barrio, .. } => assert_eq!(barrio, "bosques de pinares"),
            other => panic!("no resolvio: {other:?}"),
        }
    }
}
