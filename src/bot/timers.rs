use std::{
    collections::HashMap,
    future::Future,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc,
    },
    time::Duration,
};

use chrono::{DateTime, Utc};
use tokio::sync::Mutex;
use tokio::time::{interval, MissedTickBehavior};
use tokio_util::sync::CancellationToken;

use crate::{
    bot::{
        inactivity::{
            reminder_actions, reset_notice_actions, CONVERSATION_REMINDER_TIMEOUT,
            CONVERSATION_RESET_TIMEOUT,
        },
        state_machine::{BotAction, ConversationContext, ConversationState, ImageAsset, TimerType},
    },
    db::{
        models::ConversationStateData,
        queries::{
            get_conversation, list_active_timer_conversations, reset_conversation,
            update_order_status, update_state,
        },
    },
    messages::client_messages,
    whatsapp::types::{Button, ButtonReplyPayload},
    AppState,
};

pub type TimerKey = (String, TimerType);
pub type TimerMap = Arc<Mutex<HashMap<TimerKey, ActiveTimer>>>;

pub const RECEIPT_TIMEOUT: Duration = Duration::from_secs(10 * 60);
pub const ADVISOR_RESPONSE_TIMEOUT: Duration = Duration::from_secs(2 * 60);
pub const ADVISOR_STUCK_TIMEOUT: Duration = Duration::from_secs(30 * 60);
pub const RELAY_INACTIVITY_TIMEOUT: Duration = Duration::from_secs(30 * 60);
const TIMER_SWEEP_INTERVAL: Duration = Duration::from_secs(60);
static NEXT_TIMER_INSTANCE_ID: AtomicU64 = AtomicU64::new(1);

pub struct ActiveTimer {
    token: CancellationToken,
    instance_id: u64,
}

pub fn new_timer_map() -> TimerMap {
    Arc::new(Mutex::new(HashMap::new()))
}

pub async fn start_timer<F, Fut>(timers: TimerMap, key: TimerKey, duration: Duration, on_expire: F)
where
    F: FnOnce() -> Fut + Send + 'static,
    Fut: Future<Output = ()> + Send + 'static,
{
    let token = CancellationToken::new();
    let wait_token = token.clone();
    let map = timers.clone();
    let key_for_task = key.clone();
    let instance_id = NEXT_TIMER_INSTANCE_ID.fetch_add(1, Ordering::Relaxed);

    {
        let mut active = timers.lock().await;
        if let Some(previous) = active.insert(key, ActiveTimer { token, instance_id }) {
            previous.token.cancel();
        }
    }

    tokio::spawn(async move {
        tokio::select! {
            _ = tokio::time::sleep(duration) => {
                on_expire().await;
            }
            _ = wait_token.cancelled() => {}
        }

        let mut active = map.lock().await;
        let should_remove = active
            .get(&key_for_task)
            .map(|entry| entry.instance_id == instance_id)
            .unwrap_or(false);
        if should_remove {
            active.remove(&key_for_task);
        }
    });
}

pub async fn cancel_timer(timers: TimerMap, key: &TimerKey) {
    let mut active = timers.lock().await;
    if let Some(token) = active.remove(key) {
        token.token.cancel();
    }
}

pub async fn restore_pending_timers(state: AppState) -> Result<(), sqlx::Error> {
    let recovery_states = timer_recovery_states();
    let conversations = list_active_timer_conversations(&state.pool, &recovery_states).await?;

    for conversation in conversations {
        match timer_recovery(&conversation, Utc::now()) {
            Some(TimerRecovery::Expired(timer_type)) => {
                expire_timer_now(state.clone(), conversation.phone_number.clone(), timer_type)
                    .await;
            }
            Some(TimerRecovery::Active {
                timer_type,
                timeout,
                started_at,
            }) => {
                restore_timer(
                    state.clone(),
                    conversation.phone_number.clone(),
                    timer_type,
                    timeout,
                    started_at,
                )
                .await;
            }
            None => {}
        }
    }

    Ok(())
}

