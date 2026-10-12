use leptos::prelude::*;
use crate::state::{Severity, TelemetryReading};

#[component]
pub fn AlarmBanner(
    reading: Signal<TelemetryReading>,
    alarm_timestamp: Signal<String>,
) -> impl IntoView {
    let is_critical = move || reading.get().severity() == Severity::Critical;

    let alarm_cause = move || {
        let r = reading.get();
        if r.temp_c() > -12.0 {
            (
                "CRITICAL THERMAL BREACH DETECTED",
                format!("Freezer temp reached {:.2}°C, exceeding critical safety limit!", r.temp_c()),
                "TEMPERATURE_BREACH",
            )
        } else if r.door_open_secs > 180 {
            (
                "PROLONGED DOOR OPEN ALARM",
                format!("Freezer door unsealed for {}s. Cryo envelope compromised.", r.door_open_secs),
                "DOOR_PROLONGED_OPEN",
            )
        } else {
            (
                "COMPOSITE HEALTH FAILURE",
                format!("Overall system health plummeted to {}/1000.", r.health_score),
                "HEALTH_SCORE_FAILURE",
            )
        }
    };

    view! {
        <aside class=move || if is_critical() { "alarm-banner" } else { "alarm-banner hidden" } role="alert">
            <div class="alarm-content">
                <div class="alarm-beacon">
                    <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5" class="alarm-icon">
                        <path d="M10.29 3.86L1.82 18a2 2 0 0 0 1.71 3h16.94a2 2 0 0 0 1.71-3L13.71 3.86a2 2 0 0 0-3.42 0z"/>
                        <line x1="12" y1="9" x2="12" y2="13"/>
                        <line x1="12" y1="17" x2="12.01" y2="17"/>
                    </svg>
                </div>
                <div class="alarm-text">
                    <h2 class="alarm-title">{move || alarm_cause().0}</h2>
                    <p class="alarm-desc">{move || alarm_cause().1}</p>
                </div>
            </div>
            <div class="alarm-meta">
                <span class="alarm-tag">{move || alarm_cause().2}</span>
                <span class="alarm-time">{move || alarm_timestamp.get()}</span>
            </div>
        </aside>
    }
}
