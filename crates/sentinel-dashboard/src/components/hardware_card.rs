use leptos::prelude::*;
use crate::state::TelemetryReading;

#[component]
pub fn HardwareCard(reading: Signal<TelemetryReading>) -> impl IntoView {
    let hex_node_id = move || reading.get().hex_id();

    let firmware_str = move || {
        let fv = reading.get().firmware_version;
        let major = fv / 10000;
        let minor = (fv % 10000) / 100;
        let patch = fv % 100;
        format!("v{}.{}.{}", major, minor, patch)
    };

    let rssi_str = move || format!("{} dBm", reading.get().rssi_dbm);

    let uptime_str = move || {
        let secs = reading.get().timestamp_secs;
        let mins = secs / 60;
        let rem_sec = secs % 60;
        format!("{:02}m {:02}s", mins, rem_sec)
    };

    let rssi = move || reading.get().rssi_dbm;

    view! {
        <article class="dash-card hardware-card" id="card-hw">
            <header class="card-header">
                <div class="header-left">
                    <span class="card-badge hw-badge">"ESP32-S3 SILICON"</span>
                    <h2 class="card-title">"NODE TELEMETRY"</h2>
                </div>
                <span class="hw-chip">"ONLINE"</span>
            </header>

            <div class="hw-metrics-grid">
                <div class="hw-box">
                    <span class="hw-label">"MAC / NODE ID"</span>
                    <span class="hw-val hex">{hex_node_id}</span>
                </div>
                <div class="hw-box">
                    <span class="hw-label">"FIRMWARE"</span>
                    <span class="hw-val semver">{firmware_str}</span>
                </div>
                <div class="hw-box">
                    <span class="hw-label">"WIFI RSSI"</span>
                    <div class="rssi-combo">
                        <span class="hw-val">{rssi_str}</span>
                        <div class="rssi-bars">
                            <span class={move || if rssi() > -90 { "bar b1 active" } else { "bar b1" }}></span>
                            <span class={move || if rssi() > -80 { "bar b2 active" } else { "bar b2" }}></span>
                            <span class={move || if rssi() > -70 { "bar b3 active" } else { "bar b3" }}></span>
                            <span class={move || if rssi() > -60 { "bar b4 active" } else { "bar b4" }}></span>
                        </div>
                    </div>
                </div>
                <div class="hw-box">
                    <span class="hw-label">"NODE UPTIME"</span>
                    <span class="hw-val">{uptime_str}</span>
                </div>
            </div>

            <footer class="card-footer hw-footer">
                <span class="db-indicator">"SQLite WAL: Synchronized"</span>
                <span class="db-indicator">"Transport: rumqttc v5"</span>
            </footer>
        </article>
    }
}
