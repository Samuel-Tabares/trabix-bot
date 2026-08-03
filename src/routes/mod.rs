use axum::routing::{get, patch, post};
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
        .route(
            "/internal/referral-codes",
            post(internal::create_referral_code),
        )
        .route(
            "/internal/referral-codes/:code",
            patch(internal::set_referral_code_active),
        )
        .route(
            "/internal/referral-codes/:code/boost",
            post(internal::boost_referral_code),
        )
}
