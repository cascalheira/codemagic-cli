//! Non-interactive CLI commands (run when the binary is invoked with subcommands).

use std::path::{Path, PathBuf};

use anyhow::{Context, Result, anyhow, bail};

use crate::api::{ApiClient, PAGE_SIZE};
use crate::config;
use crate::models::Artefact;

// ─── Public entry point ───────────────────────────────────────────────────────

/// `gantry-cli download apk --app-id X --workflow-id Y`
///
/// Finds the latest finished build for the given app / workflow that contains
/// an AAB artefact, converts it to a universal APK with bundletool, and writes
/// the result to `~/Gantry/{app}/{workflow}/last/build.apk`.
pub async fn run_download_apk(app_id: &str, workflow_id: &str) -> Result<()> {
    // 1. Load saved API token.
    let cfg = config::load_config()?.ok_or_else(|| {
        anyhow!(
            "No saved API token found.\n\
             Run `gantry-cli` (no arguments) to open the TUI and complete setup."
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

// ─── Remote access ────────────────────────────────────────────────────────────

/// `gantry-cli remote-access --build-id X [--json]`
///
/// Prints the SSH script URL and VNC credentials for a running build machine.
/// With `--json`, prints a machine-readable object instead, for piping into a
/// VNC client or an `ssh` wrapper from CI glue.
pub async fn run_remote_access(build_id: &str, json: bool) -> Result<()> {
    let cfg = config::load_config()?.ok_or_else(|| {
        anyhow!(
            "No saved API token found.\n\
             Run `gantry-cli` (no arguments) to open the TUI and complete setup."
        )
    })?;

    let access = gantry_core::api_v3::V3Client::new(cfg.api_token)
        .get_remote_access(build_id)
        .await
        .with_context(|| {
            format!(
                "Could not get remote access for build {build_id}.\n\
                 Remote access has to be enabled for the workflow before the build \
                 starts, and only lasts while the build is running."
            )
        })?;

    if json {
        println!(
            "{}",
            serde_json::json!({
                "ssh": { "script_url": access.ssh.script_url },
                "vnc": {
                    "url": access.vnc.url(),
                    "host": access.vnc.host,
                    "port": access.vnc.port,
                    "username": access.vnc.username,
                    "password": access.vnc.password,
                },
            })
        );
    } else {
        println!("SSH script : {}", access.ssh.script_url);
        println!("VNC URL    : {}", access.vnc.url());
        println!("VNC host   : {}", access.vnc.host);
        println!("VNC port   : {}", access.vnc.port);
        println!("VNC user   : {}", access.vnc.username);
        println!("VNC pass   : {}", access.vnc.password);
    }

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

/// Converts the AAB to a universal APK at `dest`, delegating to the shared
/// `gantry-core` implementation and echoing progress to stderr.
async fn download_and_convert(client: &ApiClient, aab: &Artefact, dest: &Path) -> Result<()> {
    // A line per ~5 MB downloaded: shows life on a big AAB without
    // flooding stderr.
    let mut last = 0u64;
    gantry_core::bundletool::convert_aab_to_apk(
        client,
        aab,
        dest,
        |msg| eprintln!("{msg}"),
        move |done, total| {
            if done - last >= 5 * 1024 * 1024 {
                last = done;
                match total {
                    Some(t) => eprintln!("  {:.0}%", done as f64 / t as f64 * 100.0),
                    None => eprintln!("  {:.1} MB", done as f64 / 1_048_576.0),
                }
            }
        },
    )
    .await?;
    Ok(())
}

// ─── Path helpers ─────────────────────────────────────────────────────────────

/// `~/Gantry/{app}/{workflow}/last/build.apk`
fn last_apk_path(app_name: &str, workflow_name: &str) -> PathBuf {
    let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));
    home.join("Gantry")
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
