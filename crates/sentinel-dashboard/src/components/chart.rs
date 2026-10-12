use leptos::prelude::*;
use crate::state::ChartDataPoint;

#[component]
pub fn TimeseriesChart(
    data: Signal<Vec<ChartDataPoint>>,
    selected_minutes: RwSignal<usize>,
) -> impl IntoView {
    let width = 1000.0;
    let height = 240.0;
    let pad_top = 20.0;
    let pad_bottom = 25.0;
    let pad_left = 60.0;
    let pad_right = 30.0;

    let plot_w = width - pad_left - pad_right;
    let plot_h = height - pad_top - pad_bottom;

    let filtered_data = move || {
        let all = data.get();
        if all.is_empty() {
            return Vec::new();
        }
        let mins = selected_minutes.get();
        let duration_secs = (mins as u64) * 60;

        let latest_ts = all.last().map(|p| p.timestamp_secs).unwrap_or(0);
        if latest_ts > duration_secs {
            let min_ts = latest_ts - duration_secs;
            let filtered: Vec<_> = all.iter().filter(|p| p.timestamp_secs >= min_ts).cloned().collect();
            if filtered.len() >= 2 {
                return filtered;
            }
        }

        let max_count = match mins {
            5 => 150,
            10 => 300,
            15 => 450,
            other => other * 30,
        };
        if all.len() > max_count {
            all[all.len() - max_count..].to_vec()
        } else {
            all
        }
    };

    let y_bounds = move || {
        let pts = filtered_data();
        if pts.is_empty() {
            return (-30.0, 35.0);
        }
        let min_val = pts
            .iter()
            .map(|p| p.temp_c)
            .chain(pts.iter().filter_map(|p| p.ambient_c))
            .fold(f64::INFINITY, f64::min);
        let max_val = pts
            .iter()
            .map(|p| p.temp_c)
            .chain(pts.iter().filter_map(|p| p.ambient_c))
            .fold(f64::NEG_INFINITY, f64::max);
        let min_y = (-30.0_f64).min(min_val - 2.0);
        let max_y = (35.0_f64).max(max_val + 2.0);
        (min_y, max_y)
    };

    let get_y = move |val: f64| -> f64 {
        let (min_y, max_y) = y_bounds();
        let range = (max_y - min_y).max(1.0);
        pad_top + plot_h - ((val - min_y) / range) * plot_h
    };

    let line_path = move || {
        let pts = filtered_data();
        if pts.len() < 2 {
            return String::new();
        }
        let len = pts.len() as f64;
        let mut d = String::new();
        for (i, p) in pts.iter().enumerate() {
            let x = pad_left + (i as f64 / (len - 1.0)) * plot_w;
            let y = get_y(p.temp_c);
            if i == 0 {
                d.push_str(&format!("M {:.1} {:.1}", x, y));
            } else {
                d.push_str(&format!(" L {:.1} {:.1}", x, y));
            }
        }
        d
    };

    let ambient_line_path = move || {
        let pts = filtered_data();
        if pts.len() < 2 {
            return String::new();
        }
        let len = pts.len() as f64;
        let mut d = String::new();
        let mut first = true;
        for (i, p) in pts.iter().enumerate() {
            if let Some(amb) = p.ambient_c {
                let x = pad_left + (i as f64 / (len - 1.0)) * plot_w;
                let y = get_y(amb);
                if first {
                    d.push_str(&format!("M {:.1} {:.1}", x, y));
                    first = false;
                } else {
                    d.push_str(&format!(" L {:.1} {:.1}", x, y));
                }
            }
        }
        d
    };

    let area_path = move || {
        let line = line_path();
        if line.is_empty() {
            return String::new();
        }
        let last_x = pad_left + plot_w;
        let first_x = pad_left;
        let bottom_y = height - pad_bottom;
        format!("{} L {:.1} {:.1} L {:.1} {:.1} Z", line, last_x, bottom_y, first_x, bottom_y)
    };

    let last_point = move || {
        let pts = filtered_data();
        if pts.is_empty() {
            None
        } else {
            let last_idx = pts.len() - 1;
            let x = pad_left + (last_idx as f64 / ((pts.len().max(2) - 1) as f64)) * plot_w;
            let y = get_y(pts[last_idx].temp_c);
            Some((x, y, pts[last_idx].temp_c))
        }
    };

    let last_ambient_point = move || {
        let pts = filtered_data();
        if pts.is_empty() {
            None
        } else {
            let last_idx = pts.len() - 1;
            pts[last_idx].ambient_c.map(|amb| {
                let x = pad_left + (last_idx as f64 / ((pts.len().max(2) - 1) as f64)) * plot_w;
                let y = get_y(amb);
                (x, y, amb)
            })
        }
    };

    let warn_y = move || get_y(-18.0);
    let crit_y = move || get_y(-12.0);

    view! {
        <article class="dash-card chart-card">
            <header class="card-header chart-header">
                <div class="header-left">
                    <span class="card-badge chart-badge">"LEPTOS REACTIVE SVG"</span>
                    <h2 class="card-title">"THERMAL DRIFT & DUAL SENSOR HISTORY"</h2>
                </div>
                <div class="chart-controls">
                    <div class="time-range-pills">
                        <button
                            type="button"
                            class=move || if selected_minutes.get() == 5 { "range-btn active" } else { "range-btn" }
                            on:click=move |_| selected_minutes.set(5)
                        >"5 Min"</button>
                        <button
                            type="button"
                            class=move || if selected_minutes.get() == 10 { "range-btn active" } else { "range-btn" }
                            on:click=move |_| selected_minutes.set(10)
                        >"10 Min"</button>
                        <button
                            type="button"
                            class=move || if selected_minutes.get() == 15 { "range-btn active" } else { "range-btn" }
                            on:click=move |_| selected_minutes.set(15)
                        >"15 Min"</button>
                    </div>
                </div>
            </header>

            <div class="canvas-wrapper">
                <svg viewBox=format!("0 0 {} {}", width, height) class="timeseries-svg" preserveAspectRatio="none">
                    <defs>
                        <linearGradient id="area-grad" x1="0" y1="0" x2="0" y2="1">
                            <stop offset="0%" stop-color="var(--cyan-glow)" stop-opacity="0.30"/>
                            <stop offset="100%" stop-color="var(--cyan-glow)" stop-opacity="0.0"/>
                        </linearGradient>
                    </defs>

                    <line x1=pad_left y1=warn_y x2=width - pad_right y2=warn_y stroke="rgba(245, 158, 11, 0.45)" stroke-dasharray="6,4" stroke-width="1.5"/>
                    <line x1=pad_left y1=crit_y x2=width - pad_right y2=crit_y stroke="rgba(239, 68, 68, 0.55)" stroke-dasharray="4,4" stroke-width="1.5"/>

                    <path d=area_path fill="url(#area-grad)"/>

                    <path d=line_path fill="none" stroke="var(--cyan-glow)" stroke-width="2.5" stroke-linecap="round"/>

                    <path d=ambient_line_path fill="none" stroke="#f59e0b" stroke-width="2.2" stroke-dasharray="5 3" stroke-linecap="round"/>

                    {move || last_point().map(|(x, y, _val)| view! {
                        <circle cx=x cy=y r="8" fill="none" stroke="rgba(0, 230, 246, 0.4)" stroke-width="2"/>
                        <circle cx=x cy=y r="4.5" fill="var(--cyan-glow)"/>
                    })}

                    {move || last_ambient_point().map(|(x, y, _val)| view! {
                        <circle cx=x cy=y r="7" fill="none" stroke="rgba(245, 158, 11, 0.4)" stroke-width="2"/>
                        <circle cx=x cy=y r="4" fill="#f59e0b"/>
                    })}
                </svg>
            </div>

            <div class="chart-legend">
                <span class="legend-item"><span class="legend-dot temp"></span>" Sensor #0 Core (°C)"</span>
                <span class="legend-item"><span class="legend-dot ambient"></span>" Sensor #1 Ambient (°C)"</span>
                <span class="legend-item"><span class="legend-dot warn"></span>" Warn Limit (-18°C)"</span>
                <span class="legend-item"><span class="legend-dot crit"></span>" Crit Limit (-12°C)"</span>
            </div>
        </article>
    }
}
