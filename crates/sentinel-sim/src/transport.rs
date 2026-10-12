use std::time::Duration;
use rumqttc::{AsyncClient, Event, EventLoop, MqttOptions, QoS};
use thiserror::Error;
use tokio::task::JoinHandle;
use crate::node::MqttWirePayload;

#[derive(Debug, Error)]
pub enum TransportError {
    #[error("MQTT client error: {0}")]
    Client(#[from] rumqttc::ClientError),
    #[error("Failed to serialize telemetry wire frame: {0}")]
    Serialization(#[from] serde_json::Error),
}

pub struct MqttTransport {
    client: AsyncClient,
}

impl MqttTransport {
    pub fn connect(
        host: &str,
        port: u16,
        client_id: &str,
    ) -> (Self, JoinHandle<()>) {
        let mut mqtt_options = MqttOptions::new(client_id, host, port);
        mqtt_options.set_keep_alive(Duration::from_secs(30));
        mqtt_options.set_clean_session(true);

        let (client, eventloop) = AsyncClient::new(mqtt_options, 128);

        let background_handle = tokio::spawn(async move {
            run_eventloop(eventloop).await;
        });

        (Self { client }, background_handle)
    }

    pub async fn publish_telemetry(&self, payload: &MqttWirePayload) -> Result<(), TransportError> {
        let topic = format!("sentinel/{:08X}/telemetry", payload.d);
        let json_bytes = serde_json::to_vec(payload)?;

        self.client
            .publish(topic, QoS::AtLeastOnce, false, json_bytes)
            .await?;

        Ok(())
    }
}

async fn run_eventloop(mut eventloop: EventLoop) {
    loop {
        match eventloop.poll().await {
            Ok(Event::Incoming(rumqttc::Incoming::ConnAck(_))) => {
                tracing::debug!("MQTT transport connected to broker successfully");
            }
            Ok(_) => {}
            Err(e) => {
                tracing::warn!(error = %e, "MQTT connection glitch, retrying in 2 seconds...");
                tokio::time::sleep(Duration::from_secs(2)).await;
            }
        }
    }
}
