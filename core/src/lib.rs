//! Shared Codemagic.io engine: REST API client, data models, and config
//! persistence. Consumed by both the terminal client (`gantry-cli`) and the
//! cross-platform Dioxus GUI (`gantry`).
//!
//! Nothing in this crate depends on a UI toolkit, so it compiles unchanged for
//! macOS, Windows, Linux, iOS, and Android.

pub mod api;
pub mod api_v3;
pub mod bundletool;
pub mod config;
pub mod log;
pub mod models;
pub mod status;
pub mod update;
pub mod web;

pub use api::{ApiClient, PAGE_SIZE};
