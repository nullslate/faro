use super::*;

mod body;
mod content;
mod replay;
mod syntax;
mod tabs;

pub(super) use body::{
    ImageProtocol, is_image_request, is_sse_request, iterm_favicon_escape, kitty_favicon_escape,
    parse_image_data_url, terminal_image_protocol,
};
use body::{
    body_lines, body_preview_lines, image_preview_lines, response_body_lines,
    response_body_panel_lines, sse_body_lines,
};
#[cfg(test)]
pub(super) use body::{parse_sse_events, response_body_content_lines};
use content::detail_lines;
pub(super) use content::{empty_state_lines, labeled_line};
pub(super) use replay::replay_lines;
use replay::replay_summary_lines;
pub(super) use syntax::{
    highlight_javascript_line, syntax_body_lines, syntax_body_lines_for_request,
};
pub(super) use tabs::detail_tab_lines;
#[cfg(test)]
pub(super) use tabs::line_width;

pub(super) fn render_detail(frame: &mut ratatui::Frame, area: Rect, app: &WorkbenchState) {
    let block = themed_panel_block(
        detail_title(app),
        Some('D'),
        app.focus == FocusPane::Detail
            || (app.detail_tab == DetailTab::Replay && app.focus == FocusPane::Body),
        &app.config.theme,
    );
    let inner = block.inner(area);
    frame.render_widget(block, area);
    let tab_lines = detail_tab_lines(app, inner.width);
    let tab_height = tab_lines.len().max(1) as u16;
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(tab_height), Constraint::Min(1)])
        .split(inner);

    frame.render_widget(
        Paragraph::new(tab_lines).style(Style::default().fg(app.config.theme.text)),
        chunks[0],
    );

    let lines = detail_lines(app, chunks[1].width);
    let paragraph = Paragraph::new(lines)
        .style(Style::default().fg(app.config.theme.text))
        .scroll((app.detail_scroll, 0))
        .wrap(Wrap { trim: false });
    frame.render_widget(paragraph, chunks[1]);
}

pub(super) fn render_replay_workspace(
    frame: &mut ratatui::Frame,
    area: Rect,
    app: &WorkbenchState,
) {
    let block = themed_panel_block(
        detail_title(app),
        Some('D'),
        app.focus == FocusPane::Detail || app.focus == FocusPane::Body,
        &app.config.theme,
    );
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let tab_lines = detail_tab_lines(app, inner.width);
    let tab_height = tab_lines.len().max(1) as u16;
    let outer = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(tab_height), Constraint::Min(1)])
        .split(inner);

    frame.render_widget(
        Paragraph::new(tab_lines).style(Style::default().fg(app.config.theme.text)),
        outer[0],
    );

    let summary = replay_summary_lines(app, outer[1].width, usize::from(outer[1].height));
    frame.render_widget(
        Paragraph::new(summary)
            .style(Style::default().fg(app.config.theme.text))
            .wrap(Wrap { trim: true }),
        outer[1],
    );
}

pub(super) fn render_body(frame: &mut ratatui::Frame, area: Rect, app: &WorkbenchState) {
    let visible_rows = area.height.saturating_sub(2) as usize;
    let panel = response_body_panel_lines(app, visible_rows);
    let paragraph = Paragraph::new(panel.lines)
        .block(themed_panel_block(
            response_body_title(app),
            Some('B'),
            app.focus == FocusPane::Body,
            &app.config.theme,
        ))
        .style(Style::default().fg(app.config.theme.text))
        .scroll((
            if panel.pre_scrolled {
                0
            } else {
                app.body_scroll
            },
            0,
        ))
        .wrap(Wrap { trim: false });
    frame.render_widget(paragraph, area);
}

fn detail_title(app: &WorkbenchState) -> String {
    let Some(request) = app.selected_request() else {
        return " Request Detail ".to_string();
    };
    format!(
        " Detail {} {} ",
        request.request.method,
        compact_value(&path_for_url(&request.request.url), 48)
    )
}
