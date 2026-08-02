use std::{
    error::Error,
    io::{Error as IoError, ErrorKind},
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
            create_conversation, create_order, create_or_update_customer, create_or_update_referral_analytics, get_conversation, replace_order_items,
            reset_conversation, update_customer_data, update_customer_totals, update_last_message, update_order,
            update_order_delivery_cost, update_order_status, update_state,
        },
    },
    logging::{log_bot_action, mask_phone, summarize_action_kinds},
    messages::client_messages,
    AppState,
};

pub struct ExecutionOutcome {
    pub reset_requested: bool,
}

pub async fn process_customer_input(
    state: AppState,
    phone: String,
    profile_name: Option<String>,
    username: Option<String>,
    ctwa_clid: Option<String>,
    input: UserInput,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    let _case_lock = crate::lock_conversation(&state.conversation_locks, &phone).await;

    log_inbound_event(&state, &phone, CHANNEL_CLIENT, ACTOR_CLIENT, &input).await;

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

    // Primer contacto en motor agente: se responde con un saludo de bienvenida
    // FIJO (sin gastar una llamada al LLM). De ahí en adelante todo lo maneja
    // el LLM (ver docs/canary-fixes-2026-07-19.md item 3).
    if should_use_agent(&current_state) && !context.has_greeted {
        context.has_greeted = true;
        send_text(&state, &phone, &client_messages().agent.welcome).await?;
        log_outbound_text(&state, &phone, &phone, &client_messages().agent.welcome).await;
        update_state(
            &state.pool,
            &phone,
            current_state.as_storage_key(),
            &context.to_state_data(),
        )
        .await?;
        update_last_message(&state.pool, &phone).await?;
        return Ok(());
    }

    let (new_state, mut actions) = if should_use_agent(&current_state) {
        match crate::ai::agent::run_customer_turn(&state, &mut context, &current_state, &input)
            .await
        {
            Ok(result) => result,
            Err(err) => {
                return degrade_agent_failure(
                    &state,
                    &phone,
                    &context,
                    &current_state,
                    &input,
                    "customer",
                    err,
                )
                .await;
            }
        }
    } else {
        transition(&current_state, &input, &mut context)?
    };
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

    // Meta vs personalizado (hallazgo C): el nombre/celular manual solo se
    // guarda como "_manual" cuando el cliente puso uno DISTINTO al de Meta; así
    // el registro conserva el dato real de Meta aparte del personalizado.
    let manual_name = manual_override(context.customer_name.as_deref(), context.meta_customer_name.as_deref());
    let manual_phone = manual_override(context.customer_phone.as_deref(), context.meta_customer_phone.as_deref());
    create_or_update_customer(
        &state.pool,
        &phone,
        manual_phone,
        context.meta_customer_name.as_deref(),
        manual_name,
        username.as_deref(),
        context.delivery_address.as_deref(),
        ctwa_clid.as_deref(),
    )
    .await?;

    let session_phone = context.phone_number.clone();
    let execution = execute_actions(
        &state,
        conversation.id,
        &mut context,
        &actions,
        Some(session_phone.as_str()),
        Some(&new_state),
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

/// Advisor sends this daily to keep the 24h WhatsApp service window open
/// before it lapses; the bot must stay silent so it isn't mistaken for a reply.
const WINDOW_KEEPALIVE_PING: &str = "✅";

fn is_window_keepalive_ping(input: &UserInput) -> bool {
    matches!(input, UserInput::TextMessage(text) if text.trim() == WINDOW_KEEPALIVE_PING)
}

// --- Conversation trace (message_events) ---------------------------------
// Best-effort append-only log of every message so the CRM can replay the full
// flow: the customer<->bot lane ("client") and the internal bot<->advisor lane
// ("advisor"). Logging failures never block delivery.

const CHANNEL_CLIENT: &str = "client";
const CHANNEL_ADVISOR: &str = "advisor";
const ACTOR_CLIENT: &str = "client";
const ACTOR_BOT: &str = "bot";
const ACTOR_ADVISOR: &str = "advisor";

fn outbound_recipient(action: &BotAction) -> Option<&str> {
    match action {
        BotAction::SendText { to, .. }
        | BotAction::SendButtons { to, .. }
        | BotAction::SendList { to, .. }
        | BotAction::SendImage { to, .. }
        | BotAction::SendAssetImage { to, .. }
        | BotAction::SendTransferInstructions { to } => Some(to),
        _ => None,
    }
}

type OutboundDescription = (&'static str, Option<String>, Option<serde_json::Value>);

fn describe_outbound_action(action: &BotAction) -> Option<OutboundDescription> {
    match action {
        BotAction::SendText { body, .. } => Some(("text", Some(body.clone()), None)),
        BotAction::SendTransferInstructions { .. } => Some((
            "text",
            Some("[instrucciones de transferencia]".to_string()),
            None,
        )),
        BotAction::SendButtons { body, buttons, .. } => Some((
            "buttons",
            Some(body.clone()),
            Some(json!({ "buttons": buttons })),
        )),
        BotAction::SendList {
            body,
            button_text,
            sections,
            ..
        } => Some((
            "list",
            Some(body.clone()),
            Some(json!({ "button_text": button_text, "sections": sections })),
        )),
        BotAction::SendImage {
            media_id, caption, ..
        } => Some((
            "image",
            caption.clone(),
            Some(json!({ "media_id": media_id })),
        )),
        BotAction::SendAssetImage { asset, caption, .. } => Some((
            "image",
            caption.clone(),
            Some(json!({ "asset": format!("{asset:?}") })),
        )),
        _ => None,
    }
}

fn channel_for_recipient(to: &str, advisor_phone: &str) -> &'static str {
    if to == advisor_phone {
        CHANNEL_ADVISOR
    } else {
        CHANNEL_CLIENT
    }
}

fn describe_inbound_input(input: &UserInput) -> OutboundDescription {
    match input {
        UserInput::TextMessage(text) => ("text", Some(text.clone()), None),
        UserInput::ButtonPress(id) => (
            "button_reply",
            Some(id.clone()),
            Some(json!({ "button_id": id })),
        ),
        UserInput::ListSelection(id) => (
            "list_reply",
            Some(id.clone()),
            Some(json!({ "list_id": id })),
        ),
        UserInput::ImageMessage(media_id) => {
            ("image", None, Some(json!({ "media_id": media_id })))
        }
    }
}

async fn log_outbound_event(state: &AppState, case_phone: &str, action: &BotAction) {
    let Some(to) = outbound_recipient(action) else {
        return;
    };
    let Some((content_type, body, payload)) = describe_outbound_action(action) else {
        return;
    };
    let channel = channel_for_recipient(to, &state.config.advisor_phone);
    if let Err(err) = crate::db::queries::record_message_event(
        &state.pool,
        case_phone,
        channel,
        ACTOR_BOT,
        content_type,
        body.as_deref(),
        payload,
        None,
    )
    .await
    {
        tracing::warn!(error = %err, "failed to record outbound message event");
    }
}

async fn log_inbound_event(
    state: &AppState,
    case_phone: &str,
    channel: &str,
    actor: &str,
    input: &UserInput,
) {
    let (content_type, body, payload) = describe_inbound_input(input);
    if let Err(err) = crate::db::queries::record_message_event(
        &state.pool,
        case_phone,
        channel,
        actor,
        content_type,
        body.as_deref(),
        payload,
        None,
    )
    .await
    {
        tracing::warn!(error = %err, "failed to record inbound message event");
    }
}

async fn log_outbound_text(state: &AppState, case_phone: &str, to: &str, body: &str) {
    let channel = channel_for_recipient(to, &state.config.advisor_phone);
    if let Err(err) = crate::db::queries::record_message_event(
        &state.pool,
        case_phone,
        channel,
        ACTOR_BOT,
        "text",
        Some(body),
        None,
        None,
    )
    .await
    {
        tracing::warn!(error = %err, "failed to record outbound text event");
    }
}

pub async fn process_advisor_input(
    state: AppState,
    input: UserInput,
    reply_to_message_id: Option<String>,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    if is_window_keepalive_ping(&input) {
        return Ok(());
    }

    let advisor_phone = state.config.advisor_phone.clone();
    let advisor_conversation = load_or_create_conversation(&state, &advisor_phone).await?;

    let target_phone = resolve_advisor_target_phone(
        &advisor_conversation.state_data.0,
        &input,
        reply_to_message_id.as_deref(),
    );
    let Some(target_phone) = target_phone else {
        tracing::info!(
            advisor_phone = %mask_phone(&advisor_phone),
            "advisor message arrived without an active target"
        );
        send_text(
            &state,
            &advisor_phone,
            "Primero usa un botón de un caso pendiente para indicar a qué cliente responder.",
        )
        .await?;
        update_last_message(&state.pool, &advisor_phone).await?;
        return Ok(());
    };

    process_advisor_turn_for_case(&state, &target_phone, input, true).await
}

/// El turno del asesor sobre un caso concreto, sin la parte de "¿a qué cliente
/// le está respondiendo?".
///
/// Existe separado porque hay dos formas de que llegue un mensaje del asesor y
/// solo difieren en cómo se resuelve el caso:
///
/// - **WhatsApp** (`process_advisor_input`): el caso sale del botón que apretó
///   o del mensaje al que le hizo reply.
/// - **`crm-app`** (`POST /internal/advisor/reply`): la consola ya sabe en qué
///   conversación está parada y manda el `case_phone` explícito.
///
/// En los dos casos el mensaje entra al **motor de agente**, no directo al
/// cliente. Eso importa: `confirm_advisor_availability` y
/// `set_manual_delivery_cost` son pasos bloqueantes del flujo de pedido que solo
/// existen si el agente interpreta la respuesta del asesor. Mandarle texto
/// crudo al cliente (`POST /internal/advisor/send`) se salta el agente y deja el
/// pedido colgado esperando una respuesta que nunca llega.
pub async fn process_advisor_turn_for_case(
    state: &AppState,
    target_phone: &str,
    input: UserInput,
    from_whatsapp: bool,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    let state = state.clone();
    let advisor_phone = state.config.advisor_phone.clone();
    let target_phone = target_phone.to_string();

    let _case_lock = crate::lock_conversation(&state.conversation_locks, &target_phone).await;

    log_inbound_event(&state, &target_phone, CHANNEL_ADVISOR, ACTOR_ADVISOR, &input).await;

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
        )
        .await?;
        update_last_message(&state.pool, &advisor_phone).await?;
        return Ok(());
    };

    let (current_state, mut context) =
        rehydrate_client_conversation(&state, &client_conversation).await?;
    let (new_state, mut actions) = if should_use_agent(&current_state) {
        match crate::ai::agent::run_advisor_turn(&state, &mut context, &current_state, &input)
            .await
        {
            Ok(result) => result,
            Err(err) => {
                return degrade_agent_failure(
                    &state,
                    &target_phone,
                    &context,
                    &current_state,
                    &input,
                    "advisor",
                    err,
                )
                .await;
            }
        }
    } else {
        transition_advisor(&current_state, &input, &mut context)?
    };
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
        source = if from_whatsapp { "whatsapp" } else { "crm-app" },
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
        Some(&new_state),
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

    // `last_message_at` del asesor solo tiene sentido si de verdad hubo un
    // mensaje suyo por WhatsApp: es lo que mantiene viva esa ventana de 24h.
    // Desde `crm-app` no hay tal conversación, así que no se toca.
    if from_whatsapp {
        update_last_message(&state.pool, &advisor_phone).await?;
    }
    update_last_message(&state.pool, &target_phone).await?;
    Ok(())
}

