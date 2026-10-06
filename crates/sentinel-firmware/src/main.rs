#![no_std]
#![no_main]
use embassy_executor::Spawner;
use embassy_sync::{blocking_mutex::raw::CriticalSectionRawMutex, channel::Channel};
use embassy_time::{Duration, Ticker};
use esp_hal::gpio::Flex;
use esp_println::println;


use sentinel_core::{
    alert::{AlertEngine, Severity, Thresholds},
    health,
    telemetry::{DeviceId, FreezerReading},
};

mod onewire;
mod publisher;
mod sensor;

pub static TELEMETRY_CHANNEL: Channel<CriticalSectionRawMutex, FreezerReading, 8> = Channel::new();

const SAMPLE_INTERVAL_SECS: u64 = 10;

const MAX_SENSOR_RETRIES: u8 = 3;


#[esp_hal::main]
async fn main(spawner: Spawner) -> ! {
    // 1. Initialize ESP32-S3 system peripherals, power rails, and clocks
    let peripherals = esp_hal::init(esp_hal::Config::default());
    println!();
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    println!("          SENTINEL Production Firmware v0.1.0             ");
    println!("                 Target: ESP32-S3                           ");
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");

// 2. Extract silicon-unique hardware ID from factory eFuse MAC
    let base_mac = esp_hal::efuse::Efuse::read_base_mac();
    let device_id = DeviceId::from_mac_tail(base_mac);
    let hex_id = device_id.as_hex();
    let hex_str = core::str::from_utf8(&hex_id).unwrap_or("UNKNOWN");

    println!(
        "[SILICON] MAC: {:02X}:{:02X}:{:02X}:{:02X}:{:02X}:{:02X} | Node ID: 0x{:08X} ({})",
        base_mac[0], base_mac[1], base_mac[2], base_mac[3], base_mac[4], base_mac[5],
        device_id.0, hex_str
    );


    // 3. Spawn background MQTT publishing worker
    match spawner.spawn(publisher::mqtt_publish_task()) {
        Ok(()) => println!("[BOOT]  MQTT Telemetry Publisher online."),
        Err(_) => println!("[BOOT]  Failed to spawn MQTT publisher task slot!"),
    }

    // 4. Safely initialize GPIO 6 via peripheral ownership (100% safe, no steal())
    let mut sensor_pin = Flex::new(peripherals.GPIO6);

    // 5. Initialize hysteresis alert state machine
    let mut alert_engine = AlertEngine::new(Thresholds::home_freezer());
    
    // 6. Setup drift-free periodic timer
    let mut ticker = Ticker::every(Duration::from_secs(SAMPLE_INTERVAL_SECS));

    let mut uptime_secs: u64 = 0;
    let mut consecutive_faults: u32 = 0;

    println!(
        "[BOOT] 1-Wire active on GPIO 6. Starting telemetry loop ({}s interval)...",
        SAMPLE_INTERVAL_SECS
    );
    println!("──────────────────────────────────────────────────────────────────────────");
    
 loop {
    // Sample DS18B20 with non-blocking async conversion
    let temp_result = sensor::read_temperature_async(&mut sensor_pin, MAX_SENSOR_RETRIES).await;

    match temp_result {
        Ok(temp) => {
            consecutive_faults = 0;
            let temp_cc = temp.raw();

            // Construct telemetry reading snapshot
            let mut reading = FreezerReading {
                device_id: device_id.0,
                timestamp_secs: uptime_secs,
                temp_centi_c: temp_cc,
                door_open: false,
                door_open_secs: 0,
                health_score: 0,
                rssi_dbm: -48,
                firmware_version: 10000,
            };

            // Mathematical health score calculation
            reading.health_score = health::calculate(&reading);

            // Evaluate thresholds with hysteresis
            let severity = alert_engine.evaluate(&reading);
            let (t_int, t_frac) = reading.temp_display_parts();
            let sign = if temp_cc < 0 { "-" } else { "+" };
            
             println!(
                "[{:>6}s] Node: {} | Temp: {}{}.{:02}°C | Score: {:>4}/1000 | Status: {}",
                uptime_secs,
                hex_str,
                sign,
                t_int.abs(),
                t_frac,
                reading.health_score,
                match severity {
                    Severity::Healthy => " HEALTHY",
                    Severity::Warning => "  WARNING",
                    Severity::Critical => " CRITICAL",
                }
            );
            
            // Dispatch active alerts if triggered
            if let Some(alert) = alert_engine.try_alert(&reading) {
                println!("╔═════════════════════════════════════════════════════════════════╗");
                println!("║  DISPATCHING ALERT: {:<40} ║", alert.message);
                println!("║  Target Severity: {:<45} ║", alert.severity);
                println!("╚═════════════════════════════════════════════════════════════════╝");
            }

            // Push reading to async MQTT channel (non-blocking)
            if TELEMETRY_CHANNEL.try_send(reading).is_err() {
                println!("[WARN] Telemetry buffer saturated. Reading dropped.");
            }
        }

        Err(err) => {
            consecutive_faults += 1;
            println!(
                "[{:>6}s]   SENSOR FAULT: {err} (Consecutive: {consecutive_faults})",
                uptime_secs
            );
            println!("         Diagnostic: Verify 4.7kΩ pull-up resistor on GPIO 6.");
        }
    }
    uptime_secs += SAMPLE_INTERVAL_SECS;
    // Zero-drift sleep until the next scheduled tick
    ticker.next().await;
    }
}

            