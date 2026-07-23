//! Center build-info column (metadata + expandable step logs) plus the
//! right-hand artifacts download rail.

use std::path::PathBuf;

use chrono::{DateTime, Utc};
use codemagic_core::{ApiClient, models::Artefact};
use dioxus::prelude::*;

use super::builds_screen::status_class;
use super::icons::{ChevronIcon, DownloadIcon, StopIcon};
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

    if selected.read().is_none() {
        return rsx! {
            div { class: "detail-empty muted",
                p { "Select a build to see its details, logs, and artifacts." }
            }
        };
    }

    let view = detail.read();
    let build = match &*view {
        None | Some(None) => {
            return rsx! { div { class: "detail-center", p { class: "muted center", "Loading build…" } } };
        }
        Some(Some(Err(e))) => {
            return rsx! {
                div { class: "detail-center",
                    div { class: "error-box",
                        p { "Couldn't load this build." }
                        p { class: "muted", "{e}" }
                    }
                }
            };
        }
        Some(Some(Ok(resp))) => &resp.build,
    };

    let app_name = match &*view {
        Some(Some(Ok(resp))) => resp.application.name.clone(),
        _ => "Unknown app".to_string(),
    };

    let number = build
        .display_build_number()
        .map(|n| format!("#{n}"))
        .unwrap_or_default();
    let duration = fmt_duration(build.started_at, build.finished_at);
    let has_downloads = build.artefacts.iter().any(|a| a.url.is_some());
    let cancellable = is_cancellable(&build.status);
    let build_id = build.id.clone();

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
                if let Some(msg) = &*cancel_status.read() {
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

                if let Some(msg) = &*dl_status.read() {
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
                    match &*log.read() {
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

/// Whether a build in this status can still be cancelled (i.e. it hasn't
/// reached a terminal state).
fn is_cancellable(status: &str) -> bool {
    !matches!(
        status,
        "finished" | "failed" | "canceled" | "timeout" | "skipped"
    )
}

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
