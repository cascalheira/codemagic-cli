//! Non-interactive CLI commands (run when the binary is invoked with subcommands).

use std::path::{Path, PathBuf};

use anyhow::{Context, Result, anyhow, bail};

use crate::api::{ApiClient, PAGE_SIZE};
use crate::config;
use crate::models::Artefact;

// ─── Public entry point ───────────────────────────────────────────────────────

/// `codemagic-cli download apk --app-id X --workflow-id Y`
///
/// Finds the latest finished build for the given app / workflow that contains
/// an AAB artefact, converts it to a universal APK with bundletool, and writes
/// the result to `~/Codemagic/{app}/{workflow}/last/build.apk`.
pub async fn run_download_apk(app_id: &str, workflow_id: &str) -> Result<()> {
    // 1. Load saved API token.
    let cfg = config::load_config()?.ok_or_else(|| {
        anyhow!(
            "No saved API token found.\n\
             Run `codemagic-cli` (no arguments) to open the TUI and complete setup."
        )
    })?;
    let client = ApiClient::new(cfg.api_token);

    // 2. Resolve human-readable names for the output path.
    eprintln!("Fetching app info…");
    let apps = client
        .get_apps()
        .await
        .context("Failed to fetch app list")?;
    let app = apps
        .iter()
        .find(|a| a.id == app_id)
        .ok_or_else(|| anyhow!("App '{app_id}' not found in your account"))?;
    let app_name = &app.name;
    let workflow_name = app
        .workflows
        .get(workflow_id)
        .map(|w| w.name.as_str())
        .unwrap_or(workflow_id);

    eprintln!("App: {app_name}  ·  Workflow: {workflow_name}");

    // 3. Find the latest finished build that contains an AAB.
    eprintln!("Searching for the latest build with an AAB artefact…");
    let (build, aab) = find_latest_aab(&client, app_id, workflow_id).await?;
    let build_label = build
        .display_build_number()
        .map(|i| format!("#{i}"))
        .unwrap_or_else(|| format!("{:.8}", build.id));
    eprintln!("Found AAB in build {build_label}: {}", aab.display_name());

    // 4. Ensure destination directory exists.
    let dest = last_apk_path(app_name, workflow_name);
    if let Some(p) = dest.parent() {
        std::fs::create_dir_all(p).context("Failed to create output directory")?;
    }

    // 5. Download the AAB, convert, and save.
    download_and_convert(&client, &aab, &dest).await?;

    println!("✓  APK saved to {}", dest.display());
    Ok(())
}

// ─── Build search ─────────────────────────────────────────────────────────────

/// Walks through builds (newest first) until it finds one with an AAB artefact.
///
/// If the list response doesn't include artefacts (empty slice), the full build
/// detail is fetched for each finished build candidate.
async fn find_latest_aab(
    client: &ApiClient,
    app_id: &str,
    workflow_id: &str,
) -> Result<(crate::models::Build, Artefact)> {
    let mut skip = 0usize;

    loop {
        let response = client
            .get_builds(skip, Some(workflow_id), Some(app_id))
            .await
            .context("Failed to fetch builds")?;

        if response.builds.is_empty() {
            bail!("No finished builds with an AAB artefact found for this app/workflow.");
        }

        for build in &response.builds {
            // Only finished builds have downloadable artefacts.
            if build.status != "finished" {
                continue;
            }

            // Use artefacts from the list response when present; otherwise fetch
            // the full build detail (the list endpoint may omit them).
            let artefacts = if !build.artefacts.is_empty() {
                build.artefacts.clone()
            } else {
                client
                    .get_build(&build.id)
                    .await
                    .map(|r| r.build.artefacts)
                    .unwrap_or_default()
            };

            if let Some(aab) = artefacts.into_iter().find(|a| a.is_aab()) {
                return Ok((build.clone(), aab));
            }
        }

        let fetched = response.builds.len();
        if fetched < PAGE_SIZE {
            bail!("Exhausted all builds — no AAB artefact found for this workflow.");
        }
        skip += fetched;
        eprintln!("  Searched {skip} builds so far, looking further back…");
    }
}

// ─── Download + bundletool conversion ────────────────────────────────────────

