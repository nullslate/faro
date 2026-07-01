use super::*;

mod navigation;
mod sessions;
mod sql;

impl WorkbenchState {
    pub(crate) fn clear_console(&mut self) {
        self.console_hidden_before = Some(now_ms());
        self.console_logs.clear();
        self.filtered_console_indices.clear();
        self.console_state.select(None);
        self.status = "console cleared".to_string();
    }

    pub(crate) fn clear_visible_requests(&mut self) {
        self.requests_hidden_before = Some(now_ms());
        self.apply_filter();
        self.status = match self.active_filter_preset_label() {
            Some(label) => format!("cleared visible {label} requests; tracking fresh traffic"),
            None if !self.request_filter.is_empty() => {
                format!(
                    "cleared visible `{}` requests; tracking fresh traffic",
                    self.request_filter
                )
            }
            None => "cleared visible requests; tracking fresh traffic".to_string(),
        };
    }

    pub(crate) fn toggle_layout_mode(&mut self) {
        self.layout_mode = self.layout_mode.toggled();
    }

    pub(crate) fn toggle_density_mode(&mut self) {
        self.density_mode = self.density_mode.toggled();
        self.status = format!("density {}", self.density_mode.label());
    }

    pub(crate) fn toggle_help(&mut self) {
        self.show_help = !self.show_help;
    }

    pub(crate) fn toggle_theme_preview(&mut self) {
        self.show_theme_preview = !self.show_theme_preview;
    }

    pub(crate) fn toggle_perf(&mut self) {
        self.show_perf = !self.show_perf;
    }

    pub(crate) fn open_palette(&mut self) {
        self.input_mode = InputMode::Palette;
        self.palette_query.clear();
        self.palette_selected = 0;
    }

    pub(crate) fn close_palette(&mut self) {
        self.input_mode = InputMode::Normal;
        self.palette_query.clear();
        self.palette_selected = 0;
    }

    pub(crate) fn open_body_search(&mut self) {
        self.input_mode = InputMode::BodySearch;
        self.body_search_query.clear();
    }

    pub(crate) fn close_body_search(&mut self) {
        self.input_mode = InputMode::Normal;
        self.body_search_query.clear();
    }

    pub(crate) fn push_body_search_char(&mut self, character: char) {
        self.body_search_query.push(character);
        self.apply_body_search();
    }

    pub(crate) fn pop_body_search_char(&mut self) {
        self.body_search_query.pop();
        self.apply_body_search();
    }

    pub(crate) fn apply_body_search(&mut self) {
        let query = self.body_search_query.trim().to_lowercase();
        if query.is_empty() {
            self.body_scroll = 0;
            return;
        }
        let Some(request) = self.selected_request() else {
            return;
        };
        let body = formatted_response_body(request);
        if let Some(index) = body
            .lines()
            .position(|line| line.to_lowercase().contains(&query))
        {
            self.body_scroll = index.min(u16::MAX as usize) as u16;
            self.status = format!("body match line {}", index + 1);
        } else {
            self.status = "no body search match".to_string();
        }
    }

    pub(crate) fn push_palette_char(&mut self, character: char) {
        self.palette_query.push(character);
        self.palette_selected = 0;
    }

    pub(crate) fn pop_palette_char(&mut self) {
        self.palette_query.pop();
        self.palette_selected = 0;
    }

    pub(crate) fn next_palette_item(&mut self) {
        let len = self.filtered_palette_entries().len();
        if len > 0 {
            self.palette_selected = (self.palette_selected + 1) % len;
        }
    }

    pub(crate) fn previous_palette_item(&mut self) {
        let len = self.filtered_palette_entries().len();
        if len > 0 {
            self.palette_selected = if self.palette_selected == 0 {
                len - 1
            } else {
                self.palette_selected - 1
            };
        }
    }

    pub(crate) fn selected_palette_command(&self) -> Option<PaletteCommand> {
        self.filtered_palette_entries()
            .get(self.palette_selected)
            .map(|entry| entry.command)
    }

    pub(crate) fn apply_layout_preset(&mut self, preset: LayoutPreset) {
        match preset {
            LayoutPreset::CompactNetwork => {
                self.set_view(WorkbenchView::Network);
                self.focus = FocusPane::Requests;
                self.layout_mode = LayoutMode::Normal;
                self.density_mode = DensityMode::Compact;
                self.requests_percent = 66;
                self.detail_percent = 45;
                self.status = "layout preset: compact network".to_string();
            }
            LayoutPreset::BodyHeavy => {
                self.set_view(WorkbenchView::Network);
                self.focus = FocusPane::Body;
                self.layout_mode = LayoutMode::Normal;
                self.density_mode = DensityMode::Comfortable;
                self.requests_percent = 36;
                self.detail_percent = 28;
                self.status = "layout preset: body heavy".to_string();
            }
            LayoutPreset::ConsoleHeavy => {
                self.set_view(WorkbenchView::Console);
                self.layout_mode = LayoutMode::Focused;
                self.status = "layout preset: console heavy".to_string();
            }
            LayoutPreset::WebSocketHeavy => {
                self.set_view(WorkbenchView::WebSockets);
                self.layout_mode = LayoutMode::Focused;
                self.status = "layout preset: websocket heavy".to_string();
            }
        }
    }

