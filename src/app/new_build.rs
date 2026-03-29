use super::*;

impl App {
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

    pub(crate) fn confirm_new_build_app(&mut self) {
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

    pub(crate) fn confirm_new_build_workflow(&mut self) {
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

    pub(crate) fn submit_new_build(&mut self) {
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
}