pub async fn sweep_expired_timers(state: AppState) -> Result<(), sqlx::Error> {
    let recovery_states = timer_recovery_states();
    let conversations = list_active_timer_conversations(&state.pool, &recovery_states).await?;

    for conversation in conversations {
        if let Some(TimerRecovery::Expired(timer_type)) = timer_recovery(&conversation, Utc::now())
        {
            expire_timer_now(state.clone(), conversation.phone_number.clone(), timer_type).await;
        }
    }

    Ok(())
}

pub fn spawn_timer_sweeper(state: AppState) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let mut ticker = interval(TIMER_SWEEP_INTERVAL);
        ticker.set_missed_tick_behavior(MissedTickBehavior::Delay);

        loop {
            ticker.tick().await;

            if let Err(err) = sweep_expired_timers(state.clone()).await {
                tracing::error!(error = %err, "failed to sweep expired timers");
            }
        }
    })
}

async fn restore_timer(
    state: AppState,
    phone_number: String,
    timer_type: TimerType,
    timeout: Duration,
    started_at: chrono::DateTime<chrono::Utc>,
) {
    let elapsed = elapsed_since(started_at, chrono::Utc::now());
    if elapsed >= timeout {
        expire_timer_now(state, phone_number, timer_type).await;
        return;
    }

    let remaining = timeout - elapsed;
    let app_state = state.clone();
    let phone = phone_number.clone();
    let kind = timer_type.clone();

    start_timer(
        state.timers.clone(),
        (phone_number, timer_type),
        remaining,
        move || {
            let app_state = app_state.clone();
            let phone = phone.clone();
            let kind = kind.clone();
            Box::pin(async move {
                expire_timer_now(app_state, phone, kind).await;
            })
        },
    )
    .await;
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum TimerRecovery {
    Active {
        timer_type: TimerType,
        timeout: Duration,
        started_at: DateTime<Utc>,
    },
    Expired(TimerType),
}

fn timer_recovery(
    conversation: &crate::db::queries::ActiveTimerConversation,
    now: DateTime<Utc>,
) -> Option<TimerRecovery> {
    let state_data = &conversation.state_data.0;

    if customer_inactivity_state(conversation.state.as_str()) {
        let started_at = state_data
            .conversation_abandon_started_at
            .unwrap_or(conversation.last_message_at);
        let timeout = if state_data.conversation_abandon_reminder_sent {
            CONVERSATION_RESET_TIMEOUT
        } else {
            CONVERSATION_REMINDER_TIMEOUT
        };

        return timer_recovery_for(TimerType::ConversationAbandon, timeout, started_at, now);
    }

    if let Some(timeout) = advisor_timeout_for_state(conversation.state.as_str()) {
        if state_data.advisor_timer_expired {
            return None;
        }

        return timer_recovery_for(
            TimerType::AdvisorResponse,
            timeout,
            state_data
                .advisor_timer_started_at
                .unwrap_or(conversation.last_message_at),
            now,
        );
    }

    match conversation.state.as_str() {
        "wait_receipt" if !state_data.receipt_timer_expired => timer_recovery_for(
            TimerType::ReceiptUpload,
            RECEIPT_TIMEOUT,
            state_data
                .receipt_timer_started_at
                .unwrap_or(conversation.last_message_at),
            now,
        ),
        "relay_mode" => timer_recovery_for(
            TimerType::RelayInactivity,
            RELAY_INACTIVITY_TIMEOUT,
            state_data
                .relay_timer_started_at
                .unwrap_or(conversation.last_message_at),
            now,
        ),
        _ => None,
    }
}

fn timer_recovery_for(
    timer_type: TimerType,
    timeout: Duration,
    started_at: DateTime<Utc>,
    now: DateTime<Utc>,
) -> Option<TimerRecovery> {
    if elapsed_since(started_at, now) >= timeout {
        Some(TimerRecovery::Expired(timer_type))
    } else {
        Some(TimerRecovery::Active {
            timer_type,
            timeout,
            started_at,
        })
    }
}

async fn expire_timer_now(state: AppState, phone_number: String, timer_type: TimerType) {
    let result = match timer_type {
        TimerType::ReceiptUpload => expire_receipt_timer(state, phone_number).await,
        TimerType::AdvisorResponse => expire_advisor_timer(state, phone_number).await,
        TimerType::RelayInactivity => expire_relay_timer(state, phone_number).await,
        TimerType::ConversationAbandon => expire_conversation_abandon(state, phone_number).await,
    };

    if let Err(err) = result {
        tracing::error!(error = %err, "failed to expire timer");
    }
}

fn elapsed_since(
    started_at: chrono::DateTime<chrono::Utc>,
    now: chrono::DateTime<chrono::Utc>,
) -> Duration {
    now.signed_duration_since(started_at)
        .to_std()
        .unwrap_or_default()
}

pub async fn expire_receipt_timer(
    state: AppState,
    phone_number: String,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let Some(conversation) = get_conversation(&state.pool, &phone_number).await? else {
        return Ok(());
    };

    if conversation.state != "wait_receipt" {
        return Ok(());
    }

    let mut state_data = conversation.state_data.0;
    if state_data.receipt_timer_expired {
        return Ok(());
    }

    state_data.receipt_timer_expired = true;
    update_state(&state.pool, &phone_number, "wait_receipt", &state_data).await?;
    state
        .wa_client
        .send_text(
            &phone_number,
            &client_messages().timers_customer.receipt_timeout_text,
        )
        .await?;
    state
        .wa_client
        .send_buttons(
            &phone_number,
            &client_messages()
                .timers_customer
                .receipt_timeout_buttons_body,
            receipt_timeout_buttons(),
        )
        .await?;

    Ok(())
}

pub async fn expire_advisor_timer(
    state: AppState,
    phone_number: String,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let Some(conversation) = get_conversation(&state.pool, &phone_number).await? else {
        return Ok(());
    };

    let Some(timeout_kind) = advisor_timeout_kind(conversation.state.as_str()) else {
        return Ok(());
    };

    let mut state_data = conversation.state_data.0;
    if state_data.advisor_timer_expired {
        return Ok(());
    }

    clear_advisor_session(&state).await?;

    match timeout_kind {
        AdvisorTimeoutKind::FallbackButtons => {
            state_data.advisor_timer_expired = true;
            state_data.advisor_timer_started_at = None;
            update_state(&state.pool, &phone_number, &conversation.state, &state_data).await?;

            match conversation.state.as_str() {
                "wait_advisor_contact" => {
                    state
                        .wa_client
                        .send_buttons(
                            &phone_number,
                            &client_messages().timers_customer.contact_timeout_body,
                            contact_timeout_buttons(),
                        )
                        .await?;
                }
                _ => {
                    let timeout_text = if conversation.state == "wait_advisor_mayor" {
                        &client_messages()
                            .timers_customer
                            .advisor_timeout_wholesale_text
                    } else {
                        &client_messages().timers_customer.advisor_timeout_text
                    };
                    state
                        .wa_client
                        .send_text(&phone_number, timeout_text)
                        .await?;
                    state
                        .wa_client
                        .send_buttons(
                            &phone_number,
                            &client_messages()
                                .timers_customer
                                .advisor_timeout_buttons_body,
                            advisor_timeout_buttons(),
                        )
                        .await?;
                }
            }
        }
        AdvisorTimeoutKind::HardReset => {
            if let Some(order_id) = state_data.current_order_id {
                update_order_status(&state.pool, order_id, "manual_followup").await?;
            }

            reset_conversation(&state.pool, &phone_number).await?;
            state
                .wa_client
                .send_text(
                    &phone_number,
                    &client_messages().timers_customer.advisor_stuck_timeout_text,
                )
                .await?;
        }
    }

    Ok(())
}

pub async fn expire_relay_timer(
    state: AppState,
    phone_number: String,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let Some(conversation) = get_conversation(&state.pool, &phone_number).await? else {
        return Ok(());
    };

    if conversation.state != "relay_mode" {
        return Ok(());
    }

    reset_conversation(&state.pool, &phone_number).await?;
    clear_advisor_session(&state).await?;
    state
        .wa_client
        .send_text(
            &phone_number,
            &client_messages().timers_customer.relay_timeout_text,
        )
        .await?;
    state
        .wa_client
        .send_text(
            &state.config.advisor_phone,
            &format!(
                "Relay {} cerrado por inactividad.",
                phone_marker(&phone_number)
            ),
        )
        .await?;

    Ok(())
}

pub async fn expire_conversation_abandon(
    state: AppState,
    phone_number: String,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let Some(conversation) = get_conversation(&state.pool, &phone_number).await? else {
        return Ok(());
    };

    if !customer_inactivity_state(&conversation.state) {
        return Ok(());
    }

    let mut state_data = conversation.state_data.0;
    let started_at = state_data
        .conversation_abandon_started_at
        .unwrap_or(conversation.last_message_at);
    let now = Utc::now();
    let elapsed = elapsed_since(started_at, now);

    if !state_data.conversation_abandon_reminder_sent && elapsed < CONVERSATION_RESET_TIMEOUT {
        let context = ConversationContext::from_persisted(
            conversation.phone_number.clone(),
            state.config.advisor_phone.clone(),
            conversation.customer_name.clone(),
            conversation.customer_phone.clone(),
            conversation.delivery_address.clone(),
            &state_data,
        );
        let current_state = match ConversationState::from_storage_key(&conversation.state, &context)
        {
            Ok(state) => state,
            Err(err) => {
                tracing::error!(
                    phone = %conversation.phone_number,
                    error = %err,
                    "failed to rehydrate state for inactivity reminder"
                );
                reset_conversation(&state.pool, &phone_number).await?;
                return Ok(());
            }
        };

        let actions = reminder_actions(&current_state, &context);
        send_timer_actions(&state, &actions).await?;

        state_data.conversation_abandon_started_at = Some(started_at);
        state_data.conversation_abandon_reminder_sent = true;
        update_state(&state.pool, &phone_number, &conversation.state, &state_data).await?;

        return Ok(());
    }

    send_timer_actions(&state, &reset_notice_actions(&phone_number)).await?;
    reset_conversation(&state.pool, &phone_number).await?;

    Ok(())
}

fn timer_recovery_states() -> Vec<&'static str> {
    vec![
        "wait_receipt",
        "wait_advisor_response",
        "wait_advisor_mayor",
        "wait_advisor_contact",
        "ask_delivery_cost",
        "negotiate_hour",
        "wait_advisor_hour_decision",
        "wait_advisor_confirm_hour",
        "relay_mode",
        "main_menu",
        "view_menu",
        "view_schedule",
        "when_delivery",
        "out_of_hours",
        "select_date",
        "select_time",
        "confirm_schedule",
        "collect_name",
        "collect_phone",
        "collect_address",
        "select_type",
        "select_flavor",
        "select_quantity",
        "add_more",
        "confirm_address",
        "show_summary",
        "offer_hour_to_client",
        "wait_client_hour",
        "contact_advisor_name",
        "contact_advisor_phone",
        "leave_message",
    ]
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AdvisorTimeoutKind {
    FallbackButtons,
    HardReset,
}

fn advisor_timeout_kind(state: &str) -> Option<AdvisorTimeoutKind> {
    match state {
        "wait_advisor_response" | "wait_advisor_mayor" | "wait_advisor_contact" => {
            Some(AdvisorTimeoutKind::FallbackButtons)
        }
        "ask_delivery_cost"
        | "negotiate_hour"
        | "wait_advisor_hour_decision"
        | "wait_advisor_confirm_hour" => Some(AdvisorTimeoutKind::HardReset),
        _ => None,
    }
}

fn advisor_timeout_for_state(state: &str) -> Option<Duration> {
    match advisor_timeout_kind(state) {
        Some(AdvisorTimeoutKind::FallbackButtons) => Some(ADVISOR_RESPONSE_TIMEOUT),
        Some(AdvisorTimeoutKind::HardReset) => Some(ADVISOR_STUCK_TIMEOUT),
        None => None,
    }
}

fn customer_inactivity_state(state: &str) -> bool {
    matches!(
        state,
        "main_menu"
            | "view_menu"
            | "view_schedule"
            | "when_delivery"
            | "out_of_hours"
            | "select_date"
            | "select_time"
            | "confirm_schedule"
            | "collect_name"
            | "collect_phone"
            | "collect_address"
            | "select_type"
            | "select_flavor"
            | "select_quantity"
            | "add_more"
            | "confirm_address"
            | "show_summary"
            | "offer_hour_to_client"
            | "wait_client_hour"
            | "contact_advisor_name"
            | "contact_advisor_phone"
            | "leave_message"
    )
}

async fn send_timer_actions(
    state: &AppState,
    actions: &[BotAction],
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
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
            BotAction::NoOp => {}
            _ => {
                tracing::warn!("skipping unsupported timer action during inactivity resend");
            }
        }
    }

    Ok(())
}

