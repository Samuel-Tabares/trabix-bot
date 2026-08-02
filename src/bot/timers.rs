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
        inactivity::CONVERSATION_REMINDER_TIMEOUT,
        state_machine::{BotAction, ConversationContext, ConversationState, TimerType},
        states::advisor,
    },
    db::{
        models::ConversationStateData,
        queries::{
            get_conversation, list_active_timer_conversations, reset_conversation,
            update_last_message, update_order_status, update_state,
        },
    },
    engine::{
        clear_advisor_session as clear_bound_advisor_session, clear_advisor_threads_for_target,
        send_timer_actions as dispatch_timer_actions,
    },
    logging::mask_phone,
    messages::client_messages,
    AppState,
};

pub type TimerKey = (String, TimerType);
pub type TimerMap = Arc<Mutex<HashMap<TimerKey, ActiveTimer>>>;

pub const RECEIPT_TIMEOUT: Duration = Duration::from_secs(10 * 60);
pub const ADVISOR_RESPONSE_TIMEOUT: Duration = Duration::from_secs(10 * 60);
const TIMER_SWEEP_INTERVAL: Duration = Duration::from_secs(60);
static NEXT_TIMER_INSTANCE_ID: AtomicU64 = AtomicU64::new(1);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TimerRule {
    AdvisorResponse,
    ReceiptUpload,
    ConversationReminder,
}

