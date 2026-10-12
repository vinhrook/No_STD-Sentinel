use leptos::prelude::*;
use crate::state::{Severity, TelemetryReading, TempUnit};

#[component]
pub fn TempGaugeCard(
    reading: Signal<TelemetryReading>,
    unit: Signal<TempUnit>,
) -> impl IntoView {
    let temp_display = move || {
        let r = reading.get();
        let u = unit.get();
        match u {
            TempUnit::Celsius => {
                let c = r.temp_c();
                (if c < 0.0 { "-" } else { "+" }, format!("{:.2}", c.abs()), "°C")
            }
            TempUnit::Fahrenheit => {
                let f = r.temp_f();
                (if f < 0.0 { "-" } else { "+" }, format!("{:.2}", f.abs()), "°F")
            }
        }
    };

    let severity_info = move || {
        let sev = reading.get().severity();
        match sev {
            Severity::Healthy => ("status-chip healthy", "HEALTHY", "var(--healthy-green)"),
            Severity::Warning => ("status-chip warning", "WARNING", "var(--warning-yellow)"),
            Severity::Critical => ("status-chip critical", "CRITICAL BREACH", "var(--critical-red)"),
        }
    };

    let gauge_stroke_offset = move || {
        let temp_c = reading.get().temp_c();
        let normalized = ((temp_c - (-20.0)) / 60.0).clamp(0.0, 1.0);
        let max_offset = 251.3;
        max_offset - (normalized * max_offset)
    };

    let scale_markers = move || {
        match unit.get() {
            TempUnit::Celsius => ("-20°C", "+10°C", "+40°C"),
            TempUnit::Fahrenheit => ("-4°F", "+50°F", "+104°F"),
        }
    };

    view! {
        <article class="dash-card hero-temp-card" id="card-temp-0">
            <header class="card-header">
                <div class="header-left">
                    <span class="card-badge temp-badge">"1-WIRE SENSOR #0"</span>
                    <h2 class="card-title">"INTERNAL CHAMBER"</h2>
                </div>
                <div class=move || severity_info().0>
                    {move || severity_info().1}
                </div>
            </header>

            <div class="primary-sensor-hero">
                <div class="temp-hero-container">
                    <div class="temp-display-cluster">
                        <span class="temp-sign">{move || temp_display().0}</span>
                        <span class="temp-value">{move || temp_display().1}</span>
                        <span class="temp-unit">{move || temp_display().2}</span>
                    </div>

                    <div class="temp-gauge-wrapper">
                        <svg class="temp-radial-svg" viewBox="0 0 200 120">
                            <path class="gauge-bg" d="M 20 110 A 80 80 0 0 1 180 110" fill="none" stroke-width="14" stroke-linecap="round"/>
                            <path
                                class="gauge-fill"
                                d="M 20 110 A 80 80 0 0 1 180 110"
                                fill="none"
                                stroke-width="14"
                                stroke-linecap="round"
                                stroke-dasharray="251.3"
                                stroke=move || severity_info().2
                                stroke-dashoffset=move || format!("{:.1}", gauge_stroke_offset())
                            />
                        </svg>
                        <div class="gauge-scale-markers">
                            <span class="marker min">{move || scale_markers().0}</span>
                            <span class="marker target">{move || scale_markers().1}</span>
                            <span class="marker max">{move || scale_markers().2}</span>
                        </div>
                    </div>
                </div>
            </div>

            <footer class="card-footer temp-footer">
                <div class="thresh-indicator">
                    <span class="thresh-label">"CRITICAL THRESHOLD:"</span>
                    <span class="thresh-val">"> -12.00°C"</span>
                </div>
                <div class="thresh-indicator">
                    <span class="thresh-label">"WARNING THRESHOLD:"</span>
                    <span class="thresh-val">"> -18.00°C"</span>
                </div>
            </footer>
        </article>
    }
}

