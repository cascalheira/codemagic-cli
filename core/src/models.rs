// Fields are part of the Codemagic API contract and may be used in future features.
#![allow(dead_code)]

use chrono::{DateTime, Utc};
use serde::Deserialize;
use std::collections::HashMap;

// ─── Commit ───────────────────────────────────────────────────────────────────

#[derive(Debug, Deserialize, Clone, PartialEq)]
pub struct Commit {
    #[serde(rename = "commitMessage", default)]
    pub message: Option<String>,
    #[serde(rename = "authorName", default)]
    pub author: Option<String>,
    /// `hash` is the field name in the actual API response.
    #[serde(rename = "hash", default)]
    pub sha: Option<String>,
}

// ─── BuildConfig (gives us the workflow name) ─────────────────────────────────

/// The `config` object embedded in each build. Its `name` field is the
/// human-readable workflow name (equivalent to `Workflow.name`).
#[derive(Debug, Deserialize, Clone, PartialEq)]
pub struct BuildConfig {
    /// Display name of the workflow used for this build.
    pub name: String,
}

// ─── Build ────────────────────────────────────────────────────────────────────

#[derive(Debug, Deserialize, Clone, PartialEq)]
pub struct Build {
    #[serde(rename = "_id")]
    pub id: String,

    #[serde(rename = "appId")]
    pub app_id: String,

    /// Set for Workflow-Editor workflows.
    #[serde(rename = "workflowId", default)]
    pub workflow_id: Option<String>,

    /// Set for codemagic.yaml workflows.
    #[serde(rename = "fileWorkflowId", default)]
    pub file_workflow_id: Option<String>,

    #[serde(default)]
    pub branch: Option<String>,

    #[serde(default)]
    pub tag: Option<String>,

    pub status: String,

    /// When the build started executing (may be absent for queued builds).
    #[serde(rename = "startedAt", default)]
    pub started_at: Option<DateTime<Utc>>,

    #[serde(rename = "finishedAt", default)]
    pub finished_at: Option<DateTime<Utc>>,

    /// When the build was created / enqueued.
    #[serde(rename = "createdAt", default)]
    pub created_at: Option<DateTime<Utc>>,

    /// Sequential build number for the app (e.g. 42 → "build #42").
    #[serde(default)]
    pub index: Option<u32>,

    /// The real build number used in the app artefact (e.g. iOS CFBundleVersion
    /// or Android versionCode). This is typically a global counter that is
    /// higher than `index`, which only counts per-app builds.
    #[serde(rename = "buildNumber", default)]
    pub build_number: Option<u32>,

    /// App version string at the time of the build (e.g. "1.2.3" or "2.0.0+42").
    #[serde(default)]
    pub version: Option<String>,

    /// Workflow configuration snapshot; contains the workflow display `name`.
    #[serde(default)]
    pub config: Option<BuildConfig>,

    #[serde(default)]
    pub commit: Option<Commit>,

    #[serde(default)]
    pub artefacts: Vec<Artefact>,

    #[serde(rename = "buildActions", default)]
    pub build_actions: Vec<BuildAction>,
}

impl Build {
    /// The effective workflow identifier: prefers Workflow-Editor ID, falls
    /// back to the codemagic.yaml file-workflow ID.
    pub fn effective_workflow_id(&self) -> Option<&str> {
        self.workflow_id
            .as_deref()
            .or(self.file_workflow_id.as_deref())
    }

    /// The `(workflow_id, branch)` pair needed to re-trigger this build.
    ///
    /// `None` when either is missing: builds triggered from a tag carry no
    /// branch, and `POST /builds` only accepts a branch, so those can't be
    /// re-run as-is.
    pub fn rerun_target(&self) -> Option<(&str, &str)> {
        let workflow = self.effective_workflow_id()?;
        let branch = self.branch.as_deref().filter(|b| !b.is_empty())?;
        Some((workflow, branch))
    }

    /// Human-readable workflow name from `config.name`, falling back to the
    /// workflow ID.
    pub fn workflow_display(&self) -> &str {
        self.config
            .as_ref()
            .map(|c| c.name.as_str())
            .or_else(|| self.effective_workflow_id())
            .unwrap_or("-")
    }

    /// The best available build number for display.
    ///
    /// Checks (in order):
    /// 1. `buildNumber` top-level field (future-proofing, currently absent).
    /// 2. `versionCode` on the first artefact that carries one — this is the
    ///    real app build number (Android versionCode / iOS CFBundleVersion).
    /// 3. `index` — Codemagic's sequential per-app build counter (fallback).
    pub fn display_build_number(&self) -> Option<u32> {
        self.build_number
            .or_else(|| {
                self.artefacts
                    .iter()
                    .find_map(|a| a.version_code.as_deref().and_then(|v| v.parse().ok()))
            })
            .or(self.index)
    }

