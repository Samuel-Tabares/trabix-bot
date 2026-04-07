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

#[derive(Clone)]
pub struct AppState {
    pub config: config::Config,
    pub pool: sqlx::PgPool,
    pub transport: transport::OutboundTransport,
    pub timers: bot::timers::TimerMap,
    pub timer_overrides: bot::timers::TimerOverridesHandle,
}
