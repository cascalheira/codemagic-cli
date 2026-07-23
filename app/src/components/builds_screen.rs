//! The main authenticated screen: a build-list sidebar on the left and a
//! detail pane on the right.

use std::collections::{HashMap, HashSet};

use chrono::{DateTime, Local, Utc};
use dioxus::prelude::*;
use gantry_core::{
    PAGE_SIZE,
    api_v3::{self, BuildFilter, BuildStatusFilter, Team, V3Client},
    models::{Build, WorkflowChoice},
};

use super::app_info::AppInfoModal;
use super::build_detail::BuildDetail;
use super::icons::{BranchIcon, GearIcon, InfoIcon, PlusIcon, RefreshIcon, TagIcon};
use super::new_build::NewBuildModal;
use super::settings::SettingsModal;
use super::shortcuts::{HelpModal, Shortcut, step, use_shortcuts};
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
    let mut help_open = use_signal(|| false);
    let mut app_info_open = use_signal(|| false);
    // A newer release, once the startup check finds one and until dismissed.
    let mut update = use_signal(|| Option::<gantry_core::update::Update>::None);
    let mut refreshing = use_signal(|| false);
    // How many pages to load; reaching the bottom of the list bumps this.
    let mut pages = use_signal(|| 1usize);
    // Optional workflow to restrict the list to.
    let mut workflow_filter = use_signal(|| Option::<WorkflowChoice>::None);
    // Optional build status to restrict the list to. Needs the v3 listing.
    let mut status_filter = use_signal(|| Option::<BuildStatusFilter>::None);
    // Every workflow seen so far. Only ever grows, so filtering to one workflow
    // can't shrink the set of choices offered.
    let mut known_workflows = use_signal(Vec::<WorkflowChoice>::new);
    // Teams the token can see; `None` until resolved. Empty means the v3
    // listing is unavailable and the v1 one is used instead.
    let mut teams = use_signal(|| Option::<Vec<Team>>::None);
    // Last status seen per build id, used to spot running -> finished
    // transitions. `None` until the first load completes, so the builds that
    // already existed at startup don't all fire notifications at once.
    let mut last_status = use_signal(|| Option::<HashMap<String, String>>::None);

    // Fetches every loaded page in sequence and concatenates them. Refetching
    // all of them (rather than appending) keeps the list consistent as new
    // builds arrive and shift the pagination window.
    let mut builds = use_resource(move || {
        let client = state.client();
        let v3 = state.v3_client();
        let n = pages();
        let wf = workflow_filter();
        let status = status_filter();
        async move {
            // Teams gate the v3 listing and rarely change, so they are resolved
            // once and reused for the lifetime of the screen.
            // Scoped so the read guard is dropped before the write below.
            let known = teams.peek().clone();
            let teams = match known {
                Some(known) => known,
                None => {
                    // A failure here is a downgrade, not an error: the v1 list
                    // still works, it just can't filter by status.
                    let resolved = v3.list_teams().await.unwrap_or_default();
                    teams.set(Some(resolved.clone()));
                    resolved
                }
            };

            if teams.is_empty() {
                return load_v1(&client, n, wf.as_ref()).await;
            }
            let filter = BuildFilter {
                app_id: wf.as_ref().map(|w| w.app_id.clone()),
                workflow_id: wf.as_ref().map(|w| w.id.clone()),
                status,
                ..Default::default()
            };
            load_v3(&v3, &teams, n, &filter).await
        }
    });

    // App names for the list rows. The v1 listing embeds them in its response,
    // but v3 build pages carry only app IDs, so they are fetched separately —
    // once, rather than on every auto-refresh.
    let app_names = use_resource(move || {
        let client = state.client();
        async move {
            client
                .get_apps()
                .await
                .map(|apps| {
                    apps.into_iter()
                        .map(|a| (a.id, a.name))
                        .collect::<HashMap<String, String>>()
                })
                .unwrap_or_default()
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

        // `peek` reads without subscribing. Using `read` here would make this
        // effect depend on signals it also writes, retriggering itself forever.
        // Keyed by (app, workflow): the same workflow ID can appear under more
        // than one app, and only the pair identifies a workflow.
        let mut seen: HashSet<String> = known_workflows.peek().iter().map(|w| w.key()).collect();
        let fresh: Vec<WorkflowChoice> = page
            .builds
            .iter()
            .filter_map(|b| b.workflow_choice())
            .filter(|choice| seen.insert(choice.key()))
            .collect();

        // Notify on builds that have just left a running state. The first load
        // only seeds the map, so pre-existing builds don't all notify at once.
        let previous = last_status.peek().clone();
        let current: Vec<(String, String)> = page
            .builds
            .iter()
            .map(|b| (b.id.clone(), b.status.clone()))
            .collect();
        let just_finished = newly_finished(previous.as_ref(), &current);
        let finished: Vec<(String, String, String)> = just_finished
            .iter()
            .filter_map(|id| page.builds.iter().find(|b| &b.id == id))
            .map(|b| {
                let app = page
                    .names
                    .get(&b.app_id)
                    .cloned()
                    .unwrap_or_else(|| "Unknown app".to_string());
                (app, b.workflow_display().to_string(), b.status.clone())
            })
            .collect();
        last_status.set(Some(current.into_iter().collect()));

        cached.set(Some(page));
        if !fresh.is_empty() {
            known_workflows.write().extend(fresh);
        }

        for (app, workflow, status) in finished {
            let title = match gantry_core::status::outcome(&status) {
                Some(true) => "Build succeeded",
                Some(false) => "Build failed",
                None => "Build finished",
            };
            crate::notify::build_finished(title, &format!("{app} · {workflow} ({status})"));
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

    // One update check per launch, and only if the user hasn't opted out. A
    // failed check is silent: an unreachable GitHub is not the user's problem
    // to solve, and nagging about it every launch would be worse than useless.
    let check_updates = state.check_updates;
    use_future(move || async move {
        if !*check_updates.peek() {
            return;
        }
        if let Ok(Some(found)) = gantry_core::update::check(env!("CARGO_PKG_VERSION")).await {
            update.set(Some(found));
        }
    });

    // Keyboard shortcuts. Everything here reads with `peek()`: this closure
    // runs from a background task, and subscribing to signals outside the
    // render would make it a reactive dependency of itself.
    let mut handle = move |shortcut: Shortcut| {
        // An open overlay swallows everything but Escape, so typing a branch
        // name in the wizard can't drive the list behind it.
        if *help_open.peek()
            || *new_build_open.peek()
            || *settings_open.peek()
            || *app_info_open.peek()
        {
            if shortcut == Shortcut::Close {
                help_open.set(false);
                new_build_open.set(false);
                settings_open.set(false);
                app_info_open.set(false);
            }
            return;
        }
        // Snapshot the list before doing anything that writes a signal.
        let list: Vec<Build> = cached
            .peek()
            .as_ref()
            .map(|p| p.builds.clone())
            .unwrap_or_default();
        let current = selected.peek().clone();

        match shortcut {
            Shortcut::Next | Shortcut::Prev => {
                let delta = if shortcut == Shortcut::Next { 1 } else { -1 };
                let order: Vec<String> = list.iter().map(|b| b.id.clone()).collect();
                if let Some(next) = step(&order, current.as_deref(), delta) {
                    selected.set(Some(next));
                    // Keep the new row on screen. The scroll has to wait for
                    // the row to actually be marked selected in the DOM.
                    spawn(async move {
                        tokio::time::sleep(std::time::Duration::from_millis(60)).await;
                        document::eval(
                            "document.querySelector('.build-row.selected')\
                             ?.scrollIntoView({block:'nearest'})",
                        );
                    });
                }
            }
            Shortcut::Refresh => {
                refreshing.set(true);
                builds.restart();
            }
            Shortcut::NewBuild => new_build_open.set(true),
            Shortcut::Settings => settings_open.set(true),
            Shortcut::OpenInBrowser => {
                if let Some(build) = current.and_then(|id| list.iter().find(|b| b.id == id)) {
                    gantry_core::web::open_in_browser(&gantry_core::web::build_url(
                        &build.app_id,
                        &build.id,
                    ));
                }
            }
            Shortcut::FocusFilter => {
                document::eval("document.querySelector('.wf-filter')?.focus()");
            }
            Shortcut::AppInfo => app_info_open.set(true),
            Shortcut::Help => help_open.set(true),
            // Nothing is open, so Escape clears the selection.
            Shortcut::Close => selected.set(None),
        }
    };
    use_shortcuts(handle);

    // The native menu bar drives the same actions, plus a couple of links.
    #[cfg(feature = "desktop")]
    crate::menu::use_menu_commands(move |command| match command {
        crate::menu::MenuCommand::Action(shortcut) => handle(shortcut),
        crate::menu::MenuCommand::OpenUrl(url) => gantry_core::web::open_in_browser(url),
    });

    // Snapshot signal state into owned locals. Holding a `read()` guard alive
    // inside `rsx!` keeps it borrowed across the reactive flush, which
    // deadlocks the app as soon as an effect writes the same signal.
    let workflows: Vec<WorkflowChoice> = known_workflows.read().clone();
    let current_filter = workflow_filter().map(|w| w.key()).unwrap_or_default();
    let current_status = status_filter()
        .map(|s| s.as_str().to_string())
        .unwrap_or_default();
    // Workflow names are only unique within an app, so qualify them once more
    // than one app is in play.
    let multi_app = workflows
        .iter()
        .map(|w| &w.app_id)
        .collect::<HashSet<_>>()
        .len()
        > 1;
    // The status filter is server-side and only exists on the v3 listing.
    let can_filter_status = teams().is_some_and(|t| !t.is_empty());
    let page_snapshot: Option<BuildPage> = cached.read().clone();
    let load_error: Option<String> = builds
        .read()
        .as_ref()
        .and_then(|r| r.as_ref().err().map(|e| e.to_string()));

    let spinning = refreshing() || loading;
    let update_banner: Option<gantry_core::update::Update> = update.read().clone();

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
                            title: "App & workflow IDs",
                            onclick: move |_| app_info_open.set(true),
                            InfoIcon {}
                        }
                        button {
                            class: "ghost icon-btn",
                            title: "Settings",
                            onclick: move |_| settings_open.set(true),
                            GearIcon {}
                        }
                    }
                }
                if !workflows.is_empty() || can_filter_status {
                    div { class: "filter-bar",
                        if !workflows.is_empty() {
                            select {
                                class: "wf-filter",
                                value: "{current_filter}",
                                onchange: move |e| {
                                    let key = e.value();
                                    let choices = known_workflows.peek().clone();
                                    workflow_filter.set(
                                        WorkflowChoice::from_key(&choices, &key).cloned(),
                                    );
                                    // Restart paging, and drop the old list so the
                                    // sidebar can't briefly show unfiltered builds.
                                    pages.set(1);
                                    cached.set(None);
                                },
                                option { value: "", "All workflows" }
                                for choice in workflows.iter() {
                                    option {
                                        key: "{choice.key()}",
                                        value: "{choice.key()}",
                                        if multi_app {
                                            {format!("{} · {}", app_name_of(&app_names, &choice.app_id), choice.name)}
                                        } else {
                                            "{choice.name}"
                                        }
                                    }
                                }
                            }
                        }
                        if can_filter_status {
                            select {
                                class: "status-filter",
                                value: "{current_status}",
                                onchange: move |e| {
                                    status_filter.set(BuildStatusFilter::from_wire(&e.value()));
                                    pages.set(1);
                                    cached.set(None);
                                },
                                option { value: "", "Any status" }
                                for status in BuildStatusFilter::ALL {
                                    option {
                                        key: "{status.as_str()}",
                                        value: "{status.as_str()}",
                                        "{status.label()}"
                                    }
                                }
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
                            // Group consecutive builds by calendar day. Each
                            // keeps its overall list position, which drives
                            // the staggered entrance animation.
                            let mut groups: Vec<(String, Vec<(usize, Build)>)> = Vec::new();
                            for (i, b) in page.builds.iter().enumerate() {
                                let label = b.display_time().map(day_label).unwrap_or_else(|| "Earlier".into());
                                match groups.last_mut() {
                                    Some((l, v)) if *l == label => v.push((i, b.clone())),
                                    _ => groups.push((label, vec![(i, b.clone())])),
                                }
                            }
                            let has_more = page.has_more;
                            // v1 embeds app names in the listing; v3 does not,
                            // so fall back to the separately fetched map.
                            let mut names = app_names.read().clone().unwrap_or_default();
                            names.extend(page.names.clone());
                            // With a single app in view its name is on every
                            // row and carries no information, so drop it and
                            // give the space back to the workflow name.
                            let show_app = distinct_apps(&page.builds) > 1;
                            rsx! {
                                for (label, gbuilds) in groups.iter() {
                                    div { key: "{label}", class: "day-group",
                                        div { class: "day-header", "{label}" }
                                        ul { class: "build-list",
                                            for (order, build) in gbuilds.iter() {
                                                BuildRow {
                                                    key: "{build.id}",
                                                    data: build.clone(),
                                                    app_name: show_app.then(|| names.get(&build.app_id).cloned()).flatten(),
                                                    order: *order,
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
                if let Some(release) = update_banner.as_ref() {
                    {
                        let url = release.url.clone();
                        rsx! {
                            div { class: "update-banner",
                                div { class: "update-text",
                                    strong { "Version {release.version} is available" }
                                    span { class: "muted", "You're on {env!(\"CARGO_PKG_VERSION\")}" }
                                }
                                button {
                                    class: "primary small",
                                    onclick: move |_| gantry_core::web::open_in_browser(&url),
                                    "Get it"
                                }
                                button {
                                    class: "ghost icon-btn",
                                    title: "Dismiss",
                                    onclick: move |_| update.set(None),
                                    "×"
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
                BuildDetail {
                    selected,
                    on_started: move |build_id: String| {
                        builds.restart();
                        selected.set(Some(build_id));
                    },
                }
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
        if help_open() {
            HelpModal { open: help_open }
        }
        if app_info_open() {
            AppInfoModal { open: app_info_open }
        }
    }
}

#[component]
fn BuildRow(
    data: Build,
    app_name: Option<String>,
    order: usize,
    selected: Signal<Option<String>>,
) -> Element {
    let build = &data;
    let id = build.id.clone();
    let is_selected = selected.read().as_deref() == Some(id.as_str());
    // Stagger the entrance by list position, capped so late pages and long
    // lists don't make rows near the bottom wait noticeably.
    let delay = order.min(16) * 18;

    let number = build
        .display_build_number()
        .map(|n| format!("· #{n}"))
        .unwrap_or_default();
    let when = build.display_time().map(relative_time).unwrap_or_default();

    // Prefer the branch; fall back to the tag (with its own glyph).
    let branch = build.branch.clone().filter(|b| !b.is_empty());
    let tag = build.tag.clone().filter(|t| !t.is_empty());
    let git_ref = branch.clone().or(tag.clone());

    let mut cls = String::from("build-row");
    // The one broken build in a sea of green should pop before its dot is
    // even looked at.
    if status_class(&build.status) == "fail" {
        cls.push_str(" failed");
    }
    if is_selected {
        cls.push_str(" selected");
    }

    rsx! {
        li {
            class: "{cls}",
            style: "animation-delay: {delay}ms",
            onclick: move |_| selected.set(Some(id.clone())),
            span { class: "status-dot {status_class(&build.status)}" }
            div { class: "build-main",
                div { class: "bl-top",
                    span { class: "bl-title", "{build.workflow_display()}" }
                    span { class: "bl-time", "{when}" }
                }
                div { class: "bl-sub",
                    // `None` when every visible build belongs to the same app.
                    if let Some(app) = app_name.as_ref() {
                        span { class: "bl-app", "{app} ·" }
                    }
                    if let Some(r) = git_ref.as_ref() {
                        if branch.is_some() {
                            BranchIcon {}
                        } else {
                            TagIcon {}
                        }
                        span { class: "bl-ref", "{r}" }
                    }
                    if !number.is_empty() {
                        span { class: "bl-num", "{number}" }
                    }
                }
            }
        }
    }
}

/// Loads `pages` pages from the v1 endpoint, which only filters by workflow.
///
/// The fallback path: used when the token sees no teams, or the teams lookup
/// failed.
async fn load_v1(
    client: &gantry_core::ApiClient,
    pages: usize,
    workflow: Option<&WorkflowChoice>,
) -> anyhow::Result<BuildPage> {
    let mut all: Vec<Build> = Vec::new();
    let mut seen: HashSet<String> = HashSet::new();
    let mut names: HashMap<String, String> = HashMap::new();
    let mut has_more = false;

    for _ in 0..pages {
        // `skip` is the number of builds held so far, not page * size: the API
        // can return more than PAGE_SIZE per response.
        let resp = client
            .get_builds(all.len(), workflow.map(|w| w.id.as_str()), None)
            .await?;
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
        // Stop if this page added nothing new, so a stalled cursor can't spin
        // forever.
        if !has_more || all.len() == before {
            break;
        }
    }

    Ok(BuildPage {
        builds: all,
        names,
        has_more,
    })
}

/// Loads `pages` pages from the v3 listing, merged across every team.
///
/// Each team pages independently by cursor, so the merged result is sorted by
/// time at the end rather than being ordered by arrival. A team that fails is
/// skipped: with several teams, one inaccessible team should not blank the
/// list. The error only propagates when nothing at all came back.
async fn load_v3(
    client: &V3Client,
    teams: &[Team],
    pages: usize,
    filter: &BuildFilter,
) -> anyhow::Result<BuildPage> {
    let mut all: Vec<Build> = Vec::new();
    let mut seen: HashSet<String> = HashSet::new();
    let mut has_more = false;
    let mut last_error = None;

    for team in teams {
        let mut cursor: Option<String> = None;
        for page_index in 0..pages {
            // Every page after the first needs the previous page's cursor; not
            // having one means this team is exhausted.
            if page_index > 0 && cursor.is_none() {
                break;
            }
            match client
                .list_team_builds(&team.id, filter, cursor.as_deref(), api_v3::PAGE_SIZE)
                .await
            {
                Ok(page) => {
                    for b in page.builds {
                        if seen.insert(b.id.clone()) {
                            all.push(b);
                        }
                    }
                    cursor = page.cursor;
                }
                Err(e) => {
                    last_error = Some(e);
                    break;
                }
            }
        }
        has_more |= cursor.is_some();
    }

    if let Some(e) = last_error
        && all.is_empty()
    {
        return Err(e);
    }

    all.sort_by_key(|b| std::cmp::Reverse(b.display_time()));

    Ok(BuildPage {
        builds: all,
        // Filled in from the apps endpoint; v3 build pages carry only app IDs.
        names: HashMap::new(),
        has_more,
    })
}

/// An app's display name from the fetched name map, falling back to a short
/// form of its ID while the map is still loading.
fn app_name_of(names: &Resource<HashMap<String, String>>, app_id: &str) -> String {
    names
        .read()
        .as_ref()
        .and_then(|m| m.get(app_id).cloned())
        .unwrap_or_else(|| app_id[..app_id.len().min(6)].to_string())
}

/// How many distinct apps the visible builds belong to.
fn distinct_apps(builds: &[Build]) -> usize {
    builds
        .iter()
        .map(|b| b.app_id.as_str())
        .collect::<HashSet<_>>()
        .len()
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

/// Build ids that have just left a running state, given the statuses seen on
/// the previous load.
///
/// Returns nothing when `previous` is `None` (the very first load), so builds
/// that were already finished at startup don't all fire notifications at once.
fn newly_finished(
    previous: Option<&HashMap<String, String>>,
    current: &[(String, String)],
) -> Vec<String> {
    let Some(prev) = previous else {
        return Vec::new();
    };
    current
        .iter()
        .filter(|(id, status)| {
            prev.get(id)
                .is_some_and(|old| gantry_core::status::is_running(old))
                && !gantry_core::status::is_running(status)
        })
        .map(|(id, _)| id.clone())
        .collect()
}

/// Maps a Codemagic status string to a CSS modifier class.
pub fn status_class(status: &str) -> &'static str {
    gantry_core::status::class(status)
}

#[cfg(test)]
mod tests {
    use super::distinct_apps;

    fn build(app_id: &str) -> super::Build {
        serde_json::from_value(serde_json::json!({
            "_id": format!("b-{app_id}"), "appId": app_id, "status": "finished",
        }))
        .unwrap()
    }

    #[test]
    fn one_app_means_its_name_is_redundant() {
        assert_eq!(distinct_apps(&[build("a"), build("a")]), 1);
    }

    #[test]
    fn two_apps_need_naming() {
        assert_eq!(distinct_apps(&[build("a"), build("b"), build("a")]), 2);
    }

    #[test]
    fn an_empty_list_has_no_apps() {
        assert_eq!(distinct_apps(&[]), 0);
    }

    use super::*;

    fn map(pairs: &[(&str, &str)]) -> HashMap<String, String> {
        pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect()
    }

    fn pairs(items: &[(&str, &str)]) -> Vec<(String, String)> {
        items
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect()
    }

    #[test]
    fn first_load_never_notifies() {
        // Everything is "new" on the first load; notifying would spam the user.
        let current = pairs(&[("a", "finished"), ("b", "failed")]);
        assert!(newly_finished(None, &current).is_empty());
    }

    #[test]
    fn notifies_only_on_running_to_terminal() {
        let previous = map(&[
            ("a", "building"), // -> finished: notify
            ("b", "queued"),   // -> failed:   notify
            ("c", "building"), // -> testing:  still running, no
            ("d", "finished"), // -> finished: unchanged, no
        ]);
        let current = pairs(&[
            ("a", "finished"),
            ("b", "failed"),
            ("c", "testing"),
            ("d", "finished"),
        ]);
        let mut got = newly_finished(Some(&previous), &current);
        got.sort();
        assert_eq!(got, vec!["a".to_string(), "b".to_string()]);
    }

    #[test]
    fn unseen_builds_do_not_notify() {
        // A build that appears already-finished was never observed running.
        let previous = map(&[("a", "building")]);
        let current = pairs(&[("a", "building"), ("new", "finished")]);
        assert!(newly_finished(Some(&previous), &current).is_empty());
    }

    #[test]
    fn cancelled_and_timeout_count_as_finished() {
        let previous = map(&[("a", "building"), ("b", "publishing")]);
        let current = pairs(&[("a", "canceled"), ("b", "timeout")]);
        let mut got = newly_finished(Some(&previous), &current);
        got.sort();
        assert_eq!(got, vec!["a".to_string(), "b".to_string()]);
    }
}
