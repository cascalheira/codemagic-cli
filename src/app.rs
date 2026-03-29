use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

use anyhow::{Context, Result, anyhow};
use crossterm::event::{KeyCode, KeyEvent};
use tokio::sync::mpsc;

use crate::api::{ApiClient, PAGE_SIZE};
use crate::config::{self, Config};
use crate::models::{Application, Artefact, Build, BuildsResponse};

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
    /// The app’s MongoDB ID — selectable.
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
    BuildsLoaded(Result<BuildsResponse>),
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
    AppsLoaded(Result<Vec<crate::models::Application>>),
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
    pub new_build_apps: Vec<crate::models::Application>,
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
    api_client: Option<ApiClient>,
    tx: mpsc::Sender<AppMessage>,
}

impl App {
    pub fn new(tx: mpsc::Sender<AppMessage>, config: Option<Config>) -> Self {
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

    // ─── Message handler ─────────────────────────────────────────────────────

    pub fn handle_message(&mut self, msg: AppMessage) {
        match msg {
            // ── Builds list ──────────────────────────────────────────────────
            AppMessage::BuildsLoaded(result) => {
                self.loading_state = LoadingState::Idle;
                match result {
                    Ok(response) => {
                        self.has_more = response.builds.len() >= PAGE_SIZE;
                        for app in response.applications {
                            self.applications.insert(app.id.clone(), app);
                        }
                        if self.skip == 0 {
                            self.builds = response.builds;
                            self.selected_index = 0;
                        } else {
                            self.builds.extend(response.builds);
                        }
                        self.update_available_workflows();
                        self.status_message = None;
                    }
                    Err(e) => {
                        let msg = e.to_string();
                        self.loading_state = LoadingState::Error(msg.clone());
                        self.status_message = Some(msg);
                    }
                }
            }

            AppMessage::TokenValidated(result) => {
                self.onboarding_loading = false;
                match result {
                    Ok(true) => {
                        let token = self.api_token_input.trim().to_string();
                        let cfg = Config {
                            api_token: token.clone(),
                        };
                        if let Err(e) = config::save_config(&cfg) {
                            self.onboarding_error = Some(format!("Could not save config: {e}"));
                            return;
                        }
                        self.api_client = Some(ApiClient::new(token));
                        self.screen = Screen::Builds;
                        self.fetch_builds();
                    }
                    Ok(false) => {
                        self.onboarding_error = Some(
                            "Invalid API token. Check Settings > Integrations > \
                             Codemagic API > Show."
                                .to_string(),
                        );
                    }
                    Err(e) => {
                        self.onboarding_error = Some(format!("Connection error: {e}"));
                    }
                }
            }

            // ── Build detail popup ───────────────────────────────────────────
            AppMessage::BuildDetailLoaded(result) => {
                self.detail_loading = false;
                match result {
                    Ok(build) => {
                        self.detail_build = Some(build);
                        self.detail_error = None;
                        // Auto-advance to the right sub-popup.
                        match self.build_popup {
                            Some(BuildPopup::Artifacts) => {
                                self.artifact_index = 0;
                                self.artifact_message = None;
                            }
                            Some(BuildPopup::LogSteps) => {
                                self.log_step_index = 0;
                            }
                            _ => {}
                        }
                    }
                    Err(e) => {
                        self.detail_error = Some(e.to_string());
                    }
                }
            }

            AppMessage::LogContentLoaded(result) => {
                self.log_loading = false;
                match result {
                    Ok(content) => {
                        self.log_lines = content.lines().map(|l| l.to_string()).collect();
                        self.log_scroll = 0;
                        self.build_popup = Some(BuildPopup::LogContent);
                    }
                    Err(e) => {
                        self.log_lines = vec![format!("Error loading log: {e}")];
                        self.log_scroll = 0;
                        self.build_popup = Some(BuildPopup::LogContent);
                    }
                }
            }

            AppMessage::ArtifactDownloaded { name, result } => match result {
                Ok(path) => {
                    self.artifact_message = Some(format!("✓ {name} saved to {}", path.display()));
                }
                Err(e) => {
                    self.artifact_message = Some(format!("✗ Download failed: {e}"));
                }
            },

            AppMessage::ApkStatus(msg) => {
                self.apk_message = Some(msg);
            }

            AppMessage::BuildStatusUpdated { build_id, result } => {
                if let Ok(updated) = result {
                    // Patch the build inside the visible list.
                    if let Some(b) = self.builds.iter_mut().find(|b| b.id == build_id) {
                        *b = updated.clone();
                    }
                    // Also refresh the detail pane if it's open for this build.
                    if self
                        .detail_build
                        .as_ref()
                        .map(|b| b.id == build_id)
                        .unwrap_or(false)
                    {
                        self.detail_build = Some(updated);
                    }
                }
            }

            AppMessage::ApkReady(result) => match result {
                Ok(path) => {
                    self.apk_message = Some(format!("✓ APK saved to {}", path.display()));
                }
                Err(e) => {
                    self.apk_message = Some(format!("✗ {e}"));
                }
            },

            AppMessage::AppsLoaded(result) => {
                self.new_build_apps_loading = false;
                match result {
                    Ok(apps) => {
                        self.new_build_apps = apps;
                        self.new_build_app_index = 0;
                    }
                    Err(e) => {
                        self.new_build_error = Some(format!("Failed to load apps: {e}"));
                    }
                }
            }

            AppMessage::SettingsTokenValidated(result) => {
                self.settings_loading = false;
                match result {
                    Ok(true) => {
                        let token = self.settings_token_input.trim().to_string();
                        let cfg = Config {
                            api_token: token.clone(),
                        };
                        if let Err(e) = config::save_config(&cfg) {
                            self.settings_error = Some(format!("Failed to save config: {e}"));
                            return;
                        }
                        self.api_client = Some(ApiClient::new(token));
                        self.settings_success = Some("✓ Token updated successfully.".into());
                        // Reload the builds list with the new credentials.
                        self.skip = 0;
                        self.builds.clear();
                        self.fetch_builds();
                    }
                    Ok(false) => {
                        self.settings_error = Some(
                            "Invalid token. Check Settings › Integrations › \
                             Codemagic API › Show."
                                .into(),
                        );
                    }
                    Err(e) => {
                        self.settings_error = Some(format!("Connection error: {e}"));
                    }
                }
            }

            AppMessage::BuildStarted(result) => {
                self.new_build_submitting = false;
                match result {
                    Ok(build_id) => {
                        self.new_build_step = None;
                        // Show the new build ID briefly in the status bar.
                        let short = &build_id[..build_id.len().min(8)];
                        self.status_message = Some(format!("✓ Build queued  (id: {short}…)"));
                        // Reload the list from the top so the new build appears.
                        self.skip = 0;
                        self.builds.clear();
                        self.fetch_builds();
                    }
                    Err(e) => {
                        self.new_build_error = Some(format!("Failed to start build: {e}"));
                    }
                }
            }
        }
    }

    // ─── Key handler ─────────────────────────────────────────────────────────

    pub fn handle_key(&mut self, key: KeyEvent) {
        match self.screen {
            Screen::Onboarding => self.handle_onboarding_key(key),
            Screen::Builds => {
                if self.app_info_open {
                    self.handle_app_info_key(key);
                } else if self.settings_open {
                    self.handle_settings_key(key);
                } else if self.new_build_step.is_some() {
                    self.handle_new_build_key(key);
                } else if self.show_filter_popup {
                    self.handle_filter_popup_key(key);
                } else if let Some(popup) = self.build_popup.clone() {
                    self.handle_build_popup_key(popup, key);
                } else {
                    self.handle_builds_key(key);
                }
            }
        }
    }

    fn handle_onboarding_key(&mut self, key: KeyEvent) {
        if self.onboarding_loading {
            return;
        }
        match key.code {
            KeyCode::Char(c) => {
                self.api_token_input.push(c);
                self.onboarding_error = None;
            }
            KeyCode::Backspace => {
                self.api_token_input.pop();
                self.onboarding_error = None;
            }
            KeyCode::Enter => self.submit_onboarding(),
            KeyCode::Esc => self.should_quit = true,
            _ => {}
        }
    }

    fn handle_builds_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Char('q') | KeyCode::Esc => self.should_quit = true,
            KeyCode::Up | KeyCode::Char('k') => self.move_selection_up(),
            KeyCode::Down | KeyCode::Char('j') => self.move_selection_down(),
            KeyCode::Enter => self.open_build_actions(),
            KeyCode::Char('n') => self.open_new_build(),
            KeyCode::Char('i') => self.open_app_info(),
            KeyCode::Char('s') => self.open_settings(),
            KeyCode::Char('f') => self.open_filter_popup(),
            KeyCode::Char('l') => self.load_more(),
            KeyCode::Char('r') => self.refresh(),
            _ => {}
        }
    }

    fn handle_build_popup_key(&mut self, popup: BuildPopup, key: KeyEvent) {
        match popup {
            BuildPopup::Actions => match key.code {
                KeyCode::Esc => self.build_popup = None,
                KeyCode::Up | KeyCode::Char('k') => {
                    if self.popup_action_index > 0 {
                        self.popup_action_index -= 1;
                    }
                }
                KeyCode::Down | KeyCode::Char('j') => {
                    if self.popup_action_index < 1 {
                        self.popup_action_index += 1;
                    }
                }
                KeyCode::Enter => self.confirm_build_action(),
                _ => {}
            },

            BuildPopup::Artifacts => match key.code {
                KeyCode::Esc | KeyCode::Char('q') => self.build_popup = Some(BuildPopup::Actions),
                KeyCode::Up | KeyCode::Char('k') => {
                    if self.artifact_index > 0 {
                        self.artifact_index -= 1;
                    }
                }
                KeyCode::Down | KeyCode::Char('j') => {
                    // Extra row at the end when an AAB is present.
                    let max = self
                        .detail_build
                        .as_ref()
                        .map(|b| {
                            let n = b.artefacts.len();
                            let extra = if b.artefacts.iter().any(|a| a.is_aab()) {
                                1
                            } else {
                                0
                            };
                            (n + extra).saturating_sub(1)
                        })
                        .unwrap_or(0);
                    if self.artifact_index < max {
                        self.artifact_index += 1;
                    }
                }
                KeyCode::Enter => self.download_selected_artifact(),
                _ => {}
            },

            BuildPopup::LogSteps => match key.code {
                KeyCode::Esc | KeyCode::Char('q') => self.build_popup = Some(BuildPopup::Actions),
                KeyCode::Up | KeyCode::Char('k') => {
                    if self.log_step_index > 0 {
                        self.log_step_index -= 1;
                    }
                }
                KeyCode::Down | KeyCode::Char('j') => {
                    let max = self
                        .detail_build
                        .as_ref()
                        .map(|b| b.build_actions.len().saturating_sub(1))
                        .unwrap_or(0);
                    if self.log_step_index < max {
                        self.log_step_index += 1;
                    }
                }
                KeyCode::Enter => self.load_selected_step_log(),
                _ => {}
            },

            BuildPopup::LogContent => match key.code {
                KeyCode::Esc | KeyCode::Char('q') => {
                    self.build_popup = Some(BuildPopup::LogSteps);
                }
                KeyCode::Up | KeyCode::Char('k') => {
                    self.log_scroll = self.log_scroll.saturating_sub(1);
                }
                KeyCode::Down | KeyCode::Char('j') => {
                    let max = self.log_lines.len().saturating_sub(1);
                    if self.log_scroll < max {
                        self.log_scroll += 1;
                    }
                }
                KeyCode::PageUp => {
                    self.log_scroll = self.log_scroll.saturating_sub(20);
                }
                KeyCode::PageDown => {
                    let max = self.log_lines.len().saturating_sub(1);
                    self.log_scroll = (self.log_scroll + 20).min(max);
                }
                _ => {}
            },
        }
    }

    fn handle_filter_popup_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Esc => self.show_filter_popup = false,
            KeyCode::Up | KeyCode::Char('k') => self.move_filter_up(),
            KeyCode::Down | KeyCode::Char('j') => self.move_filter_down(),
            KeyCode::Enter => self.confirm_filter(),
            _ => {}
        }
    }

    // ─── New-build wizard ───────────────────────────────────────────────────────

    fn handle_new_build_key(&mut self, key: KeyEvent) {
        match self.new_build_step.clone() {
            None => {}

            Some(NewBuildStep::SelectApp) => match key.code {
                KeyCode::Esc => self.new_build_step = None,
                KeyCode::Up | KeyCode::Char('k') => {
                    if self.new_build_app_index > 0 {
                        self.new_build_app_index -= 1;
                    }
                }
                KeyCode::Down | KeyCode::Char('j') => {
                    let max = self.new_build_apps.len().saturating_sub(1);
                    if self.new_build_app_index < max {
                        self.new_build_app_index += 1;
                    }
                }
                KeyCode::Enter => self.confirm_new_build_app(),
                _ => {}
            },

            Some(NewBuildStep::SelectWorkflow) => {
                if self.new_build_typing_workflow {
                    // Manual workflow-ID text input.
                    match key.code {
                        KeyCode::Esc => {
                            self.new_build_typing_workflow = false;
                            self.new_build_workflow_input.clear();
                        }
                        KeyCode::Enter => {
                            if !self.new_build_workflow_input.trim().is_empty() {
                                self.new_build_step = Some(NewBuildStep::EnterBranch);
                                self.new_build_branch_filter.clear();
                                self.new_build_branch_list_index = 0;
                                self.new_build_error = None;
                            }
                        }
                        KeyCode::Char(c) => {
                            self.new_build_workflow_input.push(c);
                            self.new_build_error = None;
                        }
                        KeyCode::Backspace => {
                            self.new_build_workflow_input.pop();
                        }
                        _ => {}
                    }
                } else {
                    // List selection.
                    let wf_count = self.get_new_build_workflows().len();
                    // +1 for "Enter ID manually…" at the bottom.
                    let total = wf_count + 1;
                    match key.code {
                        KeyCode::Esc => {
                            self.new_build_step = Some(NewBuildStep::SelectApp);
                            self.new_build_workflow_index = 0;
                        }
                        KeyCode::Up | KeyCode::Char('k') => {
                            if self.new_build_workflow_index > 0 {
                                self.new_build_workflow_index -= 1;
                            }
                        }
                        KeyCode::Down | KeyCode::Char('j') => {
                            if self.new_build_workflow_index + 1 < total {
                                self.new_build_workflow_index += 1;
                            }
                        }
                        KeyCode::Enter => self.confirm_new_build_workflow(),
                        _ => {}
                    }
                }
            }

            Some(NewBuildStep::EnterBranch) => {
                if self.new_build_submitting {
                    return;
                }
                match key.code {
                    KeyCode::Esc => {
                        self.new_build_step = Some(NewBuildStep::SelectWorkflow);
                        self.new_build_error = None;
                    }
                    KeyCode::Enter => self.submit_new_build(),
                    // Arrow keys navigate the filtered list.
                    KeyCode::Up => {
                        if self.new_build_branch_list_index > 0 {
                            self.new_build_branch_list_index -= 1;
                        }
                    }
                    KeyCode::Down => {
                        let max = self.get_filtered_branches().len().saturating_sub(1);
                        if self.new_build_branch_list_index < max {
                            self.new_build_branch_list_index += 1;
                        }
                    }
                    // Every other printable character goes to the filter.
                    KeyCode::Char(c) => {
                        self.new_build_branch_filter.push(c);
                        self.new_build_branch_list_index = 0;
                        self.new_build_error = None;
                    }
                    KeyCode::Backspace => {
                        self.new_build_branch_filter.pop();
                        self.new_build_branch_list_index = 0;
                    }
                    _ => {}
                }
            }
        }
    }

    pub fn open_new_build(&mut self) {
        if self.api_client.is_none() {
            return;
        }
        self.new_build_step = Some(NewBuildStep::SelectApp);
        self.new_build_apps = Vec::new();
        self.new_build_apps_loading = true;
        self.new_build_app_index = 0;
        self.new_build_workflow_index = 0;
        self.new_build_typing_workflow = false;
        self.new_build_workflow_input.clear();
        self.new_build_branch_filter.clear();
        self.new_build_branch_list_index = 0;
        self.new_build_error = None;
        self.new_build_submitting = false;

        let Some(client) = self.api_client.clone() else {
            return;
        };
        let tx = self.tx.clone();
        tokio::spawn(async move {
            let result = client.get_apps().await;
            let _ = tx.send(AppMessage::AppsLoaded(result)).await;
        });
    }

    fn confirm_new_build_app(&mut self) {
        if self.new_build_apps.is_empty() {
            return;
        }
        self.new_build_step = Some(NewBuildStep::SelectWorkflow);
        self.new_build_workflow_index = 0;
        self.new_build_typing_workflow = false;
        self.new_build_workflow_input.clear();
        self.new_build_error = None;

        // If the app has no workflows at all, skip straight to manual input.
        let workflows = self.get_new_build_workflows();
        if workflows.is_empty() {
            self.new_build_typing_workflow = true;
        }
    }

    fn confirm_new_build_workflow(&mut self) {
        let wf_len = self.get_new_build_workflows().len();
        if self.new_build_workflow_index == wf_len {
            // "Enter ID manually…" row.
            self.new_build_typing_workflow = true;
            self.new_build_workflow_input.clear();
        } else {
            self.new_build_step = Some(NewBuildStep::EnterBranch);
            self.new_build_branch_filter.clear();
            self.new_build_branch_list_index = 0;
            self.new_build_error = None;
        }
    }

    fn submit_new_build(&mut self) {
        // Prefer the highlighted item from the filtered list; if the list is
        // empty (no match or repo has no branches), fall back to the raw filter
        // text as a custom branch name.
        let branch = {
            let filtered = self.get_filtered_branches();
            if !filtered.is_empty() {
                let idx = self
                    .new_build_branch_list_index
                    .min(filtered.len().saturating_sub(1));
                filtered[idx].to_string()
            } else {
                self.new_build_branch_filter.trim().to_string()
            }
        };
        if branch.is_empty() {
            self.new_build_error = Some("Please select or type a branch name.".into());
            return;
        }
        let Some(app) = self.new_build_apps.get(self.new_build_app_index) else {
            return;
        };
        let app_id = app.id.clone();

        let workflow_id = if self.new_build_typing_workflow {
            let id = self.new_build_workflow_input.trim().to_string();
            if id.is_empty() {
                self.new_build_error = Some("Please enter a workflow ID.".into());
                return;
            }
            id
        } else {
            let wfs = self.get_new_build_workflows();
            match wfs.get(self.new_build_workflow_index) {
                Some((id, _)) => id.clone(),
                None => {
                    self.new_build_error = Some("No workflow selected.".into());
                    return;
                }
            }
        };

        self.new_build_submitting = true;
        self.new_build_error = None;

        let Some(client) = self.api_client.clone() else {
            return;
        };
        let tx = self.tx.clone();
        tokio::spawn(async move {
            let result = client.start_build(&app_id, &workflow_id, &branch).await;
            let _ = tx.send(AppMessage::BuildStarted(result)).await;
        });
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

    // ─── Build actions ──────────────────────────────────────────────────────────

    fn open_build_actions(&mut self) {
        if self.builds.is_empty() {
            return;
        }
        self.build_popup = Some(BuildPopup::Actions);
        self.popup_action_index = 0;
        self.detail_error = None;
        self.artifact_message = None;
        self.apk_message = None;
    }

    fn confirm_build_action(&mut self) {
        match self.popup_action_index {
            0 => self.open_artifacts(),
            1 => self.open_log_steps(),
            _ => {}
        }
    }

    /// Returns the currently selected build (from the list).
    fn selected_build(&self) -> Option<&Build> {
        self.builds.get(self.selected_index)
    }

    // ── Details ───────────────────────────────────────────────────────────────

    // Details just switches the popup; the UI reads from `selected_build()`.

    // ── Artifacts ─────────────────────────────────────────────────────────────

    fn open_artifacts(&mut self) {
        self.build_popup = Some(BuildPopup::Artifacts);
        self.artifact_index = 0;
        self.artifact_message = None;
        // If we already fetched the detail build for this build, use it;
        // otherwise kick off a fetch.
        let needs_fetch = self
            .detail_build
            .as_ref()
            .map(|b| {
                self.selected_build()
                    .map(|sel| b.id != sel.id)
                    .unwrap_or(true)
            })
            .unwrap_or(true);

        if needs_fetch {
            self.fetch_build_detail();
        }
    }

    fn download_selected_artifact(&mut self) {
        // If the index sits past the artefact list, it points to the
        // "Convert → APK" row that appears when an AAB is present.
        let artefacts_len = self
            .detail_build
            .as_ref()
            .map(|b| b.artefacts.len())
            .unwrap_or(0);
        if self.artifact_index == artefacts_len {
            self.do_convert_aab();
            return;
        }

        let Some(build) = self.detail_build.as_ref() else {
            return;
        };
        let Some(artefact) = build.artefacts.get(self.artifact_index) else {
            return;
        };
        let Some(artifact_url) = artefact.url.clone() else {
            self.artifact_message = Some("Artefact has no download URL.".into());
            return;
        };

        // Gather path components from the current build context.
        let artifact_name = artefact.display_name().to_string();
        let app_name = self.app_name(&build.app_id).to_string();
        let workflow_name = build.workflow_display().to_string();
        let build_index = build.index;

        let Some(client) = self.api_client.clone() else {
            return;
        };
        self.artifact_message = Some(format!("Downloading {artifact_name}…"));
        let tx = self.tx.clone();
        tokio::spawn(async move {
            let result = download_artifact(
                client,
                artifact_url,
                app_name,
                workflow_name,
                build_index,
                artifact_name.clone(),
            )
            .await;
            let _ = tx
                .send(AppMessage::ArtifactDownloaded {
                    name: artifact_name,
                    result,
                })
                .await;
        });
    }

    // ── APK from AAB ──────────────────────────────────────────────────────────

    fn do_convert_aab(&mut self) {
        let Some(build) = self.detail_build.as_ref() else {
            return;
        };
        let aab = build.artefacts.iter().find(|a| a.is_aab()).cloned();
        let Some(aab) = aab else {
            self.apk_message = Some("No AAB artefact found in this build.".into());
            return;
        };
        // Capture the same path components used by regular artifact downloads.
        let app_name = self.app_name(&build.app_id).to_string();
        let workflow_name = build.workflow_display().to_string();
        let build_index = build.index;
        let Some(client) = self.api_client.clone() else {
            return;
        };
        let tx = self.tx.clone();
        self.apk_message = Some("Starting…".into());
        tokio::spawn(async move {
            let result = convert_aab_to_apk(
                client,
                aab,
                app_name,
                workflow_name,
                build_index,
                tx.clone(),
            )
            .await;
            let _ = tx.send(AppMessage::ApkReady(result)).await;
        });
    }

    // ── Logs ──────────────────────────────────────────────────────────────────

    fn open_log_steps(&mut self) {
        self.build_popup = Some(BuildPopup::LogSteps);
        self.log_step_index = 0;

        let needs_fetch = self
            .detail_build
            .as_ref()
            .map(|b| {
                self.selected_build()
                    .map(|sel| b.id != sel.id)
                    .unwrap_or(true)
            })
            .unwrap_or(true);

        if needs_fetch {
            self.fetch_build_detail();
        }
    }

    fn load_selected_step_log(&mut self) {
        let Some(build) = self.detail_build.as_ref() else {
            return;
        };
        let Some(step) = build.build_actions.get(self.log_step_index) else {
            return;
        };
        let Some(log_url) = step.log_url.clone() else {
            self.log_lines = vec!["This step has no log URL.".into()];
            self.log_scroll = 0;
            self.build_popup = Some(BuildPopup::LogContent);
            return;
        };
        let client = match self.api_client.clone() {
            Some(c) => c,
            None => return,
        };
        self.log_loading = true;
        self.log_lines = vec!["Loading…".into()];
        self.log_scroll = 0;
        self.build_popup = Some(BuildPopup::LogContent);
        let tx = self.tx.clone();
        tokio::spawn(async move {
            let result = client.fetch_log(&log_url).await;
            let _ = tx.send(AppMessage::LogContentLoaded(result)).await;
        });
    }

    // ─── Common detail fetch ──────────────────────────────────────────────────

    fn fetch_build_detail(&mut self) {
        let Some(build_id) = self.selected_build().map(|b| b.id.clone()) else {
            return;
        };
        let Some(client) = self.api_client.clone() else {
            return;
        };
        self.detail_loading = true;
        self.detail_build = None;
        let tx = self.tx.clone();
        tokio::spawn(async move {
            let result = client.get_build(&build_id).await.map(|r| r.build);
            let _ = tx.send(AppMessage::BuildDetailLoaded(result)).await;
        });
    }

    // ─── App / workflow ID browser ─────────────────────────────────────────────

    pub fn open_app_info(&mut self) {
        self.app_info_open = true;
        self.app_info_scroll = 0;
        self.app_info_selected = 0;
        self.app_info_copy_msg = None;
        // Reuse the same app list as the new-build wizard; fetch if not loaded.
        if self.new_build_apps.is_empty() && !self.new_build_apps_loading {
            if let Some(client) = self.api_client.clone() {
                self.new_build_apps_loading = true;
                let tx = self.tx.clone();
                tokio::spawn(async move {
                    let result = client.get_apps().await;
                    let _ = tx.send(AppMessage::AppsLoaded(result)).await;
                });
            }
        }
    }

    fn handle_app_info_key(&mut self, key: KeyEvent) {
        let entries = self.build_info_entries();
        // Indices (into `entries`) of rows that carry a selectable ID.
        let selectable: Vec<usize> = entries
            .iter()
            .enumerate()
            .filter_map(|(i, e)| e.selectable_id().map(|_| i))
            .collect();
        let sel_count = selectable.len();

        match key.code {
            KeyCode::Esc | KeyCode::Char('q') => {
                self.app_info_open = false;
                self.app_info_copy_msg = None;
            }
            KeyCode::Up | KeyCode::Char('k') => {
                if self.app_info_selected > 0 {
                    self.app_info_selected -= 1;
                    if let Some(&li) = selectable.get(self.app_info_selected) {
                        self.ensure_app_info_visible(li);
                    }
                }
            }
            KeyCode::Down | KeyCode::Char('j') => {
                if self.app_info_selected + 1 < sel_count {
                    self.app_info_selected += 1;
                    if let Some(&li) = selectable.get(self.app_info_selected) {
                        self.ensure_app_info_visible(li);
                    }
                }
            }
            KeyCode::Enter | KeyCode::Char('y') => {
                if let Some(&li) = selectable.get(self.app_info_selected) {
                    if let Some(entry) = entries.get(li) {
                        if let Some(id) = entry.selectable_id() {
                            let label = entry.copy_label();
                            match copy_to_clipboard(id) {
                                Ok(()) => {
                                    self.app_info_copy_msg = Some(format!("✓ Copied {label}…"));
                                }
                                Err(e) => {
                                    self.app_info_copy_msg =
                                        Some(format!("✗ Clipboard error: {e}"));
                                }
                            }
                        }
                    }
                }
            }
            KeyCode::PageUp => {
                self.app_info_scroll = self.app_info_scroll.saturating_sub(10);
            }
            KeyCode::PageDown => {
                let max = entries.len().saturating_sub(1);
                self.app_info_scroll = (self.app_info_scroll + 10).min(max);
            }
            _ => {}
        }
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

    /// Total rendered lines (= entries count). Used to clamp scroll.
    #[allow(dead_code)]
    pub fn app_info_line_count(&self) -> usize {
        self.build_info_entries().len()
    }

    /// Adjusts scroll so `line_idx` is within the visible window.
    fn ensure_app_info_visible(&mut self, line_idx: usize) {
        const VISIBLE_H: usize = 26;
        if line_idx < self.app_info_scroll {
            self.app_info_scroll = line_idx;
        } else if line_idx >= self.app_info_scroll + VISIBLE_H {
            self.app_info_scroll = line_idx.saturating_sub(VISIBLE_H - 1);
        }
    }

    // ─── Settings dialog ────────────────────────────────────────────────────

    pub fn open_settings(&mut self) {
        // Pre-populate with the currently active token so the user can see it.
        self.settings_token_input = self
            .api_client
            .as_ref()
            .map(|c| c.api_token.clone())
            .unwrap_or_default();
        self.settings_loading = false;
        self.settings_error = None;
        self.settings_success = None;
        self.settings_open = true;
    }

    fn handle_settings_key(&mut self, key: KeyEvent) {
        if self.settings_loading {
            return;
        }
        match key.code {
            KeyCode::Esc => {
                self.settings_open = false;
                self.settings_error = None;
                self.settings_success = None;
            }
            KeyCode::Enter => self.submit_settings(),
            KeyCode::Char(c) => {
                self.settings_token_input.push(c);
                self.settings_error = None;
                self.settings_success = None;
            }
            KeyCode::Backspace => {
                self.settings_token_input.pop();
                self.settings_error = None;
                self.settings_success = None;
            }
            _ => {}
        }
    }

    fn submit_settings(&mut self) {
        let token = self.settings_token_input.trim().to_string();
        if token.is_empty() {
            self.settings_error = Some("Token cannot be empty.".into());
            return;
        }
        self.settings_loading = true;
        self.settings_error = None;
        self.settings_success = None;
        let tx = self.tx.clone();
        tokio::spawn(async move {
            let client = ApiClient::new(token);
            let result = client.validate_token().await;
            let _ = tx.send(AppMessage::SettingsTokenValidated(result)).await;
        });
    }

    // ─── Builds list actions ─────────────────────────────────────────────────

    fn submit_onboarding(&mut self) {
        if self.api_token_input.trim().is_empty() {
            self.onboarding_error = Some("Please enter your API token.".to_string());
            return;
        }
        self.onboarding_loading = true;
        self.onboarding_error = None;
        let tx = self.tx.clone();
        let token = self.api_token_input.trim().to_string();
        tokio::spawn(async move {
            let client = ApiClient::new(token);
            let result = client.validate_token().await;
            let _ = tx.send(AppMessage::TokenValidated(result)).await;
        });
    }

    pub fn fetch_builds(&mut self) {
        let Some(client) = self.api_client.clone() else {
            return;
        };
        self.loading_state = LoadingState::Loading;
        let tx = self.tx.clone();
        let skip = self.skip;
        let wf = self.workflow_filter.clone();
        tokio::spawn(async move {
            let result = client.get_builds(skip, wf.as_deref(), None).await;
            let _ = tx.send(AppMessage::BuildsLoaded(result)).await;
        });
    }

    pub fn load_more(&mut self) {
        if !self.has_more || matches!(self.loading_state, LoadingState::Loading) {
            return;
        }
        self.skip = self.builds.len();
        self.fetch_builds();
    }

    pub fn refresh(&mut self) {
        if matches!(self.loading_state, LoadingState::Loading) {
            return;
        }
        self.skip = 0;
        self.builds.clear();
        self.selected_index = 0;
        self.has_more = true;
        self.fetch_builds();
    }

    fn open_filter_popup(&mut self) {
        self.show_filter_popup = true;
        self.filter_selected_index = match &self.workflow_filter {
            None => 0,
            Some(wf) => self
                .available_workflows
                .iter()
                .position(|(id, _)| id == wf)
                .map(|i| i + 1)
                .unwrap_or(0),
        };
    }

    fn confirm_filter(&mut self) {
        let new_filter = if self.filter_selected_index == 0 {
            None
        } else {
            self.available_workflows
                .get(self.filter_selected_index - 1)
                .map(|(id, _)| id.clone())
        };
        self.show_filter_popup = false;
        if new_filter != self.workflow_filter {
            self.workflow_filter = new_filter;
            self.skip = 0;
            self.builds.clear();
            self.selected_index = 0;
            self.has_more = true;
            self.fetch_builds();
        }
    }

    fn move_selection_up(&mut self) {
        if self.selected_index > 0 {
            self.selected_index -= 1;
        }
    }

    fn move_selection_down(&mut self) {
        if !self.builds.is_empty() && self.selected_index + 1 < self.builds.len() {
            self.selected_index += 1;
        }
    }

    fn move_filter_up(&mut self) {
        if self.filter_selected_index > 0 {
            self.filter_selected_index -= 1;
        }
    }

    fn move_filter_down(&mut self) {
        if self.filter_selected_index < self.available_workflows.len() {
            self.filter_selected_index += 1;
        }
    }

    // ─── Helpers ─────────────────────────────────────────────────────────────

    fn update_available_workflows(&mut self) {
        let mut seen: HashSet<String> = HashSet::new();
        let mut workflows: Vec<(String, String)> = Vec::new();
        for build in &self.builds {
            if let Some(id) = build.effective_workflow_id() {
                if seen.insert(id.to_string()) {
                    workflows.push((id.to_string(), build.workflow_display().to_string()));
                }
            }
        }
        self.available_workflows = workflows;
    }

    // ─── Public getters ───────────────────────────────────────────────────────

    pub fn app_name(&self, app_id: &str) -> &str {
        self.applications
            .get(app_id)
            .map(|a| a.name.as_str())
            .unwrap_or("Unknown App")
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

    /// Number of builds in the list that are currently in a running state.
    /// Used by the UI to show the live-build badge.
    pub fn running_build_count(&self) -> usize {
        self.builds
            .iter()
            .filter(|b| is_running_status(&b.status))
            .count()
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
}

/// Returns `true` for any build status that means the build is still in progress.
pub fn is_running_status(status: &str) -> bool {
    matches!(
        status,
        "building" | "queued" | "preparing" | "fetching" | "testing" | "publishing" | "finishing"
    )
}

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

async fn convert_aab_to_apk(
    client: ApiClient,
    artefact: Artefact,
    app_name: String,
    workflow_name: String,
    build_index: Option<u32>,
    tx: mpsc::Sender<AppMessage>,
) -> Result<PathBuf> {
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
        anyhow::bail!("bundletool failed: {stderr}");
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
        anyhow::bail!("unzip failed: {stderr}");
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
async fn ensure_bundletool(tx: &mpsc::Sender<AppMessage>) -> Result<BundletoolCmd> {
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
        anyhow::bail!(
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
async fn download_artifact(
    client: ApiClient,
    artifact_url: String,
    app_name: String,
    workflow_name: String,
    build_index: Option<u32>,
    artifact_name: String,
) -> Result<PathBuf> {
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

// ─── Clipboard ──────────────────────────────────────────────────────────────

fn copy_to_clipboard(text: &str) -> Result<(), String> {
    arboard::Clipboard::new()
        .map_err(|e| e.to_string())?
        .set_text(text)
        .map_err(|e| e.to_string())
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
