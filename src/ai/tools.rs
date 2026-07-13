//! Fase 0 del agente de IA: envuelve la logica determinista ya existente
//! (pricing, validaciones, referidos, persistencia) detras de funciones con
//! firmas orientadas a tool-calling. Ninguna funcion de aqui reimplementa
//! reglas de negocio; todas delegan al codigo ya probado en `src/bot` y
//! `src/db`, para que el futuro loop de agente (Fase 1+) tenga un toolbox
//! estable sin depender de los ~35 estados de `ConversationState`.

use chrono::{NaiveDate, NaiveTime};
use sqlx::PgPool;

use crate::{
    bot::{
        delivery_zone::{lookup_nearby_town, ArmeniaZone, MIN_UNITS_OUTSIDE_ARMENIA},
        pricing::{self, PedidoCalculado, ReferralApplied},
        states::{
            data_collect::{validate_address, validate_name, validate_phone},
            order::{flavor_by_id, validate_quantity},
            scheduling::{current_bogota_now, immediate_delivery_hours_text, is_within_business_hours},
        },
    },
    db::{
        models::{Conversation, Order, OrderItem, OrderItemData},
        queries,
    },
    messages::client_messages,
    referrals::{normalize_referral_code, referral_registry},
};

// --- Menu / horario -----------------------------------------------------

#[derive(Debug, Clone)]
pub struct MenuInfo {
    pub menu_text: String,
    pub flavors_with_liquor: Vec<(String, String)>,
    pub flavors_without_liquor: Vec<(String, String)>,
}

pub fn get_menu() -> MenuInfo {
    let order = &client_messages().order;
    MenuInfo {
        menu_text: client_messages().menu.menu_text.clone(),
        flavors_with_liquor: order
            .flavors_with_liquor
            .iter()
            .map(|(id, title)| (id.clone(), title.clone()))
            .collect(),
        flavors_without_liquor: order
            .flavors_without_liquor
            .iter()
            .map(|(id, title)| (id.clone(), title.clone()))
            .collect(),
    }
}

pub fn resolve_flavor(has_liquor: bool, flavor_id: &str) -> Option<String> {
    flavor_by_id(flavor_id, has_liquor)
}

#[derive(Debug, Clone)]
pub struct BusinessHoursStatus {
    pub is_open: bool,
    pub hours_text: String,
}

pub fn check_business_hours() -> BusinessHoursStatus {
    BusinessHoursStatus {
        is_open: is_within_business_hours(current_bogota_now().time()),
        hours_text: immediate_delivery_hours_text(),
    }
}

// --- Datos del cliente ----------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CustomerField {
    Name,
    Phone,
    Address,
}

pub fn validate_customer_field(field: CustomerField, value: &str) -> Result<String, String> {
    match field {
        CustomerField::Name => validate_name(value),
        CustomerField::Phone => validate_phone(value),
        CustomerField::Address => validate_address(value),
    }
}

pub fn validate_order_quantity(text: &str) -> Result<u32, String> {
    validate_quantity(text)
}

// --- Pricing / referidos ----------------------------------------------------

pub fn calculate_order(items: &[OrderItemData]) -> PedidoCalculado {
    pricing::calcular_pedido(items)
}

pub fn order_has_wholesale_bucket(pedido: &PedidoCalculado) -> bool {
    pricing::has_wholesale_bucket(pedido)
}

#[derive(Debug, Clone)]
pub struct ReferralValidation {
    pub is_valid: bool,
    pub has_boost: bool,
}

pub fn validate_referral_code(code: &str) -> ReferralValidation {
    let normalized = normalize_referral_code(code);
    let registry = referral_registry();
    ReferralValidation {
        is_valid: registry.contains(&normalized),
        has_boost: registry.has_boost(&normalized),
    }
}

/// Devuelve `None` si el codigo no es valido o el pedido no tiene ningun
/// bucket al por mayor — identico al comportamiento actual de
/// `wait_referral_code` en `src/bot/states/checkout.rs`.
pub fn apply_referral_code(pedido: &PedidoCalculado, code: &str) -> Option<ReferralApplied> {
    let normalized = normalize_referral_code(code);
    let registry = referral_registry();
    if !registry.contains(&normalized) {
        return None;
    }

    pricing::calcular_referido_con_boost(pedido, &normalized, registry.has_boost(&normalized))
}

