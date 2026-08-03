#![allow(dead_code)]

use chrono::{NaiveDate, NaiveTime};
use sqlx::{types::Json, PgPool};

use super::models::{
    Conversation, ConversationStateData, Customer, CustomerAddress, Order, OrderItem, ReferralCodeAnalytics,
};
use crate::bot::delivery_zone;

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct ActiveTimerConversation {
    pub id: i32,
    pub phone_number: String,
    pub state: String,
    pub state_data: Json<ConversationStateData>,
    pub customer_name: Option<String>,
    pub customer_phone: Option<String>,
    pub delivery_address: Option<String>,
    pub last_message_at: chrono::DateTime<chrono::Utc>,
    pub human_takeover_until: Option<chrono::DateTime<chrono::Utc>>,
}

pub async fn get_conversation(
    pool: &PgPool,
    phone_number: &str,
) -> Result<Option<Conversation>, sqlx::Error> {
    sqlx::query_as::<_, Conversation>(
        r#"
        SELECT id, phone_number, state, state_data, customer_name, customer_phone, delivery_address, last_message_at, created_at, human_takeover_until
        FROM conversations
        WHERE phone_number = $1
        "#,
    )
    .bind(phone_number)
    .fetch_optional(pool)
    .await
}

pub async fn list_conversations(pool: &PgPool) -> Result<Vec<Conversation>, sqlx::Error> {
    sqlx::query_as::<_, Conversation>(
        r#"
        SELECT id, phone_number, state, state_data, customer_name, customer_phone, delivery_address, last_message_at, created_at, human_takeover_until
        FROM conversations
        ORDER BY last_message_at DESC, id DESC
        "#,
    )
    .fetch_all(pool)
    .await
}

pub async fn create_conversation(
    pool: &PgPool,
    phone_number: &str,
) -> Result<Conversation, sqlx::Error> {
    sqlx::query_as::<_, Conversation>(
        r#"
        INSERT INTO conversations (phone_number, state, state_data)
        VALUES ($1, 'main_menu', $2)
        RETURNING id, phone_number, state, state_data, customer_name, customer_phone, delivery_address, last_message_at, created_at, human_takeover_until
        "#,
    )
    .bind(phone_number)
    .bind(Json(ConversationStateData::default()))
    .fetch_one(pool)
    .await
}

/// Fase 2: se llama SOLO desde `POST /internal/advisor/send` (texto libre del
/// asesor al cliente). `until` es una ventana deslizante: cada llamada nueva
/// la reemplaza, no la acumula. `POST /internal/advisor/reply` NO llama esto
/// a propósito — ese endpoint existe para que el bot SIGA el checkout
/// automático después de que el asesor destraba una pregunta puntual.
pub async fn set_human_takeover(
    pool: &PgPool,
    phone_number: &str,
    until: chrono::DateTime<chrono::Utc>,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        r#"
        UPDATE conversations
        SET human_takeover_until = $2
        WHERE phone_number = $1
        "#,
    )
    .bind(phone_number)
    .bind(until)
    .execute(pool)
    .await?;

    Ok(())
}

/// Devuelve la conversación al bot antes de que venza la ventana de
/// `set_human_takeover` — llamado desde `POST /internal/advisor/release`.
pub async fn clear_human_takeover(pool: &PgPool, phone_number: &str) -> Result<(), sqlx::Error> {
    sqlx::query(
        r#"
        UPDATE conversations
        SET human_takeover_until = NULL
        WHERE phone_number = $1
        "#,
    )
    .bind(phone_number)
    .execute(pool)
    .await?;

    Ok(())
}

pub async fn update_state(
    pool: &PgPool,
    phone_number: &str,
    state: &str,
    state_data: &ConversationStateData,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        r#"
        UPDATE conversations
        SET state = $2, state_data = $3, last_message_at = NOW()
        WHERE phone_number = $1
        "#,
    )
    .bind(phone_number)
    .bind(state)
    .bind(Json(state_data.clone()))
    .execute(pool)
    .await?;

    Ok(())
}

