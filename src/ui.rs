use chrono::{DateTime, Utc};
use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Layout, Margin, Position, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{
        Block, Borders, Cell, Clear, List, ListItem, ListState, Paragraph, Row, Table, TableState,
    },
};

use crate::app::{App, BuildPopup, LoadingState, NewBuildStep, Screen, is_running_status};
use crate::models::BuildAction;

// ─── Entry point ─────────────────────────────────────────────────────────────

pub fn draw(f: &mut Frame, app: &App) {
    match app.screen {
        Screen::Onboarding => draw_onboarding(f, app),
        Screen::Builds => {
            draw_builds(f, app);
            if app.app_info_open {
                draw_app_info(f, app);
            } else if app.settings_open {
                draw_settings(f, app);
            } else if let Some(ref step) = app.new_build_step.clone() {
                draw_new_build(f, app, step);
            } else if app.show_filter_popup {
                draw_filter_popup(f, app);
            } else if let Some(popup) = &app.build_popup {
                match popup {
                    BuildPopup::Actions => draw_build_actions(f, app),
                    BuildPopup::Artifacts => draw_artifacts(f, app),
                    BuildPopup::LogSteps => draw_log_steps(f, app),
                    BuildPopup::LogContent => draw_log_content(f, app),
                }
            }
        }
    }
}

// ─── Onboarding ──────────────────────────────────────────────────────────────

fn draw_onboarding(f: &mut Frame, app: &App) {
    let area = f.area();

    // Centre a card vertically and horizontally.
    let vert = Layout::vertical([
        Constraint::Fill(1),
        Constraint::Length(14),
        Constraint::Fill(1),
    ])
    .split(area);

    let horiz = Layout::horizontal([
        Constraint::Fill(1),
        Constraint::Min(56),
        Constraint::Fill(1),
    ])
    .split(vert[1]);

    let card = horiz[1];

    let border_color = if app.onboarding_loading {
        Color::DarkGray
    } else {
        Color::Cyan
    };

    let block = Block::default()
        .title(Line::from(vec![
            Span::raw(" "),
            Span::styled(
                "Codemagic CLI",
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw(" "),
        ]))
        .title_alignment(Alignment::Center)
        .borders(Borders::ALL)
        .border_style(Style::default().fg(border_color));

    f.render_widget(block, card);

    let inner = card.inner(Margin {
        horizontal: 2,
        vertical: 1,
    });

    let rows = Layout::vertical([
        Constraint::Length(1), // greeting
        Constraint::Length(1), // blank
        Constraint::Length(2), // where-to-find hint
        Constraint::Length(1), // blank
        Constraint::Length(3), // input box
        Constraint::Length(1), // blank
        Constraint::Length(1), // action hint / spinner
        Constraint::Fill(1),   // spacer
        Constraint::Length(1), // error
    ])
    .split(inner);

    // Greeting
    f.render_widget(
        Paragraph::new("Welcome! Enter your Codemagic API token to get started.")
            .style(Style::default().fg(Color::White)),
        rows[0],
    );

    // Where to find hint
    f.render_widget(
        Paragraph::new("Find it at:\n  Settings › Integrations › Codemagic API › Show")
            .style(Style::default().fg(Color::DarkGray)),
        rows[2],
    );

    // Token input box
    let input_border = if app.onboarding_loading {
        Style::default().fg(Color::DarkGray)
    } else {
        Style::default().fg(Color::Yellow)
    };
    let input = Paragraph::new(app.api_token_input.as_str())
        .block(
            Block::default()
                .title(" API Token ")
                .borders(Borders::ALL)
                .border_style(input_border),
        )
        .style(Style::default().fg(Color::White));
    f.render_widget(input, rows[4]);

    // Show cursor inside the input box when not loading.
    if !app.onboarding_loading {
        let cursor_x = rows[4].x + 1 + app.api_token_input.len() as u16;
        let cursor_y = rows[4].y + 1;
        // Clamp so it doesn't overflow the box.
        let cursor_x = cursor_x.min(rows[4].x + rows[4].width.saturating_sub(2));
        f.set_cursor_position(Position::new(cursor_x, cursor_y));
    }

    // Action hint / spinner
    let hint = if app.onboarding_loading {
        Span::styled(
            "  Validating token…",
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::ITALIC),
        )
    } else {
        Span::styled(
            "  Press Enter to continue  ·  Esc to quit",
            Style::default().fg(Color::DarkGray),
        )
    };
    f.render_widget(Paragraph::new(Line::from(hint)), rows[6]);

    // Error message
    if let Some(ref err) = app.onboarding_error {
        f.render_widget(
            Paragraph::new(format!("  ✗ {err}"))
                .style(Style::default().fg(Color::Red).add_modifier(Modifier::BOLD)),
            rows[8],
        );
    }
}

// ─── Builds list ─────────────────────────────────────────────────────────────

