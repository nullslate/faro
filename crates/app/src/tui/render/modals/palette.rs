use super::{active_filter_text, centered_rect};
use crate::tui::render::{
    GB_BG2, GB_FG, compact_value, key_style, modal_selection_style, muted_style,
    themed_panel_block, warning_style,
};
use crate::tui::state::WorkbenchState;
use ratatui::layout::{Constraint, Direction, Layout, Margin};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Cell, Clear, Paragraph, Row, Table};

pub(crate) fn render_palette(frame: &mut ratatui::Frame, app: &WorkbenchState) {
    let area = centered_rect(frame.area(), 82, 22);
    let entries = app.filtered_palette_entries();
    frame.render_widget(Clear, area);
    frame.render_widget(
        themed_panel_block(" Command Palette ", Some('P'), true, &app.config.theme),
        area,
    );
    let inner = area.inner(Margin {
        vertical: 1,
        horizontal: 2,
    });
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(2),
            Constraint::Min(8),
            Constraint::Length(1),
        ])
        .split(inner);
    let query = if app.palette_query.is_empty() {
        "type to fuzzy search commands, presets, and views".to_string()
    } else {
        app.palette_query.clone()
    };
    frame.render_widget(
        Paragraph::new(vec![
            Line::from(vec![
                Span::styled("> ", key_style()),
                Span::styled(query, Style::default().fg(app.config.theme.text)),
            ]),
            Line::from(vec![
                Span::styled("matches ", muted_style()),
                Span::raw(entries.len().to_string()),
                Span::styled("  active ", muted_style()),
                Span::raw(app.view.label()),
                Span::styled("  filter ", muted_style()),
                Span::raw(active_filter_text(app)),
            ]),
        ])
        .style(Style::default().fg(GB_FG)),
        chunks[0],
    );
    if entries.is_empty() {
        frame.render_widget(
            Paragraph::new(Line::styled("No commands match.", warning_style()))
                .style(Style::default().fg(GB_FG)),
            chunks[1],
        );
    } else {
        let visible_rows = chunks[1].height.saturating_sub(3).max(1) as usize;
        let visible_start = app.palette_selected.saturating_sub(visible_rows / 2);
        let rows = entries
            .iter()
            .enumerate()
            .skip(visible_start)
            .take(visible_rows)
            .map(|(index, entry)| {
                let selected = index == app.palette_selected;
                let title_style = if selected {
                    Style::default()
                        .fg(app.config.theme.text)
                        .bg(GB_BG2)
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(GB_FG)
                };
                let hint_style = if selected {
                    muted_style().bg(GB_BG2)
                } else {
                    muted_style()
                };
                Row::new([
                    Cell::from(Span::styled(
                        palette_entry_group(entry.title),
                        modal_selection_style(selected),
                    )),
                    Cell::from(Span::styled(palette_entry_name(entry.title), title_style)),
                    Cell::from(Span::styled(compact_value(entry.hint, 36), hint_style)),
                ])
            })
            .collect::<Vec<_>>();
        frame.render_widget(
            Table::new(
                rows,
                [
                    Constraint::Length(12),
                    Constraint::Length(28),
                    Constraint::Min(18),
                ],
            )
            .header(
                Row::new(["SCOPE", "COMMAND", "MATCH"])
                    .style(muted_style().add_modifier(Modifier::BOLD)),
            )
            .style(Style::default().fg(GB_FG))
            .row_highlight_style(Style::default().bg(GB_BG2).fg(app.config.theme.text))
            .highlight_symbol("▎ "),
            chunks[1],
        );
    }
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled("enter", key_style()),
            Span::raw(" run  "),
            Span::styled("esc", key_style()),
            Span::raw(" close  "),
            Span::styled("up/down", key_style()),
            Span::raw(" select"),
        ]))
        .style(Style::default().fg(GB_FG)),
        chunks[2],
    );
}

fn palette_entry_group(title: &str) -> &'static str {
    if title.starts_with("View:") {
        "VIEW"
    } else if title.starts_with("Filter:") {
        "REQUEST"
    } else if title.starts_with("Console") {
        "CONSOLE"
    } else if title.starts_with("WebSocket") {
        "WS"
    } else if title.starts_with("Scripts:") {
        "SCRIPT"
    } else if title.starts_with("Layout:") {
        "LAYOUT"
    } else if title.starts_with("Sort:") {
        "SORT"
    } else if title.starts_with("Debug:") {
        "DEBUG"
    } else {
        "ACTION"
    }
}

fn palette_entry_name(title: &str) -> String {
    title
        .split_once(':')
        .map(|(_, name)| name.trim().to_string())
        .unwrap_or_else(|| title.to_string())
}