pub async fn update_customer_data(
    pool: &PgPool,
    phone_number: &str,
    name: Option<&str>,
    phone: Option<&str>,
    address: Option<&str>,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        r#"
        UPDATE conversations
        SET customer_name = COALESCE($2, customer_name),
            customer_phone = COALESCE($3, customer_phone),
            delivery_address = COALESCE($4, delivery_address),
            last_message_at = NOW()
        WHERE phone_number = $1
        "#,
    )
    .bind(phone_number)
    .bind(name)
    .bind(phone)
    .bind(address)
    .execute(pool)
    .await?;

    Ok(())
}

pub async fn update_last_message(pool: &PgPool, phone_number: &str) -> Result<(), sqlx::Error> {
    sqlx::query(
        r#"
        UPDATE conversations
        SET last_message_at = NOW()
        WHERE phone_number = $1
        "#,
    )
    .bind(phone_number)
    .execute(pool)
    .await?;

    Ok(())
}

pub async fn create_order(
    pool: &PgPool,
    conversation_id: i32,
    delivery_type: &str,
    scheduled_date: Option<NaiveDate>,
    scheduled_time: Option<NaiveTime>,
    scheduled_date_text: Option<&str>,
    scheduled_time_text: Option<&str>,
    payment_method: &str,
    receipt_media_id: Option<&str>,
    referral_code: Option<&str>,
    referral_discount_total: Option<i32>,
    ambassador_commission_total: Option<i32>,
    total_estimated: i32,
) -> Result<Order, sqlx::Error> {
    sqlx::query_as::<_, Order>(
        r#"
        INSERT INTO orders (
            conversation_id, delivery_type, scheduled_date, scheduled_time,
            scheduled_date_text, scheduled_time_text,
            payment_method, receipt_media_id, referral_code, referral_discount_total,
            ambassador_commission_total, total_estimated
        )
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12)
        RETURNING id, conversation_id, delivery_type, scheduled_date, scheduled_time,
                  scheduled_date_text, scheduled_time_text,
                  payment_method, receipt_media_id, referral_code, referral_discount_total,
                  ambassador_commission_total, delivery_cost, total_estimated, total_final,
                  status, created_at
        "#,
    )
    .bind(conversation_id)
    .bind(delivery_type)
    .bind(scheduled_date)
    .bind(scheduled_time)
    .bind(scheduled_date_text)
    .bind(scheduled_time_text)
    .bind(payment_method)
    .bind(receipt_media_id)
    .bind(referral_code)
    .bind(referral_discount_total)
    .bind(ambassador_commission_total)
    .bind(total_estimated)
    .fetch_one(pool)
    .await
}

pub async fn update_order(
    pool: &PgPool,
    order_id: i32,
    delivery_type: &str,
    scheduled_date: Option<NaiveDate>,
    scheduled_time: Option<NaiveTime>,
    scheduled_date_text: Option<&str>,
    scheduled_time_text: Option<&str>,
    payment_method: &str,
    receipt_media_id: Option<&str>,
    referral_code: Option<&str>,
    referral_discount_total: Option<i32>,
    ambassador_commission_total: Option<i32>,
    total_estimated: i32,
    status: &str,
) -> Result<Order, sqlx::Error> {
    sqlx::query_as::<_, Order>(
        r#"
        UPDATE orders
        SET delivery_type = $2,
            scheduled_date = $3,
            scheduled_time = $4,
            scheduled_date_text = $5,
            scheduled_time_text = $6,
            payment_method = $7,
            receipt_media_id = $8,
            referral_code = $9,
            referral_discount_total = $10,
            ambassador_commission_total = $11,
            total_estimated = $12,
            status = $13
        WHERE id = $1
        RETURNING id, conversation_id, delivery_type, scheduled_date, scheduled_time,
                  scheduled_date_text, scheduled_time_text,
                  payment_method, receipt_media_id, referral_code, referral_discount_total,
                  ambassador_commission_total, delivery_cost, total_estimated, total_final,
                  status, created_at
        "#,
    )
    .bind(order_id)
    .bind(delivery_type)
    .bind(scheduled_date)
    .bind(scheduled_time)
    .bind(scheduled_date_text)
    .bind(scheduled_time_text)
    .bind(payment_method)
    .bind(receipt_media_id)
    .bind(referral_code)
    .bind(referral_discount_total)
    .bind(ambassador_commission_total)
    .bind(total_estimated)
    .bind(status)
    .fetch_one(pool)
    .await
}

