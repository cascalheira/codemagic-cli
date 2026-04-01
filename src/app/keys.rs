use super::*;

use crossterm::event::{KeyCode, KeyEvent};

impl App {
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

    pub(crate) fn handle_build_popup_key(&mut self, popup: BuildPopup, key: KeyEvent) {
        match popup {
            BuildPopup::Actions => match key.code {
                KeyCode::Esc => self.build_popup = None,
                KeyCode::Up | KeyCode::Char('k') if self.popup_action_index > 0 => {
                    self.popup_action_index -= 1;
                }
                KeyCode::Down | KeyCode::Char('j') if self.popup_action_index < 1 => {
                    self.popup_action_index += 1;
                }
                KeyCode::Enter => self.confirm_build_action(),
                _ => {}
            },

            BuildPopup::Artifacts => match key.code {
                KeyCode::Esc | KeyCode::Char('q') => self.build_popup = Some(BuildPopup::Actions),
                KeyCode::Up | KeyCode::Char('k') if self.artifact_index > 0 => {
                    self.artifact_index -= 1;
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
                KeyCode::Up | KeyCode::Char('k') if self.log_step_index > 0 => {
                    self.log_step_index -= 1;
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
                // Jump to top / bottom.
                KeyCode::Char('g') => {
                    self.log_scroll = 0;
                }
                KeyCode::Char('G') => {
                    self.log_scroll = self.log_lines.len().saturating_sub(1);
                }
                _ => {}
            },
        }
    }

    pub(crate) fn handle_filter_popup_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Esc => self.show_filter_popup = false,
            KeyCode::Up | KeyCode::Char('k') => self.move_filter_up(),
            KeyCode::Down | KeyCode::Char('j') => self.move_filter_down(),
            KeyCode::Enter => self.confirm_filter(),
            _ => {}
        }
    }

    pub(crate) fn handle_new_build_key(&mut self, key: KeyEvent) {
        match self.new_build_step.clone() {
            None => {}

            Some(NewBuildStep::SelectApp) => match key.code {
                KeyCode::Esc => self.new_build_step = None,
                KeyCode::Up | KeyCode::Char('k') if self.new_build_app_index > 0 => {
                    self.new_build_app_index -= 1;
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
                        KeyCode::Enter if !self.new_build_workflow_input.trim().is_empty() => {
                            self.new_build_step = Some(NewBuildStep::EnterBranch);
                            self.new_build_branch_filter.clear();
                            self.new_build_branch_list_index = 0;
                            self.new_build_error = None;
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
                        KeyCode::Up | KeyCode::Char('k') if self.new_build_workflow_index > 0 => {
                            self.new_build_workflow_index -= 1;
                        }
                        KeyCode::Down | KeyCode::Char('j')
                            if self.new_build_workflow_index + 1 < total =>
                        {
                            self.new_build_workflow_index += 1;
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
                    KeyCode::Up if self.new_build_branch_list_index > 0 => {
                        self.new_build_branch_list_index -= 1;
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
}
