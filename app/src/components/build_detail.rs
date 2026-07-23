//! Center build-info column (metadata + expandable step logs) plus the
//! right-hand artifacts download rail.

use std::path::PathBuf;
use std::time::Duration;

use chrono::{DateTime, Utc};
use codemagic_core::{
    ApiClient,
    models::{Artefact, BuildDetailResponse},
};
use dioxus::prelude::*;

/// How often to re-poll a build that hasn't reached a terminal state. Polling
/// stops on its own once the build finishes.
const POLL_SECS: u64 = 5;

use super::builds_screen::status_class;
use super::icons::{ChevronIcon, DownloadIcon, ExternalLinkIcon, StopIcon};
use crate::state::AppState;

#[component]
pub fn BuildDetail(selected: Signal<Option<String>>) -> Element {
    let state = use_context::<AppState>();
    let mut dl_status = use_signal(|| Option::<String>::None);
    let mut cancel_status = use_signal(|| Option::<String>::None);

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
            let running = {
                let c = cached.read();
                c.as_ref()
                    .is_some_and(|(_, r)| is_cancellable(&r.build.status))
            };
            if running {
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
    // Owned copies: a live read guard inside `rsx!` can deadlock against a
    // concurrent signal write (the async download/cancel tasks write these).
    let cancel_msg: Option<String> = cancel_status.read().clone();
    let dl_msg: Option<String> = dl_status.read().clone();

    rsx! {
        div { class: "detail-body",
            // ── Center: build info + step logs ──────────────────────
            div { class: "detail-center",
                div { class: "detail-head",
                    span { class: "status {status_class(&build.status)}", "{build.status}" }
                    div { class: "detail-head-main",
                        h2 { "{app_name}" }
                        p { class: "muted", "{build.workflow_display()}  ·  {build.git_ref()}  {number}" }
                    }
                    {
                        let url = codemagic_core::web::build_url(&build.app_id, &build.id);
                        rsx! {
                            button {
                                class: "ghost icon-btn",
                                title: "Open in Codemagic",
                                onclick: move |_| codemagic_core::web::open_in_browser(&url),
                                ExternalLinkIcon {}
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
                                        cancel_status.set(Some("Stopping build…".into()));
                                        spawn(async move {
                                            match client.cancel_build(&build_id).await {
                                                Ok(()) => {
                                                    cancel_status.set(Some("Build stop requested.".into()));
                                                    detail.restart();
                                                }
                                                Err(e) => cancel_status.set(Some(format!("Couldn't stop build: {e}"))),
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
                if let Some(msg) = cancel_msg.as_ref() {
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
                div { class: "rail-head",
                    h3 { "Artifacts" }
                    if has_downloads {
                        {
                            let arts = build.artefacts.clone();
                            let client = state.client();
                            rsx! {
                                button {
                                    class: "ghost small",
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
    name: String,
    status: String,
    duration: Option<String>,
    log_url: Option<String>,
) -> Element {
    let state = use_context::<AppState>();
    let mut expanded = use_signal(|| false);
    let mut log = use_signal(|| LogState::Idle);

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

    // Owned copy: the log is written by a spawned task, so holding a read
    // guard across `rsx!` risks deadlocking the reactive flush.
    let log_state = log.read().clone();

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
                        LogState::Loaded(t) => rsx! { pre { class: "log", "{t}" } },
                        LogState::Failed(e) => rsx! { p { class: "error log-note", "{e}" } },
                    }
                }
            }
        }
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
                    class: "primary small",
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
                                    .set_file_name(codemagic_core::bundletool::suggested_apk_name(&art))
                                    .save_file()
                                    .await
                                else {
                                    return;
                                };
                                let dest = handle.path().to_path_buf();
                                let status = dl_status;
                                let result = codemagic_core::bundletool::convert_aab_to_apk(
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

use codemagic_core::status::is_cancellable;

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