pub fn rehydrate_context_for_timer(
    phone_number: String,
    advisor_phone: String,
    customer_name: Option<String>,
    customer_phone: Option<String>,
    delivery_address: Option<String>,
    state_data: &ConversationStateData,
) -> ConversationContext {
    ConversationContext::from_persisted(
        phone_number,
        advisor_phone,
        customer_name,
        customer_phone,
        delivery_address,
        state_data,
    )
}

async fn clear_advisor_session(
    state: &AppState,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    if let Some(advisor_conversation) =
        get_conversation(&state.pool, &state.config.advisor_phone).await?
    {
        let mut state_data = advisor_conversation.state_data.0;
        state_data.advisor_target_phone = None;
        update_state(
            &state.pool,
            &state.config.advisor_phone,
            &advisor_conversation.state,
            &state_data,
        )
        .await?;
    }

    Ok(())
}

fn receipt_timeout_buttons() -> Vec<Button> {
    vec![
        reply_button(
            "change_payment_method",
            &client_messages()
                .checkout
                .receipt_timeout_change_payment_button,
        ),
        reply_button(
            "cancel_order",
            &client_messages().checkout.receipt_timeout_cancel_button,
        ),
    ]
}

fn advisor_timeout_buttons() -> Vec<Button> {
    vec![
        reply_button(
            "advisor_timeout_schedule",
            &client_messages()
                .timers_customer
                .advisor_timeout_schedule_button,
        ),
        reply_button(
            "advisor_timeout_retry",
            &client_messages()
                .timers_customer
                .advisor_timeout_retry_button,
        ),
        reply_button(
            "advisor_timeout_menu",
            &client_messages()
                .timers_customer
                .advisor_timeout_menu_button,
        ),
    ]
}

