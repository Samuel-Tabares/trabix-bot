use std::{
    error::Error,
    fmt,
    io::{Error as IoError, ErrorKind},
};

use axum::{
    body::Bytes,
    extract::State,
    http::{HeaderMap, HeaderValue, StatusCode},
    response::IntoResponse,
};
use hmac::{Hmac, Mac};
use sha2::Sha256;

use crate::{
    bot::{
        pricing::calcular_pedido,
        state_machine::{
            extract_input, transition, transition_advisor, BotAction, ConversationContext,
            ConversationState, ImageAsset, TimerType, UserInput,
        },
        states::advisor::parse_advisor_button_id,
        timers::{cancel_timer, expire_advisor_timer, expire_receipt_timer, expire_relay_timer, start_timer},
    },
    db::{
        models::{Conversation, ConversationStateData},
        queries::{
            create_conversation, create_order, get_conversation, replace_order_items,
            reset_conversation, update_customer_data, update_last_message, update_order,
            update_order_delivery_cost, update_order_status, update_state,
        },
    },
    messages::client_messages,
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
        handle_advisor_message(state, input).await?;
    } else {
        handle_client_message(state, from, input).await?;
    }

    Ok(())
}

async fn handle_client_message(
    state: AppState,
    phone: String,
    input: UserInput,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    let conversation = load_or_create_conversation(&state, &phone).await?;
    let (current_state, mut context) = rehydrate_client_conversation(&state, &conversation).await?;

    let (new_state, actions) = transition(&current_state, &input, &mut context)?;

    update_customer_data(
        &state.pool,
        &phone,
        context.customer_name.as_deref(),
        context.customer_phone.as_deref(),
        context.delivery_address.as_deref(),
    )
    .await?;

    let execution = execute_actions(&state, conversation.id, &mut context, &actions).await?;

    if !execution.reset_requested {
        update_state(
            &state.pool,
            &phone,
            new_state.as_storage_key(),
            &context.to_state_data(),
        )
        .await?;
    }

    update_last_message(&state.pool, &phone).await?;
    Ok(())
}

async fn handle_advisor_message(
    state: AppState,
    input: UserInput,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    let advisor_phone = state.config.advisor_phone.clone();
    let advisor_conversation = load_or_create_conversation(&state, &advisor_phone).await?;

    let target_phone = resolve_advisor_target_phone(&advisor_conversation.state_data.0, &input);
    let Some(target_phone) = target_phone else {
        state
            .wa_client
            .send_text(
                &advisor_phone,
                "Primero usa un botón de un caso pendiente para indicar a qué cliente responder.",
            )
            .await?;
        update_last_message(&state.pool, &advisor_phone).await?;
        return Ok(());
    };

    let Some(client_conversation) = get_conversation(&state.pool, &target_phone).await? else {
        clear_advisor_session(&state, &advisor_phone).await?;
        state
            .wa_client
            .send_text(
                &advisor_phone,
                "Ese caso ya no está disponible. Usa un botón de un caso pendiente.",
            )
            .await?;
        update_last_message(&state.pool, &advisor_phone).await?;
        return Ok(());
    };

    let (current_state, mut context) =
        rehydrate_client_conversation(&state, &client_conversation).await?;
    let (new_state, actions) = transition_advisor(&current_state, &input, &mut context)?;

    update_customer_data(
        &state.pool,
        &target_phone,
        context.customer_name.as_deref(),
        context.customer_phone.as_deref(),
        context.delivery_address.as_deref(),
    )
    .await?;

    let execution = execute_actions(&state, client_conversation.id, &mut context, &actions).await?;

    if !execution.reset_requested {
        update_state(
            &state.pool,
            &target_phone,
            new_state.as_storage_key(),
            &context.to_state_data(),
        )
        .await?;
    }

    update_last_message(&state.pool, &advisor_phone).await?;
    update_last_message(&state.pool, &target_phone).await?;
    Ok(())
}

async fn load_or_create_conversation(
    state: &AppState,
    phone: &str,
) -> Result<Conversation, sqlx::Error> {
    match get_conversation(&state.pool, phone).await? {
        Some(conversation) => Ok(conversation),
        None => create_conversation(&state.pool, phone).await,
    }
}

