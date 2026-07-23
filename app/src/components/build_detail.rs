//! Center build-info column (metadata + expandable step logs) plus the
//! right-hand artifacts download rail.

use std::collections::HashMap;
use std::path::PathBuf;
use std::time::Duration;

use chrono::{DateTime, Utc};
use dioxus::prelude::*;
use gantry_core::{
    ApiClient,
    models::{Artefact, BuildDetailResponse},
};

/// How often to re-poll a build that hasn't reached a terminal state. Polling
/// stops on its own once the build finishes.
const POLL_SECS: u64 = 5;

use super::builds_screen::status_class;
use super::icons::{
    ChevronIcon, CopyIcon, DownloadIcon, ExternalLinkIcon, JumpIcon, RerunIcon, StopIcon, WrapIcon,
};
use crate::state::AppState;

#[component]
pub fn BuildDetail(selected: Signal<Option<String>>, on_started: EventHandler<String>) -> Element {
    let state = use_context::<AppState>();
    let mut dl_status = use_signal(|| Option::<String>::None);
    // Per-artifact download progress, name → (done, total). Shared between
    // the per-card downloads, "Download all", and the AAB→APK conversion, so
    // whatever is transferring, its card shows the live bar.
    let downloads = use_signal(HashMap::<String, (u64, Option<u64>)>::new);
    // Shared status line for the header actions (stop / re-run).
    let mut action_status = use_signal(|| Option::<String>::None);
    let mut rerunning = use_signal(|| false);

    // Re-fetches whenever the selected build id changes.
    let mut detail = use_resource(move || {
        let client = state.client();
        let id = selected.read().clone();
        async move {
            match id {
                Some(id) => Some(client.get_build(&id).await),
                None => None,
            }
        }
    });

    // Last successful load and last error, each tagged with the build id they
    // belong to. Rendering from this cache means a poll refresh swaps in new
    // data without blanking the pane, while a *new* selection correctly falls
    // back to the loading state (the tag won't match).
    let mut cached = use_signal(|| Option::<(String, BuildDetailResponse)>::None);
    let mut failed = use_signal(|| Option::<(String, String)>::None);

    use_effect(move || {
        // Extract owned data and drop every read guard *before* writing back:
        // holding one across a signal write deadlocks the reactive flush.
        // (`anyhow::Error` isn't Clone, so the error is flattened to a String.)
        let outcome = match &*detail.read() {
            Some(Some(Ok(resp))) => Some(Ok(resp.clone())),
            Some(Some(Err(e))) => Some(Err(e.to_string())),
            _ => None,
        };
        let sel = selected.read().clone();

        match (outcome, sel) {
            (Some(Ok(resp)), _) => {
                cached.set(Some((resp.build.id.clone(), resp)));
                failed.set(None);
            }
            (Some(Err(msg)), Some(id)) => failed.set(Some((id, msg))),
            _ => {}
        }
    });

    // Poll while the selected build is still in a non-terminal state, so a
    // running build's status, steps and artifacts update live.
    use_future(move || async move {
        loop {
            tokio::time::sleep(Duration::from_secs(POLL_SECS)).await;
            let should_poll = {
                let sel = selected.read().clone();
                // `None` means a fetch is still in flight; restarting would
                // cancel it, and a build slower than POLL_SECS would then never
                // finish loading at all.
                let settled = detail.read().is_some();
                let c = cached.read();
                match (sel, c.as_ref()) {
                    // Only poll the build actually on screen: the cache may
                    // still hold a previously viewed (possibly running) build,
                    // and polling that one would keep cancelling this one.
                    // `is_running` rather than `is_cancellable` so an
                    // unrecognised status doesn't poll forever.
                    (Some(sel_id), Some((id, r))) => {
                        *id == sel_id && settled && is_running(&r.build.status)
                    }
                    _ => false,
                }
            };
            if should_poll {
                detail.restart();
            }
        }
    });

    // Heartbeat for live elapsed times: bumps once a second, but only while
    // the build on screen is still running, so finished builds cost nothing.
    let mut tick = use_signal(|| 0u64);
    use_future(move || async move {
        loop {
            tokio::time::sleep(Duration::from_secs(1)).await;
            let running = cached
                .peek()
                .as_ref()
                .is_some_and(|(_, r)| is_running(&r.build.status));
            if running {
                tick += 1;
            }
        }
    });

    let Some(sel_id) = selected.read().clone() else {
        return rsx! {
            div { class: "detail-empty muted",
                p { "Select a build to see its details, logs, and artifacts." }
            }
        };
    };

    // Snapshot into an owned value: holding a `read()` guard across `rsx!`
    // keeps it borrowed through the reactive flush, and the 5s poll below
    // writes this same signal — that combination deadlocks the app.
    let view: Option<(String, BuildDetailResponse)> = cached.read().clone();
    let resp = match view.as_ref() {
        Some((id, resp)) if *id == sel_id => resp,
        // Nothing cached for this selection yet — show its error, or loading.
        _ => {
            let msg = failed
                .read()
                .as_ref()
                .and_then(|(id, m)| (*id == sel_id).then(|| m.clone()));
            return match msg {
                Some(msg) => rsx! {
                    div { class: "detail-center",
                        div { class: "error-box",
                            p { "Couldn't load this build." }
                            p { class: "muted", "{msg}" }
                        }
                    }
                },
                None => rsx! {
                    div { class: "detail-center", p { class: "muted center", "Loading build…" } }
                },
            };
        }
    };

    let build = &resp.build;
    let app_name = resp.application.name.clone();

    let number = build
        .display_build_number()
        .map(|n| format!("#{n}"))
        .unwrap_or_default();
    // Subscribes to the heartbeat, so the elapsed times below tick each
    // second while the build runs.
    let _ = tick();
    // A running build shows a live elapsed time instead of a blank.
    let duration = fmt_duration(
        build.started_at,
        build
            .finished_at
            .or_else(|| is_running(&build.status).then(Utc::now)),
    );
    let has_downloads = build.artefacts.iter().any(|a| a.url.is_some());
    let cancellable = is_cancellable(&build.status);
    let build_id = build.id.clone();
    // `None` for tag-triggered builds, which carry no branch to re-run against.
    let rerun = build
        .rerun_target()
        .map(|(w, b)| (build.app_id.clone(), w.to_string(), b.to_string()));
    let rerun_busy = rerunning();
    // Owned copies: a live read guard inside `rsx!` can deadlock against a
    // concurrent signal write (the async download/cancel tasks write these).
    let action_msg: Option<String> = action_status.read().clone();
    let dl_msg: Option<String> = dl_status.read().clone();

    rsx! {
        div { class: "detail-body",
            // ── Center: build info + step logs ──────────────────────
            div { class: "detail-center wash-{status_class(&build.status)}",
                div { class: "detail-head", onmousedown: move |_| crate::start_drag(),
                    div { class: "detail-head-main",
                        h2 { "{app_name}" }
                        p { class: "muted", "{build.workflow_display()}  ·  {build.git_ref()}  {number}" }
                    }
                    span { class: "status hero {status_class(&build.status)}",
                        "{build.status}"
                        if let Some(d) = duration.clone() {
                            span { class: "status-dur", "  ·  {d}" }
                        }
                    }
                    div { class: "detail-actions", onmousedown: move |e| e.stop_propagation(),
                    {
                        let url = gantry_core::web::build_url(&build.app_id, &build.id);
                        rsx! {
                            button {
                                class: "ghost icon-btn",
                                title: "Open in Codemagic",
                                onclick: move |_| gantry_core::web::open_in_browser(&url),
                                ExternalLinkIcon {}
                            }
                        }
                    }
                    if let Some((app_id, workflow_id, branch)) = rerun.clone() {
                        {
                            let client = state.client();
                            rsx! {
                                button {
                                    class: "ghost icon-btn",
                                    disabled: rerun_busy,
                                    title: "Re-run this workflow on {branch}",
                                    onclick: move |_| {
                                        let client = client.clone();
                                        let (app_id, workflow_id, branch) =
                                            (app_id.clone(), workflow_id.clone(), branch.clone());
                                        rerunning.set(true);
                                        action_status.set(Some(format!("Starting a new build on {branch}…")));
                                        spawn(async move {
                                            let started = client
                                                .start_build(&app_id, &workflow_id, &branch)
                                                .await;
                                            rerunning.set(false);
                                            match started {
                                                // Hands the new id back to the list, which
                                                // refreshes and selects it — same path the
                                                // "New build" wizard takes.
                                                Ok(new_id) => {
                                                    action_status.set(None);
                                                    on_started.call(new_id);
                                                }
                                                Err(e) => action_status
                                                    .set(Some(format!("Couldn't start build: {e}"))),
                                            }
                                        });
                                    },
                                    RerunIcon {}
                                }
                            }
                        }
                    }
                    if cancellable {
                        {
                            let client = state.client();
                            rsx! {
                                button {
                                    class: "danger small",
                                    onclick: move |_| {
                                        let client = client.clone();
                                        let build_id = build_id.clone();
                                        action_status.set(Some("Stopping build…".into()));
                                        spawn(async move {
                                            match client.cancel_build(&build_id).await {
                                                Ok(()) => {
                                                    action_status.set(Some("Build stop requested.".into()));
                                                    detail.restart();
                                                }
                                                Err(e) => action_status.set(Some(format!("Couldn't stop build: {e}"))),
                                            }
                                        });
                                    },
                                    StopIcon {}
                                    span { "Stop build" }
                                }
                            }
                        }
                    }
                    }
                }
                if let Some(msg) = action_msg.as_ref() {
                    p { class: "dl-status", "{msg}" }
                }

                dl { class: "meta",
                    MetaItem { label: "Version", value: build.version.clone().unwrap_or_else(|| "-".into()) }
                    MetaItem { label: "Started", value: fmt_time(build.display_time()) }
                    MetaItem { label: "Finished", value: fmt_time(build.finished_at) }
                    MetaItem { label: "Duration", value: duration.unwrap_or_else(|| "-".into()) }
                    MetaItem {
                        label: "Queued",
                        value: fmt_duration(build.created_at, build.started_at)
                            .unwrap_or_else(|| "-".into()),
                    }
                    MetaItem { label: "Artifacts", value: artifacts_summary(&build.artefacts) }
                }

                if let Some(commit) = build.commit.clone() {
                    if commit.message.is_some() || commit.author.is_some() {
                        CommitCard { commit }
                    }
                }

                h3 { "Steps" }
                if build.build_actions.is_empty() {
                    p { class: "muted", "No steps recorded for this build." }
                } else {
                    {
                        // Bars are scaled against the slowest step, so the
                        // dominant one reads full-width and the rest show
                        // their cost relative to it.
                        let max_secs = build
                            .build_actions
                            .iter()
                            .filter_map(|a| duration_secs(a.started_at, a.finished_at))
                            .max()
                            .unwrap_or(0);
                        // The first failed step is why anyone opens a broken
                        // build, so it arrives already expanded.
                        let first_fail = build.build_actions.iter().position(|a| {
                            a.status
                                .as_deref()
                                .is_some_and(|s| gantry_core::status::outcome(s) == Some(false))
                        });
                        rsx! {
                            ul { class: "step-list",
                                for (i, action) in build.build_actions.iter().enumerate() {
                                    StepAccordion {
                                        // Keyed by build too: switching builds must
                                        // remount the accordions, resetting expansion
                                        // and letting auto-open apply to this build.
                                        key: "{build.id}-{i}-{action.name}",
                                        auto_open: first_fail == Some(i),
                                        idx: i,
                                        name: action.name.clone(),
                                        status: action.status.clone().unwrap_or_default(),
                                        // A running step ticks its elapsed time live.
                                        duration: fmt_duration(
                                            action.started_at,
                                            action.finished_at.or_else(|| {
                                                action
                                                    .status
                                                    .as_deref()
                                                    .is_some_and(is_running)
                                                    .then(Utc::now)
                                            }),
                                        ),
                                        frac: duration_secs(action.started_at, action.finished_at)
                                            .map(|s| s as f64 / max_secs.max(1) as f64),
                                        log_url: action.log_url.clone(),
                                    }
                                }
                            }
                        }
                    }
                }
            }

            // ── Right rail: artifacts ───────────────────────────────
            aside { class: "artifacts-rail",
                div { class: "rail-head", onmousedown: move |_| crate::start_drag(),
                    h3 { "Artifacts" }
                    if has_downloads {
                        {
                            let arts = build.artefacts.clone();
                            let client = state.client();
                            rsx! {
                                button {
                                    class: "ghost small",
                                    // Otherwise the click starts a window drag.
                                    onmousedown: move |e| e.stop_propagation(),
                                    onclick: move |_| {
                                        let arts = arts.clone();
                                        let client = client.clone();
                                        spawn(async move {
                                            let Some(folder) = rfd::AsyncFileDialog::new().pick_folder().await else {
                                                return;
                                            };
                                            let dir = folder.path().to_path_buf();
                                            dl_status.set(Some("Preparing downloads…".into()));
                                            let targets: Vec<Artefact> = arts.iter().filter(|a| a.url.is_some()).cloned().collect();
                                            let total = targets.len();
                                            let mut done = 0;
                                            for a in targets {
                                                dl_status.set(Some(format!("Downloading {} ({}/{})…", a.display_name(), done + 1, total)));
                                                let dest = dir.join(sanitize(a.display_name()));
                                                if download_tracked(client.clone(), a, dest, downloads).await.is_ok() {
                                                    done += 1;
                                                }
                                            }
                                            dl_status.set(Some(format!("Saved {done}/{total} to {}", dir.display())));
                                        });
                                    },
                                    "Download all"
                                }
                            }
                        }
                    }
                }

                if build.artefacts.is_empty() {
                    p { class: "muted rail-empty", "This build produced no artifacts." }
                } else {
                    ul { class: "artifact-list",
                        for art in build.artefacts.iter() {
                            ArtifactCard { key: "{art.display_name()}", art: art.clone(), dl_status, downloads }
                        }
                    }
                }

                if let Some(msg) = dl_msg.as_ref() {
                    p { class: "dl-status", "{msg}" }
                }
            }
        }
    }
}

