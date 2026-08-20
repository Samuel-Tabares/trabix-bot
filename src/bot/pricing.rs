use std::{
    error::Error,
    fmt,
    sync::{Arc, OnceLock, RwLock},
};

use serde::Deserialize;

use crate::db::models::OrderItemData;

const LIQUOR_DETAIL_FULL_PRICE: u32 = 8_000;
const LIQUOR_DETAIL_PROMO_PRICE: u32 = 4_000;
const NON_LIQUOR_DETAIL_PRICE: u32 = 7_000;

static PRICING_TABLE: OnceLock<RwLock<Arc<PricingTable>>> = OnceLock::new();

/// Un tier mayorista: desde cuántas unidades aplica, precio unitario y los
/// dos porcentajes de referido (comisión del embajador, descuento al
/// cliente) para ese rango.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WholesaleTier {
    pub min_quantity: u32,
    pub unit_price: u32,
    pub commission_pct: u8,
    pub client_discount_pct: u8,
}

/// Tiers mayoristas por variante, sincronizados con la `pricing_version`
/// activa de `crm-app` (`GET /api/internal/pricing`) — reemplaza el espejo
/// estático manual que se volvía obsoleto cada vez que se cambiaban precios
/// desde `/settings/precios` sin editar este archivo también. Ver
/// `init_pricing_table`/`swap_pricing_table` para cómo se carga.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PricingTable {
    pub with_alcohol: Vec<WholesaleTier>,
    pub without_alcohol: Vec<WholesaleTier>,
}

impl Default for PricingTable {
    /// Precios vigentes de Trabix al momento de este cambio (CLAUDE.md raíz
    /// del workspace) — usados hasta el primer fetch exitoso a `crm-app`, y
    /// como red de seguridad si un fetch alguna vez devuelve un tier que no
    /// cubre la cantidad pedida.
    fn default() -> Self {
        Self {
            with_alcohol: vec![
                WholesaleTier { min_quantity: 20, unit_price: 4_900, commission_pct: 15, client_discount_pct: 10 },
                WholesaleTier { min_quantity: 50, unit_price: 4_700, commission_pct: 18, client_discount_pct: 12 },
                WholesaleTier { min_quantity: 100, unit_price: 4_500, commission_pct: 20, client_discount_pct: 15 },
            ],
            without_alcohol: vec![
                WholesaleTier { min_quantity: 20, unit_price: 4_800, commission_pct: 15, client_discount_pct: 10 },
                WholesaleTier { min_quantity: 50, unit_price: 4_500, commission_pct: 18, client_discount_pct: 12 },
                WholesaleTier { min_quantity: 100, unit_price: 4_200, commission_pct: 20, client_discount_pct: 15 },
            ],
        }
    }
}

#[derive(Debug)]
pub enum PricingSyncError {
    Http(reqwest::Error),
    Status(u16),
}

impl fmt::Display for PricingSyncError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Http(source) => write!(f, "failed to fetch pricing table: {source}"),
            Self::Status(status) => write!(f, "crm-app returned status {status} for pricing table"),
        }
    }
}

impl Error for PricingSyncError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Http(source) => Some(source),
            Self::Status(_) => None,
        }
    }
}

/// Primera carga al boot, antes de aceptar tráfico — ver `main.rs`.
pub fn init_pricing_table(table: PricingTable) {
    PRICING_TABLE
        .set(RwLock::new(Arc::new(table)))
        .expect("pricing table must only be initialized once");
}

/// Reemplaza la tabla cacheada. La llama el refresco de background
/// (`main.rs`) cada vez que vuelve a consultar `crm-app`.
pub fn swap_pricing_table(table: PricingTable) {
    if let Some(lock) = PRICING_TABLE.get() {
        *lock.write().expect("pricing table lock poisoned") = Arc::new(table);
    }
}

fn current_pricing_table() -> Arc<PricingTable> {
    match PRICING_TABLE.get() {
        Some(lock) => lock.read().expect("pricing table lock poisoned").clone(),
        // No debería pasar en producción (main.rs inicializa antes de servir
        // tráfico) pero los tests unitarios de este módulo nunca llaman
        // init_pricing_table — caen al default compilado, que es justo lo
        // que esos tests esperan.
        None => Arc::new(PricingTable::default()),
    }
}

