use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::get,
    Json, Router,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tower_http::cors::CorsLayer;
use tower_http::services::ServeDir;

use sentinel_core::telemetry::FreezerReading;
use crate::{db::{DbError, DeviceInfo}, websocket, AppState};

pub fn build_router(state: Arc<AppState>) -> Router {
    let dist_dir = if std::path::Path::new("crates/sentinel-dashboard/dist").exists() {
        "crates/sentinel-dashboard/dist"
    } else {
        "../sentinel-dashboard/dist"
    };

    Router::new()
        .route("/healthz", get(healthz))
        .route("/api/devices", get(list_devices))
        .route("/api/devices/{device_id}/readings", get(get_readings))
        .route("/ws/live", get(websocket::ws_handler))
        .fallback_service(ServeDir::new(dist_dir))
        .layer(CorsLayer::permissive())
        .with_state(state)
}

#[derive(Debug, Serialize)]
pub struct HealthResponse {
    pub status: &'static str,
    pub service: &'static str,
    pub version: &'static str,
}

#[derive(Debug, Deserialize)]
pub struct ReadingQuery {
    pub limit: Option<u32>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct DeviceListResponse {
    pub devices: Vec<DeviceInfo>,
    pub count: usize,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ReadingsResponse {
    pub device_id: u32,
    pub readings: Vec<FreezerReading>,
    pub count: usize,
}

#[derive(Debug)]
pub enum ApiError {
    Database(DbError),
}

impl From<DbError> for ApiError {
    fn from(err: DbError) -> Self {
        Self::Database(err)
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let (status, client_message) = match self {
            Self::Database(err) => {
                tracing::error!(error = %err, "Database query fault in API route");
                (StatusCode::INTERNAL_SERVER_ERROR, "Internal database error")
            }
        };

        (
            status,
            Json(serde_json::json!({
                "error": client_message,
                "status": status.as_u16()
            })),
        )
            .into_response()
    }
}

async fn healthz() -> Json<HealthResponse> {
    Json(HealthResponse {
        status: "healthy",
        service: "sentinel-gateway",
        version: env!("CARGO_PKG_VERSION"),
    })
}

async fn list_devices(
    State(state): State<Arc<AppState>>,
) -> Result<Json<DeviceListResponse>, ApiError> {
    let devices = state.db.get_devices()?;
    let count = devices.len();

    Ok(Json(DeviceListResponse { devices, count }))
}

async fn get_readings(
    State(state): State<Arc<AppState>>,
    Path(device_id): Path<u32>,
    Query(params): Query<ReadingQuery>,
) -> Result<Json<ReadingsResponse>, ApiError> {
    let limit = params.limit.unwrap_or(100).clamp(1, 1000);
    let readings = state.db.get_latest_readings(device_id, limit)?;
    let count = readings.len();

    Ok(Json(ReadingsResponse {
        device_id,
        readings,
        count,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use reqwest::StatusCode;
    use sentinel_core::telemetry::{
        DeviceId, FirmwareVersion, FreezerReading, HealthScore, TempCentiC,
    };
    use crate::config::GatewayConfig;
    use crate::db::Database;

    async fn spawn_test_app() -> (String, Arc<AppState>) {
        let db = Database::open_in_memory().expect("failed to open test in-memory database");
        db.run_migrations().expect("failed to run database migrations");
        let (live_tx, _) = tokio::sync::broadcast::channel(256);
        let config = GatewayConfig::default();
        let state = Arc::new(AppState { db, live_tx, config });
        let app = build_router(Arc::clone(&state));

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("failed to bind ephemeral port");
        let port = listener.local_addr().unwrap().port();
        let base_url = format!("http://127.0.0.1:{port}");

        tokio::spawn(async move {
            let _ = axum::serve(listener, app).await;
        });

        (base_url, state)
    }

    #[tokio::test]
    async fn test_healthz_endpoint() {
        let (base_url, _state) = spawn_test_app().await;
        let client = reqwest::Client::new();

        let response = client
            .get(format!("{base_url}/healthz"))
            .send()
            .await
            .expect("failed to send request");

        assert_eq!(response.status(), StatusCode::OK);

        let body: serde_json::Value = response.json().await.expect("invalid json response");
        assert_eq!(body["status"], "healthy");
        assert_eq!(body["service"], "sentinel-gateway");
        assert_eq!(body["version"], env!("CARGO_PKG_VERSION"));
    }

    #[tokio::test]
    async fn test_list_devices_empty() {
        let (base_url, _state) = spawn_test_app().await;
        let client = reqwest::Client::new();

        let response = client
            .get(format!("{base_url}/api/devices"))
            .send()
            .await
            .expect("failed to send request");

        assert_eq!(response.status(), StatusCode::OK);

        let body: DeviceListResponse = response.json().await.expect("invalid json response");
        assert_eq!(body.count, 0);
        assert!(body.devices.is_empty());
    }

    #[tokio::test]
    async fn test_list_devices_populated() {
        let (base_url, state) = spawn_test_app().await;
        let client = reqwest::Client::new();

        state.db.upsert_device(0x00A4_F3B2).unwrap();
        state.db.upsert_device(0x0011_2233).unwrap();

        let response = client
            .get(format!("{base_url}/api/devices"))
            .send()
            .await
            .expect("failed to send request");

        assert_eq!(response.status(), StatusCode::OK);

        let body: DeviceListResponse = response.json().await.expect("invalid json response");
        assert_eq!(body.count, 2);
        assert_eq!(body.devices.len(), 2);

        let ids: Vec<u32> = body.devices.into_iter().map(|d| d.device_id).collect();
        assert!(ids.contains(&0x00A4_F3B2));
        assert!(ids.contains(&0x0011_2233));
    }

    #[tokio::test]
    async fn test_get_readings_empty_history() {
        let (base_url, state) = spawn_test_app().await;
        let client = reqwest::Client::new();
        let dev_id: u32 = 0xDEAD_BEEF;

        state.db.upsert_device(dev_id).unwrap();

        let response = client
            .get(format!("{base_url}/api/devices/{dev_id}/readings"))
            .send()
            .await
            .expect("failed to send request");

        assert_eq!(response.status(), StatusCode::OK);

        let body: ReadingsResponse = response.json().await.expect("invalid json response");
        assert_eq!(body.device_id, dev_id);
        assert_eq!(body.count, 0);
        assert!(body.readings.is_empty());
    }

    #[tokio::test]
    async fn test_get_readings_with_limit_and_order() {
        let (base_url, state) = spawn_test_app().await;
        let client = reqwest::Client::new();

        let dev_id: u32 = 0x00A4_F3B2;
        state.db.upsert_device(dev_id).unwrap();

        for i in 1..=5 {
            let reading = FreezerReading {
                device_id: DeviceId::new(dev_id),
                timestamp_secs: 1_710_000_000 + i,
                temp: TempCentiC::from_centi(-1800 - i as i16 * 10),
                door_open: false,
                door_open_secs: 0,
                health_score: HealthScore::new(900 + i as u16 * 10),
                rssi_dbm: -60,
                firmware_version: FirmwareVersion::new(1, 0, 0),
            };
            state.db.insert_reading(&reading).unwrap();
        }

        let response = client
            .get(format!("{base_url}/api/devices/{dev_id}/readings?limit=2"))
            .send()
            .await
            .expect("failed to send request");

        assert_eq!(response.status(), StatusCode::OK);

        let body: ReadingsResponse = response.json().await.expect("invalid json response");
        assert_eq!(body.device_id, dev_id);
        assert_eq!(body.count, 2);
        assert_eq!(body.readings.len(), 2);
    }

    #[tokio::test]
    async fn test_get_readings_limit_clamping() {
        let (base_url, state) = spawn_test_app().await;
        let client = reqwest::Client::new();

        let dev_id: u32 = 0xCAFE_BABE;
        state.db.upsert_device(dev_id).unwrap();

        let reading = FreezerReading {
            device_id: DeviceId::new(dev_id),
            timestamp_secs: 1_710_000_000,
            temp: TempCentiC::from_centi(-1850),
            door_open: false,
            door_open_secs: 0,
            health_score: HealthScore::new(950),
            rssi_dbm: -55,
            firmware_version: FirmwareVersion::new(1, 0, 0),
        };
        state.db.insert_reading(&reading).unwrap();

        let resp_zero = client
            .get(format!("{base_url}/api/devices/{dev_id}/readings?limit=0"))
            .send()
            .await
            .unwrap();

        assert_eq!(resp_zero.status(), StatusCode::OK);
        let body_zero: ReadingsResponse = resp_zero.json().await.unwrap();
        assert_eq!(body_zero.count, 1);
    }

    #[tokio::test]
    async fn test_cors_headers_present() {
        let (base_url, _state) = spawn_test_app().await;
        let client = reqwest::Client::new();

        let response = client
            .get(format!("{base_url}/healthz"))
            .header("Origin", "http://localhost:3000")
            .send()
            .await
            .expect("failed to send request");

        assert_eq!(response.status(), StatusCode::OK);
        let allow_origin = response.headers().get("access-control-allow-origin");
        assert!(allow_origin.is_some(), "CORS header access-control-allow-origin should be present");
        assert_eq!(allow_origin.unwrap(), "*");
    }

    #[tokio::test]
    async fn test_unknown_route_returns_404() {
        let (base_url, _state) = spawn_test_app().await;
        let client = reqwest::Client::new();

        let response = client
            .get(format!("{base_url}/api/non_existent_route"))
            .send()
            .await
            .expect("failed to send request");

        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }

    #[test]
    fn test_api_error_status_and_body() {
        let err = ApiError::Database(DbError::LockPoisoned);
        let resp = err.into_response();

        assert_eq!(resp.status(), axum::http::StatusCode::INTERNAL_SERVER_ERROR);
    }
}
