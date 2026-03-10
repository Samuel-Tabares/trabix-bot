use std::{error::Error, fmt};

use axum::{
    body::Bytes,
    extract::State,
    http::{HeaderMap, HeaderValue, StatusCode},
    response::IntoResponse,
};
use hmac::{Hmac, Mac};
use sha2::Sha256;

use crate::{
    whatsapp::types::{IncomingMessage, InteractiveContent, WebhookPayload},
    AppState,
};

type HmacSha256 = Hmac<Sha256>;

const SIGNATURE_HEADER: &str = "X-Hub-Signature-256";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SignatureError {
    MissingHeader,
    InvalidHeaderFormat,
    InvalidSignature,
}

impl fmt::Display for SignatureError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingHeader => write!(f, "missing X-Hub-Signature-256 header"),
            Self::InvalidHeaderFormat => write!(f, "invalid X-Hub-Signature-256 header format"),
            Self::InvalidSignature => write!(f, "invalid X-Hub-Signature-256 signature"),
        }
    }
}

impl Error for SignatureError {}

pub async fn receive_webhook(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: Bytes,
) -> impl IntoResponse {
    let signature = headers.get(SIGNATURE_HEADER);
    if let Err(err) = verify_signature(&body, signature, &state.config.whatsapp_app_secret) {
        tracing::error!("webhook rejected: {}", err);
        return StatusCode::UNAUTHORIZED;
    }

    let processing_state = state.clone();
    let processing_body = body.clone();
    tokio::spawn(async move {
        if let Err(err) = process_webhook(processing_state, processing_body).await {
            tracing::error!("failed to process webhook: {}", err);
        }
    });

    StatusCode::OK
}

pub fn verify_signature(
    body: &[u8],
    header: Option<&HeaderValue>,
    app_secret: &str,
) -> Result<(), SignatureError> {
    let header = header.ok_or(SignatureError::MissingHeader)?;
    let header = header
        .to_str()
        .map_err(|_| SignatureError::InvalidHeaderFormat)?;
    let provided = header
        .strip_prefix("sha256=")
        .ok_or(SignatureError::InvalidHeaderFormat)?;

    if provided.len() != 64 || !provided.chars().all(|ch| ch.is_ascii_hexdigit()) {
        return Err(SignatureError::InvalidHeaderFormat);
    }

    let mut mac =
        HmacSha256::new_from_slice(app_secret.as_bytes()).map_err(|_| SignatureError::InvalidHeaderFormat)?;
    mac.update(body);
    let expected = to_lower_hex(&mac.finalize().into_bytes());

    if expected.eq_ignore_ascii_case(provided) {
        Ok(())
    } else {
        Err(SignatureError::InvalidSignature)
    }
}

async fn process_webhook(state: AppState, body: Bytes) -> Result<(), Box<dyn Error + Send + Sync>> {
    let payload: WebhookPayload = serde_json::from_slice(&body)?;
    let Some(message) = payload.first_message() else {
        tracing::info!("webhook without incoming messages ignored");
        return Ok(());
    };

    let from = message.from.clone();
    let message_id = message.id.clone();
    let (message_type, content, echo) = describe_message(message);

    tracing::info!(
        phone = %from,
        message_type = %message_type,
        content = %content,
        "received whatsapp message"
    );

    state.wa_client.mark_as_read(&message_id).await?;
    state.wa_client.send_text(&from, &echo).await?;

    Ok(())
}

fn describe_message(message: &IncomingMessage) -> (&'static str, String, String) {
    match message.kind.as_str() {
        "text" => {
            let body = message
                .text
                .as_ref()
                .map(|text| text.body.clone())
                .unwrap_or_default();
            ("text", body.clone(), format!("Recibí tu mensaje: {body}"))
        }
        "interactive" => describe_interactive(message.interactive.as_ref()),
        "image" => {
            let media_id = message
                .image
                .as_ref()
                .map(|image| image.id.clone())
                .unwrap_or_default();
            ("image", media_id, "Recibí tu imagen".to_string())
        }
        other => (
            "unsupported",
            other.to_string(),
            "Recibí tu mensaje".to_string(),
        ),
    }
}

fn describe_interactive(content: Option<&InteractiveContent>) -> (&'static str, String, String) {
    match content.map(|content| content.kind.as_str()) {
        Some("button_reply") => {
            let id = content
                .and_then(|interactive| interactive.button_reply.as_ref())
                .map(|reply| reply.id.clone())
                .unwrap_or_default();
            ("button_reply", id.clone(), format!("Recibí tu mensaje: {id}"))
        }
        Some("list_reply") => {
            let id = content
                .and_then(|interactive| interactive.list_reply.as_ref())
                .map(|reply| reply.id.clone())
                .unwrap_or_default();
            ("list_reply", id.clone(), format!("Recibí tu mensaje: {id}"))
        }
        Some(other) => (
            "interactive",
            other.to_string(),
            "Recibí tu mensaje".to_string(),
        ),
        None => (
            "interactive",
            String::new(),
            "Recibí tu mensaje".to_string(),
        ),
    }
}

fn to_lower_hex(bytes: &[u8]) -> String {
    let mut hex = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        hex.push(nibble_to_hex(byte >> 4));
        hex.push(nibble_to_hex(byte & 0x0f));
    }
    hex
}

fn nibble_to_hex(value: u8) -> char {
    match value {
        0..=9 => (b'0' + value) as char,
        10..=15 => (b'a' + (value - 10)) as char,
        _ => '0',
    }
}

#[cfg(test)]
mod tests {
    use super::{describe_message, verify_signature, SignatureError};
    use crate::whatsapp::types::WebhookPayload;
    use axum::http::HeaderValue;
    use hmac::{Hmac, Mac};
    use sha2::Sha256;

    type HmacSha256 = Hmac<Sha256>;

    #[test]
    fn accepts_valid_signature() {
        let body = br#"{"ok":true}"#;
        let mut mac = HmacSha256::new_from_slice(b"secret").expect("hmac key");
        mac.update(body);
        let signature = format!("sha256={:x}", mac.finalize().into_bytes());
        let header = HeaderValue::from_str(&signature).expect("header");

        let result = verify_signature(body, Some(&header), "secret");

        assert_eq!(result, Ok(()));
    }

    #[test]
    fn rejects_missing_signature() {
        let result = verify_signature(br#"{}"#, None, "secret");

        assert_eq!(result, Err(SignatureError::MissingHeader));
    }

    #[test]
    fn rejects_malformed_signature_header() {
        let header = HeaderValue::from_static("bad-header");

        let result = verify_signature(br#"{}"#, Some(&header), "secret");

        assert_eq!(result, Err(SignatureError::InvalidHeaderFormat));
    }

    #[test]
    fn rejects_invalid_signature() {
        let header = HeaderValue::from_static(
            "sha256=aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
        );

        let result = verify_signature(br#"{}"#, Some(&header), "secret");

        assert_eq!(result, Err(SignatureError::InvalidSignature));
    }

    #[test]
    fn describes_text_payload() {
        let payload: WebhookPayload = serde_json::from_str(
            r#"{
                "entry": [{
                    "changes": [{
                        "value": {
                            "messages": [{
                                "from": "573001234567",
                                "type": "text",
                                "text": { "body": "Hola" },
                                "id": "wamid.1"
                            }]
                        }
                    }]
                }]
            }"#,
        )
        .expect("payload");

        let message = payload.first_message().expect("message");
        let (_, content, echo) = describe_message(message);

        assert_eq!(content, "Hola");
        assert_eq!(echo, "Recibí tu mensaje: Hola");
    }
}
