// Fields are part of the Codemagic API contract and may be used in future features.
#![allow(dead_code)]

use chrono::{DateTime, Utc};
use serde::Deserialize;
use std::collections::HashMap;

// ─── Commit ───────────────────────────────────────────────────────────────────

#[derive(Debug, Deserialize, Clone)]
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
#[derive(Debug, Deserialize, Clone)]
pub struct BuildConfig {
    /// Display name of the workflow used for this build.
    pub name: String,
}

// ─── Build ────────────────────────────────────────────────────────────────────

#[derive(Debug, Deserialize, Clone)]
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

    /// Human-readable workflow name from `config.name`, falling back to the
    /// workflow ID.
    pub fn workflow_display(&self) -> &str {
        self.config
            .as_ref()
            .map(|c| c.name.as_str())
            .or_else(|| self.effective_workflow_id())
            .unwrap_or("-")
    }

    /// Returns the git ref (branch or tag) as a display string.
    pub fn git_ref(&self) -> String {
        if let Some(ref branch) = self.branch {
            if !branch.is_empty() {
                return branch.clone();
            }
        }
        if let Some(ref tag) = self.tag {
            if !tag.is_empty() {
                return format!("tag:{}", tag);
            }
        }
        "-".to_string()
    }

    /// The best available "started" timestamp: prefers `startedAt`, falls
    /// back to `createdAt` (so queued/preparing builds still show a time).
    pub fn display_time(&self) -> Option<DateTime<Utc>> {
        self.started_at.or(self.created_at)
    }
}

// ─── Workflow (embedded in Application) ──────────────────────────────────────

#[derive(Debug, Deserialize, Clone)]
pub struct WorkflowInfo {
    pub name: String,
}

// ─── Application ─────────────────────────────────────────────────────────────

#[derive(Debug, Deserialize, Clone)]
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

// ─── Start-build response ─────────────────────────────────────────────────────

/// Response from `POST /builds`.
#[derive(Debug, Deserialize)]
pub struct StartBuildResponse {
    #[serde(rename = "buildId")]
    pub build_id: String,
}

// ─── Artefact ────────────────────────────────────────────────────────────────

#[derive(Debug, Deserialize, Clone)]
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

#[derive(Debug, Deserialize, Clone)]
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
#[derive(Debug, Deserialize)]
pub struct BuildDetailResponse {
    pub application: Application,
    pub build: Build,
}