/// Degradación segura cuando el motor de agente falla (timeout/5xx/saldo de
/// Anthropic, error de red, etc.): el cliente NUNCA queda en silencio y el
/// asesor SIEMPRE recibe el contexto del caso. El estado de la conversación
/// no se toca: el cliente puede reintentar y el caso queda donde estaba.
async fn degrade_agent_failure(
    state: &AppState,
    customer_phone: &str,
    context: &ConversationContext,
    current_state: &ConversationState,
    input: &UserInput,
    turn_actor: &str,
    err: Box<dyn Error + Send + Sync>,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    tracing::error!(
        actor = %turn_actor,
        phone = %mask_phone(customer_phone),
        state = %current_state.as_storage_key(),
        error = %err,
        "agent turn failed, degrading to fixed message"
    );

    if turn_actor == "customer" {
        let body = client_messages().agent.llm_failure_customer.clone();
        if let Err(send_err) = send_text(state, customer_phone, &body).await {
            tracing::error!(
                phone = %mask_phone(customer_phone),
                error = %send_err,
                "failed to send LLM-failure fallback message to customer"
            );
        }
        log_outbound_text(state, customer_phone, customer_phone, &body).await;
    }

    let last_message = match input {
        UserInput::TextMessage(text) => {
            let preview: String = text.chars().take(200).collect();
            preview
        }
        UserInput::ButtonPress(id) | UserInput::ListSelection(id) => format!("[botón: {id}]"),
        UserInput::ImageMessage(_) => "[imagen]".to_string(),
    };
    let advisor_body = format!(
        "⚠️ Error técnico del bot IA (mensaje de {} sin procesar).\nCliente: {} ({})\nÚltimo \
         mensaje: {}\nEstado del caso: {}\n\nEl caso quedó donde estaba; cuando el sistema se \
         recupere, el bot retoma solo. Si es urgente, contacta al cliente directamente.",
        if turn_actor == "customer" { "cliente" } else { "asesor" },
        context.customer_name.as_deref().unwrap_or("sin nombre"),
        customer_phone,
        last_message,
        current_state.as_storage_key(),
    );
    if let Err(send_err) = send_text(state, &state.config.advisor_phone, &advisor_body).await {
        tracing::error!(
            phone = %mask_phone(customer_phone),
            error = %send_err,
            "failed to notify advisor about agent failure"
        );
    }
    log_outbound_text(state, customer_phone, &state.config.advisor_phone, &advisor_body).await;

    update_last_message(&state.pool, customer_phone).await?;
    Ok(())
}

