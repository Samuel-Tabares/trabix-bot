//! Tarifas de domicilio por zona/destino. Igual que `pricing.rs`, esto es
//! deterministico y no depende del LLM: el agente solo elige que zona o
//! pueblo aplica segun lo que diga el cliente, este modulo calcula el valor.
//!
//! Domicilio gratis (aprobado 2026-07-30, ver docs/PENDIENTE_domicilio_gratis.md):
//! exclusivo de Armenia, pedidos de 6-19 unidades. Pueblos aledaños (Grupo A)
//! pierden el minimo de unidades del detal pero SIEMPRE cobran tarifa; los
//! pueblos lejanos (Grupo B) conservan el minimo de `MIN_UNITS_OUTSIDE_ARMENIA`.

/// Minimo de unidades para pedidos fuera de Armenia. Solo aplica a pueblos
/// del Grupo B (`TownGroup::Lejano`) — el Grupo A no tiene minimo.
pub const MIN_UNITS_OUTSIDE_ARMENIA: u32 = 20;

/// Umbral inferior/superior (inclusive) de unidades para domicilio gratis en
/// Armenia. Por debajo se cobra tarifa de zona; en o por encima de
/// `ARMENIA_FREE_DELIVERY_MAX` (mayorista) tambien se cobra.
const ARMENIA_FREE_DELIVERY_MIN: u32 = 6;
const ARMENIA_FREE_DELIVERY_MAX: u32 = 19;

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

    /// Tarifa de zona sin considerar la regla de domicilio gratis. Usar
    /// `armenia_delivery_cost` para el costo real que se cobra al cliente.
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

/// Costo real de domicilio en Armenia para un pedido de `total_units`
/// unidades: gratis entre 6 y 19 unidades (ambos inclusive), tarifa de zona
/// en cualquier otro caso (1-5 al detal, o 20+ que ya es mayorista).
pub fn armenia_delivery_cost(zone: ArmeniaZone, total_units: u32) -> u32 {
    if (ARMENIA_FREE_DELIVERY_MIN..=ARMENIA_FREE_DELIVERY_MAX).contains(&total_units) {
        0
    } else {
        zone.delivery_cost()
    }
}

/// Cuántas unidades le faltan a un pedido para calificar al domicilio
/// gratis en Armenia. `None` si ya lo tiene o si ya pasó el rango (20+).
pub fn units_until_free_delivery(total_units: u32) -> Option<u32> {
    if total_units < ARMENIA_FREE_DELIVERY_MIN {
        Some(ARMENIA_FREE_DELIVERY_MIN - total_units)
    } else {
        None
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TownGroup {
    /// Pueblos aledaños: detal completo, sin mínimo de unidades, domicilio
    /// siempre cobrado (nunca gratis, eso es exclusivo de Armenia).
    Aledano,
    /// Pueblos lejanos: se mantiene el mínimo de `MIN_UNITS_OUTSIDE_ARMENIA`
    /// por costo de oportunidad operativo (viaje largo bloquea al domiciliario).
    Lejano,
}

const NEARBY_TOWNS: &[(&str, &str, u32, TownGroup)] = &[
    ("calarca", "Calarcá", 15_000, TownGroup::Aledano),
    ("pueblo tapao", "Pueblo Tapao", 20_000, TownGroup::Aledano),
    ("barcelona", "Barcelona", 21_000, TownGroup::Aledano),
    ("el caimo", "El Caimo", 15_000, TownGroup::Aledano),
    ("filandia", "Filandia", 45_000, TownGroup::Lejano),
    ("circasia", "Circasia", 16_000, TownGroup::Aledano),
    ("quimbaya", "Quimbaya", 32_000, TownGroup::Lejano),
    ("montenegro", "Montenegro", 16_000, TownGroup::Aledano),
    ("tebaida", "La Tebaida", 16_000, TownGroup::Aledano),
    ("la tebaida", "La Tebaida", 16_000, TownGroup::Aledano),
    ("salento", "Salento", 40_000, TownGroup::Lejano),
    ("cordoba", "Córdoba", 48_000, TownGroup::Lejano),
    ("buenavista", "Buenavista", 45_000, TownGroup::Lejano),
    ("genova", "Génova", 48_000, TownGroup::Lejano),
    ("pijao", "Pijao", 45_000, TownGroup::Lejano),
];

pub struct NearbyTown {
    pub name: &'static str,
    pub delivery_cost: u32,
    /// Mínimo de unidades para este destino. `0` significa sin mínimo
    /// (Grupo A / aledaños); Grupo B usa `MIN_UNITS_OUTSIDE_ARMENIA`.
    pub min_units: u32,
}

/// Busca un pueblo cercano por nombre libre (case/acentos-insensible).
/// `None` significa que no esta en la lista conocida: es una ciudad/pueblo
/// distinto, que sigue el proceso manual con el asesor.
pub fn lookup_nearby_town(input: &str) -> Option<NearbyTown> {
    let normalized = normalize(input);
    NEARBY_TOWNS
        .iter()
        .find(|(key, _, _, _)| *key == normalized)
        .map(|(_, name, cost, group)| NearbyTown {
            name,
            delivery_cost: *cost,
            min_units: match group {
                TownGroup::Aledano => 0,
                TownGroup::Lejano => MIN_UNITS_OUTSIDE_ARMENIA,
            },
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

    #[test]
    fn armenia_delivery_is_free_between_six_and_nineteen_units() {
        for units in 6..=19 {
            assert_eq!(
                armenia_delivery_cost(ArmeniaZone::Centro, units),
                0,
                "expected {units} units to be free delivery"
            );
        }
    }

    #[test]
    fn armenia_delivery_charges_zone_rate_below_six_or_at_twenty_plus() {
        for units in [1, 2, 5, 20, 50] {
            assert_eq!(
                armenia_delivery_cost(ArmeniaZone::Norte, units),
                ArmeniaZone::Norte.delivery_cost(),
                "expected {units} units to charge the zone rate"
            );
        }
    }

    #[test]
    fn units_until_free_delivery_counts_down_to_six() {
        assert_eq!(units_until_free_delivery(1), Some(5));
        assert_eq!(units_until_free_delivery(5), Some(1));
        assert_eq!(units_until_free_delivery(6), None);
        assert_eq!(units_until_free_delivery(19), None);
        assert_eq!(units_until_free_delivery(20), None);
    }

    #[test]
    fn grupo_a_towns_have_no_unit_minimum() {
        for town in [
            "calarca",
            "el caimo",
            "circasia",
            "montenegro",
            "la tebaida",
            "pueblo tapao",
            "barcelona",
        ] {
            let found = lookup_nearby_town(town).expect("grupo A town should resolve");
            assert_eq!(found.min_units, 0, "expected {town} to have no minimum");
        }
    }

    #[test]
    fn grupo_b_towns_keep_the_twenty_unit_minimum() {
        for town in ["quimbaya", "salento", "filandia", "buenavista", "pijao", "cordoba", "genova"] {
            let found = lookup_nearby_town(town).expect("grupo B town should resolve");
            assert_eq!(
                found.min_units, MIN_UNITS_OUTSIDE_ARMENIA,
                "expected {town} to keep the minimum"
            );
        }
    }
}
