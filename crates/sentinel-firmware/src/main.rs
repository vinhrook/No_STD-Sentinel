#![no_std]
#![no_main]
#![feature(impl_trait_in_assoc_type)]

use embassy_executor::Spawner;
use embassy_net::{Config as NetConfig, Runner, Stack, StackResources};
use embassy_time::{Duration, Ticker, Timer};
use esp_alloc::heap_allocator;
use esp_backtrace as _;
use esp_hal::{
    delay::Delay,
    gpio::{Flex, Pull},
    rng::Rng,
    timer::timg::TimerGroup,
};
use esp_println::println;
use esp_wifi::wifi::{
    ClientConfiguration, Configuration, WifiController, WifiDevice, WifiStaDevice,
};
use static_cell::StaticCell;

use sentinel_core::{
    alert::{AlertEngine, Severity, Thresholds},
    health,
    telemetry::{DeviceId, FirmwareVersion, FreezerReading, HealthScore, TempCentiC},
};
#[allow(unused_imports)]
use sentinel_firmware::app_desc as _;
use sentinel_firmware::{
    config,
    onewire::{self, OneWire},
    ota, publisher, sensor, MqttPayload, TELEMETRY_CHANNEL,
};

static WIFI_INIT: StaticCell<esp_wifi::EspWifiController<'static>> = StaticCell::new();
static STACK_RESOURCES: StaticCell<StackResources<8>> = StaticCell::new();
static STACK: StaticCell<Stack<'static>> = StaticCell::new();

const SAMPLE_INTERVAL_SECS: u64 = 2;
const MAX_SENSOR_RETRIES: u8 = 2;

