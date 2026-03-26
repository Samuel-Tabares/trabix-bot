use std::{
    collections::HashMap,
    future::Future,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc, RwLock,
    },
    time::Duration,
};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::json;
use tokio::sync::Mutex;
use tokio::time::{interval, MissedTickBehavior};
use tokio_util::sync::CancellationToken;

use crate::{
    bot::{
        inactivity::{
            reminder_actions, reset_notice_actions, CONVERSATION_REMINDER_TIMEOUT,
            CONVERSATION_RESET_TIMEOUT,
        },
        state_machine::{BotAction, ConversationContext, ConversationState, TimerType},
    },
    db::{
        models::ConversationStateData,
        queries::{
            get_conversation, list_active_timer_conversations, reset_conversation,
            update_order_status, update_state,
        },
    },
    engine::{
        clear_advisor_session as clear_bound_advisor_session,
        send_timer_actions as dispatch_timer_actions,
    },
    logging::mask_phone,
    messages::client_messages,
    simulator::{create_message, get_session_by_phone, NewSimulatorMessage},
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

pub type TimerOverridesHandle = Arc<RwLock<SimulatorTimerOverrides>>;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TimerRule {
    AdvisorResponse,
    ReceiptUpload,
    AdvisorStuck,
    RelayInactivity,
    ConversationReminder,
    ConversationReset,
}

impl TimerRule {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::AdvisorResponse => "advisor_response",
            Self::ReceiptUpload => "receipt_upload",
            Self::AdvisorStuck => "advisor_stuck",
            Self::RelayInactivity => "relay_inactivity",
            Self::ConversationReminder => "conversation_reminder",
            Self::ConversationReset => "conversation_reset",
        }
    }

    pub fn label(&self) -> &'static str {
        match self {
            Self::AdvisorResponse => "Asesor sin responder",
            Self::ReceiptUpload => "Espera de comprobante",
            Self::AdvisorStuck => "Asesor atascado",
            Self::RelayInactivity => "Relay inactivo",
            Self::ConversationReminder => "Recordatorio por inactividad",
            Self::ConversationReset => "Reinicio por inactividad",
        }
    }

    pub fn default_duration(&self) -> Duration {
        match self {
            Self::AdvisorResponse => ADVISOR_RESPONSE_TIMEOUT,
            Self::ReceiptUpload => RECEIPT_TIMEOUT,
            Self::AdvisorStuck => ADVISOR_STUCK_TIMEOUT,
            Self::RelayInactivity => RELAY_INACTIVITY_TIMEOUT,
            Self::ConversationReminder => CONVERSATION_REMINDER_TIMEOUT,
            Self::ConversationReset => CONVERSATION_RESET_TIMEOUT,
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SimulatorTimerOverrides {
    pub advisor_response_seconds: Option<u64>,
    pub receipt_upload_seconds: Option<u64>,
    pub advisor_stuck_seconds: Option<u64>,
    pub relay_inactivity_seconds: Option<u64>,
    pub conversation_reminder_seconds: Option<u64>,
    pub conversation_reset_seconds: Option<u64>,
}

impl SimulatorTimerOverrides {
    pub fn seconds_for(&self, rule: TimerRule) -> Option<u64> {
        match rule {
            TimerRule::AdvisorResponse => self.advisor_response_seconds,
            TimerRule::ReceiptUpload => self.receipt_upload_seconds,
            TimerRule::AdvisorStuck => self.advisor_stuck_seconds,
            TimerRule::RelayInactivity => self.relay_inactivity_seconds,
            TimerRule::ConversationReminder => self.conversation_reminder_seconds,
            TimerRule::ConversationReset => self.conversation_reset_seconds,
        }
    }

    fn duration_for(&self, rule: TimerRule) -> Option<Duration> {
        self.seconds_for(rule).map(Duration::from_secs)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SimulatorTimerRuleInfo {
    pub key: String,
    pub label: String,
    pub default_seconds: u64,
    pub override_seconds: Option<u64>,
    pub effective_seconds: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SimulatorTimerSnapshot {
    pub timer_type: String,
    pub rule_key: String,
    pub label: String,
    pub phase: String,
    pub state: String,
    pub started_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
    pub effective_seconds: i64,
    pub remaining_seconds: i64,
    pub expired: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TimerSource {
    Runtime,
    Sweep,
    BootReconcile,
}

impl TimerSource {
    fn as_str(&self) -> &'static str {
        match self {
            Self::Runtime => "runtime",
            Self::Sweep => "sweep",
            Self::BootReconcile => "boot_reconcile",
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

pub fn new_timer_overrides() -> TimerOverridesHandle {
    Arc::new(RwLock::new(SimulatorTimerOverrides::default()))
}

pub fn simulator_timer_rules(state: &AppState) -> Vec<SimulatorTimerRuleInfo> {
    let overrides = state
        .timer_overrides
        .read()
        .expect("timer overrides lock poisoned")
        .clone();

    [
        TimerRule::AdvisorResponse,
        TimerRule::ReceiptUpload,
        TimerRule::AdvisorStuck,
        TimerRule::RelayInactivity,
        TimerRule::ConversationReminder,
        TimerRule::ConversationReset,
    ]
    .into_iter()
    .map(|rule| SimulatorTimerRuleInfo {
        key: rule.as_str().to_string(),
        label: rule.label().to_string(),
        default_seconds: rule.default_duration().as_secs(),
        override_seconds: overrides.seconds_for(rule),
        effective_seconds: effective_duration(state, rule).as_secs(),
    })
    .collect()
}

pub fn update_simulator_timer_overrides(
    state: &AppState,
    overrides: SimulatorTimerOverrides,
) -> Vec<SimulatorTimerRuleInfo> {
    *state
        .timer_overrides
        .write()
        .expect("timer overrides lock poisoned") = overrides;
    simulator_timer_rules(state)
}

pub fn effective_duration_for_start_timer(
    state: &AppState,
    timer_type: &TimerType,
    requested_duration: Duration,
) -> Duration {
    match timer_rule_for_start_timer(timer_type, requested_duration) {
        Some(rule) => effective_duration(state, rule),
        None => requested_duration,
    }
}

pub fn simulator_timer_snapshots(
    state: &AppState,
    conversation: &crate::db::models::Conversation,
    now: DateTime<Utc>,
) -> Vec<SimulatorTimerSnapshot> {
    derive_timer_snapshots(
        state,
        &conversation.state,
        &conversation.state_data.0,
        conversation.last_message_at,
        now,
    )
}

fn effective_duration(state: &AppState, rule: TimerRule) -> Duration {
    if !state.config.mode.is_simulator() {
        return rule.default_duration();
    }

    state
        .timer_overrides
        .read()
        .expect("timer overrides lock poisoned")
        .duration_for(rule)
        .unwrap_or_else(|| rule.default_duration())
}

fn duration_from_overrides(
    is_simulator: bool,
    overrides: &SimulatorTimerOverrides,
    rule: TimerRule,
) -> Duration {
    if is_simulator {
        overrides
            .duration_for(rule)
            .unwrap_or_else(|| rule.default_duration())
    } else {
        rule.default_duration()
    }
}

fn timer_rule_for_start_timer(
    timer_type: &TimerType,
    requested_duration: Duration,
) -> Option<TimerRule> {
    match timer_type {
        TimerType::ReceiptUpload => Some(TimerRule::ReceiptUpload),
        TimerType::RelayInactivity => Some(TimerRule::RelayInactivity),
        TimerType::ConversationAbandon => Some(TimerRule::ConversationReminder),
        TimerType::AdvisorResponse if requested_duration == ADVISOR_STUCK_TIMEOUT => {
            Some(TimerRule::AdvisorStuck)
        }
        TimerType::AdvisorResponse if requested_duration == ADVISOR_RESPONSE_TIMEOUT => {
            Some(TimerRule::AdvisorResponse)
        }
        TimerType::AdvisorResponse => None,
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
        match timer_recovery(&state, &conversation, Utc::now()) {
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
    MarkInactivityReminderAndRestore {
        started_at: DateTime<Utc>,
    },
    None,
}

async fn reconcile_boot_expired_timer(
    state: AppState,
    conversation: &crate::db::queries::ActiveTimerConversation,
    timer_type: TimerType,
) -> Result<(), sqlx::Error> {
    match boot_expiration_action(&state, conversation, timer_type.clone(), Utc::now()) {
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
            record_simulator_timer_notice(
                &state,
                &conversation.phone_number,
                TimerType::ReceiptUpload,
                TimerSource::BootReconcile,
                &conversation.state,
                "marked_expired_silently",
                false,
            )
            .await;
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
            record_simulator_timer_notice(
                &state,
                &conversation.phone_number,
                TimerType::AdvisorResponse,
                TimerSource::BootReconcile,
                &conversation.state,
                "marked_expired_silently",
                false,
            )
            .await;
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

            if clear_advisor_session {
                clear_bound_advisor_session(&state, &state.config.advisor_phone).await?;
            }

            record_simulator_timer_notice(
                &state,
                &conversation.phone_number,
                timer_type,
                TimerSource::BootReconcile,
                &conversation.state,
                "reset_main_menu_silently",
                true,
            )
            .await;
        }
        BootExpirationAction::MarkInactivityReminderAndRestore { started_at } => {
            let mut state_data = conversation.state_data.0.clone();
            state_data.conversation_abandon_started_at = Some(started_at);
            state_data.conversation_abandon_reminder_sent = true;
            update_state(
                &state.pool,
                &conversation.phone_number,
                &conversation.state,
                &state_data,
            )
            .await?;
            restore_timer(
                state.clone(),
                conversation.phone_number.clone(),
                TimerType::ConversationAbandon,
                effective_duration(&state, TimerRule::ConversationReset),
                started_at,
            )
            .await;
            record_simulator_timer_notice(
                &state,
                &conversation.phone_number,
                TimerType::ConversationAbandon,
                TimerSource::BootReconcile,
                &conversation.state,
                "restored_after_silent_reminder",
                false,
            )
            .await;
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
            timer_recovery(&state, &conversation, Utc::now())
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
    state: &AppState,
    conversation: &crate::db::queries::ActiveTimerConversation,
    now: DateTime<Utc>,
) -> Option<TimerRecovery> {
    let overrides = state
        .timer_overrides
        .read()
        .expect("timer overrides lock poisoned")
        .clone();
    timer_recovery_with_overrides(
        state.config.mode.is_simulator(),
        &overrides,
        conversation,
        now,
    )
}

fn timer_recovery_with_overrides(
    is_simulator: bool,
    overrides: &SimulatorTimerOverrides,
    conversation: &crate::db::queries::ActiveTimerConversation,
    now: DateTime<Utc>,
) -> Option<TimerRecovery> {
    let state_data = &conversation.state_data.0;

    if customer_inactivity_state(conversation.state.as_str()) {
        let Some(started_at) = state_data.conversation_abandon_started_at else {
            return None;
        };
        let timeout = if state_data.conversation_abandon_reminder_sent {
            duration_from_overrides(is_simulator, overrides, TimerRule::ConversationReset)
        } else {
            duration_from_overrides(is_simulator, overrides, TimerRule::ConversationReminder)
        };

        return timer_recovery_for(TimerType::ConversationAbandon, timeout, started_at, now);
    }

    if let Some(timeout) = advisor_timeout_for_state_with_overrides(
        is_simulator,
        overrides,
        conversation.state.as_str(),
    ) {
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
            duration_from_overrides(is_simulator, overrides, TimerRule::ReceiptUpload),
            state_data
                .receipt_timer_started_at
                .unwrap_or(conversation.last_message_at),
            now,
        ),
        "relay_mode" => timer_recovery_for(
            TimerType::RelayInactivity,
            duration_from_overrides(is_simulator, overrides, TimerRule::RelayInactivity),
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

fn boot_expiration_action(
    state: &AppState,
    conversation: &crate::db::queries::ActiveTimerConversation,
    timer_type: TimerType,
    now: DateTime<Utc>,
) -> BootExpirationAction {
    let overrides = state
        .timer_overrides
        .read()
        .expect("timer overrides lock poisoned")
        .clone();
    boot_expiration_action_with_overrides(
        state.config.mode.is_simulator(),
        &overrides,
        conversation,
        timer_type,
        now,
    )
}

fn boot_expiration_action_with_overrides(
    is_simulator: bool,
    overrides: &SimulatorTimerOverrides,
    conversation: &crate::db::queries::ActiveTimerConversation,
    timer_type: TimerType,
    now: DateTime<Utc>,
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

            let Some(started_at) = state_data.conversation_abandon_started_at else {
                return BootExpirationAction::None;
            };

            let elapsed = elapsed_since(started_at, now);
            if !state_data.conversation_abandon_reminder_sent
                && elapsed
                    < duration_from_overrides(is_simulator, overrides, TimerRule::ConversationReset)
            {
                BootExpirationAction::MarkInactivityReminderAndRestore { started_at }
            } else {
                BootExpirationAction::ResetConversation {
                    clear_advisor_session: false,
                    mark_manual_followup: false,
                }
            }
        }
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
    record_simulator_timer_notice(
        &state,
        &phone_number,
        TimerType::ReceiptUpload,
        source,
        &conversation.state,
        "timeout_buttons",
        false,
    )
    .await;
    dispatch_timer_actions(
        &state,
        &[
            BotAction::SendText {
                to: phone_number.clone(),
                body: client_messages()
                    .timers_customer
                    .receipt_timeout_text
                    .clone(),
            },
            BotAction::SendButtons {
                to: phone_number.clone(),
                body: client_messages()
                    .timers_customer
                    .receipt_timeout_buttons_body
                    .clone(),
                buttons: receipt_timeout_buttons(),
            },
        ],
        Some(&phone_number),
    )
    .await?;

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

    let Some(timeout_kind) = advisor_timeout_kind(conversation.state.as_str()) else {
        return Ok(());
    };

    let mut state_data = conversation.state_data.0;
    if state_data.advisor_timer_expired {
        return Ok(());
    }

    clear_bound_advisor_session(&state, &state.config.advisor_phone).await?;

    match timeout_kind {
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
            record_simulator_timer_notice(
                &state,
                &phone_number,
                TimerType::AdvisorResponse,
                source,
                &conversation.state,
                "fallback_buttons",
                false,
            )
            .await;

            match conversation.state.as_str() {
                "wait_advisor_contact" => {
                    dispatch_timer_actions(
                        &state,
                        &[BotAction::SendButtons {
                            to: phone_number.clone(),
                            body: client_messages()
                                .timers_customer
                                .contact_timeout_body
                                .clone(),
                            buttons: contact_timeout_buttons(),
                        }],
                        Some(&phone_number),
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
                    dispatch_timer_actions(
                        &state,
                        &[
                            BotAction::SendText {
                                to: phone_number.clone(),
                                body: timeout_text.clone(),
                            },
                            BotAction::SendButtons {
                                to: phone_number.clone(),
                                body: client_messages()
                                    .timers_customer
                                    .advisor_timeout_buttons_body
                                    .clone(),
                                buttons: advisor_timeout_buttons(),
                            },
                        ],
                        Some(&phone_number),
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
            tracing::info!(
                phone = %mask_phone(&phone_number),
                timer_type = %TimerType::AdvisorResponse.as_str(),
                timeout_kind = "hard_reset",
                order_id = ?state_data.current_order_id,
                source = %source.as_str(),
                "advisor stuck timer reset conversation"
            );
            record_simulator_timer_notice(
                &state,
                &phone_number,
                TimerType::AdvisorResponse,
                source,
                &conversation.state,
                "hard_reset_main_menu",
                true,
            )
            .await;
            dispatch_timer_actions(
                &state,
                &[BotAction::SendText {
                    to: phone_number.clone(),
                    body: client_messages()
                        .timers_customer
                        .advisor_stuck_timeout_text
                        .clone(),
                }],
                Some(&phone_number),
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

    if conversation.state != "relay_mode" {
        return Ok(());
    }

    reset_conversation(&state.pool, &phone_number).await?;
    clear_bound_advisor_session(&state, &state.config.advisor_phone).await?;
    tracing::info!(
        phone = %mask_phone(&phone_number),
        timer_type = %TimerType::RelayInactivity.as_str(),
        source = %source.as_str(),
        "relay timer expired"
    );
    record_simulator_timer_notice(
        &state,
        &phone_number,
        TimerType::RelayInactivity,
        source,
        &conversation.state,
        "reset_main_menu",
        true,
    )
    .await;
    dispatch_timer_actions(
        &state,
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
        Some(&phone_number),
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

    if !customer_inactivity_state(&conversation.state) {
        return Ok(());
    }

    let mut state_data = conversation.state_data.0;
    let Some(started_at) = state_data.conversation_abandon_started_at else {
        return Ok(());
    };
    let now = Utc::now();
    let elapsed = elapsed_since(started_at, now);

    if !state_data.conversation_abandon_reminder_sent
        && elapsed < effective_duration(&state, TimerRule::ConversationReset)
    {
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
        tracing::info!(
            phone = %mask_phone(&phone_number),
            timer_type = %TimerType::ConversationAbandon.as_str(),
            state = %conversation.state,
            source = %source.as_str(),
            "sending inactivity reminder"
        );
        record_simulator_timer_notice(
            &state,
            &phone_number,
            TimerType::ConversationAbandon,
            source,
            &conversation.state,
            "reminder_sent",
            false,
        )
        .await;
        dispatch_timer_actions(&state, &actions, Some(&phone_number)).await?;

        state_data.conversation_abandon_started_at = Some(started_at);
        state_data.conversation_abandon_reminder_sent = true;
        update_state(&state.pool, &phone_number, &conversation.state, &state_data).await?;

        return Ok(());
    }

    dispatch_timer_actions(
        &state,
        &reset_notice_actions(&phone_number),
        Some(&phone_number),
    )
    .await?;
    reset_conversation(&state.pool, &phone_number).await?;
    tracing::info!(
        phone = %mask_phone(&phone_number),
        timer_type = %TimerType::ConversationAbandon.as_str(),
        state = %conversation.state,
        source = %source.as_str(),
        "reset conversation after inactivity timeout"
    );
    record_simulator_timer_notice(
        &state,
        &phone_number,
        TimerType::ConversationAbandon,
        source,
        &conversation.state,
        "reset_main_menu",
        true,
    )
    .await;

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
        "select_customer_data_field",
        "edit_customer_name",
        "edit_customer_phone",
        "edit_customer_address",
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

fn advisor_timeout_for_state_with_overrides(
    is_simulator: bool,
    overrides: &SimulatorTimerOverrides,
    state: &str,
) -> Option<Duration> {
    match advisor_timeout_kind(state) {
        Some(AdvisorTimeoutKind::FallbackButtons) => Some(duration_from_overrides(
            is_simulator,
            overrides,
            TimerRule::AdvisorResponse,
        )),
        Some(AdvisorTimeoutKind::HardReset) => Some(duration_from_overrides(
            is_simulator,
            overrides,
            TimerRule::AdvisorStuck,
        )),
        None => None,
    }
}

fn derive_timer_snapshots(
    state: &AppState,
    conversation_state: &str,
    state_data: &ConversationStateData,
    last_message_at: DateTime<Utc>,
    now: DateTime<Utc>,
) -> Vec<SimulatorTimerSnapshot> {
    let mut snapshots = Vec::new();

    if customer_inactivity_state(conversation_state) {
        if let Some(started_at) = state_data.conversation_abandon_started_at {
            let (rule, phase) = if state_data.conversation_abandon_reminder_sent {
                (TimerRule::ConversationReset, "reset")
            } else {
                (TimerRule::ConversationReminder, "reminder")
            };
            snapshots.push(build_timer_snapshot(
                state,
                TimerType::ConversationAbandon,
                rule,
                phase,
                conversation_state,
                started_at,
                now,
            ));
        }
    }

    if !state_data.advisor_timer_expired {
        if let Some(kind) = advisor_timeout_kind(conversation_state) {
            let (rule, phase) = match kind {
                AdvisorTimeoutKind::FallbackButtons => {
                    (TimerRule::AdvisorResponse, "fallback_buttons")
                }
                AdvisorTimeoutKind::HardReset => (TimerRule::AdvisorStuck, "hard_reset"),
            };
            snapshots.push(build_timer_snapshot(
                state,
                TimerType::AdvisorResponse,
                rule,
                phase,
                conversation_state,
                state_data
                    .advisor_timer_started_at
                    .unwrap_or(last_message_at),
                now,
            ));
        }
    }

    if conversation_state == "wait_receipt" && !state_data.receipt_timer_expired {
        snapshots.push(build_timer_snapshot(
            state,
            TimerType::ReceiptUpload,
            TimerRule::ReceiptUpload,
            "receipt_upload",
            conversation_state,
            state_data
                .receipt_timer_started_at
                .unwrap_or(last_message_at),
            now,
        ));
    }

    if conversation_state == "relay_mode" {
        snapshots.push(build_timer_snapshot(
            state,
            TimerType::RelayInactivity,
            TimerRule::RelayInactivity,
            "relay_inactivity",
            conversation_state,
            state_data.relay_timer_started_at.unwrap_or(last_message_at),
            now,
        ));
    }

    snapshots
}

fn build_timer_snapshot(
    state: &AppState,
    timer_type: TimerType,
    rule: TimerRule,
    phase: &str,
    conversation_state: &str,
    started_at: DateTime<Utc>,
    now: DateTime<Utc>,
) -> SimulatorTimerSnapshot {
    let duration = effective_duration(state, rule);
    let elapsed = elapsed_since(started_at, now);
    let remaining = duration
        .checked_sub(elapsed)
        .map(|value| value.as_secs() as i64)
        .unwrap_or(0);

    SimulatorTimerSnapshot {
        timer_type: timer_type.as_str().to_string(),
        rule_key: rule.as_str().to_string(),
        label: rule.label().to_string(),
        phase: phase.to_string(),
        state: conversation_state.to_string(),
        started_at,
        expires_at: started_at + chrono::Duration::from_std(duration).unwrap_or_default(),
        effective_seconds: duration.as_secs() as i64,
        remaining_seconds: remaining,
        expired: elapsed >= duration,
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
            | "select_customer_data_field"
            | "edit_customer_name"
            | "edit_customer_phone"
            | "edit_customer_address"
            | "show_summary"
            | "offer_hour_to_client"
            | "wait_client_hour"
            | "contact_advisor_name"
            | "contact_advisor_phone"
            | "leave_message"
    )
}

async fn record_simulator_timer_notice(
    state: &AppState,
    phone_number: &str,
    timer_type: TimerType,
    source: TimerSource,
    conversation_state: &str,
    outcome: &str,
    reset_to_main_menu: bool,
) {
    if !state.transport.is_simulator() {
        return;
    }

    let session_id = match get_session_by_phone(&state.pool, phone_number).await {
        Ok(Some(session)) => Some(session.id),
        Ok(None) => None,
        Err(err) => {
            tracing::warn!(
                phone = %mask_phone(phone_number),
                error = %err,
                "failed to load simulator session for timer notice"
            );
            None
        }
    };

    let body = format!(
        "Timer {} ({}) en {} -> {}{}",
        timer_type.as_str(),
        source.as_str(),
        conversation_state,
        outcome,
        if reset_to_main_menu {
            " -> main_menu"
        } else {
            ""
        }
    );

    if let Err(err) = create_message(
        &state.pool,
        NewSimulatorMessage {
            session_id,
            actor: "system".to_string(),
            audience: "system".to_string(),
            message_kind: "state_notice".to_string(),
            body: Some(body),
            payload: json!({
                "phone_number": phone_number,
                "timer_type": timer_type.as_str(),
                "source": source.as_str(),
                "state": conversation_state,
                "outcome": outcome,
                "reset_to_main_menu": reset_to_main_menu,
            }),
        },
    )
    .await
    {
        tracing::warn!(
            phone = %mask_phone(phone_number),
            error = %err,
            "failed to record simulator timer notice"
        );
    }
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

    use super::{
        boot_expiration_action_with_overrides, timer_recovery_with_overrides, BootExpirationAction,
        SimulatorTimerOverrides, TimerRecovery, ADVISOR_STUCK_TIMEOUT,
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
        }
    }

    fn production_overrides() -> SimulatorTimerOverrides {
        SimulatorTimerOverrides::default()
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

        let recovery =
            timer_recovery_with_overrides(false, &production_overrides(), &conversation, now);

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

        let recovery =
            timer_recovery_with_overrides(false, &production_overrides(), &conversation, now);

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

        let recovery =
            timer_recovery_with_overrides(false, &production_overrides(), &conversation, now);

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
            timer_recovery_with_overrides(false, &production_overrides(), &conversation, now);

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

        let recovery = timer_recovery_with_overrides(
            false,
            &production_overrides(),
            &conversation,
            now + ChronoDuration::minutes(40),
        );

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
            timer_recovery_with_overrides(false, &production_overrides(), &conversation, now);

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
            timer_recovery_with_overrides(false, &production_overrides(), &conversation, now);

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

        let recovery =
            timer_recovery_with_overrides(false, &production_overrides(), &conversation, now);

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

        let recovery =
            timer_recovery_with_overrides(false, &production_overrides(), &conversation, now);

        assert_eq!(
            recovery,
            Some(TimerRecovery::Active {
                timer_type: TimerType::ConversationAbandon,
                timeout: crate::bot::inactivity::CONVERSATION_RESET_TIMEOUT,
                started_at: now - ChronoDuration::minutes(10),
            })
        );
    }

    #[test]
    fn boot_expiration_marks_receipt_timeout_without_sending() {
        let now = chrono::Utc::now();
        let conversation =
            active_timer_conversation("wait_receipt", ConversationStateData::default(), now);

        let action = boot_expiration_action_with_overrides(
            false,
            &production_overrides(),
            &conversation,
            TimerType::ReceiptUpload,
            now,
        );

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

        let action = boot_expiration_action_with_overrides(
            false,
            &production_overrides(),
            &conversation,
            TimerType::AdvisorResponse,
            now,
        );

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

        let action = boot_expiration_action_with_overrides(
            false,
            &production_overrides(),
            &conversation,
            TimerType::AdvisorResponse,
            now,
        );

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

        let action = boot_expiration_action_with_overrides(
            false,
            &production_overrides(),
            &conversation,
            TimerType::RelayInactivity,
            now,
        );

        assert_eq!(
            action,
            BootExpirationAction::ResetConversation {
                clear_advisor_session: true,
                mark_manual_followup: false,
            }
        );
    }

    #[test]
    fn boot_expiration_marks_customer_reminder_and_restores_deadline() {
        let now = chrono::Utc::now();
        let started_at = now - ChronoDuration::minutes(3);
        let conversation = active_timer_conversation(
            "collect_phone",
            ConversationStateData {
                conversation_abandon_started_at: Some(started_at),
                ..Default::default()
            },
            now,
        );

        let action = boot_expiration_action_with_overrides(
            false,
            &production_overrides(),
            &conversation,
            TimerType::ConversationAbandon,
            now,
        );

        assert_eq!(
            action,
            BootExpirationAction::MarkInactivityReminderAndRestore { started_at }
        );
    }

    #[test]
    fn boot_expiration_resets_customer_after_full_inactivity_window() {
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

        let action = boot_expiration_action_with_overrides(
            false,
            &production_overrides(),
            &conversation,
            TimerType::ConversationAbandon,
            now,
        );

        assert_eq!(
            action,
            BootExpirationAction::ResetConversation {
                clear_advisor_session: false,
                mark_manual_followup: false,
            }
        );
    }

    #[test]
    fn timer_recovery_uses_simulator_override_for_receipt_timeout() {
        let now = chrono::Utc::now();
        let conversation = active_timer_conversation(
            "wait_receipt",
            ConversationStateData {
                receipt_timer_started_at: Some(now - ChronoDuration::seconds(31)),
                ..Default::default()
            },
            now,
        );
        let overrides = SimulatorTimerOverrides {
            receipt_upload_seconds: Some(30),
            ..Default::default()
        };

        let recovery = timer_recovery_with_overrides(true, &overrides, &conversation, now);

        assert_eq!(
            recovery,
            Some(TimerRecovery::Expired(TimerType::ReceiptUpload))
        );
    }
}
