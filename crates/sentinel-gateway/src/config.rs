
use serde::Deserialize;
use std::fmt;
use std::fs;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};

/// Gateway runtime configuration.
#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct GatewayConfig {
    /// HTTP server listen socket (e.g., "0.0.0.0:8080").
    pub bind_addr: SocketAddr,

    /// SQLite database storage path.
    pub db_path: PathBuf,

    /// MQTT broker connection parameters.
    pub mqtt: MqttConfig,

    /// Outbound alerting & notification services.
    pub alerts: AlertConfig,
}

impl Default for GatewayConfig {
    fn default() -> Self {
        Self {
            bind_addr: SocketAddr::from(([0, 0, 0, 0], 8080)),
            db_path: PathBuf::from("sentinel.db"),
            mqtt: MqttConfig::default(),
            alerts: AlertConfig::default(),
        }
    }
}

/// MQTT broker connectivity configuration.
#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct MqttConfig {
    /// MQTT broker host address or hostname.
    pub host: String,

    /// MQTT broker TCP port.
    pub port: u16,

    /// Unique MQTT client identifier for this gateway instance.
    pub client_id: String,

    /// Wildcard topic subscription filter.
    pub topic_filter: String,
}

impl Default for MqttConfig {
    fn default() -> Self {
        Self {
            host: String::from("localhost"),
            port: 1883,
            client_id: String::from("sentinel-gateway-01"),
            topic_filter: String::from("sentinel/+/telemetry"),
        }
    }
}

/// Alert notification integrations.
#[derive(Clone, Deserialize, Default)]
#[serde(default)]
pub struct AlertConfig {
    /// Enable WhatsApp delivery via CallMeBot.
    pub whatsapp_enabled: bool,
    /// Target WhatsApp phone number with country prefix.
    pub whatsapp_phone: String,
    /// CallMeBot authentication key.
    pub whatsapp_api_key: String,

    /// Enable Telegram bot delivery.
    pub telegram_enabled: bool,
    /// Telegram Bot token.
    pub telegram_bot_token: String,
    /// Telegram destination chat ID.
    pub telegram_chat_id: String,
}

// Custom Debug implementation to prevent credential leaks in logging/tracing
impl fmt::Debug for AlertConfig {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("AlertConfig")
            .field("whatsapp_enabled", &self.whatsapp_enabled)
            .field("whatsapp_phone", &self.whatsapp_phone)
            .field(
                "whatsapp_api_key",
                &if self.whatsapp_api_key.is_empty() { "<empty>" } else { "[REDACTED]" },
            )
            .field("telegram_enabled", &self.telegram_enabled)
            .field("telegram_chat_id", &self.telegram_chat_id)
            .field(
                "telegram_bot_token",
                &if self.telegram_bot_token.is_empty() { "<empty>" } else { "[REDACTED]" },
            )
            .finish()
    }
}
// Errors & Loading Logic

#[derive(Debug)]
pub enum ConfigError {
    Io(std::io::Error),
    Parse(toml::de::Error),
}

impl fmt::Display for ConfigError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(err) => write!(f, "config I/O fault: {err}"),
            Self::Parse(err) => write!(f, "TOML parsing fault: {err}"),
        }
    }
}

impl std::error::Error for ConfigError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(err) => Some(err),
            Self::Parse(err) => Some(err),
        }
    }
}

impl From<std::io::Error> for ConfigError {
    fn from(err: std::io::Error) -> Self {
        Self::Io(err)
    }
}

impl From<toml::de::Error> for ConfigError {
    fn from(err: toml::de::Error) -> Self {
        Self::Parse(err)
    }
}

impl GatewayConfig {
    pub fn load(path: impl AsRef<Path>) -> Result<Self, ConfigError> {
        let path = path.as_ref();

        match fs::read_to_string(path) {
            Ok(content) => {
                let config: Self = toml::from_str(&content)?;
                Ok(config)
            }
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
                tracing::warn!(
                    path = %path.display(),
                    "Config file not found, applying built-in defaults"
                );
                Ok(Self::default())
            }
            Err(err) => Err(ConfigError::Io(err)),
        }
    }
}
