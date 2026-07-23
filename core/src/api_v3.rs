//! Client for Codemagic's v3 REST API.
//!
//! The v3 API lives on a different host than the v1 API this crate's
//! [`crate::api::ApiClient`] talks to (`codemagic.io/api/v3` rather than
//! `api.codemagic.io`) and is described by an OpenAPI schema at
//! <https://codemagic.io/api/v3/schema>. Authentication is identical: the same
//! personal API token in the `x-auth-token` header.
//!
//! Only the parts Gantry needs are modelled here:
//!
//! * **Build listing** with server-side filters (status / branch / tag /
//!   workflow / app) and cursor pagination, which v1 cannot do — v1 returns an
//!   unfiltered page and leaves the narrowing to the client.
//! * **Remote access**, which has no v1 equivalent at all: SSH and VNC
//!   credentials for a build machine while the build is running.
//!
//! Responses are converted into the v1 [`Build`] model so the rest of the code
//! stays on a single build type.

use anyhow::{Context, Result, anyhow, bail};
use chrono::{DateTime, Utc};
use reqwest::Client;
use serde::Deserialize;

use crate::models::{Artefact, Build, BuildConfig, Commit};

const V3_BASE_URL: &str = "https://codemagic.io/api/v3";

/// Builds fetched per page. The API caps `page_size` at 100.
pub const PAGE_SIZE: usize = 50;

// ─── Client ──────────────────────────────────────────────────────────────────

/// A thin async wrapper around the Codemagic v3 REST API.
///
/// Cheaply cloneable, like [`crate::api::ApiClient`].
#[derive(Clone)]
pub struct V3Client {
    client: Client,
    api_token: String,
}

impl V3Client {
    pub fn new(api_token: String) -> Self {
        Self {
            client: Client::new(),
            api_token,
        }
    }

    /// Teams the authenticated user belongs to.
    ///
    /// v3 has no account-wide build feed — builds are listed per team — so this
    /// is the entry point for any build query.
    pub async fn list_teams(&self) -> Result<Vec<Team>> {
        let response = self
            .client
            .get(format!("{V3_BASE_URL}/user/teams"))
            .header("x-auth-token", &self.api_token)
            .query(&[("page_size", "100")])
            .send()
            .await
            .context("Failed to fetch teams")?;

        let body = read_body(response, "teams").await?;
        let page: Paginated<Team> = parse(&body, "teams")?;
        Ok(page.data)
    }

    /// One page of a team's builds, newest first.
    ///
    /// `cursor` is the `cursor` returned by the previous page; pass `None` for
    /// the first page. A `None` cursor in the response means the last page.
    pub async fn list_team_builds(
        &self,
        team_id: &str,
        filter: &BuildFilter,
        cursor: Option<&str>,
        page_size: usize,
    ) -> Result<BuildPage> {
        let mut params: Vec<(&str, String)> = vec![("page_size", page_size.to_string())];
        if let Some(c) = cursor {
            params.push(("cursor", c.to_string()));
        }
        if let Some(v) = &filter.app_id {
            params.push(("app_id", v.clone()));
        }
        if let Some(v) = &filter.workflow_id {
            params.push(("workflow_id", v.clone()));
        }
        if let Some(v) = &filter.branch {
            params.push(("branch", v.clone()));
        }
        if let Some(v) = &filter.tag {
            params.push(("tag", v.clone()));
        }
        if let Some(v) = filter.status {
            params.push(("status", v.as_str().to_string()));
        }

        let response = self
            .client
            .get(format!("{V3_BASE_URL}/teams/{team_id}/builds"))
            .header("x-auth-token", &self.api_token)
            .query(&params)
            .send()
            .await
            .context("Failed to fetch builds")?;

        let body = read_body(response, "builds").await?;
        let page: CursorPage<V3Build> = parse(&body, "builds")?;

        Ok(BuildPage {
            builds: page.data.into_iter().map(V3Build::into_build).collect(),
            cursor: page.cursor,
        })
    }