#[component]
pub fn AmbientTempCard(
    reading: Signal<TelemetryReading>,
    unit: Signal<TempUnit>,
) -> impl IntoView {
    let ambient_display = move || {
        let r = reading.get();
        let u = unit.get();
        match u {
            TempUnit::Celsius => {
                r.ambient_c().map(|c| {
                    (if c < 0.0 { "-" } else { "+" }, format!("{:.2}", c.abs()), "°C")
                })
            }
            TempUnit::Fahrenheit => {
                r.ambient_f().map(|f| {
                    (if f < 0.0 { "-" } else { "+" }, format!("{:.2}", f.abs()), "°F")
                })
            }
        }
    };

    let delta_display = move || {
        let r = reading.get();
        let u = unit.get();
        match u {
            TempUnit::Celsius => {
                r.delta_c().map(|d| format!("{:+.2} °C", d))
            }
            TempUnit::Fahrenheit => {
                r.delta_f().map(|d| format!("{:+.2} °F", d))
            }
        }
    };

    let is_online = move || reading.get().ambient_temp.is_some();

    let gauge_stroke_offset = move || {
        if let Some(amb_c) = reading.get().ambient_c() {
            let normalized = ((amb_c - (-20.0)) / 60.0).clamp(0.0, 1.0);
            let max_offset = 251.3;
            max_offset - (normalized * max_offset)
        } else {
            251.3
        }
    };

    let scale_markers = move || {
        match unit.get() {
            TempUnit::Celsius => ("-20°C", "+10°C", "+40°C"),
            TempUnit::Fahrenheit => ("-4°F", "+50°F", "+104°F"),
        }
    };

    view! {
        <article class="dash-card hero-temp-card" id="card-temp-1">
            <header class="card-header">
                <div class="header-left">
                    <span class="card-badge ambient-badge">"1-WIRE SENSOR #1"</span>
                    <h2 class="card-title">"AMBIENT TEMPERATURE"</h2>
                </div>
                <div class=move || if is_online() { "status-chip healthy" } else { "status-chip offline" }>
                    {move || if is_online() { "ONLINE" } else { "OFFLINE" }}
                </div>
            </header>

            <div class="primary-sensor-hero">
                <div class="temp-hero-container">
                    <div class="temp-display-cluster">
                        {move || match ambient_display() {
                            Some((sign, val, u_str)) => view! {
                                <span class="temp-sign">{sign}</span>
                                <span class="temp-value ambient-value">{val}</span>
                                <span class="temp-unit">{u_str}</span>
                            }.into_any(),
                            None => view! {
                                <span class="temp-value offline-value">"OFFLINE"</span>
                            }.into_any(),
                        }}
                    </div>

                    <div class="temp-gauge-wrapper">
                        <svg class="temp-radial-svg" viewBox="0 0 200 120">
                            <path class="gauge-bg" d="M 20 110 A 80 80 0 0 1 180 110" fill="none" stroke-width="14" stroke-linecap="round"/>
                            <path
                                class="gauge-fill-ambient"
                                d="M 20 110 A 80 80 0 0 1 180 110"
                                fill="none"
                                stroke="var(--cyan-glow)"
                                stroke-width="14"
                                stroke-linecap="round"
                                stroke-dasharray="251.3"
                                stroke-dashoffset=move || format!("{:.1}", gauge_stroke_offset())
                            />
                        </svg>
                        <div class="gauge-scale-markers">
                            <span class="marker min">{move || scale_markers().0}</span>
                            <span class="marker target">{move || scale_markers().1}</span>
                            <span class="marker max">{move || scale_markers().2}</span>
                        </div>
                    </div>
                </div>
            </div>

            <footer class="card-footer temp-footer">
                <div class="thresh-indicator">
                    <span class="thresh-label">"GRADIENT (ΔT):"</span>
                    <span class="thresh-val delta-val">{move || delta_display().unwrap_or_else(|| "--".to_string())}</span>
                </div>
                <div class="thresh-indicator">
                    <span class="thresh-label">"BUS ADDRESS:"</span>
                    <span class="thresh-val">"ROM #1 (0x28...)"</span>
                </div>
            </footer>
        </article>
    }
}
