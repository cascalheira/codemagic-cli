use super::*;
use crate::app::InfoEntry;
use ratatui::widgets::Wrap;

const ACTIONS: [&str; 2] = ["  Download Artifacts", "  View Build Logs"];

// ── 1. Actions menu ───────────────────────────────────────────────────────────

pub(super) fn draw_build_actions(f: &mut Frame, app: &App) {
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
            .display_build_number()
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

    // ── Help hint ──────────────────────────────────────────────────────────────
    f.render_widget(
        Paragraph::new(Line::from(vec![
            Span::raw(" "),
            Span::styled("[↑↓/jk]", Style::default().fg(Color::Yellow)),
            Span::styled(" Select  ", Style::default().fg(Color::DarkGray)),
            Span::styled("[Enter]", Style::default().fg(Color::Yellow)),
            Span::styled(" Open  ", Style::default().fg(Color::DarkGray)),
            Span::styled("[Esc]", Style::default().fg(Color::Yellow)),
            Span::styled(" Close", Style::default().fg(Color::DarkGray)),
        ])),
        layout[4],
    );
}

// ── 2. Artifacts ──────────────────────────────────────────────────────────────

pub(super) fn draw_artifacts(f: &mut Frame, app: &App) {
    let area = centered_popup(f, 64, 21);
    let block = popup_block("Artifacts");
    let inner = block.inner(area);
    f.render_widget(block, area);

    let layout = Layout::vertical([
        Constraint::Fill(1),
        Constraint::Length(2), // status (2 rows so long errors wrap)
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
                        Cell::from(format!("{name} → APK"))
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
            Paragraph::new(format!(" {msg}"))
                .style(Style::default().fg(color))
                .wrap(Wrap { trim: false }),
            layout[1],
        );
    }

    f.render_widget(
        Paragraph::new(Line::from(vec![
            Span::raw(" "),
            Span::styled("[↑↓/jk]", Style::default().fg(Color::Yellow)),
            Span::styled(" Select  ", Style::default().fg(Color::DarkGray)),
            Span::styled("[Enter]", Style::default().fg(Color::Yellow)),
            Span::styled(
                " Download / Convert  ",
                Style::default().fg(Color::DarkGray),
            ),
            Span::styled("[Esc]", Style::default().fg(Color::Yellow)),
            Span::styled(" Back", Style::default().fg(Color::DarkGray)),
        ])),
        layout[2],
    );
}

// ── 3. Log steps ──────────────────────────────────────────────────────────────

pub(super) fn draw_log_steps(f: &mut Frame, app: &App) {
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
        Paragraph::new(Line::from(vec![
            Span::raw(" "),
            Span::styled("[↑↓/jk]", Style::default().fg(Color::Yellow)),
            Span::styled(" Select  ", Style::default().fg(Color::DarkGray)),
            Span::styled("[Enter]", Style::default().fg(Color::Yellow)),
            Span::styled(" View log  ", Style::default().fg(Color::DarkGray)),
            Span::styled("[Esc]", Style::default().fg(Color::Yellow)),
            Span::styled(" Back", Style::default().fg(Color::DarkGray)),
        ])),
        layout[1],
    );
}

// ── 4. Log content viewer ─────────────────────────────────────────────────────

pub(super) fn draw_log_content(f: &mut Frame, app: &App) {
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

    let mut para = Paragraph::new(lines).style(Style::default().fg(Color::White));
    if app.log_wrap {
        para = para.wrap(Wrap { trim: false });
    }
    f.render_widget(para, layout[0]);

    let total = app.log_lines.len();
    let pct = (((app.log_scroll + visible_h).min(total)) * 100)
        .checked_div(total)
        .unwrap_or(0) as u16;
    f.render_widget(
        Paragraph::new(Line::from(vec![
            Span::raw(" "),
            Span::styled("[↑↓/jk]", Style::default().fg(Color::Yellow)),
            Span::styled(" Scroll  ", Style::default().fg(Color::DarkGray)),
            Span::styled("[PgUp/PgDn]", Style::default().fg(Color::Yellow)),
            Span::styled(" Page  ", Style::default().fg(Color::DarkGray)),
            Span::styled("[g/G]", Style::default().fg(Color::Yellow)),
            Span::styled(" Top/Bottom  ", Style::default().fg(Color::DarkGray)),
            Span::styled("[w]", Style::default().fg(Color::Yellow)),
            Span::styled(
                if app.log_wrap {
                    " Wrap:on  "
                } else {
                    " Wrap:off  "
                },
                Style::default().fg(Color::DarkGray),
            ),
            Span::styled("[Esc]", Style::default().fg(Color::Yellow)),
            Span::styled(
                format!(" Back    line {}/{total} ({pct}%)", app.log_scroll + 1),
                Style::default().fg(Color::DarkGray),
            ),
        ])),
        layout[1],
    );
}

// ── Private helpers ───────────────────────────────────────────────────────────

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

// ── Shared entry renderer (also used by dialogs.rs) ───────────────────────────

/// Renders one `InfoEntry` as a ratatui `Line`, applying a blue highlight when selected.
pub(super) fn render_info_entry(entry: &InfoEntry, selected: bool) -> Line<'static> {
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