pub async fn add_order_item(
    pool: &PgPool,
    order_id: i32,
    flavor: &str,
    has_liquor: bool,
    quantity: i32,
    unit_price: i32,
    subtotal: i32,
) -> Result<OrderItem, sqlx::Error> {
    sqlx::query_as::<_, OrderItem>(
        r#"
        INSERT INTO order_items (order_id, flavor, has_liquor, quantity, unit_price, subtotal)
        VALUES ($1, $2, $3, $4, $5, $6)
        RETURNING id, order_id, flavor, has_liquor, quantity, unit_price, subtotal, created_at
        "#,
    )
    .bind(order_id)
    .bind(flavor)
    .bind(has_liquor)
    .bind(quantity)
    .bind(unit_price)
    .bind(subtotal)
    .fetch_one(pool)
    .await
}

pub async fn replace_order_items(
    pool: &PgPool,
    order_id: i32,
    items: &[(String, bool, i32, i32, i32)],
) -> Result<Vec<OrderItem>, sqlx::Error> {
    sqlx::query(
        r#"
        DELETE FROM order_items
        WHERE order_id = $1
        "#,
    )
    .bind(order_id)
    .execute(pool)
    .await?;

    let mut created = Vec::with_capacity(items.len());
    for (flavor, has_liquor, quantity, unit_price, subtotal) in items {
        created.push(
            add_order_item(
                pool,
                order_id,
                flavor,
                *has_liquor,
                *quantity,
                *unit_price,
                *subtotal,
            )
            .await?,
        );
    }

    Ok(created)
}

pub async fn update_order_status(
    pool: &PgPool,
    order_id: i32,
    status: &str,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        r#"
        UPDATE orders
        SET status = $2
        WHERE id = $1
        "#,
    )
    .bind(order_id)
    .bind(status)
    .execute(pool)
    .await?;

    Ok(())
}

pub async fn update_order_receipt_media_id(
    pool: &PgPool,
    order_id: i32,
    receipt_media_id: Option<&str>,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        r#"
        UPDATE orders
        SET receipt_media_id = $2
        WHERE id = $1
        "#,
    )
    .bind(order_id)
    .bind(receipt_media_id)
    .execute(pool)
    .await?;

    Ok(())
}

pub async fn update_order_delivery_cost(
    pool: &PgPool,
    order_id: i32,
    delivery_cost: i32,
    total_final: i32,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        r#"
        UPDATE orders
        SET delivery_cost = $2, total_final = $3
        WHERE id = $1
        "#,
    )
    .bind(order_id)
    .bind(delivery_cost)
    .bind(total_final)
    .execute(pool)
    .await?;

    Ok(())
}

pub async fn get_order(pool: &PgPool, order_id: i32) -> Result<Option<Order>, sqlx::Error> {
    sqlx::query_as::<_, Order>(
        r#"
        SELECT id, conversation_id, delivery_type, scheduled_date, scheduled_time,
               scheduled_date_text, scheduled_time_text,
               payment_method, receipt_media_id, referral_code, referral_discount_total,
               ambassador_commission_total, delivery_cost, total_estimated, total_final,
               status, created_at
        FROM orders
        WHERE id = $1
        "#,
    )
    .bind(order_id)
    .fetch_optional(pool)
    .await
}

