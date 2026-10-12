use std::sync::Arc;
use tokio::sync::broadcast;
use tracing::info;

use sentinel_gateway::{api, config, db, mqtt_subscriber, AppState};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info,sentinel_gateway=debug".into()),
        )
        .compact()
        .init();

    info!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    info!(" SENTINEL Edge Gateway v0.1.0 ");
    info!("   Architecture: Tokio Core · Axum · SQLite WAL · MQTT v5");
    info!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");

    let config = config::GatewayConfig::load("gateway.toml")?;
    info!(
        bind = %config.bind_addr,
        db = %config.db_path.display(),
        mqtt = %config.mqtt.host,
        "Configuration successfully loaded"
    );

    let database = db::Database::open(&config.db_path)?;
    database.run_migrations()?;
    info!(path = %config.db_path.display(), "SQLite WAL database engine online");

    let (live_tx, _) = broadcast::channel::<String>(256);

    let state = Arc::new(AppState {
        db: database,
        live_tx,
        config: config.clone(),
    });

    let mqtt_state = Arc::clone(&state);
    let mqtt_handle = tokio::spawn(async move {
        if let Err(err) = mqtt_subscriber::run(mqtt_state).await {
            tracing::error!(error = %err, "MQTT subscriber ingestion task halted");
        }
    });

    let app = api::build_router(Arc::clone(&state));

    let listener = tokio::net::TcpListener::bind(state.config.bind_addr).await?;
    info!(addr = %state.config.bind_addr, "HTTP & WebSocket server listening");

    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await?;

    mqtt_handle.abort();
    info!("Sentinel Gateway shut down cleanly. SQLite WAL flushed.");

    Ok(())
}

async fn shutdown_signal() {
    let ctrl_c = async {
        tokio::signal::ctrl_c()
            .await
            .expect("Failed to register Ctrl+C handler");
    };

    #[cfg(unix)]
    let terminate = async {
        tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            .expect("Failed to register SIGTERM handler")
            .recv()
            .await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => info!("Received SIGINT (Ctrl+C). Initiating graceful shutdown..."),
        _ = terminate => info!("Received SIGTERM. Initiating graceful shutdown..."),
    }
}