fn draw_builds(f: &mut Frame, app: &App) {
    let area = f.area();

    let layout = Layout::vertical([
        Constraint::Length(1), // title bar
        Constraint::Length(1), // filter bar
        Constraint::Fill(1),   // table
        Constraint::Length(1), // status bar
        Constraint::Length(1), // help bar
    ])
    .split(area);

    // ── Title bar ──
    f.render_widget(
        Paragraph::new(Line::from(vec![Span::styled(
            " Codemagic Builds ",
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        )]))
        .alignment(Alignment::Center),
        layout[0],
    );

    // ── Filter bar ──
    let filter_label = app.active_workflow_name();
    let filter_color = if app.workflow_filter.is_some() {
        Color::Yellow
    } else {
        Color::DarkGray
    };
    f.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(" Filter: ", Style::default().fg(Color::DarkGray)),
            Span::styled(filter_label, Style::default().fg(filter_color)),
            Span::styled("  [f] change", Style::default().fg(Color::DarkGray)),
        ])),
        layout[1],
    );

    // ── Builds table ──
    draw_builds_table(f, app, layout[2]);

    // ── Status bar ──
    let status = match &app.loading_state {
        LoadingState::Loading => Line::from(vec![Span::styled(
            " Loading…",
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::ITALIC),
        )]),
        LoadingState::Error(_) => Line::from(vec![Span::styled(
            format!(" ✗ {}", app.status_message.as_deref().unwrap_or("Error")),
            Style::default().fg(Color::Red),
        )]),
        LoadingState::Idle => {
            let total = app.builds.len();
            let more = if app.has_more {
                " (more available)"
            } else {
                ""
            };
            let live = app.running_build_count();

            let mut spans = vec![Span::styled(
                format!(" {total} builds loaded{more}"),
                Style::default().fg(Color::DarkGray),
            )];
            if live > 0 {
                let s = spinner_frame();
                spans.push(Span::styled(
                    format!("   {s} {live} live"),
                    Style::default()
                        .fg(Color::Yellow)
                        .add_modifier(Modifier::BOLD),
                ));
            }
            Line::from(spans)
        }
    };
    f.render_widget(Paragraph::new(status), layout[3]);

    // ── Help bar ──
    f.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(" [↑↓/jk]", Style::default().fg(Color::Yellow)),
            Span::raw(" Navigate  "),
            Span::styled("[Enter]", Style::default().fg(Color::Yellow)),
            Span::raw(" Actions  "),
            Span::styled("[f]", Style::default().fg(Color::Yellow)),
            Span::raw(" Filter  "),
            Span::styled("[l]", Style::default().fg(Color::Yellow)),
            Span::raw(" Load More  "),
            Span::styled("[r]", Style::default().fg(Color::Yellow)),
            Span::raw(" Refresh  "),
            Span::styled("[n]", Style::default().fg(Color::Yellow)),
            Span::raw(" New Build  "),
            Span::styled("[i]", Style::default().fg(Color::Yellow)),
            Span::raw(" App IDs  "),
            Span::styled("[s]", Style::default().fg(Color::Yellow)),
            Span::raw(" Settings  "),
            Span::styled("[q]", Style::default().fg(Color::Yellow)),
            Span::raw(" Quit"),
        ])),
        layout[4],
    );
}

fn draw_builds_table(f: &mut Frame, app: &App, area: Rect) {
    let header = Row::new([
        Cell::from("Status").style(
            Style::default()
                .fg(Color::White)
                .add_modifier(Modifier::BOLD),
        ),
        Cell::from("Application").style(
            Style::default()
                .fg(Color::White)
                .add_modifier(Modifier::BOLD),
        ),
        Cell::from("Workflow").style(
            Style::default()
                .fg(Color::White)
                .add_modifier(Modifier::BOLD),
        ),
        Cell::from("Branch / Tag").style(
            Style::default()
                .fg(Color::White)
                .add_modifier(Modifier::BOLD),
        ),
        Cell::from("#").style(
            Style::default()
                .fg(Color::White)
                .add_modifier(Modifier::BOLD),
        ),
        Cell::from("Started").style(
            Style::default()
                .fg(Color::White)
                .add_modifier(Modifier::BOLD),
        ),
    ])
    .height(1)
    .style(Style::default().bg(Color::DarkGray));

    let mut rows: Vec<Row> = app
        .builds
        .iter()
        .map(|build| {
            let (status_text, status_style) = status_cell(&build.status);
            let app_name = app.app_name(&build.app_id);
            let workflow = build.workflow_display();
            let git_ref = build.git_ref();
            let started = build
                .display_time()
                .map(format_time_ago)
                .unwrap_or_else(|| "-".to_string());

            let build_num = build.index.map(|i| format!("#{i}")).unwrap_or_default();

            Row::new([
                Cell::from(status_text).style(status_style),
                Cell::from(app_name.to_string()),
                Cell::from(workflow.to_string()),
                Cell::from(git_ref),
                Cell::from(build_num).style(Style::default().fg(Color::DarkGray)),
                Cell::from(started),
            ])
            .height(1)
        })
        .collect();

    // Append a non-selectable footer row for load-more hints.
    let footer_row = match &app.loading_state {
        LoadingState::Loading => Some(
            Row::new([Cell::from("  Loading more builds…").style(
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::ITALIC),
            )])
            .height(1),
        ),
        _ if app.has_more => Some(
            Row::new([Cell::from("  Press [l] to load more builds")
                .style(Style::default().fg(Color::DarkGray))])
            .height(1),
        ),
        _ if !app.builds.is_empty() => Some(
            Row::new([Cell::from("  — End of list —").style(Style::default().fg(Color::DarkGray))])
                .height(1),
        ),
        _ => None,
    };
    if let Some(row) = footer_row {
        rows.push(row);
    }

    // Show an empty-state message when there are no builds and we're done loading.
    if app.builds.is_empty() && matches!(app.loading_state, LoadingState::Idle) {
        rows.push(
            Row::new([Cell::from("  No builds found.")])
                .height(1)
                .style(Style::default().fg(Color::DarkGray)),
        );
    }

    let widths = [
        Constraint::Length(13), // status
        Constraint::Fill(2),    // app name
        Constraint::Fill(1),    // workflow
        Constraint::Length(18), // branch/tag
        Constraint::Length(5),  // build #
        Constraint::Length(11), // started
    ];

    let table = Table::new(rows, widths)
        .header(header)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::DarkGray)),
        )
        .row_highlight_style(
            Style::default()
                .bg(Color::Blue)
                .fg(Color::White)
                .add_modifier(Modifier::BOLD),
        );

    let mut state = TableState::default();
    if !app.builds.is_empty() {
        state.select(Some(app.selected_index));
    }

    f.render_stateful_widget(table, area, &mut state);
}

