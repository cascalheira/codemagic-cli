use chrono::Utc;

use super::*;

use crate::api::PAGE_SIZE;
use crate::config::{self, Config};

impl App {
    pub fn handle_message(&mut self, msg: AppMessage) {
        match msg {
            // ── Builds list ──────────────────────────────────────────────────
            AppMessage::BuildsLoaded(result) => {
                self.loading_state = LoadingState::Idle;
                match result {
                    Ok(response) => {
                        for app in response.applications {
                            self.applications.insert(app.id.clone(), app);
                        }
                        if self.skip == 0 && self.is_soft_refresh {
                            // ── Soft (background) refresh ────────────────────
                            // Merge the fresh first-page results into the
                            // existing list so that any extra pages the user
                            // already loaded are preserved.
                            self.is_soft_refresh = false;

                            // 1. Collect IDs that are already in the list.
                            let existing_ids: std::collections::HashSet<String> =
                                self.builds.iter().map(|b| b.id.clone()).collect();

                            // 2. Update builds that are already visible
                            //    (e.g. status changes for running builds).
                            for new_build in &response.builds {
                                if let Some(b) =
                                    self.builds.iter_mut().find(|b| b.id == new_build.id)
                                {
                                    *b = new_build.clone();
                                }
                            }

                            // 3. Prepend builds that are genuinely new (arrived
                            //    since the last refresh).
                            let prepend: Vec<_> = response
                                .builds
                                .into_iter()
                                .filter(|b| !existing_ids.contains(&b.id))
                                .collect();

                            if !prepend.is_empty() {
                                let prepend_count = prepend.len();
                                let old = std::mem::take(&mut self.builds);
                                self.builds = prepend;
                                self.builds.extend(old);
                                // Shift the cursor down so it stays on the
                                // same build row despite the new entries above.
                                self.selected_index += prepend_count;
                            }

                            self.last_refreshed = Some(Utc::now());
                        } else if self.skip == 0 {
                            // ── Hard (manual / filter) refresh ───────────────
                            self.has_more = response.builds.len() >= PAGE_SIZE;
                            self.builds = response.builds;
                            // Clamp selection so it stays in bounds.
                            self.selected_index =
                                self.selected_index.min(self.builds.len().saturating_sub(1));
                            self.last_refreshed = Some(Utc::now());
                        } else {
                            // ── Load-more page append ────────────────────────
                            self.has_more = response.builds.len() >= PAGE_SIZE;
                            self.builds.extend(response.builds);
                        }
                        self.update_available_workflows();
                        self.status_message = None;
                    }
                    Err(e) => {
                        self.is_soft_refresh = false;
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

            AppMessage::BuildCancelled(result) => {
                match result {
                    Ok(()) => {
                        self.cancel_message = Some("✓ Build cancelled.".into());
                        // Refresh so the status flips to 'canceled' immediately.
                        self.skip = 0;
                        self.is_soft_refresh = true;
                        self.fetch_builds();
                    }
                    Err(e) => {
                        self.cancel_message = Some(format!("✗ Cancel failed: {e}"));
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
}
