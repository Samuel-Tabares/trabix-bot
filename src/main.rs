use std::net::SocketAddr;

use axum::Router;
use granizado_bot::{
    bot::pricing::{fetch_pricing_table, init_pricing_table, swap_pricing_table, PricingTable},
    bot::timers::{new_timer_map, restore_pending_timers, spawn_timer_sweeper},
    config::Config,
    db::init_pool,
    messages::{set_client_messages, ClientMessages},
    referrals::{init_referral_registry, swap_referral_registry, ReferralRegistry},
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
    set_client_messages(messages)?;

    let pool = init_pool(&config.database_url).await?;
    sqlx::migrate!().run(&pool).await?;

    let initial_referral_registry = ReferralRegistry::load_from_db(&pool).await?;
    init_referral_registry(initial_referral_registry);

    // Tiers mayoristas: fuente viva es `crm-app` (`/settings/precios`), no un
    // espejo estático a mano. Sin las dos variables el bot arranca igual con
    // los tiers compilados por defecto — nunca bloquea el arranque. La
    // propagación normal es instantánea (`crm-app` llama
    // `POST /internal/pricing/refresh` justo después de guardar, ver
    // `routes/internal.rs::refresh_pricing`) — este refresco cada hora es
    // solo la red de seguridad, igual que el de `referrals.rs` cada 30s para
    // ese cache (acá una hora alcanza de sobra: los precios cambian mucho
    // menos seguido que los códigos de referido).
    let pricing_http_client = reqwest::Client::new();
    if let (Some(pricing_url), Some(pricing_token)) = (
        config.crm_app_pricing_url.clone(),
        config.crm_app_pricing_token.clone(),
    ) {
        match fetch_pricing_table(&pricing_http_client, &pricing_url, &pricing_token).await {
            Ok(table) => init_pricing_table(table),
            Err(err) => {
                tracing::warn!(%err, "initial pricing fetch failed, using compiled defaults");
                init_pricing_table(PricingTable::default());
            }
        }

        let refresh_client = pricing_http_client.clone();
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(std::time::Duration::from_secs(3_600));
            loop {
                interval.tick().await;
                match fetch_pricing_table(&refresh_client, &pricing_url, &pricing_token).await {
                    Ok(table) => swap_pricing_table(table),
                    Err(err) => tracing::warn!(%err, "failed to refresh pricing table"),
                }
            }
        });
    } else {
        init_pricing_table(PricingTable::default());
    }

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

    // Fase 6: red de seguridad si algo escribe `referral_codes` sin pasar por
    // los endpoints internos (o si otra instancia lo hizo). Las escrituras
    // normales ya refrescan el cache al instante — esto solo cubre el peor
    // caso, hasta 30s de rezago.
    {
        let pool = app_state.pool.clone();
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(std::time::Duration::from_secs(30));
            loop {
                interval.tick().await;
                match ReferralRegistry::load_from_db(&pool).await {
                    Ok(registry) => swap_referral_registry(registry),
                    Err(err) => tracing::warn!(%err, "failed to refresh referral registry"),
                }
            }
        });
    }

    let public_app: Router = routes::public_router().with_state(app_state.clone());
    let internal_app: Router = routes::internal_router().with_state(app_state);

    let public_addr = SocketAddr::new(config.bind_ip, config.port);
    let internal_addr = SocketAddr::new(config.bind_ip, config.internal_port);
    let public_listener = tokio::net::TcpListener::bind(public_addr).await?;
    let internal_listener = tokio::net::TcpListener::bind(internal_addr).await?;

    tracing::info!("public server listening on {}", public_addr);
    tracing::info!(
        "internal server listening on {} (private network only)",
        internal_addr
    );

    let public_server = axum::serve(public_listener, public_app);
    let internal_server = axum::serve(internal_listener, internal_app);
    tokio::try_join!(public_server, internal_server)?;

    Ok(())
}
