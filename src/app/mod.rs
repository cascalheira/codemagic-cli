use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

use anyhow::Result;
use tokio::sync::mpsc;

use crate::api::ApiClient;
use crate::models::{Application, Build};

pub mod build_list;
pub mod build_popup;
pub mod dialogs;
pub mod download;
pub mod keys;
pub mod messages;
pub mod new_build;

// ─── Screens ─────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq)]
pub enum Screen {
    Onboarding,
    Builds,
}

// ─── Build detail popup hierarchy ────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq)]
pub enum BuildPopup {
    /// Top-level action menu (Enter on a build row).
    /// Build details are shown inline at the top of this popup.
    Actions,
    /// Downloadable artefact list (needs full build detail).
    Artifacts,
    /// Build-step list whose entries carry log URLs.
    LogSteps,
    /// Scrollable plain-text log for a single build step.
    LogContent,
}

// ─── App-info dialog entry ─────────────────────────────────────────────────

/// One display row in the App & Workflow IDs dialog.
/// Variants whose name ends in a selectable ID implement `selectable_id()`.
#[derive(Clone, Debug)]
pub enum InfoEntry {
    Separator,
    AppName(String),
    /// The app's MongoDB ID — selectable.
    AppId(String),
    WorkflowsHeader,
    /// A Workflow-Editor workflow — selectable.
    WorkflowRow {
        name: String,
        id: String,
    },
    NoWorkflows,
}

impl InfoEntry {
    /// Returns the ID that should be copied when this entry is selected,
    /// or `None` for non-selectable display rows.
    pub fn selectable_id(&self) -> Option<&str> {
        match self {
            Self::AppId(id) | Self::WorkflowRow { id, .. } => Some(id),
            _ => None,
        }
    }

    /// Short label used in the "✓ Copied …" status line.
    pub fn copy_label(&self) -> String {
        match self {
            Self::AppId(id) => format!("App ID {}", &id[..id.len().min(12)]),
            Self::WorkflowRow { name, id } => format!("{name} ID {}", &id[..id.len().min(12)]),
            _ => String::new(),
        }
    }
}

// ─── New-build wizard steps ─────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq)]
pub enum NewBuildStep {
    /// Fetching and showing the app list.
    SelectApp,
    /// Showing the workflow list (or a manual-ID text field).
    SelectWorkflow,
    /// Branch text input.
    EnterBranch,
}

// ─── Loading state ───────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub enum LoadingState {
    Idle,
    Loading,
    #[allow(dead_code)]
    Error(String),
}

// ─── Async messages ──────────────────────────────────────────────────────────

pub enum AppMessage {
    // Builds list
    BuildsLoaded(Result<crate::models::BuildsResponse>),
    TokenValidated(Result<bool>),
    // Build detail sheet
    BuildDetailLoaded(Result<Build>),
    LogContentLoaded(Result<String>),
    ArtifactDownloaded {
        name: String,
        result: Result<PathBuf>,
    },
    /// Intermediate progress shown while downloading bundletool or the AAB.
    ApkStatus(String),
    ApkReady(Result<PathBuf>),
    /// Result of polling a single running build.
    BuildStatusUpdated {
        build_id: String,
        result: Result<Build>,
    },
    // New-build wizard
    AppsLoaded(Result<Vec<Application>>),
    BuildStarted(Result<String>),
    // Settings dialog
    SettingsTokenValidated(Result<bool>),
}

// ─── App ─────────────────────────────────────────────────────────────────────

pub struct App {
    // ── Screen ──
    pub screen: Screen,

    // ── Onboarding ──
    pub api_token_input: String,
    pub onboarding_loading: bool,
    pub onboarding_error: Option<String>,

    // ── Builds list ──
    pub builds: Vec<Build>,
    pub applications: HashMap<String, Application>,
    pub selected_index: usize,
    pub skip: usize,
    pub has_more: bool,
    pub loading_state: LoadingState,

    // ── Workflow filter ──
    pub workflow_filter: Option<String>,
    pub show_filter_popup: bool,
    pub available_workflows: Vec<(String, String)>,
    pub filter_selected_index: usize,

    // ── Build detail popup ──
    pub build_popup: Option<BuildPopup>,
    /// Selection index inside the Actions menu (0-3).
    pub popup_action_index: usize,
    /// Selection index inside the Artifacts list.
    pub artifact_index: usize,
    /// Selection index inside the LogSteps list.
    pub log_step_index: usize,