async fn rehydrate_client_conversation(
    state: &AppState,
    conversation: &Conversation,
) -> Result<(ConversationState, ConversationContext), Box<dyn Error + Send + Sync>> {
    let mut context = ConversationContext::from_persisted(
        conversation.phone_number.clone(),
        state.config.advisor_phone.clone(),
        conversation.customer_name.clone(),
        conversation.customer_phone.clone(),
        conversation.delivery_address.clone(),
        &conversation.state_data.0,
    );

    let current_state = match ConversationState::from_storage_key(&conversation.state, &context) {
        Ok(state) => state,
        Err(err) => {
            tracing::error!(
                phone = %conversation.phone_number,
                error = %err,
                "failed to rehydrate state, resetting conversation"
            );
            reset_conversation(&state.pool, &conversation.phone_number).await?;
            context = ConversationContext::from_persisted(
                conversation.phone_number.clone(),
                state.config.advisor_phone.clone(),
                conversation.customer_name.clone(),
                conversation.customer_phone.clone(),
                conversation.delivery_address.clone(),
                &ConversationStateData::default(),
            );
            ConversationState::MainMenu
        }
    };

    Ok((current_state, context))
}

fn resolve_advisor_target_phone(
    state_data: &ConversationStateData,
    input: &UserInput,
) -> Option<String> {
    match input {
        UserInput::ButtonPress(id) | UserInput::ListSelection(id) => parse_advisor_button_id(id)
            .map(|(_, phone)| phone)
            .or_else(|| state_data.advisor_target_phone.clone()),
        UserInput::TextMessage(_) => state_data.advisor_target_phone.clone(),
        UserInput::ImageMessage(_) => state_data.advisor_target_phone.clone(),
    }
}

struct ExecutionOutcome {
    reset_requested: bool,
}

async fn execute_actions(
    state: &AppState,
    conversation_id: i32,
    context: &mut ConversationContext,
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
            BotAction::SendAssetImage { to, asset, caption } => {
                let media_id = match *asset {
                    ImageAsset::Menu => &state.config.menu_image_media_id,
                };

                state
                    .wa_client
                    .send_image(to, media_id, caption.as_deref())
                    .await?;
            }
            BotAction::SendTransferInstructions { to } => {
                let configured = client_messages().checkout.transfer_payment_text.trim();
                let body = if configured.is_empty() {
                    state
                        .config
                        .transfer_payment_text
                        .as_deref()
                        .unwrap_or_default()
                } else {
                    configured
                };
                state
                    .wa_client
                    .send_text(to, body)
                    .await?;
            }
            BotAction::ResetConversation { phone } => {
                reset_conversation(&state.pool, phone).await?;
                reset_requested = true;
            }
            BotAction::NoOp => {}
            BotAction::StartTimer {
                timer_type,
                phone,
                duration,
            } => {
                let timer_type = timer_type.clone();
                let phone = phone.clone();
                let app_state = state.clone();
                start_timer(
                    state.timers.clone(),
                    (phone.clone(), timer_type.clone()),
                    *duration,
                    move || async move {
                        match timer_type {
                            TimerType::ReceiptUpload => {
                                if let Err(err) = expire_receipt_timer(app_state, phone).await {
                                    tracing::error!(error = %err, "failed to expire receipt timer");
                                }
                            }
                            TimerType::AdvisorResponse => {
                                if let Err(err) = expire_advisor_timer(app_state, phone).await {
                                    tracing::error!(error = %err, "failed to expire advisor timer");
                                }
                            }
                            TimerType::RelayInactivity => {
                                if let Err(err) = expire_relay_timer(app_state, phone).await {
                                    tracing::error!(error = %err, "failed to expire relay timer");
                                }
                            }
                            TimerType::ConversationAbandon => {}
                        }
                    },
                )
                .await;
            }
            BotAction::CancelTimer { timer_type, phone } => {
                cancel_timer(state.timers.clone(), &(phone.clone(), timer_type.clone())).await;
            }
            BotAction::UpsertDraftOrder { status } | BotAction::FinalizeCurrentOrder { status } => {
                upsert_order_from_context(&state.pool, conversation_id, context, status).await?;
            }
            BotAction::UpdateCurrentOrderDeliveryCost {
                delivery_cost,
                total_final,
                status,
            } => {
                let order_id = context.current_order_id.ok_or_else(|| {
                    IoError::new(ErrorKind::InvalidData, "missing current_order_id")
                })?;
                update_order_delivery_cost(&state.pool, order_id, *delivery_cost, *total_final)
                    .await?;
                update_order_status(&state.pool, order_id, status).await?;
            }
            BotAction::CancelCurrentOrder { order_id } => {
                update_order_status(&state.pool, *order_id, "cancelled").await?;
            }
            BotAction::SaveOrder { .. } => {
                tracing::info!("action not implemented yet");
            }
            BotAction::BindAdvisorSession {
                advisor_phone,
                target_phone,
            } => {
                bind_advisor_session(state, advisor_phone, Some(target_phone.clone())).await?;
            }
            BotAction::ClearAdvisorSession { advisor_phone } => {
                bind_advisor_session(state, advisor_phone, None).await?;
            }
            BotAction::RelayMessage { to, body, .. } => {
                state.wa_client.send_text(to, body).await?;
            }
        }
    }

    Ok(ExecutionOutcome { reset_requested })
}

