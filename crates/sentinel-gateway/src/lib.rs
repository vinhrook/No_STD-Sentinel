use tokio::sync::broadcast;

pub mod alerts;
pub mod api;
pub mod config;
pub mod db;
pub mod mqtt_subscriber;
pub mod websocket;

pub struct AppState {
    pub db: db::Database,
    pub live_tx: broadcast::Sender<String>,
    pub config: config::GatewayConfig,
}