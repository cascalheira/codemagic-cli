use super::*;

pub(super) fn draw_builds(f: &mut Frame, app: &App) {
    let area = f.area();

    let layout = Layout::vertical([
        Constraint::Length(1), // title bar
        Constraint::Length(1), // filter + status bar
        Constraint::Fill(1),   // table
        Constraint::Length(1), // help bar
    ])
    .split(area);

    // Split the filter+status row into left (filter) and right (status)
    let filter_status = Layout::horizontal([
        Constraint::Fill(1), // filter (left)
        Constraint::Fill(1), // status (right)
    ])
    .split(layout[1]);

    // ── Title bar ──
    f.render_widget(
        Paragraph::new(Line::from(vec![Span::styled(
            " Gantry Builds ",
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        )]))
        .alignment(Alignment::Center),
        layout[0],
    );

    // ── Filter bar (left) ──
    let active_color = |on: bool| {
        if on { Color::Yellow } else { Color::DarkGray }
    };
    let mut filter_spans = vec![
        Span::styled(" Filter: ", Style::default().fg(Color::DarkGray)),
        Span::styled(
            app.active_workflow_name(),
            Style::default().fg(active_color(app.workflow_filter.is_some())),
        ),
    ];
    if app.v3_listing_active() {
        filter_spans.push(Span::styled(" · ", Style::default().fg(Color::DarkGray)));
        filter_spans.push(Span::styled(
            app.active_status_name(),
            Style::default().fg(active_color(app.status_filter.is_some())),
        ));
    }
    filter_spans.extend([
        Span::styled("  ", Style::default().fg(Color::DarkGray)),
        Span::styled("[f]", Style::default().fg(Color::Yellow)),
        Span::styled(" change", Style::default().fg(Color::DarkGray)),
    ]);
    f.render_widget(Paragraph::new(Line::from(filter_spans)), filter_status[0]);

    // ── Status bar (right side of filter row) ──
    let status_right = match &app.loading_state {
        LoadingState::Loading => Line::from(vec![Span::styled(
            "Loading… ",
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::ITALIC),
        )]),
        LoadingState::Error(_) => Line::from(vec![Span::styled(
            format!("✗ {}  ", app.status_message.as_deref().unwrap_or("Error")),
            Style::default().fg(Color::Red),
        )]),
        LoadingState::Idle if app.builds.is_empty() => Line::from(Span::styled(
            "No builds found.  ",
            Style::default().fg(Color::DarkGray),
        )),
        LoadingState::Idle => {
            let total = app.builds.len();
            let live = app.running_build_count();
            let row = if total == 0 {
                0
            } else {
                app.selected_index + 1
            };

            let mut spans = vec![
                Span::styled(format!("{row}/{total}"), Style::default().fg(Color::White)),
                Span::styled(" builds", Style::default().fg(Color::DarkGray)),
            ];

            if app.has_more {
                spans.push(Span::styled("   ", Style::default().fg(Color::DarkGray)));
                spans.push(Span::styled("[l]", Style::default().fg(Color::Yellow)));
                spans.push(Span::styled(
                    " load more",
                    Style::default().fg(Color::DarkGray),
                ));
            } else {
                spans.push(Span::styled(
                    "   · all loaded",
                    Style::default().fg(Color::DarkGray),
                ));
            }

            if live > 0 {
                let s = spinner_frame();
                spans.push(Span::styled(
                    format!("   {s} {live} live"),
                    Style::default()
                        .fg(Color::Yellow)
                        .add_modifier(Modifier::BOLD),
                ));
            }
            if let Some(ts) = app.last_refreshed {
                spans.push(Span::styled(
                    format!("   ↻ {}  ", format_time_ago(ts)),
                    Style::default().fg(Color::DarkGray),
                ));
            } else {
                spans.push(Span::styled("  ", Style::default()));
            }
            Line::from(spans)
        }
    };
    f.render_widget(
        Paragraph::new(status_right).alignment(Alignment::Right),
        filter_status[1],
    );

    // ── Builds table ──
    draw_builds_table(f, app, layout[2]);

    // ── Help bar ──
    f.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(" [↑↓/jk]", Style::default().fg(Color::Yellow)),
            Span::raw(" Navigate  "),
            Span::styled("[Enter]", Style::default().fg(Color::Yellow)),
            Span::raw(" Actions  "),
            Span::styled("[r]", Style::default().fg(Color::Yellow)),
            Span::raw(" Refresh  "),
            Span::styled("[n]", Style::default().fg(Color::Yellow)),
            Span::raw(" New Build  "),
            Span::styled("[i]", Style::default().fg(Color::Yellow)),
            Span::raw(" App IDs  "),
            Span::styled("[o]", Style::default().fg(Color::Yellow)),
            Span::raw(" Open in Browser  "),
            Span::styled("[s]", Style::default().fg(Color::Yellow)),
            Span::raw(" Settings  "),
            Span::styled("[?]", Style::default().fg(Color::Yellow)),
            Span::raw(" Help  "),
            Span::styled("[q]", Style::default().fg(Color::Yellow)),
            Span::raw(" Quit"),
        ])),
        layout[3],
    );
}