pub async fn mark_as_read_if_supported(
    state: &AppState,
    message_id: &str,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    state.transport.mark_as_read(message_id).await?;

    Ok(())
}

pub async fn send_text(
    state: &AppState,
    to: &str,
    body: &str,
) -> Result<Option<String>, Box<dyn Error + Send + Sync>> {
    send_via_transport(state, to, "text", Some(body.to_string()), json!({})).await
}

pub async fn send_timer_actions(
    state: &AppState,
    case_phone: &str,
    actions: &[BotAction],
) -> Result<(), Box<dyn Error + Send + Sync>> {
    for action in actions {
        log_bot_action(action);
        log_outbound_event(state, case_phone, action).await;
        match action {
            BotAction::SendText { to, body } => {
                send_text(state, to, body).await?;
            }
            BotAction::SendButtons { to, body, buttons } => {
                send_via_transport(
                    state,
                    to,
                    "buttons",
                    Some(body.clone()),
                    json!({ "buttons": buttons }),
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
                )
                .await?;
            }
            BotAction::SendImage {
                to,
                media_id,
                caption,
            } => {
                send_image(state, to, media_id, caption.clone()).await?;
            }
            BotAction::SendAssetImage { to, asset, caption } => {
                send_asset_image(state, to, asset.clone(), caption.clone()).await?;
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
    thread_recording_state: Option<&ConversationState>,
) -> Result<ExecutionOutcome, Box<dyn Error + Send + Sync>> {
    let mut reset_requested = false;
    let case_phone = context.phone_number.clone();

    for action in actions {
        log_bot_action(action);
        log_outbound_event(state, &case_phone, action).await;
        match action {
            BotAction::SendText { to, body } => {
                let message_id = send_text(state, to, body).await?;
                record_advisor_reply_thread_if_needed(
                    state,
                    to,
                    session_phone,
                    thread_recording_state,
                    message_id,
                )
                .await?;
            }
            BotAction::SendButtons { to, body, buttons } => {
                let message_id = send_via_transport(
                    state,
                    to,
                    "buttons",
                    Some(body.clone()),
                    json!({ "buttons": buttons }),
                )
                .await?;
                record_advisor_reply_thread_if_needed(
                    state,
                    to,
                    session_phone,
                    thread_recording_state,
                    message_id,
                )
                .await?;
            }
            BotAction::SendList {
                to,
                body,
                button_text,
                sections,
            } => {
                let message_id = send_via_transport(
                    state,
                    to,
                    "list",
                    Some(body.clone()),
                    json!({
                        "button_text": button_text,
                        "sections": sections,
                    }),
                )
                .await?;
                record_advisor_reply_thread_if_needed(
                    state,
                    to,
                    session_phone,
                    thread_recording_state,
                    message_id,
                )
                .await?;
            }
            BotAction::SendImage {
                to,
                media_id,
                caption,
            } => {
                let message_id = send_image(state, to, media_id, caption.clone()).await?;
                record_advisor_reply_thread_if_needed(
                    state,
                    to,
                    session_phone,
                    thread_recording_state,
                    message_id,
                )
                .await?;
            }
            BotAction::SendAssetImage { to, asset, caption } => {
                let message_id =
                    send_asset_image(state, to, asset.clone(), caption.clone()).await?;
                record_advisor_reply_thread_if_needed(
                    state,
                    to,
                    session_phone,
                    thread_recording_state,
                    message_id,
                )
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
                let message_id = send_text(state, to, body).await?;
                record_advisor_reply_thread_if_needed(
                    state,
                    to,
                    session_phone,
                    thread_recording_state,
                    message_id,
                )
                .await?;
            }
            BotAction::ResetConversation { phone } => {
                reset_conversation(&state.pool, phone).await?;
                clear_advisor_threads_for_target(state, phone).await?;
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
                    effective_duration_for_start_timer(&timer_type, *duration);
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
                let message_id = send_text(state, to, body).await?;
                record_advisor_reply_thread_if_needed(
                    state,
                    to,
                    session_phone,
                    thread_recording_state,
                    message_id,
                )
                .await?;
            }
            BotAction::UpdateCustomerAndAnalytics {
                phone_number_meta,
                order_id,
                total_spent_cop,
                total_units_purchased,
                referral_code,
                referral_discount_cop,
                ambassador_commission_cop,
                referral_times_used_inc,
            } => {
                update_customer_totals(&state.pool, phone_number_meta, *total_spent_cop, *total_units_purchased)
                    .await?;

                // Delta > 0 = venta nueva o ampliada; deltas <= 0 son
                // modificaciones que reducen un pedido ya reportado, no una
                // compra nueva que reportarle a Meta (ver docs/PENDIENTE_capi_meta.md).
                // Corre en background: la CAPI es telemetría, nunca puede
                // demorar la confirmación del pedido al cliente.
                if *total_spent_cop > 0 {
                    let capi = state.capi.clone();
                    let pool = state.pool.clone();
                    let phone = phone_number_meta.clone();
                    let order_id = *order_id;
                    let value_cop = *total_spent_cop;
                    tokio::spawn(async move {
                        let ctwa_clid = match crate::db::queries::get_customer(&pool, &phone).await
                        {
                            Ok(Some(customer)) => customer.ctwa_clid,
                            Ok(None) => None,
                            Err(err) => {
                                tracing::warn!(
                                    order_id,
                                    error = %err,
                                    "failed to look up ctwa_clid for CAPI purchase report"
                                );
                                None
                            }
                        };
                        capi.report_purchase(order_id, ctwa_clid, value_cop).await;
                    });
                }

                if let Some(code) = referral_code {
                    let discount_inc = referral_discount_cop.unwrap_or(0);
                    let commission_inc = ambassador_commission_cop.unwrap_or(0);
                    create_or_update_referral_analytics(
                        &state.pool,
                        code,
                        *referral_times_used_inc,
                        discount_inc,
                        commission_inc,
                        *total_units_purchased,
                        *total_spent_cop,
                    )
                    .await?;
                }

                tracing::info!(
                    phone = %mask_phone(phone_number_meta),
                    total_spent_cop = *total_spent_cop,
                    total_units_purchased = *total_units_purchased,
                    has_referral_code = referral_code.is_some(),
                    "updated customer totals and referral analytics"
                );
            }
        }
    }

    Ok(ExecutionOutcome { reset_requested })
}

/// El agente de IA controla la conversacion de autoservicio del cliente
/// (menu, armar el pedido, datos, checkout) y, desde `AskDeliveryCost` en
/// adelante, tambien la coordinacion con el asesor (domicilio, pago,
/// comprobante) — ver `src/ai/agent.rs`. Fuera de esta lista (negociacion de
/// hora, pedido al por mayor sin tomar, "Hablar con Asesor" sin pedido,
/// relay) sigue el motor deterministico sin cambios.
fn is_agent_owned_state(state: &ConversationState) -> bool {
    matches!(
        state,
        ConversationState::MainMenu
            | ConversationState::AgentChat
            | ConversationState::ViewMenu
            | ConversationState::ViewSchedule
            | ConversationState::WhenDelivery
            | ConversationState::CheckSchedule
            | ConversationState::OutOfHours
            | ConversationState::SelectDate
            | ConversationState::SelectTime
            | ConversationState::ConfirmSchedule
            | ConversationState::CollectName
            | ConversationState::CollectPhone
            | ConversationState::CollectAddress
            | ConversationState::SelectType
            | ConversationState::SelectFlavor { .. }
            | ConversationState::SelectQuantity { .. }
            | ConversationState::AddMore
            | ConversationState::ConfirmRestartOrder
            | ConversationState::ConfirmCustomerData
            | ConversationState::SelectCustomerDataField
            | ConversationState::EditCustomerName
            | ConversationState::EditCustomerPhone
            | ConversationState::EditCustomerAddress
            | ConversationState::ReviewCheckout
            | ConversationState::AskDeliveryCost
            | ConversationState::SelectPaymentMethod
            | ConversationState::WaitReceipt
    )
}

fn should_use_agent(current_state: &ConversationState) -> bool {
    is_agent_owned_state(current_state)
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

/// Devuelve el valor personalizado solo si difiere del de Meta; si coinciden
/// (o no hay personalizado) devuelve None para no duplicar el dato de Meta en
/// la columna manual (hallazgo C).
fn manual_override<'a>(custom: Option<&'a str>, meta: Option<&str>) -> Option<&'a str> {
    match (custom, meta) {
        (Some(c), Some(m)) if c.trim() == m.trim() => None,
        (Some(c), _) => Some(c),
        (None, _) => None,
    }
}

fn seed_customer_data(
    context: &mut ConversationContext,
    phone: &str,
    profile_name: Option<&str>,
) -> SeededCustomerData {
    let mut seeded = SeededCustomerData::default();

    // Datos base de Meta (inmutables, siempre visibles para el asesor). El
    // celular de Meta es el número de la conversación; el nombre viene del
    // perfil de WhatsApp (ver docs/canary-fixes-2026-07-19.md hallazgo C).
    context.meta_customer_phone = Some(phone.to_string());
    if let Some(profile_name) = profile_name {
        let trimmed = profile_name.trim();
        if !trimmed.is_empty() {
            context.meta_customer_name = Some(trimmed.to_string());
        }
    }

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
    reply_to_message_id: Option<&str>,
) -> Option<String> {
    if let Some(target) = reply_to_message_id
        .and_then(|message_id| state_data.advisor_reply_threads.get(message_id))
        .cloned()
    {
        return Some(target);
    }

    match input {
        UserInput::ButtonPress(id) | UserInput::ListSelection(id) => parse_advisor_button_id(id)
            .map(|(_, phone)| phone)
            .or_else(|| state_data.advisor_target_phone.clone()),
        UserInput::TextMessage(_) => state_data.advisor_target_phone.clone(),
        UserInput::ImageMessage(_) => state_data.advisor_target_phone.clone(),
    }
}

fn should_record_advisor_thread(state: Option<&ConversationState>) -> bool {
    matches!(
        state,
        Some(
            ConversationState::WaitAdvisorResponse
                | ConversationState::AskDeliveryCost
                | ConversationState::NegotiateHour
                | ConversationState::WaitAdvisorHourDecision { .. }
                | ConversationState::WaitAdvisorConfirmHour
                | ConversationState::WaitAdvisorMayor
                | ConversationState::WaitAdvisorContact
        )
    )
}

async fn record_advisor_reply_thread_if_needed(
    state: &AppState,
    to: &str,
    target_phone: Option<&str>,
    thread_recording_state: Option<&ConversationState>,
    message_id: Option<String>,
) -> Result<(), sqlx::Error> {
    if to != state.config.advisor_phone || !should_record_advisor_thread(thread_recording_state) {
        return Ok(());
    }

    let (Some(target_phone), Some(message_id)) = (target_phone, message_id) else {
        return Ok(());
    };

    let conversation = load_or_create_conversation(state, &state.config.advisor_phone).await?;
    let mut state_data = conversation.state_data.0;
    state_data
        .advisor_reply_threads
        .insert(message_id.clone(), target_phone.to_string());
    update_state(
        &state.pool,
        &state.config.advisor_phone,
        &conversation.state,
        &state_data,
    )
    .await?;
    tracing::debug!(
        advisor_phone = %mask_phone(&state.config.advisor_phone),
        target_phone = %mask_phone(target_phone),
        message_id = %message_id,
        "recorded advisor reply thread"
    );
    Ok(())
}

pub async fn clear_advisor_threads_for_target(
    state: &AppState,
    target_phone: &str,
) -> Result<(), sqlx::Error> {
    if let Some(conversation) = get_conversation(&state.pool, &state.config.advisor_phone).await? {
        let mut state_data = conversation.state_data.0;
        let original_len = state_data.advisor_reply_threads.len();
        state_data
            .advisor_reply_threads
            .retain(|_, phone| phone != target_phone);
        if state_data.advisor_reply_threads.len() != original_len {
            update_state(
                &state.pool,
                &state.config.advisor_phone,
                &conversation.state,
                &state_data,
            )
            .await?;
        }
    }

    Ok(())
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
) -> Result<Option<String>, Box<dyn Error + Send + Sync>> {
    // Corte de Fase 4: con el canal directo apagado, nada dirigido al asesor
    // sale a Meta. Se descarta acá, en el único punto por el que pasa TODO lo
    // saliente, y a propósito DESPUÉS de `log_outbound_event` — el evento ya
    // quedó en `message_events` con `channel='advisor'`, que es de donde
    // `crm-app` arma la cola de casos. El asesor no recibe WhatsApp; la
    // información no se pierde.
    if !state.config.advisor_whatsapp_enabled && to == state.config.advisor_phone {
        tracing::debug!(
            advisor_phone = %mask_phone(to),
            message_kind,
            "advisor whatsapp disabled; message kept in trace only"
        );
        return Ok(None);
    }

    let client = &state.transport;
    let message_id = match message_kind {
        "text" => {
            client
                .send_text(to, body.as_deref().unwrap_or_default())
                .await?
        }
        "buttons" => {
            let buttons = serde_json::from_value(payload["buttons"].clone())?;
            client
                .send_buttons(to, body.as_deref().unwrap_or_default(), buttons)
                .await?
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
                .await?
        }
        "image" => {
            let media_id = payload["media_id"].as_str().unwrap_or_default();
            let caption = payload["caption"].as_str();
            client.send_image(to, media_id, caption).await?
        }
        _ => None,
    };

    Ok(message_id)
}

async fn send_image(
    state: &AppState,
    to: &str,
    media_id: &str,
    caption: Option<String>,
) -> Result<Option<String>, Box<dyn Error + Send + Sync>> {
    let payload_caption = caption.clone();
    send_via_transport(
        state,
        to,
        "image",
        caption,
        json!({ "media_id": media_id, "caption": payload_caption }),
    )
    .await
}

async fn send_asset_image(
    state: &AppState,
    to: &str,
    asset: ImageAsset,
    caption: Option<String>,
) -> Result<Option<String>, Box<dyn Error + Send + Sync>> {
    let ImageAsset::Menu = asset;
    let media_id = &state.config.menu_image_media_id;
    send_via_transport(
        state,
        to,
        "image",
        caption.clone(),
        json!({ "media_id": media_id, "caption": caption }),
    )
    .await
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

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use crate::{bot::state_machine::UserInput, db::models::ConversationStateData};

    use super::resolve_advisor_target_phone;

    #[test]
    fn advisor_quoted_message_wins_over_active_session() {
        let mut threads = BTreeMap::new();
        threads.insert("wamid-old-case".to_string(), "573001111111".to_string());
        let state_data = ConversationStateData {
            advisor_target_phone: Some("573002222222".to_string()),
            advisor_reply_threads: threads,
            ..Default::default()
        };

        let target = resolve_advisor_target_phone(
            &state_data,
            &UserInput::TextMessage("Confirmo".to_string()),
            Some("wamid-old-case"),
        );

        assert_eq!(target.as_deref(), Some("573001111111"));
    }

    #[test]
    fn advisor_routing_falls_back_to_active_session_without_valid_quote() {
        let state_data = ConversationStateData {
            advisor_target_phone: Some("573002222222".to_string()),
            ..Default::default()
        };

        let target = resolve_advisor_target_phone(
            &state_data,
            &UserInput::TextMessage("Confirmo".to_string()),
            Some("wamid-missing"),
        );

        assert_eq!(target.as_deref(), Some("573002222222"));
    }

    #[test]
    fn keepalive_ping_matches_bare_checkmark_ignoring_whitespace() {
        assert!(super::is_window_keepalive_ping(&UserInput::TextMessage(
            "✅".to_string()
        )));
        assert!(super::is_window_keepalive_ping(&UserInput::TextMessage(
            "  ✅  ".to_string()
        )));
    }

    #[test]
    fn keepalive_ping_does_not_match_other_text_or_buttons() {
        assert!(!super::is_window_keepalive_ping(&UserInput::TextMessage(
            "✅ listo".to_string()
        )));
        assert!(!super::is_window_keepalive_ping(&UserInput::ButtonPress(
            "✅".to_string()
        )));
    }

    #[test]
    fn channel_classification_splits_client_and_advisor_lanes() {
        assert_eq!(
            super::channel_for_recipient("573001111111", "573009999999"),
            super::CHANNEL_CLIENT
        );
        assert_eq!(
            super::channel_for_recipient("573009999999", "573009999999"),
            super::CHANNEL_ADVISOR
        );
    }

    #[test]
    fn outbound_recipient_only_matches_message_actions() {
        use crate::bot::state_machine::BotAction;

        let text = BotAction::SendText {
            to: "573001111111".to_string(),
            body: "hola".to_string(),
        };
        assert_eq!(super::outbound_recipient(&text), Some("573001111111"));

        let reset = BotAction::ResetConversation {
            phone: "573001111111".to_string(),
        };
        assert_eq!(super::outbound_recipient(&reset), None);
    }

    #[test]
    fn describe_outbound_action_extracts_type_and_body() {
        use crate::bot::state_machine::BotAction;

        let (content_type, body, payload) = super::describe_outbound_action(&BotAction::SendText {
            to: "x".to_string(),
            body: "hola".to_string(),
        })
        .expect("text is loggable");
        assert_eq!(content_type, "text");
        assert_eq!(body.as_deref(), Some("hola"));
        assert!(payload.is_none());

        assert!(super::describe_outbound_action(&BotAction::NoOp).is_none());
    }

    #[test]
    fn describe_inbound_input_maps_every_variant() {
        assert_eq!(
            super::describe_inbound_input(&UserInput::TextMessage("hola".to_string())).0,
            "text"
        );
        assert_eq!(
            super::describe_inbound_input(&UserInput::ButtonPress("btn".to_string())).0,
            "button_reply"
        );
        assert_eq!(
            super::describe_inbound_input(&UserInput::ListSelection("opt".to_string())).0,
            "list_reply"
        );
        assert_eq!(
            super::describe_inbound_input(&UserInput::ImageMessage("mid".to_string())).0,
            "image"
        );
    }
}
