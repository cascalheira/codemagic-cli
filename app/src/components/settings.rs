//! Settings modal: API token, auto-refresh interval, and sign out.

use dioxus::prelude::*;
use gantry_core::ApiClient;

use crate::state::{AppState, MIN_REFRESH_SECS};

/// Result of a manual "Check now".
#[derive(Clone, PartialEq)]
enum UpdateStatus {
    Idle,
    Checking,
    UpToDate,
    Available(gantry_core::update::Update),
    Error(String),
}

#[derive(Clone, PartialEq)]
enum TokenStatus {
    Idle,
    Checking,
    Saved,
    Error(String),
}

#[component]
pub fn SettingsModal(open: Signal<bool>) -> Element {
    let mut state = use_context::<AppState>();

    let mut token_input = use_signal(|| state.token.read().clone());
    let mut token_status = use_signal(|| TokenStatus::Idle);
    let mut refresh_input = use_signal(|| state.refresh_secs.read().to_string());
    let mut update_status = use_signal(|| UpdateStatus::Idle);

    let save_token = move |_| {
        let candidate = token_input.read().trim().to_string();
        if candidate.is_empty() {
            token_status.set(TokenStatus::Error("Token can't be empty.".into()));
            return;
        }
        token_status.set(TokenStatus::Checking);
        spawn(async move {
            let client = ApiClient::new(candidate.clone());
            match client.validate_token().await {
                Ok(true) => {
                    state.save_token(candidate);
                    token_status.set(TokenStatus::Saved);
                }
                Ok(false) => token_status.set(TokenStatus::Error("Token was rejected.".into())),
                Err(e) => token_status.set(TokenStatus::Error(format!("Network error: {e}"))),
            }
        });
    };

    let save_refresh = move |_| {
        let parsed = refresh_input.read().trim().parse::<u64>();
        if let Ok(secs) = parsed {
            let secs = secs.max(MIN_REFRESH_SECS);
            state.set_refresh_secs(secs);
            refresh_input.set(secs.to_string());
        }
    };

    let check_now = move |_| {
        update_status.set(UpdateStatus::Checking);
        spawn(async move {
            match gantry_core::update::check(env!("CARGO_PKG_VERSION")).await {
                Ok(Some(found)) => update_status.set(UpdateStatus::Available(found)),
                Ok(None) => update_status.set(UpdateStatus::UpToDate),
                Err(e) => update_status.set(UpdateStatus::Error(e.to_string())),
            }
        });
    };

    let checking = matches!(*token_status.read(), TokenStatus::Checking);

    // Owned snapshots: a read guard held live inside `rsx!` stays borrowed
    // across the reactive flush, which deadlocks if a spawned task writes the
    // same signal.
    let token_state = token_status.read().clone();
    let update_state = update_status.read().clone();
    let auto_check = *state.check_updates.read();

    rsx! {
        div { class: "modal-overlay", onclick: move |_| open.set(false),
            div { class: "modal form-modal", onclick: move |e| e.stop_propagation(),
                div { class: "modal-head",
                    h3 { "Settings" }
                    button { class: "ghost small", onclick: move |_| open.set(false), "Close" }
                }
                div { class: "form-body",
                    // ── API token ──
                    label { "API token" }
                    input {
                        r#type: "password",
                        value: "{token_input}",
                        disabled: checking,
                        oninput: move |e| { token_input.set(e.value()); token_status.set(TokenStatus::Idle); },
                    }
                    div { class: "row-actions",
                        button { class: "primary small", disabled: checking, onclick: save_token,
                            if checking { "Validating…" } else { "Update token" }
                        }
                        match &token_state {
                            TokenStatus::Saved => rsx! { span { class: "ok-text", "Saved ✓" } },
                            TokenStatus::Error(m) => rsx! { span { class: "error", "{m}" } },
                            _ => rsx! {},
                        }
                    }

                    // ── Auto-refresh ──
                    label { "Auto-refresh interval (seconds)" }
                    div { class: "row-actions",
                        input {
                            r#type: "number",
                            min: "{MIN_REFRESH_SECS}",
                            value: "{refresh_input}",
                            oninput: move |e| refresh_input.set(e.value()),
                        }
                        button { class: "ghost small", onclick: save_refresh, "Save" }
                    }
                    p { class: "muted hint", "The builds list refreshes automatically at this interval (minimum {MIN_REFRESH_SECS}s)." }

                    // ── Updates ──
                    label { "Updates" }
                    label { class: "check-row",
                        input {
                            r#type: "checkbox",
                            checked: auto_check,
                            onchange: move |e| state.set_check_updates(e.checked()),
                        }
                        span { "Check for a new version on startup" }
                    }
                    div { class: "row-actions",
                        button {
                            class: "ghost small",
                            disabled: update_state == UpdateStatus::Checking,
                            onclick: check_now,
                            if update_state == UpdateStatus::Checking { "Checking…" } else { "Check now" }
                        }
                        match &update_state {
                            UpdateStatus::UpToDate => rsx! {
                                span { class: "ok-text", "You're up to date (", {env!("CARGO_PKG_VERSION")}, ")" }
                            },
                            UpdateStatus::Available(found) => {
                                let url = found.url.clone();
                                rsx! {
                                    button {
                                        class: "primary small",
                                        onclick: move |_| gantry_core::web::open_in_browser(&url),
                                        "Get {found.version}"
                                    }
                                }
                            }
                            UpdateStatus::Error(m) => rsx! { span { class: "error", "{m}" } },
                            UpdateStatus::Idle | UpdateStatus::Checking => rsx! {},
                        }
                    }
                }
                div { class: "modal-foot",
                    button {
                        class: "danger",
                        onclick: move |_| { state.sign_out(); open.set(false); },
                        "Sign out"
                    }
                    button { class: "ghost", onclick: move |_| open.set(false), "Done" }
                }
            }
        }
    }
}
