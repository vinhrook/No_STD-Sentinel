use leptos::prelude::*;
use crate::state::TelemetryReading;

#[component]
pub fn DoorCard(reading: Signal<TelemetryReading>) -> impl IntoView {
    let is_open = move || reading.get().door_open;
    let duration = move || reading.get().door_open_secs;

    let timer_pct = move || {
        let secs = duration() as f64;
        (secs / 300.0 * 100.0).clamp(0.0, 100.0)
    };

    view! {
        <article class="dash-card door-card" id="card-door">
            <header class="card-header">
                <div class="header-left">
                    <span class="card-badge door-badge">"REED SENSOR"</span>
                    <h2 class="card-title">"DOOR INTERLOCK"</h2>
                </div>
                <span class=move || if is_open() { "door-status-pill open" } else { "door-status-pill closed" }>
                    {move || if is_open() { "UNSEALED" } else { "SEALED" }}
                </span>
            </header>

            <div class="door-visual-container">
                <div class=move || if is_open() { "door-icon-box open" } else { "door-icon-box" }>
                    <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" class="door-svg">
                        <path d="M4 3h16a1 1 0 0 1 1 1v17H3V4a1 1 0 0 1 1-1z"/>
                        <circle cx="8" cy="12" r="1.5"/>
                        <path d="M4 21h16"/>
                    </svg>
                </div>
                <div class="door-metrics">
                    <div class="door-metric-group">
                        <span class="door-metric-label">"STATUS"</span>
                        <span class="door-metric-val">{move || if is_open() { "OPEN" } else { "CLOSED" }}</span>
                    </div>
                    <div class="door-metric-group">
                        <span class="door-metric-label">"DURATION OPEN"</span>
                        <span class="door-metric-val">{move || format!("{}s", duration())}</span>
                    </div>
                </div>
            </div>

            <footer class="card-footer">
                <div class="door-progress-track">
                    <div class="door-progress-fill" style=move || format!("width: {:.1}%", timer_pct())></div>
                </div>
                <div class="door-timer-labels">
                    <span>"Safe (0s)"</span>
                    <span>"Warn (120s)"</span>
                    <span>"Crit (300s)"</span>
                </div>
            </footer>
        </article>
    }
}
