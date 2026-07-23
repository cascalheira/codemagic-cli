//! AAB → universal APK conversion via Google's `bundletool`.
//!
//! This is inherently desktop-only: it shells out to `bundletool` (or
//! `java -jar`) and, if neither a `bundletool` binary nor a cached JAR is
//! present, downloads the latest JAR from GitHub. It is shared by the terminal
//! and GUI clients through `gantry-core`.
//!
//! Progress is reported through a `status` callback so each front end can
//! surface it however it likes (status bar, toast, etc.).

use std::path::{Path, PathBuf};

use anyhow::{Context, Result, anyhow, bail};

use crate::api::ApiClient;
use crate::models::Artefact;

/// How to invoke bundletool: a `bundletool` binary on `PATH`, or `java -jar`.
enum BundletoolCmd {
    Binary,
    Jar(PathBuf),
}

impl BundletoolCmd {
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

/// Downloads the given AAB artefact, converts it to a universal APK with
/// bundletool, and writes the result to `dest`. Returns `dest` on success.
///
/// `status` is invoked with human-readable progress messages; `on_download`
/// with `(bytes_so_far, total_if_known)` while the AAB itself downloads, so
/// a front end can draw a real progress bar for the transfer.
pub async fn convert_aab_to_apk(
    client: &ApiClient,
    aab: &Artefact,
    dest: &Path,
    mut status: impl FnMut(String),
    on_download: impl FnMut(u64, Option<u64>),
) -> Result<PathBuf> {
    // 1. Short-lived public URL for the AAB.
    status("Creating artifact download URL…".into());
    let aab_url = aab
        .url
        .as_deref()
        .ok_or_else(|| anyhow!("AAB artefact has no URL"))?;
    let public_url = client.create_artifact_public_url(aab_url).await?;

    // 2. Download the AAB to a temp workspace.
    let tmp = std::env::temp_dir().join("gantry");
    tokio::fs::create_dir_all(&tmp).await?;
    let aab_name = aab.name.as_deref().unwrap_or("app.aab");
    let stem = aab_name.trim_end_matches(".aab");
    let aab_path = tmp.join(aab_name);
    let apks_path = tmp.join(format!("{stem}.apks"));

    status(format!("Downloading {aab_name} ({})…", aab.display_size()));
    client
        .download_file_progress(&public_url, &aab_path, on_download)
        .await?;

    // 3. Locate (or fetch) bundletool.
    let bt = ensure_bundletool(&mut status).await?;
    let (prog, mut args) = bt.program_and_prefix();

    // 4. Build the universal APK set.
    status("Converting AAB → APK set…".into());
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
        .await
        .with_context(|| format!("Failed to run {prog}"))?;
    if !out.status.success() {
        let stderr = String::from_utf8_lossy(&out.stderr);
        let summary = stderr
            .lines()
            .find(|l| !l.trim().is_empty())
            .unwrap_or("(no output)")
            .trim()
            .to_string();
        bail!("bundletool failed: {summary}");
    }

    // 5. Extract universal.apk from the .apks ZIP (works on Windows, no `unzip`).
    status("Extracting universal APK…".into());
    let apks_path_clone = apks_path.clone();
    let dest_owned = dest.to_path_buf();
    let dest_for_task = dest_owned.clone();
    tokio::task::spawn_blocking(move || -> Result<()> {
        if let Some(parent) = dest_for_task.parent() {
            std::fs::create_dir_all(parent).context("Failed to create output directory")?;
        }
        let file = std::fs::File::open(&apks_path_clone).context("Failed to open .apks archive")?;
        let mut archive = zip::ZipArchive::new(file).context("Failed to read .apks archive")?;
        let mut zip_entry = archive
            .by_name("universal.apk")
            .context("universal.apk not found in .apks archive")?;
        let mut out_file =
            std::fs::File::create(&dest_for_task).context("Failed to create output APK")?;
        std::io::copy(&mut zip_entry, &mut out_file).context("Failed to extract universal.apk")?;
        Ok(())
    })
    .await
    .context("APK extraction task panicked")??;

    Ok(dest_owned)
}

/// Suggests a default APK filename for an AAB artefact (`foo.aab` → `foo.apk`).
pub fn suggested_apk_name(aab: &Artefact) -> String {
    let name = aab.name.as_deref().unwrap_or("app.aab");
    format!("{}.apk", name.trim_end_matches(".aab"))
}

// ─── bundletool resolution ───────────────────────────────────────────────────

/// Path to the cached bundletool JAR (shared with the CLI):
/// `~/.config/gantry/bundletool.jar`.
fn bundletool_jar_path() -> PathBuf {
    dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("gantry")
        .join("bundletool.jar")
}

/// Resolves how to run bundletool:
/// 1. `bundletool` binary on `PATH`.
/// 2. Cached JAR + `java`.
/// 3. Download the latest JAR from GitHub (needs `java`), cache it.
/// 4. Otherwise, error with install instructions.
async fn ensure_bundletool<F: FnMut(String)>(status: &mut F) -> Result<BundletoolCmd> {
    let binary_ok = tokio::process::Command::new("bundletool")
        .arg("version")
        .output()
        .await
        .map(|o| o.status.success())
        .unwrap_or(false);
    if binary_ok {
        return Ok(BundletoolCmd::Binary);
    }

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

    let jar_path = bundletool_jar_path();
    if jar_path.exists() {
        status("Using cached bundletool JAR…".into());
        return Ok(BundletoolCmd::Jar(jar_path));
    }

    status("bundletool not found — fetching latest release from GitHub…".into());
    let http = reqwest::Client::new();
    let jar_url = fetch_bundletool_jar_url(&http).await?;

    status("Downloading bundletool JAR (one-time)…".into());
    let bytes = http
        .get(&jar_url)
        .header("User-Agent", "gantry")
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

    status(format!(
        "bundletool JAR saved ({:.1} MB) — continuing…",
        bytes.len() as f64 / 1_048_576.0
    ));

    Ok(BundletoolCmd::Jar(jar_path))
}

/// Returns the `browser_download_url` for the JAR asset of bundletool's latest
/// GitHub release.
async fn fetch_bundletool_jar_url(http: &reqwest::Client) -> Result<String> {
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
        .header("User-Agent", "gantry")
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
