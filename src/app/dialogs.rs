use super::*;

use crossterm::event::{KeyCode, KeyEvent};

use crate::api::ApiClient;

impl App {
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

    pub(crate) fn handle_app_info_key(&mut self, key: KeyEvent) {
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

    /// Adjusts scroll so `line_idx` is within the visible window.
    pub(crate) fn ensure_app_info_visible(&mut self, line_idx: usize) {
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

    pub(crate) fn handle_settings_key(&mut self, key: KeyEvent) {
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

    pub(crate) fn submit_settings(&mut self) {
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
}

// ─── Clipboard ──────────────────────────────────────────────────────────────

fn copy_to_clipboard(text: &str) -> Result<(), String> {
    arboard::Clipboard::new()
        .map_err(|e| e.to_string())?
        .set_text(text)
        .map_err(|e| e.to_string())
}