/// Pide a `crm-app` la `pricing_version` activa. Se usa tanto en el fetch
/// inicial del boot como en el refresco periódico — nunca panics, un fallo
/// de red o un 5xx solo se loguea y el bot se queda con la tabla anterior.
pub async fn fetch_pricing_table(
    http_client: &reqwest::Client,
    url: &str,
    token: &str,
) -> Result<PricingTable, PricingSyncError> {
    let response = http_client
        .get(url)
        .header("X-Internal-Token", token)
        .send()
        .await
        .map_err(PricingSyncError::Http)?;

    if !response.status().is_success() {
        return Err(PricingSyncError::Status(response.status().as_u16()));
    }

    response
        .json::<PricingTable>()
        .await
        .map_err(PricingSyncError::Http)
}

fn select_tier(tiers: &[WholesaleTier], cantidad: u32) -> Option<WholesaleTier> {
    tiers
        .iter()
        .filter(|tier| tier.min_quantity <= cantidad)
        .max_by_key(|tier| tier.min_quantity)
        .cloned()
}

/// El tier aplicable para una cantidad + variante, leído de la tabla viva si
/// la cubre y si no del default compilado — nunca deja un pedido real sin
/// tier por un fetch que trajo datos incompletos.
fn resolve_tier(cantidad: u32, has_liquor: bool) -> WholesaleTier {
    let table = current_pricing_table();
    let tiers = if has_liquor { &table.with_alcohol } else { &table.without_alcohol };
    if let Some(tier) = select_tier(tiers, cantidad) {
        return tier;
    }

    tracing::warn!(
        cantidad,
        has_liquor,
        "pricing table has no matching tier, falling back to compiled default"
    );
    let default = PricingTable::default();
    let default_tiers = if has_liquor { &default.with_alcohol } else { &default.without_alcohol };
    select_tier(default_tiers, cantidad)
        .expect("compiled default pricing table must always cover cantidad >= 20")
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PersistedOrderItem {
    pub flavor: String,
    pub has_liquor: bool,
    pub quantity: u32,
    pub unit_price: u32,
    pub subtotal: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ItemCalculated {
    pub flavor: String,
    pub has_liquor: bool,
    pub quantity: u32,
    pub subtotal: u32,
    pub is_wholesale: bool,
    pub promo_units: u32,
    pub unit_price_reference: Option<u32>,
    pub persistence_lines: Vec<PersistedOrderItem>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PedidoCalculado {
    pub items_detalle: Vec<ItemCalculated>,
    pub cantidad_con_licor: u32,
    pub cantidad_sin_licor: u32,
    pub total_con_licor: u32,
    pub total_sin_licor: u32,
    pub es_mayor_con_licor: bool,
    pub es_mayor_sin_licor: bool,
    pub total_estimado: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReferralBucketCalculated {
    pub has_liquor: bool,
    pub quantity: u32,
    pub subtotal_before_discount: u32,
    pub client_discount_percent: u8,
    pub client_discount_amount: u32,
    pub subtotal_after_discount: u32,
    pub ambassador_commission_percent: u8,
    pub ambassador_commission_amount: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReferralApplied {
    pub code: String,
    pub buckets: Vec<ReferralBucketCalculated>,
    pub total_client_discount: u32,
    pub subtotal_after_discount: u32,
    pub total_ambassador_commission: u32,
}

pub fn calcular_precio_licor_detal(cantidad: u32) -> u32 {
    let pares = cantidad / 2;
    let impares = cantidad % 2;
    (pares * 12_000) + (impares * 8_000)
}

pub fn calcular_precio_sin_licor_detal(cantidad: u32) -> u32 {
    cantidad * NON_LIQUOR_DETAIL_PRICE
}

pub fn precio_unitario_mayor(cantidad: u32, has_liquor: bool) -> u32 {
    resolve_tier(cantidad, has_liquor).unit_price
}

pub fn calcular_pedido(items: &[OrderItemData]) -> PedidoCalculado {
    let total_con_licor_qty = total_quantity(items, true);
    let total_sin_licor_qty = total_quantity(items, false);
    let es_mayor_con_licor = total_con_licor_qty >= 20;
    let es_mayor_sin_licor = total_sin_licor_qty >= 20;

    let mut licor_posicion = 0;
    let mut items_detalle = Vec::with_capacity(items.len());
    let mut total_con_licor = 0;
    let mut total_sin_licor = 0;

    for item in items {
        let calculado = if item.has_liquor {
            if es_mayor_con_licor {
                calcular_item_mayor(item, precio_unitario_mayor(total_con_licor_qty, true))
            } else {
                let item_calculado = calcular_item_licor_detal(item, licor_posicion);
                licor_posicion += item.quantity;
                item_calculado
            }
        } else if es_mayor_sin_licor {
            calcular_item_mayor(item, precio_unitario_mayor(total_sin_licor_qty, false))
        } else {
            calcular_item_sin_licor_detal(item)
        };

        if item.has_liquor {
            total_con_licor += calculado.subtotal;
        } else {
            total_sin_licor += calculado.subtotal;
        }

        items_detalle.push(calculado);
    }

    PedidoCalculado {
        items_detalle,
        cantidad_con_licor: total_con_licor_qty,
        cantidad_sin_licor: total_sin_licor_qty,
        total_con_licor,
        total_sin_licor,
        es_mayor_con_licor,
        es_mayor_sin_licor,
        total_estimado: total_con_licor + total_sin_licor,
    }
}

pub fn has_wholesale_bucket(pedido: &PedidoCalculado) -> bool {
    pedido.es_mayor_con_licor || pedido.es_mayor_sin_licor
}

pub fn calcular_referido(pedido: &PedidoCalculado, code: &str) -> Option<ReferralApplied> {
    calcular_referido_con_boost(pedido, code, false)
}

pub fn calcular_referido_con_boost(
    pedido: &PedidoCalculado,
    code: &str,
    has_boost: bool,
) -> Option<ReferralApplied> {
    let mut buckets = Vec::new();

    if pedido.es_mayor_con_licor {
        buckets.push(calcular_bucket_referido(
            true,
            pedido.cantidad_con_licor,
            pedido.total_con_licor,
            has_boost,
        ));
    }
    if pedido.es_mayor_sin_licor {
        buckets.push(calcular_bucket_referido(
            false,
            pedido.cantidad_sin_licor,
            pedido.total_sin_licor,
            has_boost,
        ));
    }

    if buckets.is_empty() {
        return None;
    }

    let total_client_discount = buckets
        .iter()
        .map(|bucket| bucket.client_discount_amount)
        .sum();
    let subtotal_after_discount = buckets
        .iter()
        .map(|bucket| bucket.subtotal_after_discount)
        .sum::<u32>()
        + detalle_subtotal(pedido);
    let total_ambassador_commission = buckets
        .iter()
        .map(|bucket| bucket.ambassador_commission_amount)
        .sum();

    Some(ReferralApplied {
        code: code.to_string(),
        buckets,
        total_client_discount,
        subtotal_after_discount,
        total_ambassador_commission,
    })
}

fn total_quantity(items: &[OrderItemData], has_liquor: bool) -> u32 {
    items
        .iter()
        .filter(|item| item.has_liquor == has_liquor)
        .map(|item| item.quantity)
        .sum()
}

fn detalle_subtotal(pedido: &PedidoCalculado) -> u32 {
    let liquor_detail = if pedido.es_mayor_con_licor {
        0
    } else {
        pedido.total_con_licor
    };
    let non_liquor_detail = if pedido.es_mayor_sin_licor {
        0
    } else {
        pedido.total_sin_licor
    };

    liquor_detail + non_liquor_detail
}

fn calcular_bucket_referido(
    has_liquor: bool,
    quantity: u32,
    subtotal_before_discount: u32,
    has_boost: bool,
) -> ReferralBucketCalculated {
    let (client_discount_percent, mut ambassador_commission_percent) =
        porcentaje_referido(quantity, has_liquor);
    if has_boost {
        ambassador_commission_percent += 5;
    }
    let client_discount_amount =
        aplicar_descuento_cliente(subtotal_before_discount, client_discount_percent);
    let subtotal_after_discount = subtotal_before_discount - client_discount_amount;
    let ambassador_commission_amount =
        aplicar_porcentaje(subtotal_after_discount, ambassador_commission_percent);

    ReferralBucketCalculated {
        has_liquor,
        quantity,
        subtotal_before_discount,
        client_discount_percent,
        client_discount_amount,
        subtotal_after_discount,
        ambassador_commission_percent,
        ambassador_commission_amount,
    }
}

fn porcentaje_referido(quantity: u32, has_liquor: bool) -> (u8, u8) {
    let tier = resolve_tier(quantity, has_liquor);
    (tier.client_discount_pct, tier.commission_pct)
}

fn aplicar_porcentaje(value: u32, percent: u8) -> u32 {
    (((u64::from(value) * u64::from(percent)) + 50) / 100) as u32
}

fn aplicar_descuento_cliente(value: u32, percent: u8) -> u32 {
    redondear_hacia_arriba_al_siguiente_centenar(aplicar_porcentaje(value, percent))
}

fn redondear_hacia_arriba_al_siguiente_centenar(value: u32) -> u32 {
    if value == 0 {
        return 0;
    }

    value.div_ceil(100) * 100
}

fn calcular_item_mayor(item: &OrderItemData, unit_price: u32) -> ItemCalculated {
    let subtotal = item.quantity * unit_price;

    ItemCalculated {
        flavor: item.flavor.clone(),
        has_liquor: item.has_liquor,
        quantity: item.quantity,
        subtotal,
        is_wholesale: true,
        promo_units: 0,
        unit_price_reference: Some(unit_price),
        persistence_lines: vec![PersistedOrderItem {
            flavor: item.flavor.clone(),
            has_liquor: item.has_liquor,
            quantity: item.quantity,
            unit_price,
            subtotal,
        }],
    }
}

fn calcular_item_sin_licor_detal(item: &OrderItemData) -> ItemCalculated {
    let subtotal = calcular_precio_sin_licor_detal(item.quantity);

    ItemCalculated {
        flavor: item.flavor.clone(),
        has_liquor: false,
        quantity: item.quantity,
        subtotal,
        is_wholesale: false,
        promo_units: 0,
        unit_price_reference: Some(NON_LIQUOR_DETAIL_PRICE),
        persistence_lines: vec![PersistedOrderItem {
            flavor: item.flavor.clone(),
            has_liquor: false,
            quantity: item.quantity,
            unit_price: NON_LIQUOR_DETAIL_PRICE,
            subtotal,
        }],
    }
}

fn calcular_item_licor_detal(item: &OrderItemData, start_position: u32) -> ItemCalculated {
    let mut subtotal = 0;
    let mut regular_units = 0;
    let mut promo_units = 0;

    for offset in 0..item.quantity {
        let is_promo = (start_position + offset + 1) % 2 == 0;
        if is_promo {
            subtotal += LIQUOR_DETAIL_PROMO_PRICE;
            promo_units += 1;
        } else {
            subtotal += LIQUOR_DETAIL_FULL_PRICE;
            regular_units += 1;
        }
    }

    let mut persistence_lines = Vec::with_capacity(2);
    if regular_units > 0 {
        persistence_lines.push(PersistedOrderItem {
            flavor: item.flavor.clone(),
            has_liquor: true,
            quantity: regular_units,
            unit_price: LIQUOR_DETAIL_FULL_PRICE,
            subtotal: regular_units * LIQUOR_DETAIL_FULL_PRICE,
        });
    }
    if promo_units > 0 {
        persistence_lines.push(PersistedOrderItem {
            flavor: item.flavor.clone(),
            has_liquor: true,
            quantity: promo_units,
            unit_price: LIQUOR_DETAIL_PROMO_PRICE,
            subtotal: promo_units * LIQUOR_DETAIL_PROMO_PRICE,
        });
    }

    ItemCalculated {
        flavor: item.flavor.clone(),
        has_liquor: true,
        quantity: item.quantity,
        subtotal,
        is_wholesale: false,
        promo_units,
        unit_price_reference: None,
        persistence_lines,
    }
}

#[cfg(test)]
mod tests {
    use crate::db::models::OrderItemData;

    use super::{
        calcular_pedido, calcular_referido, calcular_referido_con_boost, has_wholesale_bucket,
    };

    #[test]
    fn detail_orders_are_not_referral_eligible() {
        let pedido = calcular_pedido(&[OrderItemData {
            flavor: "Maracumango".to_string(),
            has_liquor: false,
            quantity: 12,
        }]);

        assert!(!has_wholesale_bucket(&pedido));
        assert!(calcular_referido(&pedido, "codigo").is_none());
    }

    #[test]
    fn applies_first_referral_tier_for_20_units() {
        let pedido = calcular_pedido(&[OrderItemData {
            flavor: "Maracumango".to_string(),
            has_liquor: false,
            quantity: 20,
        }]);

        let referido = calcular_referido(&pedido, "codigo").expect("eligible referral");
        assert_eq!(referido.total_client_discount, 9_600);
        assert_eq!(referido.subtotal_after_discount, 86_400);
        assert_eq!(referido.total_ambassador_commission, 12_960);
    }

    #[test]
    fn boost_referral_keeps_client_discount_and_adds_ambassador_commission() {
        let pedido = calcular_pedido(&[OrderItemData {
            flavor: "Maracumango".to_string(),
            has_liquor: false,
            quantity: 20,
        }]);

        let regular =
            calcular_referido_con_boost(&pedido, "codigo", false).expect("eligible referral");
        let boosted =
            calcular_referido_con_boost(&pedido, "codigo", true).expect("eligible referral");

        assert_eq!(boosted.total_client_discount, regular.total_client_discount);
        assert_eq!(
            boosted.subtotal_after_discount,
            regular.subtotal_after_discount
        );
        assert_eq!(regular.total_ambassador_commission, 12_960);
        assert_eq!(boosted.total_ambassador_commission, 17_280);
        assert_eq!(boosted.buckets[0].ambassador_commission_percent, 20);
    }

    #[test]
    fn applies_second_referral_tier_for_50_units() {
        let pedido = calcular_pedido(&[OrderItemData {
            flavor: "Blueberry".to_string(),
            has_liquor: true,
            quantity: 50,
        }]);

        let referido = calcular_referido(&pedido, "codigo").expect("eligible referral");
        assert_eq!(referido.total_client_discount, 28_200);
        assert_eq!(referido.subtotal_after_discount, 206_800);
        assert_eq!(referido.total_ambassador_commission, 37_224);
    }

    #[test]
    fn rounds_client_discount_up_to_next_hundred() {
        let pedido = calcular_pedido(&[OrderItemData {
            flavor: "Maracumango".to_string(),
            has_liquor: false,
            quantity: 22,
        }]);

        let referido = calcular_referido(&pedido, "codigo").expect("eligible referral");
        assert_eq!(referido.total_client_discount, 10_600);
        assert_eq!(referido.subtotal_after_discount, 95_000);
        assert_eq!(referido.total_ambassador_commission, 14_250);
    }

    #[test]
    fn applies_third_referral_tier_for_100_units() {
        let pedido = calcular_pedido(&[OrderItemData {
            flavor: "Blueberry".to_string(),
            has_liquor: false,
            quantity: 100,
        }]);

        let referido = calcular_referido(&pedido, "codigo").expect("eligible referral");
        assert_eq!(referido.total_client_discount, 63_000);
        assert_eq!(referido.subtotal_after_discount, 357_000);
        assert_eq!(referido.total_ambassador_commission, 71_400);
    }

    #[test]
    fn mixed_wholesale_order_combines_bucket_specific_tiers() {
        let pedido = calcular_pedido(&[
            OrderItemData {
                flavor: "Maracumango".to_string(),
                has_liquor: false,
                quantity: 20,
            },
            OrderItemData {
                flavor: "Blueberry".to_string(),
                has_liquor: true,
                quantity: 50,
            },
            OrderItemData {
                flavor: "Bonbonbum".to_string(),
                has_liquor: false,
                quantity: 2,
            },
        ]);

        let referido = calcular_referido(&pedido, "codigo").expect("eligible referral");
        assert_eq!(referido.buckets.len(), 2);
        assert_eq!(referido.total_client_discount, 38_800);
        assert_eq!(referido.subtotal_after_discount, 301_800);
        assert_eq!(referido.total_ambassador_commission, 51_474);
    }
}