    /// SSH and VNC credentials for a build machine.
    ///
    /// Only available while the build is running *and* the workflow opted in
    /// (`enable_remote_access` / "Enable remote access" in the UI); otherwise
    /// the API answers 400 and the message is surfaced as the error.
    pub async fn get_remote_access(&self, build_id: &str) -> Result<RemoteAccess> {
        let response = self
            .client
            .get(format!("{V3_BASE_URL}/builds/{build_id}/remote-access"))
            .header("x-auth-token", &self.api_token)
            .send()
            .await
            .context("Failed to fetch remote access details")?;

        let body = read_body(response, "remote access").await?;
        parse(&body, "remote access")
    }
}

// ─── Request/response plumbing ───────────────────────────────────────────────

/// Reads a response body, turning a non-2xx status into an error that carries
/// the API's own message where there is one.
///
/// v3 reports failures as `{"status_code": 400, "detail": "…"}` (validation
/// errors add an `extra` array) or, for unrouted paths, as
/// `{"error": "NOT_FOUND", "message": "…"}`. Both are far more useful than the
/// bare status code, so they are preferred when present.
async fn read_body(response: reqwest::Response, what: &str) -> Result<String> {
    let status = response.status();
    let body = response
        .text()
        .await
        .with_context(|| format!("Failed to read {what} response body"))?;

    if status.is_success() {
        return Ok(body);
    }

    let detail = serde_json::from_str::<serde_json::Value>(&body)
        .ok()
        .and_then(|v| {
            v.get("detail")
                .or_else(|| v.get("message"))
                .and_then(|d| d.as_str())
                .map(str::to_string)
        });

    match detail {
        Some(msg) => bail!("{msg}"),
        None => bail!("API error: HTTP {status}"),
    }
}

fn parse<T: serde::de::DeserializeOwned>(body: &str, what: &str) -> Result<T> {
    serde_json::from_str(body).map_err(|err| {
        let snippet = &body[..body.len().min(800)];
        anyhow!("Failed to parse {what} response: {err}\n\nRaw:\n{snippet}")
    })
}

#[derive(Debug, Deserialize)]
struct Paginated<T> {
    #[serde(default = "Vec::new")]
    data: Vec<T>,
}

#[derive(Debug, Deserialize)]
struct CursorPage<T> {
    #[serde(default = "Vec::new")]
    data: Vec<T>,
    #[serde(default)]
    cursor: Option<String>,
}

// ─── Public types ────────────────────────────────────────────────────────────

#[derive(Debug, Deserialize, Clone, PartialEq)]
pub struct Team {
    pub id: String,
    pub name: String,
}

/// Server-side filters for [`V3Client::list_team_builds`].
///
/// Every field is optional and unset fields are simply not sent, so
/// `BuildFilter::default()` lists everything.
///
/// One constraint the schema does not express: `workflow_id` is only accepted
/// alongside `app_id`. Workflow IDs are scoped to an app, and sending one on its
/// own is answered with HTTP 400.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct BuildFilter {
    pub app_id: Option<String>,
    pub workflow_id: Option<String>,
    pub branch: Option<String>,
    pub tag: Option<String>,
    pub status: Option<BuildStatusFilter>,
}

impl BuildFilter {
    pub fn is_empty(&self) -> bool {
        *self == Self::default()
    }
}

/// The statuses the API accepts as a `status` query value.
///
/// Narrower than the status a build can actually report: the API buckets the
/// in-progress statuses (`initializing`, `preparing`, `fetching`, `testing`,
/// `publishing`, `finishing`) under `building`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BuildStatusFilter {
    Queued,
    Building,
    Finished,
    Failed,
    Canceled,
    Timeout,
    Skipped,
}

impl BuildStatusFilter {
    /// Every filterable status, in the order the UI lists them.
    pub const ALL: [Self; 7] = [
        Self::Queued,
        Self::Building,
        Self::Finished,
        Self::Failed,
        Self::Canceled,
        Self::Timeout,
        Self::Skipped,
    ];

    /// The wire value expected by the `status` query parameter.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Queued => "queued",
            Self::Building => "building",
            Self::Finished => "finished",
            Self::Failed => "failed",
            Self::Canceled => "canceled",
            Self::Timeout => "timeout",
            Self::Skipped => "skipped",
        }
    }

    /// Parses a wire value back into a filter, for UI widgets that round-trip
    /// through a string. Unknown values give `None`, i.e. "no filter".
    pub fn from_wire(value: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|s| s.as_str() == value)
    }

    /// Title-cased label for display.
    pub fn label(self) -> &'static str {
        match self {
            Self::Queued => "Queued",
            Self::Building => "Building",
            Self::Finished => "Finished",
            Self::Failed => "Failed",
            Self::Canceled => "Canceled",
            Self::Timeout => "Timed out",
            Self::Skipped => "Skipped",
        }
    }
}