pub(super) fn draw_builds_table(f: &mut Frame, app: &App, area: Rect) {
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
        Cell::from("Duration").style(
            Style::default()
                .fg(Color::White)
                .add_modifier(Modifier::BOLD),
        ),
    ])
    .height(1)
    .style(Style::default().bg(Color::DarkGray));

    let rows: Vec<Row> = app
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

            let build_num = build
                .display_build_number()
                .map(|i| format!("#{i}"))
                .unwrap_or_default();

            let duration = match (build.started_at, build.finished_at) {
                (Some(s), Some(e)) => format_duration((e - s).num_seconds().max(0)),
                (Some(s), None) if is_running_status(&build.status) => {
                    format_duration((Utc::now() - s).num_seconds().max(0))
                }
                _ => "-".to_string(),
            };

            Row::new([
                Cell::from(status_text).style(status_style),
                Cell::from(app_name.to_string()),
                Cell::from(workflow.to_string()),
                Cell::from(git_ref),
                Cell::from(build_num).style(Style::default().fg(Color::DarkGray)),
                Cell::from(started),
                Cell::from(duration).style(Style::default().fg(Color::DarkGray)),
            ])
            .height(1)
        })
        .collect();

    let widths = [
        Constraint::Length(15), // status
        Constraint::Fill(2),    // app name
        Constraint::Fill(1),    // workflow
        Constraint::Length(16), // branch/tag
        Constraint::Length(5),  // build #
        Constraint::Length(11), // started
        Constraint::Length(9),  // duration
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

pub(super) fn draw_filter_popup(f: &mut Frame, app: &App) {
    // The status column only exists on the v3 listing; without it the popup
    // stays the single workflow list it has always been.
    let with_status = app.v3_listing_active();

    let rows = app.available_workflows.len().max(if with_status {
        gantry_core::api_v3::BuildStatusFilter::ALL.len()
    } else {
        0
    }) + 1;
    // borders(2) + header(1) + rows + hint(1)
    let popup_height = (rows + 4).min(22) as u16;
    let popup_width = if with_status { 62 } else { 44 };
    let popup_area = centered_popup(f, popup_width, popup_height);

    let block = Block::default()
        .title(Line::from(vec![
            Span::raw(" "),
            Span::styled(
                "Filter Builds",
                Style::default().add_modifier(Modifier::BOLD),
            ),
            Span::raw(" "),
        ]))
        .title_alignment(Alignment::Center)
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Cyan));

    let inner = block.inner(popup_area);
    f.render_widget(block, popup_area);

    let body = Layout::vertical([
        Constraint::Length(1), // column headers
        Constraint::Fill(1),   // lists
        Constraint::Length(1), // hint
    ])
    .split(inner);

    let columns = if with_status {
        Layout::horizontal([Constraint::Percentage(58), Constraint::Percentage(42)]).split(body[1])
    } else {
        Layout::horizontal([Constraint::Percentage(100)]).split(body[1])
    };
    let headers = if with_status {
        Layout::horizontal([Constraint::Percentage(58), Constraint::Percentage(42)]).split(body[0])
    } else {
        Layout::horizontal([Constraint::Percentage(100)]).split(body[0])
    };

    // ── Workflow column ──────────────────────────────────────────────────────
    let workflow_focused = app.filter_column == FilterColumn::Workflow;
    f.render_widget(column_header("Workflow", workflow_focused), headers[0]);

    // Workflow names are only unique within an app, so qualify them once more
    // than one app is in play.
    let multi_app = app
        .available_workflows
        .iter()
        .map(|w| &w.app_id)
        .collect::<std::collections::HashSet<_>>()
        .len()
        > 1;

    let mut items: Vec<ListItem> = vec![filter_row("All Workflows", app.workflow_filter.is_none())];
    for workflow in &app.available_workflows {
        let label = if multi_app {
            format!("{} · {}", app.app_name(&workflow.app_id), workflow.name)
        } else {
            workflow.name.clone()
        };
        items.push(filter_row(
            &label,
            app.workflow_filter.as_ref() == Some(workflow),
        ));
    }
    render_filter_column(
        f,
        columns[0],
        items,
        app.filter_selected_index,
        workflow_focused,
    );

    // ── Status column ────────────────────────────────────────────────────────
    if with_status {
        let status_focused = app.filter_column == FilterColumn::Status;
        f.render_widget(column_header("Status", status_focused), headers[1]);

        let mut items: Vec<ListItem> = vec![filter_row("Any status", app.status_filter.is_none())];
        for status in gantry_core::api_v3::BuildStatusFilter::ALL {
            items.push(filter_row(
                status.label(),
                app.status_filter == Some(status),
            ));
        }
        render_filter_column(
            f,
            columns[1],
            items,
            app.filter_status_index,
            status_focused,
        );
    }

    // ── Help hint ────────────────────────────────────────────────────────────
    let mut hint = vec![Span::raw(" ")];
    if with_status {
        hint.push(Span::styled("[Tab]", Style::default().fg(Color::Yellow)));
        hint.push(Span::styled(
            " Switch column  ",
            Style::default().fg(Color::DarkGray),
        ));
    }
    hint.extend([
        Span::styled("[Enter]", Style::default().fg(Color::Yellow)),
        Span::styled(" Apply  ", Style::default().fg(Color::DarkGray)),
        Span::styled("[Esc]", Style::default().fg(Color::Yellow)),
        Span::styled(" Cancel", Style::default().fg(Color::DarkGray)),
    ]);
    f.render_widget(Paragraph::new(Line::from(hint)), body[2]);
}

/// A filter row, tinted green when it is the filter currently in effect.
fn filter_row(label: &str, active: bool) -> ListItem<'static> {
    ListItem::new(Line::from(vec![Span::styled(
        format!(" {label}"),
        if active {
            Style::default().fg(Color::Green)
        } else {
            Style::default()
        },
    )]))
}

fn column_header(label: &str, focused: bool) -> Paragraph<'static> {
    let style = if focused {
        Style::default()
            .fg(Color::Cyan)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(Color::DarkGray)
    };
    Paragraph::new(Line::from(Span::styled(format!(" {label}"), style)))
}

/// Renders one filter column, dimming the highlight when it is not focused so
/// the two columns' cursors can't be mistaken for each other.
fn render_filter_column(
    f: &mut Frame,
    area: Rect,
    items: Vec<ListItem<'static>>,
    selected: usize,
    focused: bool,
) {
    let highlight = if focused {
        Style::default()
            .bg(Color::Blue)
            .fg(Color::White)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(Color::DarkGray)
    };

    let list = List::new(items)
        .highlight_style(highlight)
        .highlight_symbol(if focused { "▶ " } else { "  " });

    let mut state = ListState::default();
    state.select(Some(selected));
    f.render_stateful_widget(list, area, &mut state);
}
