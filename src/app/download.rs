use super::*;

use std::path::PathBuf;

use anyhow::{Context, anyhow, bail};
use tokio::sync::mpsc;

use crate::api::ApiClient;
use crate::models::Artefact;

// ─── AAB → APK conversion (bundletool) ───────────────────────────────────────

/// How to invoke bundletool: as a system binary or via `java -jar`.
enum BundletoolCmd {
    /// `bundletool` is on PATH.
    Binary,
    /// `java -jar <path>` fallback with a (possibly auto-downloaded) JAR.
    Jar(PathBuf),
}

impl BundletoolCmd {
    /// Returns the program name and any leading arguments needed before the
    /// bundletool sub-command.
    fn program_and_prefix(&self) -> (String, Vec<String>) {
        match self {
            BundletoolCmd::Binary => ("bundletool".into(), vec![]),
            BundletoolCmd::Jar(jar) => (
                "java".into(),
                vec!["-jar".into(), jar.to_string_lossy().into_owned()],
            ),
        }
    }
}

pub(crate) async fn convert_aab_to_apk(
    client: ApiClient,
    artefact: Artefact,
    app_name: String,
    workflow_name: String,
    build_index: Option<u32>,
    tx: mpsc::Sender<AppMessage>,
) -> anyhow::Result<PathBuf> {
    macro_rules! status {
        ($msg:expr) => {
            tx.send(AppMessage::ApkStatus($msg.into())).await.ok();
        };
    }

    // 1. Create a short-lived public URL for the AAB.
    status!("Creating artifact download URL…");
    let aab_url = artefact
        .url
        .as_deref()
        .ok_or_else(|| anyhow!("AAB artefact has no URL"))?;
    let public_url = client.create_artifact_public_url(aab_url).await?;

    // 2. Download the AAB to a temp directory.
    let tmp = std::env::temp_dir().join("codemagic-cli");
    tokio::fs::create_dir_all(&tmp).await?;

    let aab_name = artefact.name.as_deref().unwrap_or("app.aab");
    let aab_path = tmp.join(aab_name);
    let stem = aab_name.trim_end_matches(".aab");
    let apks_path = tmp.join(format!("{stem}.apks"));

    status!(format!(
        "Downloading {} ({})...",
        aab_name,
        artefact.display_size()
    ));
    client.download_file(&public_url, &aab_path).await?;

    // 3. Locate or download bundletool.
    let bt = ensure_bundletool(&tx).await?;
    let (prog, mut args) = bt.program_and_prefix();

    // 4. Build the universal APK set.
    status!("Converting AAB → APK set…");
    args.extend([
        "build-apks".into(),
        "--bundle".into(),
        aab_path.to_string_lossy().into_owned(),
        "--output".into(),
        apks_path.to_string_lossy().into_owned(),
        "--mode=universal".into(),
        "--overwrite".into(),
    ]);

    let out = tokio::process::Command::new(&prog)
        .args(&args)
        .output()
        .await?;
    if !out.status.success() {
        let stderr = String::from_utf8_lossy(&out.stderr);
        bail!("bundletool failed: {stderr}");
    }

    // 5. Extract universal.apk from the .apks ZIP.
    status!("Extracting universal APK…");
    let extract = tokio::process::Command::new("unzip")
        .args([
            "-o",
            apks_path.to_str().unwrap_or(""),
            "universal.apk",
            "-d",
            tmp.to_str().unwrap_or("/tmp"),
        ])
        .output()
        .await?;
    if !extract.status.success() {
        let stderr = String::from_utf8_lossy(&extract.stderr);
        bail!("unzip failed: {stderr}");
    }

    // 6. Copy to the same structured path used for regular artifact downloads.
    let apk_name = format!("{stem}.apk");
    let apk_dest = artifact_download_path(&app_name, &workflow_name, build_index, &apk_name);
    if let Some(parent) = apk_dest.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }
    tokio::fs::copy(tmp.join("universal.apk"), &apk_dest).await?;

    Ok(apk_dest)
}

