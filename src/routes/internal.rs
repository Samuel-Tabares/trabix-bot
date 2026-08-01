//! Endpoint interno para que `crm-app` mande WhatsApp sin volverse un segundo
//! escritor sobre la conversación.
//!
//! El bot sigue siendo el ÚNICO dueño de la sesión de WhatsApp: `crm-app` no
//! habla con la Graph API de Meta, le pide a este endpoint que hable por él.
//! Así hay un solo emisor, un solo lugar donde se traza (`message_events`) y un
//! solo sitio donde viven las credenciales de Meta.
//!
//! No es público: exige el header `X-Internal-Token` con el valor de
//! `INTERNAL_API_TOKEN`. Si esa variable no está configurada el endpoint queda
//! deshabilitado (503) en vez de quedar abierto.

use axum::{
    extract::{rejection::JsonRejection, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    Json,
};
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::{
    db::queries::{get_conversation, record_message_event, update_last_message},
    logging::{mask_phone, preview_text},
    whatsapp::client::WhatsAppError,
    AppState,
};

pub const TOKEN_HEADER: &str = "x-internal-token";

/// Límite de texto de la Cloud API de Meta.
const MAX_BODY_CHARS: usize = 4096;

const CHANNEL_CLIENT: &str = "client";
const ACTOR_ADVISOR: &str = "advisor";

#[derive(Debug, Deserialize)]
pub struct AdvisorSendRequest {
    pub case_phone: String,
    pub body: String,
    /// Quién lo mandó desde la consola (usuario del CRM). Solo para la traza.
    #[serde(default)]
    pub sent_by: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct AdvisorSendResponse {
    pub wa_message_id: Option<String>,
}

#[derive(Debug, Serialize)]
struct ErrorBody {
    code: &'static str,
    message: String,
}

/// Códigos alineados con `SendError["code"]` de `crm-app/src/server/inbox/send.ts`
/// para que la consola pueda mapearlos a mensajes de UI sin traducir nada.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ApiError {
    Disabled,
    Unauthorized,
    InvalidRequest(String),
    UnknownCase,
    WindowClosed,
    MetaError(String),
    MetaUnavailable(String),
    Internal(String),
}

impl ApiError {
    fn code(&self) -> &'static str {
        match self {
            Self::Disabled => "not_connected",
            Self::Unauthorized => "unauthorized",
            Self::InvalidRequest(_) => "invalid_request",
            Self::UnknownCase => "unknown_case",
            Self::WindowClosed => "window_closed",
            Self::MetaError(_) => "meta_error",
            Self::MetaUnavailable(_) => "meta_unavailable",
            Self::Internal(_) => "internal_error",
        }
    }

    fn status(&self) -> StatusCode {
        match self {
            Self::Disabled => StatusCode::SERVICE_UNAVAILABLE,
            Self::Unauthorized => StatusCode::UNAUTHORIZED,
            Self::InvalidRequest(_) => StatusCode::BAD_REQUEST,
            Self::UnknownCase => StatusCode::NOT_FOUND,
            Self::WindowClosed => StatusCode::CONFLICT,
            Self::MetaError(_) => StatusCode::BAD_GATEWAY,
            Self::MetaUnavailable(_) => StatusCode::BAD_GATEWAY,
            Self::Internal(_) => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }

    fn message(&self) -> String {
        match self {
            Self::Disabled => "el endpoint interno no está configurado en este despliegue".into(),
            Self::Unauthorized => "token interno inválido o ausente".into(),
            Self::InvalidRequest(detail) => detail.clone(),
            Self::UnknownCase => "no existe una conversación para ese número".into(),
            Self::WindowClosed => {
                "la ventana de 24h con el cliente está cerrada; se necesita una plantilla".into()
            }
            Self::MetaError(detail) | Self::MetaUnavailable(detail) | Self::Internal(detail) => {
                detail.clone()
            }
        }
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        (
            self.status(),
            Json(ErrorBody {
                code: self.code(),
                message: self.message(),
            }),
        )
            .into_response()
    }
}

pub async fn advisor_send(
    State(state): State<AppState>,
    headers: HeaderMap,
    payload: Result<Json<AdvisorSendRequest>, JsonRejection>,
) -> Result<Json<AdvisorSendResponse>, ApiError> {
    authorize(&headers, state.config.internal_api_token.as_deref())?;

    let Json(payload) = payload
        .map_err(|err| ApiError::InvalidRequest(format!("cuerpo JSON inválido: {err}")))?;

    let case_phone = validate_phone(&payload.case_phone)?;
    let body = validate_body(&payload.body)?;

    // Mismo lock que usa el motor: si el agente está a mitad de un turno para
    // este cliente, el mensaje del asesor espera en vez de intercalarse.
    let _case_lock = crate::lock_conversation(&state.conversation_locks, &case_phone).await;

    let conversation = get_conversation(&state.pool, &case_phone)
        .await
        .map_err(|err| ApiError::Internal(format!("error consultando la conversación: {err}")))?;
    if conversation.is_none() {
        return Err(ApiError::UnknownCase);
    }

    let wa_message_id = state
        .transport
        .send_text(&case_phone, &body)
        .await
        .map_err(|err| {
            tracing::error!(
                case_phone = %mask_phone(&case_phone),
                error = %err,
                "internal advisor send failed at meta"
            );
            classify_whatsapp_error(&err)
        })?;

    // La traza es best-effort a propósito: el mensaje ya salió, y devolver
    // error acá haría que la consola reintente y el cliente reciba doble.
    if let Err(err) = record_message_event(
        &state.pool,
        &case_phone,
        CHANNEL_CLIENT,
        ACTOR_ADVISOR,
        "text",
        Some(&body),
        Some(json!({ "source": "crm-app", "sent_by": payload.sent_by })),
        wa_message_id.as_deref(),
    )
    .await
    {
        tracing::warn!(error = %err, "failed to record internal advisor send event");
    }

    if let Err(err) = update_last_message(&state.pool, &case_phone).await {
        tracing::warn!(error = %err, "failed to bump last_message_at after internal send");
    }

    tracing::info!(
        case_phone = %mask_phone(&case_phone),
        sent_by = %payload.sent_by.as_deref().unwrap_or("<desconocido>"),
        message_id = %wa_message_id.as_deref().unwrap_or("<none>"),
        preview = %preview_text(&body),
        "advisor message sent from crm-app"
    );

    Ok(Json(AdvisorSendResponse { wa_message_id }))
}

fn authorize(headers: &HeaderMap, configured: Option<&str>) -> Result<(), ApiError> {
    let Some(expected) = configured.map(str::trim).filter(|token| !token.is_empty()) else {
        return Err(ApiError::Disabled);
    };
    let provided = headers
        .get(TOKEN_HEADER)
        .and_then(|value| value.to_str().ok())
        .ok_or(ApiError::Unauthorized)?;

    if constant_time_eq(provided.as_bytes(), expected.as_bytes()) {
        Ok(())
    } else {
        Err(ApiError::Unauthorized)
    }
}

/// Compara sin salir temprano en el primer byte distinto. La longitud sí se
/// filtra, pero eso no ayuda a adivinar un secreto aleatorio.
fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    a.iter()
        .zip(b.iter())
        .fold(0u8, |acc, (x, y)| acc | (x ^ y))
        == 0
}