fn contact_timeout_buttons() -> Vec<Button> {
    vec![
        reply_button(
            "leave_message",
            &client_messages()
                .timers_customer
                .contact_timeout_leave_message_button,
        ),
        reply_button(
            "back_main_menu",
            &client_messages()
                .timers_customer
                .contact_timeout_menu_button,
        ),
    ]
}

fn reply_button(id: &str, title: &str) -> Button {
    Button {
        kind: "reply".to_string(),
        reply: ButtonReplyPayload {
            id: id.to_string(),
            title: title.to_string(),
        },
    }
}

fn phone_marker(phone: &str) -> String {
    let suffix = if phone.len() >= 4 {
        &phone[phone.len() - 4..]
    } else {
        phone
    };
    format!("[...{suffix}]")
}

#[cfg(test)]
mod tests {
    use chrono::Duration as ChronoDuration;
    use sqlx::types::Json;

    use super::{timer_recovery, TimerRecovery, ADVISOR_STUCK_TIMEOUT};
    use crate::{
        bot::state_machine::TimerType,
        db::{models::ConversationStateData, queries::ActiveTimerConversation},
    };

    fn active_timer_conversation(
        state: &str,
        state_data: ConversationStateData,
        last_message_at: chrono::DateTime<chrono::Utc>,
    ) -> ActiveTimerConversation {
        ActiveTimerConversation {
            id: 1,
            phone_number: "573001234567".to_string(),
            state: state.to_string(),
            state_data: Json(state_data),
            customer_name: Some("Ana".to_string()),
            customer_phone: Some("3001234567".to_string()),
            delivery_address: Some("Cra 15 #20-30".to_string()),
            last_message_at,
        }
    }

