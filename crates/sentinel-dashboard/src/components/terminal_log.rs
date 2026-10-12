use leptos::prelude::*;
use crate::state::LogEntry;

#[component]
pub fn TerminalLogCard(
    logs: Signal<Vec<LogEntry>>,
    on_clear: Callback<()>,
) -> impl IntoView {
    view! {
        <article class="dash-card stream-log-card">
            <header class="card-header">
                <div class="header-left">
                    <span class="card-badge stream-badge">"EVENT BUS"</span>
                    <h2 class="card-title">"LIVE WIRE INGESTION FEED"</h2>
                </div>
                <div class="stream-actions">
                    <button type="button" class="btn-ghost" on:click=move |_| on_clear.run(())>
                        "Clear"
                    </button>
                    <span class="frame-counter">{move || format!("{} frames", logs.get().len())}</span>
                </div>
            </header>

            <div class="stream-terminal" tabindex="0">
                <For
                    each=move || logs.get()
                    key=|entry| format!("{}-{}-{}", entry.time, entry.tag, entry.message)
                    children=move |entry| {
                        let class_name = if entry.is_alert { "log-entry alert" } else { "log-entry rx" };
                        view! {
                            <div class=class_name>
                                <span class="log-time">{format!("[{}]", entry.time)}</span>
                                <span class="log-tag">{format!("[{}]", entry.tag)}</span>
                                <span class="log-msg">{entry.message}</span>
                            </div>
                        }
                    }
                />
            </div>
        </article>
    }
}