    /// Returns the git ref (branch or tag) as a display string.
    pub fn git_ref(&self) -> String {
        if let Some(ref branch) = self.branch
            && !branch.is_empty()
        {
            return branch.clone();
        }
        if let Some(ref tag) = self.tag
            && !tag.is_empty()
        {
            return format!("tag:{}", tag);
        }
        "-".to_string()
    }

    /// The best available "started" timestamp: prefers `startedAt`, falls
    /// back to `createdAt` (so queued/preparing builds still show a time).
    pub fn display_time(&self) -> Option<DateTime<Utc>> {
        self.started_at.or(self.created_at)
    }
}

// ─── WorkflowChoice ──────────────────────────────────────────────────────────

/// A workflow the user can filter builds by.
///
/// Carries the app it belongs to because workflow IDs are only unique within an
/// app, and the v3 listing rejects a `workflow_id` that arrives without its
/// `app_id`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkflowChoice {
    pub id: String,
    pub name: String,
    pub app_id: String,
}

impl WorkflowChoice {
    /// A stable string identity, for UI widgets that can only carry a string
    /// (an HTML `<option value>`, say). Round-trips through [`Self::from_key`].
    pub fn key(&self) -> String {
        format!("{}/{}", self.app_id, self.id)
    }

    /// Finds the choice matching `key` in `choices`.
    ///
    /// Matching against a known list rather than parsing the key means a stale
    /// or hand-edited value yields `None` instead of a plausible-looking
    /// workflow that doesn't exist.
    pub fn from_key<'a>(choices: &'a [Self], key: &str) -> Option<&'a Self> {
        choices.iter().find(|c| c.key() == key)
    }
}

impl Build {
    /// The workflow this build ran, as a filterable choice.
    ///
    /// `None` for a build with no workflow ID at all, which cannot be filtered
    /// on.
    pub fn workflow_choice(&self) -> Option<WorkflowChoice> {
        Some(WorkflowChoice {
            id: self.effective_workflow_id()?.to_string(),
            name: self.workflow_display().to_string(),
            app_id: self.app_id.clone(),
        })
    }
}

// ─── Workflow (embedded in Application) ──────────────────────────────────────

#[derive(Debug, Deserialize, Clone, PartialEq)]
pub struct WorkflowInfo {
    pub name: String,
}

// ─── Application ─────────────────────────────────────────────────────────────

#[derive(Debug, Deserialize, Clone, PartialEq)]
pub struct Application {
    #[serde(rename = "_id")]
    pub id: String,

    #[serde(rename = "appName")]
    pub name: String,

    /// Workflow-Editor workflows keyed by their ID.
    #[serde(default)]
    pub workflows: HashMap<String, WorkflowInfo>,

    /// Known branches for this repository (returned by `GET /apps`).
    #[serde(default)]
    pub branches: Vec<String>,
}

// ─── BuildsResponse ──────────────────────────────────────────────────────────

/// Response from `GET /builds`.
///
/// **Important:** `applications` is a JSON *array*, not a map.
#[derive(Debug, Deserialize)]
pub struct BuildsResponse {
    #[serde(default)]
    pub builds: Vec<Build>,

    /// All apps that appear in the returned builds (as an array).
    #[serde(default)]
    pub applications: Vec<Application>,
}

// ─── Apps list response ───────────────────────────────────────────────────────

/// Response from `GET /apps`.
#[derive(Debug, Deserialize)]
pub struct AppsResponse {
    pub applications: Vec<Application>,
}

// ─── Single app response ─────────────────────────────────────────────────────

/// Response from `GET /apps/:id`.
#[derive(Debug, Deserialize)]
pub struct AppResponse {
    pub application: Application,
}

// ─── Start-build response ─────────────────────────────────────────────────────

/// Response from `POST /builds`.
#[derive(Debug, Deserialize)]
pub struct StartBuildResponse {
    #[serde(rename = "buildId")]
    pub build_id: String,
}

// ─── Artefact ────────────────────────────────────────────────────────────────

#[derive(Debug, Deserialize, Clone, PartialEq)]
pub struct Artefact {
    #[serde(default)]
    pub name: Option<String>,
    /// Authenticated download URL.
    #[serde(default)]
    pub url: Option<String>,
    #[serde(rename = "type", default)]
    pub artefact_type: Option<String>,
    /// File size in bytes.
    #[serde(default)]
    pub size: Option<u64>,
    #[serde(rename = "packageName", default)]
    pub package_name: Option<String>,
    #[serde(rename = "versionName", default)]
    pub version_name: Option<String>,
    /// The app build number embedded in the artefact (e.g. Android versionCode
    /// or iOS CFBundleVersion). Returned as a string by the API.
    #[serde(rename = "versionCode", default)]
    pub version_code: Option<String>,
    #[serde(default)]
    pub md5: Option<String>,
}

impl Artefact {
    pub fn display_name(&self) -> &str {
        self.name.as_deref().unwrap_or("Unknown")
    }

    pub fn display_type(&self) -> &str {
        self.artefact_type.as_deref().unwrap_or("-")
    }

