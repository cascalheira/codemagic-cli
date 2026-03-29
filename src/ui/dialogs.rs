use super::build_popup::render_info_entry;
use super::*;
use crate::app::InfoEntry;

// ─── App / workflow ID browser ────────────────────────────────────────────────

pub(super) fn draw_app_info(f: &mut Frame, app: &App) {
    let area = centered_popup(f, 72, 30);
    let block = popup_block("App & Workflow IDs");
    let inner = block.inner(area);
    f.render_widget(block, area);

    let layout = Layout::vertical([
        Constraint::Fill(1),
        Constraint::Length(1), // copy message
        Constraint::Length(1), // hint
    ])
    .split(inner);

    if app.new_build_apps_loading {
        f.render_widget(
            Paragraph::new(" Loading…").style(
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::ITALIC),
            ),
            layout[0],
        );
        f.render_widget(
            Paragraph::new(" [Esc] Close").style(Style::default().fg(Color::DarkGray)),
            layout[2],
        );
        return;
    }

    if app.new_build_apps.is_empty() {
        f.render_widget(
            Paragraph::new(" No apps found.").style(Style::default().fg(Color::DarkGray)),
            layout[0],
        );
        f.render_widget(
            Paragraph::new(" [Esc] Close").style(Style::default().fg(Color::DarkGray)),
            layout[2],
        );
        return;
    }

    // ── Build entries and selection map ──────────────────────────────────────
    let entries = app.build_info_entries();
    let selectable: Vec<usize> = entries
        .iter()
        .enumerate()
        .filter_map(|(i, e)| e.selectable_id().map(|_| i))
        .collect();
    let selected_line_idx: Option<usize> = selectable.get(app.app_info_selected).copied();

    let visible_h = layout[0].height as usize;
    let start = app.app_info_scroll;
    let total = entries.len();
    let end = (start + visible_h).min(total);
    let pct = if total == 0 { 100 } else { end * 100 / total };

    // ── Render each visible entry ───────────────────────────────────────────
    let lines: Vec<Line> = entries
        .iter()
        .enumerate()
        .skip(start)
        .take(visible_h)
        .map(|(idx, entry)| render_info_entry(entry, Some(idx) == selected_line_idx))
        .collect();

    f.render_widget(Paragraph::new(lines), layout[0]);

    // ── Copy status ─────────────────────────────────────────────────────
    if let Some(ref msg) = app.app_info_copy_msg {
        let color = if msg.starts_with('✗') {
            Color::Red
        } else {
            Color::Green
        };
        f.render_widget(
            Paragraph::new(format!(" {msg}")).style(Style::default().fg(color)),
            layout[1],
        );
    }

    f.render_widget(
        Paragraph::new(format!(
            " [↑↓/jk] Select  [Enter/y] Copy ID  [PgUp/PgDn] Scroll  [Esc] Close   {end}/{total} ({pct}%)"
        ))
        .style(Style::default().fg(Color::DarkGray)),
        layout[2],
    );
}

// ─── Settings popup ───────────────────────────────────────────────────────────

pub(super) fn draw_settings(f: &mut Frame, app: &App) {
    let area = centered_popup(f, 60, 14);
    let block = popup_block("Settings");
    let inner = block.inner(area).inner(Margin {
        horizontal: 2,
        vertical: 0,
    });
    f.render_widget(block, area);

    let layout = Layout::vertical([
        Constraint::Length(1), // label
        Constraint::Length(1), // hint where to find the token
        Constraint::Length(1), // blank
        Constraint::Length(3), // token input
        Constraint::Fill(1),
        Constraint::Length(1), // status / error
        Constraint::Length(1), // hint
    ])
    .split(inner);

    f.render_widget(
        Paragraph::new("Codemagic API Token").style(
            Style::default()
                .fg(Color::White)
                .add_modifier(Modifier::BOLD),
        ),
        layout[0],
    );
    f.render_widget(
        Paragraph::new("Find it at: Settings › Integrations › Codemagic API › Show")
            .style(Style::default().fg(Color::DarkGray)),
        layout[1],
    );

    let border_style = if app.settings_loading {
        Style::default().fg(Color::DarkGray)
    } else if app.settings_success.is_some() {
        Style::default().fg(Color::Green)
    } else {
        Style::default().fg(Color::Yellow)
    };

    f.render_widget(
        Paragraph::new(app.settings_token_input.as_str())
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_style(border_style),
            )
            .style(Style::default().fg(Color::White)),
        layout[3],
    );

    // Cursor
    if !app.settings_loading {
        let cx = layout[3].x + 1 + app.settings_token_input.len() as u16;
        let cy = layout[3].y + 1;
        let cx = cx.min(layout[3].x + layout[3].width.saturating_sub(2));
        f.set_cursor_position(Position::new(cx, cy));
    }

    // Status line
    if app.settings_loading {
        f.render_widget(
            Paragraph::new("  Validating…").style(
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::ITALIC),
            ),
            layout[5],
        );
    } else if let Some(ref msg) = app.settings_success {
        f.render_widget(
            Paragraph::new(format!("  {msg}")).style(Style::default().fg(Color::Green)),
            layout[5],
        );
    } else if let Some(ref err) = app.settings_error {
        f.render_widget(
            Paragraph::new(format!("  ✗ {err}")).style(Style::default().fg(Color::Red)),
            layout[5],
        );
    }

    let hint = if app.settings_loading {
        ""
    } else {
        "  [Enter] Save  [Esc] Cancel"
    };
    f.render_widget(
        Paragraph::new(hint).style(Style::default().fg(Color::DarkGray)),
        layout[6],
    );
}

// ── Private helpers ───────────────────────────────────────────────────────────

/// Converts a flat list of apps into un-highlighted display lines.
/// The real rendering path uses `draw_app_info` which calls `render_info_entry`
/// with per-entry selection state; this helper is available for contexts where
/// selection state is not needed.
#[allow(dead_code)]
fn app_info_lines(apps: &[crate::models::Application]) -> Vec<Line<'static>> {
    let mut entries: Vec<InfoEntry> = Vec::new();
    for (i, app) in apps.iter().enumerate() {
        if i > 0 {
            entries.push(InfoEntry::Separator);
        }
        entries.push(InfoEntry::AppName(app.name.clone()));
        entries.push(InfoEntry::AppId(app.id.clone()));
        if app.workflows.is_empty() {
            entries.push(InfoEntry::NoWorkflows);
        } else {
            entries.push(InfoEntry::WorkflowsHeader);
            let mut wfs: Vec<_> = app.workflows.iter().collect();
            wfs.sort_by(|a, b| a.1.name.cmp(&b.1.name));
            for (id, info) in wfs {
                entries.push(InfoEntry::WorkflowRow {
                    name: info.name.clone(),
                    id: id.clone(),
                });
            }
        }
    }
    entries
        .iter()
        .map(|e| render_info_entry(e, false))
        .collect()
}