/// One page of builds plus the cursor that fetches the next one.
#[derive(Debug, Clone)]
pub struct BuildPage {
    pub builds: Vec<Build>,
    /// `None` once the last page has been returned.
    pub cursor: Option<String>,
}

// ─── Remote access ───────────────────────────────────────────────────────────

#[derive(Debug, Deserialize, Clone, PartialEq)]
pub struct RemoteAccess {
    pub ssh: RemoteAccessSsh,
    pub vnc: RemoteAccessVnc,
}

#[derive(Debug, Deserialize, Clone, PartialEq)]
pub struct RemoteAccessSsh {
    /// URL of a script that opens an SSH session to the build machine.
    pub script_url: String,
}

#[derive(Debug, Deserialize, Clone, PartialEq)]
pub struct RemoteAccessVnc {
    pub host: String,
    pub port: u16,
    pub username: String,
    pub password: String,
}

impl RemoteAccessVnc {
    /// `vnc://user:password@host:port` — what a VNC viewer wants.
    pub fn url(&self) -> String {
        format!(
            "vnc://{}:{}@{}:{}",
            self.username, self.password, self.host, self.port
        )
    }
}

// ─── v3 build → v1 build ─────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
struct V3Build {
    id: String,
    app_id: String,
    status: String,
    #[serde(default)]
    index: Option<u32>,
    #[serde(default)]
    workflow: Option<V3Workflow>,
    #[serde(default)]
    branch: Option<String>,
    #[serde(default)]
    tag: Option<String>,
    #[serde(default)]
    commit: Option<V3Commit>,
    #[serde(default)]
    artifacts: Vec<V3Artifact>,
    #[serde(default)]
    created_at: Option<DateTime<Utc>>,
    #[serde(default)]
    started_at: Option<DateTime<Utc>>,
    #[serde(default)]
    finished_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Deserialize)]
struct V3Workflow {
    id: String,
    /// `"ui"` for Workflow-Editor workflows, `"file"` for codemagic.yaml ones.
    #[serde(default)]
    source: Option<String>,
    #[serde(default)]
    name: Option<String>,
}

#[derive(Debug, Deserialize)]
struct V3Commit {
    #[serde(default)]
    hash: Option<String>,
    #[serde(default)]
    author_name: Option<String>,
    #[serde(default)]
    message: Option<String>,
}

#[derive(Debug, Deserialize)]
struct V3Artifact {
    name: String,
    #[serde(rename = "type", default)]
    artifact_type: Option<String>,
    #[serde(default)]
    size_in_bytes: Option<u64>,
    #[serde(default)]
    version_code: Option<String>,
    #[serde(default)]
    version_name: Option<String>,
}

impl V3Build {
    fn into_build(self) -> Build {
        // v1 splits the workflow ID across two fields depending on where the
        // workflow is defined; v3 carries one ID plus a `source` discriminator.
        let (workflow_id, file_workflow_id) = match &self.workflow {
            Some(w) if w.source.as_deref() == Some("ui") => (Some(w.id.clone()), None),
            Some(w) => (None, Some(w.id.clone())),
            None => (None, None),
        };

        Build {
            id: self.id,
            app_id: self.app_id,
            workflow_id,
            file_workflow_id,
            branch: self.branch,
            tag: self.tag,
            status: self.status,
            started_at: self.started_at,
            finished_at: self.finished_at,
            created_at: self.created_at,
            index: self.index,
            build_number: None,
            version: None,
            config: self
                .workflow
                .and_then(|w| w.name)
                .map(|name| BuildConfig { name }),
            commit: self.commit.map(|c| Commit {
                message: c.message,
                author: c.author_name,
                sha: c.hash,
            }),
            artefacts: self
                .artifacts
                .into_iter()
                .map(V3Artifact::into_v1)
                .collect(),
            // Build actions (and their log URLs) are a separate v3 endpoint and
            // are not part of a build listing. The detail fetch fills them in.
            build_actions: Vec::new(),
        }
    }
}

