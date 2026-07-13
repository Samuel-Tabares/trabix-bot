//! Memoria conversacional del agente: cada invocacion del bot es stateless
//! (llega un webhook, se procesa, se responde), asi que el historial de
//! turnos con Anthropic se persiste por telefono en `agent_case_messages` y
//! se reconstruye en cada turno. `orders`/`order_items` siguen siendo la
//! fuente de verdad transaccional; esto es solo memoria de conversacion.

use sqlx::{types::Json, PgPool};

use crate::ai::client::Message;

pub async fn load_messages(pool: &PgPool, phone_number: &str) -> Result<Vec<Message>, sqlx::Error> {
    let row: Option<(Json<Vec<Message>>,)> = sqlx::query_as(
        "SELECT messages FROM agent_case_messages WHERE phone_number = $1",
    )
    .bind(phone_number)
    .fetch_optional(pool)
    .await?;

    Ok(row.map(|(messages,)| messages.0).unwrap_or_default())
}

pub async fn save_messages(
    pool: &PgPool,
    phone_number: &str,
    messages: &[Message],
) -> Result<(), sqlx::Error> {
    sqlx::query(
        r#"
        INSERT INTO agent_case_messages (phone_number, messages, updated_at)
        VALUES ($1, $2, NOW())
        ON CONFLICT (phone_number)
        DO UPDATE SET messages = EXCLUDED.messages, updated_at = NOW()
        "#,
    )
    .bind(phone_number)
    .bind(Json(messages))
    .execute(pool)
    .await?;

    Ok(())
}

/// Se llama al finalizar un checkout (handoff al asesor) para que la
/// siguiente conversacion del cliente empiece con memoria limpia en vez de
/// arrastrar el transcript del pedido ya cerrado.
pub async fn clear_messages(pool: &PgPool, phone_number: &str) -> Result<(), sqlx::Error> {
    sqlx::query("DELETE FROM agent_case_messages WHERE phone_number = $1")
        .bind(phone_number)
        .execute(pool)
        .await?;

    Ok(())
}
