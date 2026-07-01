use super::*;

pub(super) fn visible_request_rows(area: Rect) -> usize {
    // Border top/bottom plus a one-line header.
    area.height.saturating_sub(3).max(1) as usize
}

pub(super) fn request_window_start(selected: usize, visible_rows: usize, total: usize) -> usize {
    if total <= visible_rows {
        return 0;
    }
    selected
        .saturating_sub(visible_rows / 2)
        .min(total.saturating_sub(visible_rows))
}

pub(super) fn visible_list_rows(area: Rect) -> usize {
    area.height.saturating_sub(2).max(1) as usize
}

pub(super) fn visible_list_window(
    selected: usize,
    visible_rows: usize,
    total: usize,
) -> (usize, usize) {
    if total == 0 {
        return (0, 0);
    }
    let offset = request_window_start(selected.min(total - 1), visible_rows, total);
    let end = offset.saturating_add(visible_rows).min(total);
    (offset, end)
}

pub(super) fn visible_list_state(
    selected: Option<usize>,
    offset: usize,
    total: usize,
) -> ListState {
    if total == 0 {
        return ListState::default();
    }
    let visible_selected =
        selected.map(|selected| selected.min(total.saturating_sub(1)).saturating_sub(offset));
    ListState::default().with_selected(visible_selected)
}

pub(super) fn visible_request_table_state(app: &WorkbenchState, visible_rows: usize) -> TableState {
    let total = app.filtered_request_rows.len();
    let selected = app
        .table_state
        .selected()
        .map(|selected| selected.min(total.saturating_sub(1)));
    let visible_selected = selected.map(|selected| {
        selected.saturating_sub(request_window_start(selected, visible_rows, total))
    });
    TableState::default().with_selected(visible_selected)
}

pub(super) fn faded_lines(
    lines: Vec<Line<'static>>,
    scroll: u16,
    area: Rect,
    theme: &Theme,
    fade_rows: usize,
) -> Vec<Line<'static>> {
    let visible_rows = pane_visible_rows(area);
    let offset = usize::from(scroll);
    let has_more_below = offset + visible_rows < lines.len();
    lines
        .into_iter()
        .enumerate()
        .map(|(index, line)| {
            let fade = bottom_overlay_fade(index, offset, visible_rows, has_more_below, fade_rows);
            if matches!(fade, RowFade::Full) {
                line
            } else {
                line.patch_style(fade.base_style(theme))
            }
        })
        .collect()
}

pub(super) fn pane_visible_rows(area: Rect) -> usize {
    area.height.saturating_sub(2).max(1) as usize
}

pub(super) fn selected_window_start(selected: usize, visible_rows: usize, total: usize) -> usize {
    if total <= visible_rows {
        return 0;
    }
    selected
        .saturating_sub(visible_rows / 2)
        .min(total.saturating_sub(visible_rows))
}
