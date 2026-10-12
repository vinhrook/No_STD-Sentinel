use leptos::prelude::*;
use crate::components::*;
use crate::state::{ChartDataPoint, LogEntry, TelemetryReading, TempUnit};
use wasm_bindgen::prelude::*;
use web_sys::{MessageEvent, WebSocket};

#[component]
pub fn App() -> impl IntoView {
    let reading = RwSignal::new(TelemetryReading::default());
    let unit = RwSignal::new(TempUnit::Celsius);
    let logs = RwSignal::new(vec![LogEntry {
        time: "00:00:00".to_string(),
        tag: "SYSTEM".to_string(),
        message: "Sentinel Leptos Reactive Dashboard initialized. Connecting to WebSocket...".to_string(),
        is_alert: false,
    }]);
    let chart_data = RwSignal::new(Vec::<ChartDataPoint>::new());
    let chart_minutes = RwSignal::new(5_usize);
    let is_connected = RwSignal::new(false);
    let alarm_timestamp = RwSignal::new("00:00:00".to_string());

    Effect::new(move |_| {
        let host = web_sys::window()
            .and_then(|w| w.location().hostname().ok())
            .filter(|h| !h.is_empty())
            .unwrap_or_else(|| "localhost".to_string());
        let ws_url = format!("ws://{host}:8080/ws/live");
        if let Ok(ws) = WebSocket::new(&ws_url) {
            let is_conn = is_connected;
            let log_sig = logs;
            let onopen = Closure::<dyn FnMut()>::wrap(Box::new(move || {
                is_conn.set(true);
                let mut current = log_sig.get();
                current.insert(0, LogEntry {
                    time: "LIVE".to_string(),
                    tag: "SYSTEM".to_string(),
                    message: "Connected to Sentinel Gateway live WebSocket broadcast.".to_string(),
                    is_alert: false,
                });
                log_sig.set(current);
            }));
            ws.set_onopen(Some(onopen.as_ref().unchecked_ref()));
            onopen.forget();

            let read_sig = reading;
            let chart_sig = chart_data;
            let log_sig = logs;
            let alarm_ts = alarm_timestamp;

            let onmessage = Closure::<dyn FnMut(MessageEvent)>::wrap(Box::new(move |e: MessageEvent| {
                if let Some(txt) = e.data().as_string() {
                    if let Ok(mut incoming) = serde_json::from_str::<TelemetryReading>(&txt) {
                        if incoming.ambient_temp.is_none() {
                            incoming.ambient_temp = read_sig.get().ambient_temp;
                        }
                        let temp_c = incoming.temp_c();
                        let score = incoming.health_score;
                        let is_alert = temp_c > -12.0;

                        read_sig.set(incoming.clone());

                        let ambient_c = incoming.ambient_c();
                        let mut pts = chart_sig.get();
                        pts.push(ChartDataPoint {
                            time: "T".to_string(),
                            timestamp_secs: incoming.timestamp_secs,
                            temp_c,
                            ambient_c,
                            score,
                        });
                        while pts.len() > 1000 {
                            pts.remove(0);
                        }
                        chart_sig.set(pts);

                        let amb_str = match ambient_c {
                            Some(a) => format!(" | Amb: {:.2}°C | ΔT: {:+.2}°C", a, a - temp_c),
                            None => String::new(),
                        };
                        let mut entries = log_sig.get();
                        entries.insert(0, LogEntry {
                            time: "NOW".to_string(),
                            tag: format!("RX:0x{:08X}", incoming.device_id),
                            message: format!(
                                "Core: {:.2}°C{} | Score: {}/1000 | Door: {} | RSSI: {}dBm",
                                temp_c, amb_str, score, if incoming.door_open { "OPEN" } else { "CLOSED" }, incoming.rssi_dbm
                            ),
                            is_alert,
                        });
                        while entries.len() > 60 {
                            entries.pop();
                        }
                        log_sig.set(entries);

                        if is_alert {
                            alarm_ts.set("ACTIVE".to_string());
                        }
                    }
                }
            }));
            ws.set_onmessage(Some(onmessage.as_ref().unchecked_ref()));
            onmessage.forget();

            let is_conn = is_connected;
            let onclose = Closure::<dyn FnMut()>::wrap(Box::new(move || {
                is_conn.set(false);
            }));
            ws.set_onclose(Some(onclose.as_ref().unchecked_ref()));
            onclose.forget();
        }
    });


    view! {
        <div class="app-layout">
            <header class="top-nav">
                <div class=move || if is_connected.get() { "status-pill online" } else { "status-pill offline" }>
                    <span class="pulse-dot"></span>
                    <span>{move || if is_connected.get() { "LIVE FEED ONLINE" } else { "DISCONNECTED" }}</span>
                </div>

                <div class="unit-toggle">
                    <button
                        type="button"
                        class=move || if unit.get() == TempUnit::Celsius { "unit-btn active" } else { "unit-btn" }
                        on:click=move |_| unit.set(TempUnit::Celsius)
                    >"°C"</button>
                    <button
                        type="button"
                        class=move || if unit.get() == TempUnit::Fahrenheit { "unit-btn active" } else { "unit-btn" }
                        on:click=move |_| unit.set(TempUnit::Fahrenheit)
                    >"°F"</button>
                </div>
            </header>

            <AlarmBanner reading=reading.into() alarm_timestamp=alarm_timestamp.into() />



            <main class="dashboard-grid">
                <TempGaugeCard reading=reading.into() unit=unit.into() />
                <AmbientTempCard reading=reading.into() unit=unit.into() />
                <HealthScoreCard reading=reading.into() />
                <DoorCard reading=reading.into() />
                <HardwareCard reading=reading.into() />
                <TimeseriesChart data=chart_data.into() selected_minutes=chart_minutes />
                <TerminalLogCard logs=logs.into() on_clear=Callback::new(move |_| logs.set(Vec::new())) />
            </main>

            <footer class="app-footer">
                <p>"SENTINEL Cryogenic Architecture // 100% Type-Safe Pure Rust from Embedded Firmware to Leptos WebAssembly"</p>
            </footer>
        </div>
    }
}