// ─── Workflow filter popup ────────────────────────────────────────────────────

fn draw_filter_popup(f: &mut Frame, app: &App) {
    let area = f.area();

    // +2 for the "All Workflows" entry and borders.
    let popup_height = (app.available_workflows.len() + 4).min(20) as u16;
    let popup_width = 44u16;

    let x = area.width.saturating_sub(popup_width) / 2;
    let y = area.height.saturating_sub(popup_height) / 2;
    let popup_area = Rect::new(x, y, popup_width, popup_height);

    f.render_widget(Clear, popup_area);

    let block = Block::default()
        .title(Line::from(vec![
            Span::raw(" "),
            Span::styled(
                "Filter by Workflow",
                Style::default().add_modifier(Modifier::BOLD),
            ),
            Span::raw(" "),
        ]))
        .title_alignment(Alignment::Center)
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Cyan));

    let inner = block.inner(popup_area);
    f.render_widget(block, popup_area);

    // Build items: first entry is always "All Workflows".
    let mut items: Vec<ListItem> = vec![ListItem::new(Line::from(vec![Span::styled(
        " All Workflows",
        if app.workflow_filter.is_none() {
            Style::default().fg(Color::Green)
        } else {
            Style::default()
        },
    )]))];

    for (id, name) in &app.available_workflows {
        let active = app.workflow_filter.as_deref() == Some(id.as_str());
        items.push(ListItem::new(Line::from(vec![Span::styled(
            format!(" {name}"),
            if active {
                Style::default().fg(Color::Green)
            } else {
                Style::default()
            },
        )])));
    }

    let list = List::new(items)
        .highlight_style(
            Style::default()
                .bg(Color::Blue)
                .fg(Color::White)
                .add_modifier(Modifier::BOLD),
        )
        .highlight_symbol("▶ ");

    let mut list_state = ListState::default();
    list_state.select(Some(app.filter_selected_index));

    f.render_stateful_widget(list, inner, &mut list_state);
}

// ─── Helpers ─────────────────────────────────────────────────────────────────

