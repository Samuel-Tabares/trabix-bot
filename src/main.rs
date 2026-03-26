use std::net::SocketAddr;

use axum::Router;
use granizado_bot::{
    bot::timers::{
        new_timer_map, new_timer_overrides, restore_pending_timers, spawn_timer_sweeper,
    },
    config::{BotMode, Config},
    db::init_pool,
    messages::{set_client_messages, ClientMessages},
    routes,
    transport::{OutboundTransport, SimulatorTransport},
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

    let transport = match config.mode {
        BotMode::Production => {
            let production = config.production();
            OutboundTransport::Production(WhatsAppClient::new(
                production.whatsapp_token.clone(),
                production.whatsapp_phone_id.clone(),
            ))
        }
        BotMode::Simulator => {
            let simulator = config.simulator();
            std::fs::create_dir_all(&simulator.upload_dir)?;
            OutboundTransport::Simulator(SimulatorTransport {
                menu_image_path: simulator.menu_image_path.clone(),
            })
        }
    };

    let app_state = AppState {
        config: config.clone(),
        pool,
        transport,
        timers: new_timer_map(),
        timer_overrides: new_timer_overrides(),
    };

    restore_pending_timers(app_state.clone()).await?;
    let _timer_sweeper = spawn_timer_sweeper(app_state.clone());

    let app: Router = routes::router(&config).with_state(app_state);

    let addr = SocketAddr::new(config.bind_ip, config.port);
    let listener = tokio::net::TcpListener::bind(addr).await?;

    tracing::info!("server listening on {}", addr);
    axum::serve(listener, app).await?;

    Ok(())
}
