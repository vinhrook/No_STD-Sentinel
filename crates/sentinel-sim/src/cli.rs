use clap::Parser;
use crate::scenario::ScenarioKind;

#[derive(Debug, Clone, Parser)]
#[command(
    name = "sentinel-sim",
    author = "Sentinel Team",
    version,
    about = "Simulates concurrent bare-metal ESP32-S3 nodes with dual DS18B20 1-Wire sensors over MQTT",
    long_about = "Spawns concurrent virtual ESP32-S3 nodes modeling Newton's law of cooling, thermostat hysteresis, 12-bit ADC quantization, and chaos scenarios, streaming directly to the Sentinel Gateway."
)]
pub struct CliArgs {
    #[arg(short = 'n', long, default_value_t = 3)]
    pub nodes: usize,

    #[arg(short = 's', long, default_value = "normal")]
    pub scenario: ScenarioKind,

    #[arg(short = 'i', long, default_value_t = 2000)]
    pub interval_ms: u64,

    #[arg(short = 'H', long, default_value = "localhost")]
    pub mqtt_host: String,

    #[arg(short = 'p', long, default_value_t = 1883)]
    pub mqtt_port: u16,

    #[arg(long, default_value = "sentinel-sim-cluster")]
    pub client_id_prefix: String,
}
