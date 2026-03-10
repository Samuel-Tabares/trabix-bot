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
    bot::state_machine::{extract_input, transition, BotAction, ConversationContext, ConversationState, UserInput},
    db::{
        models::ConversationStateData,
        queries::{
            create_conversation, get_conversation, reset_conversation, update_customer_data,
            update_last_message, update_state,
        },
    },
    whatsapp::types::WebhookPayload,
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
    let input = extract_input(message);
    let (message_type, content) = describe_input(&input);

    tracing::info!(
        phone = %from,
        message_type = %message_type,
        content = %content,
        "received whatsapp message"
    );

    state.wa_client.mark_as_read(&message_id).await?;

    if from == state.config.advisor_phone {
        tracing::info!("advisor message received but advisor flow is not active in this phase");
        return Ok(());
    }

    let conversation = match get_conversation(&state.pool, &from).await? {
        Some(conversation) => conversation,
        None => create_conversation(&state.pool, &from).await?,
    };

    let mut context = ConversationContext::from_persisted(
        conversation.phone_number.clone(),
        conversation.customer_name.clone(),
        conversation.customer_phone.clone(),
        conversation.delivery_address.clone(),
        &conversation.state_data.0,
    );

    let current_state = match ConversationState::from_storage_key(&conversation.state, &context) {
        Ok(state) => state,
        Err(err) => {
            tracing::error!(phone = %from, error = %err, "failed to rehydrate state, resetting conversation");
            reset_conversation(&state.pool, &from).await?;
            context = ConversationContext::from_persisted(
                from.clone(),
                conversation.customer_name.clone(),
                conversation.customer_phone.clone(),
                conversation.delivery_address.clone(),
                &ConversationStateData::default(),
            );
            ConversationState::MainMenu
        }
    };

    let (new_state, actions) = transition(&current_state, &input, &mut context)?;

    update_customer_data(
        &state.pool,
        &from,
        context.customer_name.as_deref(),
        context.customer_phone.as_deref(),
        context.delivery_address.as_deref(),
    )
    .await?;

    let execution = execute_actions(&state, &actions).await?;

    if !execution.reset_requested {
        update_state(
            &state.pool,
            &from,
            new_state.as_storage_key(),
            &context.to_state_data(),
        )
        .await?;
    }

    update_last_message(&state.pool, &from).await?;

    Ok(())
}

struct ExecutionOutcome {
    reset_requested: bool,
}

async fn execute_actions(
    state: &AppState,
    actions: &[BotAction],
) -> Result<ExecutionOutcome, Box<dyn Error + Send + Sync>> {
    let mut reset_requested = false;

    for action in actions {
        match action {
            BotAction::SendText { to, body } => {
                state.wa_client.send_text(to, body).await?;
            }
            BotAction::SendButtons { to, body, buttons } => {
                state
                    .wa_client
                    .send_buttons(to, body, buttons.clone())
                    .await?;
            }
            BotAction::SendList {
                to,
                body,
                button_text,
                sections,
            } => {
                state
                    .wa_client
                    .send_list(to, body, button_text, sections.clone())
                    .await?;
            }
            BotAction::SendImage {
                to,
                media_id,
                caption,
            } => {
                state
                    .wa_client
                    .send_image(to, media_id, caption.as_deref())
                    .await?;
            }
            BotAction::ResetConversation { phone } => {
                reset_conversation(&state.pool, phone).await?;
                reset_requested = true;
            }
            BotAction::NoOp => {}
            BotAction::StartTimer { timer_type, phone, .. } => {
                tracing::info!(?timer_type, phone = %phone, "action not implemented yet");
            }
            BotAction::CancelTimer { timer_type, phone } => {
                tracing::info!(?timer_type, phone = %phone, "action not implemented yet");
            }
            BotAction::SaveOrder { .. } => {
                tracing::info!("action not implemented yet");
            }
            BotAction::RelayMessage { from, to, .. } => {
                tracing::info!(from = %from, to = %to, "action not implemented yet");
            }
        }
    }

    Ok(ExecutionOutcome { reset_requested })
}

fn describe_input(input: &UserInput) -> (&'static str, String) {
    match input {
        UserInput::TextMessage(body) => ("text", body.clone()),
        UserInput::ButtonPress(id) => ("button_reply", id.clone()),
        UserInput::ListSelection(id) => ("list_reply", id.clone()),
        UserInput::ImageMessage(media_id) => ("image", media_id.clone()),
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
    use super::{describe_input, verify_signature, SignatureError};
    use crate::bot::state_machine::extract_input;
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

        let input = extract_input(payload.first_message().expect("message"));
        let (_, content) = describe_input(&input);

        assert_eq!(content, "Hola");
    }
}
