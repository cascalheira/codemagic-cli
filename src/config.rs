use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

#[derive(Debug, Deserialize, Serialize, Default, Clone)]
pub struct Config {
    pub api_token: String,
}

/// Returns the path to the config file: `~/.config/codemagic-cli/config.toml`.
pub fn config_path() -> PathBuf {
    let mut path = dirs::config_dir().unwrap_or_else(|| PathBuf::from("."));
    path.push("codemagic-cli");
    path.push("config.toml");
    path
}

/// Loads the config from disk. Returns `None` if the file doesn't exist or
/// the API token is empty.
pub fn load_config() -> Result<Option<Config>> {
    let path = config_path();
    if !path.exists() {
        return Ok(None);
    }
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