async fn bind_advisor_session(
    state: &AppState,
    advisor_phone: &str,
    target_phone: Option<String>,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    let conversation = load_or_create_conversation(state, advisor_phone).await?;
    let mut state_data = conversation.state_data.0;
    state_data.advisor_target_phone = target_phone;
    update_state(&state.pool, advisor_phone, &conversation.state, &state_data).await?;
    Ok(())
}

async fn clear_advisor_session(
    state: &AppState,
    advisor_phone: &str,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    if let Some(conversation) = get_conversation(&state.pool, advisor_phone).await? {
        let mut state_data = conversation.state_data.0;
        state_data.advisor_target_phone = None;
        update_state(&state.pool, advisor_phone, &conversation.state, &state_data).await?;
    }

    Ok(())
}

async fn upsert_order_from_context(
    pool: &sqlx::PgPool,
    conversation_id: i32,
    context: &mut ConversationContext,
    status: &str,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    let delivery_type = required_field(&context.delivery_type, "delivery_type")?;
    let payment_method = required_field(&context.payment_method, "payment_method")?;
    let scheduled_date = parse_optional_date(context.scheduled_date.as_deref());
    let scheduled_time = parse_optional_time(context.scheduled_time.as_deref());
    let pedido = calcular_pedido(&context.items);
    let total_estimated = i32::try_from(pedido.total_estimado)
        .map_err(|_| IoError::new(ErrorKind::InvalidData, "total_estimated out of range"))?;
    let receipt_media_id = context.receipt_media_id.as_deref();

    let order = match context.current_order_id {
        Some(order_id) => {
            update_order(
                pool,
                order_id,
                delivery_type,
                scheduled_date,
                scheduled_time,
                payment_method,
                receipt_media_id,
                total_estimated,
                status,
            )
            .await?
        }
        None => {
            let order = create_order(
                pool,
                conversation_id,
                delivery_type,
                scheduled_date,
                scheduled_time,
                payment_method,
                receipt_media_id,
                total_estimated,
            )
            .await?;
            update_order_status(pool, order.id, status).await?;
            context.current_order_id = Some(order.id);
            order
        }
    };

    let persisted_items = pedido
        .items_detalle
        .iter()
        .flat_map(|item| item.persistence_lines.iter())
        .map(|line| {
            Ok((
                line.flavor.clone(),
                line.has_liquor,
                i32::try_from(line.quantity)
                    .map_err(|_| IoError::new(ErrorKind::InvalidData, "quantity out of range"))?,
                i32::try_from(line.unit_price)
                    .map_err(|_| IoError::new(ErrorKind::InvalidData, "unit_price out of range"))?,
                i32::try_from(line.subtotal)
                    .map_err(|_| IoError::new(ErrorKind::InvalidData, "subtotal out of range"))?,
            ))
        })
        .collect::<Result<Vec<_>, IoError>>()?;

    replace_order_items(pool, order.id, &persisted_items).await?;
    Ok(())
}

fn required_field<'a>(
    value: &'a Option<String>,
    field: &'static str,
) -> Result<&'a str, Box<dyn Error + Send + Sync>> {
    value.as_deref().ok_or_else(|| {
        IoError::new(
            ErrorKind::InvalidData,
            format!("missing required field {field}"),
        )
        .into()
    })
}

fn parse_optional_date(value: Option<&str>) -> Option<chrono::NaiveDate> {
    let raw = value?;

    [
        "%Y-%m-%d",
        "%d/%m/%Y",
        "%d-%m-%Y",
        "%Y/%m/%d",
    ]
    .iter()
    .find_map(|format| chrono::NaiveDate::parse_from_str(raw, format).ok())
}

fn parse_optional_time(value: Option<&str>) -> Option<chrono::NaiveTime> {
    let raw = value?;

    [
        "%H:%M",
        "%H:%M:%S",
        "%I:%M%P",
        "%I:%M %P",
        "%I%P",
        "%I %P",
    ]
    .iter()
    .find_map(|format| chrono::NaiveTime::parse_from_str(raw, format).ok())
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
