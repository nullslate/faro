use super::super::*;

impl WorkbenchState {
    pub(crate) fn selected_replay(&self) -> Option<&ReplayView> {
        let request = self.selected_request()?;
        let index = self.selected_replay_display_index()?;
        request.replays.get(index)
    }

    pub(crate) fn selected_replay_display_index(&self) -> Option<usize> {
        let request = self.selected_request()?;
        (!request.replays.is_empty()).then(|| {
            self.selected_replay_index
                .min(request.replays.len().saturating_sub(1))
        })
    }

    pub(crate) fn set_view(&mut self, view: WorkbenchView) {
        self.view = view;
        self.focus = match view {
            WorkbenchView::Network => FocusPane::Requests,
            WorkbenchView::Console => FocusPane::Console,
            WorkbenchView::WebSockets => FocusPane::WebSockets,
            WorkbenchView::Scripts => FocusPane::Scripts,
            WorkbenchView::Storage => FocusPane::Storage,
            WorkbenchView::Cookies => FocusPane::Cookies,
        };
        self.hydrate_selected_request_for_active_detail();
    }

    pub(crate) fn set_focus(&mut self, focus: FocusPane) {
        self.focus = match self.view {
            WorkbenchView::Network => match focus {
                FocusPane::Body if self.detail_tab == DetailTab::Replay => FocusPane::Detail,
                FocusPane::Requests | FocusPane::Detail | FocusPane::Body => focus,
                _ => FocusPane::Requests,
            },
            WorkbenchView::Console => FocusPane::Console,
            WorkbenchView::WebSockets => FocusPane::WebSockets,
            WorkbenchView::Scripts => FocusPane::Scripts,
            WorkbenchView::Storage => FocusPane::Storage,
            WorkbenchView::Cookies => FocusPane::Cookies,
        };
    }

    pub(crate) fn next(&mut self) {
        match self.focus {
            FocusPane::Requests => self.next_request(),
            FocusPane::Detail if self.detail_tab == DetailTab::Replay => self.next_replay(),
            FocusPane::Detail => self.scroll_down(),
            FocusPane::Body if self.has_body_tree() => self.next_body_tree_node(),
            FocusPane::Body => self.scroll_down(),
            FocusPane::Console => self.next_console(),
            FocusPane::WebSockets => self.next_websocket_frame(),
            FocusPane::Scripts => self.next_script(),
            FocusPane::Storage => self.next_storage_entry(),
            FocusPane::Cookies => self.next_cookie_entry(),
        }
    }

    pub(crate) fn previous(&mut self) {
        match self.focus {
            FocusPane::Requests => self.previous_request(),
            FocusPane::Detail if self.detail_tab == DetailTab::Replay => self.previous_replay(),
            FocusPane::Detail => self.scroll_up(),
            FocusPane::Body if self.has_body_tree() => self.previous_body_tree_node(),
            FocusPane::Body => self.scroll_up(),
            FocusPane::Console => self.previous_console(),
            FocusPane::WebSockets => self.previous_websocket_frame(),
            FocusPane::Scripts => self.previous_script(),
            FocusPane::Storage => self.previous_storage_entry(),
            FocusPane::Cookies => self.previous_cookie_entry(),
        }
    }

    pub(crate) fn next_replay(&mut self) {
        let Some(replay_count) = self.selected_request().map(|request| request.replays.len())
        else {
            return;
        };
        if replay_count == 0 {
            self.selected_replay_index = 0;
            return;
        }
        self.selected_replay_index = self
            .selected_replay_index
            .saturating_add(1)
            .min(replay_count.saturating_sub(1));
        self.detail_scroll = 0;
    }

    pub(crate) fn previous_replay(&mut self) {
        if self
            .selected_request()
            .map(|request| request.replays.is_empty())
            .unwrap_or(true)
        {
            self.selected_replay_index = 0;
            return;
        }
        self.selected_replay_index = self.selected_replay_index.saturating_sub(1);
        self.detail_scroll = 0;
    }

    pub(crate) fn next_focus(&mut self) {
        self.focus = match self.view {
            WorkbenchView::Network => match self.focus {
                FocusPane::Requests => FocusPane::Detail,
                FocusPane::Detail if self.detail_tab == DetailTab::Replay => FocusPane::Requests,
                FocusPane::Detail => FocusPane::Body,
                _ => FocusPane::Requests,
            },
            WorkbenchView::Console => FocusPane::Console,
            WorkbenchView::WebSockets => FocusPane::WebSockets,
            WorkbenchView::Scripts => FocusPane::Scripts,
            WorkbenchView::Storage => FocusPane::Storage,
            WorkbenchView::Cookies => FocusPane::Cookies,
        };
        self.hydrate_selected_request_for_active_detail();
    }

