use axum::routing::{get, patch, post};
use axum::Router;

use crate::AppState;

pub mod internal;
pub mod legal;
pub mod verify;
pub mod webhook;

/// Sirve en el listener público (mismo puerto que Railway expone a internet):
/// solo lo que Meta necesita golpear desde afuera.
pub fn public_router() -> Router<AppState> {
    Router::new()
        .route("/privacy-policy", get(legal::privacy_policy))
        .route("/terms-of-service", get(legal::terms_of_service))
        .route(
            "/webhook",
            get(verify::verify_webhook).post(webhook::receive_webhook),
        )
}

/// Sirve en un listener aparte (puerto propio, sin dominio público asignado en
/// Railway) para que `/internal/*` sea alcanzable únicamente por la red
/// privada, nunca desde internet — antes compartía listener con `/webhook` y
/// el token era la única protección real.
pub fn internal_router() -> Router<AppState> {
    Router::new()
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
