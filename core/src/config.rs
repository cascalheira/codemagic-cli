use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

#[derive(Debug, Deserialize, Serialize, Default, Clone)]
pub struct Config {
    pub api_token: String,
    /// How often to poll running builds for live status updates (seconds).
    /// Defaults to 5 when absent.
    #[serde(default)]
    pub poll_interval_secs: Option<u64>,
    /// How often to silently auto-refresh the full builds list (seconds).
    /// Defaults to 30 when absent.
    #[serde(default)]
    pub refresh_interval_secs: Option<u64>,
    /// Whether to check GitHub for a newer release on startup.
    /// Defaults to enabled when absent.
    #[serde(default)]
    pub check_for_updates: Option<bool>,
}

/// Returns the path to the config file: `~/.config/gantry/config.toml`.
pub fn config_path() -> PathBuf {
    config_dir_named("gantry")
}

/// Pre-rename config location, still read once so upgrading users keep their
/// token. The next save writes to [`config_path`].
fn legacy_config_path() -> PathBuf {
    config_dir_named("codemagic-cli")
}

fn config_dir_named(dir: &str) -> PathBuf {
    let mut path = dirs::config_dir().unwrap_or_else(|| PathBuf::from("."));
    path.push(dir);
    path.push("config.toml");
    path
}

/// Loads the config from disk. Returns `None` if the file doesn't exist or
/// the API token is empty.
///
/// Falls back to the pre-rename location so an existing install isn't signed
/// out by the upgrade.
pub fn load_config() -> Result<Option<Config>> {
    let path = match config_path() {
        p if p.exists() => p,
        _ => match legacy_config_path() {
            p if p.exists() => p,
            _ => return Ok(None),
        },
    };
    let content = fs::read_to_string(&path)
        .with_context(|| format!("Failed to read config from {path:?}"))?;
    let config: Config = toml::from_str(&content).with_context(|| "Failed to parse config file")?;
    if config.api_token.trim().is_empty() {
        return Ok(None);
    }
    Ok(Some(config))
}

/// Persists the config to disk, creating parent directories as needed.
pub fn save_config(config: &Config) -> Result<()> {
    let path = config_path();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("Failed to create config directory {parent:?}"))?;
    }
    let content = toml::to_string_pretty(config).with_context(|| "Failed to serialize config")?;
    fs::write(&path, content).with_context(|| format!("Failed to write config to {path:?}"))?;
    Ok(())
}
