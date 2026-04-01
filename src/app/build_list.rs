use super::*;

use crate::api::ApiClient;

impl App {
    pub(crate) fn submit_onboarding(&mut self) {
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

    /// Silently re-fetches the first page from the API without clearing the
    /// currently displayed list or resetting the selection. Used by the
    /// background auto-refresh timer so the UI stays stable between ticks.
    pub fn soft_refresh(&mut self) {
        if matches!(self.loading_state, LoadingState::Loading) {
            return;
        }
        self.skip = 0;
        self.is_soft_refresh = true;
        self.fetch_builds();
    }

    /// Opens the selected build's Codemagic web page in the system browser.
    pub(crate) fn open_selected_build_in_browser(&self) {
        let Some(build) = self.builds.get(self.selected_index) else {
            return;
        };
        let url = format!(
            "https://codemagic.io/app/{}/build/{}",
            build.app_id, build.id
        );
        #[cfg(target_os = "macos")]
        let _ = std::process::Command::new("open").arg(&url).spawn();
        #[cfg(target_os = "linux")]
        let _ = std::process::Command::new("xdg-open").arg(&url).spawn();
        #[cfg(target_os = "windows")]
        let _ = std::process::Command::new("cmd")
            .args(["/c", "start", &url])
            .spawn();
    }

    pub(crate) fn open_filter_popup(&mut self) {
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

    pub(crate) fn confirm_filter(&mut self) {
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

    pub(crate) fn move_selection_up(&mut self) {
        if self.selected_index > 0 {
            self.selected_index -= 1;
        }
    }

    pub(crate) fn move_selection_down(&mut self) {
        if !self.builds.is_empty() && self.selected_index + 1 < self.builds.len() {
            self.selected_index += 1;
        }
    }

    pub(crate) fn move_filter_up(&mut self) {
        if self.filter_selected_index > 0 {
            self.filter_selected_index -= 1;
        }
    }

    pub(crate) fn move_filter_down(&mut self) {
        if self.filter_selected_index < self.available_workflows.len() {
            self.filter_selected_index += 1;
        }
    }
}
