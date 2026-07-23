//! Shared Codemagic.io engine: REST API client, data models, and config
//! persistence. Consumed by both the terminal client (`codemagic-cli`) and the
//! cross-platform Dioxus GUI (`codemagic-app`).
//!
//! Nothing in this crate depends on a UI toolkit, so it compiles unchanged for
//! macOS, Windows, Linux, iOS, and Android.

pub mod api;
pub mod bundletool;
pub mod config;
pub mod models;
pub mod web;

pub use api::{ApiClient, PAGE_SIZE};
