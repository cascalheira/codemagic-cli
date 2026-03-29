use super::*;

use super::download::{convert_aab_to_apk, download_artifact};

impl App {
    pub(crate) fn open_build_actions(&mut self) {
        if self.builds.is_empty() {
            return;
        }
        self.build_popup = Some(BuildPopup::Actions);
        self.popup_action_index = 0;
        self.detail_error = None;
        self.artifact_message = None;
        self.apk_message = None;
    }

    pub(crate) fn confirm_build_action(&mut self) {
        match self.popup_action_index {
            0 => self.open_artifacts(),
            1 => self.open_log_steps(),
            _ => {}
        }
    }

    /// Returns the currently selected build (from the list).
    pub(crate) fn selected_build(&self) -> Option<&Build> {
        self.builds.get(self.selected_index)
    }

    // ── Artifacts ─────────────────────────────────────────────────────────────

    pub(crate) fn open_artifacts(&mut self) {
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

    pub(crate) fn download_selected_artifact(&mut self) {
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

    pub(crate) fn do_convert_aab(&mut self) {
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

    pub(crate) fn open_log_steps(&mut self) {
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

    pub(crate) fn load_selected_step_log(&mut self) {
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

    pub(crate) fn fetch_build_detail(&mut self) {
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
}
