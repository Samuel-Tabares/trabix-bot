//! Tarifas de domicilio por zona/destino. Igual que `pricing.rs`, esto es
//! deterministico y no depende del LLM: el agente solo elige que zona o
//! pueblo aplica segun lo que diga el cliente, este modulo calcula el valor.

pub const MIN_UNITS_OUTSIDE_ARMENIA: u32 = 20;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArmeniaZone {
    Norte,
    Centro,
    Sur,
}

impl ArmeniaZone {
    pub fn from_text(input: &str) -> Option<Self> {
        match normalize(input).as_str() {
            "norte" => Some(Self::Norte),
            "centro" => Some(Self::Centro),
            "sur" => Some(Self::Sur),
            _ => None,
        }
    }

    pub fn delivery_cost(self) -> u32 {
        match self {
            Self::Norte => 6_000,
            Self::Centro => 8_000,
            Self::Sur => 10_000,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Norte => "Norte",
            Self::Centro => "Centro",
            Self::Sur => "Sur",
        }
    }
}

const NEARBY_TOWNS: &[(&str, &str, u32)] = &[
    ("calarca", "Calarcá", 15_000),
    ("pueblo tapao", "Pueblo Tapao", 20_000),
    ("barcelona", "Barcelona", 21_000),
    ("el caimo", "El Caimo", 15_000),
    ("filandia", "Filandia", 45_000),
    ("circasia", "Circasia", 16_000),
    ("quimbaya", "Quimbaya", 32_000),
    ("montenegro", "Montenegro", 16_000),
    ("tebaida", "La Tebaida", 16_000),
    ("la tebaida", "La Tebaida", 16_000),
    ("salento", "Salento", 40_000),
    ("cordoba", "Córdoba", 48_000),
    ("buenavista", "Buenavista", 45_000),
    ("genova", "Génova", 48_000),
    ("pijao", "Pijao", 45_000),
];

pub struct NearbyTown {
    pub name: &'static str,
    pub delivery_cost: u32,
}

/// Busca un pueblo cercano por nombre libre (case/acentos-insensible).
/// `None` significa que no esta en la lista conocida: es una ciudad/pueblo
/// distinto, que sigue el proceso manual con el asesor.
pub fn lookup_nearby_town(input: &str) -> Option<NearbyTown> {
    let normalized = normalize(input);
    NEARBY_TOWNS
        .iter()
        .find(|(key, _, _)| *key == normalized)
        .map(|(_, name, cost)| NearbyTown {
            name,
            delivery_cost: *cost,
        })
}

fn normalize(input: &str) -> String {
    strip_accents(input.trim().to_lowercase().as_str())
}

fn strip_accents(input: &str) -> String {
    input
        .chars()
        .map(|ch| match ch {
            'á' => 'a',
            'é' => 'e',
            'í' => 'i',
            'ó' => 'o',
            'ú' => 'u',
            'ñ' => 'n',
            other => other,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn armenia_zone_prices_match_agreed_values() {
        assert_eq!(ArmeniaZone::Norte.delivery_cost(), 6_000);
        assert_eq!(ArmeniaZone::Centro.delivery_cost(), 8_000);
        assert_eq!(ArmeniaZone::Sur.delivery_cost(), 10_000);
    }

    #[test]
    fn armenia_zone_parses_accents_and_case() {
        assert_eq!(ArmeniaZone::from_text("Norte"), Some(ArmeniaZone::Norte));
        assert_eq!(ArmeniaZone::from_text("  sur "), Some(ArmeniaZone::Sur));
        assert_eq!(ArmeniaZone::from_text("oeste"), None);
    }

    #[test]
    fn nearby_town_lookup_matches_accents_and_case() {
        let found = lookup_nearby_town("Córdoba").expect("cordoba should match");
        assert_eq!(found.name, "Córdoba");
        assert_eq!(found.delivery_cost, 48_000);

        let found2 = lookup_nearby_town("la tebaida").expect("la tebaida should match");
        assert_eq!(found2.delivery_cost, 16_000);

        assert!(lookup_nearby_town("Bogotá").is_none());
    }

    #[test]
    fn all_fourteen_nearby_towns_are_covered() {
        for town in [
            "calarca",
            "pueblo tapao",
            "barcelona",
            "el caimo",
            "filandia",
            "circasia",
            "quimbaya",
            "montenegro",
            "tebaida",
            "salento",
            "cordoba",
            "buenavista",
            "genova",
            "pijao",
        ] {
            assert!(
                lookup_nearby_town(town).is_some(),
                "expected {town} to resolve to a known nearby town"
            );
        }
    }
}
