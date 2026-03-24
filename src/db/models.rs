#![allow(dead_code)]

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
    pub payment_method: Option<String>,
    pub receipt_media_id: Option<String>,
    pub receipt_timer_started_at: Option<DateTime<Utc>>,
    pub advisor_target_phone: Option<String>,
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
}

impl Default for ConversationStateData {
    fn default() -> Self {
        Self {
            items: Vec::new(),
            delivery_type: None,
            scheduled_date: None,
            scheduled_time: None,
            payment_method: None,
            receipt_media_id: None,
            receipt_timer_started_at: None,
            advisor_target_phone: None,
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
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct OrderItemData {
    pub flavor: String,
    pub has_liquor: bool,
    pub quantity: u32,
}

#[derive(Debug, Clone, FromRow)]
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

#[derive(Debug, Clone, FromRow)]
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
    pub delivery_cost: Option<i32>,
    pub total_estimated: i32,
    pub total_final: Option<i32>,
    pub status: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, FromRow)]
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