    pub(crate) fn next_tab(&mut self) {
        self.detail_tab = self.detail_tab.next();
        self.detail_scroll = 0;
        if self.detail_tab == DetailTab::Replay && self.focus == FocusPane::Body {
            self.focus = FocusPane::Detail;
        }
        self.hydrate_selected_request_for_active_detail();
    }

    pub(crate) fn previous_tab(&mut self) {
        self.detail_tab = self.detail_tab.previous();
        self.detail_scroll = 0;
        if self.detail_tab == DetailTab::Replay && self.focus == FocusPane::Body {
            self.focus = FocusPane::Detail;
        }
        self.hydrate_selected_request_for_active_detail();
    }

    pub(crate) fn scroll_down(&mut self) {
        match self.focus {
            FocusPane::Detail => self.detail_scroll = self.detail_scroll.saturating_add(4),
            FocusPane::Body => self.body_scroll = self.body_scroll.saturating_add(8),
            FocusPane::Requests => self.next_request(),
            FocusPane::Console => self.next_console(),
            FocusPane::WebSockets => {
                self.websocket_detail_scroll = self.websocket_detail_scroll.saturating_add(8)
            }
            FocusPane::Scripts => self.next_script(),
            FocusPane::Storage => {
                for _ in 0..4 {
                    self.next_storage_entry();
                }
            }
            FocusPane::Cookies => {
                for _ in 0..4 {
                    self.next_cookie_entry();
                }
            }
        }
    }

    pub(crate) fn scroll_up(&mut self) {
        match self.focus {
            FocusPane::Detail => self.detail_scroll = self.detail_scroll.saturating_sub(4),
            FocusPane::Body => self.body_scroll = self.body_scroll.saturating_sub(8),
            FocusPane::Requests => self.previous_request(),
            FocusPane::Console => self.previous_console(),
            FocusPane::WebSockets => {
                self.websocket_detail_scroll = self.websocket_detail_scroll.saturating_sub(8)
            }
            FocusPane::Scripts => self.previous_script(),
            FocusPane::Storage => {
                for _ in 0..4 {
                    self.previous_storage_entry();
                }
            }
            FocusPane::Cookies => {
                for _ in 0..4 {
                    self.previous_cookie_entry();
                }
            }
        }
    }

    pub(crate) fn scroll_top(&mut self) {
        match self.focus {
            FocusPane::Detail => self.detail_scroll = 0,
            FocusPane::Body => {
                self.body_scroll = 0;
                self.body_tree_selected = 0;
                self.sync_body_tree_selected_key();
            }
            FocusPane::Storage => {
                self.storage_selected = 0;
                self.storage_scroll = 0;
            }
            FocusPane::Cookies => {
                self.cookie_selected = 0;
                self.cookie_scroll = 0;
            }
            FocusPane::Requests => self
                .table_state
                .select((!self.filtered_request_indices.is_empty()).then_some(0)),
            FocusPane::Console => self
                .console_state
                .select((!self.filtered_console_indices.is_empty()).then_some(0)),
            FocusPane::WebSockets => {
                self.websocket_detail_scroll = 0;
                self.websocket_state
                    .select((!self.filtered_websocket_indices.is_empty()).then_some(0));
            }
            FocusPane::Scripts => {
                self.script_state
                    .select((!self.scripts.is_empty()).then_some(0));
            }
        }
    }

    pub(crate) fn scroll_bottom(&mut self) {
        match self.focus {
            FocusPane::Detail => self.detail_scroll = self.detail_line_count().saturating_sub(1),
            FocusPane::Body => {
                let len = self.body_tree_items().len();
                if len > 0 {
                    self.body_tree_selected = len - 1;
                    self.sync_body_tree_selected_key();
                }
                self.body_scroll = self.body_line_count().saturating_sub(1);
            }
            FocusPane::Storage => {
                self.storage_selected = self.current_storage_entries().len().saturating_sub(1);
                self.storage_scroll = self
                    .storage_selected
                    .saturating_sub(3)
                    .min(u16::MAX as usize) as u16;
            }
            FocusPane::Cookies => {
                self.cookie_selected = self.current_cookie_entries().len().saturating_sub(1);
                self.cookie_scroll = self
                    .cookie_selected
                    .saturating_sub(3)
                    .min(u16::MAX as usize) as u16;
            }
            FocusPane::Requests => self.table_state.select(
                (!self.filtered_request_rows.is_empty())
                    .then_some(self.filtered_request_rows.len() - 1),
            ),
            FocusPane::Console => self.console_state.select(
                (!self.filtered_console_indices.is_empty())
                    .then_some(self.filtered_console_indices.len() - 1),
            ),
            FocusPane::WebSockets => {
                self.websocket_detail_scroll = u16::MAX;
                self.websocket_state.select(
                    (!self.filtered_websocket_indices.is_empty())
                        .then_some(self.filtered_websocket_indices.len() - 1),
                );
            }
            FocusPane::Scripts => self
                .script_state
                .select((!self.scripts.is_empty()).then_some(self.scripts.len() - 1)),
        }
    }
}