// ─── bundletool auto-download ─────────────────────────────────────────────────

/// Path to the cached bundletool JAR: `~/.config/codemagic-cli/bundletool.jar`.
fn bundletool_jar_path() -> PathBuf {
    dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("codemagic-cli")
        .join("bundletool.jar")
}

/// Returns the invocation strategy:
/// 1. `bundletool` binary on PATH  → use it directly.
/// 2. Cached JAR + `java` on PATH  → `java -jar <cached>`.
/// 3. No cached JAR but `java` available → download JAR from latest GitHub
///    release, cache it, then use `java -jar`.
/// 4. No `java` either → error with clear install instructions.
async fn ensure_bundletool(tx: &mpsc::Sender<AppMessage>) -> anyhow::Result<BundletoolCmd> {
    // 1. Binary on PATH?
    let binary_ok = tokio::process::Command::new("bundletool")
        .arg("version")
        .output()
        .await
        .map(|o| o.status.success())
        .unwrap_or(false);
    if binary_ok {
        return Ok(BundletoolCmd::Binary);
    }

    // 2. Java available?
    let java_ok = tokio::process::Command::new("java")
        .arg("-version")
        .output()
        .await
        .map(|o| o.status.success())
        .unwrap_or(false);
    if !java_ok {
        bail!(
            "bundletool not found and no Java runtime available.\n\
             Install one of:\n\
             • bundletool (Homebrew): brew install bundletool\n\
             • Java (JRE):            brew install openjdk"
        );
    }

    // 3. Cached JAR?
    let jar_path = bundletool_jar_path();
    if jar_path.exists() {
        tx.send(AppMessage::ApkStatus("Using cached bundletool JAR…".into()))
            .await
            .ok();
        return Ok(BundletoolCmd::Jar(jar_path));
    }

    // 4. Download latest JAR from GitHub releases.
    tx.send(AppMessage::ApkStatus(
        "bundletool not found — fetching latest release info from GitHub…".into(),
    ))
    .await
    .ok();

    let http = reqwest::Client::new();
    let jar_url = fetch_bundletool_jar_url(&http).await?;

    tx.send(AppMessage::ApkStatus(
        "Downloading bundletool JAR (this only happens once)…".into(),
    ))
    .await
    .ok();

    let bytes = http
        .get(&jar_url)
        .header("User-Agent", "codemagic-cli")
        .send()
        .await
        .context("Failed to download bundletool JAR")?
        .bytes()
        .await
        .context("Failed to read bundletool JAR response")?;

    if let Some(parent) = jar_path.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }
    tokio::fs::write(&jar_path, &bytes)
        .await
        .context("Failed to cache bundletool JAR")?;

    tx.send(AppMessage::ApkStatus(format!(
        "bundletool JAR saved ({:.1} MB) — continuing…",
        bytes.len() as f64 / 1_048_576.0
    )))
    .await
    .ok();

    Ok(BundletoolCmd::Jar(jar_path))
}

/// Hits the GitHub releases API and returns the `browser_download_url` for the
/// bundletool JAR asset of the latest release.
async fn fetch_bundletool_jar_url(http: &reqwest::Client) -> anyhow::Result<String> {
    #[derive(serde::Deserialize)]
    struct Asset {
        name: String,
        browser_download_url: String,
    }
    #[derive(serde::Deserialize)]
    struct Release {
        assets: Vec<Asset>,
    }

    let release: Release = http
        .get("https://api.github.com/repos/google/bundletool/releases/latest")
        .header("User-Agent", "codemagic-cli")
        .header("Accept", "application/vnd.github+json")
        .send()
        .await
        .context("Failed to fetch bundletool release info from GitHub")?
        .json()
        .await
        .context("Failed to parse bundletool release JSON")?;

    release
        .assets
        .into_iter()
        .find(|a| a.name.ends_with(".jar"))
        .map(|a| a.browser_download_url)
        .ok_or_else(|| anyhow!("No JAR asset found in bundletool latest release"))
}

// ─── Artifact direct download ───────────────────────────────────────────────────

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
