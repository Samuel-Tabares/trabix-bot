use std::{
    error::Error,
    io::{Error as IoError, ErrorKind},
    path::Path,
};

use serde_json::json;

use crate::{
    bot::{
        inactivity::sync_customer_inactivity_timer,
        pricing::calcular_pedido,
        state_machine::{
            transition, transition_advisor, BotAction, ConversationContext, ConversationState,
            ImageAsset, TimerType, UserInput,
        },
        states::{advisor::parse_advisor_button_id, data_collect},
        timers::{
            cancel_timer, effective_duration_for_start_timer, expire_advisor_timer,
            expire_receipt_timer, expire_relay_timer, start_timer,
        },
    },
    db::{
        models::{Conversation, ConversationStateData},
        queries::{
            create_conversation, create_order, get_conversation, replace_order_items,
            reset_conversation, update_customer_data, update_last_message, update_order,
            update_order_delivery_cost, update_order_status, update_state,
        },
    },
    logging::{log_bot_action, mask_phone, summarize_action_kinds},
    messages::client_messages,
    simulator::{create_message, get_media, get_session_by_phone, NewSimulatorMessage},
    transport::{OutboundTransport, SIMULATOR_MENU_ASSET_PATH},
    AppState,
};

pub struct ExecutionOutcome {
    pub reset_requested: bool,
}

