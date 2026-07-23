//! The main authenticated screen: a build-list sidebar on the left and a
//! detail pane on the right.

use std::collections::{HashMap, HashSet};

use chrono::{DateTime, Local, Utc};
use codemagic_core::{PAGE_SIZE, models::Build};
use dioxus::prelude::*;

use super::build_detail::BuildDetail;
use super::icons::{GearIcon, PlusIcon, RefreshIcon};
use super::new_build::NewBuildModal;
use super::settings::SettingsModal;
use crate::state::AppState;

/// One accumulated view of the build list: every page loaded so far, the app
/// names referenced by those builds, and whether more pages remain.
#[derive(Clone, PartialEq)]
struct BuildPage {
    builds: Vec<Build>,
    names: HashMap<String, String>,
    has_more: bool,
}

#[component]
pub fn BuildsScreen() -> Element {
    let state = use_context::<AppState>();

    // Currently selected build id, shared with the detail pane.
    let mut selected = use_signal(|| Option::<String>::None);
    let mut new_build_open = use_signal(|| false);
    let mut settings_open = use_signal(|| false);
    let mut refreshing = use_signal(|| false);
    // How many pages to load; reaching the bottom of the list bumps this.
    let mut pages = use_signal(|| 1usize);
    // Optional workflow id to restrict the list to.
    let mut workflow_filter = use_signal(|| Option::<String>::None);
    // Every workflow seen so far, as (id, display name). Only ever grows, so
    // filtering to one workflow can't shrink the set of choices offered.
    let mut known_workflows = use_signal(Vec::<(String, String)>::new);

    // Fetches every loaded page in sequence and concatenates them. Refetching
    // all of them (rather than appending) keeps the list consistent as new
    // builds shift the skip-based pagination window.
    let mut builds = use_resource(move || {
        let client = state.client();
        let n = pages();
        let wf = workflow_filter();
        async move {
            let mut all: Vec<Build> = Vec::new();
            let mut seen: HashSet<String> = HashSet::new();
            let mut names: HashMap<String, String> = HashMap::new();
            let mut has_more = false;
            for _ in 0..n {
                // `skip` is the number of builds held so far, not page * size:
                // the API can return more than PAGE_SIZE per response.
                let resp = client.get_builds(all.len(), wf.as_deref(), None).await?;
                for a in &resp.applications {
                    names.insert(a.id.clone(), a.name.clone());
                }
                has_more = resp.builds.len() >= PAGE_SIZE;
                let before = all.len();
                // A build can repeat across pages if new ones arrived meanwhile.
                for b in resp.builds {
                    if seen.insert(b.id.clone()) {
                        all.push(b);
                    }
                }
                // Stop if this page added nothing new, so a stalled cursor can't
                // spin forever.
                if !has_more || all.len() == before {
                    break;
                }
            }
            anyhow::Ok(BuildPage {
                builds: all,
                names,
                has_more,
            })
        }
    });

    // Last good list, so refreshing or loading more never blanks the sidebar.
    let mut cached = use_signal(|| Option::<BuildPage>::None);

    // Clear the spinner once a (re)fetch resolves, keep the cache current, and
    // fold any newly seen workflows into the filter choices.
    use_effect(move || {
        // Snapshot first so no borrow is held while writing back to signals.
        let resolved = builds
            .read()
            .as_ref()
            .map(|result| result.as_ref().ok().cloned());
        let Some(page) = resolved else { return };
        refreshing.set(false);
        let Some(page) = page else { return };

        let mut seen: HashSet<String> =
            known_workflows.read().iter().map(|(id, _)| id.clone()).collect();
        let fresh: Vec<(String, String)> = page
            .builds
            .iter()
            .filter_map(|b| b.effective_workflow_id().map(|id| (id.to_string(), b)))
            .filter(|(id, _)| seen.insert(id.clone()))
            .map(|(id, b)| (id, b.workflow_display().to_string()))
            .collect();

        cached.set(Some(page));
        if !fresh.is_empty() {
            known_workflows.write().extend(fresh);
        }
    });

    // True while a fetch is in flight (auto-refresh or "Load more").
    let loading = builds.read().is_none();

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

    // Snapshot signal state into owned locals. Holding a `read()` guard alive
    // inside `rsx!` keeps it borrowed across the reactive flush, which
    // deadlocks the app as soon as an effect writes the same signal.
    let workflows: Vec<(String, String)> = known_workflows.read().clone();
    let current_filter = workflow_filter().unwrap_or_default();
    let page_snapshot: Option<BuildPage> = cached.read().clone();
    let load_error: Option<String> = builds
        .read()
        .as_ref()
        .and_then(|r| r.as_ref().err().map(|e| e.to_string()));

    let spinning = refreshing() || loading;

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
                if !workflows.is_empty() {
                    div { class: "filter-bar",
                        select {
                            class: "wf-filter",
                            value: "{current_filter}",
                            onchange: move |e| {
                                let v = e.value();
                                workflow_filter.set((!v.is_empty()).then_some(v));
                                // Restart paging, and drop the old list so the
                                // sidebar can't briefly show unfiltered builds.
                                pages.set(1);
                                cached.set(None);
                            },
                            option { value: "", "All workflows" }
                            for (id, name) in workflows.iter() {
                                option { key: "{id}", value: "{id}", "{name}" }
                            }
                        }
                    }
                }
                div { class: "sidebar-list",
                    match page_snapshot.as_ref() {
                        // Nothing loaded yet: show the first error, or loading.
                        None => match load_error.as_ref() {
                            Some(e) => rsx! {
                                div { class: "error-box",
                                    p { "Couldn't load builds." }
                                    p { class: "muted", "{e}" }
                                    button { class: "ghost", onclick: move |_| builds.restart(), "Retry" }
                                }
                            },
                            None => rsx! { p { class: "muted center", "Loading builds…" } },
                        },
                        Some(page) if page.builds.is_empty() => {
                            rsx! { p { class: "muted center", "No builds yet." } }
                        }
                        Some(page) => {
                            // Group consecutive builds by calendar day.
                            let mut groups: Vec<(String, Vec<Build>)> = Vec::new();
                            for b in &page.builds {
                                let label = b.display_time().map(day_label).unwrap_or_else(|| "Earlier".into());
                                match groups.last_mut() {
                                    Some((l, v)) if *l == label => v.push(b.clone()),
                                    _ => groups.push((label, vec![b.clone()])),
                                }
                            }
                            let has_more = page.has_more;
                            let names = page.names.clone();
                            rsx! {
                                for (label, gbuilds) in groups.iter() {
                                    div { key: "{label}", class: "day-group",
                                        div { class: "day-header", "{label}" }
                                        ul { class: "build-list",
                                            for build in gbuilds.iter() {
                                                BuildRow {
                                                    key: "{build.id}",
                                                    data: build.clone(),
                                                    app_name: names.get(&build.app_id).cloned(),
                                                    selected,
                                                }
                                            }
                                        }
                                    }
                                }
                                // Infinite scroll: this sentinel sits below the
                                // last row, so scrolling it into view pulls the
                                // next page. Clipping by the scroll container
                                // means it only intersects once actually reached.
                                if has_more {
                                    div {
                                        class: "list-end",
                                        onvisible: move |e| {
                                            if e.data().is_intersecting().unwrap_or(false) && !loading {
                                                pages += 1;
                                            }
                                        },
                                        div { class: "list-spinner" }
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
