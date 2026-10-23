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
    db::queries::{create_conversation, get_conversation},
    engine::{process_customer_input, return_case_to_bot},
    messages::{set_client_messages, ClientMessages},
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
        crm_app_pricing_url: None,
        crm_app_pricing_token: None,
        crm_app_flavors_url: None,
        carta_url: None,
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

/// Devolver un caso al bot no corre el modelo ni le escribe al cliente: deja el
/// carrito que tenía como pedido en Pendientes y arranca de cero. Regresión del
/// pedido 47 (2026-10-20): el turno de recuperación que existía antes le
/// confirmó a las 8:00 a un cliente un pedido que el asesor ya había entregado.
#[tokio::test]
#[ignore = "requires TEST_DATABASE_URL and a reachable PostgreSQL instance"]
async fn returning_a_case_starts_from_scratch_and_parks_the_cart_in_pendientes() {
    let state = setup_state().await;
    let phone = unique_phone();

    create_conversation(&state.pool, &phone)
        .await
        .expect("create conversation");

    sqlx::query(
        r#"
        UPDATE conversations
        SET state_data = state_data || jsonb_build_object(
              'items', jsonb_build_array(jsonb_build_object(
                'flavor', 'Uva Vodka', 'has_liquor', true, 'quantity', 2)),
              'delivery_type', 'immediate',
              'handoff_reason', 'unreadable_media'),
            human_takeover_until = NOW() - INTERVAL '1 minute'
        WHERE phone_number = $1
        "#,
    )
    .bind(&phone)
    .execute(&state.pool)
    .await
    .expect("seed a cart in handoff");

    let client_messages_before: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM message_events WHERE case_phone = $1 AND channel = 'client'",
    )
    .bind(&phone)
    .fetch_one(&state.pool)
    .await
    .expect("count client messages");

    return_case_to_bot(&state, &phone, "test")
        .await
        .expect("returning the case should not fail");

    let conversation = get_conversation(&state.pool, &phone)
        .await
        .expect("query conversation")
        .expect("conversation exists");
    assert_eq!(conversation.state, "main_menu");
    assert!(conversation.state_data.0.items.is_empty());
    assert!(conversation.state_data.0.handoff_reason.is_none());
    assert!(conversation.human_takeover_until.is_none());

    let parked: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM orders WHERE conversation_id = $1 AND status = 'pending_advisor'",
    )
    .bind(conversation.id)
    .fetch_one(&state.pool)
    .await
    .expect("count parked orders");
    assert_eq!(parked, 1, "the cart must land in Pendientes");

    let client_messages_after: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM message_events WHERE case_phone = $1 AND channel = 'client'",
    )
    .bind(&phone)
    .fetch_one(&state.pool)
    .await
    .expect("count client messages");
    assert_eq!(client_messages_after, client_messages_before, "nothing goes to the customer");
}