async fn download_and_convert(client: &ApiClient, aab: &Artefact, dest: &Path) -> Result<()> {
    // Public URL (1-hour TTL).
    let aab_url = aab
        .url
        .as_deref()
        .ok_or_else(|| anyhow!("AAB has no URL"))?;
    eprintln!("Generating download link…");
    let public_url = client
        .create_artifact_public_url(aab_url)
        .await
        .context("Failed to create artifact public URL")?;

    // Temp workspace.
    let tmp = std::env::temp_dir().join("codemagic-cli-dl");
    tokio::fs::create_dir_all(&tmp).await?;
    let aab_path = tmp.join("app.aab");
    let apks_path = tmp.join("app.apks");

    eprintln!("Downloading AAB ({})…", aab.display_size());
    client
        .download_file(&public_url, &aab_path)
        .await
        .context("Failed to download AAB")?;

    // Bundletool.
    let bt = ensure_bundletool().await?;
    let (prog, mut args) = match &bt {
        Bt::Binary => ("bundletool".to_string(), vec![]),
        Bt::Jar(jar) => (
            "java".to_string(),
            vec!["-jar".to_string(), jar.to_string_lossy().into_owned()],
        ),
    };
    args.extend([
        "build-apks".to_string(),
        "--bundle".to_string(),
        aab_path.to_string_lossy().into_owned(),
        "--output".to_string(),
        apks_path.to_string_lossy().into_owned(),
        "--mode=universal".to_string(),
        "--overwrite".to_string(),
    ]);

    eprintln!("Converting AAB → APK (bundletool)…");
    let out = tokio::process::Command::new(&prog)
        .args(&args)
        .output()
        .await
        .context("Failed to run bundletool")?;
    if !out.status.success() {
        bail!(
            "bundletool failed:\n{}",
            String::from_utf8_lossy(&out.stderr)
        );
    }

    // Extract universal.apk from the .apks archive.
    eprintln!("Extracting universal APK…");
    let extract = tokio::process::Command::new("unzip")
        .args([
            "-o",
            apks_path.to_str().unwrap_or(""),
            "universal.apk",
            "-d",
            tmp.to_str().unwrap_or("/tmp"),
        ])
        .output()
        .await
        .context("Failed to run unzip")?;
    if !extract.status.success() {
        bail!(
            "unzip failed:\n{}",
            String::from_utf8_lossy(&extract.stderr)
        );
    }

    // Move to final destination.
    tokio::fs::copy(tmp.join("universal.apk"), dest)
        .await
        .context("Failed to copy APK to destination")?;

    Ok(())
}

// ─── bundletool resolution ────────────────────────────────────────────────────

enum Bt {
    Binary,
    Jar(PathBuf),
}

async fn ensure_bundletool() -> Result<Bt> {
    // 1. System binary on PATH?
    if tokio::process::Command::new("bundletool")
        .arg("version")
        .output()
        .await
        .map(|o| o.status.success())
        .unwrap_or(false)
    {
        return Ok(Bt::Binary);
    }

    // 2. Java available?
    if !tokio::process::Command::new("java")
        .arg("-version")
        .output()
        .await
        .map(|o| o.status.success())
        .unwrap_or(false)
    {
        bail!(
            "bundletool not found and no Java runtime available.\n\
             Install one of:\n  \
             brew install bundletool   # adds `bundletool` to PATH\n  \
             brew install openjdk      # lets us run the downloaded JAR"
        );
    }

    // 3. Cached JAR (shared with the TUI's auto-download cache)?
    let jar = bundletool_jar_path();
    if jar.exists() {
        eprintln!("Using cached bundletool JAR.");
        return Ok(Bt::Jar(jar));
    }

    // 4. Download the latest release JAR from GitHub.
    eprintln!("bundletool not found — downloading latest JAR from GitHub (one-time)…");
    let http = reqwest::Client::new();
    let jar_url = github_bundletool_jar_url(&http).await?;
    let bytes = http
        .get(&jar_url)
        .header("User-Agent", "codemagic-cli")
        .send()
        .await
        .context("Failed to download bundletool JAR")?
        .bytes()
        .await
        .context("Failed to read bundletool JAR")?;

    if let Some(p) = jar.parent() {
        tokio::fs::create_dir_all(p).await?;
    }
    tokio::fs::write(&jar, &bytes)
        .await
        .context("Failed to cache bundletool JAR")?;
    eprintln!(
        "bundletool JAR saved ({:.1} MB).",
        bytes.len() as f64 / 1_048_576.0
    );

    Ok(Bt::Jar(jar))
}

/// Path where the bundletool JAR is cached (shared with the TUI).
fn bundletool_jar_path() -> PathBuf {
    dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("codemagic-cli")
        .join("bundletool.jar")
}

async fn github_bundletool_jar_url(http: &reqwest::Client) -> Result<String> {
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
        .context("Failed to reach GitHub API for bundletool release info")?
        .json()
        .await
        .context("Failed to parse bundletool release JSON")?;

    release
        .assets
        .into_iter()
        .find(|a| a.name.ends_with(".jar"))
        .map(|a| a.browser_download_url)
        .ok_or_else(|| anyhow!("No bundletool JAR found in GitHub release"))
}

// ─── Path helpers ─────────────────────────────────────────────────────────────

/// `~/Codemagic/{app}/{workflow}/last/build.apk`
fn last_apk_path(app_name: &str, workflow_name: &str) -> PathBuf {
    let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));
    home.join("Codemagic")
        .join(sanitize(app_name))
        .join(sanitize(workflow_name))
        .join("last")
        .join("build.apk")
}

fn sanitize(s: &str) -> String {
    s.chars()
        .map(|c| match c {
            '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|' => '_',
            c => c,
        })
        .collect::<String>()
        .trim()
        .to_string()
}
