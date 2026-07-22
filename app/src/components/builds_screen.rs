//! The main authenticated screen: a build-list sidebar on the left and a
//! detail pane on the right.

use std::collections::HashMap;

use codemagic_core::models::Build;
use dioxus::prelude::*;

use super::build_detail::BuildDetail;
use crate::state::AppState;

#[component]
pub fn BuildsScreen() -> Element {
    let mut state = use_context::<AppState>();

    // Currently selected build id, shared with the detail pane.
    let selected = use_signal(|| Option::<String>::None);

    let mut builds = use_resource(move || {
        let client = state.client();
        async move { client.get_builds(0, None, None).await }
    });

    rsx! {
        div { class: "layout",
            aside { class: "sidebar",
                header { class: "topbar",
                    h1 { "Builds" }
                    div { class: "actions",
                        button { class: "ghost", onclick: move |_| builds.restart(), "Refresh" }
                        button { class: "ghost", onclick: move |_| state.sign_out(), "Sign out" }
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