impl TimerRule {
    pub fn default_duration(&self) -> Duration {
        match self {
            Self::AdvisorResponse => ADVISOR_RESPONSE_TIMEOUT,
            Self::ReceiptUpload => RECEIPT_TIMEOUT,
            Self::ConversationReminder => CONVERSATION_REMINDER_TIMEOUT,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TimerSource {
    Runtime,
    Sweep,
}

impl TimerSource {
    fn as_str(&self) -> &'static str {
        match self {
            Self::Runtime => "runtime",
            Self::Sweep => "sweep",
        }
    }
}

pub struct ActiveTimer {
    token: CancellationToken,
    instance_id: u64,
}

pub fn new_timer_map() -> TimerMap {
    Arc::new(Mutex::new(HashMap::new()))
}

pub fn effective_duration_for_start_timer(
    timer_type: &TimerType,
    requested_duration: Duration,
) -> Duration {
    match timer_rule_for_start_timer(timer_type) {
        Some(rule) => rule.default_duration(),
        None => requested_duration,
    }
}

fn timer_rule_for_start_timer(timer_type: &TimerType) -> Option<TimerRule> {
    match timer_type {
        TimerType::ReceiptUpload => Some(TimerRule::ReceiptUpload),
        TimerType::ConversationAbandon => Some(TimerRule::ConversationReminder),
        TimerType::AdvisorResponse => Some(TimerRule::AdvisorResponse),
        TimerType::RelayInactivity => None,
        // No se arma vía StartTimer: se resuelve puramente por el sweep de
        // 60s reconsultando check_business_hours (ver `timer_recovery`), no
        // hay una duración fija que trackear.
        TimerType::BusinessHoursReopen => None,
    }
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
                tracing::info!(
                    phone = %mask_phone(&conversation.phone_number),
                    timer_type = %timer_type.as_str(),
                    state = %conversation.state,
                    source = "boot_reconcile",
                    "reconciling overdue timer on boot"
                );
                reconcile_boot_expired_timer(state.clone(), &conversation, timer_type).await?;
            }
            Some(TimerRecovery::Active {
                timer_type,
                timeout,
                started_at,
            }) => {
                tracing::info!(
                    phone = %mask_phone(&conversation.phone_number),
                    timer_type = %timer_type.as_str(),
                    state = %conversation.state,
                    timeout_secs = timeout.as_secs(),
                    source = "boot_restore",
                    "restoring active timer on boot"
                );
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

#[derive(Debug, Clone, PartialEq, Eq)]
enum BootExpirationAction {
    UpdateReceiptExpired,
    UpdateAdvisorExpiredAndClearSession,
    ResetConversation {
        clear_advisor_session: bool,
        mark_manual_followup: bool,
    },
    MarkInactivityReminderSilently,
    None,
}

async fn reconcile_boot_expired_timer(
    state: AppState,
    conversation: &crate::db::queries::ActiveTimerConversation,
    timer_type: TimerType,
) -> Result<(), sqlx::Error> {
    // Caso borde de un redeploy en medio de una toma de control humana activa
    // (Fase 2): sin este guard, un timer vencido podría resetear la
    // conversación o mandarle algo al cliente saltándose la pausa. Se difiere:
    // el próximo boot/sweep lo vuelve a evaluar cuando ya no aplique.
    if human_takeover_active(conversation.human_takeover_until) {
        return Ok(());
    }

    match boot_expiration_action(conversation, timer_type.clone()) {
        BootExpirationAction::UpdateReceiptExpired => {
            let mut state_data = conversation.state_data.0.clone();
            state_data.receipt_timer_expired = true;
            update_state(
                &state.pool,
                &conversation.phone_number,
                "wait_receipt",
                &state_data,
            )
            .await?;
        }
        BootExpirationAction::UpdateAdvisorExpiredAndClearSession => {
            let mut state_data = conversation.state_data.0.clone();
            state_data.advisor_timer_expired = true;
            state_data.advisor_timer_started_at = None;
            clear_bound_advisor_session(&state, &state.config.advisor_phone).await?;
            update_state(
                &state.pool,
                &conversation.phone_number,
                &conversation.state,
                &state_data,
            )
            .await?;
        }
        BootExpirationAction::ResetConversation {
            clear_advisor_session,
            mark_manual_followup,
        } => {
            if mark_manual_followup {
                if let Some(order_id) = conversation.state_data.0.current_order_id {
                    update_order_status(&state.pool, order_id, "manual_followup").await?;
                }
            }

            reset_conversation(&state.pool, &conversation.phone_number).await?;
            clear_advisor_threads_for_target(&state, &conversation.phone_number).await?;

            if clear_advisor_session {
                clear_bound_advisor_session(&state, &state.config.advisor_phone).await?;
            }
        }
        BootExpirationAction::MarkInactivityReminderSilently => {
            let mut state_data = conversation.state_data.0.clone();
            state_data.conversation_abandon_reminder_sent = true;
            update_state(
                &state.pool,
                &conversation.phone_number,
                &conversation.state,
                &state_data,
            )
            .await?;
        }
        BootExpirationAction::None => {}
    }

    Ok(())
}

pub async fn sweep_expired_timers(state: AppState) -> Result<(), sqlx::Error> {
    let recovery_states = timer_recovery_states();
    let conversations = list_active_timer_conversations(&state.pool, &recovery_states).await?;

    for conversation in conversations {
        if let Some(TimerRecovery::Expired(timer_type)) =
            timer_recovery(&conversation, Utc::now())
        {
            tracing::info!(
                phone = %mask_phone(&conversation.phone_number),
                timer_type = %timer_type.as_str(),
                state = %conversation.state,
                source = "sweep",
                "found overdue timer during sweep"
            );
            expire_timer_now(
                state.clone(),
                conversation.phone_number.clone(),
                timer_type,
                TimerSource::Sweep,
            )
            .await;
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
        expire_timer_now(state, phone_number, timer_type, TimerSource::Runtime).await;
        return;
    }

    let remaining = timeout - elapsed;
    let app_state = state.clone();
    let phone = phone_number.clone();
    let kind = timer_type.clone();
    tracing::info!(
        phone = %mask_phone(&phone_number),
        timer_type = %timer_type.as_str(),
        remaining_secs = remaining.as_secs(),
        "restored runtime timer"
    );

    start_timer(
        state.timers.clone(),
        (phone_number, timer_type),
        remaining,
        move || {
            let app_state = app_state.clone();
            let phone = phone.clone();
            let kind = kind.clone();
            Box::pin(async move {
                expire_timer_now(app_state, phone, kind, TimerSource::Runtime).await;
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
        let Some(started_at) = state_data.conversation_abandon_started_at else {
            return None;
        };
        if state_data.conversation_abandon_reminder_sent {
            return None;
        }
        let timeout = TimerRule::ConversationReminder.default_duration();

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
            TimerRule::ReceiptUpload.default_duration(),
            state_data
                .receipt_timer_started_at
                .unwrap_or(conversation.last_message_at),
            now,
        ),
        // Sin duración que trackear: cada tick del sweep vuelve a preguntar
        // si el horario ya abrió. No usa `timer_recovery_for` (no hay
        // `started_at` relevante) — el pedido puede esperar horas, no hay
        // "vencido por inactividad" aquí.
        "wait_business_hours" => {
            if crate::ai::tools::check_business_hours().is_open {
                Some(TimerRecovery::Expired(TimerType::BusinessHoursReopen))
            } else {
                None
            }
        }
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

fn boot_expiration_action(
    conversation: &crate::db::queries::ActiveTimerConversation,
    timer_type: TimerType,
) -> BootExpirationAction {
    let state_data = &conversation.state_data.0;

    match timer_type {
        TimerType::ReceiptUpload => {
            if conversation.state == "wait_receipt" && !state_data.receipt_timer_expired {
                BootExpirationAction::UpdateReceiptExpired
            } else {
                BootExpirationAction::None
            }
        }
        TimerType::AdvisorResponse => {
            if state_data.advisor_timer_expired {
                return BootExpirationAction::None;
            }

            match advisor_timeout_kind(conversation.state.as_str()) {
                Some(AdvisorTimeoutKind::AutoCannot) => {
                    BootExpirationAction::UpdateAdvisorExpiredAndClearSession
                }
                Some(AdvisorTimeoutKind::FallbackButtons) => {
                    BootExpirationAction::UpdateAdvisorExpiredAndClearSession
                }
                Some(AdvisorTimeoutKind::HardReset) => BootExpirationAction::ResetConversation {
                    clear_advisor_session: true,
                    mark_manual_followup: true,
                },
                None => BootExpirationAction::None,
            }
        }
        TimerType::RelayInactivity => {
            if conversation.state == "relay_mode" {
                BootExpirationAction::ResetConversation {
                    clear_advisor_session: true,
                    mark_manual_followup: false,
                }
            } else {
                BootExpirationAction::None
            }
        }
        TimerType::ConversationAbandon => {
            if !customer_inactivity_state(conversation.state.as_str()) {
                return BootExpirationAction::None;
            }

            if state_data.conversation_abandon_started_at.is_none()
                || state_data.conversation_abandon_reminder_sent
            {
                return BootExpirationAction::None;
            }

            // La ventana del recordatorio venció mientras el bot estaba
            // apagado: se marca en silencio (en boot no se envían mensajes)
            // y no hay reset por inactividad.
            BootExpirationAction::MarkInactivityReminderSilently
        }
        // No-op a propósito: en boot no se envían mensajes reales (ver los
        // otros brazos), y acá SÍ hace falta mandar mensajes de verdad
        // (avisarle al cliente que su pedido quedó confirmado). Se deja para
        // el próximo tick del sweep normal (`sweep_expired_timers`, cada
        // 60s), que ya corre apenas arranca el proceso.
        TimerType::BusinessHoursReopen => BootExpirationAction::None,
    }
}

async fn expire_timer_now(
    state: AppState,
    phone_number: String,
    timer_type: TimerType,
    source: TimerSource,
) {
    tracing::info!(
        phone = %mask_phone(&phone_number),
        timer_type = %timer_type.as_str(),
        source = %source.as_str(),
        "expiring timer now"
    );
    let result = match timer_type {
        TimerType::ReceiptUpload => {
            expire_receipt_timer_with_source(state, phone_number, source).await
        }
        TimerType::AdvisorResponse => {
            expire_advisor_timer_with_source(state, phone_number, source).await
        }
        TimerType::RelayInactivity => {
            expire_relay_timer_with_source(state, phone_number, source).await
        }
        TimerType::ConversationAbandon => {
            expire_conversation_abandon_with_source(state, phone_number, source).await
        }
        TimerType::BusinessHoursReopen => {
            expire_business_hours_timer_with_source(state, phone_number, source).await
        }
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
    expire_receipt_timer_with_source(state, phone_number, TimerSource::Runtime).await
}

async fn expire_receipt_timer_with_source(
    state: AppState,
    phone_number: String,
    source: TimerSource,
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
    tracing::info!(
        phone = %mask_phone(&phone_number),
        timer_type = %TimerType::ReceiptUpload.as_str(),
        source = %source.as_str(),
        "receipt timer expired"
    );
    // El texto ya describe las opciones y la respuesta la interpreta el LLM
    // (ver docs/canary-fixes-2026-07-19.md item 3) — no se mandan botones.
    let actions = vec![BotAction::SendText {
        to: phone_number.clone(),
        body: client_messages()
            .timers_customer
            .receipt_timeout_text
            .clone(),
    }];
    dispatch_timer_actions(&state, &phone_number, &actions).await?;

    Ok(())
}

pub async fn expire_advisor_timer(
    state: AppState,
    phone_number: String,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    expire_advisor_timer_with_source(state, phone_number, TimerSource::Runtime).await
}

async fn expire_advisor_timer_with_source(
    state: AppState,
    phone_number: String,
    source: TimerSource,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let Some(conversation) = get_conversation(&state.pool, &phone_number).await? else {
        return Ok(());
    };

    if human_takeover_active(conversation.human_takeover_until) {
        return Ok(());
    }

    let Some(timeout_kind) = advisor_timeout_kind(conversation.state.as_str()) else {
        return Ok(());
    };

    let mut state_data = conversation.state_data.0;
    if state_data.advisor_timer_expired {
        return Ok(());
    }

    clear_bound_advisor_session(&state, &state.config.advisor_phone).await?;

    match timeout_kind {
        AdvisorTimeoutKind::AutoCannot => {
            state_data.advisor_timer_expired = true;
            state_data.delivery_type = Some("scheduled".to_string());
            state_data.scheduled_date = Some(today_bogota_iso_date());
            state_data.scheduled_time = None;
            state_data.advisor_proposed_hour = None;
            state_data.client_counter_hour = None;
            state_data.advisor_timer_started_at = Some(Utc::now());
            state_data.advisor_timer_expired = false;

            tracing::info!(
                phone = %mask_phone(&phone_number),
                timer_type = %TimerType::AdvisorResponse.as_str(),
                timeout_kind = "auto_no_puedo",
                state = %conversation.state,
                source = %source.as_str(),
                "advisor timer auto-transitioned immediate order"
            );

            bind_advisor_session_for_timer(
                &state,
                &state.config.advisor_phone,
                &conversation.phone_number,
            )
            .await?;
            update_state(&state.pool, &phone_number, "negotiate_hour", &state_data).await?;
            update_last_message(&state.pool, &phone_number).await?;
            dispatch_timer_actions(
                &state,
                &phone_number,
                &[
                    BotAction::SendText {
                        to: state.config.advisor_phone.clone(),
                        body: format!(
                            "Pedido {} quedó como programado para hoy. ¿Qué hora puede proponer?",
                            advisor::phone_marker(&phone_number)
                        ),
                    },
                    BotAction::SendText {
                        to: phone_number.clone(),
                        body: client_messages()
                            .advisor_customer
                            .wait_negotiate_hour_text
                            .clone(),
                    },
                ],
            )
            .await?;
        }
        AdvisorTimeoutKind::FallbackButtons => {
            state_data.advisor_timer_expired = true;
            state_data.advisor_timer_started_at = None;
            update_state(&state.pool, &phone_number, &conversation.state, &state_data).await?;
            tracing::info!(
                phone = %mask_phone(&phone_number),
                timer_type = %TimerType::AdvisorResponse.as_str(),
                timeout_kind = "fallback_buttons",
                state = %conversation.state,
                source = %source.as_str(),
                "advisor timer expired"
            );

            match conversation.state.as_str() {
                "wait_advisor_contact" => {
                    // Solo texto: la respuesta la interpreta el LLM (motor agente).
                    let action = BotAction::SendText {
                        to: phone_number.clone(),
                        body: client_messages().timers_customer.contact_timeout_body.clone(),
                    };
                    dispatch_timer_actions(&state, &phone_number, &[action]).await?;
                }
                _ => {
                    let timeout_text = if conversation.state == "wait_advisor_mayor" {
                        &client_messages()
                            .timers_customer
                            .advisor_timeout_wholesale_text
                    } else {
                        &client_messages().timers_customer.advisor_timeout_text
                    };
                    let actions = vec![BotAction::SendText {
                        to: phone_number.clone(),
                        body: timeout_text.clone(),
                    }];
                    dispatch_timer_actions(&state, &phone_number, &actions).await?;
                }
            }
        }
        AdvisorTimeoutKind::HardReset => {
            if let Some(order_id) = state_data.current_order_id {
                update_order_status(&state.pool, order_id, "manual_followup").await?;
            }

            reset_conversation(&state.pool, &phone_number).await?;
            clear_advisor_threads_for_target(&state, &phone_number).await?;
            tracing::info!(
                phone = %mask_phone(&phone_number),
                timer_type = %TimerType::AdvisorResponse.as_str(),
                timeout_kind = "hard_reset",
                order_id = ?state_data.current_order_id,
                source = %source.as_str(),
                "advisor stuck timer reset conversation"
            );
            dispatch_timer_actions(
                &state,
                &phone_number,
                &[BotAction::SendText {
                    to: phone_number.clone(),
                    body: client_messages()
                        .timers_customer
                        .advisor_stuck_timeout_text
                        .clone(),
                }],
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
    expire_relay_timer_with_source(state, phone_number, TimerSource::Runtime).await
}

async fn expire_relay_timer_with_source(
    state: AppState,
    phone_number: String,
    source: TimerSource,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let Some(conversation) = get_conversation(&state.pool, &phone_number).await? else {
        return Ok(());
    };

    if human_takeover_active(conversation.human_takeover_until) {
        return Ok(());
    }

    if conversation.state != "relay_mode" {
        return Ok(());
    }

    reset_conversation(&state.pool, &phone_number).await?;
    clear_advisor_threads_for_target(&state, &phone_number).await?;
    clear_bound_advisor_session(&state, &state.config.advisor_phone).await?;
    tracing::info!(
        phone = %mask_phone(&phone_number),
        timer_type = %TimerType::RelayInactivity.as_str(),
        source = %source.as_str(),
        "relay timer expired"
    );
    dispatch_timer_actions(
        &state,
        &phone_number,
        &[
            BotAction::SendText {
                to: phone_number.clone(),
                body: client_messages().timers_customer.relay_timeout_text.clone(),
            },
            BotAction::SendText {
                to: state.config.advisor_phone.clone(),
                body: format!(
                    "Relay {} cerrado por inactividad.",
                    phone_marker(&phone_number)
                ),
            },
        ],
    )
    .await?;

    Ok(())
}

pub async fn expire_conversation_abandon(
    state: AppState,
    phone_number: String,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    expire_conversation_abandon_with_source(state, phone_number, TimerSource::Runtime).await
}

async fn expire_conversation_abandon_with_source(
    state: AppState,
    phone_number: String,
    source: TimerSource,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let Some(conversation) = get_conversation(&state.pool, &phone_number).await? else {
        return Ok(());
    };

    if human_takeover_active(conversation.human_takeover_until) {
        return Ok(());
    }

    if !customer_inactivity_state(&conversation.state) {
        return Ok(());
    }

    let mut state_data = conversation.state_data.0;
    let Some(started_at) = state_data.conversation_abandon_started_at else {
        return Ok(());
    };

    // Recordatorio una sola vez; después el bot sigue esperando input sin
    // resetear la conversación (FASE 5: no existe reset por inactividad).
    if !state_data.conversation_abandon_reminder_sent {
        // Texto suave y neutro: la conversación la retoma el LLM, que no debe
        // recibir botones/listas reinyectados (ver docs/canary-fixes-2026-07-19.md item 3).
        let actions = vec![BotAction::SendText {
            to: phone_number.clone(),
            body: client_messages()
                .timers_customer
                .agent_inactivity_nudge_text
                .clone(),
        }];
        tracing::info!(
            phone = %mask_phone(&phone_number),
            timer_type = %TimerType::ConversationAbandon.as_str(),
            state = %conversation.state,
            source = %source.as_str(),
            "sending inactivity reminder"
        );
        dispatch_timer_actions(&state, &phone_number, &actions).await?;

        state_data.conversation_abandon_started_at = Some(started_at);
        state_data.conversation_abandon_reminder_sent = true;
        update_state(&state.pool, &phone_number, &conversation.state, &state_data).await?;
    }

    Ok(())
}

/// No se arma vía `BotAction::StartTimer` (`BusinessHoursReopen` no tiene
/// duración fija, ver `timer_rule_for_start_timer`), pero se expone igual con
/// el mismo shape que los demás `expire_*` para que `engine.rs` pueda cubrir
/// el `match` exhaustivo de `TimerType` sin una rama muerta.
pub async fn expire_business_hours_timer(
    state: AppState,
    phone_number: String,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    expire_business_hours_timer_with_source(state, phone_number, TimerSource::Runtime).await
}

/// Reabrimos apenas el sweep detecta que volvió a abrir (`timer_recovery`,
/// caso `wait_business_hours`). Reusa `auto_accept_order_actions` (agent.rs)
/// -- la MISMA lógica que usa el tool-call del agente para no duplicarla --
/// si el domicilio ya se conocía; si el pueblo seguía sin resolver, pasa a
/// pedir el costo con el timer normal de 10 min (ahora sí hay atención real).
async fn expire_business_hours_timer_with_source(
    state: AppState,
    phone_number: String,
    source: TimerSource,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let Some(conversation) = get_conversation(&state.pool, &phone_number).await? else {
        return Ok(());
    };

    if human_takeover_active(conversation.human_takeover_until) {
        return Ok(());
    }

    if conversation.state != "wait_business_hours" {
        return Ok(());
    }

    // Guard contra condición de carrera al filo del horario: si para cuando
    // corre esto ya volvió a cerrar, no hacer nada -- el próximo tick lo
    // vuelve a intentar.
    if !crate::ai::tools::check_business_hours().is_open {
        return Ok(());
    }

    let state_data = conversation.state_data.0.clone();
    let mut context = rehydrate_context_for_timer(
        phone_number.clone(),
        state.config.advisor_phone.clone(),
        conversation.customer_name.clone(),
        conversation.customer_phone.clone(),
        conversation.delivery_address.clone(),
        &state_data,
    );

    tracing::info!(
        phone = %mask_phone(&phone_number),
        timer_type = %TimerType::BusinessHoursReopen.as_str(),
        source = %source.as_str(),
        "business hours reopened, resolving waiting order"
    );

    let (next_state, actions) = if let Some(delivery_cost) = context.delivery_cost {
        let (_total_final, mut accept_actions) =
            crate::ai::agent::auto_accept_order_actions(&mut context, delivery_cost);
        accept_actions.push(BotAction::SendText {
            to: phone_number.clone(),
            body: "✅ ¡Ya abrimos! Tu pedido quedó confirmado. Cuéntanos cómo prefieres pagar \
                   (efectivo contra entrega o transferencia) y seguimos."
                .to_string(),
        });
        (ConversationState::SelectPaymentMethod, accept_actions)
    } else {
        context.advisor_timer_started_at = Some(Utc::now());
        context.advisor_timer_expired = false;
        let ask_cost_actions = vec![
            BotAction::StartTimer {
                timer_type: TimerType::AdvisorResponse,
                phone: phone_number.clone(),
                duration: ADVISOR_RESPONSE_TIMEOUT,
            },
            BotAction::SendText {
                to: state.config.advisor_phone.clone(),
                body: format!(
                    "☀️ Ya abrimos. El pedido inmediato {} sigue esperando el costo de domicilio \
                     (municipio/zona desconocida) — contesta con el valor.",
                    phone_marker(&phone_number)
                ),
            },
            BotAction::SendText {
                to: phone_number.clone(),
                body: "✅ ¡Ya abrimos! Estamos confirmando el valor del domicilio para tu \
                       pedido, en un momento te decimos el total."
                    .to_string(),
            },
        ];
        (ConversationState::AskDeliveryCost, ask_cost_actions)
    };

    update_state(
        &state.pool,
        &phone_number,
        next_state.as_storage_key(),
        &context.to_state_data(),
    )
    .await?;
    dispatch_timer_actions(&state, &phone_number, &actions).await?;

    Ok(())
}

fn timer_recovery_states() -> Vec<&'static str> {
    vec![
        "wait_receipt",
        "wait_advisor_response",
        "wait_advisor_mayor",
        "wait_advisor_contact",
        "ask_delivery_cost",
        "wait_business_hours",
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
        "select_customer_data_field",
        "edit_customer_name",
        "edit_customer_phone",
        "edit_customer_address",
        "review_checkout",
        "select_payment_method",
        "offer_hour_to_client",
        "wait_client_hour",
        "contact_advisor_name",
        "contact_advisor_phone",
        "leave_message",
    ]
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AdvisorTimeoutKind {
    AutoCannot,
    FallbackButtons,
    HardReset,
}

/// Fase 2: si un asesor tomó el caso desde `crm-app` (`set_human_takeover`),
/// ningún timer de este archivo debe dispararle nada al cliente hasta que
/// venza `until`. Compartido por los 4 `expire_*_with_source` y por la
/// reconciliación de arranque.
fn human_takeover_active(until: Option<DateTime<Utc>>) -> bool {
    until.is_some_and(|until| until > Utc::now())
}

fn advisor_timeout_kind(state: &str) -> Option<AdvisorTimeoutKind> {
    match state {
        "wait_advisor_response" => Some(AdvisorTimeoutKind::AutoCannot),
        "wait_advisor_mayor" | "wait_advisor_contact" => Some(AdvisorTimeoutKind::FallbackButtons),
        "ask_delivery_cost"
        | "negotiate_hour"
        | "wait_advisor_hour_decision"
        | "wait_advisor_confirm_hour" => Some(AdvisorTimeoutKind::HardReset),
        _ => None,
    }
}

fn advisor_timeout_for_state(state: &str) -> Option<Duration> {
    advisor_timeout_kind(state).map(|_| TimerRule::AdvisorResponse.default_duration())
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
            | "select_customer_data_field"
            | "edit_customer_name"
            | "edit_customer_phone"
            | "edit_customer_address"
            | "review_checkout"
            | "select_payment_method"
            | "offer_hour_to_client"
            | "wait_client_hour"
            | "contact_advisor_name"
            | "contact_advisor_phone"
            | "leave_message"
    )
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

async fn bind_advisor_session_for_timer(
    state: &AppState,
    advisor_phone: &str,
    target_phone: &str,
) -> Result<(), sqlx::Error> {
    if let Some(conversation) = get_conversation(&state.pool, advisor_phone).await? {
        let mut state_data = conversation.state_data.0;
        state_data.advisor_target_phone = Some(target_phone.to_string());
        update_state(&state.pool, advisor_phone, &conversation.state, &state_data).await?;
    }

    Ok(())
}

fn phone_marker(phone: &str) -> String {
    let suffix = if phone.len() >= 4 {
        &phone[phone.len() - 4..]
    } else {
        phone
    };
    format!("[...{suffix}]")
}

fn today_bogota_iso_date() -> String {
    let offset = chrono::FixedOffset::west_opt(5 * 3600).expect("valid Bogota offset");
    Utc::now()
        .with_timezone(&offset)
        .date_naive()
        .format("%Y-%m-%d")
        .to_string()
}

#[cfg(test)]
mod tests {
    use chrono::Duration as ChronoDuration;
    use sqlx::types::Json;

    use super::{
        boot_expiration_action, human_takeover_active, timer_recovery, BootExpirationAction,
        TimerRecovery, ADVISOR_RESPONSE_TIMEOUT,
    };
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
            human_takeover_until: None,
        }
    }

    #[test]
    fn timer_recovery_uses_last_message_when_start_timestamp_is_missing() {
        let now = chrono::Utc::now();
        let conversation = active_timer_conversation(
            "wait_advisor_response",
            ConversationStateData::default(),
            now - ChronoDuration::minutes(3),
        );

        let recovery =
            timer_recovery(&conversation, now);

        assert_eq!(
            recovery,
            Some(TimerRecovery::Active {
                timer_type: TimerType::AdvisorResponse,
                timeout: ADVISOR_RESPONSE_TIMEOUT,
                started_at: now - ChronoDuration::minutes(3),
            })
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

        let recovery =
            timer_recovery(&conversation, now);

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

        let recovery =
            timer_recovery(&conversation, now);

        assert_eq!(
            recovery,
            Some(TimerRecovery::Expired(TimerType::ConversationAbandon))
        );
    }

    #[test]
    fn timer_recovery_does_not_arm_customer_inactivity_without_timestamp() {
        let now = chrono::Utc::now();
        let conversation =
            active_timer_conversation("main_menu", ConversationStateData::default(), now);

        let recovery = timer_recovery(&conversation, now + ChronoDuration::minutes(40));

        assert!(recovery.is_none());
    }

    #[test]
    fn timer_recovery_ignores_reset_main_menu_even_with_stale_last_message() {
        let now = chrono::Utc::now();
        let conversation = active_timer_conversation(
            "main_menu",
            ConversationStateData::default(),
            now - ChronoDuration::minutes(40),
        );

        let recovery =
            timer_recovery(&conversation, now);

        assert!(recovery.is_none());
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

        let recovery =
            timer_recovery(&conversation, now);

        assert_eq!(
            recovery,
            Some(TimerRecovery::Active {
                timer_type: TimerType::AdvisorResponse,
                timeout: ADVISOR_RESPONSE_TIMEOUT,
                started_at: now - ChronoDuration::minutes(3),
            })
        );
    }

    #[test]
    fn timer_recovery_uses_unified_timeout_for_all_advisor_response() {
        let now = chrono::Utc::now();
        let conversation = active_timer_conversation(
            "ask_delivery_cost",
            ConversationStateData {
                delivery_type: Some("scheduled".to_string()),
                advisor_timer_started_at: Some(now - ChronoDuration::minutes(2)),
                ..Default::default()
            },
            now,
        );

        let recovery =
            timer_recovery(&conversation, now);

        assert_eq!(
            recovery,
            Some(TimerRecovery::Active {
                timer_type: TimerType::AdvisorResponse,
                timeout: ADVISOR_RESPONSE_TIMEOUT,
                started_at: now - ChronoDuration::minutes(2),
            })
        );
    }

    #[test]
    fn timer_recovery_keeps_immediate_delivery_cost_on_short_stuck_timeout() {
        let now = chrono::Utc::now();
        let conversation = active_timer_conversation(
            "ask_delivery_cost",
            ConversationStateData {
                delivery_type: Some("immediate".to_string()),
                advisor_timer_started_at: Some(now - ChronoDuration::minutes(3)),
                ..Default::default()
            },
            now,
        );

        let recovery =
            timer_recovery(&conversation, now);

        assert_eq!(
            recovery,
            Some(TimerRecovery::Active {
                timer_type: TimerType::AdvisorResponse,
                timeout: ADVISOR_RESPONSE_TIMEOUT,
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

        let recovery =
            timer_recovery(&conversation, now);

        assert_eq!(
            recovery,
            Some(TimerRecovery::Expired(TimerType::AdvisorResponse))
        );
    }

    #[test]
    fn timer_recovery_stops_after_reminder_sent() {
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

        let recovery =
            timer_recovery(&conversation, now);

        assert_eq!(recovery, None);
    }

    #[test]
    fn boot_expiration_marks_receipt_timeout_without_sending() {
        let now = chrono::Utc::now();
        let conversation =
            active_timer_conversation("wait_receipt", ConversationStateData::default(), now);

        let action = boot_expiration_action(&conversation, TimerType::ReceiptUpload);

        assert_eq!(action, BootExpirationAction::UpdateReceiptExpired);
    }

    #[test]
    fn boot_expiration_marks_advisor_timeout_without_sending() {
        let now = chrono::Utc::now();
        let conversation = active_timer_conversation(
            "wait_advisor_response",
            ConversationStateData::default(),
            now,
        );

        let action = boot_expiration_action(&conversation, TimerType::AdvisorResponse);

        assert_eq!(
            action,
            BootExpirationAction::UpdateAdvisorExpiredAndClearSession
        );
    }

    #[test]
    fn boot_expiration_resets_stuck_advisor_silently() {
        let now = chrono::Utc::now();
        let conversation = active_timer_conversation(
            "ask_delivery_cost",
            ConversationStateData {
                current_order_id: Some(42),
                ..Default::default()
            },
            now,
        );

        let action = boot_expiration_action(&conversation, TimerType::AdvisorResponse);

        assert_eq!(
            action,
            BootExpirationAction::ResetConversation {
                clear_advisor_session: true,
                mark_manual_followup: true,
            }
        );
    }

    #[test]
    fn boot_expiration_resets_relay_silently() {
        let now = chrono::Utc::now();
        let conversation =
            active_timer_conversation("relay_mode", ConversationStateData::default(), now);

        let action = boot_expiration_action(&conversation, TimerType::RelayInactivity);

        assert_eq!(
            action,
            BootExpirationAction::ResetConversation {
                clear_advisor_session: true,
                mark_manual_followup: false,
            }
        );
    }

    #[test]
    fn boot_expiration_ignores_customer_after_reminder_sent() {
        let now = chrono::Utc::now();
        let conversation = active_timer_conversation(
            "collect_phone",
            ConversationStateData {
                conversation_abandon_started_at: Some(now - ChronoDuration::minutes(40)),
                conversation_abandon_reminder_sent: true,
                ..Default::default()
            },
            now,
        );

        let action = boot_expiration_action(&conversation, TimerType::ConversationAbandon);

        assert_eq!(action, BootExpirationAction::None);
    }

    #[test]
    fn boot_expiration_marks_pending_reminder_silently() {
        let now = chrono::Utc::now();
        let conversation = active_timer_conversation(
            "collect_phone",
            ConversationStateData {
                conversation_abandon_started_at: Some(now - ChronoDuration::minutes(10)),
                conversation_abandon_reminder_sent: false,
                ..Default::default()
            },
            now,
        );

        let action = boot_expiration_action(&conversation, TimerType::ConversationAbandon);

        assert_eq!(action, BootExpirationAction::MarkInactivityReminderSilently);
    }

    // Depende de la hora real de Bogotá (sin seam de reloj, mismo patrón que
    // las pruebas de `tools::check_business_hours` en agent.rs) — toma una
    // foto de `is_open` y verifica que el resultado sea consistente con ella,
    // en vez de asumir un valor fijo.
    #[test]
    fn timer_recovery_wait_business_hours_matches_check_business_hours() {
        let now = chrono::Utc::now();
        let conversation =
            active_timer_conversation("wait_business_hours", ConversationStateData::default(), now);
        let is_open = crate::ai::tools::check_business_hours().is_open;

        let recovery = timer_recovery(&conversation, now);

        if is_open {
            assert_eq!(
                recovery,
                Some(TimerRecovery::Expired(TimerType::BusinessHoursReopen))
            );
        } else {
            assert_eq!(recovery, None);
        }
    }

    #[test]
    fn boot_expiration_business_hours_reopen_is_a_noop() {
        let now = chrono::Utc::now();
        let conversation =
            active_timer_conversation("wait_business_hours", ConversationStateData::default(), now);

        let action = boot_expiration_action(&conversation, TimerType::BusinessHoursReopen);

        assert_eq!(action, BootExpirationAction::None);
    }

    #[test]
    fn human_takeover_active_only_while_until_is_in_the_future() {
        let now = chrono::Utc::now();
        assert!(!human_takeover_active(None));
        assert!(human_takeover_active(Some(now + ChronoDuration::hours(1))));
        assert!(!human_takeover_active(Some(now - ChronoDuration::minutes(1))));
    }
}