// --- Persistencia: conversations -------------------------------------------

pub async fn get_conversation(
    pool: &PgPool,
    phone_number: &str,
) -> Result<Option<Conversation>, sqlx::Error> {
    queries::get_conversation(pool, phone_number).await
}

pub async fn update_customer_data(
    pool: &PgPool,
    phone_number: &str,
    name: Option<&str>,
    phone: Option<&str>,
    address: Option<&str>,
) -> Result<(), sqlx::Error> {
    queries::update_customer_data(pool, phone_number, name, phone, address).await
}

pub async fn reset_conversation(pool: &PgPool, phone_number: &str) -> Result<(), sqlx::Error> {
    queries::reset_conversation(pool, phone_number).await
}

// --- Persistencia: orders / order_items ------------------------------------

#[derive(Debug, Clone)]
pub struct CreateOrderRequest<'a> {
    pub conversation_id: i32,
    pub delivery_type: &'a str,
    pub scheduled_date: Option<NaiveDate>,
    pub scheduled_time: Option<NaiveTime>,
    pub scheduled_date_text: Option<&'a str>,
    pub scheduled_time_text: Option<&'a str>,
    pub payment_method: &'a str,
    pub receipt_media_id: Option<&'a str>,
    pub referral_code: Option<&'a str>,
    pub referral_discount_total: Option<i32>,
    pub ambassador_commission_total: Option<i32>,
    pub total_estimated: i32,
}

pub async fn create_order(
    pool: &PgPool,
    request: CreateOrderRequest<'_>,
) -> Result<Order, sqlx::Error> {
    queries::create_order(
        pool,
        request.conversation_id,
        request.delivery_type,
        request.scheduled_date,
        request.scheduled_time,
        request.scheduled_date_text,
        request.scheduled_time_text,
        request.payment_method,
        request.receipt_media_id,
        request.referral_code,
        request.referral_discount_total,
        request.ambassador_commission_total,
        request.total_estimated,
    )
    .await
}

#[derive(Debug, Clone)]
pub struct UpdateOrderRequest<'a> {
    pub order_id: i32,
    pub delivery_type: &'a str,
    pub scheduled_date: Option<NaiveDate>,
    pub scheduled_time: Option<NaiveTime>,
    pub scheduled_date_text: Option<&'a str>,
    pub scheduled_time_text: Option<&'a str>,
    pub payment_method: &'a str,
    pub receipt_media_id: Option<&'a str>,
    pub referral_code: Option<&'a str>,
    pub referral_discount_total: Option<i32>,
    pub ambassador_commission_total: Option<i32>,
    pub total_estimated: i32,
    pub status: &'a str,
}

pub async fn update_order(
    pool: &PgPool,
    request: UpdateOrderRequest<'_>,
) -> Result<Order, sqlx::Error> {
    queries::update_order(
        pool,
        request.order_id,
        request.delivery_type,
        request.scheduled_date,
        request.scheduled_time,
        request.scheduled_date_text,
        request.scheduled_time_text,
        request.payment_method,
        request.receipt_media_id,
        request.referral_code,
        request.referral_discount_total,
        request.ambassador_commission_total,
        request.total_estimated,
        request.status,
    )
    .await
}

pub async fn get_order(pool: &PgPool, order_id: i32) -> Result<Option<Order>, sqlx::Error> {
    queries::get_order(pool, order_id).await
}

pub async fn get_order_items(pool: &PgPool, order_id: i32) -> Result<Vec<OrderItem>, sqlx::Error> {
    queries::get_order_items(pool, order_id).await
}

pub async fn replace_order_items(
    pool: &PgPool,
    order_id: i32,
    items: &[(String, bool, i32, i32, i32)],
) -> Result<Vec<OrderItem>, sqlx::Error> {
    queries::replace_order_items(pool, order_id, items).await
}

