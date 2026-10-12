use axum::{
    extract::{
        ws::{Message, WebSocket, WebSocketUpgrade},
        State,
    },
    response::Response,
};
use futures_util::SinkExt;
use std::{sync::Arc, time::Duration};
use tokio::time::{interval_at, Instant, MissedTickBehavior};

use crate::AppState;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WsSupervisorConfig {
    pub heartbeat_interval: Duration,
    pub client_timeout: Duration,
}

impl Default for WsSupervisorConfig {
    fn default() -> Self {
        Self {
            heartbeat_interval: Duration::from_secs(25),
            client_timeout: Duration::from_secs(60),
        }
    }
}

pub async fn ws_handler(
    ws: WebSocketUpgrade,
    State(state): State<Arc<AppState>>,
) -> Response {
    ws.on_upgrade(move |socket| handle_ws_connection(socket, state))
}

pub async fn ws_handler_with_config(
    ws: WebSocketUpgrade,
    state: Arc<AppState>,
    config: WsSupervisorConfig,
) -> Response {
    ws.on_upgrade(move |socket| handle_ws_connection_with_config(socket, state, config))
}

pub async fn handle_ws_connection(socket: WebSocket, state: Arc<AppState>) {
    handle_ws_connection_with_config(socket, state, WsSupervisorConfig::default()).await;
}