    #[test]
    fn timer_recovery_marks_expired_relay_as_overdue() {
        let now = chrono::Utc::now();
        let conversation = active_timer_conversation(
            "relay_mode",
            ConversationStateData {
                relay_timer_started_at: Some(now - ChronoDuration::minutes(31)),
                ..Default::default()
            },
            now,
        );

        let recovery = timer_recovery(&conversation, now);

        assert_eq!(
            recovery,
            Some(TimerRecovery::Expired(TimerType::RelayInactivity))
        );
    }

    #[test]
    fn timer_recovery_uses_last_message_when_start_timestamp_is_missing() {
        let now = chrono::Utc::now();
        let conversation = active_timer_conversation(
            "wait_advisor_response",
            ConversationStateData::default(),
            now - ChronoDuration::minutes(3),
        );

        let recovery = timer_recovery(&conversation, now);

        assert_eq!(
            recovery,
            Some(TimerRecovery::Expired(TimerType::AdvisorResponse))
        );
    }

    #[test]
    fn timer_recovery_skips_already_expired_receipt_waits() {
        let now = chrono::Utc::now();
        let conversation = active_timer_conversation(
            "wait_receipt",
            ConversationStateData {
                receipt_timer_started_at: Some(now - ChronoDuration::minutes(20)),
                receipt_timer_expired: true,
                ..Default::default()
            },
            now,
        );

        let recovery = timer_recovery(&conversation, now);

        assert!(recovery.is_none());
    }

