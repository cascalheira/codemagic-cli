// ─── Re-exports (available to all sub-modules via `use super::*`) ────────────
pub use chrono::{DateTime, Utc};
pub use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Layout, Margin, Position, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{
        Block, Borders, Cell, Clear, List, ListItem, ListState, Paragraph, Row, Table, TableState,
    },
};

pub use crate::app::{App, BuildPopup, LoadingState, NewBuildStep, Screen, is_running_status};
pub use crate::models::BuildAction;

// ─── Sub-modules ─────────────────────────────────────────────────────────────
pub mod build_popup;
pub mod builds;
pub mod dialogs;
pub mod new_build;
pub mod onboarding;

// ─── Entry point ─────────────────────────────────────────────────────────────

pub fn draw(f: &mut Frame, app: &App) {
    match app.screen {
        Screen::Onboarding => onboarding::draw_onboarding(f, app),
        Screen::Builds => {
            builds::draw_builds(f, app);
            if app.help_open {
                dialogs::draw_help(f, app);
            } else if app.app_info_open {
                dialogs::draw_app_info(f, app);
            } else if app.settings_open {
                dialogs::draw_settings(f, app);
            } else if let Some(ref step) = app.new_build_step.clone() {
                new_build::draw_new_build(f, app, step);
            } else if app.show_filter_popup {
                builds::draw_filter_popup(f, app);
            } else if let Some(popup) = &app.build_popup {
                match popup {
                    BuildPopup::Actions => build_popup::draw_build_actions(f, app),
                    BuildPopup::Artifacts => build_popup::draw_artifacts(f, app),
                    BuildPopup::LogSteps => build_popup::draw_log_steps(f, app),
                    BuildPopup::LogContent => build_popup::draw_log_content(f, app),
                }
            }
        }
    }
}

// ─── Shared helpers ───────────────────────────────────────────────────────────

/// Returns the display text and colour for a build status string.
pub(super) fn status_cell(status: &str) -> (String, Style) {
    if is_running_status(status) {
        let s = spinner_frame();
        let label = match status {
            "building" => "Building    ",
            "queued" => "Queued      ",
            "preparing" => "Preparing   ",
            "fetching" => "Fetching    ",
            "initializing" => "Initializing",
            "testing" => "Testing     ",
            "publishing" => "Publishing  ",
            "finishing" => "Finishing   ",
            other => other,
        };
        let style = if status == "building" {
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(Color::Cyan)
        };
        return (format!("{s} {label}"), style);
    }

    match status {
        "finished" => ("✓ Success ".to_string(), Style::default().fg(Color::Green)),
        "failed" => ("✗ Failed  ".to_string(), Style::default().fg(Color::Red)),
        "canceled" | "cancelled" => (
            "⊘ Canceled".to_string(),
            Style::default().fg(Color::DarkGray),
        ),
        "timeout" => ("⏱ Timeout ".to_string(), Style::default().fg(Color::Red)),
        "skipped" => (
            "⏭ Skipped ".to_string(),
            Style::default().fg(Color::DarkGray),
        ),
        "warning" => ("⚠ Warning ".to_string(), Style::default().fg(Color::Yellow)),
        other => (format!("  {other}"), Style::default().fg(Color::DarkGray)),
    }
}

/// Braille-dot spinner frame derived from wall-clock time (no extra state).
/// Cycles every ~1.25 s through 10 frames at 125 ms per frame.
pub(super) fn spinner_frame() -> char {
    const FRAMES: [char; 10] = ['⠋', '⠙', '⠹', '⠸', '⠼', '⠴', '⠦', '⠧', '⠇', '⠏'];
    let idx = (Utc::now().timestamp_millis() / 125) as usize % FRAMES.len();
    FRAMES[idx]
}

/// Formats a UTC datetime as a human-readable "time ago" string.
pub(super) fn format_time_ago(dt: DateTime<Utc>) -> String {
    let now = Utc::now();
    let secs = (now - dt).num_seconds().max(0);

    if secs < 60 {
        format!("{secs}s ago")
    } else if secs < 3600 {
        format!("{}m ago", secs / 60)
    } else if secs < 86_400 {
        format!("{}h ago", secs / 3600)
    } else if secs < 7 * 86_400 {
        format!("{}d ago", secs / 86_400)
    } else {
        dt.format("%Y-%m-%d").to_string()
    }
}

/// Very light ANSI escape stripper (removes `ESC[…m` colour sequences).
pub(super) fn strip_ansi(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\x1b' {
            // Consume everything up to and including the final letter of the
            // CSI sequence.
            if chars.peek() == Some(&'[') {
                chars.next();
                for ch in chars.by_ref() {
                    if ch.is_ascii_alphabetic() {
                        break;
                    }
                }
            }
        } else {
            out.push(c);
        }
    }
    out
}

/// Formats a duration in seconds as a compact human-readable string.
/// e.g. 75 → "1m 15s", 3725 → "1h 02m", 0 → "0s".
pub(super) fn format_duration(secs: i64) -> String {
    if secs < 60 {
        format!("{secs}s")
    } else if secs < 3600 {
        format!("{}m {:02}s", secs / 60, secs % 60)
    } else {
        format!("{}h {:02}m", secs / 3600, (secs % 3600) / 60)
    }
}

/// Shared helper: create a centred overlay rect and clear the background.
pub(super) fn centered_popup(f: &mut Frame, width: u16, height: u16) -> Rect {
    let area = f.area();
    let x = area.width.saturating_sub(width) / 2;
    let y = area.height.saturating_sub(height) / 2;
    let r = Rect::new(x, y, width.min(area.width), height.min(area.height));
    f.render_widget(Clear, r);
    r
}

/// A thin popup block with a cyan border and centred bold title.
pub(super) fn popup_block(title: &str) -> Block<'_> {
    Block::default()
        .title(Line::from(vec![
            Span::raw(" "),
            Span::styled(title, Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(" "),
        ]))
        .title_alignment(Alignment::Center)
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Cyan))
}
