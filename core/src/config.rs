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
    /// Where the GUI window was when it last closed.
    #[serde(default)]
    pub window: Option<WindowState>,
}

/// A remembered window position and size, in logical points.
#[derive(Debug, Deserialize, Serialize, Clone, Copy, PartialEq)]
pub struct WindowState {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

/// A monitor's usable area, in the same coordinate space as [`WindowState`].
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Monitor {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

/// How much of the window has to land on a monitor for the saved position to
/// be worth restoring. Enough to grab and drag, not so much that a mostly
/// off-screen-but-deliberate placement gets overridden.
const MIN_VISIBLE_W: f64 = 200.0;
const MIN_VISIBLE_H: f64 = 60.0;

impl WindowState {
    /// Whether enough of this window would land on one of `monitors`.
    ///
    /// Guards the case where the window was last closed on a display that is
    /// no longer attached — restoring it there would put it somewhere the user
    /// can't reach.
    pub fn is_reachable(&self, monitors: &[Monitor]) -> bool {
        monitors.iter().any(|m| {
            overlap(self.x, self.width, m.x, m.width) >= MIN_VISIBLE_W
                && overlap(self.y, self.height, m.y, m.height) >= MIN_VISIBLE_H
        })
    }

    /// The size, never below the window's own minimum.
    pub fn clamped_size(&self, min_w: f64, min_h: f64) -> (f64, f64) {
        (self.width.max(min_w), self.height.max(min_h))
    }

    /// Whether the numbers are usable at all — a crashed or hand-edited config
    /// can hold NaN, or a zero size that would produce an invisible window.
    pub fn is_sane(&self) -> bool {
        [self.x, self.y, self.width, self.height]
            .iter()
            .all(|v| v.is_finite())
            && self.width >= 1.0
            && self.height >= 1.0
    }
}

/// Length of the overlap between two 1-D spans.
fn overlap(a_start: f64, a_len: f64, b_start: f64, b_len: f64) -> f64 {
    let start = a_start.max(b_start);
    let end = (a_start + a_len).min(b_start + b_len);
    (end - start).max(0.0)
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

/// Stores the window geometry, leaving every other setting alone.
///
/// Best-effort: a failed write costs the remembered position, nothing more.
pub fn save_window_state(state: WindowState) {
    let mut config = match load_config() {
        Ok(Some(c)) => c,
        // With no config there is no token either, so there's nothing to
        // remember a window for yet.
        _ => return,
    };
    config.window = Some(state);
    let _ = save_config(&config);
}

#[cfg(test)]
mod tests {
    use super::{Monitor, WindowState};

    const LAPTOP: Monitor = Monitor { x: 0.0, y: 0.0, width: 1440.0, height: 900.0 };
    const EXTERNAL: Monitor = Monitor { x: 1440.0, y: 0.0, width: 2560.0, height: 1440.0 };

    fn win(x: f64, y: f64) -> WindowState {
        WindowState { x, y, width: 1180.0, height: 760.0 }
    }

    #[test]
    fn a_window_on_the_main_display_is_reachable() {
        assert!(win(100.0, 100.0).is_reachable(&[LAPTOP]));
    }

    /// The case this exists for: closed on an external display, reopened
    /// after unplugging it.
    #[test]
    fn a_window_on_a_detached_display_is_not_reachable() {
        let on_external = win(2000.0, 300.0);
        assert!(on_external.is_reachable(&[LAPTOP, EXTERNAL]));
        assert!(!on_external.is_reachable(&[LAPTOP]));
    }

    #[test]
    fn a_mostly_offscreen_window_still_counts_if_enough_is_grabbable() {
        // 300pt of width and all of its height remain on the laptop screen.
        assert!(win(1140.0, 50.0).is_reachable(&[LAPTOP]));
        // Only 40pt left: not enough to grab.
        assert!(!win(1400.0, 50.0).is_reachable(&[LAPTOP]));
    }

    #[test]
    fn a_window_dragged_above_the_top_edge_is_not_reachable() {
        assert!(!win(100.0, -740.0).is_reachable(&[LAPTOP]));
    }

    #[test]
    fn no_monitors_means_nothing_is_reachable() {
        assert!(!win(0.0, 0.0).is_reachable(&[]));
    }

    #[test]
    fn size_never_restores_below_the_minimum() {
        let tiny = WindowState { x: 0.0, y: 0.0, width: 200.0, height: 100.0 };
        assert_eq!(tiny.clamped_size(720.0, 480.0), (720.0, 480.0));
        assert_eq!(win(0.0, 0.0).clamped_size(720.0, 480.0), (1180.0, 760.0));
    }

    #[test]
    fn nonsense_geometry_is_rejected() {
        assert!(win(0.0, 0.0).is_sane());
        for bad in [
            WindowState { x: f64::NAN, y: 0.0, width: 800.0, height: 600.0 },
            WindowState { x: 0.0, y: f64::INFINITY, width: 800.0, height: 600.0 },
            WindowState { x: 0.0, y: 0.0, width: 0.0, height: 600.0 },
            WindowState { x: 0.0, y: 0.0, width: 800.0, height: 0.0 },
        ] {
            assert!(!bad.is_sane(), "{bad:?}");
        }
    }
}
