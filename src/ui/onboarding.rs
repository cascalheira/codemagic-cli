use super::*;

pub(super) fn draw_onboarding(f: &mut Frame, app: &App) {
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
    let masked = "•".repeat(app.api_token_input.len());
    let input = Paragraph::new(masked.as_str())
        .block(
            Block::default()
                .title(" API Token ")
                .borders(Borders::ALL)
                .border_style(input_border),
        )
        .style(Style::default().fg(Color::White));
    f.render_widget(input, rows[4]);

    // Show cursor inside the input box when not loading.
    // Use the real token length so the cursor tracks correctly.
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
