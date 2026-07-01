use super::super::*;

impl WorkbenchState {
    pub(crate) fn open_sessions(&mut self) {
        if self.sessions.is_empty() {
            self.session_state.select(None);
        } else if self.session_state.selected().is_none() {
            let selected = self
                .active_session_id
                .as_deref()
                .and_then(|id| {
                    self.sessions
                        .iter()
                        .position(|entry| entry.session.id.as_str() == id)
                })
                .unwrap_or_else(|| self.sessions.len() - 1);
            self.session_state.select(Some(selected));
        }
        self.show_sessions = true;
    }

    pub(crate) fn close_sessions(&mut self) {
        self.show_sessions = false;
    }

    pub(crate) fn next_session(&mut self) {
        if self.sessions.is_empty() {
            self.session_state.select(None);
            return;
        }
        let selected = self
            .session_state
            .selected()
            .map(|index| (index + 1).min(self.sessions.len() - 1))
            .unwrap_or(0);
        self.session_state.select(Some(selected));
    }

    pub(crate) fn previous_session(&mut self) {
        if self.sessions.is_empty() {
            self.session_state.select(None);
            return;
        }
        let selected = self
            .session_state
            .selected()
            .map(|index| index.saturating_sub(1))
            .unwrap_or(0);
        self.session_state.select(Some(selected));
    }

    pub(crate) fn selected_session_id(&self) -> Option<String> {
        let selected = self.session_state.selected()?;
        self.sessions
            .get(selected)
            .map(|entry| entry.session.id.clone())
    }

    pub(crate) fn remove_session_optimistic(&mut self, session_id: &str) {
        let Some(position) = self
            .sessions
            .iter()
            .position(|entry| entry.session.id == session_id)
        else {
            return;
        };
        self.sessions.remove(position);
        if self.sessions.is_empty() {
            self.session_state.select(None);
        } else {
            self.session_state
                .select(Some(position.min(self.sessions.len() - 1)));
        }

        if self.active_session_id.as_deref() != Some(session_id) {
            return;
        }
        self.active_session_id = self
            .session_state
            .selected()
            .and_then(|index| self.sessions.get(index))
            .map(|entry| entry.session.id.clone());
        self.requests.clear();
        self.request_tree_metas.clear();
        self.filtered_request_indices.clear();
        self.filtered_request_rows.clear();
        self.filtered_route_descendant_counts.clear();
        self.console_logs.clear();
        self.filtered_console_indices.clear();
        self.websocket_frames.clear();
        self.filtered_websocket_indices.clear();
        self.storage_events.clear();
        self.storage_snapshots.clear();
        self.cookie_events.clear();
        self.cookie_snapshots.clear();
        self.table_state.select(None);
        self.console_state.select(None);
        self.websocket_state.select(None);
    }
}
