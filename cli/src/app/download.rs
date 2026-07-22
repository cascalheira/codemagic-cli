use super::*;

use std::path::PathBuf;

use anyhow::Context;
use tokio::sync::mpsc;

use crate::api::ApiClient;
use crate::models::Artefact;

// ─── AAB → APK conversion ─────────────────────────────────────────────────────

/// Converts an AAB artefact to a universal APK and saves it under the structured
/// download path `~/Codemagic/{app}/{workflow}/{build_index}/{name}.apk`.
///
/// The heavy lifting (bundletool resolution, download, conversion, extraction)
/// lives in `codemagic-core`; here we just choose the destination and forward
/// progress to the TUI as `ApkStatus` messages.
pub(crate) async fn convert_aab_to_apk(
    client: ApiClient,
    artefact: Artefact,
    app_name: String,
    workflow_name: String,
    build_index: Option<u32>,
    tx: mpsc::Sender<AppMessage>,
) -> anyhow::Result<PathBuf> {
    let stem = artefact
        .name
        .as_deref()
        .unwrap_or("app.aab")
        .trim_end_matches(".aab")
        .to_string();
    let apk_name = format!("{stem}.apk");
    let dest = artifact_download_path(&app_name, &workflow_name, build_index, &apk_name);

    codemagic_core::bundletool::convert_aab_to_apk(&client, &artefact, &dest, move |msg| {
        let _ = tx.try_send(AppMessage::ApkStatus(msg));
    })
    .await
}

// ─── Artifact direct download ─────────────────────────────────────────────────

/// Downloads a single build artefact into the structured local directory:
/// `~/Codemagic/{app_name}/{workflow_name}/{build_index}/{artifact_name}`
pub(crate) async fn download_artifact(
    client: ApiClient,
    artifact_url: String,
    app_name: String,
    workflow_name: String,
    build_index: Option<u32>,
    artifact_name: String,
) -> anyhow::Result<PathBuf> {
    // 1. Turn the private artifact URL into a 1-hour public download link.
    let public_url = client.create_artifact_public_url(&artifact_url).await?;

    // 2. Build the destination path.
    let dest = artifact_download_path(&app_name, &workflow_name, build_index, &artifact_name);

    // 3. Ensure the directory tree exists.
    if let Some(parent) = dest.parent() {
        tokio::fs::create_dir_all(parent)
            .await
            .context("Failed to create download directory")?;
    }

    // 4. Stream the file to disk.
    client.download_file(&public_url, &dest).await?;

    Ok(dest)
}

/// Returns the canonical local path for a build artefact.
///
/// `~/Codemagic/{app}/{workflow}/{build_index}/{filename}`
fn artifact_download_path(
    app_name: &str,
    workflow_name: &str,
    build_index: Option<u32>,
    artifact_name: &str,
) -> PathBuf {
    let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));
    let index = build_index
        .map(|i| i.to_string())
        .unwrap_or_else(|| "unknown".to_string());

    home.join("Codemagic")
        .join(sanitize_path_component(app_name))
        .join(sanitize_path_component(workflow_name))
        .join(sanitize_path_component(&index))
        .join(sanitize_path_component(artifact_name))
}

/// Replaces characters that are illegal in file/directory names on common
/// operating systems with an underscore.
fn sanitize_path_component(s: &str) -> String {
    s.chars()
        .map(|c| match c {
            '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|' => '_',
            c => c,
        })
        .collect::<String>()
        .trim()
        .to_string()
}

// ─── Platform-specific browser open ──────────────────────────────────────────────

#[allow(dead_code)]
fn open_in_browser(url: &str) {
    #[cfg(target_os = "macos")]
    let _ = std::process::Command::new("open").arg(url).spawn();
    #[cfg(target_os = "linux")]
    let _ = std::process::Command::new("xdg-open").arg(url).spawn();
    #[cfg(target_os = "windows")]
    let _ = std::process::Command::new("cmd")
        .args(["/C", "start", url])
        .spawn();
}
