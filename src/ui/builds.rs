use super::*;

pub(super) fn draw_builds(f: &mut Frame, app: &App) {
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

            let build_num = build
                .display_build_number()
                .map(|i| format!("#{i}"))
                .unwrap_or_default();

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

pub(super) fn draw_filter_popup(f: &mut Frame, app: &App) {
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