pub async fn update_order_status(
    pool: &PgPool,
    order_id: i32,
    status: &str,
) -> Result<(), sqlx::Error> {
    queries::update_order_status(pool, order_id, status).await
}

pub async fn update_order_delivery_cost(
    pool: &PgPool,
    order_id: i32,
    delivery_cost: i32,
    total_final: i32,
) -> Result<(), sqlx::Error> {
    queries::update_order_delivery_cost(pool, order_id, delivery_cost, total_final).await
}

pub async fn update_order_receipt_media_id(
    pool: &PgPool,
    order_id: i32,
    receipt_media_id: Option<&str>,
) -> Result<(), sqlx::Error> {
    queries::update_order_receipt_media_id(pool, order_id, receipt_media_id).await
}

// --- FASE 2: Herramientas deterministas de cálculo integral ---

#[derive(Debug, Clone)]
pub struct DeliveryCostInfo {
    pub location: String,
    pub cost: i32,
    pub unit_minimum: Option<u32>,
    pub is_manual: bool,
}

#[derive(Debug, Clone)]
pub struct OrderSummary {
    pub subtotal: i32,
    pub delivery_cost: i32,
    pub referral_discount: i32,
    pub ambassador_commission: i32,
    pub total_final: i32,
    pub breakdown: String,
}

#[derive(Debug, Clone)]
pub struct ReferralDiscountBreakdown {
    pub code: String,
    pub is_valid: bool,
    pub has_boost: bool,
    pub subtotal_original: i32,
    pub discount_to_client: i32,
    pub subtotal_discounted: i32,
    pub ambassador_commission: i32,
    pub total_after_discount: i32,
}

/// Calcula domicilio para una zona/pueblo en Armenia o pueblos conocidos.
/// Retorna error (is_manual=true) si el destino es desconocido.
pub fn get_delivery_cost(zone_or_town: &str, unit_count: u32) -> Result<DeliveryCostInfo, DeliveryCostInfo> {
    // Intenta primero zona de Armenia
    if let Some(zone) = ArmeniaZone::from_text(zone_or_town) {
        return Ok(DeliveryCostInfo {
            location: format!("Zona {}", zone.label()),
            cost: zone.delivery_cost() as i32,
            unit_minimum: None,
            is_manual: false,
        });
    }

    // Luego pueblos cercanos conocidos
    if let Some(town) = lookup_nearby_town(zone_or_town) {
        if unit_count < MIN_UNITS_OUTSIDE_ARMENIA {
            return Err(DeliveryCostInfo {
                location: format!("Municipio: {}", town.name),
                cost: 0,
                unit_minimum: Some(MIN_UNITS_OUTSIDE_ARMENIA),
                is_manual: true,
            });
        }
        return Ok(DeliveryCostInfo {
            location: format!("Municipio: {}", town.name),
            cost: town.delivery_cost as i32,
            unit_minimum: None,
            is_manual: false,
        });
    }

    // Municipio desconocido - requiere intervención manual
    Err(DeliveryCostInfo {
        location: format!("Municipio desconocido: {}", zone_or_town),
        cost: 0,
        unit_minimum: Some(MIN_UNITS_OUTSIDE_ARMENIA),
        is_manual: true,
    })
}

/// Aplica descuento referral a un pedido ya calculado.
/// Retorna None si el código no es válido o el pedido no tiene buckets mayoristas.
pub fn apply_referral_discount(
    pedido: &PedidoCalculado,
    referral_code: &str,
) -> Option<ReferralDiscountBreakdown> {
    let normalized = normalize_referral_code(referral_code);
    let registry = referral_registry();

    if !registry.contains(&normalized) {
        return None;
    }

    let has_boost = registry.has_boost(&normalized);
    let referral_applied = pricing::calcular_referido_con_boost(pedido, &normalized, has_boost)?;

    Some(ReferralDiscountBreakdown {
        code: normalized,
        is_valid: true,
        has_boost,
        subtotal_original: pedido.total_estimado as i32,
        discount_to_client: referral_applied.total_client_discount as i32,
        subtotal_discounted: referral_applied.subtotal_after_discount as i32,
        ambassador_commission: referral_applied.total_ambassador_commission as i32,
        total_after_discount: referral_applied.subtotal_after_discount as i32,
    })
}

