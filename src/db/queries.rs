#![allow(dead_code)]

use chrono::{NaiveDate, NaiveTime};
use sqlx::{types::Json, PgPool};

use super::models::{Conversation, ConversationStateData, Order, OrderItem};

pub async fn get_conversation(
    pool: &PgPool,
    phone_number: &str,
) -> Result<Option<Conversation>, sqlx::Error> {
    sqlx::query_as::<_, Conversation>(
        r#"
        SELECT id, phone_number, state, state_data, customer_name, customer_phone, delivery_address, last_message_at, created_at
        FROM conversations
        WHERE phone_number = $1
        "#,
    )
    .bind(phone_number)
    .fetch_optional(pool)
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
        RETURNING id, phone_number, state, state_data, customer_name, customer_phone, delivery_address, last_message_at, created_at
        "#,
    )
    .bind(phone_number)
    .bind(Json(ConversationStateData::default()))
    .fetch_one(pool)
    .await
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
    payment_method: &str,
    receipt_media_id: Option<&str>,
    total_estimated: i32,
) -> Result<Order, sqlx::Error> {
    sqlx::query_as::<_, Order>(
        r#"
        INSERT INTO orders (
            conversation_id, delivery_type, scheduled_date, scheduled_time,
            payment_method, receipt_media_id, total_estimated
        )
        VALUES ($1, $2, $3, $4, $5, $6, $7)
        RETURNING id, conversation_id, delivery_type, scheduled_date, scheduled_time,
                  payment_method, receipt_media_id, delivery_cost, total_estimated,
                  total_final, status, created_at
        "#,
    )
    .bind(conversation_id)
    .bind(delivery_type)
    .bind(scheduled_date)
    .bind(scheduled_time)
    .bind(payment_method)
    .bind(receipt_media_id)
    .bind(total_estimated)
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

#[cfg(test)]
mod tests {
    use super::{create_conversation, get_conversation, update_state};
    use crate::db::models::ConversationStateData;

    #[tokio::test]
    #[ignore = "requires TEST_DATABASE_URL and a reachable PostgreSQL instance"]
    async fn creates_and_loads_conversation() {
        let database_url =
            std::env::var("TEST_DATABASE_URL").expect("TEST_DATABASE_URL must be set for ignored DB tests");
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
        let database_url =
            std::env::var("TEST_DATABASE_URL").expect("TEST_DATABASE_URL must be set for ignored DB tests");
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
        assert_eq!(loaded.state_data.0.delivery_type.as_deref(), Some("immediate"));
    }
}