#[esp_hal_embassy::main]
async fn main(spawner: Spawner) -> ! {
    heap_allocator!(72 * 1024);

    let peripherals = esp_hal::init(esp_hal::Config::default());

    let timg0 = esp_hal::timer::timg::TimerGroup::new(peripherals.TIMG0);
    esp_hal_embassy::init(timg0.timer0);

    let timg1 = TimerGroup::new(peripherals.TIMG1);
    let esp_wifi_ctrl = WIFI_INIT.init(
        esp_wifi::init(
            timg1.timer0,
            Rng::new(peripherals.RNG),
            peripherals.RADIO_CLK,
        )
        .unwrap(),
    );

    let (wifi_interface, controller) = esp_wifi::wifi::new_with_mode(
        esp_wifi_ctrl,
        peripherals.WIFI,
        WifiStaDevice,
    )
    .unwrap();

    let net_config = NetConfig::dhcpv4(Default::default());
    let (stack, runner) = embassy_net::new(
        wifi_interface,
        net_config,
        STACK_RESOURCES.init(StackResources::new()),
        1234,
    );
    let stack = STACK.init(stack);

    if let Err(_) = spawner.spawn(wifi_connection_task(controller)) {
        println!("[WIFI] Failed to spawn Wi-Fi connection task!");
    }
    if let Err(_) = spawner.spawn(net_task(runner)) {
        println!("[WIFI] Failed to spawn Embassy-net task!");
    }

    println!("[WIFI] Hardware initialized. Connecting to '{}'...", config::WIFI_SSID);

    println!();
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    println!(
        "          SENTINEL Production Firmware v{}.{}.{}             ",
        config::FW_VERSION_MAJOR,
        config::FW_VERSION_MINOR,
        config::FW_VERSION_PATCH
    );
    println!("                 Target: ESP32-S3                           ");
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");

    let base_mac = esp_hal::efuse::Efuse::read_base_mac_address();
    let device_id = DeviceId::from_mac_tail(base_mac);
    let hex_id = device_id.as_hex();
    let hex_str = core::str::from_utf8(&hex_id).unwrap_or("UNKNOWN");

    println!(
        "[SILICON] MAC: {:02X}:{:02X}:{:02X}:{:02X}:{:02X}:{:02X} | Node ID: 0x{:08X} ({})",
        base_mac[0], base_mac[1], base_mac[2], base_mac[3], base_mac[4], base_mac[5],
        device_id.as_u32(), hex_str
    );

    match spawner.spawn(publisher::mqtt_publish_task(*stack)) {
        Ok(()) => println!("[BOOT]  MQTT Telemetry Publisher online."),
        Err(_) => println!("[BOOT]  Failed to spawn MQTT publisher task slot!"),
    }

    match spawner.spawn(ota::ota_listener_task(*stack)) {
        Ok(()) => println!("[BOOT]  Wireless OTA Flasher online (Port 8032)."),
        Err(_) => println!("[BOOT]  Failed to spawn OTA listener task slot!"),
    }

    let mut boot_log = heapless::String::<128>::new();
    let _ = core::fmt::write(
        &mut boot_log,
        format_args!("[BOOT] Sentinel Node 0x{:08X} online | Wireless link ready", device_id.as_u32()),
    );
    let _ = TELEMETRY_CHANNEL.try_send(MqttPayload::Log {
        device_id,
        message: boot_log,
    });

    let mut sensor_pin = Flex::new(peripherals.GPIO6);
    sensor_pin.set_as_open_drain(Pull::Up);
    sensor_pin.set_high();

    Timer::after(Duration::from_millis(200)).await;

    println!("[BOOT] 1-Wire active on GPIO 6. Scanning bus for DS18B20 sensors...");
    let mut discovered_roms: heapless::Vec<onewire::RomAddress, 4> = heapless::Vec::new();

    for attempt in 1..=3 {
        let mut bus = OneWire::new(&mut sensor_pin, Delay::new());
        match bus.search_all::<4>() {
            Ok(roms) if !roms.is_empty() => {
                println!(
                    "[BOOT] ✅ Discovered {} DS18B20 sensor(s) on 1-Wire bus (Attempt {attempt}):",
                    roms.len()
                );
                for (i, rom) in roms.iter().enumerate() {
                    let role = if i == 0 { "Internal / Primary" } else { "Ambient / Secondary" };
                    println!(
                        "       -> Sensor #{i} [{role}]: {:02X}-{:02X}-{:02X}-{:02X}-{:02X}-{:02X}-{:02X}-{:02X}",
                        rom[0], rom[1], rom[2], rom[3], rom[4], rom[5], rom[6], rom[7]
                    );
                }
                discovered_roms = roms;
                break;
            }
            Ok(_) => {
                println!("[BOOT] ⚠️ Attempt {attempt}/3: No devices found, retrying...");
            }
            Err(e) => {
                println!("[BOOT] ⚠️ Attempt {attempt}/3 bus scan error: {e:?}, recovering bus...");
                let _ = bus.recover();
            }
        }
        Timer::after(Duration::from_millis(50)).await;
    }

    const HARDWARE_ROMS: [onewire::RomAddress; 2] = [
        [0x28, 0xCA, 0x4A, 0x23, 0x00, 0x00, 0x00, 0xE2],
        [0x28, 0xA1, 0x18, 0x23, 0x00, 0x00, 0x00, 0xCF],
    ];

    if discovered_roms.is_empty() {
        println!("[BOOT] ℹ️ Applying known hardware ROMs for dual DS18B20 sensors:");
        let _ = discovered_roms.push(HARDWARE_ROMS[0]);
        let _ = discovered_roms.push(HARDWARE_ROMS[1]);
        for (i, rom) in discovered_roms.iter().enumerate() {
            let role = if i == 0 { "Internal / Primary" } else { "Ambient / Secondary" };
            println!(
                "       -> Sensor #{i} [{role}]: {:02X}-{:02X}-{:02X}-{:02X}-{:02X}-{:02X}-{:02X}-{:02X}",
                rom[0], rom[1], rom[2], rom[3], rom[4], rom[5], rom[6], rom[7]
            );
        }
    }

    let mut alert_engine = AlertEngine::new(Thresholds::home_freezer());
    
    let mut ticker = Ticker::every(Duration::from_secs(SAMPLE_INTERVAL_SECS));

    let mut uptime_secs: u64 = 0;
    let mut consecutive_faults: u32 = 0;
    let mut last_valid_temp: Option<sensor::CentiCelsius> = None;
    let mut consecutive_ambient_faults: u32 = 0;
    let mut last_valid_ambient: Option<sensor::CentiCelsius> = None;

    println!(
        "[BOOT] Starting telemetry loop ({}s interval)...",
        SAMPLE_INTERVAL_SECS
    );
    println!("──────────────────────────────────────────────────────────────────────────");
    
    loop {
        let dual_reading = sensor::read_dual_temperatures_async(
            &mut sensor_pin,
            &discovered_roms,
            MAX_SENSOR_RETRIES,
        )
        .await;

        let (active_temp, ambient_temp) = match (dual_reading.internal, dual_reading.ambient) {
            (Ok(int_t), Some(Ok(amb_t))) => {
                consecutive_faults = 0;
                last_valid_temp = Some(int_t);
                consecutive_ambient_faults = 0;
                last_valid_ambient = Some(amb_t);
                (Some(int_t), Some(amb_t))
            }
            (Ok(int_t), amb_opt) => {
                consecutive_faults = 0;
                last_valid_temp = Some(int_t);
                if let Some(Err(err)) = amb_opt {
                    consecutive_ambient_faults += 1;
                    if consecutive_ambient_faults <= 3 {
                        println!(
                            "[{:>6}s] ⚠️ AMBIENT SENSOR FAULT: {err} (Consecutive: {consecutive_ambient_faults})",
                            uptime_secs
                        );
                    }
                }
                (Some(int_t), None)
            }
            (Err(_), Some(Ok(amb_t))) => {
                consecutive_faults = 0;
                last_valid_temp = Some(amb_t);
                consecutive_ambient_faults = 0;
                last_valid_ambient = Some(amb_t);
                (Some(amb_t), None)
            }
            (Err(err), _) => {
                consecutive_faults += 1;
                println!(
                    "[{:>6}s] ⚠️ SENSOR FAULT: {err} (Consecutive: {consecutive_faults})",
                    uptime_secs
                );
                let mut fault_log = heapless::String::<128>::new();
                let _ = core::fmt::write(
                    &mut fault_log,
                    format_args!("[FAULT] Sensor error: {err} (Consecutive: {consecutive_faults})"),
                );
                let _ = TELEMETRY_CHANNEL.try_send(MqttPayload::Log {
                    device_id,
                    message: fault_log,
                });
                let cached = if let Some(cached) = last_valid_temp {
                    if consecutive_faults <= 15 {
                        println!(
                            "[{:>6}s] ℹ️ Using cached temperature ({cached}) for continuous stream",
                            uptime_secs
                        );
                        Some(cached)
                    } else {
                        println!("         Diagnostic: Verify 4.7kΩ pull-up resistor on GPIO 6.");
                        None
                    }
                } else {
                    println!("         Diagnostic: Verify 4.7kΩ pull-up resistor on GPIO 6.");
                    None
                };
                (cached, None)
            }
        };

        if let Some(temp) = active_temp {
            let temp_cc = temp.raw();

            let mut reading = FreezerReading {
                device_id,
                timestamp_secs: uptime_secs,
                temp: TempCentiC::from_centi(temp_cc),
                door_open: false,
                door_open_secs: 0,
                health_score: HealthScore::ZERO,
                rssi_dbm: -48,
                firmware_version: FirmwareVersion::new(
                    config::FW_VERSION_MAJOR,
                    config::FW_VERSION_MINOR,
                    config::FW_VERSION_PATCH,
                ),
            };

            reading.health_score = health::calculate(&reading);

            let severity = alert_engine.evaluate(&reading);
            let parts = reading.temp_display_parts();
            let sign = if parts.is_negative { "-" } else { "+" };
            
            if let Some(amb) = ambient_temp {
                let delta = amb.to_celsius() - (reading.temp.as_centi() as f32 / 100.0);
                println!(
                    "[{:>6}s] Node: {} | Temp: {}{}.{:02}°C | Amb: {} | ΔT: {:+.2}°C | Score: {:>4}/1000 | Status: {}{}",
                    uptime_secs,
                    hex_str,
                    sign,
                    parts.whole,
                    parts.centis,
                    amb,
                    delta,
                    reading.health_score.as_u16(),
                    match severity {
                        Severity::Healthy => " HEALTHY",
                        Severity::Warning => "  WARNING",
                        Severity::Critical => " CRITICAL",
                    },
                    if consecutive_faults > 0 { " (CACHED)" } else { "" }
                );
            } else {
                println!(
                    "[{:>6}s] Node: {} | Temp: {}{}.{:02}°C | Score: {:>4}/1000 | Status: {}{}",
                    uptime_secs,
                    hex_str,
                    sign,
                    parts.whole,
                    parts.centis,
                    reading.health_score.as_u16(),
                    match severity {
                        Severity::Healthy => " HEALTHY",
                        Severity::Warning => "  WARNING",
                        Severity::Critical => " CRITICAL",
                    },
                    if consecutive_faults > 0 { " (CACHED)" } else { "" }
                );
            }
            
            if let Some(alert) = alert_engine.try_alert(&reading) {
                println!("╔═════════════════════════════════════════════════════════════════╗");
                println!("║  DISPATCHING ALERT: {:<40} ║", alert.message);
                println!("║  Target Severity: {:<45} ║", alert.severity);
                println!("╚═════════════════════════════════════════════════════════════════╝");

                let mut alert_log = heapless::String::<128>::new();
                let _ = core::fmt::write(&mut alert_log, format_args!("[ALERT] {}", alert.message));
                let _ = TELEMETRY_CHANNEL.try_send(MqttPayload::Log {
                    device_id,
                    message: alert_log,
                });
            }

            let mut status_log = heapless::String::<128>::new();
            let _ = core::fmt::write(
                &mut status_log,
                format_args!(
                    "[{:>6}s] Temp: {}{}.{:02}°C | Score: {:>4}/1000 | Status: {}{}",
                    uptime_secs,
                    sign,
                    parts.whole,
                    parts.centis,
                    reading.health_score.as_u16(),
                    match severity {
                        Severity::Healthy => "HEALTHY",
                        Severity::Warning => "WARNING",
                        Severity::Critical => "CRITICAL",
                    },
                    if consecutive_faults > 0 { " (CACHED)" } else { "" }
                ),
            );
            let _ = TELEMETRY_CHANNEL.try_send(MqttPayload::Log {
                device_id,
                message: status_log,
            });

            let ambient_cc = ambient_temp.map(|t| t.raw());
            if TELEMETRY_CHANNEL
                .try_send(MqttPayload::Telemetry {
                    reading,
                    ambient_cc,
                })
                .is_err()
            {
                println!("[WARN] Telemetry buffer saturated. Reading dropped.");
            }
        }

        uptime_secs += SAMPLE_INTERVAL_SECS;
        ticker.next().await;
    }
}

