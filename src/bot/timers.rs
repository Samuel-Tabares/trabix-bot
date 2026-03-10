use std::{collections::HashMap, future::Future, sync::Arc, time::Duration};

use tokio::sync::Mutex;
use tokio_util::sync::CancellationToken;

use crate::{
    bot::state_machine::{ConversationContext, TimerType},
    db::{
        models::ConversationStateData,
        queries::{get_conversation, list_active_timer_conversations, update_state},
    },
    whatsapp::types::{Button, ButtonReplyPayload},
    AppState,
};

pub type TimerKey = (String, TimerType);
pub type TimerMap = Arc<Mutex<HashMap<TimerKey, CancellationToken>>>;

pub const RECEIPT_TIMEOUT: Duration = Duration::from_secs(10 * 60);

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
    let conversations = list_active_timer_conversations(&state.pool, &["wait_receipt"]).await?;

    for conversation in conversations {
        let state_data = &conversation.state_data.0;
        if state_data.receipt_timer_expired {
            continue;
        }

        let elapsed = chrono::Utc::now()
            .signed_duration_since(conversation.last_message_at)
            .to_std()
            .unwrap_or_default();

        if elapsed >= RECEIPT_TIMEOUT {
            if let Err(err) =
                expire_receipt_timer(state.clone(), conversation.phone_number.clone()).await
            {
                tracing::error!(phone = %conversation.phone_number, error = %err, "failed to expire restored receipt timer");
            }
            continue;
        }

        let remaining = RECEIPT_TIMEOUT - elapsed;
        let phone = conversation.phone_number.clone();
        let cloned = state.clone();
        start_timer(
            state.timers.clone(),
            (phone.clone(), TimerType::ReceiptUpload),
            remaining,
            move || async move {
                if let Err(err) = expire_receipt_timer(cloned, phone).await {
                    tracing::error!(error = %err, "failed to expire restored receipt timer");
                }
            },
        )
        .await;
    }

    Ok(())
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
            "No recibimos el comprobante dentro de los 10 minutos. Puedes cambiar la forma de pago o cancelar el pedido.",
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

pub fn rehydrate_context_for_timer(
    phone_number: String,
    customer_name: Option<String>,
    customer_phone: Option<String>,
    delivery_address: Option<String>,
    state_data: &ConversationStateData,
) -> ConversationContext {
    ConversationContext::from_persisted(
        phone_number,
        customer_name,
        customer_phone,
        delivery_address,
        state_data,
    )
}

fn receipt_timeout_buttons() -> Vec<Button> {
    vec![
        reply_button("change_payment_method", "Cambiar pago"),
        reply_button("cancel_order", "Cancelar"),
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

#[cfg(test)]
mod tests {
    use std::sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    };

    use super::{cancel_timer, new_timer_map, start_timer};
    use crate::bot::state_machine::TimerType;

    #[tokio::test]
    async fn starts_and_expires_timer_once() {
        let timers = new_timer_map();
        let runs = Arc::new(AtomicUsize::new(0));
        let runs_for_task = runs.clone();

        start_timer(
            timers,
            ("573001234567".to_string(), TimerType::ReceiptUpload),
            std::time::Duration::from_millis(25),
            move || async move {
                runs_for_task.fetch_add(1, Ordering::SeqCst);
            },
        )
        .await;

        tokio::time::sleep(std::time::Duration::from_millis(60)).await;

        assert_eq!(runs.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn cancels_existing_timer() {
        let timers = new_timer_map();
        let key = ("573001234567".to_string(), TimerType::ReceiptUpload);
        let runs = Arc::new(AtomicUsize::new(0));
        let runs_for_task = runs.clone();

        start_timer(
            timers.clone(),
            key.clone(),
            std::time::Duration::from_millis(40),
            move || async move {
                runs_for_task.fetch_add(1, Ordering::SeqCst);
            },
        )
        .await;

        cancel_timer(timers, &key).await;
        tokio::time::sleep(std::time::Duration::from_millis(70)).await;

        assert_eq!(runs.load(Ordering::SeqCst), 0);
    }
}