pub async fn list_orders(pool: &PgPool) -> Result<Vec<Order>, sqlx::Error> {
    sqlx::query_as::<_, Order>(
        r#"
        SELECT id, conversation_id, delivery_type, scheduled_date, scheduled_time,
               scheduled_date_text, scheduled_time_text,
               payment_method, receipt_media_id, referral_code, referral_discount_total,
               ambassador_commission_total, delivery_cost, total_estimated, total_final,
               status, created_at
        FROM orders
        ORDER BY created_at DESC, id DESC
        "#,
    )
    .fetch_all(pool)
    .await
}

pub async fn get_order_items(pool: &PgPool, order_id: i32) -> Result<Vec<OrderItem>, sqlx::Error> {
    sqlx::query_as::<_, OrderItem>(
        r#"
        SELECT id, order_id, flavor, has_liquor, quantity, unit_price, subtotal, created_at
        FROM order_items
        WHERE order_id = $1
        ORDER BY id ASC
        "#,
    )
    .bind(order_id)
    .fetch_all(pool)
    .await
}

pub async fn list_order_items(pool: &PgPool) -> Result<Vec<OrderItem>, sqlx::Error> {
    sqlx::query_as::<_, OrderItem>(
        r#"
        SELECT id, order_id, flavor, has_liquor, quantity, unit_price, subtotal, created_at
        FROM order_items
        ORDER BY created_at DESC, id DESC
        "#,
    )
    .fetch_all(pool)
    .await
}

pub async fn list_active_timer_conversations(
    pool: &PgPool,
    states: &[&str],
) -> Result<Vec<ActiveTimerConversation>, sqlx::Error> {
    sqlx::query_as::<_, ActiveTimerConversation>(
        r#"
        SELECT id, phone_number, state, state_data, customer_name, customer_phone, delivery_address, last_message_at, human_takeover_until
        FROM conversations
        WHERE state = ANY($1)
        "#,
    )
    .bind(states)
    .fetch_all(pool)
    .await
}

pub async fn reset_conversation(pool: &PgPool, phone_number: &str) -> Result<(), sqlx::Error> {
    sqlx::query(
        r#"
        UPDATE conversations
        SET state = 'main_menu', state_data = $2, last_message_at = NOW()
        WHERE phone_number = $1
        "#,
    )
    .bind(phone_number)
    .bind(Json(ConversationStateData::default()))
    .execute(pool)
    .await?;

    Ok(())
}

pub async fn get_customer(pool: &PgPool, phone_number_meta: &str) -> Result<Option<Customer>, sqlx::Error> {
    sqlx::query_as::<_, Customer>(
        r#"
        SELECT phone_number_meta, phone_number_manual, customer_name_meta, customer_name_manual,
               customer_username, delivery_address_last, total_spent_cop, total_units_purchased,
               first_contact_at, last_contact_at, created_at, updated_at, ctwa_clid
        FROM customers
        WHERE phone_number_meta = $1
        "#,
    )
    .bind(phone_number_meta)
    .fetch_optional(pool)
    .await
}