    // Full build detail (fetched on demand).
    pub detail_build: Option<Build>,
    pub detail_loading: bool,
    pub detail_error: Option<String>,

    // Log content for the selected step.
    pub log_lines: Vec<String>,
    pub log_scroll: usize,
    pub log_loading: bool,

    // Status / feedback lines shown inside the popups.
    pub artifact_message: Option<String>,
    pub apk_message: Option<String>,

    // ── New-build wizard ──
    pub new_build_step: Option<NewBuildStep>,
    /// All apps loaded from `GET /apps`.
    pub new_build_apps: Vec<Application>,
    pub new_build_apps_loading: bool,
    pub new_build_app_index: usize,
    /// Sorted (id, name) workflows for the currently selected app.
    pub new_build_workflow_index: usize,
    /// `true` when the user is typing a workflow ID manually.
    pub new_build_typing_workflow: bool,
    pub new_build_workflow_input: String,
    /// Text typed in the branch filter bar (case-insensitive substring match).
    pub new_build_branch_filter: String,
    /// Highlighted row in the filtered branch list.
    pub new_build_branch_list_index: usize,
    pub new_build_error: Option<String>,
    pub new_build_submitting: bool,

    // ── App / workflow ID browser ──
    pub app_info_open: bool,
    pub app_info_scroll: usize,
    /// Index into the selectable-only subset of `build_info_entries()`.
    pub app_info_selected: usize,
    pub app_info_copy_msg: Option<String>,

    // ── Settings dialog ──
    pub settings_open: bool,
    pub settings_token_input: String,
    pub settings_loading: bool,
    pub settings_error: Option<String>,
    pub settings_success: Option<String>,

    // ── Misc ──
    pub should_quit: bool,
    pub status_message: Option<String>,

    // ── Internals ──
    pub(crate) api_client: Option<ApiClient>,
    pub(crate) tx: mpsc::Sender<AppMessage>,
}

impl App {
    pub fn new(tx: mpsc::Sender<AppMessage>, config: Option<crate::config::Config>) -> Self {
        let (screen, api_client) = match config {
            Some(cfg) => (Screen::Builds, Some(ApiClient::new(cfg.api_token))),
            None => (Screen::Onboarding, None),
        };
        Self {
            screen,
            api_token_input: String::new(),
            onboarding_loading: false,
            onboarding_error: None,
            builds: Vec::new(),
            applications: HashMap::new(),
            selected_index: 0,
            skip: 0,
            has_more: true,
            loading_state: LoadingState::Idle,
            workflow_filter: None,
            show_filter_popup: false,
            available_workflows: Vec::new(),
            filter_selected_index: 0,
            build_popup: None,
            popup_action_index: 0,
            artifact_index: 0,
            log_step_index: 0,
            detail_build: None,
            detail_loading: false,
            detail_error: None,
            log_lines: Vec::new(),
            log_scroll: 0,
            log_loading: false,
            artifact_message: None,
            apk_message: None,
            new_build_step: None,
            new_build_apps: Vec::new(),
            new_build_apps_loading: false,
            new_build_app_index: 0,
            new_build_workflow_index: 0,
            new_build_typing_workflow: false,
            new_build_workflow_input: String::new(),
            new_build_branch_filter: String::new(),
            new_build_branch_list_index: 0,
            new_build_error: None,
            new_build_submitting: false,
            app_info_open: false,
            app_info_scroll: 0,
            app_info_selected: 0,
            app_info_copy_msg: None,
            settings_open: false,
            settings_token_input: String::new(),
            settings_loading: false,
            settings_error: None,
            settings_success: None,
            should_quit: false,
            status_message: None,
            api_client,
            tx,
        }
    }

    // ─── Public getters (used from ui.rs) ────────────────────────────────────

    pub fn app_name(&self, app_id: &str) -> &str {
        self.applications
            .get(app_id)
            .map(|a| a.name.as_str())
            .unwrap_or("Unknown App")
    }

    pub fn active_workflow_name(&self) -> &str {
        match &self.workflow_filter {
            None => "All Workflows",
            Some(id) => self
                .available_workflows
                .iter()
                .find(|(wid, _)| wid == id)
                .map(|(_, name)| name.as_str())
                .unwrap_or(id.as_str()),
        }
    }

