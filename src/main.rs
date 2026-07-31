use std::net::SocketAddr;

use axum::Router;
use granizado_bot::{
    bot::timers::{new_timer_map, restore_pending_timers, spawn_timer_sweeper},
    config::Config,
    db::init_pool,
    messages::{set_client_messages, ClientMessages},
    referrals::{set_referral_registry, ReferralRegistry},
    routes,
    whatsapp::client::WhatsAppClient,
    AppState,
};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "granizado_bot=info,tower_http=info".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    let config = Config::from_env()?;
    let messages = ClientMessages::load_default()?;
    let referral_registry = ReferralRegistry::load_default()?;
    set_client_messages(messages)?;
    set_referral_registry(referral_registry)?;
    let pool = init_pool(&config.database_url).await?;
    sqlx::migrate!().run(&pool).await?;

    let transport = WhatsAppClient::new(
        config.whatsapp_token.clone(),
        config.whatsapp_phone_id.clone(),
    );
    let capi = granizado_bot::capi::CapiClient::new(
        config.capi_dataset_id.clone(),
        config.capi_access_token.clone(),
        config.waba_id.clone(),
    );

    let app_state = AppState {
        llm_budget: granizado_bot::ai::budget::new_llm_budget_handle(
            config.agent_daily_llm_call_limit,
        ),
        webhook_dedup: granizado_bot::new_webhook_dedup_cache(),
        config: config.clone(),
        pool,
        transport,
        capi,
        timers: new_timer_map(),
        conversation_locks: granizado_bot::new_conversation_locks(),
    };

    restore_pending_timers(app_state.clone()).await?;
    let _timer_sweeper = spawn_timer_sweeper(app_state.clone());

    let app: Router = routes::router().with_state(app_state);

    let addr = SocketAddr::new(config.bind_ip, config.port);
    let listener = tokio::net::TcpListener::bind(addr).await?;

    tracing::info!("server listening on {}", addr);
    axum::serve(listener, app).await?;

    Ok(())
}
