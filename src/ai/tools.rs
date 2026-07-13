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
}