    #[test]
    fn timer_recovery_marks_customer_inactivity_reminder_as_due() {
        let now = chrono::Utc::now();
        let conversation = active_timer_conversation(
            "main_menu",
            ConversationStateData {
                conversation_abandon_started_at: Some(now - ChronoDuration::minutes(3)),
                ..Default::default()
            },
            now,
        );

        let recovery = timer_recovery(&conversation, now);

        assert_eq!(
            recovery,
            Some(TimerRecovery::Expired(TimerType::ConversationAbandon))
        );
    }

    #[test]
    fn timer_recovery_keeps_stuck_advisor_wait_active_for_thirty_minutes() {
        let now = chrono::Utc::now();
        let conversation = active_timer_conversation(
            "ask_delivery_cost",
            ConversationStateData {
                advisor_timer_started_at: Some(now - ChronoDuration::minutes(3)),
                ..Default::default()
            },
            now,
        );

        let recovery = timer_recovery(&conversation, now);

        assert_eq!(
            recovery,
            Some(TimerRecovery::Active {
                timer_type: TimerType::AdvisorResponse,
                timeout: ADVISOR_STUCK_TIMEOUT,
                started_at: now - ChronoDuration::minutes(3),
            })
        );
    }

    #[test]
    fn timer_recovery_marks_stuck_advisor_wait_overdue_after_thirty_minutes() {
        let now = chrono::Utc::now();
        let conversation = active_timer_conversation(
            "wait_advisor_confirm_hour",
            ConversationStateData {
                advisor_timer_started_at: Some(now - ChronoDuration::minutes(31)),
                ..Default::default()
            },
            now,
        );

        let recovery = timer_recovery(&conversation, now);

        assert_eq!(
            recovery,
            Some(TimerRecovery::Expired(TimerType::AdvisorResponse))
        );
    }

    #[test]
    fn timer_recovery_keeps_customer_reset_deadline_after_reminder() {
        let now = chrono::Utc::now();
        let conversation = active_timer_conversation(
            "collect_name",
            ConversationStateData {
                conversation_abandon_started_at: Some(now - ChronoDuration::minutes(10)),
                conversation_abandon_reminder_sent: true,
                ..Default::default()
            },
            now,
        );

        let recovery = timer_recovery(&conversation, now);

        assert_eq!(
            recovery,
            Some(TimerRecovery::Active {
                timer_type: TimerType::ConversationAbandon,
                timeout: crate::bot::inactivity::CONVERSATION_RESET_TIMEOUT,
                started_at: now - ChronoDuration::minutes(10),
            })
        );
    }
}