// ─── Step accordion ──────────────────────────────────────────────────────────

#[derive(Clone, PartialEq)]
enum LogState {
    Idle,
    Loading,
    /// Already parsed out of the HTML the API serves.
    Loaded(Vec<gantry_core::log::Line>),
    Failed(String),
}

#[component]
fn StepAccordion(
    idx: usize,
    name: String,
    status: String,
    duration: Option<String>,
    /// This step's duration as a share of the slowest step's, for the bar.
    frac: Option<f64>,
    /// Start expanded (the build's first failed step), log already loading.
    auto_open: bool,
    log_url: Option<String>,
) -> Element {
    let state = use_context::<AppState>();
    let mut expanded = use_signal(|| auto_open);
    let mut log = use_signal(|| LogState::Idle);
    let mut query = use_signal(String::new);
    let mut wrap = use_signal(|| true);
    let mut save_msg = use_signal(|| Option::<String>::None);

    // Stable per-step id so the toolbar can scroll and read back the <pre>.
    // Only one build's steps are on screen at a time, so the index suffices.
    let log_id = format!("steplog-{idx}");

    // Kicks off the log fetch, once; shared by the click and the auto-open.
    let load = {
        let log_url = log_url.clone();
        move || {
            if *log.peek() != LogState::Idle {
                return;
            }
            match log_url.clone() {
                None => log.set(LogState::Failed("This step has no log.".into())),
                Some(url) => {
                    log.set(LogState::Loading);
                    let client = state.client();
                    spawn(async move {
                        match client.fetch_log(&url).await {
                            Ok(t) => log.set(LogState::Loaded(gantry_core::log::parse(&t))),
                            Err(e) => log.set(LogState::Failed(e.to_string())),
                        }
                    });
                }
            }
        }
    };

    // A step that mounts open needs its log without waiting for a click.
    use_hook({
        let mut load = load.clone();
        move || {
            if auto_open {
                load();
            }
        }
    });

    let toggle = {
        let mut load = load.clone();
        move |_| {
            let now = !expanded();
            expanded.set(now);
            if now {
                load();
            }
        }
    };

    // Owned copies: these are written by spawned tasks and by the input
    // handlers, so holding a read guard across `rsx!` risks deadlocking the
    // reactive flush.
    let log_state = log.read().clone();
    let q = query.read().clone();
    let wrapped = wrap();
    let saved = save_msg.read().clone();

    // Scrolls the <pre> itself (not the accordion) so the toolbar stays put.
    let jump = {
        let log_id = log_id.clone();
        move |to_bottom: bool| {
            let target = if to_bottom { "el.scrollHeight" } else { "0" };
            document::eval(&format!(
                "const el = document.getElementById('{log_id}'); if (el) el.scrollTop = {target};"
            ));
        }
    };

    rsx! {
        li { class: if expanded() { "step-row open" } else { "step-row" },
            div { class: "step-head", onclick: toggle,
                ChevronIcon {}
                // A passing step is the norm, so it gets a dot; anything else
                // keeps the pill and stands out from the column.
                if gantry_core::status::outcome(&status) == Some(true) {
                    span { class: "step-dot", title: "{status}" }
                } else {
                    span { class: "status small {status_class(&status)}", "{status}" }
                }
                span { class: "step-name", "{name}" }
                if let Some(frac) = frac {
                    {
                        // A 0s step still gets a sliver, so the track never
                        // looks broken or empty.
                        let pct = (frac * 100.0).max(2.5);
                        rsx! {
                            span { class: "step-track",
                                span {
                                    class: "step-bar {status_class(&status)}",
                                    style: "width: {pct:.1}%",
                                }
                            }
                        }
                    }
                } else if is_running(&status) {
                    // No duration yet — the step in progress sweeps instead.
                    span { class: "step-track",
                        span { class: "step-bar run indet" }
                    }
                }
                span { class: "step-dur muted", { duration.clone().unwrap_or_default() } }
            }
            if expanded() {
                div { class: "step-log",
                    match &log_state {
                        LogState::Idle | LogState::Loading => rsx! { p { class: "muted log-note", "Loading log…" } },
                        LogState::Failed(e) => rsx! { p { class: "error log-note", "{e}" } },
                        LogState::Loaded(lines) => {
                            let shown = filter_log(lines, &q);
                            let jump_up = jump.clone();
                            let jump_down = jump.clone();
                            let copy_id = log_id.clone();
                            let save_text = shown.text();
                            let save_name = format!("{}.log", sanitize(&name));
                            rsx! {
                                div { class: "log-bar",
                                    input {
                                        class: "log-search",
                                        r#type: "search",
                                        placeholder: "Filter lines…",
                                        value: "{q}",
                                        oninput: move |e| query.set(e.value()),
                                    }
                                    span { class: "muted log-count",
                                        if !q.is_empty() {
                                            {match shown.matches {
                                                0 => "no matches".to_string(),
                                                1 => "1 line".to_string(),
                                                n => format!("{n} lines"),
                                            }}
                                        }
                                    }
                                    button {
                                        class: if wrapped { "ghost icon-btn on" } else { "ghost icon-btn" },
                                        title: "Toggle word wrap",
                                        onclick: move |_| wrap.toggle(),
                                        WrapIcon {}
                                    }
                                    button {
                                        class: "ghost icon-btn",
                                        title: "Jump to top",
                                        onclick: move |_| jump_up(false),
                                        JumpIcon { up: true }
                                    }
                                    button {
                                        class: "ghost icon-btn",
                                        title: "Jump to bottom",
                                        onclick: move |_| jump_down(true),
                                        JumpIcon { up: false }
                                    }
                                    button {
                                        class: "ghost icon-btn",
                                        title: "Copy log",
                                        onclick: move |_| {
                                            // Copies what's on screen, so a
                                            // filtered view copies just those lines.
                                            crate::clipboard::copy_element(&copy_id);
                                            save_msg.set(Some("Copied".into()));
                                        },
                                        CopyIcon {}
                                    }
                                    button {
                                        class: "ghost icon-btn",
                                        title: "Save log to a file",
                                        onclick: move |_| {
                                            let text = save_text.clone();
                                            let file_name = save_name.clone();
                                            spawn(async move {
                                                let Some(handle) = rfd::AsyncFileDialog::new()
                                                    .set_file_name(&file_name)
                                                    .save_file()
                                                    .await
                                                else {
                                                    return;
                                                };
                                                let dest = handle.path().to_path_buf();
                                                match std::fs::write(&dest, text) {
                                                    Ok(()) => save_msg.set(Some(format!("Saved to {}", dest.display()))),
                                                    Err(e) => save_msg.set(Some(format!("Couldn't save: {e}"))),
                                                }
                                            });
                                        },
                                        DownloadIcon {}
                                    }
                                }
                                if let Some(msg) = saved.as_ref() {
                                    p { class: "muted log-note log-saved", "{msg}" }
                                }
                                pre {
                                    id: "{log_id}",
                                    class: if wrapped { "log" } else { "log nowrap" },
                                    for (i, line) in shown.lines.iter().enumerate() {
                                        {
                                            let line = line.clone();
                                            rsx! {
                                                for (j, seg) in line.segments.iter().enumerate() {
                                                    if let Some(color) = seg.color.as_ref() {
                                                        span { key: "{i}-{j}", style: "color: {color}", "{seg.text}" }
                                                    } else {
                                                        span { key: "{i}-{j}", "{seg.text}" }
                                                    }
                                                }
                                                // <pre> keeps newlines, so one per line.
                                                "\n"
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
}

/// The lines to display for `query`, plus how many matched.
///
/// An empty query shows everything (and reports no matches, since there is
/// nothing to count). Matching is case-insensitive substring against the
/// line's *text*, so a search can't accidentally hit the markup the colours
/// came from.
struct LogView {
    lines: Vec<gantry_core::log::Line>,
    matches: usize,
}

impl LogView {
    /// The visible lines as plain text, for saving to a file.
    fn text(&self) -> String {
        self.lines
            .iter()
            .map(gantry_core::log::Line::text)
            .collect::<Vec<_>>()
            .join("\n")
    }
}

fn filter_log(lines: &[gantry_core::log::Line], query: &str) -> LogView {
    if query.trim().is_empty() {
        return LogView {
            lines: lines.to_vec(),
            matches: 0,
        };
    }
    let needle = query.to_lowercase();
    let kept: Vec<gantry_core::log::Line> = lines
        .iter()
        .filter(|l| l.text().to_lowercase().contains(&needle))
        .cloned()
        .collect();
    LogView {
        matches: kept.len(),
        lines: kept,
    }
}

// ─── Artifact card ───────────────────────────────────────────────────────────

#[component]
fn ArtifactCard(
    art: Artefact,
    dl_status: Signal<Option<String>>,
    /// Shared per-artifact transfer progress, keyed by artifact name; this
    /// card renders the entry matching its own artifact, whoever wrote it
    /// (card click, "Download all", or an APK conversion's AAB download).
    downloads: Signal<HashMap<String, (u64, Option<u64>)>>,
) -> Element {
    let state = use_context::<AppState>();
    let name = art.display_name().to_string();
    let meta = format!("{}  ·  {}", art.display_type(), art.display_size());
    let has_url = art.url.is_some();
    let is_aab = art.is_aab();
    // True through the whole AAB→APK pipeline, not just its download phase.
    let mut converting = use_signal(|| false);
    let progress_now = downloads.read().get(&name).copied();
    let busy = progress_now.is_some() || converting();

    let (badge, family) = badge_of(&art);
    // While downloading, the type/size line becomes a live byte counter.
    let meta_line = match progress_now {
        Some((done, Some(total))) => format!("{} of {}", fmt_bytes(done), fmt_bytes(total)),
        Some((done, None)) => format!("{}…", fmt_bytes(done)),
        None => meta,
    };

    let art_dl = art.clone();
    let art_apk = art.clone();
    let name_dl = name.clone();
    let name_apk = name.clone();
    let client_dl = state.client();
    let client_apk = state.client();

    // The whole card is the download control.
    let download = move |_| {
        if !has_url || *converting.peek() || downloads.peek().contains_key(&name_dl) {
            return;
        }
        let art = art_dl.clone();
        let client = client_dl.clone();
        spawn(async move {
            let Some(handle) = rfd::AsyncFileDialog::new()
                .set_file_name(art.display_name())
                .save_file()
                .await
            else {
                return;
            };
            let dest = handle.path().to_path_buf();
            match download_tracked(client, art, dest, downloads).await {
                Ok(path) => dl_status.set(Some(format!("Saved to {}", path.display()))),
                Err(e) => dl_status.set(Some(format!("Failed: {e}"))),
            }
        });
    };

    rsx! {
        li {
            class: if busy { "artifact-card busy" } else if has_url { "artifact-card clickable" } else { "artifact-card" },
            title: if has_url && !busy { "Download {name}" },
            onclick: download,
            div { class: "artifact-top",
                span { class: "artifact-badge {family}", "{badge}" }
                div { class: "artifact-main",
                    span { class: "artifact-name", "{name}" }
                    span { class: "muted", "{meta_line}" }
                }
                span { class: "artifact-hint", DownloadIcon {} }
            }
            if let Some((done, total)) = progress_now {
                div { class: "artifact-progress",
                    if let Some(total) = total.filter(|t| *t > 0) {
                        div {
                            class: "fill",
                            style: "width: {(done as f64 / total as f64 * 100.0).min(100.0):.1}%",
                        }
                    } else {
                        div { class: "fill indet" }
                    }
                }
            }
            if is_aab {
                div { class: "artifact-actions",
                    button {
                        class: "ghost small",
                        disabled: !has_url || busy,
                        onclick: move |e| {
                            // The whole card downloads on click; converting
                            // must not also trigger that.
                            e.stop_propagation();
                            if *converting.peek() {
                                return;
                            }
                            let art = art_apk.clone();
                            let key = name_apk.clone();
                            let client = client_apk.clone();
                            spawn(async move {
                                let Some(handle) = rfd::AsyncFileDialog::new()
                                    .set_file_name(gantry_core::bundletool::suggested_apk_name(&art))
                                    .save_file()
                                    .await
                                else {
                                    return;
                                };
                                let dest = handle.path().to_path_buf();
                                converting.set(true);
                                let status = dl_status;
                                let mut map = downloads;
                                // Status messages only arrive outside the AAB
                                // transfer, so they clear the bar (post-download
                                // phases have no meaningful percentage) and the
                                // progress callback re-inserts it while bytes flow.
                                let msg_key = key.clone();
                                let bar_key = key.clone();
                                let mut last = 0u64;
                                let result = gantry_core::bundletool::convert_aab_to_apk(
                                    &client, &art, &dest,
                                    move |m| {
                                        let mut s = status;
                                        let mut m2 = map;
                                        m2.write().remove(&msg_key);
                                        s.set(Some(m));
                                    },
                                    move |done, total| {
                                        // First chunk draws the bar right away;
                                        // after that, an update per 256 KB.
                                        if last == 0 || done - last >= 256 * 1024 {
                                            last = done;
                                            let mut m2 = map;
                                            m2.write().insert(bar_key.clone(), (done, total));
                                        }
                                    },
                                ).await;
                                map.write().remove(&key);
                                converting.set(false);
                                match result {
                                    Ok(path) => dl_status.set(Some(format!("APK saved to {}", path.display()))),
                                    Err(e) => dl_status.set(Some(format!("Conversion failed: {e}"))),
                                }
                            });
                        },
                        "Convert to APK"
                    }
                }
            }
        }
    }
}

// ─── Commit card ─────────────────────────────────────────────────────────────

#[component]
fn CommitCard(commit: gantry_core::models::Commit) -> Element {
    let author = commit.author.clone().unwrap_or_default();
    let sha = commit
        .sha
        .as_deref()
        .map(|s| s[..s.len().min(8)].to_string())
        .unwrap_or_default();
    let message = commit.message.clone().unwrap_or_default();
    let mut lines = message.lines();
    let subject = lines.next().unwrap_or("").to_string();
    let body = lines.collect::<Vec<_>>().join("\n").trim().to_string();

    rsx! {
        div { class: "commit-card",
            span {
                class: "commit-avatar",
                style: "background: hsl({avatar_hue(&author)}, 45%, 48%)",
                "{initials(&author)}"
            }
            div { class: "commit-main",
                if !subject.is_empty() {
                    p { class: "commit-subject selectable", "{subject}" }
                }
                if !body.is_empty() {
                    pre { class: "commit-body selectable", "{body}" }
                }
                p { class: "commit-byline",
                    if !author.is_empty() {
                        span { "{author}" }
                    }
                    if !sha.is_empty() {
                        span { class: "commit-sha selectable", "{sha}" }
                    }
                }
            }
        }
    }
}

/// Up to two initials from the first and last words of a name, "?" when empty.
fn initials(name: &str) -> String {
    let mut words = name.split_whitespace();
    let first = words.next();
    let last = words.next_back();
    let letter = |w: Option<&str>| w.and_then(|w| w.chars().next());
    match (letter(first), letter(last)) {
        (Some(a), Some(b)) => format!("{}{}", a.to_uppercase(), b.to_uppercase()),
        (Some(a), None) => a.to_uppercase().to_string(),
        _ => "?".to_string(),
    }
}

/// Stable hue for a name, so each author keeps their avatar color.
fn avatar_hue(name: &str) -> u32 {
    name.bytes()
        .fold(0u32, |h, b| h.wrapping_mul(31).wrapping_add(u32::from(b)))
        % 360
}

/// Badge label and color family for an artifact's file-type glyph.
///
/// The API's type field is authoritative; the file extension is only a
/// fallback for types it doesn't set.
fn badge_of(art: &Artefact) -> (String, &'static str) {
    let kind = art.display_type().to_lowercase();
    let (label, family) = match kind.as_str() {
        "aab" => ("AAB", "android"),
        "apk" => ("APK", "android"),
        "ipa" => ("IPA", "apple"),
        "app" => ("APP", "apple"),
        "dsym" => ("SYM", "apple"),
        "proguard_map" => ("MAP", "doc"),
        "txt" => ("TXT", "doc"),
        "log" => ("LOG", "doc"),
        "zip" => ("ZIP", "arch"),
        _ => {
            let ext = art
                .display_name()
                .rsplit_once('.')
                .map(|(_, e)| e.to_uppercase())
                .filter(|e| !e.is_empty() && e.len() <= 4)
                .unwrap_or_else(|| "FILE".to_string());
            return (ext, "arch");
        }
    };
    (label.to_string(), family)
}

/// "3 · 253.4 MB" for the artifacts stat, "None" for an artifact-less build.
fn artifacts_summary(arts: &[Artefact]) -> String {
    if arts.is_empty() {
        return "None".to_string();
    }
    let total: u64 = arts.iter().filter_map(|a| a.size).sum();
    if total == 0 {
        return arts.len().to_string();
    }
    format!("{} · {}", arts.len(), fmt_bytes(total))
}

fn fmt_bytes(bytes: u64) -> String {
    const MB: f64 = 1_024.0 * 1_024.0;
    match bytes {
        b if (b as f64) < 1_024.0 => format!("{b} B"),
        b if (b as f64) < MB => format!("{:.1} KB", b as f64 / 1_024.0),
        b if (b as f64) < MB * 1_024.0 => format!("{:.1} MB", b as f64 / MB),
        b => format!("{:.2} GB", b as f64 / (MB * 1_024.0)),
    }
}

#[component]
fn MetaItem(label: String, value: String) -> Element {
    rsx! {
        div { class: "meta-item",
            dt { "{label}" }
            dd { "{value}" }
        }
    }
}

// ─── Download helpers ────────────────────────────────────────────────────────

/// Turns an authenticated artifact URL into a public one, then streams it to
/// `dest` (creating parent directories as needed), mirroring progress into
/// `downloads` so the artifact's card shows a live bar for the duration.
/// Returns the path written.
async fn download_tracked(
    client: ApiClient,
    art: Artefact,
    dest: PathBuf,
    mut downloads: Signal<HashMap<String, (u64, Option<u64>)>>,
) -> anyhow::Result<PathBuf> {
    let name = art.display_name().to_string();
    downloads.write().insert(name.clone(), (0, None));
    let result = async {
        let url = art
            .url
            .clone()
            .ok_or_else(|| anyhow::anyhow!("Artifact has no download URL"))?;
        let public_url = client.create_artifact_public_url(&url).await?;
        if let Some(parent) = dest.parent() {
            std::fs::create_dir_all(parent)?;
        }
        // A re-render per chunk would be thousands of updates for a big
        // artifact, so progress lands in the map every 256 KB.
        let mut last = 0u64;
        let key = name.clone();
        client
            .download_file_progress(&public_url, &dest, move |done, total| {
                if done - last >= 256 * 1024 {
                    last = done;
                    let mut m = downloads;
                    m.write().insert(key.clone(), (done, total));
                }
            })
            .await
    }
    .await;
    downloads.write().remove(&name);
    result.map(|()| dest)
}

fn sanitize(name: &str) -> String {
    name.chars()
        .map(|c| match c {
            '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|' => '_',
            c => c,
        })
        .collect()
}

// ─── Formatting ──────────────────────────────────────────────────────────────

use gantry_core::status::{is_cancellable, is_running};

fn fmt_time(t: Option<DateTime<Utc>>) -> String {
    t.map(|t| t.format("%Y-%m-%d %H:%M").to_string())
        .unwrap_or_else(|| "-".to_string())
}

fn duration_secs(start: Option<DateTime<Utc>>, end: Option<DateTime<Utc>>) -> Option<i64> {
    let secs = (end? - start?).num_seconds();
    (secs >= 0).then_some(secs)
}

fn fmt_duration(start: Option<DateTime<Utc>>, end: Option<DateTime<Utc>>) -> Option<String> {
    let (s, e) = (start?, end?);
    let secs = (e - s).num_seconds();
    if secs < 0 {
        return None;
    }
    Some(if secs >= 60 {
        format!("{}m {}s", secs / 60, secs % 60)
    } else {
        format!("{secs}s")
    })
}

#[cfg(test)]
mod tests {
    use super::filter_log;
    use gantry_core::log::parse;

    const LOG: &str = "Compiling foo\nerror: bad thing\nwarning: ERROR-ish\ndone";

    #[test]
    fn empty_query_shows_everything() {
        let v = filter_log(&parse(LOG), "  ");
        assert_eq!(v.text(), LOG);
        assert_eq!(v.matches, 0);
    }

    #[test]
    fn filters_case_insensitively() {
        let v = filter_log(&parse(LOG), "error");
        assert_eq!(v.matches, 2);
        assert_eq!(v.text(), "error: bad thing\nwarning: ERROR-ish");
    }

    #[test]
    fn no_matches_yields_no_lines() {
        let v = filter_log(&parse(LOG), "zzz");
        assert_eq!(v.matches, 0);
        assert_eq!(v.text(), "");
    }

    /// Searching must see the log text, never the markup the colours came
    /// from — otherwise "span" or "color" would match every command line.
    #[test]
    fn the_filter_cannot_match_stripped_markup() {
        let html = "<span style=\"color:#268BD2\">&gt; build</span>\nplain output";
        let lines = parse(html);
        assert_eq!(filter_log(&lines, "span").matches, 0);
        assert_eq!(filter_log(&lines, "color").matches, 0);
        assert_eq!(filter_log(&lines, "build").matches, 1);
    }
}
