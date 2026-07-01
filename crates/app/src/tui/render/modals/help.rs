use super::{active_filter_text, centered_rect};
use crate::tui::render::{
    GB_FG, key_style, label_style, modal_section_style, muted_style, panel_title_style,
    themed_panel_block,
};
use crate::tui::state::WorkbenchState;
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Clear, Paragraph, Wrap};

pub(crate) fn render_theme_preview(frame: &mut ratatui::Frame, app: &WorkbenchState) {
    let area = centered_rect(frame.area(), 72, 22);
    let theme = &app.config.theme;
    let lines = vec![
        Line::from(vec![
            Span::styled("Theme Preview", panel_title_style(true)),
            Span::styled("  esc closes", muted_style()),
        ]),
        Line::raw(""),
        theme_swatch("text", theme.text),
        theme_swatch("muted", theme.muted),
        theme_swatch("accent", theme.accent),
        theme_swatch("panel title", theme.panel_title),
        theme_swatch("panel border", theme.panel_border),
        theme_swatch("active border", theme.active_border),
        theme_swatch("tree edge", theme.tree_edge),
        Line::raw(""),
        theme_swatch("ok / 2xx", theme.ok),
        theme_swatch("redirect / 3xx", theme.redirect),
        theme_swatch("client error", theme.client_error),
        theme_swatch("server error", theme.server_error),
        Line::raw(""),
        theme_swatch("xhr/fetch", theme.resource_xhr),
        theme_swatch("image", theme.resource_image),
        theme_swatch("script", theme.resource_script),
        theme_swatch("style", theme.resource_style),
        theme_swatch("sse", theme.resource_sse),
    ];
    frame.render_widget(Clear, area);
    frame.render_widget(
        Paragraph::new(lines)
            .block(themed_panel_block(
                " Theme Preview ",
                Some('T'),
                true,
                &app.config.theme,
            ))
            .style(Style::default().fg(GB_FG))
            .wrap(Wrap { trim: false }),
        area,
    );
}