pub async fn create_or_update_customer(
    pool: &PgPool,
    phone_number_meta: &str,
    phone_number_manual: Option<&str>,
    customer_name_meta: Option<&str>,
    customer_name_manual: Option<&str>,
    customer_username: Option<&str>,
    delivery_address_last: Option<&str>,
    ctwa_clid: Option<&str>,
) -> Result<Customer, sqlx::Error> {
    sqlx::query_as::<_, Customer>(
        r#"
        INSERT INTO customers (
            phone_number_meta, phone_number_manual, customer_name_meta, customer_name_manual,
            customer_username, delivery_address_last, ctwa_clid, first_contact_at, last_contact_at
        )
        VALUES ($1, $2, $3, $4, $5, $6, $7, NOW(), NOW())
        ON CONFLICT (phone_number_meta) DO UPDATE SET
            phone_number_manual = COALESCE($2, customers.phone_number_manual),
            customer_name_meta = COALESCE($3, customers.customer_name_meta),
            customer_name_manual = COALESCE($4, customers.customer_name_manual),
            customer_username = COALESCE($5, customers.customer_username),
            delivery_address_last = COALESCE($6, customers.delivery_address_last),
            -- El ctwa_clid se captura una sola vez (primer contacto por
            -- anuncio) y nunca se sobreescribe en mensajes siguientes.
            ctwa_clid = COALESCE(customers.ctwa_clid, $7),
            last_contact_at = NOW(),
            updated_at = NOW()
        RETURNING phone_number_meta, phone_number_manual, customer_name_meta, customer_name_manual,
                  customer_username, delivery_address_last, total_spent_cop, total_units_purchased,
                  first_contact_at, last_contact_at, created_at, updated_at, ctwa_clid
        "#,
    )
    .bind(phone_number_meta)
    .bind(phone_number_manual)
    .bind(customer_name_meta)
    .bind(customer_name_manual)
    .bind(customer_username)
    .bind(delivery_address_last)
    .bind(ctwa_clid)
    .fetch_one(pool)
    .await
}

pub async fn update_customer_totals(
    pool: &PgPool,
    phone_number_meta: &str,
    total_spent_cop: i32,
    total_units_purchased: i32,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        r#"
        UPDATE customers
        SET total_spent_cop = total_spent_cop + $2,
            total_units_purchased = total_units_purchased + $3,
            updated_at = NOW()
        WHERE phone_number_meta = $1
        "#,
    )
    .bind(phone_number_meta)
    .bind(total_spent_cop)
    .bind(total_units_purchased)
    .execute(pool)
    .await?;

    Ok(())
}

/// Tope de direcciones guardadas por cliente (decisión de producto: al
/// intentar guardar una 5ª distinta se reemplaza la más antigua sin
/// preguntar, ver `upsert_customer_address`).
const MAX_CUSTOMER_ADDRESSES: i64 = 4;

pub async fn list_customer_addresses(
    pool: &PgPool,
    customer_phone_meta: &str,
) -> Result<Vec<CustomerAddress>, sqlx::Error> {
    sqlx::query_as::<_, CustomerAddress>(
        r#"
        SELECT id, customer_phone_meta, address_text, address_key, zone_kind, zone_value,
               zone_label, last_delivery_cost_cop, created_at, last_used_at
        FROM customer_addresses
        WHERE customer_phone_meta = $1
        ORDER BY last_used_at DESC
        "#,
    )
    .bind(customer_phone_meta)
    .fetch_all(pool)
    .await
}

