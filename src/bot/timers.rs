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
        state_machine::{BotAction, ConversationContext, ConversationState, TimerType},
    },
    db::{
        models::{ConversationStateData, HandoffReason},
        queries::{
            clear_human_takeover, get_conversation, list_active_timer_conversations,
            list_expired_handoffs, update_state,
        },
    },
    engine::send_timer_actions as dispatch_timer_actions,
    logging::mask_phone,
    messages::client_messages,
    AppState,
};

pub type TimerKey = (String, TimerType);
pub type TimerMap = Arc<Mutex<HashMap<TimerKey, ActiveTimer>>>;

pub const RECEIPT_TIMEOUT: Duration = Duration::from_secs(10 * 60);
const TIMER_SWEEP_INTERVAL: Duration = Duration::from_secs(60);
static NEXT_TIMER_INSTANCE_ID: AtomicU64 = AtomicU64::new(1);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TimerRule {
    ReceiptUpload,
}

impl TimerRule {
    pub fn default_duration(&self) -> Duration {
        match self {
            Self::ReceiptUpload => RECEIPT_TIMEOUT,
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

/// Retoma los handoffs que nadie devolvió. La ventana de toma de control vence
/// sola a las `ADVISOR_TAKEOVER_HOURS` (6 por defecto); si el asesor atendió el
/// caso y se olvidó de pulsar "Devolver al bot", sin esto el pedido se quedaría
/// sin cotizar o sin confirmar para siempre — y con él toda la contabilidad que
/// cuelga de `confirm_order_bookkeeping`.
///
/// Solo toca los casos con `state_data.handoff_reason` puesto: una toma de
/// control que un humano inició por su cuenta (escribirle al cliente sin que el
/// bot lo pidiera) no tiene nada que retomar.
pub async fn sweep_expired_handoffs(state: AppState) -> Result<(), sqlx::Error> {
    for phone_number in list_expired_handoffs(&state.pool).await? {
        tracing::info!(
            phone = %mask_phone(&phone_number),
            source = "sweep",
            "handoff window expired without an explicit release; resuming the case"
        );

        if let Err(err) = clear_human_takeover(&state.pool, &phone_number).await {
            tracing::error!(error = %err, "failed to clear the expired takeover window");
            continue;
        }

        if let Err(err) = crate::engine::process_resume_for_case(&state, &phone_number).await {
            tracing::error!(error = %err, "failed to resume the case after an expired handoff");
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

            if let Err(err) = sweep_expired_handoffs(state.clone()).await {
                tracing::error!(error = %err, "failed to sweep expired handoffs");
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

    match conversation.state.as_str() {
        "wait_receipt" if !state_data.receipt_timer_expired && state_data.receipt_media_id.is_none() => timer_recovery_for(
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
            if conversation.state == "wait_receipt"
                && !state_data.receipt_timer_expired
                && state_data.receipt_media_id.is_none()
            {
                BootExpirationAction::UpdateReceiptExpired
            } else {
                BootExpirationAction::None
            }
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
        // Abrimos, pero el domicilio sigue sin cotizar (municipio fuera de la
        // lista o envío nacional). El bot no puede resolverlo y ya no existe el
        // carril para preguntárselo al asesor: se entrega el caso a un humano.
        context.handoff_reason = Some(HandoffReason::DeliveryQuote);
        let handoff_actions = vec![
            BotAction::SendText {
                to: phone_number.clone(),
                body: "✅ ¡Ya abrimos!".to_string(),
            },
            BotAction::HandOffToHuman {
                reason: HandoffReason::DeliveryQuote,
                advisor_note: format!(
                    "☀️ Ya abrimos y el pedido inmediato {} sigue sin costo de envío \
                     (municipio/zona desconocida). Escríbele tú al cliente con el valor y \
                     devuélveme la conversación cuando cierres.",
                    phone_marker(&phone_number)
                ),
            },
        ];
        (ConversationState::MainMenu, handoff_actions)
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
    ]
}

/// Fase 2: si un asesor tomó el caso desde `crm-app` (`set_human_takeover`),
/// ningún timer de este archivo debe dispararle nada al cliente hasta que
/// venza `until`. Compartido por los 4 `expire_*_with_source` y por la
/// reconciliación de arranque.
fn human_takeover_active(until: Option<DateTime<Utc>>) -> bool {
    until.is_some_and(|until| until > Utc::now())
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
        boot_expiration_action, human_takeover_active, timer_recovery, BootExpirationAction,
        TimerRecovery,
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
    fn boot_expiration_marks_receipt_timeout_without_sending() {
        let now = chrono::Utc::now();
        let conversation =
            active_timer_conversation("wait_receipt", ConversationStateData::default(), now);

        let action = boot_expiration_action(&conversation, TimerType::ReceiptUpload);

        assert_eq!(action, BootExpirationAction::UpdateReceiptExpired);
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