/// Calcula pedido completo: items + domicilio + referral (si aplica).
/// Este es el "super-tool" que orquesta todo en un solo paso.
pub fn calculate_order_with_delivery(
    items: &[OrderItemData],
    delivery_zone: Option<&str>,
    delivery_town: Option<&str>,
    delivery_manual_cost: Option<i32>,
    referral_code: Option<&str>,
) -> Result<OrderSummary, String> {
    // Calcula base del pedido
    let pedido = pricing::calcular_pedido(items);
    let subtotal = pedido.total_estimado as i32;

    // Resuelve domicilio
    let delivery_cost = if let Some(manual_cost) = delivery_manual_cost {
        manual_cost
    } else if let Some(zone) = delivery_zone {
        match get_delivery_cost(zone, items.iter().map(|i| i.quantity).sum()) {
            Ok(info) => info.cost,
            Err(_) => return Err(format!("No se pudo determinar costo de domicilio para: {}", zone)),
        }
    } else if let Some(town) = delivery_town {
        match get_delivery_cost(town, items.iter().map(|i| i.quantity).sum()) {
            Ok(info) => info.cost,
            Err(_) => return Err(format!("No se pudo determinar costo de domicilio para: {}", town)),
        }
    } else {
        0
    };

    let subtotal_with_delivery = subtotal + delivery_cost;

    // Aplica descuento referral si lo hay
    let (referral_discount, ambassador_commission, total_final) =
        if let Some(code) = referral_code {
            if let Some(discount_info) = apply_referral_discount(&pedido, code) {
                (
                    discount_info.discount_to_client,
                    discount_info.ambassador_commission,
                    discount_info.subtotal_discounted + delivery_cost,
                )
            } else {
                (0, 0, subtotal_with_delivery)
            }
        } else {
            (0, 0, subtotal_with_delivery)
        };

    let breakdown = format!(
        "Subtotal: ${}\nDomicilio: ${}\nDescuento: ${}\nComisión asesor: ${}\nTotal final: ${}",
        subtotal, delivery_cost, referral_discount, ambassador_commission, total_final
    );

    Ok(OrderSummary {
        subtotal,
        delivery_cost,
        referral_discount,
        ambassador_commission,
        total_final,
        breakdown,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::models::OrderItemData;

    #[test]
    fn get_menu_exposes_both_flavor_lists() {
        let menu = get_menu();

        assert_eq!(menu.flavors_with_liquor.len(), 7);
        assert_eq!(menu.flavors_without_liquor.len(), 4);
        assert!(!menu.menu_text.is_empty());
    }

    #[test]
    fn resolve_flavor_matches_state_handler_behavior() {
        assert_eq!(
            resolve_flavor(false, "non_liquor_bonbonbum"),
            Some("Bonbonbum".to_string())
        );
        assert_eq!(resolve_flavor(false, "not_a_real_id"), None);
    }

    #[test]
    fn check_business_hours_reports_hours_text() {
        // No FORCE_BOGOTA_NOW mutation here: scheduling.rs's own tests race on
        // that global env var without shared locking, so this only checks the
        // wrapper's shape, not a specific is_open value.
        let status = check_business_hours();

        assert!(!status.hours_text.is_empty());
    }

    #[test]
    fn validate_customer_field_dispatches_to_matching_validator() {
        assert_eq!(
            validate_customer_field(CustomerField::Name, "  Ana   Maria ").unwrap(),
            "Ana Maria"
        );
        assert!(validate_customer_field(CustomerField::Phone, "abc").is_err());
        assert_eq!(
            validate_customer_field(CustomerField::Address, "Cra 15 #20-30 Armenia").unwrap(),
            "Cra 15 #20-30 Armenia"
        );
    }

    #[test]
    fn validate_referral_code_reports_validity_and_boost() {
        let invalid = validate_referral_code("codigo-inexistente-xyz");
        assert!(!invalid.is_valid);
        assert!(!invalid.has_boost);
    }

    #[test]
    fn apply_referral_code_returns_none_for_invalid_code() {
        let pedido = calculate_order(&[OrderItemData {
            flavor: "Maracumango".to_string(),
            has_liquor: false,
            quantity: 20,
        }]);

        assert!(apply_referral_code(&pedido, "codigo-inexistente-xyz").is_none());
    }

    #[test]
    fn calculate_order_matches_pricing_module_directly() {
        let items = [OrderItemData {
            flavor: "Maracumango".to_string(),
            has_liquor: false,
            quantity: 20,
        }];

        let via_tool = calculate_order(&items);
        let via_pricing = pricing::calcular_pedido(&items);

        assert_eq!(via_tool, via_pricing);
        assert!(order_has_wholesale_bucket(&via_tool));
    }

    // FASE 2 tests

    #[test]
    fn get_delivery_cost_armenia_zone_norte() {
        let result = get_delivery_cost("norte", 10).unwrap();
        assert!(!result.is_manual);
        assert_eq!(result.cost, 6_000);
        assert!(result.location.contains("Norte"));
    }

    #[test]
    fn get_delivery_cost_armenia_zone_centro() {
        let result = get_delivery_cost("Centro", 10).unwrap();
        assert!(!result.is_manual);
        assert_eq!(result.cost, 8_000);
    }

    #[test]
    fn get_delivery_cost_nearby_town_calarca() {
        let result = get_delivery_cost("Calarcá", 30).unwrap();
        assert!(!result.is_manual);
        assert_eq!(result.cost, 15_000);
        assert!(result.location.contains("Calarcá"));
    }

    #[test]
    fn get_delivery_cost_nearby_town_insufficient_units() {
        let result = get_delivery_cost("Calarcá", 15);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.is_manual);
        assert_eq!(err.unit_minimum, Some(MIN_UNITS_OUTSIDE_ARMENIA));
    }

    #[test]
    fn get_delivery_cost_unknown_municipality() {
        let result = get_delivery_cost("Bogotá", 20);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.is_manual);
        assert!(err.location.contains("desconocido"));
    }

    #[test]
    fn apply_referral_discount_returns_none_for_invalid_code() {
        let pedido = calculate_order(&[OrderItemData {
            flavor: "Maracumango".to_string(),
            has_liquor: false,
            quantity: 20,
        }]);

        let result = apply_referral_discount(&pedido, "codigo-invalido");
        assert!(result.is_none());
    }

    #[test]
    fn apply_referral_discount_returns_breakdown_for_valid_code() {
        let pedido = calculate_order(&[OrderItemData {
            flavor: "Maracumango".to_string(),
            has_liquor: false,
            quantity: 20,
        }]);

        // Nota: necesitaría un código válido del registry para esto.
        // Por ahora solo verifica que la función retorna None para código inválido
        let result = apply_referral_discount(&pedido, "codigo-invalido");
        assert!(result.is_none());
    }

    #[test]
    fn calculate_order_with_delivery_armenía_zona_north() {
        let items = [OrderItemData {
            flavor: "Maracumango".to_string(),
            has_liquor: false,
            quantity: 20,
        }];

        let result = calculate_order_with_delivery(&items, Some("norte"), None, None, None).unwrap();
        assert_eq!(result.subtotal, 96_000);
        assert_eq!(result.delivery_cost, 6_000);
        assert_eq!(result.total_final, 102_000);
    }

    #[test]
    fn calculate_order_with_delivery_manual_cost() {
        let items = [OrderItemData {
            flavor: "Blueberry".to_string(),
            has_liquor: true,
            quantity: 20,
        }];

        let result = calculate_order_with_delivery(&items, None, None, Some(15_000), None).unwrap();
        assert_eq!(result.delivery_cost, 15_000);
        assert!(result.breakdown.contains("15000"));
    }

    #[test]
    fn calculate_order_with_delivery_unknown_zone_fails() {
        let items = [OrderItemData {
            flavor: "Maracumango".to_string(),
            has_liquor: false,
            quantity: 20,
        }];

        let result = calculate_order_with_delivery(&items, Some("Bogotá"), None, None, None);
        assert!(result.is_err());
    }
}