/// Guarda (o refresca) una dirección del cliente. Si `address_text` ya
/// coincide con una guardada (mismo `address_key` normalizado) solo se
/// actualizan zona/costo/`last_used_at`; si es nueva y el cliente ya tiene
/// `MAX_CUSTOMER_ADDRESSES`, se descarta primero la de `created_at` más
/// antiguo (decisión de producto, sin preguntarle al cliente).
pub async fn upsert_customer_address(
    pool: &PgPool,
    customer_phone_meta: &str,
    address_text: &str,
    zone_kind: &str,
    zone_value: Option<&str>,
    zone_label: &str,
    delivery_cost_cop: i32,
) -> Result<CustomerAddress, sqlx::Error> {
    let address_key = delivery_zone::normalize(address_text);
    let mut tx = pool.begin().await?;

    let updated = sqlx::query_as::<_, CustomerAddress>(
        r#"
        UPDATE customer_addresses
        SET zone_kind = $3,
            zone_value = $4,
            zone_label = $5,
            last_delivery_cost_cop = $6,
            last_used_at = NOW()
        WHERE customer_phone_meta = $1 AND address_key = $2
        RETURNING id, customer_phone_meta, address_text, address_key, zone_kind, zone_value,
                  zone_label, last_delivery_cost_cop, created_at, last_used_at
        "#,
    )
    .bind(customer_phone_meta)
    .bind(&address_key)
    .bind(zone_kind)
    .bind(zone_value)
    .bind(zone_label)
    .bind(delivery_cost_cop)
    .fetch_optional(&mut *tx)
    .await?;

    if let Some(address) = updated {
        tx.commit().await?;
        return Ok(address);
    }

    let existing_count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM customer_addresses WHERE customer_phone_meta = $1")
            .bind(customer_phone_meta)
            .fetch_one(&mut *tx)
            .await?;

    if existing_count >= MAX_CUSTOMER_ADDRESSES {
        sqlx::query(
            r#"
            DELETE FROM customer_addresses
            WHERE id = (
                SELECT id FROM customer_addresses
                WHERE customer_phone_meta = $1
                ORDER BY created_at ASC
                LIMIT 1
            )
            "#,
        )
        .bind(customer_phone_meta)
        .execute(&mut *tx)
        .await?;
    }

    let inserted = sqlx::query_as::<_, CustomerAddress>(
        r#"
        INSERT INTO customer_addresses (
            customer_phone_meta, address_text, address_key, zone_kind, zone_value,
            zone_label, last_delivery_cost_cop
        )
        VALUES ($1, $2, $3, $4, $5, $6, $7)
        RETURNING id, customer_phone_meta, address_text, address_key, zone_kind, zone_value,
                  zone_label, last_delivery_cost_cop, created_at, last_used_at
        "#,
    )
    .bind(customer_phone_meta)
    .bind(address_text)
    .bind(&address_key)
    .bind(zone_kind)
    .bind(zone_value)
    .bind(zone_label)
    .bind(delivery_cost_cop)
    .fetch_one(&mut *tx)
    .await?;

    tx.commit().await?;
    Ok(inserted)
}

