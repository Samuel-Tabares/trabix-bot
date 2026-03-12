use axum::routing::get;
use axum::Router;

use crate::AppState;

pub mod legal;
pub mod verify;
pub mod webhook;

pub fn router() -> Router<AppState> {
    Router::new()
        .route(
            "/webhook",
            get(verify::verify_webhook).post(webhook::receive_webhook),
        )
        .route("/privacy-policy", get(legal::privacy_policy))
        .route("/terms-of-service", get(legal::terms_of_service))
}
