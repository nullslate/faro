use super::centered_rect;
use crate::tui::render::{
    GB_FG, compact_value, key_style, label_style, muted_style, panel_block, warning_style,
};
use crate::tui::state::{SqlResultsView, WorkbenchState};
use ratatui::layout::{Constraint, Direction, Layout};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Cell, Clear, Paragraph, Row, Table, Wrap};

pub(crate) fn render_sql_results(frame: &mut ratatui::Frame, app: &WorkbenchState) {
    let frame_area = frame.area();
    let width = frame_area.width.saturating_sub(8).clamp(48, 128);
    let height = frame_area.height.saturating_sub(4).clamp(14, 34);
    let area = centered_rect(frame_area, width, height);
    let Some(result) = &app.sql_result else {
        return;
    };

    frame.render_widget(Clear, area);
    if let Some(error) = &result.error {
        let lines = vec![
            Line::from(vec![
                Span::styled("error ", label_style()),
                Span::styled(error.clone(), warning_style()),
            ]),
            Line::raw(""),
            Line::styled("query", label_style()),
            Line::raw(compact_value(&result.query.replace('\n', " "), 180)),
            Line::raw(""),
            Line::from(vec![
                Span::styled("esc", key_style()),
                Span::raw(" close  "),
                Span::styled("p", key_style()),
                Span::raw(" palette"),
            ]),
        ];
        frame.render_widget(
            Paragraph::new(lines)
                .block(panel_block("SQL Results", true))
                .style(Style::default().fg(GB_FG))
                .wrap(Wrap { trim: false }),
            area,
        );
        return;
    }

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(6),
            Constraint::Length(1),
        ])
        .split(area);
    let summary = vec![
        Line::from(vec![
            Span::styled("rows ", label_style()),
            Span::raw(result.rows.len().to_string()),
            Span::styled("  columns ", label_style()),
            Span::raw(result.columns.len().to_string()),
            Span::styled("  duration ", label_style()),
            Span::raw(format!("{}ms", result.duration_ms)),
            Span::styled("  row ", label_style()),
            Span::raw(format!(
                "{}",
                app.sql_row_scroll
                    .saturating_add(1)
                    .min(result.rows.len().max(1))
            )),
            Span::styled("  col ", label_style()),
            Span::raw(format!(
                "{}",
                app.sql_col_scroll
                    .saturating_add(1)
                    .min(result.columns.len().max(1))
            )),
        ]),
        Line::from(vec![
            Span::styled("query ", label_style()),
            Span::raw(compact_value(&result.query.replace('\n', " "), 180)),
        ]),
    ];
    frame.render_widget(
        Paragraph::new(summary)
            .block(panel_block("SQL Results", true))
            .style(Style::default().fg(GB_FG)),
        chunks[0],
    );

    let visible_columns = visible_sql_columns(result, app.sql_col_scroll, chunks[1].width);
    let widths = visible_columns
        .iter()
        .map(|(_, width)| Constraint::Length(*width))
        .collect::<Vec<_>>();
    let header = Row::new(
        visible_columns
            .iter()
            .map(|(index, _)| {
                Cell::from(
                    result
                        .columns
                        .get(*index)
                        .cloned()
                        .unwrap_or_else(|| "-".to_string()),
                )
            })
            .collect::<Vec<_>>(),
    )
    .style(muted_style().add_modifier(Modifier::BOLD));
    let visible_rows = chunks[1].height.saturating_sub(3).max(1) as usize;
    let rows = result
        .rows
        .iter()
        .skip(app.sql_row_scroll)
        .take(visible_rows)
        .map(|row| {
            Row::new(
                visible_columns
                    .iter()
                    .map(|(index, width)| {
                        Cell::from(
                            row.get(*index)
                                .map(|value| compact_value(value, width.saturating_sub(1) as usize))
                                .unwrap_or_default(),
                        )
                    })
                    .collect::<Vec<_>>(),
            )
        })
        .collect::<Vec<_>>();
    frame.render_widget(
        Table::new(rows, widths)
            .header(header)
            .block(panel_block("Rows", false))
            .style(Style::default().fg(GB_FG)),
        chunks[1],
    );
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled("j/k", key_style()),
            Span::raw(" rows  "),
            Span::styled("h/l", key_style()),
            Span::raw(" columns  "),
            Span::styled("g/G", key_style()),
            Span::raw(" top/bottom  "),
            Span::styled("esc", key_style()),
            Span::raw(" close  "),
            Span::styled("p", key_style()),
            Span::raw(" palette"),
        ]))
        .style(Style::default().fg(GB_FG)),
        chunks[2],
    );
}

fn visible_sql_columns(
    result: &SqlResultsView,
    column_scroll: usize,
    area_width: u16,
) -> Vec<(usize, u16)> {
    if result.columns.is_empty() {
        return vec![(0, area_width.saturating_sub(4).max(8))];
    }

    let mut used_width = 0_u16;
    let mut visible = Vec::new();
    let available_width = area_width.saturating_sub(4).max(8);
    for index in column_scroll.min(result.columns.len())..result.columns.len() {
        let width = sql_column_width(result, index);
        if !visible.is_empty() && used_width.saturating_add(width) > available_width {
            break;
        }
        used_width = used_width.saturating_add(width);
        visible.push((index, width));
    }
    if visible.is_empty() {
        let index = column_scroll.min(result.columns.len().saturating_sub(1));
        visible.push((index, sql_column_width(result, index).min(available_width)));
    }
    visible
}

fn sql_column_width(result: &SqlResultsView, index: usize) -> u16 {
    let header_width = result
        .columns
        .get(index)
        .map(|column| column.len())
        .unwrap_or_default();
    let value_width = result
        .rows
        .iter()
        .filter_map(|row| row.get(index))
        .take(80)
        .map(|value| value.len())
        .max()
        .unwrap_or_default();
    header_width.max(value_width).clamp(8, 34) as u16
}
