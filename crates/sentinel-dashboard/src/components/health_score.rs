use leptos::prelude::*;
use crate::state::TelemetryReading;

#[component]
pub fn HealthScoreCard(reading: Signal<TelemetryReading>) -> impl IntoView {
    let score = move || reading.get().health_score;

    let ring_offset = move || {
        let s = score() as f64;
        let circumference = 427.2;
        circumference - ((s / 1000.0) * circumference)
    };

    let grade_info = move || {
        let s = score();
        if s < 500 {
            ("var(--critical-red)", "CRITICAL DEGRADATION", "Immediate maintenance required! Temperature or seal compromised.")
        } else if s < 750 {
            ("var(--warning-yellow)", "ATTENTION REQUIRED", "Moderate deviation. Check door seal and heat load.")
        } else {
            ("var(--healthy-green)", "OPTIMAL", "Thermal integrity verified. Operating within target envelope.")
        }
    };

    let temp_penalty = move || {
        let temp_c = reading.get().temp_c();
        if temp_c > -18.0 {
            format!("-{:.0} pts", (temp_c - -18.0) * 50.0)
        } else {
            "0 pts".to_string()
        }
    };

    let door_penalty = move || {
        let secs = reading.get().door_open_secs;
        let pen = (secs * 10).min(300);
        if pen > 0 {
            format!("-{} pts", pen)
        } else {
            "0 pts".to_string()
        }
    };

    let wifi_penalty = move || {
        if reading.get().rssi_dbm < -75 {
            "-20 pts".to_string()
        } else {
            "0 pts".to_string()
        }
    };

    view! {
        <article class="dash-card health-score-card" id="card-health">
            <header class="card-header">
                <div class="header-left">
                    <span class="card-badge health-badge">"MATH MODEL"</span>
                    <h2 class="card-title">"FREEZER HEALTH SCORE"</h2>
                </div>
                <span class="health-grade-chip" style=move || format!("color: {}; border-color: {}", grade_info().0, grade_info().0)>
                    {move || grade_info().1}
                </span>
            </header>

            <div class="health-cluster">
                <div class="radial-score-svg-wrapper">
                    <svg viewBox="0 0 160 160" class="score-radial-svg">
                        <circle class="score-bg" cx="80" cy="80" r="68" fill="none" stroke-width="12"/>
                        <circle
                            class="score-fill"
                            cx="80"
                            cy="80"
                            r="68"
                            fill="none"
                            stroke-width="12"
                            stroke-linecap="round"
                            stroke-dasharray="427.2"
                            stroke=move || grade_info().0
                            stroke-dashoffset=move || format!("{:.1}", ring_offset())
                        />
                    </svg>
                    <div class="score-numeric-center">
                        <span class="score-large">{move || score()}</span>
                        <span class="score-denom">"/ 1000"</span>
                    </div>
                </div>

                <div class="health-penalties">
                    <div class="penalty-row">
                        <div class="penalty-info">
                            <span class="penalty-dot green"></span>
                            <span class="penalty-name">"Thermal Stability"</span>
                        </div>
                        <span class="penalty-value">{temp_penalty}</span>
                    </div>
                    <div class="penalty-row">
                        <div class="penalty-info">
                            <span class="penalty-dot yellow"></span>
                            <span class="penalty-name">"Door Seal Penalty"</span>
                        </div>
                        <span class="penalty-value">{door_penalty}</span>
                    </div>
                    <div class="penalty-row">
                        <div class="penalty-info">
                            <span class="penalty-dot blue"></span>
                            <span class="penalty-name">"Link RSSI Penalty"</span>
                        </div>
                        <span class="penalty-value">{wifi_penalty}</span>
                    </div>
                </div>
            </div>

            <footer class="card-footer">
                <span class="status-hint">{move || grade_info().2}</span>
            </footer>
        </article>
    }
}
