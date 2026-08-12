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
    extract::{rejection::JsonRejection, Path, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    Json,
};
use chrono::{Duration, Utc};
use serde::{Deserialize, Serialize};
use serde_json::json;
use sqlx::error::DatabaseError;

use crate::{
    bot::state_machine::UserInput,
    db::{
        models::ReferralCode,
        queries::{
            clear_human_takeover, create_referral_code as db_create_referral_code, get_conversation,
            record_message_event, set_human_takeover, set_referral_code_active as db_set_referral_code_active,
            set_referral_code_boost as db_set_referral_code_boost, update_last_message,
        },
    },
    logging::{mask_phone, preview_text},
    referrals::{swap_referral_registry, validate_registry_code, ReferralRegistry},
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

#[derive(Debug, Deserialize)]
pub struct AdvisorReleaseRequest {
    pub case_phone: String,
    /// Quién lo liberó desde la consola. Solo para el log.
    #[serde(default)]
    pub sent_by: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct AdvisorSendResponse {
    pub wa_message_id: Option<String>,
}

/// No devuelve `wa_message_id` a propósito: el turno del asesor puede generar
/// cero, uno o varios mensajes al cliente (o ninguno, si el agente solo actualiza
/// el estado del pedido). No hay un único id que devolver.
#[derive(Debug, Serialize)]
pub struct AdvisorReplyResponse {
    pub ok: bool,
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
    DuplicateReferralCode,
    UnknownReferralCode,
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
            Self::DuplicateReferralCode => "duplicate_code",
            Self::UnknownReferralCode => "unknown_code",
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
            Self::DuplicateReferralCode => StatusCode::CONFLICT,
            Self::UnknownReferralCode => StatusCode::NOT_FOUND,
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
            Self::DuplicateReferralCode => "ya existe un código de referido con ese valor".into(),
            Self::UnknownReferralCode => "no existe un código de referido con ese valor".into(),
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

    // Fase 2: un humano escribiendo texto libre desde la consola es la señal
    // de toma de control, sin botón ni estado manual — ver
    // docs/internal_advisor_send.md. Ventana deslizante: se reemplaza en cada
    // envío, no se acumula. Deliberadamente NO se hace lo mismo en
    // `advisor_reply`: ese endpoint existe para que el bot SIGA el checkout
    // automático después de que el asesor destraba una pregunta puntual.
    let takeover_until = Utc::now() + Duration::hours(state.config.advisor_takeover_hours as i64);
    if let Err(err) = set_human_takeover(&state.pool, &case_phone, takeover_until).await {
        tracing::warn!(error = %err, "failed to set human takeover window after internal send");
    }

    tracing::info!(
        case_phone = %mask_phone(&case_phone),
        sent_by = %payload.sent_by.as_deref().unwrap_or("<desconocido>"),
        message_id = %wa_message_id.as_deref().unwrap_or("<none>"),
        preview = %preview_text(&body),
        takeover_until = %takeover_until,
        "advisor message sent from crm-app"
    );

    Ok(Json(AdvisorSendResponse { wa_message_id }))
}

/// Respuesta del asesor **hacia el bot**, no hacia el cliente.
///
/// La diferencia con `/internal/advisor/send` es la que decide si un pedido
/// avanza o se queda colgado:
///
/// - `send` manda texto crudo al cliente y **se salta el agente**. Sirve para
///   hablarle al cliente, no para contestarle al bot.
/// - `reply` (esto) inyecta el mensaje en el turno de agente del asesor, igual
///   que si hubiera contestado por WhatsApp. Es lo único que dispara
///   `set_manual_delivery_cost` (¿cuánto vale el domicilio a este municipio?),
///   el único paso bloqueante que le queda al flujo de pedido.
///
/// El caso viene explícito en `case_phone` — la consola ya sabe en qué
/// conversación está parada, así que no hace falta el mecanismo de botones que
/// usa el canal de WhatsApp para adivinar a qué cliente le está respondiendo.
pub async fn advisor_reply(
    State(state): State<AppState>,
    headers: HeaderMap,
    payload: Result<Json<AdvisorSendRequest>, JsonRejection>,
) -> Result<Json<AdvisorReplyResponse>, ApiError> {
    authorize(&headers, state.config.internal_api_token.as_deref())?;

    let Json(payload) = payload
        .map_err(|err| ApiError::InvalidRequest(format!("cuerpo JSON inválido: {err}")))?;

    let case_phone = validate_phone(&payload.case_phone)?;
    let body = validate_body(&payload.body)?;

    // Se valida ANTES de tomar el turno: el motor asume que el caso existe, y
    // así la consola recibe un 404 limpio en vez de un error interno.
    let conversation = get_conversation(&state.pool, &case_phone)
        .await
        .map_err(|err| ApiError::Internal(format!("error consultando la conversación: {err}")))?;
    if conversation.is_none() {
        return Err(ApiError::UnknownCase);
    }

    // El turno toma el lock de la conversación por dentro; acá no se toma para
    // no bloquearse contra sí mismo.
    crate::engine::process_advisor_turn_for_case(
        &state,
        &case_phone,
        UserInput::TextMessage(body.clone()),
    )
    .await
    .map_err(|err| {
        tracing::error!(
            case_phone = %mask_phone(&case_phone),
            error = %err,
            "internal advisor reply failed"
        );
        ApiError::Internal(format!("el turno del asesor falló: {err}"))
    })?;

    tracing::info!(
        case_phone = %mask_phone(&case_phone),
        sent_by = %payload.sent_by.as_deref().unwrap_or("<desconocido>"),
        preview = %preview_text(&body),
        "advisor reply processed from crm-app"
    );

    Ok(Json(AdvisorReplyResponse { ok: true }))
}

/// Devuelve la conversación al bot antes de que venza la ventana de
/// `set_human_takeover` (Fase 2). No manda nada a Meta ni escribe
/// `message_events` — solo limpia `conversations.human_takeover_until`. Lo
/// usa el botón "Devolver al bot" de `crm-app`.
pub async fn advisor_release(
    State(state): State<AppState>,
    headers: HeaderMap,
    payload: Result<Json<AdvisorReleaseRequest>, JsonRejection>,
) -> Result<Json<AdvisorReplyResponse>, ApiError> {
    authorize(&headers, state.config.internal_api_token.as_deref())?;

    let Json(payload) = payload
        .map_err(|err| ApiError::InvalidRequest(format!("cuerpo JSON inválido: {err}")))?;

    let case_phone = validate_phone(&payload.case_phone)?;

    let conversation = get_conversation(&state.pool, &case_phone)
        .await
        .map_err(|err| ApiError::Internal(format!("error consultando la conversación: {err}")))?;
    if conversation.is_none() {
        return Err(ApiError::UnknownCase);
    }

    clear_human_takeover(&state.pool, &case_phone)
        .await
        .map_err(|err| ApiError::Internal(format!("error liberando la conversación: {err}")))?;

    tracing::info!(
        case_phone = %mask_phone(&case_phone),
        sent_by = %payload.sent_by.as_deref().unwrap_or("<desconocido>"),
        "conversation released back to bot from crm-app"
    );

    Ok(Json(AdvisorReplyResponse { ok: true }))
}

/// Proxy de un adjunto de WhatsApp (imagen de comprobante, etc.) por
/// `media_id` — `crm-app` no tiene credenciales de Meta propias, así que no
/// puede resolver ni descargar el media directamente; se lo pide a este
/// endpoint, el único lugar que sí tiene el token (mismo principio que
/// `advisor_send`/etc., ver el comentario de arriba del archivo). No valida
/// que el `media_id` pertenezca a un caso conocido: son IDs opacos de Meta,
/// de un solo uso práctico (expiran), sin valor fuera de este contexto.
pub async fn media(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(media_id): Path<String>,
) -> Result<Response, ApiError> {
    authorize(&headers, state.config.internal_api_token.as_deref())?;

    let (bytes, mime_type) = state
        .transport
        .download_media(&media_id)
        .await
        .map_err(|err| {
            tracing::error!(error = %err, "internal media download failed at meta");
            classify_whatsapp_error(&err)
        })?;

    Ok((
        StatusCode::OK,
        [
            (axum::http::header::CONTENT_TYPE, mime_type),
            (
                axum::http::header::CACHE_CONTROL,
                "private, max-age=86400".to_string(),
            ),
        ],
        bytes,
    )
        .into_response())
}

#[derive(Debug, Deserialize)]
pub struct CreateReferralCodeRequest {
    pub code: String,
}

#[derive(Debug, Deserialize)]
pub struct SetReferralCodeActiveRequest {
    pub active: bool,
}

/// Fase 6: `crm-app` gestiona códigos de referido a través de estos 3
/// endpoints en vez de escribir directo a `referral_codes` — el bot sigue
/// siendo el único escritor de una tabla que afecta precio/descuento en
/// producción, y reusa la validación de formato que ya vivía en
/// `referrals.rs` para el TOML.
pub async fn create_referral_code(
    State(state): State<AppState>,
    headers: HeaderMap,
    payload: Result<Json<CreateReferralCodeRequest>, JsonRejection>,
) -> Result<(StatusCode, Json<ReferralCode>), ApiError> {
    authorize(&headers, state.config.internal_api_token.as_deref())?;

    let Json(payload) = payload
        .map_err(|err| ApiError::InvalidRequest(format!("cuerpo JSON inválido: {err}")))?;

    let normalized =
        validate_registry_code(&payload.code).map_err(|err| ApiError::InvalidRequest(err.to_string()))?;

    let created = db_create_referral_code(&state.pool, &normalized)
        .await
        .map_err(|err| {
            if is_unique_violation(&err) {
                ApiError::DuplicateReferralCode
            } else {
                ApiError::Internal(format!("error creando el código: {err}"))
            }
        })?;

    refresh_referral_cache(&state.pool).await;

    tracing::info!(code = %normalized, "referral code created from crm-app");

    Ok((StatusCode::CREATED, Json(created)))
}

pub async fn set_referral_code_active(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(code): Path<String>,
    payload: Result<Json<SetReferralCodeActiveRequest>, JsonRejection>,
) -> Result<Json<ReferralCode>, ApiError> {
    authorize(&headers, state.config.internal_api_token.as_deref())?;

    let Json(payload) = payload
        .map_err(|err| ApiError::InvalidRequest(format!("cuerpo JSON inválido: {err}")))?;

    let normalized = crate::referrals::normalize_referral_code(&code);
    let updated = db_set_referral_code_active(&state.pool, &normalized, payload.active)
        .await
        .map_err(|err| ApiError::Internal(format!("error actualizando el código: {err}")))?
        .ok_or(ApiError::UnknownReferralCode)?;

    refresh_referral_cache(&state.pool).await;

    tracing::info!(
        code = %normalized,
        active = payload.active,
        "referral code active flag updated from crm-app"
    );

    Ok(Json(updated))
}

pub async fn boost_referral_code(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(code): Path<String>,
) -> Result<Json<ReferralCode>, ApiError> {
    authorize(&headers, state.config.internal_api_token.as_deref())?;

    let normalized = crate::referrals::normalize_referral_code(&code);
    let updated = db_set_referral_code_boost(&state.pool, &normalized)
        .await
        .map_err(|err| ApiError::Internal(format!("error activando el boost: {err}")))?
        .ok_or(ApiError::UnknownReferralCode)?;

    refresh_referral_cache(&state.pool).await;

    tracing::info!(code = %normalized, "referral code boost activated from crm-app");

    Ok(Json(updated))
}

fn is_unique_violation(err: &sqlx::Error) -> bool {
    err.as_database_error()
        .is_some_and(DatabaseError::is_unique_violation)
}

/// Recarga el registro completo desde la DB y lo publica de inmediato — el
/// refresco de background (cada 30s, `main.rs`) queda como red de
/// seguridad, no como el único camino de propagación.
async fn refresh_referral_cache(pool: &sqlx::PgPool) {
    match ReferralRegistry::load_from_db(pool).await {
        Ok(registry) => swap_referral_registry(registry),
        Err(err) => tracing::warn!(%err, "failed to refresh referral registry after write"),
    }
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
