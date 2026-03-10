use std::net::{IpAddr, Ipv4Addr, SocketAddr};

use axum::Router;
use granizado_bot::{config::Config, db::init_pool, routes, whatsapp::client::WhatsAppClient, AppState};
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
    let pool = init_pool(&config.database_url).await?;
    sqlx::migrate!().run(&pool).await?;

    let wa_client = WhatsAppClient::new(
        config.whatsapp_token.clone(),
        config.whatsapp_phone_id.clone(),
    );

    let app_state = AppState {
        config: config.clone(),
        pool,
        wa_client,
    };

    let app: Router = routes::router().with_state(app_state);

    let addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::UNSPECIFIED), config.port);
    let listener = tokio::net::TcpListener::bind(addr).await?;

    tracing::info!("server listening on {}", addr);
    axum::serve(listener, app).await?;

    Ok(())
}