fn validate_phone(raw: &str) -> Result<String, ApiError> {
    let trimmed = raw.trim().trim_start_matches('+');
    if trimmed.is_empty() {
        return Err(ApiError::InvalidRequest("case_phone vacío".into()));
    }
    if !trimmed.chars().all(|ch| ch.is_ascii_digit()) {
        return Err(ApiError::InvalidRequest(
            "case_phone debe ser solo dígitos (formato E.164 sin '+')".into(),
        ));
    }
    if !(10..=15).contains(&trimmed.len()) {
        return Err(ApiError::InvalidRequest(
            "case_phone debe tener entre 10 y 15 dígitos".into(),
        ));
    }
    Ok(trimmed.to_string())
}

fn validate_body(raw: &str) -> Result<String, ApiError> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err(ApiError::InvalidRequest("body vacío".into()));
    }
    if trimmed.chars().count() > MAX_BODY_CHARS {
        return Err(ApiError::InvalidRequest(format!(
            "body excede {MAX_BODY_CHARS} caracteres"
        )));
    }
    Ok(trimmed.to_string())
}

/// Meta devuelve 131047 (y el legacy 470) cuando pasaron más de 24h desde el
/// último mensaje del cliente: ahí no sirve texto libre, hay que usar plantilla.
/// La consola necesita distinguir ese caso para decirle algo útil al asesor.
fn classify_whatsapp_error(err: &WhatsAppError) -> ApiError {
    match err {
        WhatsAppError::Request(inner) => ApiError::MetaUnavailable(format!(
            "no se pudo contactar a Meta: {inner}"
        )),
        WhatsAppError::Api { status, body } => {
            if let Some(code) = meta_error_code(body) {
                if matches!(code, 131047 | 470) {
                    return ApiError::WindowClosed;
                }
            }
            if status.is_server_error() {
                ApiError::MetaUnavailable(format!("Meta respondió {status}"))
            } else {
                ApiError::MetaError(format!("Meta rechazó el envío ({status})"))
            }
        }
    }
}

