use super::*;

pub(super) fn render(frame: &mut ratatui::Frame, area: Rect, app: &WorkbenchState) {
    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(38), Constraint::Percentage(62)])
        .split(area);

    frame.render_widget(
        Paragraph::new(script_list_lines(app))
            .block(panel_block(
                format!("Scripts {}", app.scripts.len()),
                app.focus == FocusPane::Scripts,
            ))
            .wrap(Wrap { trim: false }),
        chunks[0],
    );
    frame.render_widget(
        Paragraph::new(script_output_lines(app))
            .block(panel_block("Output", app.focus == FocusPane::Scripts))
            .wrap(Wrap { trim: false }),
        chunks[1],
    );
}

fn script_list_lines(app: &WorkbenchState) -> Vec<Line<'static>> {
    if app.scripts.is_empty() {
        return vec![
            Line::styled("no scripts yet", muted_style()),
            Line::raw(""),
            Line::from(vec![
                Span::styled("n", key_style()),
                Span::raw(" new  "),
                Span::styled("p", key_style()),
                Span::raw(" palette"),
            ]),
        ];
    }

    let mut lines = vec![Line::from(vec![
        Span::styled("n", key_style()),
        Span::raw(" new  "),
        Span::styled("e", key_style()),
        Span::raw(" edit  "),
        Span::styled("r", key_style()),
        Span::raw(" run  "),
        Span::styled("R", key_style()),
        Span::raw(" rename  "),
        Span::styled("D", key_style()),
        Span::raw(" dup  "),
        Span::styled("x", key_style()),
        Span::raw(" delete"),
    ])];
    lines.push(Line::raw(""));
    lines.extend(app.scripts.iter().enumerate().map(|(index, script)| {
        let selected = app.script_state.selected() == Some(index);
        let style = if selected {
            Style::default()
                .fg(Color::Black)
                .bg(GB_YELLOW)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(GB_FG)
        };
        Line::from(vec![
            Span::styled(if selected { "> " } else { "  " }, muted_style()),
            Span::styled(script.name.clone(), style),
            Span::styled("  ", muted_style()),
            Span::styled(
                script
                    .last_run_at
                    .map(|ts| format!("last {ts}"))
                    .unwrap_or_else(|| "never run".to_string()),
                muted_style(),
            ),
        ])
    }));
    lines
}

pub(super) fn script_output_lines(app: &WorkbenchState) -> Vec<Line<'static>> {
    let mut lines = Vec::new();
    if let Some(script) = app.selected_script() {
        lines.push(Line::from(vec![
            Span::styled("script ", label_style()),
            Span::raw(script.name.clone()),
        ]));
        lines.push(Line::from(vec![
            Span::styled("updated ", label_style()),
            Span::raw(script.updated_at.to_string()),
            Span::styled("  chars ", label_style()),
            Span::raw(script.body.len().to_string()),
        ]));
    }
    if let Some(status) = &app.script_status {
        lines.push(Line::from(vec![
            Span::styled("status ", label_style()),
            Span::raw(status.clone()),
        ]));
    }
    if let Some(duration) = app.script_duration_ms {
        lines.push(Line::from(vec![
            Span::styled("duration ", label_style()),
            Span::raw(format!("{duration}ms")),
        ]));
    }
    if !lines.is_empty() {
        lines.push(Line::raw(""));
    }
    if let Some(script) = app.selected_script() {
        lines.push(Line::from(vec![
            Span::styled("source ", label_style()),
            Span::styled("e to edit", key_style()),
        ]));
        lines.extend(script_source_lines(&script.body, 80));
        lines.push(Line::raw(""));
    }
    if app.script_output.is_empty() {
        lines.push(Line::styled("no output", muted_style()));
    } else {
        lines.push(Line::styled("output", label_style()));
        lines.extend(app.script_output.iter().map(|line| Line::raw(line.clone())));
    }
    lines
}

fn script_source_lines(source: &str, max_lines: usize) -> Vec<Line<'static>> {
    source
        .lines()
        .take(max_lines)
        .map(highlight_javascript_line)
        .collect()
}