    pub(crate) fn filtered_palette_entries(&self) -> Vec<PaletteEntry> {
        PALETTE_ENTRIES
            .iter()
            .copied()
            .filter(|entry| palette_matches(entry, &self.palette_query))
            .collect()
    }

    pub(crate) fn layout_preference(&self) -> LayoutPreference {
        LayoutPreference {
            mode: self.layout_mode,
            density: self.density_mode,
            focus: self.focus,
            requests_percent: self.requests_percent,
            detail_percent: self.detail_percent,
            filter_preset: self.active_filter_preset_label().map(str::to_string),
        }
    }

    pub(crate) fn resize_requests_split(&mut self, delta: i16) {
        self.requests_percent = adjusted_percent(self.requests_percent, delta);
        self.status = format!("requests split {}%", self.requests_percent);
    }

    pub(crate) fn resize_detail_split(&mut self, delta: i16) {
        self.detail_percent = adjusted_percent(self.detail_percent, delta);
        self.status = format!("detail split {}%", self.detail_percent);
    }

    pub(crate) fn push_filter_char(&mut self, character: char) {
        match self.view {
            WorkbenchView::Console => {
                self.console_filter.push(character);
                self.apply_console_filter();
            }
            WorkbenchView::WebSockets => {
                self.websocket_filter.push(character);
                self.apply_websocket_filter();
            }
            WorkbenchView::Scripts => {}
            _ => {
                self.request_filter.push(character);
                self.apply_filter();
            }
        }
    }

    pub(crate) fn pop_filter_char(&mut self) {
        match self.view {
            WorkbenchView::Console => {
                self.console_filter.pop();
                self.apply_console_filter();
            }
            WorkbenchView::WebSockets => {
                self.websocket_filter.pop();
                self.apply_websocket_filter();
            }
            WorkbenchView::Scripts => {}
            _ => {
                self.request_filter.pop();
                self.apply_filter();
            }
        }
    }

    pub(crate) fn clear_filter(&mut self) {
        match self.view {
            WorkbenchView::Console => {
                self.console_filter.clear();
                self.apply_console_filter();
            }
            WorkbenchView::WebSockets => {
                self.websocket_filter.clear();
                self.apply_websocket_filter();
            }
            WorkbenchView::Scripts => {}
            _ => {
                self.request_filter.clear();
                self.sql_request_filter_ids = None;
                self.sql_request_filter_query = None;
                self.apply_filter();
            }
        }
    }

    pub(crate) fn apply_filter_from_palette(&mut self) {
        self.apply_filter();
        self.set_view(WorkbenchView::Network);
        self.status = if self.request_filter.is_empty() {
            "filter all".to_string()
        } else {
            format!("filter {}", self.request_filter)
        };
    }

    pub(crate) fn apply_console_filter_from_palette(&mut self) {
        self.apply_console_filter();
        self.set_view(WorkbenchView::Console);
        self.status = if self.console_filter.is_empty() {
            "console filter all".to_string()
        } else {
            format!("console filter {}", self.console_filter)
        };
    }

    pub(crate) fn apply_websocket_filter_from_palette(&mut self) {
        self.apply_websocket_filter();
        self.set_view(WorkbenchView::WebSockets);
        self.status = if self.websocket_filter.is_empty() {
            "websocket filter all".to_string()
        } else {
            format!("websocket filter {}", self.websocket_filter)
        };
    }

    pub(crate) fn cycle_filter_preset(&mut self) {
        match self.view {
            WorkbenchView::Console => self.cycle_console_filter_preset(),
            WorkbenchView::WebSockets => self.cycle_websocket_filter_preset(),
            WorkbenchView::Network => self.cycle_request_filter_preset(),
            _ => {}
        }
    }

    fn cycle_request_filter_preset(&mut self) {
        let preset = next_filter_preset(FILTER_PRESETS, &self.request_filter);
        self.request_filter = preset.query.to_string();
        self.apply_filter();
        self.status = filter_preset_status("request", preset);
    }

    fn cycle_console_filter_preset(&mut self) {
        let preset = next_filter_preset(CONSOLE_FILTER_PRESETS, &self.console_filter);
        self.console_filter = preset.query.to_string();
        self.apply_console_filter();
        self.status = filter_preset_status("console", preset);
    }

    fn cycle_websocket_filter_preset(&mut self) {
        let preset = next_filter_preset(WEBSOCKET_FILTER_PRESETS, &self.websocket_filter);
        self.websocket_filter = preset.query.to_string();
        self.apply_websocket_filter();
        self.status = filter_preset_status("websocket", preset);
    }

    pub(crate) fn active_filter_preset_label(&self) -> Option<&'static str> {
        FILTER_PRESETS
            .iter()
            .find(|preset| preset.query == self.request_filter)
            .map(|preset| preset.label)
    }
}
