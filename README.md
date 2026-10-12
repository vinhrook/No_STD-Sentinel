# SENTINEL

**Industrial Cold-Chain Cryo-Telemetry & Predictive Safety Platform**

[![Rust Version](https://img.shields.io/badge/rust-1.80%2B-orange?logo=rust)](https://www.rust-lang.org/)
[![Embedded](https://img.shields.io/badge/target-no__std%20%7C%20ESP32--S3-blue?logo=espressif)](https://www.espressif.com/en/products/socs/esp32-s3)
[![Async Runtime](https://img.shields.io/badge/async-Embassy%20%7C%20Tokio-blueviolet)](https://embassy.dev/)
[![Frontend](https://img.shields.io/badge/frontend-Leptos%20(WASM)-teal?logo=webassembly)](https://leptos.dev/)
[![Storage](https://img.shields.io/badge/database-SQLite%20(WAL%20Mode)-003B57?logo=sqlite)](https://www.sqlite.org/)
[![License](https://img.shields.io/badge/license-MIT%20%2F%20Apache--2.0-blue)](LICENSE)

> **Sentinel** is an enterprise-grade IoT platform engineered for mission-critical cold chain monitoring, industrial freezers, and bio-pharmaceutical storage. Built in end-to-end modern Rust, Sentinel spans from bare-metal `#![no_std]` microcontroller firmware to a high-throughput edge gateway and a real-time WebAssembly dashboard and also beckend Tokio + Axum.

---

## Table of Contents

- [System Architecture](#-system-architecture)
- [Key Features](#-key-features)
- [Workspace Crates](#-workspace-crates)
- [Quickstart (Zero Hardware Required)](#-quickstart-zero-hardware-required)
- [Hardware Deployment (ESP32-S3)](#-hardware-deployment-esp32-s3)
- [Multi-Node Cluster Simulator & Chaos Testing](#-multi-node-cluster-simulator--chaos-testing)
- [Edge Gateway & Alert Pipeline](#-edge-gateway--alert-pipeline)
- [Telemetry Protocol & Data Integrity](#-telemetry-protocol--data-integrity)
- [Running Tests & Quality Assurance](#-running-tests--quality-assurance)
- [License & Authors](#-license--authors)

---

## System Architecture

Sentinel follows an edge-to-cloud decoupled architecture designed for high availability, zero telemetry packet loss, and power-failure resilience:

```text
  ┌─────────────────────────────────────────────────────────────┐
  │                 BARE-METAL EDGE HARDWARE                     │
  │  ESP32-S3 (Xtensa Dual-Core 240MHz) · Rust #![no_std]        │
  │                                                             │
  │   [Dual DS18B20 1-Wire] ──> Bit-Banged Driver (Dallas CRC8) │
  │                                      │                      │
  │   [Embassy Async Runtime] ───> Fault Detection Engine       │
  │                                      │                      │
  │   [esp-wifi / TCP Stack] ───> MQTT Client (Heapless)        │
  └──────────────────────────────┬──────────────────────────────┘
                                 │ TLS / MQTT Telemetry Frames
                                 ▼
  ┌─────────────────────────────────────────────────────────────┐
  │                   EDGE GATEWAY ORCHESTRATION                │
  │  Tokio Core · Axum 0.7 · Rumqttc · SQLite WAL               │
  │                                                             │
  │   ┌──────────────────┐       ┌───────────────────────────┐  │
  │   │  MQTT Ingestion  │ ────> │  SQLite WAL Storage       │  │
  │   │  (rumqttc async) │       │  (Crash-safe edge DB)     │  │
  │   └────────┬─────────┘       └───────────────────────────┘  │
  │            │                                                │
  │            ├─► Multi-Channel Alert Engine                   │
  │            │   ├── Telegram Bot API (HTML sanitization)     │
  │            │   └── WhatsApp CallMeBot REST API              │
  │            │                                                │
  │            └─► 256-Frame WebSocket Broadcast Channel        │
  └──────────────────────────────┬──────────────────────────────┘
                                 │ Full-Duplex WebSockets
                                 ▼
  ┌─────────────────────────────────────────────────────────────┐
  │              REAL-TIME WEB MONITORING DASHBOARD             │
  │  Leptos WebAssembly (Rust) · 60 FPS HTML5 Canvas Charts     │
  │                                                             │
  │  • Sub-millisecond chart re-renders with zero GC pauses     │
  │  • Predictive Health Score & Thermal runaway detection     │
  │  • Automatic WebSocket reconnect with state synchronization  │
  └─────────────────────────────────────────────────────────────┘
```

---

##  Key Features

### 1. Bare-Metal `#![no_std]` Firmware (`sentinel-firmware`)
* **Pure Async on Silicon:** Powered by the **Embassy** bare-metal async runtime on ESP32-S3 dual-core silicon with zero dynamic heap allocation in the hot telemetry path.
* **Bit-Banged 1-Wire Driver:** Direct microsecond-level timing control implementing the Dallas DS18B20 1-Wire protocol with Dallas CRC-8 hardware scratchpad verification.
* **Dual-Probe Redundancy:** Automatic ground-fault detection and seamless sensor failover if a primary probe is shorted or disconnected.
* **Cable-Free Wireless OTA:** Direct TCP over-the-air firmware updates on port 8032 via [`flash_ota.sh`](scripts/flash_ota.sh).

### 2. High-Performance Edge Gateway (`sentinel-gateway`)
* **Asynchronous Tokio & Axum:** Built on Axum 0.7 to serve REST APIs and full-duplex WebSockets concurrently.
* **Crash-Resilient SQLite (WAL Mode):** Write-Ahead Logging guarantees zero corruption during unexpected power disconnects on industrial edge hardware (Raspberry Pi, industrial IPCs).
* **Multi-Channel Alert Dispatcher:** Concurrent asynchronous dispatch to Telegram and WhatsApp with non-blocking timeouts and RFC-3986 parameter escaping.

### 3. Pure Rust WebAssembly Dashboard (`sentinel-dashboard`)
* **Compiled to WASM:** Full client-side reactive web application built with **Leptos** (Rust), avoiding bulky JavaScript frameworks.
* **High-FPS Canvas Charts:** Native HTML5 Canvas rendering for live temperature time-series curves with zero JavaScript overhead.
* **Predictive Health Metric:** Live calculated 0–1000 health index measuring temperature stability, compressor duty cycles, and door ajar duration.

### 4. Cluster Simulator & Chaos Testing (`sentinel-sim`)
* **Thermodynamic Modeling:** Physics-backed simulation using Newton’s Law of Cooling, heat ingress, and thermostat hysteresis.
* **Chaos Engineering:** Inject real-world industrial failures (compressor burnout, sensor cable cuts, thermal runaway) to validate gateway alerts without physical freezers.

---

##  Workspace Crates

| Crate | Target | Role |
| :--- | :--- | :--- |
| [`crates/sentinel-core`](crates/sentinel-core) | `#![no_std]` / `std` | Domain entities, telemetry types, Dallas CRC-8 validation, and health scoring. |
| [`crates/sentinel-firmware`](crates/sentinel-firmware) | `xtensa-esp32s3-none-elf` | Embedded Embassy firmware for ESP32-S3 hardware with 1-Wire & Wi-Fi. |
| [`crates/sentinel-gateway`](crates/sentinel-gateway) | `x86_64` / `aarch64` | Edge gateway backend (Axum, Tokio, SQLite WAL, MQTT ingestion, alerting). |
| [`crates/sentinel-dashboard`](crates/sentinel-dashboard) | `wasm32-unknown-unknown` | Leptos WebAssembly industrial operator dashboard. |
| [`crates/sentinel-sim`](crates/sentinel-sim) | Native | Multi-node cluster simulator with physical thermodynamic modeling. |

---

##  Quickstart (Zero Hardware Required)

You can launch and test the entire platform locally on your computer in under **60 seconds** using the built-in simulator.

### Prerequisites
* [Rust](https://rustup.rs/) (v1.80+ recommended)
* [Docker & Docker Compose](https://docs.docker.com/get-docker/)

### Step 1: Start the MQTT Broker
```bash
# Launch Mosquitto MQTT broker in the background
docker compose -f deploy/docker-compose.yml up -d mosquitto
```

### Step 2: Configure & Start the Sentinel Gateway
```bash
# Copy the example configuration
cp gateway.toml.example gateway.toml

# Launch the Axum Gateway (automatically applies SQLite migrations)
cargo run --release -p sentinel-gateway
```
* The gateway listens on `http://127.0.0.1:8080` (REST API & WebSocket `/ws/live`).

### Step 3: Run the Multi-Node Cluster Simulator
In a separate terminal, launch a cluster of 5 simulated industrial freezer nodes:
```bash
cargo run --release -p sentinel-sim -- --nodes 5 --scenario normal
```

### Step 4: Open the Live Dashboard
Open your browser to:
```
http://localhost:8080
```
You will see all simulated freezer nodes streaming live temperatures, health scores, and metrics in real time!

---

##  Hardware Deployment (ESP32-S3)

For physical hardware deployments with the **ESP32-S3** and **DS18B20** temperature sensors:

### 1. Wiring Diagram
* **DS18B20 VCC** ➔ 3.3V
* **DS18B20 GND** ➔ GND
* **DS18B20 DATA** ➔ GPIO 4 (with a 4.7kΩ pull-up resistor to 3.3V)

### 2. Configure Wi-Fi & Broker
Update your network configuration in [`crates/sentinel-firmware/src/config.rs`](crates/sentinel-firmware/src/config.rs):
```rust
pub const WIFI_SSID: &str = "YOUR_WIFI_SSID";
pub const WIFI_PASS: &str = "YOUR_WIFI_PASSWORD";
pub const BROKER_IP: [u8; 4] = [192, 168, 1, 100]; // Gateway IP
pub const BROKER_PORT: u16 = 1883;
```

### 3. Flash via USB or Wireless OTA
```bash
# Build & flash directly over USB:
cd crates/sentinel-firmware
cargo run --release

# OR push wireless OTA update over Wi-Fi (zero cables required):
./scripts/flash_ota.sh 192.168.1.150
```

---

##  Multi-Node Cluster Simulator & Chaos Testing

The [`sentinel-sim`](crates/sentinel-sim) crate lets you stress-test the gateway and dashboard under various industrial failure scenarios:

```bash
# 1. Normal operating freezer cycling (-20°C to -18°C)
cargo run -p sentinel-sim -- -n 4 -s normal

# 2. Thermal runaway (compressor failure -> heat surge -> triggers CRITICAL alert)
cargo run -p sentinel-sim -- -n 2 -s runaway

# 3. Freezer door left ajar (slow temperature creep + warning alert)
cargo run -p sentinel-sim -- -n 2 -s door-ajar

# 4. Ultra-low deep-freeze storage (-80°C bio-pharmaceutical storage)
cargo run -p sentinel-sim -- -n 3 -s deep-freeze

# 5. Full chaos mode (random hardware shorts, CRC faults, and fluctuations)
cargo run -p sentinel-sim -- -n 8 -s chaos
```

---

##  Edge Gateway & Alert Pipeline
Sentinel contains a multi-channel alert engine in [`crates/sentinel-gateway/src/alerts.rs`](crates/sentinel-gateway/src/alerts.rs). When a node breaches thermal or health thresholds, notifications dispatch concurrently:

```toml
# gateway.toml
bind_addr = "0.0.0.0:8080"
db_path = "sentinel.db"

[mqtt]
host = "localhost"
port = 1883
topic_filter = "sentinel/+/telemetry"

[alerts]
telegram_enabled = true
telegram_bot_token = "YOUR_TELEGRAM_BOT_TOKEN"
telegram_chat_id = "YOUR_TELEGRAM_CHAT_ID"

whatsapp_enabled = false
whatsapp_phone = "+15551234567"
whatsapp_api_key = "YOUR_API_KEY"
```

* **Telegram Integration:** Sends formatted HTML alerts with sanitized node IDs, temperatures, and timestamp links.
* **WhatsApp Integration:** Connects via CallMeBot REST API with automated URL encoding.
* **Non-Blocking Dispatch:** Calls are executed with `tokio::join!` with 10-second bounded timeouts so slow external networks never stall internal telemetry ingestion.

---

## Telemetry Protocol & Data Integrity

Telemetry packets use Dallas 1-Wire CRC-8 polynomial:
$$P(x) = x^8 + x^5 + x^4 + 1 \quad (\text{0x8C / 0x31})$$

The scratchpad payload verifies:
1. **CRC Checksum Validity:** Eliminates noise from long industrial sensor wiring runs.
2. **Power-On Reset Guard:** Rejects uninitialized sensor reset bytes (`0x0550`).
3. **Bus Ground-Fault Detection:** Rejects `0x000000000000000000` (short circuit) and `0xFFFFFFFFFFFFFFFFFF` (disconnected line).

---

##  Running Tests & Quality Assurance

Sentinel includes comprehensive unit and integration test coverage across all domain logic, CRC verification, and gateway configurations:

```bash
# Run all workspace unit tests
cargo test --workspace

# Test no_std core specifically with zero default features
cargo test -p sentinel-core --no-default-features
```

---

## License

This project is licensed under dual [MIT](LICENSE) OR [Apache-2.0](LICENSE) terms.
