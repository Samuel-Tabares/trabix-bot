use axum::{
    extract::{Query, State},
    http::StatusCode,
    response::IntoResponse,
};
use serde::Deserialize;

use crate::AppState;

#[derive(Debug, Deserialize)]
pub struct VerifyQuery {
    #[serde(rename = "hub.mode")]
    pub mode: Option<String>,
    #[serde(rename = "hub.verify_token")]
    pub verify_token: Option<String>,
    #[serde(rename = "hub.challenge")]
    pub challenge: Option<String>,
}

pub async fn verify_webhook(
    State(state): State<AppState>,
    Query(query): Query<VerifyQuery>,
) -> impl IntoResponse {
    if query.mode.as_deref() != Some("subscribe") {
        return (StatusCode::FORBIDDEN, String::new());
    }

    match (query.verify_token.as_deref(), query.challenge) {
        (Some(token), Some(challenge)) if token == state.config.whatsapp_verify_token => {
            (StatusCode::OK, challenge)
        }
        _ => (StatusCode::FORBIDDEN, String::new()),
    }
}