fn meta_error_code(body: &str) -> Option<i64> {
    serde_json::from_str::<serde_json::Value>(body)
        .ok()?
        .get("error")?
        .get("code")?
        .as_i64()
}

#[cfg(test)]
mod tests {
    use axum::http::{HeaderMap, HeaderValue, StatusCode};
    use reqwest::StatusCode as ReqwestStatus;

    use super::{
        authorize, classify_whatsapp_error, constant_time_eq, meta_error_code, validate_body,
        validate_phone, ApiError, TOKEN_HEADER,
    };
    use crate::whatsapp::client::WhatsAppError;

    fn headers_with(token: &str) -> HeaderMap {
        let mut headers = HeaderMap::new();
        headers.insert(TOKEN_HEADER, HeaderValue::from_str(token).expect("header"));
        headers
    }

    #[test]
    fn rejects_when_token_not_configured() {
        assert_eq!(
            authorize(&headers_with("whatever"), None),
            Err(ApiError::Disabled)
        );
        assert_eq!(
            authorize(&headers_with("whatever"), Some("   ")),
            Err(ApiError::Disabled)
        );
    }

    #[test]
    fn rejects_missing_or_wrong_token() {
        assert_eq!(
            authorize(&HeaderMap::new(), Some("secret")),
            Err(ApiError::Unauthorized)
        );
        assert_eq!(
            authorize(&headers_with("nope"), Some("secret")),
            Err(ApiError::Unauthorized)
        );
    }

    #[test]
    fn accepts_matching_token() {
        assert_eq!(authorize(&headers_with("secret"), Some("secret")), Ok(()));
    }

    #[test]
    fn constant_time_eq_matches_semantics_of_eq() {
        assert!(constant_time_eq(b"abc", b"abc"));
        assert!(!constant_time_eq(b"abc", b"abd"));
        assert!(!constant_time_eq(b"abc", b"abcd"));
    }

    #[test]
    fn normalizes_and_validates_phone() {
        assert_eq!(validate_phone(" +573001234567 ").unwrap(), "573001234567");
        assert!(matches!(
            validate_phone("57300abc4567"),
            Err(ApiError::InvalidRequest(_))
        ));
        assert!(matches!(
            validate_phone("573"),
            Err(ApiError::InvalidRequest(_))
        ));
    }

    #[test]
    fn validates_body() {
        assert_eq!(validate_body("  hola  ").unwrap(), "hola");
        assert!(matches!(
            validate_body("   "),
            Err(ApiError::InvalidRequest(_))
        ));
        let long = "a".repeat(4097);
        assert!(matches!(
            validate_body(&long),
            Err(ApiError::InvalidRequest(_))
        ));
    }

    #[test]
    fn detects_closed_window_from_meta_body() {
        let body = r#"{"error":{"message":"Re-engagement message","code":131047}}"#;
        assert_eq!(meta_error_code(body), Some(131047));

        let err = WhatsAppError::Api {
            status: ReqwestStatus::BAD_REQUEST,
            body: body.to_string(),
        };
        assert_eq!(classify_whatsapp_error(&err), ApiError::WindowClosed);
    }

    #[test]
    fn classifies_other_meta_failures() {
        let err = WhatsAppError::Api {
            status: ReqwestStatus::BAD_REQUEST,
            body: r#"{"error":{"message":"Invalid parameter","code":100}}"#.to_string(),
        };
        assert!(matches!(
            classify_whatsapp_error(&err),
            ApiError::MetaError(_)
        ));

        let err = WhatsAppError::Api {
            status: ReqwestStatus::INTERNAL_SERVER_ERROR,
            body: "{}".to_string(),
        };
        assert!(matches!(
            classify_whatsapp_error(&err),
            ApiError::MetaUnavailable(_)
        ));
    }

    #[test]
    fn error_statuses_are_stable() {
        assert_eq!(ApiError::Unauthorized.status(), StatusCode::UNAUTHORIZED);
        assert_eq!(ApiError::UnknownCase.status(), StatusCode::NOT_FOUND);
        assert_eq!(ApiError::WindowClosed.status(), StatusCode::CONFLICT);
        assert_eq!(
            ApiError::Disabled.status(),
            StatusCode::SERVICE_UNAVAILABLE
        );
    }
}
