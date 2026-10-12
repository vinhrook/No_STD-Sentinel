use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::time::sleep;

use crate::cli::CliArgs;
use crate::node::VirtualNode;
use crate::scenario::ScenarioKind;
use crate::transport::MqttTransport;

static TOTAL_FRAMES_PUBLISHED: AtomicU64 = AtomicU64::new(0);

pub async fn run_cluster(args: CliArgs) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    println!("\x1b[1;36m====================================================================\x1b[0m");
    println!("\x1b[1;36m  SENTINEL VIRTUAL HARDWARE CLUSTER (ESP32-S3 / DUAL DS18B20)\x1b[0m");
    println!("\x1b[1;36m====================================================================\x1b[0m");
    println!("  Nodes Simulated : \x1b[1;32m{}\x1b[0m", args.nodes);
    println!("  Active Scenario : \x1b[1;33m{:?}\x1b[0m", args.scenario);
    println!("  Tick Interval   : \x1b[1;37m{} ms\x1b[0m", args.interval_ms);
    println!("  MQTT Target     : \x1b[1;37m{}:{}\x1b[0m", args.mqtt_host, args.mqtt_port);
    println!("\x1b[1;36m--------------------------------------------------------------------\x1b[0m");

    let (transport, _eventloop_handle) = MqttTransport::connect(
        &args.mqtt_host,
        args.mqtt_port,
        &format!("{}-{}", args.client_id_prefix, std::process::id()),
    );
    let transport = Arc::new(transport);

    let mut node_handles = Vec::new();
    let dt_secs = (args.interval_ms as f64) / 1000.0;

    for i in 0..args.nodes {
        let node_id = 0xA101_0000 + (i as u32 + 1);
        let transport = Arc::clone(&transport);
        let interval_ms = args.interval_ms;

        let scenario_kind = if i == 0 {
            args.scenario
        } else if i == 1 && args.scenario == ScenarioKind::ThermalRunaway {
            ScenarioKind::Normal
        } else {
            args.scenario
        };

        let handle = tokio::spawn(async move {
            let mut node = VirtualNode::new(node_id, scenario_kind);
            println!(
                "  \x1b[1;32m[+]\x1b[0m Initialized virtual node \x1b[1;33m0x{:08X}\x1b[0m (Scenario: {:?})",
                node_id, scenario_kind
            );

            let mut ticker = tokio::time::interval(Duration::from_millis(interval_ms));
            ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

            loop {
                ticker.tick().await;
                let payload = node.tick(dt_secs);

                match transport.publish_telemetry(&payload).await {
                    Ok(()) => {
                        let total = TOTAL_FRAMES_PUBLISHED.fetch_add(1, Ordering::Relaxed) + 1;
                        let temp_f = (payload.tc as f64) / 100.0;
                        let amb_f = payload.ac.map(|a| (a as f64) / 100.0).unwrap_or(0.0);
                        let door_str = if payload.door_open {
                            "\x1b[1;31mOPEN\x1b[0m"
                        } else {
                            "\x1b[1;32mCLOSED\x1b[0m"
                        };

                        let alert_indicator = if payload.tc >= 3500 {
                            " \x1b[1;41;37m[CRITICAL +35°C BREACH]\x1b[0m"
                        } else if payload.tc > -1000 {
                            " \x1b[1;33m[WARM WARNING]\x1b[0m"
                        } else {
                            ""
                        };

                        if total % (args.nodes as u64).max(1) == 0 || payload.door_open || payload.tc >= 3500 {
                            println!(
                                "  \x1b[90m[{:04}s]\x1b[0m Node \x1b[1;33m0x{:08X}\x1b[0m | Core: \x1b[1;37m{:>6.2}°C\x1b[0m | Amb: \x1b[90m{:>5.2}°C\x1b[0m | Door: {} | Health: \x1b[1;36m{:>4}/1000\x1b[0m{}",
                                payload.ts, payload.d, temp_f, amb_f, door_str, payload.hs, alert_indicator
                            );
                        }
                    }
                    Err(e) => {
                        tracing::warn!(node_id = %format!("0x{:08X}", node_id), error = %e, "Frame drop: broker unreachable");
                    }
                }
            }
        });

        node_handles.push(handle);
        sleep(Duration::from_millis(150)).await;
    }

    println!("\x1b[1;32m[✓] All {} virtual nodes live and streaming.\x1b[0m Press Ctrl+C to terminate.", args.nodes);

    tokio::signal::ctrl_c().await?;
    println!("\n\x1b[1;33m[!] Shutting down simulation cluster...\x1b[0m");

    for h in node_handles {
        h.abort();
    }

    let final_count = TOTAL_FRAMES_PUBLISHED.load(Ordering::Relaxed);
    println!("\x1b[1;32m[✓] Cluster cleanly stopped. Total frames published: {}\x1b[0m\n", final_count);

    Ok(())
}