/// Bump de `last_used_at` cuando se reutiliza una dirección guardada tal
/// cual (`select_saved_address`), sin cambiar zona/costo.
pub async fn touch_customer_address(pool: &PgPool, id: i64) -> Result<(), sqlx::Error> {
    sqlx::query("UPDATE customer_addresses SET last_used_at = NOW() WHERE id = $1")
        .bind(id)
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn get_referral_code_analytics(
    pool: &PgPool,
    code: &str,
) -> Result<Option<ReferralCodeAnalytics>, sqlx::Error> {
    sqlx::query_as::<_, ReferralCodeAnalytics>(
        r#"
        SELECT code, times_used, total_discount_generated_cop, total_commission_generated_cop,
               total_units_purchased, total_sales_cop, created_at, updated_at
        FROM referral_code_analytics
        WHERE code = $1
        "#,
    )
    .bind(code)
    .fetch_optional(pool)
    .await
}

pub async fn create_or_update_referral_analytics(
    pool: &PgPool,
    code: &str,
    times_used_inc: i32,
    discount_inc: i32,
    commission_inc: i32,
    units_inc: i32,
    sales_inc: i32,
) -> Result<ReferralCodeAnalytics, sqlx::Error> {
    sqlx::query_as::<_, ReferralCodeAnalytics>(
        r#"
        INSERT INTO referral_code_analytics (
            code, times_used, total_discount_generated_cop, total_commission_generated_cop,
            total_units_purchased, total_sales_cop
        )
        VALUES ($1, $2, $3, $4, $5, $6)
        ON CONFLICT (code) DO UPDATE SET
            times_used = referral_code_analytics.times_used + $2,
            total_discount_generated_cop = referral_code_analytics.total_discount_generated_cop + $3,
            total_commission_generated_cop = referral_code_analytics.total_commission_generated_cop + $4,
            total_units_purchased = referral_code_analytics.total_units_purchased + $5,
            total_sales_cop = referral_code_analytics.total_sales_cop + $6,
            updated_at = NOW()
        RETURNING code, times_used, total_discount_generated_cop, total_commission_generated_cop,
                  total_units_purchased, total_sales_cop, created_at, updated_at
        "#,
    )
    .bind(code)
    .bind(times_used_inc)
    .bind(discount_inc)
    .bind(commission_inc)
    .bind(units_inc)
    .bind(sales_inc)
    .fetch_one(pool)
    .await
}

/// Append one message to the conversation trace (`message_events`). Best-effort
/// audit log for the CRM; callers log and swallow errors so a logging failure
/// never blocks message delivery.
#[allow(clippy::too_many_arguments)]
pub async fn record_message_event(
    pool: &PgPool,
    case_phone: &str,
    channel: &str,
    actor: &str,
    content_type: &str,
    body: Option<&str>,
    payload: Option<serde_json::Value>,
    wa_message_id: Option<&str>,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        r#"
        INSERT INTO message_events
            (case_phone, channel, actor, content_type, body, payload, wa_message_id)
        VALUES ($1, $2, $3, $4, $5, $6, $7)
        "#,
    )
    .bind(case_phone)
    .bind(channel)
    .bind(actor)
    .bind(content_type)
    .bind(body)
    .bind(payload.map(Json))
    .bind(wa_message_id)
    .execute(pool)
    .await?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{
        add_order_item, create_conversation, create_order, get_conversation, list_conversations,
        list_order_items, list_orders, reset_conversation, update_customer_data, update_state,
    };
    use crate::db::models::ConversationStateData;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn unique_suffix() -> String {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time")
            .as_nanos();
        nanos.to_string()
    }

    #[tokio::test]
    #[ignore = "requires TEST_DATABASE_URL and a reachable PostgreSQL instance"]
    async fn creates_and_loads_conversation() {
        let database_url = std::env::var("TEST_DATABASE_URL")
            .expect("TEST_DATABASE_URL must be set for ignored DB tests");
        let pool = sqlx::postgres::PgPoolOptions::new()
            .connect(&database_url)
            .await
            .expect("db connection");
        sqlx::migrate!().run(&pool).await.expect("migrations");

        let conversation = create_conversation(&pool, "573001234567")
            .await
            .expect("create conversation");
        let loaded = get_conversation(&pool, "573001234567")
            .await
            .expect("get conversation")
            .expect("conversation");

        assert_eq!(conversation.phone_number, loaded.phone_number);
    }

    #[tokio::test]
    #[ignore = "requires TEST_DATABASE_URL and a reachable PostgreSQL instance"]
    async fn updates_state_data() {
        let database_url = std::env::var("TEST_DATABASE_URL")
            .expect("TEST_DATABASE_URL must be set for ignored DB tests");
        let pool = sqlx::postgres::PgPoolOptions::new()
            .connect(&database_url)
            .await
            .expect("db connection");
        sqlx::migrate!().run(&pool).await.expect("migrations");
        create_conversation(&pool, "573009999999")
            .await
            .expect("create conversation");

        let state_data = ConversationStateData {
            delivery_type: Some("immediate".into()),
            ..ConversationStateData::default()
        };
        update_state(&pool, "573009999999", "collect_name", &state_data)
            .await
            .expect("update state");
        let loaded = get_conversation(&pool, "573009999999")
            .await
            .expect("get conversation")
            .expect("conversation");

        assert_eq!(loaded.state, "collect_name");
        assert_eq!(
            loaded.state_data.0.delivery_type.as_deref(),
            Some("immediate")
        );
    }

    #[tokio::test]
    #[ignore = "requires TEST_DATABASE_URL and a reachable PostgreSQL instance"]
    async fn reset_conversation_preserves_customer_fields() {
        let database_url = std::env::var("TEST_DATABASE_URL")
            .expect("TEST_DATABASE_URL must be set for ignored DB tests");
        let pool = sqlx::postgres::PgPoolOptions::new()
            .connect(&database_url)
            .await
            .expect("db connection");
        sqlx::migrate!().run(&pool).await.expect("migrations");
        create_conversation(&pool, "573008888888")
            .await
            .expect("create conversation");
        update_customer_data(
            &pool,
            "573008888888",
            Some("Cliente Persistente"),
            Some("573008888888"),
            Some("Calle 123"),
        )
        .await
        .expect("update customer data");

        let state_data = ConversationStateData {
            delivery_type: Some("immediate".into()),
            ..ConversationStateData::default()
        };
        update_state(&pool, "573008888888", "collect_name", &state_data)
            .await
            .expect("update state");
        reset_conversation(&pool, "573008888888")
            .await
            .expect("reset conversation");

        let loaded = get_conversation(&pool, "573008888888")
            .await
            .expect("get conversation")
            .expect("conversation");

        assert_eq!(loaded.state, "main_menu");
        assert_eq!(loaded.customer_name.as_deref(), Some("Cliente Persistente"));
        assert_eq!(loaded.customer_phone.as_deref(), Some("573008888888"));
        assert_eq!(loaded.delivery_address.as_deref(), Some("Calle 123"));
    }

    #[tokio::test]
    #[ignore = "requires TEST_DATABASE_URL and a reachable PostgreSQL instance"]
    async fn list_queries_return_newest_rows_first() {
        let database_url = std::env::var("TEST_DATABASE_URL")
            .expect("TEST_DATABASE_URL must be set for ignored DB tests");
        let pool = sqlx::postgres::PgPoolOptions::new()
            .connect(&database_url)
            .await
            .expect("db connection");
        sqlx::migrate!().run(&pool).await.expect("migrations");

        let suffix = unique_suffix();
        let phone_a = format!("5731{}", &suffix[..9.min(suffix.len())]);
        let phone_b = format!("5732{}", &suffix[..9.min(suffix.len())]);

        let conversation_a = create_conversation(&pool, &phone_a)
            .await
            .expect("create first conversation");
        let conversation_b = create_conversation(&pool, &phone_b)
            .await
            .expect("create second conversation");

        let conversations = list_conversations(&pool).await.expect("list conversations");
        let position_a = conversations
            .iter()
            .position(|conversation| conversation.id == conversation_a.id)
            .expect("first conversation present");
        let position_b = conversations
            .iter()
            .position(|conversation| conversation.id == conversation_b.id)
            .expect("second conversation present");
        assert!(position_b < position_a);

        let order_a = create_order(
            &pool,
            conversation_a.id,
            "immediate",
            None,
            None,
            None,
            None,
            "cash",
            None,
            None,
            None,
            None,
            12000,
        )
        .await
        .expect("create first order");
        let order_b = create_order(
            &pool,
            conversation_b.id,
            "scheduled",
            None,
            None,
            Some("mañana"),
            Some("7 pm"),
            "transfer",
            None,
            None,
            None,
            None,
            18000,
        )
        .await
        .expect("create second order");

        let orders = list_orders(&pool).await.expect("list orders");
        let order_position_a = orders
            .iter()
            .position(|order| order.id == order_a.id)
            .expect("first order present");
        let order_position_b = orders
            .iter()
            .position(|order| order.id == order_b.id)
            .expect("second order present");
        assert!(order_position_b < order_position_a);

        let item_a = add_order_item(&pool, order_a.id, "fresa", false, 1, 6000, 6000)
            .await
            .expect("create first order item");
        let item_b = add_order_item(&pool, order_b.id, "uva", true, 2, 9000, 18000)
            .await
            .expect("create second order item");

        let order_items = list_order_items(&pool).await.expect("list order items");
        let item_position_a = order_items
            .iter()
            .position(|item| item.id == item_a.id)
            .expect("first item present");
        let item_position_b = order_items
            .iter()
            .position(|item| item.id == item_b.id)
            .expect("second item present");
        assert!(item_position_b < item_position_a);
    }
}
