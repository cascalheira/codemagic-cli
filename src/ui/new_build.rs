use super::*;

pub(super) fn draw_new_build(f: &mut Frame, app: &App, step: &NewBuildStep) {
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
                    " No match — press Enter to use \"{}\"",
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
