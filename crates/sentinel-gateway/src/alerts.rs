use reqwest::Client;
use std::fmt;
use std::time::Duration;

use sentinel_core::alert::AlertEvent;
use crate::config::AlertConfig;

const HTTP_TIMEOUT: Duration = Duration::from_secs(10);

#[derive(Debug)]
pub enum AlertDispatchError {
    Http(reqwest::Error),
    ProviderRejected { status: reqwest::StatusCode, body: String },
}

impl fmt::Display for AlertDispatchError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Http(err) => write!(f, "network transport fault: {err}"),
            Self::ProviderRejected { status, body } => {
                write!(f, "webhook endpoint returned status {status}: {body}")
            }
        }
    }
}

impl std::error::Error for AlertDispatchError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Http(err) => Some(err),
            Self::ProviderRejected { .. } => None,
        }
    }
}

impl From<reqwest::Error> for AlertDispatchError {
    fn from(err: reqwest::Error) -> Self {
        Self::Http(err)
    }
}

pub async fn dispatch(config: &AlertConfig, alert: &AlertEvent) {
    let client = Client::builder()
        .timeout(HTTP_TIMEOUT)
        .build()
        .unwrap_or_default();

    let (whatsapp_res, telegram_res) = tokio::join!(
        async {
            if config.whatsapp_enabled {
                send_whatsapp(&client, config, alert).await
            } else {
                Ok(())
            }
        },
        async {
            if config.telegram_enabled {
                send_telegram(&client, config, alert).await
            } else {
                Ok(())
            }
        }
    );

    if let Err(err) = whatsapp_res {
        tracing::error!(channel = "whatsapp", error = %err, "Alert dispatch failed");
    }

    if let Err(err) = telegram_res {
        tracing::error!(channel = "telegram", error = %err, "Alert dispatch failed");
    }
}

#[inline]
pub fn format_whatsapp_message(alert: &AlertEvent) -> String {
    let (t_int, t_frac) = temp_parts(alert.temp_centi_c);
    format!(
        "🚨 SENTINEL {} Alert!\nNode: {:08X}\nTemp: {}.{:02}°C\nScore: {}/1000\nMessage: {}",
        alert.severity,
        alert.device_id,
        t_int, t_frac,
        alert.health_score,
        alert.message,
    )
}

#[inline]
pub fn format_telegram_message(alert: &AlertEvent) -> String {
    let (t_int, t_frac) = temp_parts(alert.temp_centi_c);
    format!(
        "🚨 <b>SENTINEL {} Alert</b>\n\
         <b>Node:</b> <code>{:08X}</code>\n\
         <b>Temp:</b> {}.{:02}°C\n\
         <b>Health Score:</b> {}/1000\n\
         <b>Event:</b> {}",
        alert.severity,
        alert.device_id,
        t_int, t_frac,
        alert.health_score,
        escape_html(alert.message),
    )
}

async fn send_whatsapp(
    client: &Client,
    config: &AlertConfig,
    alert: &AlertEvent,
) -> Result<(), AlertDispatchError> {
    let message = format_whatsapp_message(alert);

    let response = client
        .get("https://api.callmebot.com/whatsapp.php")
        .query(&[
            ("phone", config.whatsapp_phone.as_str()),
            ("text", message.as_str()),
            ("apikey", config.whatsapp_api_key.as_str()),
        ])
        .send()
        .await?;

    if response.status().is_success() {
        tracing::info!(node = format_args!("{:08X}", alert.device_id), "WhatsApp alert dispatched");
        Ok(())
    } else {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        Err(AlertDispatchError::ProviderRejected { status, body })
    }
}

async fn send_telegram(
    client: &Client,
    config: &AlertConfig,
    alert: &AlertEvent,
) -> Result<(), AlertDispatchError> {
    let message = format_telegram_message(alert);

    let url = format!(
        "https://api.telegram.org/bot{}/sendMessage",
        config.telegram_bot_token
    );

    let payload = serde_json::json!({
        "chat_id": config.telegram_chat_id,
        "text": message,
        "parse_mode": "HTML",
    });

    let response = client.post(&url).json(&payload).send().await?;

    if response.status().is_success() {
        tracing::info!(node = format_args!("{:08X}", alert.device_id), "Telegram alert dispatched");
        Ok(())
    } else {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        Err(AlertDispatchError::ProviderRejected { status, body })
    }
}

