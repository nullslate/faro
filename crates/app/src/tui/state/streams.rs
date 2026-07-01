use super::*;

impl WorkbenchState {
    pub(super) fn next_console(&mut self) {
        if self.filtered_console_indices.is_empty() {
            return;
        }
        let next = match self.console_state.selected() {
            Some(index) if index + 1 < self.filtered_console_indices.len() => index + 1,
            _ => 0,
        };
        self.console_state.select(Some(next));
    }

    pub(super) fn previous_console(&mut self) {
        if self.filtered_console_indices.is_empty() {
            return;
        }
        let previous = match self.console_state.selected() {
            Some(0) | None => self.filtered_console_indices.len() - 1,
            Some(index) => index - 1,
        };
        self.console_state.select(Some(previous));
    }

    pub(super) fn next_websocket_frame(&mut self) {
        if self.filtered_websocket_indices.is_empty() {
            return;
        }
        let next = match self.websocket_state.selected() {
            Some(index) if index + 1 < self.filtered_websocket_indices.len() => index + 1,
            _ => 0,
        };
        self.websocket_state.select(Some(next));
        self.websocket_detail_scroll = 0;
    }

    pub(super) fn previous_websocket_frame(&mut self) {
        if self.filtered_websocket_indices.is_empty() {
            return;
        }
        let previous = match self.websocket_state.selected() {
            Some(0) | None => self.filtered_websocket_indices.len() - 1,
            Some(index) => index - 1,
        };
        self.websocket_state.select(Some(previous));
        self.websocket_detail_scroll = 0;
    }

    pub(super) fn apply_console_filter(&mut self) {
        let selected_id = self.selected_console_log().map(|log| log.id.clone());
        self.filtered_console_indices =
            filter_console_indices(&self.console_logs, &self.console_filter);

        let selected = selected_id
            .and_then(|id| self.filtered_index_for_console_id(&id))
            .or_else(|| (!self.filtered_console_indices.is_empty()).then_some(0));
        self.console_state.select(selected);
    }

    pub(super) fn apply_websocket_filter(&mut self) {
        let selected_id = self
            .selected_websocket_frame()
            .map(|frame| frame.id.clone());
        self.filtered_websocket_indices =
            filter_websocket_indices(&self.websocket_frames, &self.websocket_filter);

        let selected = selected_id
            .and_then(|id| {
                self.filtered_websocket_indices.iter().position(|index| {
                    self.websocket_frames.get(*index).map(|frame| &frame.id) == Some(&id)
                })
            })
            .or_else(|| (!self.filtered_websocket_indices.is_empty()).then_some(0));
        self.websocket_state.select(selected);
    }

    fn filtered_index_for_console_id(&self, log_id: &str) -> Option<usize> {
        self.filtered_console_indices
            .iter()
            .position(|index| self.console_logs[*index].id == log_id)
    }

    pub(crate) fn selected_console_log(&self) -> Option<&ConsoleLog> {
        self.console_state
            .selected()
            .and_then(|index| self.filtered_console_indices.get(index))
            .and_then(|index| self.console_logs.get(*index))
    }

    pub(crate) fn selected_websocket_frame(&self) -> Option<&WebSocketFrameRecord> {
        self.websocket_state
            .selected()
            .and_then(|index| self.filtered_websocket_indices.get(index))
            .and_then(|index| self.websocket_frames.get(*index))
    }
}
