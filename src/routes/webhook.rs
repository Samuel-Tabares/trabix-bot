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
    bot::state_machine::{extract_input, UserInput},
    engine::{mark_as_read_if_supported, process_advisor_input, process_customer_input},
    logging::{mask_phone, preview_text},
    whatsapp::types::{Contact, WebhookPayload},
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
    if let Err(err) = verify_signature(
        &body,
        signature,
        &state.config.production().whatsapp_app_secret,
    ) {
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

    let mut mac = HmacSha256::new_from_slice(app_secret.as_bytes())
        .map_err(|_| SignatureError::InvalidHeaderFormat)?;
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
    let mut first_error: Option<Box<dyn Error + Send + Sync>> = None;
    let mut processed_any_message = false;

    for event in payload.message_events() {
        processed_any_message = true;

        if let Err(err) =
            process_incoming_message(state.clone(), event.message, event.contact).await
        {
            tracing::error!("failed to process inbound whatsapp message: {}", err);
            if first_error.is_none() {
                first_error = Some(err);
            }
        }
    }

    if !processed_any_message {
        let status_events = payload.status_events();
        if status_events.is_empty() {
            tracing::debug!("webhook without incoming messages or statuses ignored");
        } else {
            for status in status_events {
                tracing::debug!(
                    recipient = %status
                        .recipient_id
                        .as_deref()
                        .map(mask_phone)
                        .unwrap_or_else(|| "<unknown>".to_string()),
                    status = %status.status,
                    message_id = %status.id.unwrap_or_else(|| "<unknown>".to_string()),
                    "received whatsapp status update"
                );
            }
        }
        return Ok(());
    }

    if let Some(err) = first_error {
        return Err(err);
    }

    Ok(())
}

async fn process_incoming_message(
    state: AppState,
    message: crate::whatsapp::types::IncomingMessage,
    contact: Option<Contact>,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    let from = message.from.clone();
    let message_id = message.id.clone();
    let input = extract_input(&message);
    let (message_type, content) = describe_input(&input);
    let actor = if from == state.config.advisor_phone {
        "advisor"
    } else {
        "customer"
    };

    tracing::info!(
        actor = %actor,
        phone = %mask_phone(&from),
        message_id = %message_id,
        message_type = %message_type,
        preview = %preview_text(&content),
        "received whatsapp message"
    );

    if let Err(err) = mark_as_read_if_supported(&state, &message_id).await {
        tracing::warn!(
            phone = %mask_phone(&from),
            message_id = %message_id,
            error = %err,
            "failed to mark inbound whatsapp message as read; continuing"
        );
    }

    if from == state.config.advisor_phone {
        process_advisor_input(state, input).await?;
    } else {
        let profile_name = contact
            .as_ref()
            .and_then(|contact| contact.profile.as_ref())
            .map(|profile| profile.name.clone());
        process_customer_input(state, from, profile_name, input).await?;
    }

    Ok(())
}

fn describe_input(input: &UserInput) -> (&'static str, String) {
    match input {
        UserInput::ButtonPress(id) => ("button", id.clone()),
        UserInput::TextMessage(body) => ("text", body.clone()),
        UserInput::ImageMessage(id) => ("image", id.clone()),
        UserInput::ListSelection(id) => ("list", id.clone()),
    }
}

fn to_lower_hex(bytes: &[u8]) -> String {
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        output.push_str(&format!("{byte:02x}"));
    }
    output
}
