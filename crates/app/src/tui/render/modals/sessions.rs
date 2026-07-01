use super::centered_rect;
use crate::tui::render::{
    GB_BG2, GB_FG, compact_value, key_style, label_style, muted_style, themed_panel_block,
    warning_style,
};
use crate::tui::state::WorkbenchState;
use faro_core::now_ms;
use ratatui::layout::{Constraint, Direction, Layout, Margin};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Cell, Clear, Paragraph, Row, Table};

pub(crate) fn render_sessions(frame: &mut ratatui::Frame, app: &WorkbenchState) {
    let area = centered_rect(frame.area(), 92, 24);
    frame.render_widget(Clear, area);
    frame.render_widget(
        themed_panel_block(" Sessions ", Some('S'), true, &app.config.theme),
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
    frame.render_widget(
        Paragraph::new(vec![
            Line::from(vec![
                Span::styled("sessions ", label_style()),
                Span::raw(app.sessions.len().to_string()),
                Span::styled("  active ", muted_style()),
                Span::raw(
                    app.active_session_id
                        .as_deref()
                        .map(|id| compact_value(id, 12))
                        .unwrap_or_else(|| "-".to_string()),
                ),
            ]),
            Line::from(vec![
                Span::styled("enter", key_style()),
                Span::raw(" open  "),
                Span::styled("x", key_style()),
                Span::raw(" delete  "),
                Span::styled("esc", key_style()),
                Span::raw(" close"),
            ]),
        ])
        .style(Style::default().fg(GB_FG)),
        chunks[0],
    );

    if app.sessions.is_empty() {
        frame.render_widget(
            Paragraph::new(Line::styled("No sessions captured yet.", warning_style()))
                .style(Style::default().fg(GB_FG)),
            chunks[1],
        );
    } else {
        let selected = app.session_state.selected();
        let visible_rows = chunks[1].height.saturating_sub(3).max(1) as usize;
        let visible_start = selected
            .unwrap_or(0)
            .saturating_sub(visible_rows.saturating_sub(1));
        let rows = app
            .sessions
            .iter()
            .enumerate()
            .skip(visible_start)
            .take(visible_rows)
            .map(|(index, entry)| {
                let is_selected = selected == Some(index);
                let is_active = app.active_session_id.as_deref() == Some(entry.session.id.as_str());
                let base = if is_selected {
                    Style::default().fg(app.config.theme.text).bg(GB_BG2)
                } else {
                    Style::default().fg(GB_FG)
                };
                let muted = if is_selected {
                    muted_style().bg(GB_BG2)
                } else {
                    muted_style()
                };
                Row::new([
                    Cell::from(Span::styled(if is_active { "*" } else { " " }, base)),
                    Cell::from(Span::styled(
                        session_created_label(entry.session.created_at),
                        muted,
                    )),
                    Cell::from(Span::styled(
                        entry.request_count.to_string(),
                        if is_selected {
                            label_style().bg(GB_BG2)
                        } else {
                            label_style()
                        },
                    )),
                    Cell::from(Span::styled(
                        entry.console_error_count.to_string(),
                        if entry.console_error_count > 0 {
                            warning_style()
                        } else {
                            muted
                        },
                    )),
                    Cell::from(Span::styled(entry.replay_count.to_string(), muted)),
                    Cell::from(Span::styled(entry.websocket_count.to_string(), muted)),
                    Cell::from(Span::styled(entry.storage_count.to_string(), muted)),
                    Cell::from(Span::styled(entry.cookie_count.to_string(), muted)),
                    Cell::from(Span::styled(session_title(entry), base)),
                    Cell::from(Span::styled(compact_value(&entry.session.id, 6), muted)),
                ])
            })
            .collect::<Vec<_>>();
        frame.render_widget(
            Table::new(
                rows,
                [
                    Constraint::Length(2),
                    Constraint::Length(12),
                    Constraint::Length(5),
                    Constraint::Length(5),
                    Constraint::Length(4),
                    Constraint::Length(4),
                    Constraint::Length(5),
                    Constraint::Length(4),
                    Constraint::Min(28),
                    Constraint::Length(8),
                ],
            )
            .header(
                Row::new([
                    "", "CREATED", "REQS", "ERRS", "RPL", "WS", "STORE", "CK", "SESSION", "ID",
                ])
                .style(muted_style().add_modifier(Modifier::BOLD)),
            )
            .style(Style::default().fg(GB_FG)),
            chunks[1],
        );
    }

    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled("j/k", key_style()),
            Span::raw(" move  "),
            Span::styled("enter", key_style()),
            Span::raw(" switch  "),
            Span::styled("x", key_style()),
            Span::raw(" delete selected session"),
        ]))
        .style(Style::default().fg(GB_FG)),
        chunks[2],
    );
}

fn session_title(entry: &crate::tui::state::SessionView) -> String {
    let Some(root_url) = entry.session.root_url.as_deref() else {
        return entry
            .session
            .name
            .as_deref()
            .map(|value| compact_value(value, 72))
            .unwrap_or_else(|| "untitled session".to_string());
    };
    let host = host_for_url(root_url);
    let name = entry.session.name.as_deref().unwrap_or_default();
    if name.is_empty() || name == "CDP session" {
        compact_value(&format!("{host}  {root_url}"), 72)
    } else {
        compact_value(&format!("{host}  {name}  {root_url}"), 72)
    }
}

fn host_for_url(url: &str) -> String {
    let without_scheme = url.split_once("://").map(|(_, rest)| rest).unwrap_or(url);
    without_scheme
        .split(['/', '?', '#'])
        .next()
        .filter(|host| !host.is_empty())
        .unwrap_or(url)
        .to_string()
}

fn session_created_label(created_at: i64) -> String {
    if created_at <= 0 {
        return "-".to_string();
    }
    let elapsed_ms = now_ms().saturating_sub(created_at).max(0);
    let elapsed_seconds = elapsed_ms / 1000;
    if elapsed_seconds < 60 {
        return format!("{elapsed_seconds}s ago");
    }
    let elapsed_minutes = elapsed_seconds / 60;
    if elapsed_minutes < 60 {
        return format!("{elapsed_minutes}m ago");
    }
    let elapsed_hours = elapsed_minutes / 60;
    if elapsed_hours < 48 {
        return format!("{elapsed_hours}h ago");
    }
    let elapsed_days = elapsed_hours / 24;
    format!("{elapsed_days}d ago")
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::tui::state::SessionView;
    use faro_core::Session;

    #[test]
    fn session_title_prefers_domain_over_generic_cdp_name() {
        let entry = SessionView {
            session: Session {
                id: "session".to_string(),
                created_at: 0,
                name: Some("CDP session".to_string()),
                root_url: Some("https://api.example.test/path".to_string()),
            },
            request_count: 0,
            console_error_count: 0,
            replay_count: 0,
            websocket_count: 0,
            storage_count: 0,
            cookie_count: 0,
        };

        let title = session_title(&entry);

        assert!(title.starts_with("api.example.test"));
        assert!(title.contains("https://api.example.test/path"));
        assert!(!title.starts_with("CDP session"));
    }

    #[test]
    fn host_for_url_extracts_domain() {
        assert_eq!(
            host_for_url("https://example.test/path?q=1"),
            "example.test"
        );
        assert_eq!(host_for_url("localhost:5173/app"), "localhost:5173");
    }
}
