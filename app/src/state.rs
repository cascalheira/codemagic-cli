//! Shared application state, provided via Dioxus context.

use codemagic_core::{ApiClient, config};
use dioxus::prelude::*;

/// App-wide state. `Copy` because every field is a `Signal` (itself `Copy`),
/// so it can be pulled from context and moved into closures freely.
#[derive(Clone, Copy)]
pub struct AppState {
    /// The saved API token, if any. Empty string means "not onboarded".
    pub token: Signal<String>,
}

impl AppState {
    pub fn new() -> Self {
        let initial = config::load_config()
            .ok()
            .flatten()
            .map(|c| c.api_token)
            .unwrap_or_default();
        Self {
            token: Signal::new(initial),
        }
    }

    pub fn has_token(&self) -> bool {
        !self.token.read().trim().is_empty()
    }

    /// Builds an API client from the current token.
    pub fn client(&self) -> ApiClient {
        ApiClient::new(self.token.read().clone())
    }

    /// Persists a validated token to disk and updates in-memory state.
    pub fn save_token(&mut self, token: String) {
        let cfg = config::Config {
            api_token: token.clone(),
            ..Default::default()
        };
        // Persist best-effort; the in-memory token still drives the UI even if
        // the write fails (e.g. read-only mobile sandbox — handled later).
        let _ = config::save_config(&cfg);
        self.token.set(token);
    }

    pub fn sign_out(&mut self) {
        self.token.set(String::new());
    }
}
