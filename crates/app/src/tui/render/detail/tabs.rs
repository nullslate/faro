use crate::tui::render::GB_BG2;
use crate::tui::state::{DetailTab, WorkbenchState};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};

pub(crate) fn detail_tab_lines(app: &WorkbenchState, width: u16) -> Vec<Line<'static>> {
    let max_width = usize::from(width.saturating_sub(1)).max(8);
    let short = pack_detail_tab_segments(
        detail_tab_segments(app, DetailTabLabelMode::Short),
        max_width,
    );
    if short.len() <= 2 && short.iter().all(|line| line_width(line) <= max_width) {
        return short;
    }

    pack_detail_tab_segments(
        detail_tab_segments(app, DetailTabLabelMode::Tiny),
        max_width,
    )
}

fn detail_tab_segments(
    app: &WorkbenchState,
    label_mode: DetailTabLabelMode,
) -> Vec<Vec<Span<'static>>> {
    detail_tabs()
        .iter()
        .copied()
        .map(|tab| detail_tab_spans(tab, app, label_mode))
        .collect()
}

fn pack_detail_tab_segments(
    segments: Vec<Vec<Span<'static>>>,
    max_width: usize,
) -> Vec<Line<'static>> {
    let mut lines = Vec::new();
    let mut current = Vec::new();
    let mut current_width: usize = 0;
    for segment in segments {
        let segment_width = spans_width(&segment);
        let separator_width = usize::from(!current.is_empty());
        if !current.is_empty()
            && current_width
                .saturating_add(separator_width)
                .saturating_add(segment_width)
                > max_width
        {
            lines.push(Line::from(current));
            current = Vec::new();
            current_width = 0;
        }
        if !current.is_empty() {
            current.push(Span::raw(" "));
            current_width += 1;
        }
        current_width += segment_width;
        current.extend(segment);
    }
    if !current.is_empty() {
        lines.push(Line::from(current));
    }
    lines
}

fn detail_tabs() -> &'static [DetailTab] {
    &[
        DetailTab::Overview,
        DetailTab::RequestHeaders,
        DetailTab::RequestBody,
        DetailTab::ResponseHeaders,
        DetailTab::ResponseBody,
        DetailTab::Timing,
        DetailTab::Replay,
    ]
}

pub(crate) fn line_width(line: &Line<'_>) -> usize {
    spans_width(&line.spans)
}

fn spans_width(spans: &[Span<'_>]) -> usize {
    spans.iter().map(|span| span.content.chars().count()).sum()
}

fn detail_tab_spans(
    tab: DetailTab,
    app: &WorkbenchState,
    label_mode: DetailTabLabelMode,
) -> Vec<Span<'static>> {
    let label = detail_tab_label(tab, label_mode);
    if tab == app.detail_tab {
        detail_tab_pill_spans(label, app.config.theme.accent, Color::Rgb(29, 32, 33), true)
    } else {
        detail_tab_pill_spans(label, GB_BG2, app.config.theme.muted, false)
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum DetailTabLabelMode {
    Short,
    Tiny,
}

fn detail_tab_pill_spans(
    label: &'static str,
    background: Color,
    foreground: Color,
    active: bool,
) -> Vec<Span<'static>> {
    let label_style = if active {
        Style::default()
            .fg(foreground)
            .bg(background)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(foreground).bg(background)
    };
    vec![
        Span::styled("", Style::default().fg(background)),
        Span::styled(format!(" {label} "), label_style),
        Span::styled("", Style::default().fg(background)),
    ]
}

fn detail_tab_label(tab: DetailTab, mode: DetailTabLabelMode) -> &'static str {
    match mode {
        DetailTabLabelMode::Short => match tab {
            DetailTab::Overview => "overview",
            DetailTab::RequestHeaders => "req hdr",
            DetailTab::RequestBody => "req body",
            DetailTab::ResponseHeaders => "res hdr",
            DetailTab::ResponseBody => "res body",
            DetailTab::Timing => "timing",
            DetailTab::Replay => "replay",
        },
        DetailTabLabelMode::Tiny => match tab {
            DetailTab::Overview => "o",
            DetailTab::RequestHeaders => "qH",
            DetailTab::RequestBody => "qB",
            DetailTab::ResponseHeaders => "sH",
            DetailTab::ResponseBody => "sB",
            DetailTab::Timing => "t",
            DetailTab::Replay => "r",
        },
    }
}
