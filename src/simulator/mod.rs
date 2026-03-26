use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlx::{types::Json, FromRow, PgPool};

use crate::{
    bot::state_machine::ConversationContext,
    db::{
        models::{Conversation, ConversationStateData},
        queries::{create_conversation, get_conversation},
    },
};

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct SimulatorSession {
    pub id: i32,
    pub customer_phone: String,
    pub profile_name: Option<String>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct SimulatorMessage {
    pub id: i64,
    pub session_id: Option<i32>,
    pub actor: String,
    pub audience: String,
    pub message_kind: String,
    pub body: Option<String>,
    pub payload: Json<Value>,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct SimulatorMedia {
    pub id: String,
    pub session_id: Option<i32>,
    pub kind: String,
    pub file_path: String,
    pub mime_type: Option<String>,
    pub original_filename: Option<String>,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Clone, FromRow)]
pub struct SimulatorSessionSummaryRow {
    pub id: i32,
    pub customer_phone: String,
    pub profile_name: Option<String>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
    pub state: Option<String>,
    pub state_data: Option<Json<ConversationStateData>>,
    pub customer_name: Option<String>,
    pub customer_phone_stored: Option<String>,
    pub delivery_address: Option<String>,
    pub last_message_at: Option<chrono::DateTime<chrono::Utc>>,
}

#[derive(Debug, Clone, Serialize)]
pub struct SimulatorSessionSummary {
    pub id: i32,
    pub customer_phone: String,
    pub profile_name: Option<String>,
    pub conversation_exists: bool,
    pub state: String,
    pub customer_name: Option<String>,
    pub customer_phone_stored: Option<String>,
    pub delivery_address: Option<String>,
    pub current_order_id: Option<i32>,
    pub last_message_at: Option<chrono::DateTime<chrono::Utc>>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

impl From<SimulatorSessionSummaryRow> for SimulatorSessionSummary {
    fn from(row: SimulatorSessionSummaryRow) -> Self {
        let current_order_id = row
            .state_data
            .as_ref()
            .and_then(|state_data| state_data.0.current_order_id);

        Self {
            id: row.id,
            customer_phone: row.customer_phone,
            profile_name: row.profile_name,
            conversation_exists: row.state.is_some(),
            state: row.state.unwrap_or_else(|| "main_menu".to_string()),
            customer_name: row.customer_name,
            customer_phone_stored: row.customer_phone_stored,
            delivery_address: row.delivery_address,
            current_order_id,
            last_message_at: row.last_message_at,
            created_at: row.created_at,
            updated_at: row.updated_at,
        }
    }
}

#[derive(Debug, Clone)]
pub struct SimulatorStateSnapshot {
    pub session: SimulatorSession,
    pub conversation: Option<Conversation>,
    pub context: Option<ConversationContext>,
}

#[derive(Debug, Clone)]
pub struct NewSimulatorMessage {
    pub session_id: Option<i32>,
    pub actor: String,
    pub audience: String,
    pub message_kind: String,
    pub body: Option<String>,
    pub payload: Value,
}

#[derive(Debug, Clone)]
pub struct NewSimulatorMedia {
    pub id: String,
    pub session_id: Option<i32>,
    pub kind: String,
    pub file_path: String,
    pub mime_type: Option<String>,
    pub original_filename: Option<String>,
}

pub async fn create_or_update_session(
    pool: &PgPool,
    customer_phone: &str,
    profile_name: Option<&str>,
) -> Result<SimulatorSession, sqlx::Error> {
    sqlx::query_as::<_, SimulatorSession>(
        r#"
        INSERT INTO simulator_sessions (customer_phone, profile_name)
        VALUES ($1, $2)
        ON CONFLICT (customer_phone)
        DO UPDATE
        SET profile_name = COALESCE(EXCLUDED.profile_name, simulator_sessions.profile_name),
            updated_at = NOW()
        RETURNING id, customer_phone, profile_name, created_at, updated_at
        "#,
    )
    .bind(customer_phone)
    .bind(profile_name)
    .fetch_one(pool)
    .await
}

pub async fn get_session(
    pool: &PgPool,
    session_id: i32,
) -> Result<Option<SimulatorSession>, sqlx::Error> {
    sqlx::query_as::<_, SimulatorSession>(
        r#"
        SELECT id, customer_phone, profile_name, created_at, updated_at
        FROM simulator_sessions
        WHERE id = $1
        "#,
    )
    .bind(session_id)
    .fetch_optional(pool)
    .await
}

pub async fn get_session_by_phone(
    pool: &PgPool,
    customer_phone: &str,
) -> Result<Option<SimulatorSession>, sqlx::Error> {
    sqlx::query_as::<_, SimulatorSession>(
        r#"
        SELECT id, customer_phone, profile_name, created_at, updated_at
        FROM simulator_sessions
        WHERE customer_phone = $1
        "#,
    )
    .bind(customer_phone)
    .fetch_optional(pool)
    .await
}

pub async fn list_sessions(pool: &PgPool) -> Result<Vec<SimulatorSessionSummary>, sqlx::Error> {
    let rows = sqlx::query_as::<_, SimulatorSessionSummaryRow>(
        r#"
        SELECT
            ss.id,
            ss.customer_phone,
            ss.profile_name,
            ss.created_at,
            ss.updated_at,
            c.state,
            c.state_data,
            c.customer_name,
            c.customer_phone AS customer_phone_stored,
            c.delivery_address,
            c.last_message_at
        FROM simulator_sessions ss
        LEFT JOIN conversations c ON c.phone_number = ss.customer_phone
        ORDER BY ss.updated_at DESC, ss.id DESC
        "#,
    )
    .fetch_all(pool)
    .await?;

    Ok(rows.into_iter().map(Into::into).collect())
}

pub async fn ensure_session_conversation(
    pool: &PgPool,
    customer_phone: &str,
) -> Result<Conversation, sqlx::Error> {
    match get_conversation(pool, customer_phone).await? {
        Some(conversation) => Ok(conversation),
        None => create_conversation(pool, customer_phone).await,
    }
}

pub async fn create_message(
    pool: &PgPool,
    message: NewSimulatorMessage,
) -> Result<SimulatorMessage, sqlx::Error> {
    let inserted = sqlx::query_as::<_, SimulatorMessage>(
        r#"
        INSERT INTO simulator_messages (session_id, actor, audience, message_kind, body, payload)
        VALUES ($1, $2, $3, $4, $5, $6)
        RETURNING id, session_id, actor, audience, message_kind, body, payload, created_at
        "#,
    )
    .bind(message.session_id)
    .bind(message.actor)
    .bind(message.audience)
    .bind(message.message_kind)
    .bind(message.body)
    .bind(Json(message.payload))
    .fetch_one(pool)
    .await?;

    if let Some(session_id) = inserted.session_id {
        sqlx::query(
            r#"
            UPDATE simulator_sessions
            SET updated_at = NOW()
            WHERE id = $1
            "#,
        )
        .bind(session_id)
        .execute(pool)
        .await?;
    }

    Ok(inserted)
}

pub async fn list_messages_for_session(
    pool: &PgPool,
    session_id: i32,
) -> Result<Vec<SimulatorMessage>, sqlx::Error> {
    sqlx::query_as::<_, SimulatorMessage>(
        r#"
        SELECT id, session_id, actor, audience, message_kind, body, payload, created_at
        FROM simulator_messages
        WHERE session_id = $1
        ORDER BY id ASC
        "#,
    )
    .bind(session_id)
    .fetch_all(pool)
    .await
}

pub async fn list_advisor_inbox(pool: &PgPool) -> Result<Vec<SimulatorMessage>, sqlx::Error> {
    sqlx::query_as::<_, SimulatorMessage>(
        r#"
        SELECT id, session_id, actor, audience, message_kind, body, payload, created_at
        FROM simulator_messages
        WHERE audience = 'advisor'
        ORDER BY id ASC
        "#,
    )
    .fetch_all(pool)
    .await
}

pub async fn create_media(
    pool: &PgPool,
    media: NewSimulatorMedia,
) -> Result<SimulatorMedia, sqlx::Error> {
    sqlx::query_as::<_, SimulatorMedia>(
        r#"
        INSERT INTO simulator_media (id, session_id, kind, file_path, mime_type, original_filename)
        VALUES ($1, $2, $3, $4, $5, $6)
        RETURNING id, session_id, kind, file_path, mime_type, original_filename, created_at
        "#,
    )
    .bind(media.id)
    .bind(media.session_id)
    .bind(media.kind)
    .bind(media.file_path)
    .bind(media.mime_type)
    .bind(media.original_filename)
    .fetch_one(pool)
    .await
}

pub async fn get_media(
    pool: &PgPool,
    media_id: &str,
) -> Result<Option<SimulatorMedia>, sqlx::Error> {
    sqlx::query_as::<_, SimulatorMedia>(
        r#"
        SELECT id, session_id, kind, file_path, mime_type, original_filename, created_at
        FROM simulator_media
        WHERE id = $1
        "#,
    )
    .bind(media_id)
    .fetch_optional(pool)
    .await
}

pub async fn snapshot_state(
    pool: &PgPool,
    advisor_phone: &str,
    session_id: i32,
) -> Result<Option<SimulatorStateSnapshot>, sqlx::Error> {
    let Some(session) = get_session(pool, session_id).await? else {
        return Ok(None);
    };
    let conversation = get_conversation(pool, &session.customer_phone).await?;
    let context = conversation.as_ref().map(|conversation| {
        ConversationContext::from_persisted(
            conversation.phone_number.clone(),
            advisor_phone.to_string(),
            conversation.customer_name.clone(),
            conversation.customer_phone.clone(),
            conversation.delivery_address.clone(),
            &conversation.state_data.0,
        )
    });

    Ok(Some(SimulatorStateSnapshot {
        session,
        conversation,
        context,
    }))
}

#[cfg(test)]
mod tests {
    use super::{
        create_message, create_or_update_session, get_session_by_phone, list_messages_for_session,
        NewSimulatorMessage,
    };

    #[tokio::test]
    #[ignore = "requires TEST_DATABASE_URL and a reachable PostgreSQL instance"]
    async fn stores_local_session_and_transcript() {
        let database_url = std::env::var("TEST_DATABASE_URL")
            .expect("TEST_DATABASE_URL must be set for ignored DB tests");
        let pool = sqlx::postgres::PgPoolOptions::new()
            .connect(&database_url)
            .await
            .expect("db connection");
        sqlx::migrate!().run(&pool).await.expect("migrations");

        let session = create_or_update_session(&pool, "573007777777", Some("Simulado"))
            .await
            .expect("create session");
        create_message(
            &pool,
            NewSimulatorMessage {
                session_id: Some(session.id),
                actor: "customer".to_string(),
                audience: "customer".to_string(),
                message_kind: "text".to_string(),
                body: Some("Hola local".to_string()),
                payload: serde_json::json!({}),
            },
        )
        .await
        .expect("create message");

        let loaded_session = get_session_by_phone(&pool, "573007777777")
            .await
            .expect("get session")
            .expect("session");
        let messages = list_messages_for_session(&pool, loaded_session.id)
            .await
            .expect("list messages");

        assert_eq!(loaded_session.profile_name.as_deref(), Some("Simulado"));
        assert!(messages
            .iter()
            .any(|message| message.body.as_deref() == Some("Hola local")));
    }
}
