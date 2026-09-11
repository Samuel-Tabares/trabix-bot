//! Memoria conversacional del agente: cada invocacion del bot es stateless
//! (llega un webhook, se procesa, se responde), asi que el historial de
//! turnos con Anthropic se persiste por telefono en `agent_case_messages` y
//! se reconstruye en cada turno. `orders`/`order_items` siguen siendo la
//! fuente de verdad transaccional; esto es solo memoria de conversacion.

use sqlx::{types::Json, PgPool};

use crate::ai::client::{ContentBlock, Message};

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

/// Apenda una linea de transcript a la memoria del agente SIN llamar al LLM.
///
/// Existe por el handoff humano: mientras `human_takeover_until` esta en el
/// futuro el bot no corre turnos (`engine::process_customer_input` sale
/// temprano), asi que ni lo que dice el cliente ni lo que le escribe el asesor
/// entraban a `agent_case_messages`. Al devolverle la conversacion, el bot se
/// encontraba un hueco justo donde se resolvio lo importante -- el valor del
/// envio, la verificacion del pago.
///
/// Se guarda como un mensaje `user`, que es el unico rol que el turno de
/// recuperacion puede intercalar sin romper los pares `tool_use`/`tool_result`
/// del historial (ver `agent::llm_window_start`). El marcador de quien hablo lo
/// escribe el sistema, nunca el cliente: es lo que sostiene la regla
/// anti-suplantacion del prompt.
pub async fn append_transcript_entry(
    pool: &PgPool,
    phone_number: &str,
    text: &str,
) -> Result<(), sqlx::Error> {
    let mut messages = load_messages(pool, phone_number).await?;
    messages.push(Message {
        role: "user".to_string(),
        content: vec![ContentBlock::Text {
            text: text.to_string(),
        }],
    });
    save_messages(pool, phone_number, &messages).await
}

/// Marcador de un mensaje que el ASESOR le escribio al cliente durante un
/// handoff. Lo pone el sistema en `routes::internal::advisor_send`.
pub fn advisor_transcript_line(body: &str) -> String {
    format!("[Durante el handoff, el ASESOR le escribio al cliente]: {body}")
}

/// Marcador de un mensaje que el CLIENTE mando mientras el bot estaba pausado.
pub fn client_during_handoff_line(body: &str) -> String {
    format!("[Durante el handoff, el CLIENTE escribio]: {body}")
}
