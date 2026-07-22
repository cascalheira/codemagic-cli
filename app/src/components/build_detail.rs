//! Right-hand detail pane: metadata, per-step logs, and artifact downloads for
//! the selected build.

use std::path::PathBuf;

use chrono::{DateTime, Utc};
use codemagic_core::{ApiClient, models::Artefact};
use dioxus::prelude::*;

use super::builds_screen::status_class;
use crate::state::AppState;

/// State of the log viewer overlay.
#[derive(Clone, PartialEq)]
enum LogModal {
    Closed,
    Loading(String),
    Loaded { title: String, text: String },
    Failed { title: String, error: String },
}

#[component]
pub fn BuildDetail(selected: Signal<Option<String>>) -> Element {
    let state = use_context::<AppState>();
    let mut log_modal = use_signal(|| LogModal::Closed);
    let mut dl_status = use_signal(|| Option::<String>::None);

    // Re-fetches whenever the selected build id changes.
    let detail = use_resource(move || {
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
            return rsx! { p { class: "muted center", "Loading build…" } };
        }
        Some(Some(Err(e))) => {
            return rsx! {
                div { class: "error-box",
                    p { "Couldn't load this build." }
                    p { class: "muted", "{e}" }
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

    rsx! {
        div { class: "detail",
            // ── Header ──────────────────────────────────────────────
            div { class: "detail-head",
                span { class: "status {status_class(&build.status)}", "{build.status}" }
                div {
                    h2 { "{app_name}" }
                    p { class: "muted", "{build.workflow_display()}  ·  {build.git_ref()}  {number}" }
                }
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

            // ── Steps / logs ────────────────────────────────────────
            h3 { "Steps" }
            if build.build_actions.is_empty() {
                p { class: "muted", "No steps recorded for this build." }
            } else {
                ul { class: "step-list",
                    for action in build.build_actions.iter() {
                        {
                            let step_name = action.name.clone();
                            let log_url = action.log_url.clone();
                            let step_dur = fmt_duration(action.started_at, action.finished_at);
                            let status = action.status.clone().unwrap_or_default();
                            let client = state.client();
                            rsx! {
                                li { class: "step-row",
                                    span { class: "status small {status_class(&status)}", "{status}" }
                                    span { class: "step-name", "{step_name}" }
                                    span { class: "muted step-dur", { step_dur.unwrap_or_default() } }
                                    button {
                                        class: "ghost small",
                                        disabled: log_url.is_none(),
                                        onclick: move |_| {
                                            let title = step_name.clone();
                                            let Some(url) = log_url.clone() else { return; };
                                            log_modal.set(LogModal::Loading(title.clone()));
                                            let client = client.clone();
                                            spawn(async move {
                                                match client.fetch_log(&url).await {
                                                    Ok(text) => log_modal.set(LogModal::Loaded { title, text }),
                                                    Err(e) => log_modal.set(LogModal::Failed { title, error: e.to_string() }),
                                                }
                                            });
                                        },
                                        "View log"
                                    }
                                }
                            }
                        }
                    }
                }
            }

            // ── Artifacts ───────────────────────────────────────────
            div { class: "section-head",
                h3 { "Artifacts" }
                if has_downloads {
                    {
                        let arts = build.artefacts.clone();
                        let client = state.client();
                        rsx! {
                            button {
                                class: "primary small",
                                onclick: move |_| {
                                    let arts = arts.clone();
                                    let client = client.clone();
                                    spawn(async move {
                                        let Some(folder) = rfd::AsyncFileDialog::new().pick_folder().await else {
                                            return; // cancelled
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
                                        dl_status.set(Some(format!("Saved {done}/{total} files to {}", dir.display())));
                                    });
                                },
                                "Download all"
                            }
                        }
                    }
                }
            }
            if build.artefacts.is_empty() {
                p { class: "muted", "This build produced no artifacts." }
            } else {
                ul { class: "artifact-list",
                    for art in build.artefacts.iter() {
                        {
                            let art = art.clone();
                            let client = state.client();
                            let name = art.display_name().to_string();
                            let meta = format!("{}  ·  {}", art.display_type(), art.display_size());
                            let has_url = art.url.is_some();
                            rsx! {
                                li { class: "artifact-row",
                                    div { class: "artifact-main",
                                        span { class: "artifact-name", "{name}" }
                                        span { class: "muted", "{meta}" }
                                    }
                                    button {
                                        class: "ghost small",
                                        disabled: !has_url,
                                        onclick: move |_| {
                                            let art = art.clone();
                                            let client = client.clone();
                                            spawn(async move {
                                                let Some(handle) = rfd::AsyncFileDialog::new()
                                                    .set_file_name(art.display_name())
                                                    .save_file()
                                                    .await
                                                else {
                                                    return; // cancelled
                                                };
                                                let dest = handle.path().to_path_buf();
                                                dl_status.set(Some(format!("Downloading {}…", art.display_name())));
                                                match download_to(client, art, dest).await {
                                                    Ok(path) => dl_status.set(Some(format!("Saved to {}", path.display()))),
                                                    Err(e) => dl_status.set(Some(format!("Failed: {e}"))),
                                                }
                                            });
                                        },
                                        "Download"
                                    }
                                }
                            }
                        }
                    }
                }
            }

            if let Some(msg) = &*dl_status.read() {
                p { class: "dl-status", "{msg}" }
            }
        }

        // ── Log overlay ─────────────────────────────────────────────
        {
            let modal = log_modal.read().clone();
            if modal != LogModal::Closed {
                rsx! {
                    div { class: "modal-overlay", onclick: move |_| log_modal.set(LogModal::Closed),
                        div { class: "modal", onclick: move |e| e.stop_propagation(),
                            match modal {
                                LogModal::Loading(title) => rsx! {
                                    div { class: "modal-head", h3 { "{title}" }
                                        button { class: "ghost small", onclick: move |_| log_modal.set(LogModal::Closed), "Close" } }
                                    p { class: "muted center", "Loading log…" }
                                },
                                LogModal::Loaded { title, text } => rsx! {
                                    div { class: "modal-head", h3 { "{title}" }
                                        button { class: "ghost small", onclick: move |_| log_modal.set(LogModal::Closed), "Close" } }
                                    pre { class: "log", "{text}" }
                                },
                                LogModal::Failed { title, error } => rsx! {
                                    div { class: "modal-head", h3 { "{title}" }
                                        button { class: "ghost small", onclick: move |_| log_modal.set(LogModal::Closed), "Close" } }
                                    p { class: "error", "{error}" }
                                },
                                LogModal::Closed => rsx! {},
                            }
                        }
                    }
                }
            } else {
                rsx! {}
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