/// Returns the display text and colour for a build status string.
fn status_cell(status: &str) -> (String, Style) {
    if is_running_status(status) {
        let s = spinner_frame();
        let label = match status {
            "building" => "Building ",
            "queued" => "Queued   ",
            "preparing" => "Preparing",
            "fetching" => "Fetching ",
            "testing" => "Testing  ",
            "publishing" => "Publishng",
            "finishing" => "Finishing",
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
fn spinner_frame() -> char {
    const FRAMES: [char; 10] = ['⠋', '⠙', '⠹', '⠸', '⠼', '⠴', '⠦', '⠧', '⠇', '⠏'];
    let idx = (Utc::now().timestamp_millis() / 125) as usize % FRAMES.len();
    FRAMES[idx]
}

// ─── Build detail popups ─────────────────────────────────────────────────────

/// Shared helper: create a centred overlay rect and clear the background.
fn centered_popup(f: &mut Frame, width: u16, height: u16) -> Rect {
    let area = f.area();
    let x = area.width.saturating_sub(width) / 2;
    let y = area.height.saturating_sub(height) / 2;
    let r = Rect::new(x, y, width.min(area.width), height.min(area.height));
    f.render_widget(Clear, r);
    r
}

/// A thin popup block with a cyan border and centred bold title.
fn popup_block(title: &str) -> Block<'_> {
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

// ── 1. Actions menu ───────────────────────────────────────────────────────────

const ACTIONS: [&str; 2] = ["  Download Artifacts", "  View Build Logs"];

fn draw_build_actions(f: &mut Frame, app: &App) {
    // Split the commit message into up to 3 non-empty lines *before* sizing the
    // popup so we can make the height dynamic.
    let commit_lines: Vec<String> = app
        .builds
        .get(app.selected_index)
        .and_then(|b| b.commit.as_ref())
        .and_then(|c| c.message.as_deref())
        .map(|msg| {
            msg.lines()
                .map(|l| l.trim().to_string())
                .filter(|l| !l.is_empty())
                .take(3)
                .collect()
        })
        .unwrap_or_else(|| vec!["-".to_string()]);

    // Fixed single-line fields: Status, App, Workflow, Branch,
    // Version, Build #, Started, Duration = 8 rows.
    const FIXED_DETAIL: u16 = 8;
    let detail_rows = FIXED_DETAIL + commit_lines.len() as u16;

    // height = border(2) + detail_rows + separator(1) + actions + apk_status(1) + hint(1)
    let h = 2 + detail_rows + 1 + ACTIONS.len() as u16 + 1 + 1;
    let area = centered_popup(f, 58, h);
    let block = popup_block("Build Actions");
    let inner = block.inner(area).inner(Margin {
        horizontal: 1,
        vertical: 0,
    });
    f.render_widget(block, area);

    let layout = Layout::vertical([
        Constraint::Length(detail_rows),
        Constraint::Length(1),                    // separator
        Constraint::Length(ACTIONS.len() as u16), // action list
        Constraint::Length(1),                    // APK status / progress
        Constraint::Length(1),                    // hint
    ])
    .split(inner);

    // ── Build detail rows ───────────────────────────────────────────
    if let Some(build) = app.builds.get(app.selected_index) {
        let app_name = app.app_name(&build.app_id);
        let (status_txt, status_style) = status_cell(&build.status);

        let duration = match (build.started_at, build.finished_at) {
            (Some(s), Some(e)) => {
                let secs = (e - s).num_seconds().max(0);
                if secs < 3600 {
                    format!("{}m {:02}s", secs / 60, secs % 60)
                } else {
                    format!("{}h {:02}m", secs / 3600, (secs % 3600) / 60)
                }
            }
            _ => "-".into(),
        };
        let git_ref = build.git_ref();
        let workflow = build.workflow_display().to_string();
        let started_str = build
            .display_time()
            .map(|t| format!("{} ({})", t.format("%Y-%m-%d %H:%M"), format_time_ago(t)))
            .unwrap_or_else(|| "-".into());
        let version_str = build.version.as_deref().unwrap_or("-").to_string();
        let build_num_str = build
            .index
            .map(|i| format!("#{i}"))
            .unwrap_or_else(|| "-".into());

        // 8 fixed single-line rows.
        let single_fields: &[(&str, &str, Option<Style>)] = &[
            ("Status", &status_txt, Some(status_style)),
            ("App", app_name, None),
            ("Workflow", &workflow, None),
            ("Branch", &git_ref, None),
            (
                "Version",
                &version_str,
                Some(Style::default().fg(Color::Cyan)),
            ),
            (
                "Build #",
                &build_num_str,
                Some(Style::default().fg(Color::DarkGray)),
            ),
            ("Started", &started_str, None),
            ("Duration", &duration, None),
        ];

        let detail_layout = Layout::vertical(
            (0..detail_rows)
                .map(|_| Constraint::Length(1))
                .collect::<Vec<_>>(),
        )
        .split(layout[0]);

        // Single-line fields.
        for (i, (label, value, style)) in single_fields.iter().enumerate() {
            f.render_widget(
                Paragraph::new(Line::from(vec![
                    Span::styled(format!("{label:<9} "), Style::default().fg(Color::DarkGray)),
                    Span::styled(
                        value.to_string(),
                        style.unwrap_or(Style::default().fg(Color::White)),
                    ),
                ])),
                detail_layout[i],
            );
        }

        // Commit message: first line shows the "Commit" label; continuation
        // lines use blank padding of the same width.
        let commit_start = single_fields.len();
        for (j, line) in commit_lines.iter().enumerate() {
            let label = if j == 0 { "Commit" } else { "" };
            f.render_widget(
                Paragraph::new(Line::from(vec![
                    Span::styled(format!("{label:<9} "), Style::default().fg(Color::DarkGray)),
                    Span::styled(line.clone(), Style::default().fg(Color::DarkGray)),
                ])),
                detail_layout[commit_start + j],
            );
        }
    }

    // ── Separator ────────────────────────────────────────────────────
    f.render_widget(
        Block::default()
            .borders(Borders::TOP)
            .border_style(Style::default().fg(Color::DarkGray)),
        layout[1],
    );

    // ── Action list ───────────────────────────────────────────────────
    let items: Vec<ListItem> = ACTIONS
        .iter()
        .map(|a| ListItem::new(Line::from(*a)))
        .collect();

    let list = List::new(items)
        .highlight_style(
            Style::default()
                .bg(Color::Blue)
                .fg(Color::White)
                .add_modifier(Modifier::BOLD),
        )
        .highlight_symbol("▶");

    let mut state = ListState::default();
    state.select(Some(app.popup_action_index));
    f.render_stateful_widget(list, layout[2], &mut state);

    // ── APK status / progress line ──────────────────────────────────
    if let Some(ref msg) = app.apk_message {
        let color = if msg.starts_with('✗') {
            Color::Red
        } else {
            Color::Yellow
        };
        f.render_widget(
            Paragraph::new(format!(" {msg}")).style(Style::default().fg(color)),
            layout[3],
        );
    }

    // ── Help hint ──────────────────────────────────────────────────
    f.render_widget(
        Paragraph::new(" [↑↓/jk] Select  [Enter] Open  [Esc] Close")
            .style(Style::default().fg(Color::DarkGray)),
        layout[4],
    );
}

// ── 2. Artifacts ──────────────────────────────────────────────────────────────

fn draw_artifacts(f: &mut Frame, app: &App) {
    let area = centered_popup(f, 64, 20);
    let block = popup_block("Artifacts");
    let inner = block.inner(area);
    f.render_widget(block, area);

    let layout = Layout::vertical([
        Constraint::Fill(1),
        Constraint::Length(1), // status
        Constraint::Length(1), // hint
    ])
    .split(inner);

    if app.detail_loading {
        f.render_widget(
            Paragraph::new(" Loading artefacts…").style(
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::ITALIC),
            ),
            layout[0],
        );
    } else if let Some(ref err) = app.detail_error {
        f.render_widget(
            Paragraph::new(format!(" ✗ {err}")).style(Style::default().fg(Color::Red)),
            layout[0],
        );
    } else if let Some(build) = &app.detail_build {
        if build.artefacts.is_empty() {
            f.render_widget(
                Paragraph::new(" No artefacts for this build.")
                    .style(Style::default().fg(Color::DarkGray)),
                layout[0],
            );
        } else {
            let header = Row::new([
                Cell::from("Name").style(
                    Style::default()
                        .fg(Color::White)
                        .add_modifier(Modifier::BOLD),
                ),
                Cell::from("Type").style(
                    Style::default()
                        .fg(Color::White)
                        .add_modifier(Modifier::BOLD),
                ),
                Cell::from("Size").style(
                    Style::default()
                        .fg(Color::White)
                        .add_modifier(Modifier::BOLD),
                ),
            ])
            .height(1)
            .style(Style::default().bg(Color::DarkGray));

            let mut rows: Vec<Row> = build
                .artefacts
                .iter()
                .map(|a| {
                    Row::new([
                        Cell::from(a.display_name().to_string()),
                        Cell::from(a.display_type().to_string()),
                        Cell::from(a.display_size()),
                    ])
                    .height(1)
                })
                .collect();

            // Append a "Convert → APK" row when at least one AAB is present.
            let aab_name = build
                .artefacts
                .iter()
                .find(|a| a.is_aab())
                .map(|a| a.display_name().to_string());

            if let Some(ref name) = aab_name {
                rows.push(
                    Row::new([
                        Cell::from(format!("  • Convert {name} → APK  (bundletool)"))
                            .style(Style::default().fg(Color::Yellow)),
                        Cell::from(""),
                        Cell::from(""),
                    ])
                    .height(1),
                );
            }

            let widths = [
                Constraint::Fill(1),
                Constraint::Length(8),
                Constraint::Length(9),
            ];

            let table = Table::new(rows, widths).header(header).row_highlight_style(
                Style::default()
                    .bg(Color::Blue)
                    .fg(Color::White)
                    .add_modifier(Modifier::BOLD),
            );

            let mut state = TableState::default();
            state.select(Some(app.artifact_index));
            f.render_stateful_widget(table, layout[0], &mut state);
        }
    }

    // Show APK conversion progress first, fall back to artifact download status.
    let status_msg = app
        .apk_message
        .as_deref()
        .or(app.artifact_message.as_deref());
    if let Some(msg) = status_msg {
        let color = if msg.starts_with('✗') || msg.starts_with("Error") {
            Color::Red
        } else if msg.starts_with('✓') {
            Color::Green
        } else {
            Color::Yellow
        };
        f.render_widget(
            Paragraph::new(format!(" {msg}")).style(Style::default().fg(color)),
            layout[1],
        );
    }

    f.render_widget(
        Paragraph::new(" [↑↓/jk] Select  [Enter] Download / Convert  [Esc] Back")
            .style(Style::default().fg(Color::DarkGray)),
        layout[2],
    );
}

// ── 4. Log steps ──────────────────────────────────────────────────────────────

fn draw_log_steps(f: &mut Frame, app: &App) {
    let area = centered_popup(f, 60, 18);
    let block = popup_block("Build Logs — Select a Step");
    let inner = block.inner(area);
    f.render_widget(block, area);

    let layout = Layout::vertical([Constraint::Fill(1), Constraint::Length(1)]).split(inner);

    if app.detail_loading {
        f.render_widget(
            Paragraph::new(" Loading build steps…").style(
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::ITALIC),
            ),
            layout[0],
        );
    } else if let Some(ref err) = app.detail_error {
        f.render_widget(
            Paragraph::new(format!(" ✗ {err}")).style(Style::default().fg(Color::Red)),
            layout[0],
        );
    } else if let Some(build) = &app.detail_build {
        if build.build_actions.is_empty() {
            f.render_widget(
                Paragraph::new(" No build steps found.")
                    .style(Style::default().fg(Color::DarkGray)),
                layout[0],
            );
        } else {
            let items: Vec<ListItem> = build
                .build_actions
                .iter()
                .map(|step| {
                    let (icon, style) = step_status_icon(step);
                    ListItem::new(Line::from(vec![
                        Span::styled(format!(" {icon} "), style),
                        Span::raw(step.name.clone()),
                    ]))
                })
                .collect();

            let list = List::new(items)
                .highlight_style(
                    Style::default()
                        .bg(Color::Blue)
                        .fg(Color::White)
                        .add_modifier(Modifier::BOLD),
                )
                .highlight_symbol("▶ ");

            let mut state = ListState::default();
            state.select(Some(app.log_step_index));
            f.render_stateful_widget(list, layout[0], &mut state);
        }
    }

    f.render_widget(
        Paragraph::new(" [↑↓/jk] Select  [Enter] View log  [Esc] Back")
            .style(Style::default().fg(Color::DarkGray)),
        layout[1],
    );
}

// ── 5. Log content viewer ─────────────────────────────────────────────────────

fn draw_log_content(f: &mut Frame, app: &App) {
    let area = f.area();
    let popup = Rect::new(
        2,
        1,
        area.width.saturating_sub(4),
        area.height.saturating_sub(2),
    );
    f.render_widget(Clear, popup);
    let block = popup_block("Log Viewer");
    let inner = block.inner(popup);
    f.render_widget(block, popup);

    let layout = Layout::vertical([Constraint::Fill(1), Constraint::Length(1)]).split(inner);

    let visible_h = layout[0].height as usize;
    let start = app.log_scroll;
    let lines: Vec<Line> = app
        .log_lines
        .iter()
        .skip(start)
        .take(visible_h)
        .map(|l| {
            // Strip the most common ANSI escape sequences for clean display.
            let clean = strip_ansi(l);
            Line::from(Span::raw(clean))
        })
        .collect();

    f.render_widget(
        Paragraph::new(lines).style(Style::default().fg(Color::White)),
        layout[0],
    );

    let total = app.log_lines.len();
    let pct = if total == 0 {
        0
    } else {
        ((app.log_scroll + visible_h).min(total) * 100 / total) as u16
    };
    f.render_widget(
        Paragraph::new(format!(
            " [↑↓/jk] Scroll  [PgUp/PgDn] Page  [Esc] Back    line {}/{total} ({pct}%)",
            app.log_scroll + 1
        ))
        .style(Style::default().fg(Color::DarkGray)),
        layout[1],
    );
}

// ─── Helpers for popups ───────────────────────────────────────────────────────

fn step_status_icon(step: &BuildAction) -> (&'static str, Style) {
    match step.status.as_deref() {
        Some("finished") => ("✓", Style::default().fg(Color::Green)),
        Some("failed") => ("✗", Style::default().fg(Color::Red)),
        Some("building") => ("●", Style::default().fg(Color::Yellow)),
        Some("skipped") => ("⏭", Style::default().fg(Color::DarkGray)),
        Some("canceled") | Some("cancelled") => ("⊘", Style::default().fg(Color::DarkGray)),
        _ => ("○", Style::default().fg(Color::DarkGray)),
    }
}

/// Very light ANSI escape stripper (removes `ESC[…m` colour sequences).
fn strip_ansi(s: &str) -> String {
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

// ─── New-build wizard ─────────────────────────────────────────────────────────────

fn draw_new_build(f: &mut Frame, app: &App, step: &NewBuildStep) {
    match step {
        NewBuildStep::SelectApp => draw_nb_select_app(f, app),
        NewBuildStep::SelectWorkflow => draw_nb_select_workflow(f, app),
        NewBuildStep::EnterBranch => draw_nb_enter_branch(f, app),
    }
}

// ── Step 1: Select App ────────────────────────────────────────────────────────

fn draw_nb_select_app(f: &mut Frame, app: &App) {
    let area = centered_popup(f, 54, 20);
    let block = popup_block("New Build — 1/3  Select App");
    let inner = block.inner(area);
    f.render_widget(block, area);

    let layout = Layout::vertical([Constraint::Fill(1), Constraint::Length(1)]).split(inner);

    if app.new_build_apps_loading {
        f.render_widget(
            Paragraph::new(" Loading apps…").style(
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::ITALIC),
            ),
            layout[0],
        );
    } else if let Some(ref err) = app.new_build_error {
        f.render_widget(
            Paragraph::new(format!(" ✗ {err}")).style(Style::default().fg(Color::Red)),
            layout[0],
        );
    } else if app.new_build_apps.is_empty() {
        f.render_widget(
            Paragraph::new(" No apps found.").style(Style::default().fg(Color::DarkGray)),
            layout[0],
        );
    } else {
        let items: Vec<ListItem> = app
            .new_build_apps
            .iter()
            .map(|a| ListItem::new(Line::from(format!("  {}", a.name))))
            .collect();

        let list = List::new(items)
            .highlight_style(
                Style::default()
                    .bg(Color::Blue)
                    .fg(Color::White)
                    .add_modifier(Modifier::BOLD),
            )
            .highlight_symbol("▶ ");

        let mut state = ListState::default();
        state.select(Some(app.new_build_app_index));
        f.render_stateful_widget(list, layout[0], &mut state);
    }

    f.render_widget(
        Paragraph::new(" [↑↓/jk] Select  [Enter] Next  [Esc] Cancel")
            .style(Style::default().fg(Color::DarkGray)),
        layout[1],
    );
}

// ── Step 2: Select Workflow ─────────────────────────────────────────────────

fn draw_nb_select_workflow(f: &mut Frame, app: &App) {
    let app_name = app
        .new_build_apps
        .get(app.new_build_app_index)
        .map(|a| a.name.as_str())
        .unwrap_or("");

    let area = centered_popup(f, 56, 20);
    let block = popup_block("New Build — 2/3  Select Workflow");
    let inner = block.inner(area).inner(Margin {
        horizontal: 1,
        vertical: 0,
    });
    f.render_widget(block, area);

    let layout = Layout::vertical([
        Constraint::Length(1), // app name label
        Constraint::Length(1), // separator
        Constraint::Fill(1),   // list or text input
        Constraint::Length(1), // error
        Constraint::Length(1), // hint
    ])
    .split(inner);

    f.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled("App: ", Style::default().fg(Color::DarkGray)),
            Span::styled(app_name, Style::default().fg(Color::White)),
        ])),
        layout[0],
    );
    f.render_widget(
        Block::default()
            .borders(Borders::TOP)
            .border_style(Style::default().fg(Color::DarkGray)),
        layout[1],
    );

    let workflows = app.get_new_build_workflows();

    if app.new_build_typing_workflow {
        // ── Manual workflow-ID text field ────────────────────────────────
        let input_area = layout[2];
        let sub = Layout::vertical([
            Constraint::Length(1),
            Constraint::Length(3),
            Constraint::Fill(1),
        ])
        .split(input_area);

        f.render_widget(
            Paragraph::new("Enter workflow ID:").style(Style::default().fg(Color::DarkGray)),
            sub[0],
        );
        f.render_widget(
            Paragraph::new(app.new_build_workflow_input.as_str())
                .block(
                    Block::default()
                        .borders(Borders::ALL)
                        .border_style(Style::default().fg(Color::Yellow)),
                )
                .style(Style::default().fg(Color::White)),
            sub[1],
        );
        // cursor
        let cx = sub[1].x + 1 + app.new_build_workflow_input.len() as u16;
        let cy = sub[1].y + 1;
        let cx = cx.min(sub[1].x + sub[1].width.saturating_sub(2));
        f.set_cursor_position(Position::new(cx, cy));

        if let Some(ref err) = app.new_build_error {
            f.render_widget(
                Paragraph::new(format!(" ✗ {err}")).style(Style::default().fg(Color::Red)),
                layout[3],
            );
        }
        f.render_widget(
            Paragraph::new(" [Enter] Next  [Esc] Back to list")
                .style(Style::default().fg(Color::DarkGray)),
            layout[4],
        );
    } else {
        // ── Workflow list ────────────────────────────────────────────
        let mut items: Vec<ListItem> = workflows
            .iter()
            .map(|(_, name)| ListItem::new(Line::from(format!("  {name}"))))
            .collect();
        // Always offer a manual-entry escape hatch at the bottom.
        items.push(ListItem::new(Line::from(vec![Span::styled(
            "  Enter workflow ID manually…",
            Style::default().fg(Color::DarkGray),
        )])));

        let list = List::new(items)
            .highlight_style(
                Style::default()
                    .bg(Color::Blue)
                    .fg(Color::White)
                    .add_modifier(Modifier::BOLD),
            )
            .highlight_symbol("▶ ");

        let mut state = ListState::default();
        state.select(Some(app.new_build_workflow_index));
        f.render_stateful_widget(list, layout[2], &mut state);

        if let Some(ref err) = app.new_build_error {
            f.render_widget(
                Paragraph::new(format!(" ✗ {err}")).style(Style::default().fg(Color::Red)),
                layout[3],
            );
        }
        f.render_widget(
            Paragraph::new(" [↑↓/jk] Select  [Enter] Next  [Esc] Back")
                .style(Style::default().fg(Color::DarkGray)),
            layout[4],
        );
    }
}

