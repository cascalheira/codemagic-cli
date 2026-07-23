//! The main authenticated screen: a build-list sidebar on the left and a
//! detail pane on the right.

use std::collections::HashMap;

use chrono::{DateTime, Local, Utc};
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
                                // Group consecutive builds by calendar day.
                                let mut groups: Vec<(String, Vec<Build>)> = Vec::new();
                                for b in &resp.builds {
                                    let label = b.display_time().map(day_label).unwrap_or_else(|| "Earlier".into());
                                    match groups.last_mut() {
                                        Some((l, v)) if *l == label => v.push(b.clone()),
                                        _ => groups.push((label, vec![b.clone()])),
                                    }
                                }
                                rsx! {
                                    for (label, gbuilds) in groups.iter() {
                                        div { key: "{label}", class: "day-group",
                                            div { class: "day-header", "{label}" }
                                            ul { class: "build-list",
                                                for build in gbuilds.iter() {
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
                }
                div { class: "sidebar-foot",
                    button {
                        class: "ghost icon-btn foot-btn",
                        title: "New build",
                        onclick: move |_| new_build_open.set(true),
                        PlusIcon {}
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
    let number = build.display_build_number().map(|n| format!(" · #{n}")).unwrap_or_default();
    let when = build.display_time().map(relative_time).unwrap_or_default();

    rsx! {
        li {
            class: if is_selected { "build-row selected" } else { "build-row" },
            onclick: move |_| selected.set(Some(id.clone())),
            span { class: "status-dot {status_class(&build.status)}" }
            div { class: "build-main",
                div { class: "bl-top",
                    span { class: "bl-title", "{build.workflow_display()}" }
                    span { class: "bl-time", "{when}" }
                }
                div { class: "bl-sub", "{app} · {build.git_ref()}{number}" }
            }
        }
    }
}

/// Section label for a build's day: "Today", "Yesterday", or e.g. "July 22, 2026".
fn day_label(dt: DateTime<Utc>) -> String {
    let local = dt.with_timezone(&Local);
    let today = Local::now().date_naive();
    let day = local.date_naive();
    if day == today {
        "Today".to_string()
    } else if Some(day) == today.pred_opt() {
        "Yesterday".to_string()
    } else {
        local.format("%B %-d, %Y").to_string()
    }
}

/// Human relative time, e.g. "just now", "5 minutes ago", "2 days ago",
/// "last week", "3 months ago", "last year".
fn relative_time(dt: DateTime<Utc>) -> String {
    let secs = (Utc::now() - dt).num_seconds().max(0);
    let (mins, hours, days) = (secs / 60, secs / 3600, secs / 86_400);
    let (weeks, months, years) = (days / 7, days / 30, days / 365);
    let s = |n: i64| if n == 1 { "" } else { "s" };
    if secs < 60 {
        "just now".into()
    } else if mins < 60 {
        format!("{mins} minute{} ago", s(mins))
    } else if hours < 24 {
        format!("{hours} hour{} ago", s(hours))
    } else if days == 1 {
        "yesterday".into()
    } else if days < 7 {
        format!("{days} days ago")
    } else if weeks == 1 {
        "last week".into()
    } else if days < 30 {
        format!("{weeks} weeks ago")
    } else if months == 1 {
        "last month".into()
    } else if months < 12 {
        format!("{months} months ago")
    } else if years == 1 {
        "last year".into()
    } else {
        format!("{years} years ago")
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
