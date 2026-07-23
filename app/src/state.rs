//! Shared application state, provided via Dioxus context.

use dioxus::prelude::*;
use gantry_core::{ApiClient, api_v3::V3Client, config};

/// Default auto-refresh interval (seconds) when none is configured.
pub const DEFAULT_REFRESH_SECS: u64 = 30;
/// Minimum allowed auto-refresh interval.
pub const MIN_REFRESH_SECS: u64 = 5;

/// App-wide state. `Copy` because every field is a `Signal` (itself `Copy`),
/// so it can be pulled from context and moved into closures freely.
#[derive(Clone, Copy)]
pub struct AppState {
    /// The saved API token, if any. Empty string means "not onboarded".
    pub token: Signal<String>,
    /// How often to auto-refresh the builds list (seconds).
    pub refresh_secs: Signal<u64>,
    /// Whether to look for a newer release on startup.
    pub check_updates: Signal<bool>,
}

impl AppState {
    pub fn new() -> Self {
        let cfg = config::load_config().ok().flatten().unwrap_or_default();
        Self {
            token: Signal::new(cfg.api_token.clone()),
            refresh_secs: Signal::new(
                cfg.refresh_interval_secs
                    .unwrap_or(DEFAULT_REFRESH_SECS)
                    .max(MIN_REFRESH_SECS),
            ),
            check_updates: Signal::new(cfg.check_for_updates.unwrap_or(true)),
        }
    }

    pub fn has_token(&self) -> bool {
        !self.token.read().trim().is_empty()
    }

    /// Builds a v1 API client from the current token.
    pub fn client(&self) -> ApiClient {
        ApiClient::new(self.token.read().clone())
    }

    /// Builds a v3 API client from the current token. Same credentials, a
    /// different host — see [`gantry_core::api_v3`].
    pub fn v3_client(&self) -> V3Client {
        V3Client::new(self.token.read().clone())
    }

    /// Re-reads the on-disk config, applies the current in-memory settings on
    /// top (preserving fields we don't manage, like the TUI's poll interval),
    /// and writes it back. Best-effort — a failed write still leaves the UI
    /// driven by in-memory state (e.g. a read-only mobile sandbox).
    fn persist(&self) {
        let mut cfg = config::load_config().ok().flatten().unwrap_or_default();
        cfg.api_token = self.token.read().clone();
        cfg.refresh_interval_secs = Some(*self.refresh_secs.read());
        cfg.check_for_updates = Some(*self.check_updates.read());
        let _ = config::save_config(&cfg);
    }

    pub fn save_token(&mut self, token: String) {
        self.token.set(token);
        self.persist();
    }

    pub fn set_refresh_secs(&mut self, secs: u64) {
        self.refresh_secs.set(secs.max(MIN_REFRESH_SECS));
        self.persist();
    }

    pub fn set_check_updates(&mut self, enabled: bool) {
        self.check_updates.set(enabled);
        self.persist();
    }

    pub fn sign_out(&mut self) {
        self.token.set(String::new());
        self.persist();
    }
}