impl V3Artifact {
    fn into_v1(self) -> Artefact {
        Artefact {
            name: Some(self.name),
            // Deliberately not mapped from `short_lived_download_url`: that URL
            // is a signed one-shot link, and `Artefact::url` is expected to be
            // the v1 `/artifacts/{path}` form that `POST …/public-url` accepts.
            // Consumers that need to download re-fetch the build over v1, which
            // is what they already do when a listing omits artefacts.
            url: None,
            artefact_type: self.artifact_type,
            size: self.size_in_bytes,
            package_name: None,
            version_name: self.version_name,
            version_code: self.version_code,
            md5: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn v3_build(json: serde_json::Value) -> Build {
        serde_json::from_value::<V3Build>(json)
            .expect("valid v3 build")
            .into_build()
    }

    #[test]
    fn a_ui_workflow_maps_to_the_workflow_editor_id() {
        let b = v3_build(serde_json::json!({
            "id": "b1", "app_id": "a1", "status": "finished",
            "workflow": {"id": "wf1", "source": "ui", "name": "Release"},
        }));
        assert_eq!(b.workflow_id.as_deref(), Some("wf1"));
        assert_eq!(b.file_workflow_id, None);
        assert_eq!(b.workflow_display(), "Release");
    }

    #[test]
    fn a_file_workflow_maps_to_the_yaml_workflow_id() {
        let b = v3_build(serde_json::json!({
            "id": "b1", "app_id": "a1", "status": "finished",
            "workflow": {"id": "ios-release", "source": "file", "name": "iOS release"},
        }));
        assert_eq!(b.workflow_id, None);
        assert_eq!(b.file_workflow_id.as_deref(), Some("ios-release"));
        assert_eq!(b.effective_workflow_id(), Some("ios-release"));
    }

    #[test]
    fn artefact_version_code_survives_so_build_numbers_still_render() {
        let b = v3_build(serde_json::json!({
            "id": "b1", "app_id": "a1", "status": "finished", "index": 10,
            "artifacts": [{
                "name": "app.aab", "type": "aab", "size_in_bytes": 42,
                "short_lived_download_url": "https://api.codemagic.io//artifacts/signed",
                "version_code": "2093", "version_name": null,
            }],
        }));
        assert_eq!(b.display_build_number(), Some(2093));
        assert_eq!(b.artefacts[0].display_size(), "42 B");
        // The signed listing URL is not a `/public-url`-capable artefact URL.
        assert_eq!(b.artefacts[0].url, None);
    }

    #[test]
    fn a_build_with_no_artefacts_falls_back_to_the_build_index() {
        let b = v3_build(serde_json::json!({
            "id": "b1", "app_id": "a1", "status": "queued", "index": 7,
        }));
        assert_eq!(b.display_build_number(), Some(7));
    }

    #[test]
    fn commit_fields_are_renamed_onto_the_v1_shape() {
        let b = v3_build(serde_json::json!({
            "id": "b1", "app_id": "a1", "status": "finished",
            "commit": {"hash": "abc123", "author_name": "Ada", "message": "Fix it"},
        }));
        let c = b.commit.expect("commit");
        assert_eq!(c.sha.as_deref(), Some("abc123"));
        assert_eq!(c.author.as_deref(), Some("Ada"));
        assert_eq!(c.message.as_deref(), Some("Fix it"));
    }

    #[test]
    fn status_filters_use_the_wire_values_the_api_validates_against() {
        assert_eq!(BuildStatusFilter::Canceled.as_str(), "canceled");
        assert_eq!(BuildStatusFilter::Timeout.as_str(), "timeout");
    }

    #[test]
    fn status_filters_round_trip_through_their_wire_value() {
        for status in BuildStatusFilter::ALL {
            assert_eq!(BuildStatusFilter::from_wire(status.as_str()), Some(status));
        }
        assert_eq!(BuildStatusFilter::from_wire(""), None);
        assert_eq!(BuildStatusFilter::from_wire("building "), None);
    }

    #[test]
    fn vnc_details_render_as_a_viewer_url() {
        let vnc = RemoteAccessVnc {
            host: "1.2.3.4".into(),
            port: 5900,
            username: "builder".into(),
            password: "s3cret".into(),
        };
        assert_eq!(vnc.url(), "vnc://builder:s3cret@1.2.3.4:5900");
    }
}