fn theme_swatch(label: &'static str, color: Color) -> Line<'static> {
    Line::from(vec![
        Span::styled(format!("{label:<14}"), label_style()),
        Span::styled("██".to_string(), Style::default().fg(color)),
        Span::raw("  "),
        Span::styled(format!("{color:?}"), muted_style()),
    ])
}

pub(crate) fn render_help(frame: &mut ratatui::Frame, app: &WorkbenchState) {
    let area = centered_rect(frame.area(), 82, 24);
    let lines = vec![
        Line::from(vec![
            Span::styled("Faro Keys", panel_title_style(true)),
            Span::styled("  press ", muted_style()),
            Span::styled("?", key_style()),
            Span::styled(" or ", muted_style()),
            Span::styled("esc", key_style()),
            Span::styled(" to close", muted_style()),
        ]),
        Line::styled(
            "-".repeat(area.width.saturating_sub(4) as usize),
            muted_style(),
        ),
        Line::from(vec![
            Span::styled("NAV", modal_section_style()),
            Span::raw("      "),
            Span::styled("p", key_style()),
            Span::raw(" palette  "),
            Span::styled("tab", key_style()),
            Span::raw(" focus  "),
            Span::styled("1-6", key_style()),
            Span::raw(" views  "),
            Span::styled("j/k", key_style()),
            Span::raw(" move  "),
            Span::styled("u/d", key_style()),
            Span::raw(" scroll  "),
            Span::styled("g/G", key_style()),
            Span::raw(" top/bottom"),
        ]),
        Line::from(vec![
            Span::styled("NETWORK", modal_section_style()),
            Span::raw("  "),
            Span::styled("h/l", key_style()),
            Span::raw(" detail tabs  "),
            Span::styled("s", key_style()),
            Span::raw(" sort  "),
            Span::styled("S", key_style()),
            Span::raw(" sessions  "),
            Span::styled("f", key_style()),
            Span::raw(" preset  "),
            Span::styled("enter", key_style()),
            Span::raw(" enter route  "),
            Span::styled("space", key_style()),
            Span::raw(" collapse  "),
            Span::styled("backspace", key_style()),
            Span::raw(" up  "),
            Span::styled("c", key_style()),
            Span::raw(" clear visible"),
        ]),
        Line::from(vec![
            Span::styled("CAPTURE", modal_section_style()),
            Span::raw("  "),
            Span::styled("o", key_style()),
            Span::raw(" open browser  "),
            Span::styled("F5", key_style()),
            Span::raw(" refresh page  "),
            Span::styled("e", key_style()),
            Span::raw(" body/editor  "),
            Span::styled("y", key_style()),
            Span::raw(" copy curl  "),
            Span::styled("w", key_style()),
            Span::raw(" save exchange"),
        ]),
        Line::from(vec![
            Span::styled("PANES", modal_section_style()),
            Span::raw("    "),
            Span::styled("R", key_style()),
            Span::raw(" requests  "),
            Span::styled("D", key_style()),
            Span::raw(" detail  "),
            Span::styled("B", key_style()),
            Span::raw(" body"),
        ]),
        Line::from(vec![
            Span::styled("REPLAY", modal_section_style()),
            Span::raw("   "),
            Span::styled("r", key_style()),
            Span::raw(" replay  "),
            Span::styled("p", key_style()),
            Span::raw(" palette for edit replay and diff replay"),
        ]),
        Line::from(vec![
            Span::styled("SCRIPTS", modal_section_style()),
            Span::raw("  "),
            Span::styled("4", key_style()),
            Span::raw(" scripts  "),
            Span::styled("n", key_style()),
            Span::raw(" new  "),
            Span::styled("e", key_style()),
            Span::raw(" edit  "),
            Span::styled("r", key_style()),
            Span::raw(" run  "),
            Span::styled("R", key_style()),
            Span::raw(" rename  "),
            Span::styled("D", key_style()),
            Span::raw(" duplicate  "),
            Span::styled("x", key_style()),
            Span::raw(" delete"),
        ]),
        Line::from(vec![
            Span::styled("LAYOUT", modal_section_style()),
            Span::raw("   "),
            Span::styled("m", key_style()),
            Span::raw(" maximize/focus  "),
            Span::styled("z", key_style()),
            Span::raw(" density  "),
            Span::styled("ctrl+left/right", key_style()),
            Span::raw(" request width  "),
            Span::styled("ctrl+up/down", key_style()),
            Span::raw(" detail height"),
        ]),
        Line::raw(""),
        Line::from(vec![
            Span::styled("FILTERS", modal_section_style()),
            Span::raw("  plain terms, structured tokens, or regex patterns"),
        ]),
        Line::styled(
            "         status:5xx method:post domain:localhost has:body",
            muted_style(),
        ),
        Line::styled(
            "         path:/api/v[0-9]+  method:^(post|put)$  /graphql|rest/",
            muted_style(),
        ),
        Line::styled(
            "         duration:>500 size:>100kb reqbody:email resbody:error",
            muted_style(),
        ),
        Line::raw(""),
        Line::from(vec![
            Span::styled("CONSOLE", modal_section_style()),
            Span::raw("  "),
            Span::styled("2", key_style()),
            Span::raw(" console view  "),
            Span::styled("e", key_style()),
            Span::raw(" evaluate JS  "),
            Span::styled("c", key_style()),
            Span::raw(" clear visible console  filters: level:error kind:eval /token.*/"),
        ]),
        Line::raw(""),
        Line::from(vec![
            Span::styled("STATE", modal_section_style()),
            Span::raw(format!(
                "    view={}  focus={}  density={}  filter={}  split={}:{} / {}:{}",
                app.view.label(),
                app.focus.label(),
                app.density_mode.label(),
                active_filter_text(app),
                app.requests_percent,
                100 - app.requests_percent,
                app.detail_percent,
                100 - app.detail_percent
            )),
        ]),
    ];
    frame.render_widget(Clear, area);
    frame.render_widget(
        Paragraph::new(lines)
            .block(themed_panel_block(
                " Command Matrix ",
                Some('?'),
                true,
                &app.config.theme,
            ))
            .style(Style::default().fg(GB_FG))
            .wrap(Wrap { trim: false }),
        area,
    );
}
