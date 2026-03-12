use std::{collections::HashMap, future::Future, sync::Arc, time::Duration};

use tokio::sync::Mutex;
use tokio_util::sync::CancellationToken;

use crate::{
    bot::state_machine::{ConversationContext, TimerType},
    db::{
        models::ConversationStateData,
        queries::{
            get_conversation, list_active_timer_conversations, reset_conversation, update_state,
        },
    },
    messages::client_messages,
    whatsapp::types::{Button, ButtonReplyPayload},
    AppState,
};

pub type TimerKey = (String, TimerType);
pub type TimerMap = Arc<Mutex<HashMap<TimerKey, CancellationToken>>>;

pub const RECEIPT_TIMEOUT: Duration = Duration::from_secs(10 * 60);
pub const ADVISOR_RESPONSE_TIMEOUT: Duration = Duration::from_secs(2 * 60);
pub const RELAY_INACTIVITY_TIMEOUT: Duration = Duration::from_secs(30 * 60);

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

    {
        let mut active = timers.lock().await;
        if let Some(previous) = active.insert(key, token) {
            previous.cancel();
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
        active.remove(&key_for_task);
    });
}

pub async fn cancel_timer(timers: TimerMap, key: &TimerKey) {
    let mut active = timers.lock().await;
    if let Some(token) = active.remove(key) {
        token.cancel();
    }
}

pub async fn restore_pending_timers(state: AppState) -> Result<(), sqlx::Error> {
    let conversations = list_active_timer_conversations(
        &state.pool,
        &[
            "wait_receipt",
            "wait_advisor_response",
            "wait_advisor_mayor",
            "wait_advisor_contact",
            "relay_mode",
        ],
    )
    .await?;

    for conversation in conversations {
        let state_data = &conversation.state_data.0;
        match conversation.state.as_str() {
            "wait_receipt" if !state_data.receipt_timer_expired => {
                restore_timer(
                    state.clone(),
                    conversation.phone_number.clone(),
                    TimerType::ReceiptUpload,
                    RECEIPT_TIMEOUT,
                    state_data
                        .receipt_timer_started_at
                        .unwrap_or(conversation.last_message_at),
                )
                .await;
            }
            "wait_advisor_response" | "wait_advisor_mayor" | "wait_advisor_contact"
                if !state_data.advisor_timer_expired =>
            {
                restore_timer(
                    state.clone(),
                    conversation.phone_number.clone(),
                    TimerType::AdvisorResponse,
                    ADVISOR_RESPONSE_TIMEOUT,
                    state_data
                        .advisor_timer_started_at
                        .unwrap_or(conversation.last_message_at),
                )
                .await;
            }
            "relay_mode" => {
                restore_timer(
                    state.clone(),
                    conversation.phone_number.clone(),
                    TimerType::RelayInactivity,
                    RELAY_INACTIVITY_TIMEOUT,
                    state_data
                        .relay_timer_started_at
                        .unwrap_or(conversation.last_message_at),
                )
                .await;
            }
            _ => {}
        }
    }

    Ok(())
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
        move || async move {
            expire_timer_now(app_state, phone, kind).await;
        },
    )
    .await;
}

async fn expire_timer_now(state: AppState, phone_number: String, timer_type: TimerType) {
    let result = match timer_type {
        TimerType::ReceiptUpload => expire_receipt_timer(state, phone_number).await,
        TimerType::AdvisorResponse => expire_advisor_timer(state, phone_number).await,
        TimerType::RelayInactivity => expire_relay_timer(state, phone_number).await,
        TimerType::ConversationAbandon => Ok(()),
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
            "Selecciona cómo quieres continuar.",
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

    if !matches!(
        conversation.state.as_str(),
        "wait_advisor_response" | "wait_advisor_mayor" | "wait_advisor_contact"
    ) {
        return Ok(());
    }

    let mut state_data = conversation.state_data.0;
    if state_data.advisor_timer_expired {
        return Ok(());
    }

    state_data.advisor_timer_expired = true;
    state_data.advisor_timer_started_at = None;
    update_state(&state.pool, &phone_number, &conversation.state, &state_data).await?;
    clear_advisor_session(&state).await?;

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
                &client_messages().timers_customer.advisor_timeout_wholesale_text
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
                    &client_messages().timers_customer.advisor_timeout_buttons_body,
                    advisor_timeout_buttons(),
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
            &format!("Relay {} cerrado por inactividad.", phone_marker(&phone_number)),
        )
        .await?;

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
            &client_messages().timers_customer.advisor_timeout_schedule_button,
        ),
        reply_button(
            "advisor_timeout_retry",
            &client_messages().timers_customer.advisor_timeout_retry_button,
        ),
        reply_button(
            "advisor_timeout_menu",
            &client_messages().timers_customer.advisor_timeout_menu_button,
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
            &client_messages().timers_customer.contact_timeout_menu_button,
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
