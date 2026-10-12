use serde::Deserialize;
use std::fmt;
use std::fs;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct GatewayConfig {
    pub bind_addr: SocketAddr,
    pub db_path: PathBuf,
    pub mqtt: MqttConfig,
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

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct MqttConfig {
    pub host: String,
    pub port: u16,
    pub client_id: String,
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

#[derive(Clone, Deserialize, Default)]
#[serde(default)]
pub struct AlertConfig {
    pub whatsapp_enabled: bool,
    pub whatsapp_phone: String,
    pub whatsapp_api_key: String,

    pub telegram_enabled: bool,
    pub telegram_bot_token: String,
    pub telegram_chat_id: String,
}

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
    #[inline]
    pub fn parse(toml_str: &str) -> Result<Self, toml::de::Error> {
        toml::from_str(toml_str)
    }

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

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::{IpAddr, Ipv4Addr};

    #[test]
    fn test_default_gateway_config() {
        let config = GatewayConfig::default();

        assert_eq!(
            config.bind_addr,
            SocketAddr::new(IpAddr::V4(Ipv4Addr::new(0, 0, 0, 0)), 8080)
        );
        assert_eq!(config.db_path, PathBuf::from("sentinel.db"));
        assert_eq!(config.mqtt.host, "localhost");
        assert_eq!(config.mqtt.port, 1883);
        assert_eq!(config.mqtt.client_id, "sentinel-gateway-01");
        assert_eq!(config.mqtt.topic_filter, "sentinel/+/telemetry");
        assert!(!config.alerts.whatsapp_enabled);
        assert!(config.alerts.whatsapp_phone.is_empty());
        assert!(config.alerts.whatsapp_api_key.is_empty());
        assert!(!config.alerts.telegram_enabled);
        assert!(config.alerts.telegram_bot_token.is_empty());
        assert!(config.alerts.telegram_chat_id.is_empty());
    }

    #[test]
    fn test_alert_config_debug_redaction() {
        let mut alert = AlertConfig::default();
        let debug_empty = format!("{alert:?}");
        assert!(debug_empty.contains("whatsapp_api_key: \"<empty>\""));
        assert!(debug_empty.contains("telegram_bot_token: \"<empty>\""));

        alert.whatsapp_api_key = String::from("top_secret_wa_token");
        alert.telegram_bot_token = String::from("top_secret_tg_token");

        let debug_populated = format!("{alert:?}");
        assert!(!debug_populated.contains("top_secret_wa_token"));
        assert!(!debug_populated.contains("top_secret_tg_token"));
        assert!(debug_populated.contains("whatsapp_api_key: \"[REDACTED]\""));
        assert!(debug_populated.contains("telegram_bot_token: \"[REDACTED]\""));
    }

    #[test]
    fn test_load_nonexistent_file_returns_defaults() {
        let nonexistent = Path::new("target/does_not_exist_config_sentinel.toml");
        let config = GatewayConfig::load(nonexistent).expect("should fall back to defaults");

        assert_eq!(config.bind_addr, GatewayConfig::default().bind_addr);
        assert_eq!(config.db_path, GatewayConfig::default().db_path);
        assert_eq!(config.mqtt.host, "localhost");
    }

    #[test]
    fn test_parse_full_toml_configuration() {
        let toml_str = r#"
bind_addr = "192.168.1.50:9090"
db_path = "/var/lib/sentinel/prod.db"

[mqtt]
host = "mqtt.edge.local"
port = 8883
client_id = "edge-node-alpha"
topic_filter = "enterprise/+/telemetry"

[alerts]
whatsapp_enabled = true
whatsapp_phone = "+15551234567"
whatsapp_api_key = "wa_api_9988"
telegram_enabled = true
telegram_bot_token = "123456:BOT_SECRET"
telegram_chat_id = "-100998877"
"#;

        let config: GatewayConfig = GatewayConfig::parse(toml_str).expect("failed to parse TOML");

        assert_eq!(
            config.bind_addr,
            SocketAddr::new(IpAddr::V4(Ipv4Addr::new(192, 168, 1, 50)), 9090)
        );
        assert_eq!(config.db_path, PathBuf::from("/var/lib/sentinel/prod.db"));
        assert_eq!(config.mqtt.host, "mqtt.edge.local");
        assert_eq!(config.mqtt.port, 8883);
        assert_eq!(config.mqtt.client_id, "edge-node-alpha");
        assert_eq!(config.mqtt.topic_filter, "enterprise/+/telemetry");
        assert!(config.alerts.whatsapp_enabled);
        assert_eq!(config.alerts.whatsapp_phone, "+15551234567");
        assert_eq!(config.alerts.whatsapp_api_key, "wa_api_9988");
        assert!(config.alerts.telegram_enabled);
        assert_eq!(config.alerts.telegram_bot_token, "123456:BOT_SECRET");
        assert_eq!(config.alerts.telegram_chat_id, "-100998877");
    }

    #[test]
    fn test_parse_partial_toml_with_defaults() {
        let toml_str = r#"
bind_addr = "127.0.0.1:4000"
"#;
        let config: GatewayConfig = GatewayConfig::parse(toml_str).expect("failed to parse partial TOML");

        assert_eq!(
            config.bind_addr,
            SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 4000)
        );
        assert_eq!(config.db_path, PathBuf::from("sentinel.db"));
        assert_eq!(config.mqtt.host, "localhost");
        assert_eq!(config.mqtt.port, 1883);
        assert!(!config.alerts.whatsapp_enabled);
    }

    #[test]
    fn test_parse_invalid_toml_syntax_fails() {
        let invalid_toml = "bind_addr = [not a socket address";
        let res = GatewayConfig::parse(invalid_toml);
        assert!(res.is_err());
    }

    #[test]
    fn test_parse_invalid_socket_addr_fails() {
        let invalid_addr = r#"bind_addr = "999.999.999.999:99999""#;
        let res = GatewayConfig::parse(invalid_addr);
        assert!(res.is_err());
    }

    #[test]
    fn test_config_error_display_and_source() {
        let io_err = ConfigError::Io(std::io::Error::new(
            std::io::ErrorKind::PermissionDenied,
            "access denied",
        ));
        assert!(io_err.to_string().contains("config I/O fault"));
        assert!(std::error::Error::source(&io_err).is_some());

        let parse_err = GatewayConfig::parse("invalid = syntax [[[");
        let config_parse_err = ConfigError::Parse(parse_err.unwrap_err());
        assert!(config_parse_err.to_string().contains("TOML parsing fault"));
        assert!(std::error::Error::source(&config_parse_err).is_some());
    }

    #[test]
    fn test_load_from_filesystem_temp_file() {
        let temp_dir = std::env::temp_dir();
        let temp_path = temp_dir.join(format!("sentinel_test_config_{}.toml", std::process::id()));

        let toml_content = r#"
bind_addr = "127.0.0.1:8888"
db_path = "/tmp/test.db"
[mqtt]
host = "broker.test"
"#;
        std::fs::write(&temp_path, toml_content).expect("failed to write temp config");

        let loaded = GatewayConfig::load(&temp_path).expect("failed to load temp config");
        let _ = std::fs::remove_file(&temp_path);

        assert_eq!(
            loaded.bind_addr,
            SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 8888)
        );
        assert_eq!(loaded.db_path, PathBuf::from("/tmp/test.db"));
        assert_eq!(loaded.mqtt.host, "broker.test");
        assert_eq!(loaded.mqtt.port, 1883);
    }

    #[test]
    fn test_deploy_gateway_toml_matches_schema() {
        let deploy_toml = include_str!("../../../deploy/gateway.toml");
        let config: GatewayConfig = toml::from_str(deploy_toml)
            .expect("deploy/gateway.toml must be valid and deserialize cleanly into GatewayConfig");

        assert_eq!(
            config.bind_addr,
            SocketAddr::new(IpAddr::V4(Ipv4Addr::new(0, 0, 0, 0)), 8080)
        );
        assert_eq!(config.db_path, PathBuf::from("/data/sentinel.db"));
        assert_eq!(config.mqtt.host, "mosquitto");
        assert_eq!(config.mqtt.port, 1883);
        assert_eq!(config.mqtt.client_id, "sentinel-gateway-01");
        assert_eq!(config.mqtt.topic_filter, "sentinel/+/telemetry");
        assert!(!config.alerts.whatsapp_enabled);
        assert!(!config.alerts.telegram_enabled);
    }
}
