#![allow(dead_code)]

use std::collections::BTreeMap;

use chrono::{DateTime, NaiveDate, NaiveTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct ConversationStateData {
    pub items: Vec<OrderItemData>,
    pub delivery_type: Option<String>,
    pub scheduled_date: Option<String>,
    pub scheduled_time: Option<String>,
    pub customer_review_scope: Option<String>,
    pub payment_method: Option<String>,
    pub referral_code: Option<String>,
    pub referral_has_boost: bool,
    pub referral_discount_total: Option<i32>,
    pub ambassador_commission_total: Option<i32>,
    pub delivery_cost: Option<i32>,
    pub total_final: Option<i32>,
    pub receipt_media_id: Option<String>,
    pub receipt_timer_started_at: Option<DateTime<Utc>>,
    pub advisor_target_phone: Option<String>,
    pub advisor_reply_threads: BTreeMap<String, String>,
    pub advisor_timer_started_at: Option<DateTime<Utc>>,
    pub advisor_timer_expired: bool,
    pub relay_timer_started_at: Option<DateTime<Utc>>,
    pub relay_kind: Option<String>,
    pub advisor_proposed_hour: Option<String>,
    pub client_counter_hour: Option<String>,
    pub schedule_resume_target: Option<String>,
    pub current_order_id: Option<i32>,
    pub editing_address: bool,
    pub receipt_timer_expired: bool,
    pub pending_has_liquor: Option<bool>,
    pub pending_flavor: Option<String>,
    pub conversation_abandon_started_at: Option<DateTime<Utc>>,
    pub conversation_abandon_reminder_sent: bool,
    /// True una vez que el pedido `current_order_id` quedó CONFIRMADO. Bloquea
    /// crear una orden duplicada: para tocarlo de nuevo hay que reabrirlo con
    /// `modify_confirmed_order`, y para un pedido aparte hay que limpiar con
    /// `start_new_order` (ver docs/canary-fixes-2026-07-19.md hallazgo A).
    pub order_confirmed: bool,
    /// Cifras ya acumuladas en analytics para `current_order_id`. Permite, al
    /// reabrir y re-confirmar, mandar el DELTA (nuevo − viejo) en vez de sumar
    /// de nuevo el total completo (analytics es incremental).
    pub confirmed_order_snapshot: Option<ConfirmedOrderSnapshot>,
    /// True cuando el tema del código de referido ya se resolvió en un pedido
    /// mayorista: se aplicó un código válido o el cliente dijo que no tiene.
    /// `finalize_checkout` lo exige antes de confirmar un pedido mayorista
    /// (ver docs/canary-fixes-2026-07-19.md item 9).
    pub referral_prompt_resolved: bool,
    /// True una vez que se envió el saludo de bienvenida fijo (motor agente).
    /// El primer mensaje del cliente recibe un texto fijo sin LLM; de ahí en
    /// adelante todo lo maneja el LLM (ver docs/canary-fixes-2026-07-19.md item 3).
    pub has_greeted: bool,
    /// Nombre/celular que entregó Meta por webhook (contacts[].profile.name y
    /// messages[].from). Base inmutable: el cliente NO los edita. Los campos
    /// `customer_name`/`customer_phone` guardan lo personalizado (editable sin
    /// validación) y el paquete al asesor muestra ambos si difieren
    /// (ver docs/canary-fixes-2026-07-19.md hallazgo C).
    pub meta_customer_name: Option<String>,
    pub meta_customer_phone: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ConfirmedOrderSnapshot {
    pub total_spent_cop: i32,
    pub total_units_purchased: i32,
    pub referral_discount_cop: i32,
    pub ambassador_commission_cop: i32,
    pub referral_code: Option<String>,
}

impl Default for ConversationStateData {
    fn default() -> Self {
        Self {
            items: Vec::new(),
            delivery_type: None,
            scheduled_date: None,
            scheduled_time: None,
            customer_review_scope: None,
            payment_method: None,
            referral_code: None,
            referral_has_boost: false,
            referral_discount_total: None,
            ambassador_commission_total: None,
            delivery_cost: None,
            total_final: None,
            receipt_media_id: None,
            receipt_timer_started_at: None,
            advisor_target_phone: None,
            advisor_reply_threads: BTreeMap::new(),
            advisor_timer_started_at: None,
            advisor_timer_expired: false,
            relay_timer_started_at: None,
            relay_kind: None,
            advisor_proposed_hour: None,
            client_counter_hour: None,
            schedule_resume_target: None,
            current_order_id: None,
            editing_address: false,
            receipt_timer_expired: false,
            pending_has_liquor: None,
            pending_flavor: None,
            conversation_abandon_started_at: None,
            conversation_abandon_reminder_sent: false,
            order_confirmed: false,
            confirmed_order_snapshot: None,
            referral_prompt_resolved: false,
            has_greeted: false,
            meta_customer_name: None,
            meta_customer_phone: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct OrderItemData {
    pub flavor: String,
    pub has_liquor: bool,
    pub quantity: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct Conversation {
    pub id: i32,
    pub phone_number: String,
    pub state: String,
    pub state_data: sqlx::types::Json<ConversationStateData>,
    pub customer_name: Option<String>,
    pub customer_phone: Option<String>,
    pub delivery_address: Option<String>,
    pub last_message_at: DateTime<Utc>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct Order {
    pub id: i32,
    pub conversation_id: i32,
    pub delivery_type: String,
    pub scheduled_date: Option<NaiveDate>,
    pub scheduled_time: Option<NaiveTime>,
    pub scheduled_date_text: Option<String>,
    pub scheduled_time_text: Option<String>,
    pub payment_method: String,
    pub receipt_media_id: Option<String>,
    pub referral_code: Option<String>,
    pub referral_discount_total: Option<i32>,
    pub ambassador_commission_total: Option<i32>,
    pub delivery_cost: Option<i32>,
    pub total_estimated: i32,
    pub total_final: Option<i32>,
    pub status: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct OrderItem {
    pub id: i32,
    pub order_id: i32,
    pub flavor: String,
    pub has_liquor: bool,
    pub quantity: i32,
    pub unit_price: i32,
    pub subtotal: i32,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct Customer {
    pub phone_number_meta: String,
    pub phone_number_manual: Option<String>,
    pub customer_name_meta: Option<String>,
    pub customer_name_manual: Option<String>,
    pub customer_username: Option<String>,
    pub delivery_address_last: Option<String>,
    pub total_spent_cop: i32,
    pub total_units_purchased: i32,
    pub first_contact_at: Option<DateTime<Utc>>,
    pub last_contact_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct ReferralCodeAnalytics {
    pub code: String,
    pub times_used: i32,
    pub total_discount_generated_cop: i32,
    pub total_commission_generated_cop: i32,
    pub total_units_purchased: i32,
    pub total_sales_cop: i32,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[cfg(test)]
mod tests {
    use super::ConversationStateData;

    #[test]
    fn conversation_state_data_deserializes_legacy_json_without_new_fields() {
        let state_data: ConversationStateData = serde_json::from_str(
            r#"
            {
              "items": [],
              "delivery_type": "immediate",
              "referral_code": "rider332",
              "advisor_target_phone": "573001234567"
            }
            "#,
        )
        .expect("legacy state data should deserialize");

        assert!(!state_data.referral_has_boost);
        assert!(state_data.advisor_reply_threads.is_empty());
        assert_eq!(state_data.referral_code.as_deref(), Some("rider332"));
        assert_eq!(
            state_data.advisor_target_phone.as_deref(),
            Some("573001234567")
        );
    }
}
