//! The main authenticated screen: a build-list sidebar on the left and a
//! detail pane on the right.

use std::collections::HashMap;

use codemagic_core::models::Build;
use dioxus::prelude::*;

use super::build_detail::BuildDetail;
use super::icons::{GearIcon, PlusIcon, RefreshIcon};
use super::new_build::NewBuildModal;
use super::settings::SettingsModal;
use crate::state::AppState;

#[component]
pub fn BuildsScreen() -> Element {
    let state = use_context::<AppState>();

    // Currently selected build id, shared with the detail pane.
    let mut selected = use_signal(|| Option::<String>::None);
    let mut new_build_open = use_signal(|| false);
    let mut settings_open = use_signal(|| false);
    let mut refreshing = use_signal(|| false);

    let mut builds = use_resource(move || {
        let client = state.client();
        async move { client.get_builds(0, None, None).await }
    });

    // Clear the spinner once a (re)fetch resolves.
    use_effect(move || {
        if builds.read().is_some() {
            refreshing.set(false);
        }
    });

    // Auto-refresh the list on the configured interval.
    let refresh_secs = state.refresh_secs;
    use_future(move || async move {
        loop {
            let secs = *refresh_secs.read();
            tokio::time::sleep(std::time::Duration::from_secs(secs)).await;
            refreshing.set(true);
            builds.restart();
        }
    });

    let spinning = refreshing() || builds.read().is_none();

    rsx! {
        div { class: "layout",
            aside { class: "sidebar",
                header { class: "topbar", onmousedown: move |_| crate::start_drag(),
                    h1 { "Builds" }
                    div { class: "actions", onmousedown: move |e| e.stop_propagation(),
                        button {
                            class: "primary small",
                            onclick: move |_| new_build_open.set(true),
                            PlusIcon {}
                            span { "New build" }
                        }
                        button {
                            class: if spinning { "ghost icon-btn spinning" } else { "ghost icon-btn" },
                            title: "Refresh",
                            onclick: move |_| { refreshing.set(true); builds.restart(); },
                            RefreshIcon {}
                        }
                        button {
                            class: "ghost icon-btn",
                            title: "Settings",
                            onclick: move |_| settings_open.set(true),
                            GearIcon {}
                        }
                    }
                }
                div { class: "sidebar-list",
                    match &*builds.read() {
                        None => rsx! { p { class: "muted center", "Loading builds…" } },
                        Some(Err(e)) => rsx! {
                            div { class: "error-box",
                                p { "Couldn't load builds." }
                                p { class: "muted", "{e}" }
                                button { class: "ghost", onclick: move |_| builds.restart(), "Retry" }
                            }
                        },
                        Some(Ok(resp)) => {
                            let names: HashMap<&str, &str> = resp
                                .applications
                                .iter()
                                .map(|a| (a.id.as_str(), a.name.as_str()))
                                .collect();
                            if resp.builds.is_empty() {
                                rsx! { p { class: "muted center", "No builds yet." } }
                            } else {
                                rsx! {
                                    ul { class: "build-list",
                                        for build in resp.builds.iter() {
                                            BuildRow {
                                                key: "{build.id}",
                                                data: build.clone(),
                                                app_name: names.get(build.app_id.as_str()).map(|s| s.to_string()),
                                                selected,
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
            section { class: "detail-pane",
                BuildDetail { selected }
            }
        }

        if new_build_open() {
            NewBuildModal {
                open: new_build_open,
                on_started: move |build_id: String| {
                    new_build_open.set(false);
                    builds.restart();
                    selected.set(Some(build_id));
                },
            }
        }
        if settings_open() {
            SettingsModal { open: settings_open }
        }
    }
}

#[component]
fn BuildRow(data: Build, app_name: Option<String>, selected: Signal<Option<String>>) -> Element {
    let build = &data;
    let id = build.id.clone();
    let is_selected = selected.read().as_deref() == Some(id.as_str());

    let app = app_name.unwrap_or_else(|| "Unknown app".to_string());
    let number = build
        .display_build_number()
        .map(|n| format!("#{n}"))
        .unwrap_or_default();
    let when = build
        .display_time()
        .map(|t| t.format("%Y-%m-%d %H:%M").to_string())
        .unwrap_or_else(|| "-".to_string());

    rsx! {
        li {
            class: if is_selected { "build-row selected" } else { "build-row" },
            onclick: move |_| selected.set(Some(id.clone())),
            span { class: "status {status_class(&build.status)}", "{build.status}" }
            div { class: "build-main",
                div { class: "build-title",
                    span { class: "app-name", "{app}" }
                    span { class: "workflow", "{build.workflow_display()}" }
                }
                div { class: "build-sub muted",
                    span { "{build.git_ref()}" }
                    if !number.is_empty() {
                        span { "· {number}" }
                    }
                    span { "· {when}" }
                }
            }
        }
    }
}

/// Maps a Codemagic status string to a CSS modifier class.
pub fn status_class(status: &str) -> &'static str {
    match status {
        "finished" => "ok",
        "failed" | "timeout" => "fail",
        "canceled" => "cancel",
        "queued" | "preparing" | "building" | "testing" | "publishing" => "run",
        _ => "neutral",
    }
}
