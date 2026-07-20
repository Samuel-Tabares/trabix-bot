//! Camino de error del motor de agente (FASE 2 del rollout a producción):
//! si el turno del agente falla, el cliente recibe el mensaje fijo de
//! `[agent].llm_failure_customer`, el asesor recibe el contexto del caso y
//! el estado de la conversación NO cambia. Aquí el fallo se inyecta dejando
//! `anthropic_api_key = None` con `BOT_ENGINE=agent`, que hace fallar
//! `run_customer_turn` antes de tocar la red.
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
    config::{BotEngine, Config},
    db::queries::get_conversation,
    engine::process_customer_input,
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
        bind_ip: "127.0.0.1".parse::<IpAddr>().expect("ip"),
        whatsapp_token: "test-token".to_string(),
        whatsapp_phone_id: "test-phone-id".to_string(),
        whatsapp_verify_token: "test-verify".to_string(),
        whatsapp_app_secret: "test-secret".to_string(),
        menu_image_media_id: "test-media-id".to_string(),
        bot_engine: BotEngine::Agent,
        anthropic_api_key: None,
        agent_daily_llm_call_limit: None,
    };

    AppState {
        transport: WhatsAppClient::new(
            config.whatsapp_token.clone(),
            config.whatsapp_phone_id.clone(),
        ),
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
