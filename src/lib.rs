pub mod bot;
pub mod config;
pub mod db;
pub mod routes;
pub mod whatsapp;

#[derive(Clone)]
pub struct AppState {
    pub config: config::Config,
    pub pool: sqlx::PgPool,
    pub wa_client: whatsapp::client::WhatsAppClient,
    pub timers: bot::timers::TimerMap,
}