#[inline]
pub fn escape_html(input: &str) -> String {
    input
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

#[inline]
pub const fn temp_parts(centi_c: i16) -> (i16, u16) {
    (centi_c / 100, (centi_c % 100).unsigned_abs())
}

#[cfg(test)]
mod tests {
    use super::*;
    use sentinel_core::alert::{AlertCause, AlertEvent, Severity};

    fn sample_alert(temp_centi_c: i16, message: &'static str) -> AlertEvent {
        AlertEvent {
            timestamp_secs: 1_710_000_000,
            message,
            device_id: 0x00A4_F3B2,
            temp_centi_c,
            door_open_secs: 15,
            health_score: 850,
            severity: Severity::Critical,
            cause: AlertCause::TemperatureBreach,
            door_open: false,
        }
    }

    #[test]
    fn test_escape_html_entities() {
        assert_eq!(escape_html(""), "");
        assert_eq!(
            escape_html("Nominal freezer operation"),
            "Nominal freezer operation"
        );
        assert_eq!(
            escape_html("Sensor A & B < C > D"),
            "Sensor A &amp; B &lt; C &gt; D"
        );

        assert_eq!(
            escape_html("<script>alert('compromise & breach')</script>"),
            "&lt;script&gt;alert('compromise &amp; breach')&lt;/script&gt;"
        );
    }

    #[test]
    fn test_temp_parts_calculations() {
        assert_eq!(temp_parts(0), (0, 0));
        assert_eq!(temp_parts(2450), (24, 50));
        assert_eq!(temp_parts(2005), (20, 5));
        assert_eq!(temp_parts(-1825), (-18, 25));
        assert_eq!(temp_parts(-75), (0, 75));
    }

    #[test]
    fn test_format_whatsapp_message() {
        let alert = sample_alert(-1825, "Compressor duty-cycle exceeded");
        let msg = format_whatsapp_message(&alert);

        assert!(msg.starts_with(" SENTINEL CRITICAL Alert!"));
        assert!(msg.contains("Node: 00A4F3B2"));
        assert!(msg.contains("Temp: -18.25°C"));
        assert!(msg.contains("Score: 850/1000"));
        assert!(msg.contains("Message: Compressor duty-cycle exceeded"));
    }

    #[test]
    fn test_format_telegram_message_with_html_sanitization() {
        let alert = sample_alert(450, "Temp > 4°C & Door < 10cm ajar");
        let msg = format_telegram_message(&alert);

        assert!(msg.contains(" <b>SENTINEL CRITICAL Alert</b>"));
        assert!(msg.contains("<b>Node:</b> <code>00A4F3B2</code>"));
        assert!(msg.contains("<b>Temp:</b> 4.50°C"));
        assert!(msg.contains("<b>Health Score:</b> 850/1000"));
        assert!(msg.contains("<b>Event:</b> Temp &gt; 4°C &amp; Door &lt; 10cm ajar"));
        assert!(!msg.contains("Temp > 4°C"));
        assert!(!msg.contains("Door < 10cm"));
    }

    #[test]
    fn test_alert_dispatch_error_display_and_source() {
        let err = AlertDispatchError::ProviderRejected {
            status: reqwest::StatusCode::UNAUTHORIZED,
            body: String::from("Invalid API token"),
        };

        assert_eq!(
            err.to_string(),
            "webhook endpoint returned status 401 Unauthorized: Invalid API token"
        );
        assert!(std::error::Error::source(&err).is_none());
    }

    #[tokio::test]
    async fn test_dispatch_noop_when_all_channels_disabled() {
        let config = AlertConfig {
            whatsapp_enabled: false,
            telegram_enabled: false,
            ..Default::default()
        };
        let alert = sample_alert(-2000, "Routine check");

        dispatch(&config, &alert).await;
    }
}