    pub fn is_aab(&self) -> bool {
        self.name
            .as_deref()
            .map(|n| n.ends_with(".aab"))
            .unwrap_or(false)
            || self
                .artefact_type
                .as_deref()
                .map(|t| t == "aab")
                .unwrap_or(false)
    }

    /// Human-readable file size (B / KB / MB).
    pub fn display_size(&self) -> String {
        match self.size {
            None => "-".to_string(),
            Some(bytes) if bytes < 1_024 => format!("{} B", bytes),
            Some(bytes) if bytes < 1_024 * 1_024 => {
                format!("{:.1} KB", bytes as f64 / 1_024.0)
            }
            Some(bytes) => format!("{:.1} MB", bytes as f64 / (1_024.0 * 1_024.0)),
        }
    }
}

// ─── BuildAction ─────────────────────────────────────────────────────────────

#[derive(Debug, Deserialize, Clone, PartialEq)]
pub struct BuildAction {
    #[serde(rename = "_id", default)]
    pub id: Option<String>,
    pub name: String,
    #[serde(default)]
    pub status: Option<String>,
    #[serde(rename = "startedAt", default)]
    pub started_at: Option<DateTime<Utc>>,
    #[serde(rename = "finishedAt", default)]
    pub finished_at: Option<DateTime<Utc>>,
    /// URL to the raw log text for this step.
    #[serde(rename = "logUrl", default)]
    pub log_url: Option<String>,
}

// ─── BuildDetailResponse ─────────────────────────────────────────────────────

/// Response from `GET /builds/:id`.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct BuildDetailResponse {
    pub application: Application,
    pub build: Build,
}

#[cfg(test)]
mod tests {
    use super::{Build, WorkflowChoice};

    /// Builds a `Build` from JSON so the tests exercise the real field names
    /// and defaults rather than a hand-constructed struct.
    fn build(json: serde_json::Value) -> Build {
        serde_json::from_value(json).expect("valid build")
    }

    #[test]
    fn rerun_prefers_the_workflow_editor_id() {
        let b = build(serde_json::json!({
            "_id": "b1", "appId": "a1", "status": "finished",
            "workflowId": "wf-editor", "fileWorkflowId": "wf-yaml", "branch": "main",
        }));
        assert_eq!(b.rerun_target(), Some(("wf-editor", "main")));
    }

    #[test]
    fn rerun_falls_back_to_the_yaml_workflow_id() {
        let b = build(serde_json::json!({
            "_id": "b1", "appId": "a1", "status": "finished",
            "fileWorkflowId": "ios-release", "branch": "develop",
        }));
        assert_eq!(b.rerun_target(), Some(("ios-release", "develop")));
    }

    #[test]
    fn tag_builds_have_no_rerun_target() {
        let b = build(serde_json::json!({
            "_id": "b1", "appId": "a1", "status": "finished",
            "workflowId": "wf", "tag": "v1.2.3",
        }));
        assert_eq!(b.rerun_target(), None);
    }

    #[test]
    fn an_empty_branch_is_not_a_rerun_target() {
        let b = build(serde_json::json!({
            "_id": "b1", "appId": "a1", "status": "finished",
            "workflowId": "wf", "branch": "",
        }));
        assert_eq!(b.rerun_target(), None);
    }

    #[test]
    fn a_workflow_choice_carries_the_app_it_belongs_to() {
        let b = build(serde_json::json!({
            "_id": "b1", "appId": "a1", "status": "finished",
            "workflowId": "wf", "config": {"name": "Release"},
        }));
        let choice = b.workflow_choice().expect("choice");
        assert_eq!(choice.id, "wf");
        assert_eq!(choice.name, "Release");
        assert_eq!(choice.app_id, "a1");
    }

    #[test]
    fn a_build_without_a_workflow_offers_no_choice() {
        let b = build(serde_json::json!({
            "_id": "b1", "appId": "a1", "status": "finished",
        }));
        assert_eq!(b.workflow_choice(), None);
    }

    #[test]
    fn a_choice_round_trips_through_its_key() {
        let choices = vec![
            WorkflowChoice {
                id: "wf1".into(),
                name: "Release".into(),
                app_id: "a1".into(),
            },
            // Same workflow ID under a different app: only the pair is unique.
            WorkflowChoice {
                id: "wf1".into(),
                name: "Release".into(),
                app_id: "a2".into(),
            },
        ];
        let key = choices[1].key();
        assert_eq!(WorkflowChoice::from_key(&choices, &key), Some(&choices[1]));
    }

    #[test]
    fn an_unknown_key_matches_nothing() {
        let choices = vec![WorkflowChoice {
            id: "wf1".into(),
            name: "Release".into(),
            app_id: "a1".into(),
        }];
        assert_eq!(WorkflowChoice::from_key(&choices, "a9/wf9"), None);
    }

    #[test]
    fn a_missing_workflow_is_not_a_rerun_target() {
        let b = build(serde_json::json!({
            "_id": "b1", "appId": "a1", "status": "finished", "branch": "main",
        }));
        assert_eq!(b.rerun_target(), None);
    }
}
