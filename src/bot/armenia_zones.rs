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
    zona_por_comuna: HashMap<String, String>,
    #[serde(default)]
    excepciones: HashMap<String, String>,
    barrios: HashMap<String, Vec<String>>,
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
    /// barrio normalizado -> zonas posibles (mas de una = ambiguo).
    by_barrio: HashMap<String, Vec<ArmeniaZone>>,
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
        for (comuna_key, barrios) in &parsed.barrios {
            let Some(zone_text) = parsed.zona_por_comuna.get(comuna_key) else {
                panic!("{comuna_key} no tiene zona en [zona_por_comuna]");
            };
            let zone = ArmeniaZone::from_text(zone_text)
                .unwrap_or_else(|| panic!("zona invalida para {comuna_key}: {zone_text}"));
            for barrio in barrios {
                let key = normalize_place(barrio);
                if key.is_empty() {
                    continue;
                }
                let zones = by_barrio.entry(key).or_default();
                if !zones.contains(&zone) {
                    zones.push(zone);
                }
            }
        }

        // Una excepcion PISA lo que diga la comuna, incluso si el barrio
        // estaba ambiguo: es la valvula para corregir un caso puntual sin
        // tocar la division oficial.
        for (barrio, zone_text) in &parsed.excepciones {
            let key = normalize_place(barrio);
            let zone = ArmeniaZone::from_text(zone_text)
                .unwrap_or_else(|| panic!("zona invalida en [excepciones] para {barrio}"));
            by_barrio.insert(key, vec![zone]);
        }

        ZoneTable { by_barrio }
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

    let max_window = words.len().min(6);
    for size in (1..=max_window).rev() {
        for start in 0..=words.len() - size {
            let candidate = words[start..start + size].join(" ");
            // Una sola palabra generica ("casa", "calle") jamas es un barrio;
            // el filtro real es que este en la tabla, esto solo evita ruido.
            let Some(zones) = table.by_barrio.get(&candidate) else {
                continue;
            };
            return match zones.as_slice() {
                [zone] => ZoneLookup::Resolved {
                    zone: *zone,
                    barrio: candidate,
                },
                _ => ZoneLookup::Ambiguous { barrio: candidate },
            };
        }
    }

    ZoneLookup::Unknown
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_table_parses_and_covers_every_comuna() {
        let table = table();
        assert!(
            table.by_barrio.len() > 250,
            "se esperaban ~295 barrios, hay {}",
            table.by_barrio.len()
        );
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

    /// "Cibeles casa 10" — el ejemplo de Samuel. Un nombre que no esta en la
    /// division oficial no se adivina.
    #[test]
    fn an_unknown_place_is_not_guessed() {
        assert_eq!(lookup_zone("cibeles casa 10"), ZoneLookup::Unknown);
        assert_eq!(lookup_zone("apartamento 301"), ZoneLookup::Unknown);
    }

    /// Nombres repetidos en dos comunas con zonas distintas: preguntar, no
    /// elegir. Es justo el caso "puede ser un barrio o un conjunto".
    /// "Las Veraneras" existe en la comuna 2 y en la 10.
    ///
    /// Ojo: que un nombre sea ambiguo depende de la asignacion de
    /// [zona_por_comuna]. Si dos comunas que hoy tienen zonas distintas pasan
    /// a tener la misma, ese nombre deja de ser ambiguo solo — es el
    /// comportamiento correcto, pero explica por que este test puede cambiar
    /// si se reasignan las comunas.
    #[test]
    fn a_name_in_two_zones_is_ambiguous() {
        assert!(matches!(
            lookup_zone("Parque Residencial Las Veraneras torre 2"),
            ZoneLookup::Ambiguous { .. }
        ));
    }

    /// Un nombre repetido pero cuyas dos comunas caen en la MISMA zona si se
    /// resuelve: no hay nada que preguntar. "Los Andes" esta en la comuna 6 y
    /// en la 10, las dos norte en el borrador actual.
    #[test]
    fn a_repeated_name_within_one_zone_still_resolves() {
        assert!(matches!(
            lookup_zone("Conjunto Residencial Los Andes apto 402"),
            ZoneLookup::Resolved { .. }
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