    /// Number of builds in the list that are currently in a running state.
    /// Used by the UI to show the live-build badge.
    pub fn running_build_count(&self) -> usize {
        self.builds
            .iter()
            .filter(|b| is_running_status(&b.status))
            .count()
    }

    /// Total rendered lines (= entries count). Used to clamp scroll.
    #[allow(dead_code)]
    pub fn app_info_line_count(&self) -> usize {
        self.build_info_entries().len()
    }

    /// Builds the flat list of display rows for the app-info dialog.
    /// The same structure is used by `ui.rs` for rendering, ensuring the line
    /// indices match the selectable indices tracked in `handle_app_info_key`.
    pub fn build_info_entries(&self) -> Vec<InfoEntry> {
        let mut v: Vec<InfoEntry> = Vec::new();
        for (i, app) in self.new_build_apps.iter().enumerate() {
            if i > 0 {
                v.push(InfoEntry::Separator);
            }
            v.push(InfoEntry::AppName(app.name.clone()));
            v.push(InfoEntry::AppId(app.id.clone()));
            if app.workflows.is_empty() {
                v.push(InfoEntry::NoWorkflows);
            } else {
                v.push(InfoEntry::WorkflowsHeader);
                let mut wfs: Vec<_> = app.workflows.iter().collect();
                wfs.sort_by(|a, b| a.1.name.cmp(&b.1.name));
                for (id, info) in wfs {
                    v.push(InfoEntry::WorkflowRow {
                        name: info.name.clone(),
                        id: id.clone(),
                    });
                }
            }
        }
        v
    }

    /// Returns branches for the currently selected app filtered by
    /// `new_build_branch_filter` (case-insensitive substring match).
    /// An empty filter returns all branches.
    pub fn get_filtered_branches(&self) -> Vec<&str> {
        let needle = self.new_build_branch_filter.to_lowercase();
        self.new_build_apps
            .get(self.new_build_app_index)
            .map(|app| {
                app.branches
                    .iter()
                    .filter(|b| needle.is_empty() || b.to_lowercase().contains(&needle))
                    .map(|b| b.as_str())
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Returns the sorted (id, name) workflow list for the currently selected
    /// app. Used by both the key handler and the UI.
    pub fn get_new_build_workflows(&self) -> Vec<(String, String)> {
        self.new_build_apps
            .get(self.new_build_app_index)
            .map(|app| {
                let mut wfs: Vec<(String, String)> = app
                    .workflows
                    .iter()
                    .map(|(id, info)| (id.clone(), info.name.clone()))
                    .collect();
                wfs.sort_by(|a, b| a.1.cmp(&b.1));
                wfs
            })
            .unwrap_or_default()
    }

    // ─── Live-build polling ───────────────────────────────────────────────────

    /// Spawns a status-refresh task for every build that is currently running.
    /// Called periodically from the event loop; is a no-op when nothing is live.
    pub fn poll_running_builds(&mut self) {
        if self.screen != Screen::Builds {
            return;
        }
        let Some(client) = self.api_client.clone() else {
            return;
        };

        let ids: Vec<String> = self
            .builds
            .iter()
            .filter(|b| is_running_status(&b.status))
            .map(|b| b.id.clone())
            .collect();

        for build_id in ids {
            let client = client.clone();
            let tx = self.tx.clone();
            let id = build_id.clone();
            tokio::spawn(async move {
                let result = client.get_build(&id).await.map(|r| r.build);
                let _ = tx
                    .send(AppMessage::BuildStatusUpdated {
                        build_id: id,
                        result,
                    })
                    .await;
            });
        }
    }

    // ─── Internal helper ─────────────────────────────────────────────────────

    pub(crate) fn update_available_workflows(&mut self) {
        // Seed `seen` from workflows already in the list so that applying a
        // filter (which only loads builds for one workflow) never shrinks the
        // set of choices shown in the filter popup.
        let mut seen: HashSet<String> = self
            .available_workflows
            .iter()
            .map(|(id, _)| id.clone())
            .collect();

        for build in &self.builds {
            if let Some(id) = build.effective_workflow_id() {
                if seen.insert(id.to_string()) {
                    self.available_workflows
                        .push((id.to_string(), build.workflow_display().to_string()));
                }
            }
        }
    }
}

/// Returns `true` for any build status that means the build is still in progress.
pub fn is_running_status(status: &str) -> bool {
    matches!(
        status,
        "building" | "queued" | "preparing" | "fetching" | "testing" | "publishing" | "finishing"
    )
}
