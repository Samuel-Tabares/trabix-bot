//! Camino de error del motor de agente (FASE 2 del rollout a producción):
//! si el turno del agente falla, el cliente recibe el mensaje fijo de
//! `[agent].llm_failure_customer`, el asesor recibe el contexto del caso y
//! el estado de la conversación NO cambia. Aquí el fallo se inyecta con una
//! `anthropic_api_key` inválida de prueba, que hace fallar la llamada real a
//! la API de Anthropic dentro de `run_customer_turn`.
//!
//! Tras eliminar el simulador ya no hay una capa que capture los mensajes
//! salientes sin llamar a Meta, así que este test verifica la invariante
//! observable en la base de datos: la degradación no propaga el error y el
//! estado del caso queda intacto. Los envíos al cliente/asesor se intentan
//! contra Meta con credenciales de prueba y fallan de forma silenciosa (el
//! camino de degradación traga esos errores a propósito).

use std::net::IpAddr;
use std::time::{SystemTime, UNIX_EPOCH};

use granizado_bot::{
    bot::state_machine::UserInput,
    bot::timers::new_timer_map,
    config::Config,
    db::queries::get_conversation,
    engine::{process_advisor_turn_for_case, process_customer_input},
    messages::{client_messages, set_client_messages, ClientMessages},
    whatsapp::client::WhatsAppClient,
    AppState,
};
use sqlx::PgPool;

const ADVISOR_PHONE: &str = "573009999999";

async fn setup_state() -> AppState {
    let database_url = std::env::var("TEST_DATABASE_URL")
        .expect("TEST_DATABASE_URL must be set for ignored DB tests");
    let pool = PgPool::connect(&database_url).await.expect("db connection");
    sqlx::migrate!("./migrations")
        .run(&pool)
        .await
        .expect("migrations");

    let _ = set_client_messages(
        ClientMessages::from_path("config/messages.toml").expect("messages load"),
    );

    let config = Config {
        database_url,
        advisor_phone: ADVISOR_PHONE.to_string(),
        transfer_payment_text: None,
        port: 8080,
        internal_port: 8081,
        bind_ip: "127.0.0.1".parse::<IpAddr>().expect("ip"),
        whatsapp_token: "test-token".to_string(),
        whatsapp_phone_id: "test-phone-id".to_string(),
        whatsapp_verify_token: "test-verify".to_string(),
        whatsapp_app_secret: "test-secret".to_string(),
        menu_image_media_id: "test-media-id".to_string(),
        anthropic_api_key: "test-anthropic-key".to_string(),
        agent_daily_llm_call_limit: None,
        waba_id: None,
        capi_dataset_id: None,
        capi_access_token: None,
        internal_api_token: None,
        advisor_takeover_hours: 6,
    };

    AppState {
        transport: WhatsAppClient::new(
            config.whatsapp_token.clone(),
            config.whatsapp_phone_id.clone(),
        ),
        capi: granizado_bot::capi::CapiClient::new(None, None, None),
        config,
        pool,
        timers: new_timer_map(),
        conversation_locks: granizado_bot::new_conversation_locks(),
        llm_budget: granizado_bot::ai::budget::new_llm_budget_handle(None),
        webhook_dedup: granizado_bot::new_webhook_dedup_cache(),
    }
}

fn unique_phone() -> String {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time")
        .subsec_nanos() as u64;
    format!("573{:010}", nanos % 10_000_000_000)
}

#[tokio::test]
#[ignore = "requires TEST_DATABASE_URL and a reachable PostgreSQL instance"]
async fn agent_failure_does_not_propagate_or_change_state() {
    let state = setup_state().await;
    let phone = unique_phone();

    process_customer_input(
        state.clone(),
        phone.clone(),
        Some("Cliente Prueba".to_string()),
        None,
        None,
        UserInput::TextMessage("Hola, quiero un granizado".to_string()),
    )
    .await
    .expect("degradation path should not propagate the agent error");

    let conversation = get_conversation(&state.pool, &phone)
        .await
        .expect("get conversation")
        .expect("conversation exists");
    assert_eq!(
        conversation.state, "main_menu",
        "conversation state must remain unchanged after a degraded turn"
    );
}

/// Regresión: `degrade_agent_failure` solía notificar al cliente SOLO cuando
/// el turno que falló era del cliente. Si el turno que fallaba era el del
/// asesor (`replyAsAdvisor` desde `crm-app`, p. ej. destrabando un caso
/// `needs_human`), el cliente se quedaba sin ningún mensaje nuevo — el caso
/// de Santiago (573136356011) en producción el 2026-08-03. Este test cubre
/// justo ese camino: un turno de asesor que falla también debe dejar el
/// mensaje fijo de `llm_failure_customer` en la fila del cliente.
#[tokio::test]
#[ignore = "requires TEST_DATABASE_URL and a reachable PostgreSQL instance"]
async fn advisor_turn_failure_still_notifies_the_customer() {
    let state = setup_state().await;
    let phone = unique_phone();

    // Primer mensaje: solo dispara el saludo fijo (sin LLM), deja
    // `has_greeted = true` para que el segundo mensaje sí entre al agente.
    process_customer_input(
        state.clone(),
        phone.clone(),
        Some("Cliente Prueba".to_string()),
        None,
        None,
        UserInput::TextMessage("Hola".to_string()),
    )
    .await
    .expect("welcome turn should not fail");

    // Segundo mensaje del cliente: ahora sí llama al LLM (credenciales de
    // prueba inválidas), falla y degrada — turn_actor == "customer".
    process_customer_input(
        state.clone(),
        phone.clone(),
        Some("Cliente Prueba".to_string()),
        None,
        None,
        UserInput::TextMessage("Quiero un granizado".to_string()),
    )
    .await
    .expect("degradation path should not propagate the agent error");

    // El asesor intenta destrabar el caso desde crm-app (`replyAsAdvisor`):
    // también pasa por el motor de agente, también falla con las mismas
    // credenciales de prueba, y turn_actor == "advisor" esta vez.
    process_advisor_turn_for_case(
        &state,
        &phone,
        UserInput::TextMessage("tranqui, ya te ayudo".to_string()),
    )
    .await
    .expect("advisor degradation path should not propagate the agent error");

    let fallback_body = client_messages().agent.llm_failure_customer.clone();
    let client_fallback_count: i64 = sqlx::query_scalar(
        r#"
        SELECT COUNT(*)
        FROM message_events
        WHERE case_phone = $1
          AND channel = 'client'
          AND actor = 'bot'
          AND body = $2
        "#,
    )
    .bind(&phone)
    .bind(&fallback_body)
    .fetch_one(&state.pool)
    .await
    .expect("query message_events");

    assert_eq!(
        client_fallback_count, 2,
        "the customer must get the generic fallback for BOTH the customer-turn \
         failure and the advisor-turn failure — not just the first one"
    );
}