#[embassy_executor::task]
async fn wifi_connection_task(mut controller: WifiController<'static>) {
    println!("[WIFI] Starting connection manager...");
    let client_config = Configuration::Client(ClientConfiguration {
        ssid: heapless::String::try_from(config::WIFI_SSID).unwrap(),
        password: heapless::String::try_from(config::WIFI_PASS).unwrap(),
        ..Default::default()
    });

    controller.set_configuration(&client_config).unwrap();
    controller.start_async().await.unwrap();

    loop {
        match esp_wifi::wifi::wifi_state() {
            esp_wifi::wifi::WifiState::StaConnected => {
                controller.wait_for_event(esp_wifi::wifi::WifiEvent::StaDisconnected).await;
                println!("[WIFI] Disconnected! Reconnecting in 3s...");
                embassy_time::Timer::after(Duration::from_secs(3)).await;
            }
            _ => {}
        }

        match controller.connect_async().await {
            Ok(_) => {
                println!("[WIFI] Connected to AP successfully!");
            }
            Err(e) => {
                println!("[WIFI] Connect failed: {e:?}. Retrying in 5s...");
                embassy_time::Timer::after(Duration::from_secs(5)).await;
            }
        }
    }
}

#[embassy_executor::task]
async fn net_task(mut runner: Runner<'static, WifiDevice<'static, WifiStaDevice>>) {
    runner.run().await
}