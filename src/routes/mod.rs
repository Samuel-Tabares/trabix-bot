use axum::routing::{get, post};
use axum::Router;

use crate::AppState;

pub mod internal;
pub mod legal;
pub mod verify;
pub mod webhook;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/privacy-policy", get(legal::privacy_policy))
        .route("/terms-of-service", get(legal::terms_of_service))
        .route(
            "/webhook",
            get(verify::verify_webhook).post(webhook::receive_webhook),
        )
        .route("/internal/advisor/send", post(internal::advisor_send))
        .route("/internal/advisor/reply", post(internal::advisor_reply))
        .route("/internal/advisor/release", post(internal::advisor_release))
}
