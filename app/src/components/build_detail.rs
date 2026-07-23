//! Center build-info column (metadata + expandable step logs) plus the
//! right-hand artifacts download rail.

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
    let duration = fmt_duration(build.started_at, build.finished_at);
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
            div { class: "detail-center",
                div { class: "detail-head", onmousedown: move |_| crate::start_drag(),
                    span { class: "status {status_class(&build.status)}", "{build.status}" }
                    div { class: "detail-head-main",
                        h2 { "{app_name}" }
                        p { class: "muted", "{build.workflow_display()}  ·  {build.git_ref()}  {number}" }
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
                    if let Some(commit) = &build.commit {
                        MetaItem {
                            label: "Commit",
                            value: format!(
                                "{}{}",
                                commit.sha.as_deref().map(|s| format!("{}  ", &s[..s.len().min(8)])).unwrap_or_default(),
                                commit.message.as_deref().unwrap_or("-").lines().next().unwrap_or("-"),
                            ),
                        }
                    }
                }

                h3 { "Steps" }
                if build.build_actions.is_empty() {
                    p { class: "muted", "No steps recorded for this build." }
                } else {
                    ul { class: "step-list",
                        for (i, action) in build.build_actions.iter().enumerate() {
                            StepAccordion {
                                key: "{i}-{action.name}",
                                idx: i,
                                name: action.name.clone(),
                                status: action.status.clone().unwrap_or_default(),
                                duration: fmt_duration(action.started_at, action.finished_at),
                                log_url: action.log_url.clone(),
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
                                                if download_to(client.clone(), a, dest).await.is_ok() {
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
                            ArtifactCard { key: "{art.display_name()}", art: art.clone(), dl_status }
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
    Loaded(String),
    Failed(String),
}

#[component]
fn StepAccordion(
    idx: usize,
    name: String,
    status: String,
    duration: Option<String>,
    log_url: Option<String>,
) -> Element {
    let state = use_context::<AppState>();
    let mut expanded = use_signal(|| false);
    let mut log = use_signal(|| LogState::Idle);
    let mut query = use_signal(String::new);
    let mut wrap = use_signal(|| true);
    let mut save_msg = use_signal(|| Option::<String>::None);

    // Stable per-step id so the toolbar can scroll and read back the <pre>.
    // Only one build's steps are on screen at a time, so the index suffices.
    let log_id = format!("steplog-{idx}");

    let toggle = move |_| {
        let now = !expanded();
        expanded.set(now);
        if now && *log.read() == LogState::Idle {
            match log_url.clone() {
                None => log.set(LogState::Failed("This step has no log.".into())),
                Some(url) => {
                    log.set(LogState::Loading);
                    let client = state.client();
                    spawn(async move {
                        match client.fetch_log(&url).await {
                            Ok(t) => log.set(LogState::Loaded(t)),
                            Err(e) => log.set(LogState::Failed(e.to_string())),
                        }
                    });
                }
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
                span { class: "status small {status_class(&status)}", "{status}" }
                span { class: "step-name", "{name}" }
                span { class: "step-dur muted", { duration.clone().unwrap_or_default() } }
            }
            if expanded() {
                div { class: "step-log",
                    match &log_state {
                        LogState::Idle | LogState::Loading => rsx! { p { class: "muted log-note", "Loading log…" } },
                        LogState::Failed(e) => rsx! { p { class: "error log-note", "{e}" } },
                        LogState::Loaded(t) => {
                            let shown = filter_log(t, &q);
                            let jump_up = jump.clone();
                            let jump_down = jump.clone();
                            let copy_id = log_id.clone();
                            let save_text = shown.text.clone();
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
                                    "{shown.text}"
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

/// The lines of `text` to display for `query`, plus how many matched.
///
/// An empty query shows everything (and reports no matches, since there is
/// nothing to count). Matching is case-insensitive substring, like the TUI's
/// log search.
struct LogView {
    text: String,
    matches: usize,
}

fn filter_log(text: &str, query: &str) -> LogView {
    if query.trim().is_empty() {
        return LogView {
            text: text.to_string(),
            matches: 0,
        };
    }
    let needle = query.to_lowercase();
    let kept: Vec<&str> = text
        .lines()
        .filter(|l| l.to_lowercase().contains(&needle))
        .collect();
    LogView {
        matches: kept.len(),
        text: kept.join("\n"),
    }
}

// ─── Artifact card ───────────────────────────────────────────────────────────

#[component]
fn ArtifactCard(art: Artefact, dl_status: Signal<Option<String>>) -> Element {
    let state = use_context::<AppState>();
    let name = art.display_name().to_string();
    let meta = format!("{}  ·  {}", art.display_type(), art.display_size());
    let has_url = art.url.is_some();
    let is_aab = art.is_aab();

    let art_dl = art.clone();
    let art_apk = art.clone();
    let client_dl = state.client();
    let client_apk = state.client();

    rsx! {
        li { class: "artifact-card",
            div { class: "artifact-main",
                span { class: "artifact-name", "{name}" }
                span { class: "muted", "{meta}" }
            }
            div { class: "artifact-actions",
                button {
                    class: "ghost small",
                    disabled: !has_url,
                    onclick: move |_| {
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
                            dl_status.set(Some(format!("Downloading {}…", art.display_name())));
                            match download_to(client, art, dest).await {
                                Ok(path) => dl_status.set(Some(format!("Saved to {}", path.display()))),
                                Err(e) => dl_status.set(Some(format!("Failed: {e}"))),
                            }
                        });
                    },
                    DownloadIcon {}
                    span { "Download" }
                }
                if is_aab {
                    button {
                        class: "ghost small",
                        disabled: !has_url,
                        onclick: move |_| {
                            let art = art_apk.clone();
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
                                let status = dl_status;
                                let result = gantry_core::bundletool::convert_aab_to_apk(
                                    &client, &art, &dest,
                                    move |m| { let mut s = status; s.set(Some(m)); },
                                ).await;
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
/// `dest` (creating parent directories as needed). Returns the path written.
async fn download_to(client: ApiClient, art: Artefact, dest: PathBuf) -> anyhow::Result<PathBuf> {
    let url = art
        .url
        .clone()
        .ok_or_else(|| anyhow::anyhow!("Artifact has no download URL"))?;
    let public_url = client.create_artifact_public_url(&url).await?;
    if let Some(parent) = dest.parent() {
        std::fs::create_dir_all(parent)?;
    }
    client.download_file(&public_url, &dest).await?;
    Ok(dest)
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

    const LOG: &str = "Compiling foo\nerror: bad thing\nwarning: ERROR-ish\ndone";

    #[test]
    fn empty_query_shows_everything() {
        let v = filter_log(LOG, "  ");
        assert_eq!(v.text, LOG);
        assert_eq!(v.matches, 0);
    }

    #[test]
    fn filters_case_insensitively() {
        let v = filter_log(LOG, "error");
        assert_eq!(v.matches, 2);
        assert_eq!(v.text, "error: bad thing\nwarning: ERROR-ish");
    }

    #[test]
    fn no_matches_yields_empty_text() {
        let v = filter_log(LOG, "zzz");
        assert_eq!(v.matches, 0);
        assert_eq!(v.text, "");
    }
}
