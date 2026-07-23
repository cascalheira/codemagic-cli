//! App & workflow ID browser — the GUI counterpart of the terminal client's
//! `i` popup. These IDs are what `codemagic.yaml`, the public API, and CI
//! scripts refer to, and the web UI buries them, so the point of this sheet is
//! to find one and copy it.

use gantry_core::models::Application;
use dioxus::prelude::*;

use super::icons::CopyIcon;
use crate::state::AppState;

#[component]
pub fn AppInfoModal(open: Signal<bool>) -> Element {
    let state = use_context::<AppState>();
    let mut query = use_signal(String::new);
    let mut copied = use_signal(|| Option::<String>::None);

    let apps = use_resource(move || {
        let client = state.client();
        async move { client.get_apps().await }
    });

    // Owned snapshots: a read guard held live inside `rsx!` stays borrowed
    // across the reactive flush, which deadlocks once a signal is written.
    let loaded: Option<Result<Vec<Application>, String>> = match &*apps.read() {
        None => None,
        Some(Err(e)) => Some(Err(e.to_string())),
        Some(Ok(list)) => Some(Ok(list.clone())),
    };
    let q = query.read().clone();
    let copied_id = copied.read().clone();

    rsx! {
        div { class: "modal-overlay", onclick: move |_| open.set(false),
            div { class: "modal info-modal", onclick: move |e| e.stop_propagation(),
                div { class: "modal-head",
                    h3 { "App & workflow IDs" }
                    button { class: "ghost small", onclick: move |_| open.set(false), "Close" }
                }
                div { class: "info-search",
                    input {
                        r#type: "search",
                        placeholder: "Search apps, workflows, or IDs…",
                        value: "{q}",
                        oninput: move |e| query.set(e.value()),
                    }
                }
                div { class: "info-body",
                    match loaded.as_ref() {
                        None => rsx! { p { class: "muted center", "Loading apps…" } },
                        Some(Err(e)) => rsx! {
                            div { class: "error-box",
                                p { "Couldn't load apps." }
                                p { class: "muted", "{e}" }
                            }
                        },
                        Some(Ok(list)) => {
                            let shown = matching(list, &q);
                            if shown.is_empty() {
                                rsx! {
                                    p { class: "muted center",
                                        if list.is_empty() { "No apps found." } else { "Nothing matches that search." }
                                    }
                                }
                            } else {
                                rsx! {
                                    for app in shown.iter() {
                                        AppInfoCard {
                                            key: "{app.id}",
                                            app: app.clone(),
                                            copied_id: copied_id.clone(),
                                            on_copy: move |id: String| {
                                                crate::clipboard::copy_text(&id);
                                                copied.set(Some(id));
                                            },
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
}

#[component]
fn AppInfoCard(app: Application, copied_id: Option<String>, on_copy: EventHandler<String>) -> Element {
    // Stable ordering; the API returns workflows in a HashMap.
    let mut workflows: Vec<(String, String)> = app
        .workflows
        .iter()
        .map(|(id, w)| (id.clone(), w.name.clone()))
        .collect();
    workflows.sort_by_key(|(_, name)| name.to_lowercase());

    rsx! {
        section { class: "info-card",
            div { class: "info-app",
                h4 { "{app.name}" }
                IdRow {
                    label: "App ID".to_string(),
                    id: app.id.clone(),
                    copied: copied_id.as_deref() == Some(app.id.as_str()),
                    on_copy,
                }
            }
            if workflows.is_empty() {
                p { class: "muted info-none", "No Workflow-Editor workflows. This app is configured by codemagic.yaml." }
            } else {
                for (id, name) in workflows.iter() {
                    IdRow {
                        key: "{id}",
                        label: name.clone(),
                        id: id.clone(),
                        copied: copied_id.as_deref() == Some(id.as_str()),
                        on_copy,
                    }
                }
            }
        }
    }
}

#[component]
fn IdRow(label: String, id: String, copied: bool, on_copy: EventHandler<String>) -> Element {
    let to_copy = id.clone();
    rsx! {
        div { class: "info-row",
            span { class: "info-label", "{label}" }
            code { class: "info-id selectable", "{id}" }
            button {
                class: "ghost icon-btn",
                title: "Copy {label}",
                onclick: move |_| on_copy.call(to_copy.clone()),
                if copied {
                    span { class: "info-copied", "Copied" }
                } else {
                    CopyIcon {}
                }
            }
        }
    }
}

/// The apps to show for `query`.
///
/// An app matches on its own name or id, and also on any of its workflows —
/// searching for a workflow should surface the app that owns it rather than
/// nothing at all.
fn matching(apps: &[Application], query: &str) -> Vec<Application> {
    let needle = query.trim().to_lowercase();
    if needle.is_empty() {
        return apps.to_vec();
    }
    apps.iter()
        .filter(|a| {
            a.name.to_lowercase().contains(&needle)
                || a.id.to_lowercase().contains(&needle)
                || a.workflows.iter().any(|(id, w)| {
                    id.to_lowercase().contains(&needle) || w.name.to_lowercase().contains(&needle)
                })
        })
        .cloned()
        .collect()
}

#[cfg(test)]
mod tests {
    use super::matching;
    use gantry_core::models::{Application, WorkflowInfo};
    use std::collections::HashMap;

    fn app(id: &str, name: &str, workflows: &[(&str, &str)]) -> Application {
        Application {
            id: id.to_string(),
            name: name.to_string(),
            workflows: workflows
                .iter()
                .map(|(wid, wname)| {
                    (wid.to_string(), WorkflowInfo { name: wname.to_string() })
                })
                .collect::<HashMap<_, _>>(),
            branches: Vec::new(),
        }
    }

    fn fixture() -> Vec<Application> {
        vec![
            app("aaa111", "First App", &[("ios-release", "iOS Release")]),
            app("bbb222", "Second App", &[("android-beta", "Android Beta")]),
        ]
    }

    #[test]
    fn an_empty_query_shows_every_app() {
        assert_eq!(matching(&fixture(), "   ").len(), 2);
    }

    #[test]
    fn matches_app_name_case_insensitively() {
        let found = matching(&fixture(), "first app");
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].name, "First App");
    }

    #[test]
    fn matches_on_app_id() {
        assert_eq!(matching(&fixture(), "bbb")[0].name, "Second App");
    }

    /// Searching for a workflow should surface its owning app, not nothing.
    #[test]
    fn matches_on_workflow_name_and_id() {
        assert_eq!(matching(&fixture(), "Android Be")[0].name, "Second App");
        assert_eq!(matching(&fixture(), "ios-rel")[0].name, "First App");
    }

    #[test]
    fn no_match_yields_nothing() {
        assert!(matching(&fixture(), "zzz").is_empty());
    }
}
