pub mod ai;
pub mod bot;
pub mod capi;
pub mod config;
pub mod db;
pub mod engine;
pub mod logging;
pub mod messages;
pub mod referrals;
pub mod routes;
pub mod whatsapp;

use std::{
    collections::HashMap,
    sync::Arc,
    time::{Duration, Instant},
};

use tokio::sync::Mutex;

/// Registro de un mutex por numero de cliente (la "case key" de una
/// conversacion). El motor determinista es sincrono/en-memoria y su
/// ventana de carrera entre un mensaje del cliente y uno del asesor para
/// el mismo caso es de milisegundos, asi que nunca necesito esto. El
/// agente de IA hace varias idas y vueltas a la API de Anthropic por
/// turno (segundos, no milisegundos), lo que abre una ventana real donde
/// el asesor y el cliente pueden escribir casi al mismo tiempo sobre el
/// mismo pedido y pisarse la escritura en `conversations`/`orders`. Este
/// lock serializa ambos lados solo para ESE numero de cliente — casos de
/// otros clientes siguen totalmente en paralelo.
pub type ConversationLocks = Arc<Mutex<HashMap<String, Arc<Mutex<()>>>>>;

pub fn new_conversation_locks() -> ConversationLocks {
    Arc::new(Mutex::new(HashMap::new()))
}

pub async fn lock_conversation(
    locks: &ConversationLocks,
    phone_number: &str,
) -> tokio::sync::OwnedMutexGuard<()> {
    let per_phone = {
        let mut registry = locks.lock().await;
        registry
            .entry(phone_number.to_string())
            .or_insert_with(|| Arc::new(Mutex::new(())))
            .clone()
    };
    per_phone.lock_owned().await
}

/// Cache en memoria de `message_id` de Meta ya procesados. Meta reintenta
/// webhooks en ventanas de minutos cuando cree que el delivery falló; sin
/// esto, un retry procesa el mismo mensaje dos veces (doble respuesta al
/// cliente y, en modo agente, doble llamada al LLM). TTL corto porque los
/// retries de Meta no llegan horas después.
pub type WebhookDedupCache = Arc<Mutex<HashMap<String, Instant>>>;

pub const WEBHOOK_DEDUP_TTL: Duration = Duration::from_secs(10 * 60);

pub fn new_webhook_dedup_cache() -> WebhookDedupCache {
    Arc::new(Mutex::new(HashMap::new()))
}

/// Devuelve `true` si el `message_id` ya fue visto dentro del TTL (y por lo
/// tanto debe ignorarse). Registra el id y poda entradas vencidas de paso.
pub async fn is_duplicate_message(cache: &WebhookDedupCache, message_id: &str) -> bool {
    let now = Instant::now();
    let mut seen = cache.lock().await;
    seen.retain(|_, inserted| now.duration_since(*inserted) < WEBHOOK_DEDUP_TTL);
    seen.insert(message_id.to_string(), now).is_some()
}

#[derive(Clone)]
pub struct AppState {
    pub config: config::Config,
    pub pool: sqlx::PgPool,
    pub transport: whatsapp::client::WhatsAppClient,
    pub timers: bot::timers::TimerMap,
    pub conversation_locks: ConversationLocks,
    pub llm_budget: ai::budget::LlmBudgetHandle,
    pub webhook_dedup: WebhookDedupCache,
    pub capi: capi::CapiClient,
}
