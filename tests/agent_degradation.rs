//! Camino de error del motor de agente (FASE 2 del rollout a producción):
//! si el turno del agente falla, el cliente recibe el mensaje fijo de
//! `[agent].llm_failure_customer`, el asesor recibe el contexto del caso y
//! el estado de la conversación NO cambia. Aquí el fallo se inyecta dejando
//! `anthropic_api_key = None` con `BOT_ENGINE=agent`, que hace fallar
//! `run_customer_turn` antes de tocar la red.

use std::net::IpAddr;
use std::time::{SystemTime, UNIX_EPOCH};

use granizado_bot::{
    bot::state_machine::UserInput,
    bot::timers::{new_timer_map, new_timer_overrides},
    config::{BotEngine, BotMode, Config, SimulatorConfig},
    db::queries::get_conversation,
    engine::process_customer_input,
    messages::{set_client_messages, ClientMessages},
    transport::OutboundTransport,
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
        mode: BotMode::Simulator,
        database_url,
        advisor_phone: ADVISOR_PHONE.to_string(),
        transfer_payment_text: None,
        port: 8080,
        bind_ip: "127.0.0.1".parse::<IpAddr>().expect("ip"),
        production: None,
        simulator: Some(SimulatorConfig {
            upload_dir: std::env::temp_dir(),
        }),
        bot_engine: BotEngine::Agent,
        anthropic_api_key: None,
        agent_daily_llm_call_limit: None,
    };

    AppState {
        config,
        pool,
        transport: OutboundTransport::Simulator,
        timers: new_timer_map(),
        timer_overrides: new_timer_overrides(),
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

async fn bot_messages_to(pool: &PgPool, since_id: i64) -> Vec<(String, Option<String>)> {
    sqlx::query_as::<_, (String, Option<String>)>(
        "SELECT audience, body FROM simulator_messages WHERE actor = 'bot' AND id > $1 ORDER BY id",
    )
    .bind(since_id)
    .fetch_all(pool)
    .await
    .expect("query simulator messages")
}

async fn max_message_id(pool: &PgPool) -> i64 {
    sqlx::query_scalar::<_, Option<i64>>("SELECT MAX(id)::BIGINT FROM simulator_messages")
        .fetch_one(pool)
        .await
        .expect("max id")
        .unwrap_or(0)
}

#[tokio::test]
#[ignore = "requires TEST_DATABASE_URL and a reachable PostgreSQL instance"]
async fn agent_failure_sends_fixed_message_and_advisor_context_without_state_change() {
    let state = setup_state().await;
    let phone = unique_phone();
    let baseline_id = max_message_id(&state.pool).await;

    process_customer_input(
        state.clone(),
        phone.clone(),
        Some("Cliente Prueba".to_string()),
        None,
        UserInput::TextMessage("Hola, quiero un granizado".to_string()),
    )
    .await
    .expect("degradation path should not propagate the agent error");

    let messages = bot_messages_to(&state.pool, baseline_id).await;
    let expected_customer_body = granizado_bot::messages::client_messages()
        .agent
        .llm_failure_customer
        .clone();

    assert!(
        messages.iter().any(|(audience, body)| audience == "customer"
            && body.as_deref() == Some(expected_customer_body.as_str())),
        "customer must receive the fixed LLM-failure message, got: {messages:?}"
    );
    assert!(
        messages.iter().any(|(audience, body)| audience == "advisor"
            && body
                .as_deref()
                .is_some_and(|text| text.contains(&phone) && text.contains("Error técnico"))),
        "advisor must receive case context, got: {messages:?}"
    );

    let conversation = get_conversation(&state.pool, &phone)
        .await
        .expect("get conversation")
        .expect("conversation exists");
    assert_eq!(
        conversation.state, "main_menu",
        "conversation state must remain unchanged after a degraded turn"
    );
}