pub async fn handle_ws_connection_with_config(
    mut socket: WebSocket,
    state: Arc<AppState>,
    config: WsSupervisorConfig,
) {
    tracing::info!("Live telemetry WebSocket client connected");

    let mut rx = state.live_tx.subscribe();

    let mut heartbeat = interval_at(
        Instant::now() + config.heartbeat_interval,
        config.heartbeat_interval,
    );
    heartbeat.set_missed_tick_behavior(MissedTickBehavior::Skip);

    let mut last_activity = Instant::now();

    loop {
        tokio::select! {
            broadcast = rx.recv() => {
                match broadcast {
                    Ok(json) => {
                        if socket.send(Message::Text(json.into())).await.is_err() {
                            break;
                        }
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(dropped)) => {
                        tracing::warn!(
                            dropped,
                            "Slow WebSocket subscriber lagged; skipped stale telemetry frames"
                        );
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                        tracing::debug!("Global telemetry broadcast channel closed");
                        break;
                    }
                }
            }

            _ = heartbeat.tick() => {
                if last_activity.elapsed() > config.client_timeout {
                    tracing::warn!("WebSocket client timed out; dropping dead connection");
                    break;
                }

                if socket.send(Message::Ping(vec![].into())).await.is_err() {
                    break;
                }
            }

            incoming = socket.recv() => {
                match incoming {
                    Some(Ok(Message::Pong(_))) => {
                        last_activity = Instant::now();
                    }
                    Some(Ok(Message::Ping(data))) => {
                        last_activity = Instant::now();
                        if socket.send(Message::Pong(data)).await.is_err() {
                            break;
                        }
                    }
                    Some(Ok(Message::Close(_))) | None => {
                        tracing::debug!("WebSocket client initiated clean disconnection");
                        break;
                    }
                    Some(Err(err)) => {
                        tracing::debug!(error = %err, "WebSocket network transport error");
                        break;
                    }
                    _ => {
                        last_activity = Instant::now();
                    }
                }
            }
        }
    }

    let _ = socket.close().await;
    tracing::info!("Live telemetry WebSocket client disconnected");
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::extract::State;
    use axum::routing::get;
    use axum::Router;
    use crate::api::build_router;
    use crate::config::GatewayConfig;
    use crate::db::Database;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpStream;

    async fn spawn_test_server() -> (u16, Arc<AppState>) {
        let db = Database::open_in_memory().expect("in-memory db");
        db.run_migrations().expect("migrations");
        let (live_tx, _) = tokio::sync::broadcast::channel(256);
        let config = GatewayConfig::default();
        let state = Arc::new(AppState { db, live_tx, config });
        let app = build_router(Arc::clone(&state));

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind ephemeral port");
        let port = listener.local_addr().unwrap().port();

        tokio::spawn(async move {
            let _ = axum::serve(listener, app).await;
        });

        (port, state)
    }

    async fn spawn_test_server_with_config(
        ws_config: WsSupervisorConfig,
    ) -> (u16, Arc<AppState>) {
        let db = Database::open_in_memory().expect("in-memory db");
        db.run_migrations().expect("migrations");
        let (live_tx, _) = tokio::sync::broadcast::channel(256);
        let config = GatewayConfig::default();
        let state = Arc::new(AppState { db, live_tx, config });

        let app = Router::new()
            .route(
                "/ws/live",
                get(move |ws, State(state): State<Arc<AppState>>| {
                    async move { ws_handler_with_config(ws, state, ws_config).await }
                }),
            )
            .with_state(Arc::clone(&state));

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind ephemeral port");
        let port = listener.local_addr().unwrap().port();

        tokio::spawn(async move {
            let _ = axum::serve(listener, app).await;
        });

        (port, state)
    }

    async fn connect_ws(port: u16) -> TcpStream {
        let mut stream = TcpStream::connect(format!("127.0.0.1:{port}"))
            .await
            .expect("tcp connect");

        let request = format!(
            "GET /ws/live HTTP/1.1\r\n\
             Host: 127.0.0.1:{port}\r\n\
             Upgrade: websocket\r\n\
             Connection: Upgrade\r\n\
             Sec-WebSocket-Key: dGhlIHNhbXBsZSBub25jZQ==\r\n\
             Sec-WebSocket-Version: 13\r\n\r\n"
        );

        stream.write_all(request.as_bytes()).await.expect("send handshake");

        let mut response = Vec::new();
        let mut buf = [0u8; 256];
        loop {
            let n = stream.read(&mut buf).await.expect("read handshake response");
            assert!(n > 0, "connection closed prematurely during handshake");
            response.extend_from_slice(&buf[..n]);
            if response.windows(4).any(|w| w == b"\r\n\r\n") {
                break;
            }
        }

        let resp_str = String::from_utf8_lossy(&response);
        assert!(
            resp_str.starts_with("HTTP/1.1 101 Switching Protocols"),
            "Handshake failed with response: {resp_str}"
        );

        stream
    }

    async fn read_server_frame(stream: &mut TcpStream) -> (u8, Vec<u8>) {
        let mut header = [0u8; 2];
        stream.read_exact(&mut header).await.expect("read frame header");

        let opcode = header[0] & 0x0F;
        let mut payload_len = (header[1] & 0x7F) as usize;

        if payload_len == 126 {
            let mut ext = [0u8; 2];
            stream.read_exact(&mut ext).await.expect("read extended len 16");
            payload_len = u16::from_be_bytes(ext) as usize;
        } else if payload_len == 127 {
            let mut ext = [0u8; 8];
            stream.read_exact(&mut ext).await.expect("read extended len 64");
            payload_len = u64::from_be_bytes(ext) as usize;
        }

        let mut payload = vec![0u8; payload_len];
        if payload_len > 0 {
            stream.read_exact(&mut payload).await.expect("read payload bytes");
        }

        (opcode, payload)
    }

    async fn send_client_frame(stream: &mut TcpStream, opcode: u8, data: &[u8]) {
        let mut frame = Vec::new();
        frame.push(0x80 | (opcode & 0x0F));

        let mask_key = [0x12, 0x34, 0x56, 0x78];
        let len = data.len();
        if len < 126 {
            frame.push(0x80 | (len as u8));
        } else if len <= 65535 {
            frame.push(0x80 | 126);
            frame.extend_from_slice(&(len as u16).to_be_bytes());
        } else {
            frame.push(0x80 | 127);
            frame.extend_from_slice(&(len as u64).to_be_bytes());
        }

        frame.extend_from_slice(&mask_key);

        for (i, &byte) in data.iter().enumerate() {
            frame.push(byte ^ mask_key[i % 4]);
        }

        stream.write_all(&frame).await.expect("write frame");
    }

    #[test]
    fn test_ws_supervisor_config_defaults() {
        let config = WsSupervisorConfig::default();
        assert_eq!(config.heartbeat_interval, Duration::from_secs(25));
        assert_eq!(config.client_timeout, Duration::from_secs(60));
    }

    #[tokio::test]
    async fn test_ws_handshake_upgrade_success() {
        let (port, _) = spawn_test_server().await;
        let mut stream = connect_ws(port).await;

        send_client_frame(&mut stream, 0x9, b"health_check").await;
        let (opcode, payload) = read_server_frame(&mut stream).await;
        assert_eq!(opcode, 0xA);
        assert_eq!(&payload, b"health_check");
    }

    #[tokio::test]
    async fn test_ws_handshake_missing_upgrade_header_rejected() {
        let (port, _) = spawn_test_server().await;
        let mut stream = TcpStream::connect(format!("127.0.0.1:{port}"))
            .await
            .expect("tcp connect");

        let req = format!(
            "GET /ws/live HTTP/1.1\r\n\
             Host: 127.0.0.1:{port}\r\n\
             Connection: close\r\n\r\n"
        );
        stream.write_all(req.as_bytes()).await.expect("write request");

        let mut buf = [0u8; 512];
        let n = stream.read(&mut buf).await.expect("read response");
        let resp = String::from_utf8_lossy(&buf[..n]);
        assert!(!resp.starts_with("HTTP/1.1 101 Switching Protocols"));
        assert!(resp.starts_with("HTTP/1.1 426") || resp.starts_with("HTTP/1.1 400"));
    }

    #[tokio::test]
    async fn test_ws_broadcast_telemetry_forwarding() {
        let (port, state) = spawn_test_server().await;
        let mut stream = connect_ws(port).await;

        let sample_payload = r#"{"device_id":3735928559,"temp":-1850,"door_open":false}"#;
        state.live_tx.send(sample_payload.to_string()).expect("broadcast send");

        let (opcode, payload) = read_server_frame(&mut stream).await;
        assert_eq!(opcode, 1, "expected text frame");
        assert_eq!(String::from_utf8_lossy(&payload), sample_payload);
    }

    #[tokio::test]
    async fn test_ws_multiple_concurrent_subscribers() {
        let (port, state) = spawn_test_server().await;
        let mut client1 = connect_ws(port).await;
        let mut client2 = connect_ws(port).await;

        let payload_msg = r#"{"stream":"telemetry_event_v1"}"#;
        state.live_tx.send(payload_msg.to_string()).expect("broadcast send");

        let (op1, payload1) = read_server_frame(&mut client1).await;
        let (op2, payload2) = read_server_frame(&mut client2).await;

        assert_eq!(op1, 1);
        assert_eq!(op2, 1);
        assert_eq!(String::from_utf8_lossy(&payload1), payload_msg);
        assert_eq!(String::from_utf8_lossy(&payload2), payload_msg);
    }

    #[tokio::test]
    async fn test_ws_ping_pong_roundtrip() {
        let (port, _) = spawn_test_server().await;
        let mut stream = connect_ws(port).await;

        send_client_frame(&mut stream, 0x9, b"sentinel-ping-data").await;

        let (opcode, payload) = read_server_frame(&mut stream).await;
        assert_eq!(opcode, 0xA, "expected Pong frame");
        assert_eq!(&payload, b"sentinel-ping-data");
    }

    #[tokio::test]
    async fn test_ws_client_initiated_clean_close() {
        let (port, _) = spawn_test_server().await;
        let mut stream = connect_ws(port).await;

        send_client_frame(&mut stream, 0x8, &[]).await;

        let mut buf = [0u8; 128];
        let n = stream.read(&mut buf).await.expect("read after close");
        if n > 0 {
            let opcode = buf[0] & 0x0F;
            assert_eq!(opcode, 0x8, "expected close acknowledgement");
        }
    }

    #[tokio::test]
    async fn test_ws_supervisor_periodic_heartbeat_emission() {
        let (port, _) = spawn_test_server_with_config(WsSupervisorConfig {
            heartbeat_interval: Duration::from_millis(50),
            client_timeout: Duration::from_millis(500),
        })
        .await;

        let mut stream = connect_ws(port).await;

        let (opcode, _) = read_server_frame(&mut stream).await;
        assert_eq!(opcode, 0x9, "expected proactive server Ping frame");
    }

    #[tokio::test]
    async fn test_ws_supervisor_inactivity_timeout_disconnect() {
        let (port, _) = spawn_test_server_with_config(WsSupervisorConfig {
            heartbeat_interval: Duration::from_millis(20),
            client_timeout: Duration::from_millis(60),
        })
        .await;

        let mut stream = connect_ws(port).await;

        tokio::time::sleep(Duration::from_millis(120)).await;

        let mut buf = [0u8; 128];
        let n = stream.read(&mut buf).await.unwrap_or(0);
        if n > 0 {
            let opcode = buf[0] & 0x0F;
            assert!(opcode == 0x8 || opcode == 0x9);
        }
    }

    #[tokio::test]
    async fn test_ws_large_payload_forwarding() {
        let (port, state) = spawn_test_server().await;
        let mut stream = connect_ws(port).await;

        let large_msg = format!(
            r#"{{"device_id":3735928559,"details":"{}","active":true}}"#,
            "X".repeat(300)
        );
        state.live_tx.send(large_msg.clone()).expect("broadcast send");

        let (opcode, payload) = read_server_frame(&mut stream).await;
        assert_eq!(opcode, 1);
        assert_eq!(String::from_utf8_lossy(&payload), large_msg);
    }
}
