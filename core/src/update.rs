//! Checking GitHub Releases for a newer version.
//!
//! This reports an update and points at the release page; it deliberately
//! doesn't replace anything on disk. The desktop builds ship as a signed and
//! notarised `.app` and an NSIS installer, and swapping either out from under
//! a running process is a platform-specific job (Sparkle on macOS, an
//! installer relaunch on Windows) with a real chance of leaving a half-written
//! install behind. Opening the release page is the honest, safe version.

use anyhow::{Context, Result};
use serde::Deserialize;

/// Public releases feed. The repository slug is unchanged by the rename —
/// GitHub redirects old slugs, but the API is happy to answer either way.
const LATEST_RELEASE_URL: &str =
    "https://api.github.com/repos/cascalheira/codemagic-cli/releases/latest";

/// A release newer than the running build.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Update {
    /// Version without the leading `v`, e.g. `2.1.0`.
    pub version: String,
    /// The release page to open in a browser.
    pub url: String,
}

/// The subset of GitHub's release payload we care about.
#[derive(Deserialize)]
struct GithubRelease {
    tag_name: String,
    html_url: String,
}

/// Returns the newest release if it's ahead of `current`, otherwise `None`.
///
/// `/releases/latest` already excludes drafts and pre-releases, so anything it
/// returns is a real published build.
pub async fn check(current: &str) -> Result<Option<Update>> {
    // GitHub rejects requests without a User-Agent.
    let response = reqwest::Client::new()
        .get(LATEST_RELEASE_URL)
        .header("User-Agent", "gantry")
        .header("Accept", "application/vnd.github+json")
        .send()
        .await
        .context("Couldn't reach GitHub to check for updates")?;

    if !response.status().is_success() {
        anyhow::bail!("GitHub returned {}", response.status());
    }

    let release: GithubRelease = response
        .json()
        .await
        .context("Couldn't parse the release feed")?;

    let version = release.tag_name.trim_start_matches('v').to_string();
    Ok(is_newer(current, &version).then_some(Update {
        version,
        url: release.html_url,
    }))
}

/// Whether `candidate` is a later version than `current`.
///
/// Unparseable input is treated as "no update", so a malformed tag can never
/// nag the user. Pre-releases sort below the matching final release, per
/// semver, though `/releases/latest` shouldn't surface them anyway.
pub fn is_newer(current: &str, candidate: &str) -> bool {
    match (parse(current), parse(candidate)) {
        (Some(a), Some(b)) => b > a,
        _ => false,
    }
}

/// `major.minor.patch` plus a "this is a final release" flag, ordered so that
/// deriving `Ord` on the tuple gives semver precedence: `2.1.0-rc.1 < 2.1.0`.
fn parse(version: &str) -> Option<(u64, u64, u64, bool)> {
    let v = version.trim().trim_start_matches('v');
    // Build metadata is ignored for precedence; a pre-release suffix isn't.
    let v = v.split('+').next().unwrap_or(v);
    let (core, is_final) = match v.split_once('-') {
        Some((core, _)) => (core, false),
        None => (v, true),
    };

    let mut parts = core.split('.');
    let major = parts.next()?.parse().ok()?;
    // A missing minor or patch reads as zero, so "2" and "2.0" match "2.0.0".
    let minor = parts.next().map_or(Some(0), |p| p.parse().ok())?;
    let patch = parts.next().map_or(Some(0), |p| p.parse().ok())?;
    if parts.next().is_some() {
        return None;
    }
    Some((major, minor, patch, is_final))
}

#[cfg(test)]
mod tests {
    use super::{is_newer, parse};

    #[test]
    fn detects_a_newer_version_at_every_level() {
        assert!(is_newer("2.0.1", "2.0.2"));
        assert!(is_newer("2.0.1", "2.1.0"));
        assert!(is_newer("2.0.1", "3.0.0"));
    }

    #[test]
    fn the_same_or_older_version_is_not_an_update() {
        assert!(!is_newer("2.0.1", "2.0.1"));
        assert!(!is_newer("2.0.1", "2.0.0"));
        assert!(!is_newer("2.1.0", "2.0.9"));
    }

    /// Release tags carry a `v`; `CARGO_PKG_VERSION` doesn't.
    #[test]
    fn the_v_prefix_is_optional_on_either_side() {
        assert!(is_newer("2.0.1", "v2.0.2"));
        assert!(is_newer("v2.0.1", "2.0.2"));
        assert!(!is_newer("v2.0.1", "v2.0.1"));
    }

    #[test]
    fn numbers_compare_numerically_not_lexically() {
        assert!(is_newer("2.9.0", "2.10.0"));
        assert!(!is_newer("2.10.0", "2.9.0"));
    }

    #[test]
    fn a_prerelease_sorts_below_its_final_release() {
        assert!(is_newer("2.1.0-rc.1", "2.1.0"));
        assert!(!is_newer("2.1.0", "2.1.0-rc.1"));
    }

    #[test]
    fn build_metadata_is_ignored() {
        assert!(!is_newer("2.0.1", "2.0.1+build.7"));
    }

    /// A tag we can't read must never nag the user.
    #[test]
    fn unparseable_versions_never_report_an_update() {
        assert!(!is_newer("2.0.1", "nightly"));
        assert!(!is_newer("2.0.1", ""));
        assert!(!is_newer("not-a-version", "9.9.9"));
        assert!(!is_newer("2.0.1", "1.2.3.4"));
    }

    #[test]
    fn missing_components_read_as_zero() {
        assert_eq!(parse("2"), Some((2, 0, 0, true)));
        assert_eq!(parse("2.1"), Some((2, 1, 0, true)));
        assert!(!is_newer("2.0.0", "2"));
        assert!(is_newer("2.0.0", "2.1"));
    }
}