pub async fn process_customer_input(
    state: AppState,
    phone: String,
    profile_name: Option<String>,
    input: UserInput,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    let conversation = load_or_create_conversation(&state, &phone).await?;
    let (current_state, mut context) = rehydrate_client_conversation(&state, &conversation).await?;
    let seeded = seed_customer_data(&mut context, &phone, profile_name.as_deref());
    if seeded.seeded_phone || seeded.seeded_name {
        tracing::debug!(
            phone = %mask_phone(&phone),
            seeded_phone = seeded.seeded_phone,
            seeded_name = seeded.seeded_name,
            "seeded customer data from inbound metadata"
        );
    }

    let (new_state, mut actions) = transition(&current_state, &input, &mut context)?;
    let transition_resets_conversation = actions
        .iter()
        .any(|action| matches!(action, BotAction::ResetConversation { .. }));
    actions.extend(sync_customer_inactivity_timer(
        &new_state,
        &mut context,
        transition_resets_conversation,
    ));
    tracing::info!(
        actor = "customer",
        phone = %mask_phone(&phone),
        from_state = %current_state.as_storage_key(),
        to_state = %new_state.as_storage_key(),
        action_count = actions.len(),
        action_kinds = %summarize_action_kinds(&actions),
        reset_requested = transition_resets_conversation,
        "processed state transition"
    );

    update_customer_data(
        &state.pool,
        &phone,
        context.customer_name.as_deref(),
        context.customer_phone.as_deref(),
        context.delivery_address.as_deref(),
    )
    .await?;

    let session_phone = context.phone_number.clone();
    let execution = execute_actions(
        &state,
        conversation.id,
        &mut context,
        &actions,
        Some(session_phone.as_str()),
    )
    .await?;

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

pub async fn process_advisor_input(
    state: AppState,
    input: UserInput,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    let advisor_phone = state.config.advisor_phone.clone();
    let advisor_conversation = load_or_create_conversation(&state, &advisor_phone).await?;

    let target_phone = resolve_advisor_target_phone(&advisor_conversation.state_data.0, &input);
    let Some(target_phone) = target_phone else {
        tracing::info!(
            advisor_phone = %mask_phone(&advisor_phone),
            "advisor message arrived without an active target"
        );
        send_text(
            &state,
            &advisor_phone,
            "Primero usa un botón de un caso pendiente para indicar a qué cliente responder.",
            None,
        )
        .await?;
        update_last_message(&state.pool, &advisor_phone).await?;
        return Ok(());
    };

    let Some(client_conversation) = get_conversation(&state.pool, &target_phone).await? else {
        tracing::warn!(
            advisor_phone = %mask_phone(&advisor_phone),
            target_phone = %mask_phone(&target_phone),
            "advisor target conversation no longer exists"
        );
        clear_advisor_session(&state, &advisor_phone).await?;
        send_text(
            &state,
            &advisor_phone,
            "Ese caso ya no está disponible. Usa un botón de un caso pendiente.",
            None,
        )
        .await?;
        update_last_message(&state.pool, &advisor_phone).await?;
        return Ok(());
    };

    let (current_state, mut context) =
        rehydrate_client_conversation(&state, &client_conversation).await?;
    let (new_state, mut actions) = transition_advisor(&current_state, &input, &mut context)?;
    let transition_resets_conversation = actions
        .iter()
        .any(|action| matches!(action, BotAction::ResetConversation { .. }));
    actions.extend(sync_customer_inactivity_timer(
        &new_state,
        &mut context,
        transition_resets_conversation,
    ));
    tracing::info!(
        actor = "advisor",
        advisor_phone = %mask_phone(&advisor_phone),
        target_phone = %mask_phone(&target_phone),
        from_state = %current_state.as_storage_key(),
        to_state = %new_state.as_storage_key(),
        action_count = actions.len(),
        action_kinds = %summarize_action_kinds(&actions),
        reset_requested = transition_resets_conversation,
        "processed advisor transition"
    );

    update_customer_data(
        &state.pool,
        &target_phone,
        context.customer_name.as_deref(),
        context.customer_phone.as_deref(),
        context.delivery_address.as_deref(),
    )
    .await?;

    let execution = execute_actions(
        &state,
        client_conversation.id,
        &mut context,
        &actions,
        Some(target_phone.as_str()),
    )
    .await?;

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

pub async fn mark_as_read_if_supported(
    state: &AppState,
    message_id: &str,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    if let Some(client) = state.transport.production() {
        client.mark_as_read(message_id).await?;
    }

    Ok(())
}

pub async fn send_text(
    state: &AppState,
    to: &str,
    body: &str,
    session_phone: Option<&str>,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    send_via_transport(
        state,
        to,
        "text",
        Some(body.to_string()),
        json!({}),
        session_phone,
        None,
    )
    .await
}

pub async fn send_timer_actions(
    state: &AppState,
    actions: &[BotAction],
    session_phone: Option<&str>,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    for action in actions {
        log_bot_action(action);
        match action {
            BotAction::SendText { to, body } => {
                send_text(state, to, body, session_phone).await?;
            }
            BotAction::SendButtons { to, body, buttons } => {
                send_via_transport(
                    state,
                    to,
                    "buttons",
                    Some(body.clone()),
                    json!({ "buttons": buttons }),
                    session_phone,
                    None,
                )
                .await?;
            }
            BotAction::SendList {
                to,
                body,
                button_text,
                sections,
            } => {
                send_via_transport(
                    state,
                    to,
                    "list",
                    Some(body.clone()),
                    json!({
                        "button_text": button_text,
                        "sections": sections,
                    }),
                    session_phone,
                    None,
                )
                .await?;
            }
            BotAction::SendImage {
                to,
                media_id,
                caption,
            } => {
                send_image(state, to, media_id, caption.clone(), session_phone).await?;
            }
            BotAction::SendAssetImage { to, asset, caption } => {
                send_asset_image(state, to, asset.clone(), caption.clone(), session_phone).await?;
            }
            BotAction::NoOp => {}
            _ => {
                tracing::warn!("skipping unsupported timer action during resend");
            }
        }
    }

    Ok(())
}

pub async fn execute_actions(
    state: &AppState,
    conversation_id: i32,
    context: &mut ConversationContext,
    actions: &[BotAction],
    session_phone: Option<&str>,
) -> Result<ExecutionOutcome, Box<dyn Error + Send + Sync>> {
    let mut reset_requested = false;

    for action in actions {
        log_bot_action(action);
        match action {
            BotAction::SendText { to, body } => {
                send_text(state, to, body, session_phone).await?;
            }
            BotAction::SendButtons { to, body, buttons } => {
                send_via_transport(
                    state,
                    to,
                    "buttons",
                    Some(body.clone()),
                    json!({ "buttons": buttons }),
                    session_phone,
                    None,
                )
                .await?;
            }
            BotAction::SendList {
                to,
                body,
                button_text,
                sections,
            } => {
                send_via_transport(
                    state,
                    to,
                    "list",
                    Some(body.clone()),
                    json!({
                        "button_text": button_text,
                        "sections": sections,
                    }),
                    session_phone,
                    None,
                )
                .await?;
            }
            BotAction::SendImage {
                to,
                media_id,
                caption,
            } => {
                send_image(state, to, media_id, caption.clone(), session_phone).await?;
            }
            BotAction::SendAssetImage { to, asset, caption } => {
                send_asset_image(state, to, asset.clone(), caption.clone(), session_phone).await?;
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
                send_text(state, to, body, session_phone).await?;
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
                let effective_duration =
                    effective_duration_for_start_timer(state, &timer_type, *duration);
                start_timer(
                    state.timers.clone(),
                    (phone.clone(), timer_type.clone()),
                    effective_duration,
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
                            TimerType::ConversationAbandon => {
                                if let Err(err) = crate::bot::timers::expire_conversation_abandon(
                                    app_state, phone,
                                )
                                .await
                                {
                                    tracing::error!(
                                        error = %err,
                                        "failed to expire conversation inactivity timer"
                                    );
                                }
                            }
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
                tracing::info!(
                    phone = %mask_phone(&context.phone_number),
                    order_id = ?context.current_order_id,
                    status = %status,
                    "persisted order state"
                );
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
                tracing::info!(
                    phone = %mask_phone(&context.phone_number),
                    order_id = order_id,
                    delivery_cost = *delivery_cost,
                    total_final = *total_final,
                    status = %status,
                    "updated order delivery cost"
                );
            }
            BotAction::CancelCurrentOrder { order_id } => {
                update_order_status(&state.pool, *order_id, "cancelled").await?;
                tracing::info!(
                    phone = %mask_phone(&context.phone_number),
                    order_id = *order_id,
                    "cancelled current order"
                );
            }
            BotAction::SaveOrder { .. } => {
                tracing::debug!("save_order action not implemented");
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
                send_text(state, to, body, session_phone).await?;
            }
        }
    }

    Ok(ExecutionOutcome { reset_requested })
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

#[derive(Debug, Default, Clone, Copy)]
struct SeededCustomerData {
    seeded_phone: bool,
    seeded_name: bool,
}

fn seed_customer_data(
    context: &mut ConversationContext,
    phone: &str,
    profile_name: Option<&str>,
) -> SeededCustomerData {
    let mut seeded = SeededCustomerData::default();
    if context.customer_phone.is_none() {
        context.customer_phone = Some(phone.to_string());
        seeded.seeded_phone = true;
    }

    if context.customer_name.is_some() {
        return seeded;
    }

    let Some(profile_name) = profile_name else {
        return seeded;
    };

    if let Ok(name) = data_collect::validate_name(profile_name) {
        context.customer_name = Some(name);
        seeded.seeded_name = true;
    }

    seeded
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

async fn bind_advisor_session(
    state: &AppState,
    advisor_phone: &str,
    target_phone: Option<String>,
) -> Result<(), sqlx::Error> {
    let conversation = load_or_create_conversation(state, advisor_phone).await?;
    let mut state_data = conversation.state_data.0;
    state_data.advisor_target_phone = target_phone;
    update_state(&state.pool, advisor_phone, &conversation.state, &state_data).await?;
    tracing::info!(
        advisor_phone = %mask_phone(advisor_phone),
        target_phone = %state_data
            .advisor_target_phone
            .as_deref()
            .map(mask_phone)
            .unwrap_or_else(|| "<none>".to_string()),
        "updated advisor session binding"
    );
    Ok(())
}

pub async fn clear_advisor_session(
    state: &AppState,
    advisor_phone: &str,
) -> Result<(), sqlx::Error> {
    if let Some(conversation) = get_conversation(&state.pool, advisor_phone).await? {
        let mut state_data = conversation.state_data.0;
        state_data.advisor_target_phone = None;
        update_state(&state.pool, advisor_phone, &conversation.state, &state_data).await?;
        tracing::info!(
            advisor_phone = %mask_phone(advisor_phone),
            "cleared advisor session binding"
        );
    }

    Ok(())
}

async fn send_via_transport(
    state: &AppState,
    to: &str,
    message_kind: &str,
    body: Option<String>,
    payload: serde_json::Value,
    session_phone: Option<&str>,
    file_path: Option<&Path>,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    match &state.transport {
        OutboundTransport::Production(client) => match message_kind {
            "text" => {
                client
                    .send_text(to, body.as_deref().unwrap_or_default())
                    .await?
            }
            "buttons" => {
                let buttons = serde_json::from_value(payload["buttons"].clone())?;
                client
                    .send_buttons(to, body.as_deref().unwrap_or_default(), buttons)
                    .await?;
            }
            "list" => {
                let sections = serde_json::from_value(payload["sections"].clone())?;
                let button_text = payload["button_text"].as_str().unwrap_or_default();
                client
                    .send_list(
                        to,
                        body.as_deref().unwrap_or_default(),
                        button_text,
                        sections,
                    )
                    .await?;
            }
            "image" => {
                let media_id = payload["media_id"].as_str().unwrap_or_default();
                let caption = payload["caption"].as_str();
                client.send_image(to, media_id, caption).await?;
            }
            _ => {}
        },
        OutboundTransport::Simulator => {
            let session_id = resolve_session_id_for_send(state, to, session_phone).await?;
            let audience = if to == state.config.advisor_phone {
                "advisor"
            } else {
                "customer"
            };
            let mut final_payload = payload;
            if let Some(path) = file_path {
                final_payload["url"] = json!(path.to_string_lossy());
            }
            create_message(
                &state.pool,
                NewSimulatorMessage {
                    session_id,
                    actor: "bot".to_string(),
                    audience: audience.to_string(),
                    message_kind: message_kind.to_string(),
                    body,
                    payload: final_payload,
                },
            )
            .await?;
        }
    }

    Ok(())
}

async fn send_image(
    state: &AppState,
    to: &str,
    media_id: &str,
    caption: Option<String>,
    session_phone: Option<&str>,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    match &state.transport {
        OutboundTransport::Production(_) => {
            let payload_caption = caption.clone();
            send_via_transport(
                state,
                to,
                "image",
                caption,
                json!({ "media_id": media_id, "caption": payload_caption }),
                session_phone,
                None,
            )
            .await
        }
        OutboundTransport::Simulator => {
            let file_path = get_media(&state.pool, media_id)
                .await?
                .map(|media| media.file_path);
            send_via_transport(
                state,
                to,
                "image",
                caption.clone(),
                json!({
                    "media_id": media_id,
                    "caption": caption,
                    "source": if file_path.is_some() { "simulator_media" } else { "external_media_id" },
                    "media_url": format!("/simulator/api/media/{media_id}"),
                }),
                session_phone,
                file_path.as_deref().map(Path::new),
            )
            .await
        }
    }
}

async fn send_asset_image(
    state: &AppState,
    to: &str,
    asset: ImageAsset,
    caption: Option<String>,
    session_phone: Option<&str>,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    match (&state.transport, asset) {
        (OutboundTransport::Production(_), ImageAsset::Menu) => {
            let media_id = &state.config.production().menu_image_media_id;
            send_via_transport(
                state,
                to,
                "image",
                caption.clone(),
                json!({ "media_id": media_id, "caption": caption }),
                session_phone,
                None,
            )
            .await
        }
        (OutboundTransport::Simulator, ImageAsset::Menu) => {
            send_via_transport(
                state,
                to,
                "image",
                caption.clone(),
                json!({
                    "asset": "menu",
                    "caption": caption,
                    "media_url": "/simulator/api/menu-asset",
                }),
                session_phone,
                Some(Path::new(SIMULATOR_MENU_ASSET_PATH)),
            )
            .await
        }
    }
}

async fn resolve_session_id_for_send(
    state: &AppState,
    to: &str,
    session_phone: Option<&str>,
) -> Result<Option<i32>, sqlx::Error> {
    let phone = if to == state.config.advisor_phone {
        session_phone
    } else {
        Some(to)
    };
    let Some(phone) = phone else {
        return Ok(None);
    };
    Ok(get_session_by_phone(&state.pool, phone)
        .await?
        .map(|session| session.id))
}

async fn upsert_order_from_context(
    pool: &sqlx::PgPool,
    conversation_id: i32,
    context: &mut ConversationContext,
    status: &str,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    let delivery_type = required_field(&context.delivery_type, "delivery_type")?;
    let payment_method = context.payment_method.as_deref().unwrap_or("pending");
    let schedule_values = schedule_values_for_persistence(context);
    let pedido = calcular_pedido(&context.items);
    let total_estimated = i32::try_from(pedido.total_estimado)
        .map_err(|_| IoError::new(ErrorKind::InvalidData, "total_estimated out of range"))?;
    let receipt_media_id = context.receipt_media_id.as_deref();
    let referral_code = context.referral_code.as_deref();
    let referral_discount_total = context.referral_discount_total;
    let ambassador_commission_total = context.ambassador_commission_total;

    let order = match context.current_order_id {
        Some(order_id) => {
            update_order(
                pool,
                order_id,
                delivery_type,
                schedule_values.typed_date,
                schedule_values.typed_time,
                schedule_values.raw_date.as_deref(),
                schedule_values.raw_time.as_deref(),
                payment_method,
                receipt_media_id,
                referral_code,
                referral_discount_total,
                ambassador_commission_total,
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
                schedule_values.typed_date,
                schedule_values.typed_time,
                schedule_values.raw_date.as_deref(),
                schedule_values.raw_time.as_deref(),
                payment_method,
                receipt_media_id,
                referral_code,
                referral_discount_total,
                ambassador_commission_total,
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

#[derive(Debug, Clone, PartialEq, Eq)]
struct PersistedScheduleValues {
    typed_date: Option<chrono::NaiveDate>,
    typed_time: Option<chrono::NaiveTime>,
    raw_date: Option<String>,
    raw_time: Option<String>,
}

fn schedule_values_for_persistence(context: &ConversationContext) -> PersistedScheduleValues {
    if context.delivery_type.as_deref() != Some("scheduled") {
        return PersistedScheduleValues {
            typed_date: None,
            typed_time: None,
            raw_date: None,
            raw_time: None,
        };
    }

    PersistedScheduleValues {
        typed_date: context
            .scheduled_date
            .as_deref()
            .and_then(|value| chrono::NaiveDate::parse_from_str(value, "%Y-%m-%d").ok()),
        typed_time: context
            .scheduled_time
            .as_deref()
            .and_then(|value| chrono::NaiveTime::parse_from_str(value, "%H:%M").ok()),
        raw_date: context.scheduled_date.clone(),
        raw_time: context.scheduled_time.clone(),
    }
}
