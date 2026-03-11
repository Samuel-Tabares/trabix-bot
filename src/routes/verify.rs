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

#[cfg(test)]
mod tests {
    use super::{verify_webhook, VerifyQuery};
    use crate::{
        bot::timers::new_timer_map, config::Config, whatsapp::client::WhatsAppClient, AppState,
    };
    use axum::{extract::Query, extract::State, http::StatusCode, response::IntoResponse};

    fn app_state() -> AppState {
        AppState {
            config: Config {
                whatsapp_token: "token".into(),
                whatsapp_phone_id: "phone".into(),
                whatsapp_verify_token: "verify-me".into(),
                whatsapp_app_secret: "secret".into(),
                database_url: "postgres://db".into(),
                advisor_phone: "573001234567".into(),
                transfer_payment_text: "Nequi 3001234567".into(),
                menu_image_media_id: "menu-media".into(),
                port: 8080,
            },
            pool: sqlx::postgres::PgPoolOptions::new()
                .connect_lazy("postgresql://user:pass@localhost/test_db")
                .expect("lazy pool"),
            wa_client: WhatsAppClient::new("token".into(), "phone".into()),
            timers: new_timer_map(),
        }
    }

    #[tokio::test]
    async fn returns_challenge_for_valid_token() {
        let response = verify_webhook(
            State(app_state()),
            Query(VerifyQuery {
                mode: Some("subscribe".into()),
                verify_token: Some("verify-me".into()),
                challenge: Some("abc123".into()),
            }),
        )
        .await
        .into_response();

        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn rejects_invalid_token() {
        let response = verify_webhook(
            State(app_state()),
            Query(VerifyQuery {
                mode: Some("subscribe".into()),
                verify_token: Some("wrong".into()),
                challenge: Some("abc123".into()),
            }),
        )
        .await
        .into_response();

        assert_eq!(response.status(), StatusCode::FORBIDDEN);
    }
}
