use axum::routing::get;
use axum::Router;

use crate::AppState;

pub mod verify;
pub mod webhook;

pub fn router() -> Router<AppState> {
    Router::new().route("/webhook", get(verify::verify_webhook).post(webhook::receive_webhook))
}
