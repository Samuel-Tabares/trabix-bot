use axum::routing::get;
use axum::Router;

use crate::{
    config::{BotMode, Config},
    AppState,
};

pub mod legal;
pub mod simulator;
pub mod verify;
pub mod webhook;

pub fn router(config: &Config) -> Router<AppState> {
    let router = Router::new()
        .route("/privacy-policy", get(legal::privacy_policy))
        .route("/terms-of-service", get(legal::terms_of_service));

    match config.mode {
        BotMode::Production => router.route(
            "/webhook",
            get(verify::verify_webhook).post(webhook::receive_webhook),
        ),
        BotMode::Simulator => simulator::mount(router),
    }
}