// ── Step 3: Select Branch ───────────────────────────────────────────────

fn draw_nb_enter_branch(f: &mut Frame, app: &App) {
    let area = centered_popup(f, 56, 22);
    let block = popup_block("New Build — 3/3  Select Branch");
    let inner = block.inner(area).inner(Margin {
        horizontal: 1,
        vertical: 0,
    });
    f.render_widget(block, area);

    // ── Context line: App · Workflow ────────────────────────────────
    let layout = Layout::vertical([
        Constraint::Length(1), // context
        Constraint::Length(1), // separator
        Constraint::Length(3), // filter input
        Constraint::Length(1), // small gap / match count
        Constraint::Fill(1),   // branch list
        Constraint::Length(1), // error / submitting
        Constraint::Length(1), // hint
    ])
    .split(inner);

    let sel_app = app
        .new_build_apps
        .get(app.new_build_app_index)
        .map(|a| a.name.as_str())
        .unwrap_or("");
    let wfs_owned = app.get_new_build_workflows();
    let sel_workflow = if app.new_build_typing_workflow {
        app.new_build_workflow_input.as_str()
    } else {
        wfs_owned
            .get(app.new_build_workflow_index)
            .map(|(_, n)| n.as_str())
            .unwrap_or("")
    };
    f.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(sel_app, Style::default().fg(Color::White)),
            Span::styled("  ·  ", Style::default().fg(Color::DarkGray)),
            Span::styled(sel_workflow, Style::default().fg(Color::Cyan)),
        ])),
        layout[0],
    );
    f.render_widget(
        Block::default()
            .borders(Borders::TOP)
            .border_style(Style::default().fg(Color::DarkGray)),
        layout[1],
    );

    // ── Filter input ─────────────────────────────────────────────
    let filter_border = if app.new_build_submitting {
        Style::default().fg(Color::DarkGray)
    } else {
        Style::default().fg(Color::Yellow)
    };
    f.render_widget(
        Paragraph::new(app.new_build_branch_filter.as_str())
            .block(
                Block::default()
                    .title(" Filter ")
                    .borders(Borders::ALL)
                    .border_style(filter_border),
            )
            .style(Style::default().fg(Color::White)),
        layout[2],
    );
    // Blinking cursor inside the filter box.
    if !app.new_build_submitting {
        let cx = layout[2].x + 1 + app.new_build_branch_filter.len() as u16;
        let cy = layout[2].y + 1;
        let cx = cx.min(layout[2].x + layout[2].width.saturating_sub(2));
        f.set_cursor_position(Position::new(cx, cy));
    }

    // ── Match count / custom-branch hint ───────────────────────────
    let branches = app.get_filtered_branches();
    let match_line = if branches.is_empty() {
        if app.new_build_branch_filter.is_empty() {
            Span::styled(
                " No branches — type a name to use it directly",
                Style::default().fg(Color::DarkGray),
            )
        } else {
            Span::styled(
                format!(
                    " No match — press Enter to use “{}”",
                    app.new_build_branch_filter
                ),
                Style::default().fg(Color::Yellow),
            )
        }
    } else {
        Span::styled(
            format!(
                " {} branch{}",
                branches.len(),
                if branches.len() == 1 { "" } else { "es" }
            ),
            Style::default().fg(Color::DarkGray),
        )
    };
    f.render_widget(Paragraph::new(Line::from(match_line)), layout[3]);

    // ── Branch list ──────────────────────────────────────────────
    if !branches.is_empty() {
        let items: Vec<ListItem> = branches
            .iter()
            .map(|b| ListItem::new(Line::from(format!("  {b}"))))
            .collect();

        let list = List::new(items)
            .highlight_style(
                Style::default()
                    .bg(Color::Blue)
                    .fg(Color::White)
                    .add_modifier(Modifier::BOLD),
            )
            .highlight_symbol("▶ ");

        let mut state = ListState::default();
        state.select(Some(
            app.new_build_branch_list_index
                .min(branches.len().saturating_sub(1)),
        ));
        f.render_stateful_widget(list, layout[4], &mut state);
    }

    // ── Error / submitting ────────────────────────────────────────
    if app.new_build_submitting {
        f.render_widget(
            Paragraph::new(" Starting build…").style(
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::ITALIC),
            ),
            layout[5],
        );
    } else if let Some(ref err) = app.new_build_error {
        f.render_widget(
            Paragraph::new(format!(" ✗ {err}")).style(Style::default().fg(Color::Red)),
            layout[5],
        );
    }

    let hint = if app.new_build_submitting {
        ""
    } else {
        " [type] Filter  [↑↓] Navigate  [Enter] Start Build  [Esc] Back"
    };
    f.render_widget(
        Paragraph::new(hint).style(Style::default().fg(Color::DarkGray)),
        layout[6],
    );
}

