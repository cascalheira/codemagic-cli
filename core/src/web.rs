//! Links into the Codemagic web UI.

/// URL of a build's page in the Codemagic web UI.
pub fn build_url(app_id: &str, build_id: &str) -> String {
    format!("https://codemagic.io/app/{app_id}/build/{build_id}")
}

/// Opens `url` in the user's default browser.
///
/// Best-effort: spawn failures are ignored, and this is a no-op on platforms
/// without a shell command for it (iOS/Android).
pub fn open_in_browser(url: &str) {
    #[cfg(target_os = "macos")]
    let _ = std::process::Command::new("open").arg(url).spawn();

    #[cfg(target_os = "linux")]
    let _ = std::process::Command::new("xdg-open").arg(url).spawn();

    #[cfg(target_os = "windows")]
    let _ = std::process::Command::new("cmd")
        .args(["/c", "start", url])
        .spawn();

    #[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
    let _ = url;
}
