pub mod ai;
pub mod bot;
pub mod config;
pub mod db;
pub mod engine;
pub mod logging;
pub mod messages;
pub mod referrals;
pub mod routes;
pub mod simulator;
pub mod transport;
pub mod whatsapp;

use std::{collections::HashMap, sync::Arc};

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

#[derive(Clone)]
pub struct AppState {
    pub config: config::Config,
    pub pool: sqlx::PgPool,
    pub transport: transport::OutboundTransport,
    pub timers: bot::timers::TimerMap,
    pub timer_overrides: bot::timers::TimerOverridesHandle,
    pub conversation_locks: ConversationLocks,
}