/// Formats a UTC datetime as a human-readable "time ago" string.
fn format_time_ago(dt: DateTime<Utc>) -> String {
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

// ─── App / workflow ID browser ───────────────────────────────────────────────────

fn draw_app_info(f: &mut Frame, app: &App) {
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

/// Renders one `InfoEntry` as a ratatui `Line`, applying a blue highlight when selected.
fn render_info_entry(entry: &crate::app::InfoEntry, selected: bool) -> Line<'static> {
    use crate::app::InfoEntry;

    // Closure: apply blue background to a style when selected.
    let hi = |s: Style| if selected { s.bg(Color::Blue) } else { s };
    let hi_bold = |s: Style| {
        if selected {
            s.bg(Color::Blue).add_modifier(Modifier::BOLD)
        } else {
            s
        }
    };

    match entry {
        InfoEntry::Separator => Line::from(Span::styled(
            "─".repeat(66),
            Style::default().fg(Color::DarkGray),
        )),

        InfoEntry::AppName(name) => Line::from(Span::styled(
            name.clone(),
            Style::default()
                .fg(Color::White)
                .add_modifier(Modifier::BOLD),
        )),

        InfoEntry::AppId(id) => Line::from(vec![
            Span::styled("  App ID   ", hi(Style::default().fg(Color::DarkGray))),
            Span::styled(id.clone(), hi_bold(Style::default().fg(Color::Yellow))),
        ]),

        InfoEntry::WorkflowsHeader => Line::from(Span::styled(
            "  Workflows",
            Style::default().fg(Color::DarkGray),
        )),

        InfoEntry::WorkflowRow { name, id } => Line::from(vec![
            Span::styled("    • ", hi(Style::default().fg(Color::DarkGray))),
            Span::styled(
                format!("{:<32}", name),
                hi(Style::default().fg(Color::Cyan)),
            ),
            Span::styled(id.clone(), hi_bold(Style::default().fg(Color::Yellow))),
        ]),

        InfoEntry::NoWorkflows => Line::from(vec![
            Span::styled("  Workflows ", Style::default().fg(Color::DarkGray)),
            Span::styled(
                "(none — uses codemagic.yaml)",
                Style::default().fg(Color::DarkGray),
            ),
        ]),
    }
}

// ─── Settings popup ───────────────────────────────────────────────────────────

fn draw_settings(f: &mut Frame, app: &App) {
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
