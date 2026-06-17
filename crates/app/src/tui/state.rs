use crate::config::AppConfig;
use anyhow::Context;
use faro_core::{
    ConsoleLevel, ConsoleLog, CookieEventRecord, CookieSnapshotRecord, ReplayRecord, RequestRecord,
    ResponseRecord, Session, StorageEventRecord, StorageSnapshotRecord, UnixMillis,
    WebSocketFrameRecord, now_ms,
};
use faro_store::{ScriptRecord, Store};
use ratatui::widgets::{ListState, TableState};
use regex::{Regex, RegexBuilder};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::Path;
use std::time::Instant;

use super::layout::{DensityMode, LayoutMode, LayoutPreference, clamp_split_percent};

pub(crate) type ReplayContext = (String, Option<String>, Option<String>, String, String);

pub(crate) struct WorkbenchState {
    pub(crate) config: AppConfig,
    pub(crate) db_path: std::path::PathBuf,
    pub(crate) target_url: String,
    pub(crate) active_session_id: Option<String>,
    pub(crate) requests: Vec<RequestView>,
    pub(crate) request_tree_metas: Vec<RequestTreeMeta>,
    pub(crate) filtered_request_indices: Vec<usize>,
    pub(crate) collapsed_request_groups: HashSet<String>,
    pub(crate) active_request_route_group: Option<String>,
    pub(crate) sql_request_filter_ids: Option<HashSet<String>>,
    pub(crate) sql_request_filter_query: Option<String>,
    pub(crate) console_logs: Vec<ConsoleLog>,
    pub(crate) filtered_console_indices: Vec<usize>,
    pub(crate) console_hidden_before: Option<UnixMillis>,
    pub(crate) websocket_frames: Vec<WebSocketFrameRecord>,
    pub(crate) filtered_websocket_indices: Vec<usize>,
    pub(crate) websocket_state: ListState,
    pub(crate) websocket_detail_scroll: u16,
    pub(crate) storage_events: Vec<StorageEventRecord>,
    pub(crate) storage_snapshots: Vec<StorageSnapshotRecord>,
    pub(crate) storage_selected: usize,
    pub(crate) cookie_events: Vec<CookieEventRecord>,
    pub(crate) cookie_snapshots: Vec<CookieSnapshotRecord>,
    pub(crate) cookie_selected: usize,
    pub(crate) scripts: Vec<ScriptRecord>,
    pub(crate) script_state: ListState,
    pub(crate) script_output: Vec<String>,
    pub(crate) script_status: Option<String>,
    pub(crate) script_duration_ms: Option<u128>,
    pub(crate) table_state: TableState,
    pub(crate) console_state: ListState,
    pub(crate) view: WorkbenchView,
    pub(crate) focus: FocusPane,
    pub(crate) detail_tab: DetailTab,
    pub(crate) sort_mode: SortMode,
    pub(crate) sort_descending: bool,
    pub(crate) detail_scroll: u16,
    pub(crate) body_scroll: u16,
    pub(crate) body_tree_selected: usize,
    pub(crate) body_tree_selected_key: Option<String>,
    pub(crate) collapsed_body_nodes: HashSet<String>,
    pub(crate) storage_scroll: u16,
    pub(crate) cookie_scroll: u16,
    pub(crate) input_mode: InputMode,
    pub(crate) layout_mode: LayoutMode,
    pub(crate) density_mode: DensityMode,
    pub(crate) requests_percent: u16,
    pub(crate) detail_percent: u16,
    pub(crate) palette_query: String,
    pub(crate) palette_selected: usize,
    pub(crate) show_help: bool,
    pub(crate) sql_result: Option<SqlResultsView>,
    pub(crate) sql_row_scroll: usize,
    pub(crate) sql_col_scroll: usize,
    pub(crate) last_sql_query: String,
    pub(crate) request_filter: String,
    pub(crate) console_filter: String,
    pub(crate) cdp_websocket_url: Option<String>,
    pub(crate) status: String,
    pub(crate) status_updated_at: Instant,
}

impl WorkbenchState {
    pub(crate) fn load(
        store: &Store,
        db_path: &Path,
        target_url: &str,
        config: AppConfig,
    ) -> anyhow::Result<Self> {
        Self::load_for_session(store, db_path, target_url, config, None)
    }

    pub(crate) fn load_for_session(
        store: &Store,
        db_path: &Path,
        target_url: &str,
        config: AppConfig,
        active_session_id: Option<&str>,
    ) -> anyhow::Result<Self> {
        let mut requests = Vec::new();
        let mut console_logs = Vec::new();
        let mut websocket_frames = Vec::new();
        let mut storage_events = Vec::new();
        let mut storage_snapshots = Vec::new();
        let mut cookie_events = Vec::new();
        let mut cookie_snapshots = Vec::new();
        let scripts = store.scripts()?;
        let session = select_session(store.sessions()?, target_url, active_session_id);
        let selected_session_id = session.as_ref().map(|session| session.id.clone());
        if let Some(session) = &session {
            let mut responses_by_request = HashMap::new();
            for response in store.responses_for_session(&session.id)? {
                responses_by_request.insert(response.request_id.clone(), response);
            }
            console_logs.extend(store.console_logs_for_session(&session.id)?);
            websocket_frames.extend(store.websocket_frames_for_session(&session.id)?);
            storage_events.extend(store.storage_events_for_session(&session.id)?);
            storage_snapshots.extend(store.storage_snapshots_for_session(&session.id)?);
            cookie_events.extend(store.cookie_events_for_session(&session.id)?);
            cookie_snapshots.extend(store.cookie_snapshots_for_session(&session.id)?);
            for request in store.requests_for_session(&session.id)? {
                let response = responses_by_request.remove(&request.id);
                requests.push(RequestView {
                    request,
                    response,
                    request_body: None,
                    response_body: None,
                    replays: Vec::new(),
                    details_loaded: false,
                });
            }
        }

        let request_tree_metas = build_request_tree_metas(&requests);
        let filtered_request_indices = (0..requests.len()).collect::<Vec<_>>();
        let mut table_state = TableState::default();
        if !filtered_request_indices.is_empty() {
            table_state.select(Some(0));
        }
        let filtered_console_indices = (0..console_logs.len()).collect::<Vec<_>>();
        let mut console_state = ListState::default();
        if !filtered_console_indices.is_empty() {
            console_state.select(Some(filtered_console_indices.len() - 1));
        }
        let filtered_websocket_indices = (0..websocket_frames.len()).collect::<Vec<_>>();
        let mut websocket_state = ListState::default();
        if !filtered_websocket_indices.is_empty() {
            websocket_state.select(Some(filtered_websocket_indices.len() - 1));
        }

        let layout_preference = LayoutPreference::load();
        let initial_request_filter = layout_preference
            .filter_preset
            .as_deref()
            .and_then(filter_query_for_preset_label)
            .unwrap_or("")
            .to_string();
        let initial_focus = match layout_preference.focus {
            FocusPane::Requests | FocusPane::Detail | FocusPane::Body => layout_preference.focus,
            _ => FocusPane::Requests,
        };

        let mut state = Self {
            config,
            db_path: db_path.to_path_buf(),
            target_url: target_url.to_string(),
            active_session_id: selected_session_id,
            requests,
            request_tree_metas,
            filtered_request_indices,
            collapsed_request_groups: HashSet::new(),
            active_request_route_group: None,
            sql_request_filter_ids: None,
            sql_request_filter_query: None,
            console_logs,
            filtered_console_indices,
            console_hidden_before: None,
            websocket_frames,
            filtered_websocket_indices,
            websocket_state,
            websocket_detail_scroll: 0,
            storage_events,
            storage_snapshots,
            storage_selected: 0,
            cookie_events,
            cookie_snapshots,
            cookie_selected: 0,
            scripts,
            script_state: ListState::default(),
            script_output: Vec::new(),
            script_status: None,
            script_duration_ms: None,
            table_state,
            console_state,
            view: WorkbenchView::Network,
            focus: initial_focus,
            detail_tab: DetailTab::Overview,
            sort_mode: SortMode::Started,
            sort_descending: false,
            detail_scroll: 0,
            body_scroll: 0,
            body_tree_selected: 0,
            body_tree_selected_key: None,
            collapsed_body_nodes: HashSet::new(),
            storage_scroll: 0,
            cookie_scroll: 0,
            input_mode: InputMode::Normal,
            layout_mode: layout_preference.mode,
            density_mode: layout_preference.density,
            requests_percent: layout_preference.requests_percent,
            detail_percent: layout_preference.detail_percent,
            palette_query: String::new(),
            palette_selected: 0,
            show_help: false,
            sql_result: None,
            sql_row_scroll: 0,
            sql_col_scroll: 0,
            last_sql_query: String::new(),
            request_filter: initial_request_filter,
            console_filter: String::new(),
            cdp_websocket_url: None,
            status: "starting CDP capture".to_string(),
            status_updated_at: Instant::now(),
        };
        state.apply_filter();
        if !state.scripts.is_empty() {
            state.script_state.select(Some(0));
        }
        state.hydrate_selected_request();
        Ok(state)
    }

    pub(crate) fn note_status_changed(&mut self) {
        self.status_updated_at = Instant::now();
    }

    pub(crate) fn reload(&mut self) -> anyhow::Result<()> {
        let selected_id = self
            .selected_request()
            .map(|request| request.request.id.clone());
        let collapsed_request_groups = self.collapsed_request_groups.clone();
        let active_request_route_group = self.active_request_route_group.clone();
        let store = Store::open(&self.db_path)
            .with_context(|| format!("open database {}", self.db_path.display()))?;
        let loaded = Self::load_for_session(
            &store,
            &self.db_path,
            &self.target_url,
            self.config.clone(),
            self.active_session_id.as_deref(),
        )
        .with_context(|| format!("reload TUI state from {}", self.db_path.display()))?;
        self.active_session_id = loaded.active_session_id;
        self.requests = loaded.requests;
        self.request_tree_metas = loaded.request_tree_metas;
        self.collapsed_request_groups = collapsed_request_groups;
        self.active_request_route_group = active_request_route_group;
        self.console_logs = loaded.console_logs;
        self.websocket_frames = loaded.websocket_frames;
        self.apply_websocket_filter();
        if !self.filtered_websocket_indices.is_empty() {
            self.websocket_state
                .select(Some(self.filtered_websocket_indices.len() - 1));
        }
        if let Some(hidden_before) = self.console_hidden_before {
            self.console_logs.retain(|log| log.ts > hidden_before);
        }
        self.apply_console_filter();
        self.storage_events = loaded.storage_events;
        self.storage_snapshots = loaded.storage_snapshots;
        self.storage_selected = self
            .storage_selected
            .min(self.current_storage_entries().len().saturating_sub(1));
        self.cookie_events = loaded.cookie_events;
        self.cookie_snapshots = loaded.cookie_snapshots;
        let selected_script_id = self.selected_script().map(|script| script.id.clone());
        self.scripts = loaded.scripts;
        let selected_script = selected_script_id
            .and_then(|id| self.scripts.iter().position(|script| script.id == id))
            .or_else(|| (!self.scripts.is_empty()).then_some(0));
        self.script_state.select(selected_script);
        self.cookie_selected = self
            .cookie_selected
            .min(self.current_cookie_entries().len().saturating_sub(1));
        self.apply_filter();
        let selected = selected_id
            .and_then(|id| self.filtered_index_for_request_id(&id))
            .or_else(|| {
                (!self.filtered_request_indices.is_empty())
                    .then(|| self.filtered_request_indices.len() - 1)
            });
        self.table_state.select(selected);
        self.reset_request_view_scroll();
        if !self.filtered_console_indices.is_empty() {
            self.console_state
                .select(Some(self.filtered_console_indices.len() - 1));
        }
        self.hydrate_selected_request();
        Ok(())
    }

    pub(crate) fn clear_console(&mut self) {
        self.console_hidden_before = Some(now_ms());
        self.console_logs.clear();
        self.filtered_console_indices.clear();
        self.console_state.select(None);
        self.status = "console cleared".to_string();
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

    pub(crate) fn apply_sql_request_filter(&mut self, query: String, ids: HashSet<String>) {
        let count = ids.len();
        self.last_sql_query = query;
        self.sql_request_filter_ids = Some(ids);
        self.sql_request_filter_query = Some(self.last_sql_query.clone());
        self.sql_result = None;
        self.set_view(WorkbenchView::Network);
        self.apply_filter();
        self.status = format!("SQL filtered requests to {count} ids");
    }

    pub(crate) fn show_sql_error(&mut self, query: String, error: String) {
        self.sql_result = Some(SqlResultsView {
            query,
            columns: Vec::new(),
            rows: Vec::new(),
            duration_ms: 0,
            error: Some(error.clone()),
        });
        self.sql_row_scroll = 0;
        self.sql_col_scroll = 0;
        self.status = format!("SQL failed: {error}");
    }

    pub(crate) fn close_sql_result(&mut self) {
        self.sql_result = None;
        self.sql_row_scroll = 0;
        self.sql_col_scroll = 0;
    }

    pub(crate) fn scroll_sql_rows_down(&mut self) {
        if let Some(result) = &self.sql_result {
            self.sql_row_scroll = self
                .sql_row_scroll
                .saturating_add(1)
                .min(result.rows.len().saturating_sub(1));
        }
    }

    pub(crate) fn scroll_sql_rows_up(&mut self) {
        self.sql_row_scroll = self.sql_row_scroll.saturating_sub(1);
    }

    pub(crate) fn scroll_sql_columns_right(&mut self) {
        if let Some(result) = &self.sql_result {
            self.sql_col_scroll = self
                .sql_col_scroll
                .saturating_add(1)
                .min(result.columns.len().saturating_sub(1));
        }
    }

    pub(crate) fn scroll_sql_columns_left(&mut self) {
        self.sql_col_scroll = self.sql_col_scroll.saturating_sub(1);
    }

    pub(crate) fn scroll_sql_top(&mut self) {
        self.sql_row_scroll = 0;
    }

    pub(crate) fn scroll_sql_bottom(&mut self) {
        if let Some(result) = &self.sql_result {
            self.sql_row_scroll = result.rows.len().saturating_sub(1);
        }
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

    pub(crate) fn selected_request(&self) -> Option<&RequestView> {
        self.table_state
            .selected()
            .and_then(|index| self.filtered_request_indices.get(index))
            .and_then(|index| self.requests.get(*index))
    }

    pub(crate) fn select_request_position(&mut self, position: usize) {
        if position < self.filtered_request_indices.len() {
            self.table_state.select(Some(position));
            self.reset_request_view_scroll();
            self.hydrate_selected_request();
        }
    }

    pub(crate) fn select_storage_position(&mut self, position: usize) {
        if position < self.current_storage_entries().len() {
            self.storage_selected = position;
            self.storage_scroll = self
                .storage_selected
                .saturating_sub(3)
                .min(u16::MAX as usize) as u16;
        }
    }

    pub(crate) fn select_cookie_position(&mut self, position: usize) {
        if position < self.current_cookie_entries().len() {
            self.cookie_selected = position;
            self.cookie_scroll = self
                .cookie_selected
                .saturating_sub(3)
                .min(u16::MAX as usize) as u16;
        }
    }

    pub(crate) fn enter_selected_request_group(&mut self) {
        let Some(request_index) = self
            .table_state
            .selected()
            .and_then(|index| self.filtered_request_indices.get(index))
            .copied()
        else {
            return;
        };
        let Some(group) = self.collapsible_group_key_for_request_index(request_index) else {
            self.status = "no collapsible request branch".to_string();
            return;
        };
        self.collapsed_request_groups.remove(&group);
        self.active_request_route_group = Some(group.clone());
        self.status = format!(
            "entered {}; backspace to go up",
            route_label_for_group(&group)
        );
        self.apply_filter();
    }

    pub(crate) fn leave_request_route_group(&mut self) {
        let Some(group) = self.active_request_route_group.take() else {
            self.status = "already at request root".to_string();
            return;
        };
        self.active_request_route_group = parent_group_key(&group);
        self.status = match &self.active_request_route_group {
            Some(parent) => format!("up to {}", route_label_for_group(parent)),
            None => format!("left {}", route_label_for_group(&group)),
        };
        self.apply_filter();
    }

    pub(crate) fn toggle_selected_request_group(&mut self) {
        let Some(request_index) = self
            .table_state
            .selected()
            .and_then(|index| self.filtered_request_indices.get(index))
            .copied()
        else {
            return;
        };
        let Some(group) = self.collapsible_group_key_for_request_index(request_index) else {
            self.status = "no collapsible request branch".to_string();
            return;
        };
        if self.collapsed_request_groups.remove(&group) {
            self.status = format!("expanded {}", group_label(&group));
        } else {
            self.collapsed_request_groups.insert(group.clone());
            if self.active_request_route_group.as_deref() == Some(group.as_str()) {
                self.active_request_route_group = None;
            }
            self.status = format!("collapsed {}", group_label(&group));
        }
        self.apply_filter();
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
    }

    pub(crate) fn set_focus(&mut self, focus: FocusPane) {
        self.focus = match self.view {
            WorkbenchView::Network => match focus {
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

    pub(crate) fn next_focus(&mut self) {
        self.focus = match self.view {
            WorkbenchView::Network => match self.focus {
                FocusPane::Requests => FocusPane::Detail,
                FocusPane::Detail => FocusPane::Body,
                _ => FocusPane::Requests,
            },
            WorkbenchView::Console => FocusPane::Console,
            WorkbenchView::WebSockets => FocusPane::WebSockets,
            WorkbenchView::Scripts => FocusPane::Scripts,
            WorkbenchView::Storage => FocusPane::Storage,
            WorkbenchView::Cookies => FocusPane::Cookies,
        };
    }

    pub(crate) fn next_tab(&mut self) {
        self.detail_tab = self.detail_tab.next();
        self.detail_scroll = 0;
    }

    pub(crate) fn previous_tab(&mut self) {
        self.detail_tab = self.detail_tab.previous();
        self.detail_scroll = 0;
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
                (!self.filtered_request_indices.is_empty())
                    .then_some(self.filtered_request_indices.len() - 1),
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

    pub(crate) fn push_filter_char(&mut self, character: char) {
        match self.view {
            WorkbenchView::Console => {
                self.console_filter.push(character);
                self.apply_console_filter();
            }
            WorkbenchView::WebSockets => {
                self.request_filter.push(character);
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
                self.request_filter.pop();
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
                self.request_filter.clear();
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

    pub(crate) fn clear_request_filter_and_route(&mut self) {
        self.request_filter.clear();
        self.sql_request_filter_ids = None;
        self.sql_request_filter_query = None;
        self.active_request_route_group = None;
        self.apply_filter();
        self.status = "cleared request filter and route".to_string();
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

    pub(crate) fn cycle_filter_preset(&mut self) {
        let current = FILTER_PRESETS
            .iter()
            .position(|preset| preset.query == self.request_filter);
        let next = current.map(|index| index + 1).unwrap_or(1) % FILTER_PRESETS.len();
        self.request_filter = FILTER_PRESETS[next].query.to_string();
        self.apply_filter();
        self.status = if self.request_filter.is_empty() {
            "filter preset all".to_string()
        } else {
            format!("filter preset {}", FILTER_PRESETS[next].label)
        };
    }

    pub(crate) fn active_filter_preset_label(&self) -> Option<&'static str> {
        FILTER_PRESETS
            .iter()
            .find(|preset| preset.query == self.request_filter)
            .map(|preset| preset.label)
    }

    pub(crate) fn next_sort_mode(&mut self) {
        self.sort_mode = self.sort_mode.next();
        self.apply_filter();
    }

    pub(crate) fn toggle_sort_direction(&mut self) {
        self.sort_descending = !self.sort_descending;
        self.apply_filter();
    }

    fn next_request(&mut self) {
        if self.filtered_request_indices.is_empty() {
            return;
        }
        let next = match self.table_state.selected() {
            Some(index) if index + 1 < self.filtered_request_indices.len() => index + 1,
            _ => 0,
        };
        self.table_state.select(Some(next));
        self.reset_request_view_scroll();
        self.hydrate_selected_request();
    }

    fn previous_request(&mut self) {
        if self.filtered_request_indices.is_empty() {
            return;
        }
        let previous = match self.table_state.selected() {
            Some(0) | None => self.filtered_request_indices.len() - 1,
            Some(index) => index - 1,
        };
        self.table_state.select(Some(previous));
        self.reset_request_view_scroll();
        self.hydrate_selected_request();
    }

    fn next_console(&mut self) {
        if self.filtered_console_indices.is_empty() {
            return;
        }
        let next = match self.console_state.selected() {
            Some(index) if index + 1 < self.filtered_console_indices.len() => index + 1,
            _ => 0,
        };
        self.console_state.select(Some(next));
    }

    fn previous_console(&mut self) {
        if self.filtered_console_indices.is_empty() {
            return;
        }
        let previous = match self.console_state.selected() {
            Some(0) | None => self.filtered_console_indices.len() - 1,
            Some(index) => index - 1,
        };
        self.console_state.select(Some(previous));
    }

    fn next_websocket_frame(&mut self) {
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

    fn previous_websocket_frame(&mut self) {
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

    fn next_script(&mut self) {
        if self.scripts.is_empty() {
            return;
        }
        let next = match self.script_state.selected() {
            Some(index) if index + 1 < self.scripts.len() => index + 1,
            _ => 0,
        };
        self.script_state.select(Some(next));
    }

    fn previous_script(&mut self) {
        if self.scripts.is_empty() {
            return;
        }
        let previous = match self.script_state.selected() {
            Some(0) | None => self.scripts.len() - 1,
            Some(index) => index - 1,
        };
        self.script_state.select(Some(previous));
    }

    fn next_storage_entry(&mut self) {
        let len = self.current_storage_entries().len();
        if len == 0 {
            return;
        }
        self.storage_selected = (self.storage_selected + 1).min(len - 1);
        self.storage_scroll = self
            .storage_selected
            .saturating_sub(3)
            .min(u16::MAX as usize) as u16;
    }

    fn previous_storage_entry(&mut self) {
        if self.current_storage_entries().is_empty() {
            return;
        }
        self.storage_selected = self.storage_selected.saturating_sub(1);
        self.storage_scroll = self
            .storage_selected
            .saturating_sub(3)
            .min(u16::MAX as usize) as u16;
    }

    fn next_cookie_entry(&mut self) {
        let len = self.current_cookie_entries().len();
        if len == 0 {
            return;
        }
        self.cookie_selected = (self.cookie_selected + 1).min(len - 1);
        self.cookie_scroll = self
            .cookie_selected
            .saturating_sub(3)
            .min(u16::MAX as usize) as u16;
    }

    fn previous_cookie_entry(&mut self) {
        if self.current_cookie_entries().is_empty() {
            return;
        }
        self.cookie_selected = self.cookie_selected.saturating_sub(1);
        self.cookie_scroll = self
            .cookie_selected
            .saturating_sub(3)
            .min(u16::MAX as usize) as u16;
    }

    pub(crate) fn current_storage_entries(&self) -> Vec<CurrentStorageEntry> {
        let mut entries: BTreeMap<(String, String, String), String> = BTreeMap::new();

        for snapshot in &self.storage_snapshots {
            for entry in &snapshot.entries {
                entries.insert(
                    (
                        snapshot.storage_type.clone(),
                        snapshot.origin.clone(),
                        entry.key.clone(),
                    ),
                    entry.value.clone(),
                );
            }
        }

        for event in &self.storage_events {
            match event.operation.as_str() {
                "clear" => {
                    let storage_type = &event.storage_type;
                    let origin = &event.origin;
                    entries.retain(|(entry_type, entry_origin, _), _| {
                        entry_type != storage_type || entry_origin != origin
                    });
                }
                "remove" => {
                    if let Some(key) = &event.key {
                        entries.remove(&(
                            event.storage_type.clone(),
                            event.origin.clone(),
                            key.clone(),
                        ));
                    }
                }
                _ => {
                    if let (Some(key), Some(value)) = (&event.key, &event.new_value) {
                        entries.insert(
                            (
                                event.storage_type.clone(),
                                event.origin.clone(),
                                key.clone(),
                            ),
                            value.clone(),
                        );
                    }
                }
            }
        }

        entries
            .into_iter()
            .map(|((storage_type, origin, key), value)| CurrentStorageEntry {
                storage_type,
                origin,
                key,
                value,
            })
            .collect()
    }

    pub(crate) fn current_cookie_entries(&self) -> Vec<CurrentCookieEntry> {
        let mut entries: BTreeMap<(String, String, String), CurrentCookieEntry> = BTreeMap::new();

        for snapshot in &self.cookie_snapshots {
            for cookie in &snapshot.cookies {
                entries.insert(
                    (
                        cookie.domain.clone(),
                        cookie.path.clone(),
                        cookie.name.clone(),
                    ),
                    CurrentCookieEntry {
                        name: cookie.name.clone(),
                        value: cookie.value.clone(),
                        domain: cookie.domain.clone(),
                        path: cookie.path.clone(),
                        expires: cookie.expires,
                        http_only: cookie.http_only,
                        secure: cookie.secure,
                        same_site: cookie.same_site.clone(),
                        flags: cookie_flags(
                            cookie.http_only,
                            cookie.secure,
                            cookie.same_site.as_deref(),
                        ),
                    },
                );
            }
        }

        for event in &self.cookie_events {
            let Some(name) = event.name.as_ref() else {
                continue;
            };
            if event.operation == "delete" || event.operation == "expire" {
                let domain = event.domain.clone().unwrap_or_default();
                let path = event.path.clone().unwrap_or_else(|| "/".to_string());
                entries.remove(&(domain, path, name.clone()));
                continue;
            }

            let domain = event.domain.clone().unwrap_or_default();
            let path = event.path.clone().unwrap_or_else(|| "/".to_string());
            let value = event.value.clone().unwrap_or_default();
            let flags = cookie_event_flags(event.attributes_json.as_ref());

            entries.insert(
                (domain.clone(), path.clone(), name.clone()),
                CurrentCookieEntry {
                    name: name.clone(),
                    value,
                    domain,
                    path,
                    expires: None,
                    http_only: flags.contains("httpOnly"),
                    secure: flags.contains("secure"),
                    same_site: None,
                    flags,
                },
            );
        }

        entries.into_values().collect()
    }

    pub(crate) fn selected_storage_entry(&self) -> Option<CurrentStorageEntry> {
        self.current_storage_entries()
            .get(self.storage_selected)
            .cloned()
    }

    pub(crate) fn selected_cookie_entry(&self) -> Option<CurrentCookieEntry> {
        self.current_cookie_entries()
            .get(self.cookie_selected)
            .cloned()
    }

    pub(crate) fn selected_script(&self) -> Option<&ScriptRecord> {
        self.script_state
            .selected()
            .and_then(|index| self.scripts.get(index))
    }

    fn apply_filter(&mut self) {
        let selected_id = self
            .selected_request()
            .map(|request| request.request.id.clone());
        let filter = RequestFilter::parse(&self.request_filter);
        self.filtered_request_indices = self
            .requests
            .iter()
            .enumerate()
            .filter_map(|(index, request)| {
                let sql_matches = self
                    .sql_request_filter_ids
                    .as_ref()
                    .is_none_or(|ids| ids.contains(&request.request.id));
                let route_matches = self.request_in_active_route(index);
                (sql_matches
                    && route_matches
                    && filter.matches(request)
                    && !self.request_hidden_by_collapsed_group(index))
                .then_some(index)
            })
            .collect();
        let sort_mode = self.sort_mode;
        let sort_descending = self.sort_descending;
        self.filtered_request_indices.sort_by(|left, right| {
            let ordering = sort_mode.compare(&self.requests[*left], &self.requests[*right]);
            if sort_descending {
                ordering.reverse()
            } else {
                ordering
            }
        });

        let selected = selected_id
            .and_then(|id| self.filtered_index_for_request_id(&id))
            .or_else(|| (!self.filtered_request_indices.is_empty()).then_some(0));
        self.table_state.select(selected);
        self.reset_request_view_scroll();
        self.hydrate_selected_request();
    }

    pub(crate) fn hydrate_selected_request(&mut self) {
        let Some(request_index) = self
            .table_state
            .selected()
            .and_then(|index| self.filtered_request_indices.get(index))
            .copied()
        else {
            return;
        };
        let Some(request) = self.requests.get(request_index) else {
            return;
        };
        if request.details_loaded {
            return;
        }
        let request_id = request.request.id.clone();
        if let Err(error) = self.load_request_details(request_index, &request_id) {
            self.status = format!("request detail load failed: {error}");
            self.note_status_changed();
        }
    }

    fn load_request_details(
        &mut self,
        request_index: usize,
        request_id: &str,
    ) -> anyhow::Result<()> {
        let store = Store::open(&self.db_path)
            .with_context(|| format!("open database {}", self.db_path.display()))?;
        let Some(request) = self.requests.get_mut(request_index) else {
            return Ok(());
        };

        request.request_body =
            body_text_for_ref(&store, request.request.request_body_ref.as_deref())
                .with_context(|| format!("load request body for {}", request.request.id))?;
        request.response_body = request
            .response
            .as_ref()
            .and_then(|response| response.body_ref.as_deref())
            .map(|body_id| body_text_for_ref(&store, Some(body_id)))
            .transpose()
            .with_context(|| format!("load response body for {}", request.request.id))?
            .flatten();
        request.replays = store
            .replays_for_request(request_id)?
            .into_iter()
            .map(|record| {
                let body = body_text_for_ref(&store, record.response_body_ref.as_deref())
                    .with_context(|| format!("load replay body for {}", record.id))?;
                anyhow::Ok(ReplayView { record, body })
            })
            .collect::<anyhow::Result<Vec<_>>>()?;
        request.details_loaded = true;
        Ok(())
    }

    fn apply_console_filter(&mut self) {
        let selected_id = self.selected_console_log().map(|log| log.id.clone());
        let filter = ConsoleFilter::parse(&self.console_filter);
        self.filtered_console_indices = self
            .console_logs
            .iter()
            .enumerate()
            .filter_map(|(index, log)| filter.matches(log).then_some(index))
            .collect();

        let selected = selected_id
            .and_then(|id| self.filtered_index_for_console_id(&id))
            .or_else(|| (!self.filtered_console_indices.is_empty()).then_some(0));
        self.console_state.select(selected);
    }

    fn apply_websocket_filter(&mut self) {
        let selected_id = self
            .selected_websocket_frame()
            .map(|frame| frame.id.clone());
        let filter = self.request_filter.trim().to_lowercase();
        self.filtered_websocket_indices = self
            .websocket_frames
            .iter()
            .enumerate()
            .filter_map(|(index, frame)| websocket_frame_matches(frame, &filter).then_some(index))
            .collect();

        let selected = selected_id
            .and_then(|id| {
                self.filtered_websocket_indices.iter().position(|index| {
                    self.websocket_frames.get(*index).map(|frame| &frame.id) == Some(&id)
                })
            })
            .or_else(|| (!self.filtered_websocket_indices.is_empty()).then_some(0));
        self.websocket_state.select(selected);
    }

    fn filtered_index_for_request_id(&self, request_id: &str) -> Option<usize> {
        self.filtered_request_indices
            .iter()
            .position(|index| self.requests[*index].request.id == request_id)
    }

    fn request_hidden_by_collapsed_group(&self, request_index: usize) -> bool {
        self.request_tree_metas
            .get(request_index)
            .map(|meta| {
                meta.ancestor_keys
                    .iter()
                    .any(|key| self.collapsed_request_groups.contains(key))
            })
            .unwrap_or(false)
    }

    fn request_in_active_route(&self, request_index: usize) -> bool {
        let Some(active_group) = &self.active_request_route_group else {
            return true;
        };
        self.request_tree_metas
            .get(request_index)
            .map(|meta| meta.ancestor_keys.iter().any(|key| key == active_group))
            .unwrap_or(false)
    }

    fn collapsible_group_key_for_request_index(&self, request_index: usize) -> Option<String> {
        let meta = self.request_tree_metas.get(request_index)?;
        meta.group_key
            .as_ref()
            .filter(|key| self.route_group_child_count(key) > 0)
            .cloned()
            .or_else(|| {
                meta.ancestor_keys
                    .iter()
                    .rev()
                    .find_map(|key| (self.route_group_child_count(key) > 0).then(|| key.clone()))
            })
    }

    fn route_group_child_count(&self, group: &str) -> usize {
        if let Some(child_count) = self
            .request_tree_metas
            .iter()
            .find(|meta| meta.group_key.as_deref() == Some(group))
            .map(|meta| meta.child_count)
            && child_count > 0
        {
            return child_count;
        }
        self.request_tree_metas
            .iter()
            .filter(|meta| meta.ancestor_keys.iter().any(|key| key == group))
            .count()
    }

    pub(crate) fn request_tree_meta(&self, request_index: usize) -> Option<RequestTreeMeta> {
        let mut meta = self.request_tree_metas.get(request_index)?.clone();
        let parts = self
            .requests
            .get(request_index)
            .map(request_tree_parts)
            .unwrap_or_default();
        let visible_child_count = self.filtered_route_child_count_for_parts(request_index, &parts);
        if visible_child_count > 0 {
            meta.has_children = true;
            meta.child_count = visible_child_count;
        }
        meta.collapsed = meta
            .group_key
            .as_deref()
            .map(|key| self.collapsed_request_groups.contains(key))
            .unwrap_or(false);
        Some(meta)
    }

    fn filtered_route_child_count_for_parts(
        &self,
        request_index: usize,
        parts: &[String],
    ) -> usize {
        if parts.len() <= 1 {
            return 0;
        }
        self.filtered_request_indices
            .iter()
            .filter(|candidate_index| **candidate_index != request_index)
            .filter_map(|candidate_index| self.requests.get(*candidate_index))
            .map(request_tree_parts)
            .filter(|candidate_parts| {
                candidate_parts.len() > parts.len() && candidate_parts.starts_with(parts)
            })
            .count()
    }

    pub(crate) fn request_open_route_child_count(
        &self,
        request_index: usize,
    ) -> Option<(bool, usize)> {
        let group = self.collapsible_group_key_for_request_index(request_index)?;
        Some((
            self.collapsed_request_groups.contains(&group),
            self.route_group_child_count(&group),
        ))
    }

    pub(crate) fn active_expanded_request_group(&self) -> Option<String> {
        self.active_request_route_group.clone()
    }

    pub(crate) fn active_request_route_breadcrumb(&self) -> Option<String> {
        self.active_expanded_request_group()
            .map(|group| route_breadcrumb_for_group(&group))
    }

    pub(crate) fn active_route_summary(&self) -> Option<RouteSummary> {
        self.active_request_route_group.as_ref()?;
        let mut summary = RouteSummary::default();
        for index in &self.filtered_request_indices {
            let Some(request) = self.requests.get(*index) else {
                continue;
            };
            summary.count += 1;
            match request.status_code() {
                Some(400..=599) => summary.errors += 1,
                None => summary.pending += 1,
                _ => {}
            }
            if let Some(duration) = request.duration_ms() {
                summary.max_duration_ms =
                    Some(summary.max_duration_ms.unwrap_or(duration).max(duration));
                if duration >= 500 {
                    summary.slow += 1;
                }
            }
            if let Some(size) = request
                .response
                .as_ref()
                .and_then(|response| response.body_size)
            {
                summary.total_size += size;
            }
        }
        Some(summary)
    }

    pub(crate) fn request_route_remainder(&self, request_index: usize) -> Option<String> {
        let active_group = self.active_expanded_request_group()?;
        let meta = self.request_tree_metas.get(request_index)?;
        let in_active_group = meta.ancestor_keys.iter().any(|key| key == &active_group);
        if !in_active_group {
            return None;
        }
        let raw_path = path_for_url(&self.requests.get(request_index)?.request.url);
        Some(strip_route_segments(
            &raw_path,
            group_path_segment_count(&active_group),
        ))
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

    fn reset_request_view_scroll(&mut self) {
        self.detail_scroll = 0;
        self.body_scroll = 0;
        self.body_tree_selected = 0;
        self.body_tree_selected_key = None;
        self.collapsed_body_nodes.clear();
    }

    pub(crate) fn toggle_selected_body_tree_node(&mut self) {
        let items = self.body_tree_items();
        let Some(item) = items.get(self.body_tree_selected) else {
            return;
        };
        if !item.expandable {
            self.status = "body node is not expandable".to_string();
            return;
        }
        if self.collapsed_body_nodes.remove(&item.key) {
            self.status = format!("expanded {}", item.label);
        } else {
            self.collapsed_body_nodes.insert(item.key.clone());
            self.status = format!("collapsed {}", item.label);
        }
        let selected_key = item.key.clone();
        let len = self.body_tree_items().len();
        if len > 0 {
            self.body_tree_selected = self.body_tree_selected.min(len - 1);
        }
        self.select_body_tree_key(&selected_key);
        self.note_status_changed();
    }

    fn has_body_tree(&self) -> bool {
        !self.body_tree_items().is_empty()
    }

    fn next_body_tree_node(&mut self) {
        let len = self.body_tree_items().len();
        if len == 0 {
            return;
        }
        self.body_tree_selected = (self.body_tree_selected + 1).min(len - 1);
        self.sync_body_tree_selected_key();
        self.body_scroll = self
            .body_tree_selected
            .saturating_sub(3)
            .min(u16::MAX as usize) as u16;
    }

    fn previous_body_tree_node(&mut self) {
        if self.body_tree_items().is_empty() {
            return;
        }
        self.body_tree_selected = self.body_tree_selected.saturating_sub(1);
        self.sync_body_tree_selected_key();
        self.body_scroll = self
            .body_tree_selected
            .saturating_sub(3)
            .min(u16::MAX as usize) as u16;
    }

    fn select_body_tree_key(&mut self, key: &str) {
        let items = self.body_tree_items();
        if let Some(index) = items.iter().position(|item| item.key == key) {
            self.body_tree_selected = index;
            self.body_tree_selected_key = Some(key.to_string());
            self.body_scroll = self
                .body_tree_selected
                .saturating_sub(3)
                .min(u16::MAX as usize) as u16;
        } else {
            self.sync_body_tree_selected_key();
        }
    }

    fn sync_body_tree_selected_key(&mut self) {
        let items = self.body_tree_items();
        if let Some(item) = items.get(self.body_tree_selected) {
            self.body_tree_selected_key = Some(item.key.clone());
        } else {
            self.body_tree_selected = 0;
            self.body_tree_selected_key = items.first().map(|item| item.key.clone());
        }
    }

    fn detail_line_count(&self) -> u16 {
        let count = match (self.detail_tab, self.selected_request()) {
            (_, None) => 1,
            (DetailTab::Overview, Some(request)) => {
                9 + request
                    .response
                    .as_ref()
                    .map(|response| usize::from(response.body_truncated))
                    .unwrap_or(0)
            }
            (DetailTab::RequestHeaders, Some(request)) => request.request.request_headers.len() + 2,
            (DetailTab::RequestBody, Some(_)) => self
                .selected_request()
                .map(formatted_request_body)
                .unwrap_or_default()
                .lines()
                .count()
                .max(1),
            (DetailTab::ResponseHeaders, Some(request)) => request
                .response
                .as_ref()
                .map(|response| response.response_headers.len() + 2)
                .unwrap_or(1),
            (DetailTab::ResponseBody, Some(_)) => self
                .selected_request()
                .map(formatted_response_body)
                .unwrap_or_default()
                .lines()
                .count()
                .max(1),
            (DetailTab::Timing, Some(_)) => 6,
            (DetailTab::Replay, Some(request)) => request
                .replays
                .last()
                .and_then(|replay| replay.body.as_deref())
                .map(|body| body.lines().count() + 8)
                .unwrap_or(1),
        };
        count.min(u16::MAX as usize) as u16
    }

    fn body_line_count(&self) -> u16 {
        let tree_count = self.body_tree_items().len();
        if tree_count > 0 {
            return (tree_count + 2).min(u16::MAX as usize) as u16;
        }
        self.selected_request()
            .map(formatted_response_body)
            .unwrap_or_else(|| "No response body captured for this request.".to_string())
            .lines()
            .count()
            .max(1)
            .min(u16::MAX as usize) as u16
    }

    pub(crate) fn body_tree_items(&self) -> Vec<BodyTreeItem> {
        let Some(request) = self.selected_request() else {
            return Vec::new();
        };
        let Some(body) = request.response_body.as_deref() else {
            return Vec::new();
        };
        let mime = request
            .response
            .as_ref()
            .and_then(|response| response.mime_type.as_deref());
        if looks_like_json(mime, body)
            && let Ok(value) = serde_json::from_str::<serde_json::Value>(body)
        {
            let mut items = Vec::new();
            push_json_tree_item(
                &mut items,
                &self.collapsed_body_nodes,
                "$".to_string(),
                "$".to_string(),
                &value,
                0,
            );
            return items;
        }
        if mime.map(|mime| mime.contains("html")).unwrap_or(false) {
            return html_body_tree_items(body, &self.collapsed_body_nodes);
        }
        Vec::new()
    }

    pub(crate) fn copy_curl_text(&self) -> Option<String> {
        self.selected_request().map(build_curl)
    }

    pub(crate) fn replay_curl_args(&self) -> Option<Vec<String>> {
        self.selected_request().map(build_curl_args)
    }

    pub(crate) fn selected_replay_context(&self) -> Option<ReplayContext> {
        let request = self.selected_request()?;
        Some((
            request.request.session_id.clone(),
            request.request.tab_id.clone(),
            request.request.run_id.clone(),
            request.request.id.clone(),
            self.copy_curl_text()?,
        ))
    }

    pub(crate) fn selected_editable_request(&self) -> Option<String> {
        let request = self.selected_request()?;
        let mut text = format!("{} {}\n", request.request.method, request.request.url);
        for header in &request.request.request_headers {
            text.push_str(&format!("{}: {}\n", header.name, header.value));
        }
        text.push('\n');
        if let Some(body) = request.request_body.as_deref() {
            text.push_str(body);
        }
        Some(text)
    }

    pub(crate) fn latest_replay_diff_bodies(&self) -> Option<(String, String)> {
        let request = self.selected_request()?;
        let original = formatted_response_body(request);
        let replay = request.replays.last()?.body.clone().unwrap_or_default();
        Some((original, replay))
    }

    pub(crate) fn selected_response_body_for_editor(&self) -> Option<(String, String)> {
        let request = self.selected_request()?;
        let extension = request
            .response
            .as_ref()
            .and_then(|response| response.mime_type.as_deref())
            .map(extension_for_mime)
            .unwrap_or("txt")
            .to_string();
        Some((formatted_response_body(request), extension))
    }

    pub(crate) fn selected_request_body_for_editor(&self) -> Option<(String, String)> {
        let request = self.selected_request()?;
        let extension = request
            .request
            .request_headers
            .iter()
            .find(|header| header.name.eq_ignore_ascii_case("content-type"))
            .map(|header| extension_for_mime(&header.value))
            .unwrap_or("txt")
            .to_string();
        Some((formatted_request_body(request), extension))
    }
}

fn body_text_for_ref(store: &Store, body_id: Option<&str>) -> anyhow::Result<Option<String>> {
    let Some(body_id) = body_id else {
        return Ok(None);
    };
    store
        .response_body(body_id)
        .with_context(|| format!("load body {body_id}"))?
        .map(|body| body.as_text())
        .transpose()
        .with_context(|| format!("decode body {body_id} as utf-8"))
}

fn websocket_frame_matches(frame: &WebSocketFrameRecord, filter: &str) -> bool {
    if filter.is_empty() {
        return true;
    }
    let direction = frame.direction.as_str();
    let opcode = websocket_opcode_label(frame.opcode);
    direction.contains(filter)
        || opcode.contains(filter)
        || frame.browser_request_id.to_lowercase().contains(filter)
        || frame.payload.to_lowercase().contains(filter)
}

pub(crate) fn websocket_opcode_label(opcode: i64) -> &'static str {
    match opcode {
        0 => "continuation",
        1 => "text",
        2 => "binary",
        8 => "close",
        9 => "ping",
        10 => "pong",
        _ => "other",
    }
}

fn select_session(
    sessions: Vec<Session>,
    target_url: &str,
    active_session_id: Option<&str>,
) -> Option<Session> {
    if let Some(active_session_id) = active_session_id
        && let Some(session) = sessions
            .iter()
            .find(|session| session.id == active_session_id)
            .cloned()
    {
        return Some(session);
    }

    if target_url == "offline" {
        return sessions.into_iter().last();
    }

    sessions
        .into_iter()
        .rev()
        .find(|session| session.root_url.as_deref() == Some(target_url))
}

fn adjusted_percent(value: u16, delta: i16) -> u16 {
    clamp_split_percent(value.saturating_add_signed(delta))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum FocusPane {
    Requests,
    Detail,
    Body,
    Console,
    WebSockets,
    Scripts,
    Storage,
    Cookies,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum WorkbenchView {
    Network,
    Console,
    WebSockets,
    Scripts,
    Storage,
    Cookies,
}

impl WorkbenchView {
    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Network => "network",
            Self::Console => "console",
            Self::WebSockets => "websockets",
            Self::Scripts => "scripts",
            Self::Storage => "storage",
            Self::Cookies => "cookies",
        }
    }
}

impl FocusPane {
    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Requests => "requests",
            Self::Detail => "detail",
            Self::Body => "body",
            Self::Console => "console",
            Self::WebSockets => "websockets",
            Self::Scripts => "scripts",
            Self::Storage => "storage",
            Self::Cookies => "cookies",
        }
    }

    pub(crate) fn parse(value: &str) -> Self {
        match value.trim() {
            "detail" => Self::Detail,
            "body" => Self::Body,
            "console" => Self::Console,
            "websockets" => Self::WebSockets,
            "scripts" => Self::Scripts,
            "storage" => Self::Storage,
            "cookies" => Self::Cookies,
            _ => Self::Requests,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum DetailTab {
    Overview,
    RequestHeaders,
    RequestBody,
    ResponseHeaders,
    ResponseBody,
    Timing,
    Replay,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SortMode {
    Started,
    Status,
    Duration,
    Size,
    Method,
}

impl SortMode {
    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Started => "started",
            Self::Status => "status",
            Self::Duration => "duration",
            Self::Size => "size",
            Self::Method => "method",
        }
    }

    fn next(self) -> Self {
        match self {
            Self::Started => Self::Status,
            Self::Status => Self::Duration,
            Self::Duration => Self::Size,
            Self::Size => Self::Method,
            Self::Method => Self::Started,
        }
    }

    fn compare(self, left: &RequestView, right: &RequestView) -> std::cmp::Ordering {
        match self {
            Self::Started => left.request.started_at.cmp(&right.request.started_at),
            Self::Status => left.status_code().cmp(&right.status_code()),
            Self::Duration => left.duration_ms().cmp(&right.duration_ms()),
            Self::Size => left
                .response
                .as_ref()
                .and_then(|response| response.body_size)
                .cmp(
                    &right
                        .response
                        .as_ref()
                        .and_then(|response| response.body_size),
                ),
            Self::Method => left.request.method.cmp(&right.request.method),
        }
        .then_with(|| left.request.url.cmp(&right.request.url))
    }
}

impl DetailTab {
    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Overview => "overview",
            Self::RequestHeaders => "request headers",
            Self::RequestBody => "request body",
            Self::ResponseHeaders => "response headers",
            Self::ResponseBody => "response body",
            Self::Timing => "timing",
            Self::Replay => "replay",
        }
    }

    fn next(self) -> Self {
        match self {
            Self::Overview => Self::RequestHeaders,
            Self::RequestHeaders => Self::RequestBody,
            Self::RequestBody => Self::ResponseHeaders,
            Self::ResponseHeaders => Self::ResponseBody,
            Self::ResponseBody => Self::Timing,
            Self::Timing => Self::Replay,
            Self::Replay => Self::Overview,
        }
    }

    fn previous(self) -> Self {
        match self {
            Self::Overview => Self::Replay,
            Self::RequestHeaders => Self::Overview,
            Self::RequestBody => Self::RequestHeaders,
            Self::ResponseHeaders => Self::RequestBody,
            Self::ResponseBody => Self::ResponseHeaders,
            Self::Timing => Self::ResponseBody,
            Self::Replay => Self::Timing,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum InputMode {
    Normal,
    Filtering,
    Palette,
}

impl InputMode {
    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Normal => "normal",
            Self::Filtering => "filter",
            Self::Palette => "palette",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PaletteCommand {
    View(WorkbenchView),
    Filter(&'static str),
    ClearFilter,
    SortNext,
    SortDirection,
    ToggleLayout,
    ToggleDensity,
    ToggleHelp,
    OpenBrowser,
    RefreshPage,
    CopyCurl,
    SaveExchange,
    Replay,
    EditReplay,
    DiffReplay,
    OpenEditor,
    EditConsole,
    SqlQuery,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct PaletteEntry {
    pub(crate) title: &'static str,
    pub(crate) hint: &'static str,
    pub(crate) command: PaletteCommand,
}

const PALETTE_ENTRIES: &[PaletteEntry] = &[
    PaletteEntry {
        title: "View: Network",
        hint: "requests traffic http",
        command: PaletteCommand::View(WorkbenchView::Network),
    },
    PaletteEntry {
        title: "View: Console",
        hint: "logs javascript errors",
        command: PaletteCommand::View(WorkbenchView::Console),
    },
    PaletteEntry {
        title: "View: WebSockets",
        hint: "ws frames streaming realtime",
        command: PaletteCommand::View(WorkbenchView::WebSockets),
    },
    PaletteEntry {
        title: "View: Scripts",
        hint: "script workflows rhai javascript",
        command: PaletteCommand::View(WorkbenchView::Scripts),
    },
    PaletteEntry {
        title: "View: Storage",
        hint: "localStorage sessionStorage",
        command: PaletteCommand::View(WorkbenchView::Storage),
    },
    PaletteEntry {
        title: "View: Cookies",
        hint: "cookie jar events",
        command: PaletteCommand::View(WorkbenchView::Cookies),
    },
    PaletteEntry {
        title: "Filter: All",
        hint: "preset clear",
        command: PaletteCommand::Filter(""),
    },
    PaletteEntry {
        title: "Filter: Errors",
        hint: "preset 4xx 5xx",
        command: PaletteCommand::Filter("has:error"),
    },
    PaletteEntry {
        title: "Filter: JSON",
        hint: "preset mime json",
        command: PaletteCommand::Filter("mime:json"),
    },
    PaletteEntry {
        title: "Filter: Fetch",
        hint: "preset fetch",
        command: PaletteCommand::Filter("type:fetch"),
    },
    PaletteEntry {
        title: "Filter: XHR",
        hint: "preset xhr ajax",
        command: PaletteCommand::Filter("type:xhr"),
    },
    PaletteEntry {
        title: "Filter: SSE",
        hint: "preset server sent events event-stream",
        command: PaletteCommand::Filter("mime:event-stream"),
    },
    PaletteEntry {
        title: "Filter: Images",
        hint: "preset png jpg svg webp",
        command: PaletteCommand::Filter("type:image"),
    },
    PaletteEntry {
        title: "Filter: Scripts",
        hint: "preset js javascript",
        command: PaletteCommand::Filter("type:script"),
    },
    PaletteEntry {
        title: "Filter: Styles",
        hint: "preset css stylesheet",
        command: PaletteCommand::Filter("type:stylesheet"),
    },
    PaletteEntry {
        title: "Filter: Documents",
        hint: "preset html document",
        command: PaletteCommand::Filter("type:document"),
    },
    PaletteEntry {
        title: "Filter: With Body",
        hint: "preset payload response",
        command: PaletteCommand::Filter("has:body"),
    },
    PaletteEntry {
        title: "Filter: Slow",
        hint: "preset latency duration",
        command: PaletteCommand::Filter("duration:>500"),
    },
    PaletteEntry {
        title: "Filter: Large",
        hint: "preset bytes size",
        command: PaletteCommand::Filter("size:>100kb"),
    },
    PaletteEntry {
        title: "Filter: Replayed",
        hint: "preset replay history",
        command: PaletteCommand::Filter("has:replay"),
    },
    PaletteEntry {
        title: "Clear Filter",
        hint: "reset search",
        command: PaletteCommand::ClearFilter,
    },
    PaletteEntry {
        title: "Sort: Next Mode",
        hint: "status duration size method",
        command: PaletteCommand::SortNext,
    },
    PaletteEntry {
        title: "Sort: Toggle Direction",
        hint: "ascending descending",
        command: PaletteCommand::SortDirection,
    },
    PaletteEntry {
        title: "Layout: Toggle Focus",
        hint: "maximize pane",
        command: PaletteCommand::ToggleLayout,
    },
    PaletteEntry {
        title: "Layout: Toggle Density",
        hint: "compact comfortable chrome",
        command: PaletteCommand::ToggleDensity,
    },
    PaletteEntry {
        title: "Open Browser",
        hint: "start capture cdp",
        command: PaletteCommand::OpenBrowser,
    },
    PaletteEntry {
        title: "Refresh Page",
        hint: "reload browser cdp f5",
        command: PaletteCommand::RefreshPage,
    },
    PaletteEntry {
        title: "Copy Curl",
        hint: "selected request",
        command: PaletteCommand::CopyCurl,
    },
    PaletteEntry {
        title: "Save Exchange",
        hint: "selected request response",
        command: PaletteCommand::SaveExchange,
    },
    PaletteEntry {
        title: "Replay Request",
        hint: "curl selected",
        command: PaletteCommand::Replay,
    },
    PaletteEntry {
        title: "Edit Replay Request",
        hint: "modify curl",
        command: PaletteCommand::EditReplay,
    },
    PaletteEntry {
        title: "Diff Latest Replay",
        hint: "compare response",
        command: PaletteCommand::DiffReplay,
    },
    PaletteEntry {
        title: "Open Body in Editor",
        hint: "response request",
        command: PaletteCommand::OpenEditor,
    },
    PaletteEntry {
        title: "Console: Evaluate JS",
        hint: "scratch expression",
        command: PaletteCommand::EditConsole,
    },
    PaletteEntry {
        title: "SQL Query",
        hint: "read-only sqlite workbench database",
        command: PaletteCommand::SqlQuery,
    },
    PaletteEntry {
        title: "Show Keys",
        hint: "help modal shortcuts",
        command: PaletteCommand::ToggleHelp,
    },
];

fn palette_matches(entry: &PaletteEntry, query: &str) -> bool {
    let query = query.trim().to_lowercase();
    if query.is_empty() {
        return true;
    }
    let haystack = format!("{} {}", entry.title, entry.hint).to_lowercase();
    query
        .split_whitespace()
        .all(|part| fuzzy_contains(&haystack, part))
}

#[derive(Debug, Clone)]
pub(crate) struct SqlResultsView {
    pub(crate) query: String,
    pub(crate) columns: Vec<String>,
    pub(crate) rows: Vec<Vec<String>>,
    pub(crate) duration_ms: u128,
    pub(crate) error: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub(crate) struct RouteSummary {
    pub(crate) count: usize,
    pub(crate) errors: usize,
    pub(crate) pending: usize,
    pub(crate) slow: usize,
    pub(crate) total_size: i64,
    pub(crate) max_duration_ms: Option<i64>,
}

fn fuzzy_contains(haystack: &str, needle: &str) -> bool {
    let mut chars = haystack.chars();
    needle
        .chars()
        .all(|needle_char| chars.any(|haystack_char| haystack_char == needle_char))
}

pub(crate) struct RequestView {
    pub(crate) request: RequestRecord,
    pub(crate) response: Option<ResponseRecord>,
    pub(crate) request_body: Option<String>,
    pub(crate) response_body: Option<String>,
    pub(crate) replays: Vec<ReplayView>,
    pub(crate) details_loaded: bool,
}

#[derive(Debug, Clone)]
pub(crate) struct CurrentStorageEntry {
    pub(crate) storage_type: String,
    pub(crate) origin: String,
    pub(crate) key: String,
    pub(crate) value: String,
}

#[derive(Debug, Clone)]
pub(crate) struct CurrentCookieEntry {
    pub(crate) name: String,
    pub(crate) value: String,
    pub(crate) domain: String,
    pub(crate) path: String,
    pub(crate) expires: Option<f64>,
    pub(crate) http_only: bool,
    pub(crate) secure: bool,
    pub(crate) same_site: Option<String>,
    pub(crate) flags: String,
}

impl CurrentCookieEntry {
    pub(crate) fn to_cookie_record(&self) -> faro_core::CookieRecord {
        faro_core::CookieRecord {
            name: self.name.clone(),
            value: self.value.clone(),
            domain: self.domain.clone(),
            path: self.path.clone(),
            expires: self.expires,
            http_only: self.http_only,
            secure: self.secure,
            same_site: self.same_site.clone(),
        }
    }
}

#[derive(Clone)]
pub(crate) struct RequestTreeMeta {
    pub(crate) depth: usize,
    pub(crate) group_key: Option<String>,
    pub(crate) ancestor_keys: Vec<String>,
    pub(crate) has_children: bool,
    pub(crate) child_count: usize,
    pub(crate) collapsed: bool,
}

#[derive(Clone)]
pub(crate) struct BodyTreeItem {
    pub(crate) key: String,
    pub(crate) depth: usize,
    pub(crate) label: String,
    pub(crate) value: Option<String>,
    pub(crate) expandable: bool,
    pub(crate) collapsed: bool,
}

pub(crate) struct ReplayView {
    pub(crate) record: ReplayRecord,
    pub(crate) body: Option<String>,
}

impl RequestView {
    pub(crate) fn status_code(&self) -> Option<i64> {
        self.response
            .as_ref()
            .and_then(|response| response.status_code)
    }

    pub(crate) fn duration_ms(&self) -> Option<i64> {
        Some(self.request.completed_at? - self.request.started_at)
    }
}

struct RequestFilter {
    raw_terms: Vec<FilterPattern>,
    method: Option<FilterPattern>,
    status: Option<String>,
    resource_type: Option<FilterPattern>,
    domain: Option<FilterPattern>,
    url: Option<FilterPattern>,
    path: Option<FilterPattern>,
    mime: Option<FilterPattern>,
    header: Option<FilterPattern>,
    body: Option<FilterPattern>,
    request_body: Option<FilterPattern>,
    response_body: Option<FilterPattern>,
    has: Vec<String>,
    duration: Option<Threshold>,
    size: Option<Threshold>,
}

struct ConsoleFilter {
    raw_terms: Vec<FilterPattern>,
    level: Option<FilterPattern>,
    source: Option<FilterPattern>,
    kind: Option<String>,
}

struct FilterPattern {
    lower: String,
    regex: Option<Regex>,
}

impl FilterPattern {
    fn parse(value: &str) -> Self {
        let (raw, explicit_regex) = slash_pattern(value).unwrap_or((value, false));
        let mut regex = None;
        if explicit_regex || looks_like_regex(raw) {
            let compiled = RegexBuilder::new(raw).case_insensitive(true).build();
            if let Ok(compiled_regex) = compiled {
                regex = Some(compiled_regex);
            }
        }
        Self {
            lower: raw.to_lowercase(),
            regex,
        }
    }

    fn matches(&self, value: &str) -> bool {
        self.regex
            .as_ref()
            .map(|regex| regex.is_match(value))
            .unwrap_or_else(|| contains_lower(value, &self.lower))
    }
}

fn slash_pattern(value: &str) -> Option<(&str, bool)> {
    (value.len() >= 2 && value.starts_with('/') && value.ends_with('/'))
        .then(|| (&value[1..value.len() - 1], true))
}

fn looks_like_regex(value: &str) -> bool {
    value.chars().any(|ch| {
        matches!(
            ch,
            '^' | '$' | '.' | '*' | '+' | '?' | '(' | ')' | '[' | ']' | '{' | '}' | '|' | '\\'
        )
    })
}

impl ConsoleFilter {
    fn parse(input: &str) -> Self {
        let mut filter = Self {
            raw_terms: Vec::new(),
            level: None,
            source: None,
            kind: None,
        };

        for token in input.split_whitespace() {
            let Some((key, value)) = token.split_once(':') else {
                if token.eq_ignore_ascii_case("eval") {
                    filter.kind = Some("eval".to_string());
                } else {
                    filter.raw_terms.push(FilterPattern::parse(token));
                }
                continue;
            };
            let value = value.trim().to_lowercase();
            if value.is_empty() {
                continue;
            }
            match key.to_lowercase().as_str() {
                "level" => filter.level = Some(FilterPattern::parse(&value)),
                "source" => filter.source = Some(FilterPattern::parse(&value)),
                "kind" | "type" => filter.kind = Some(value),
                _ => filter.raw_terms.push(FilterPattern::parse(token)),
            }
        }

        filter
    }

    fn matches(&self, log: &ConsoleLog) -> bool {
        if let Some(level) = &self.level {
            let log_level = console_level_name(&log.level);
            let level_matches = level.matches(log_level)
                || (level.matches("error") && matches!(log.level, ConsoleLevel::Fatal));
            if !level_matches {
                return false;
            }
        }

        if let Some(source) = &self.source {
            let log_source = log.source.as_deref().unwrap_or("-");
            if !source.matches(log_source) {
                return false;
            }
        }

        if let Some(kind) = &self.kind {
            let is_eval = log.source.as_deref() == Some("faro-console");
            match kind.as_str() {
                "eval" if !is_eval => return false,
                "page" if is_eval => return false,
                _ => {}
            }
        }

        if self.raw_terms.is_empty() {
            return true;
        }

        let haystack = [
            log.message.as_str(),
            log.source.as_deref().unwrap_or_default(),
            console_level_name(&log.level),
        ]
        .join(" ");
        self.raw_terms.iter().all(|term| term.matches(&haystack))
    }
}

fn console_level_name(level: &ConsoleLevel) -> &'static str {
    match level {
        ConsoleLevel::Trace => "trace",
        ConsoleLevel::Debug => "debug",
        ConsoleLevel::Info => "info",
        ConsoleLevel::Warning => "warn",
        ConsoleLevel::Error => "error",
        ConsoleLevel::Fatal => "fatal",
    }
}

fn cookie_flags(http_only: bool, secure: bool, same_site: Option<&str>) -> String {
    let mut flags = Vec::new();
    if http_only {
        flags.push("httpOnly".to_string());
    }
    if secure {
        flags.push("secure".to_string());
    }
    if let Some(same_site) = same_site {
        flags.push(format!("sameSite={same_site}"));
    }
    flags.join(",")
}

fn cookie_event_flags(attributes_json: Option<&serde_json::Value>) -> String {
    let Some(attributes) = attributes_json.and_then(|value| value.as_object()) else {
        return String::new();
    };

    attributes
        .keys()
        .filter(|key| !matches!(key.as_str(), "name" | "value" | "domain" | "path"))
        .cloned()
        .collect::<Vec<_>>()
        .join(",")
}

struct FilterPreset {
    label: &'static str,
    query: &'static str,
}

const FILTER_PRESETS: &[FilterPreset] = &[
    FilterPreset {
        label: "all",
        query: "",
    },
    FilterPreset {
        label: "errors",
        query: "has:error",
    },
    FilterPreset {
        label: "json",
        query: "mime:json",
    },
    FilterPreset {
        label: "fetch",
        query: "type:fetch",
    },
    FilterPreset {
        label: "xhr",
        query: "type:xhr",
    },
    FilterPreset {
        label: "sse",
        query: "mime:event-stream",
    },
    FilterPreset {
        label: "images",
        query: "type:image",
    },
    FilterPreset {
        label: "scripts",
        query: "type:script",
    },
    FilterPreset {
        label: "styles",
        query: "type:stylesheet",
    },
    FilterPreset {
        label: "docs",
        query: "type:document",
    },
    FilterPreset {
        label: "with body",
        query: "has:body",
    },
    FilterPreset {
        label: "slow",
        query: "duration:>500",
    },
    FilterPreset {
        label: "large",
        query: "size:>100kb",
    },
    FilterPreset {
        label: "replayed",
        query: "has:replay",
    },
];

fn filter_query_for_preset_label(label: &str) -> Option<&'static str> {
    FILTER_PRESETS
        .iter()
        .find(|preset| preset.label == label)
        .map(|preset| preset.query)
}

#[derive(Debug, Clone, Copy)]
struct Threshold {
    op: ThresholdOp,
    value: i64,
}

#[derive(Debug, Clone, Copy)]
enum ThresholdOp {
    Gt,
    Gte,
    Lt,
    Lte,
    Eq,
}

impl RequestFilter {
    fn parse(input: &str) -> Self {
        let mut filter = Self {
            raw_terms: Vec::new(),
            method: None,
            status: None,
            resource_type: None,
            domain: None,
            url: None,
            path: None,
            mime: None,
            header: None,
            body: None,
            request_body: None,
            response_body: None,
            has: Vec::new(),
            duration: None,
            size: None,
        };

        for token in input.split_whitespace() {
            if let Some(value) = token.strip_prefix("method:") {
                filter.method = Some(FilterPattern::parse(value));
            } else if let Some(value) = token.strip_prefix("status:") {
                filter.status = Some(value.to_lowercase());
            } else if let Some(value) = token.strip_prefix("type:") {
                filter.resource_type = Some(FilterPattern::parse(value));
            } else if let Some(value) = token.strip_prefix("domain:") {
                filter.domain = Some(FilterPattern::parse(value));
            } else if let Some(value) = token.strip_prefix("url:") {
                filter.url = Some(FilterPattern::parse(value));
            } else if let Some(value) = token.strip_prefix("path:") {
                filter.path = Some(FilterPattern::parse(value));
            } else if let Some(value) = token.strip_prefix("mime:") {
                filter.mime = Some(FilterPattern::parse(value));
            } else if let Some(value) = token.strip_prefix("header:") {
                filter.header = Some(FilterPattern::parse(value));
            } else if let Some(value) = token.strip_prefix("body:") {
                filter.body = Some(FilterPattern::parse(value));
            } else if let Some(value) = token.strip_prefix("reqbody:") {
                filter.request_body = Some(FilterPattern::parse(value));
            } else if let Some(value) = token.strip_prefix("resbody:") {
                filter.response_body = Some(FilterPattern::parse(value));
            } else if let Some(value) = token.strip_prefix("has:") {
                filter.has.push(value.to_lowercase());
            } else if let Some(value) = token.strip_prefix("duration:") {
                filter.duration = parse_threshold(value, |_| None);
            } else if let Some(value) = token.strip_prefix("size:") {
                filter.size = parse_threshold(value, byte_multiplier);
            } else {
                filter.raw_terms.push(FilterPattern::parse(token));
            }
        }

        filter
    }

    #[allow(clippy::collapsible_if)]
    fn matches(&self, request: &RequestView) -> bool {
        if self.is_empty() {
            return true;
        }

        if let Some(method) = &self.method {
            if !method.matches(&request.request.method) {
                return false;
            }
        }

        if let Some(status) = &self.status {
            if !matches_status_filter(request.status_code(), status) {
                return false;
            }
        }

        if let Some(resource_type) = &self.resource_type {
            if !resource_type.matches(request.request.resource_type.as_deref().unwrap_or("")) {
                return false;
            }
        }

        if let Some(domain) = &self.domain {
            if !domain.matches(&domain_for_url(&request.request.url)) {
                return false;
            }
        }

        if let Some(url) = &self.url {
            if !url.matches(&request.request.url) {
                return false;
            }
        }

        if let Some(path) = &self.path {
            if !path.matches(&path_for_url(&request.request.url)) {
                return false;
            }
        }

        if let Some(mime) = &self.mime {
            if !request
                .response
                .as_ref()
                .and_then(|response| response.mime_type.as_deref())
                .map(|value| mime.matches(value))
                .unwrap_or(false)
            {
                return false;
            }
        }

        if let Some(header) = &self.header {
            if !headers_contain(request, header) {
                return false;
            }
        }

        if let Some(body) = &self.body {
            if !body_contains(request, body) {
                return false;
            }
        }

        if let Some(body) = &self.request_body {
            if !request
                .request_body
                .as_deref()
                .map(|value| body.matches(value))
                .unwrap_or(false)
            {
                return false;
            }
        }

        if let Some(body) = &self.response_body {
            if !request
                .response_body
                .as_deref()
                .map(|value| body.matches(value))
                .unwrap_or(false)
            {
                return false;
            }
        }

        if self
            .has
            .iter()
            .any(|value| !matches_has_filter(request, value))
        {
            return false;
        }

        if let Some(threshold) = self.duration {
            if !request
                .duration_ms()
                .map(|duration| threshold.matches(duration))
                .unwrap_or(false)
            {
                return false;
            }
        }

        if let Some(threshold) = self.size {
            if !request
                .response
                .as_ref()
                .and_then(|response| response.body_size)
                .map(|size| threshold.matches(size))
                .unwrap_or(false)
            {
                return false;
            }
        }

        self.raw_terms.iter().all(|term| {
            term.matches(&request.request.method)
                || term.matches(&request.request.url)
                || request
                    .request
                    .resource_type
                    .as_deref()
                    .map(|resource_type| term.matches(resource_type))
                    .unwrap_or(false)
                || request
                    .status_code()
                    .map(|status| term.matches(&status.to_string()))
                    .unwrap_or(false)
                || request
                    .response
                    .as_ref()
                    .and_then(|response| response.mime_type.as_deref())
                    .map(|mime| term.matches(mime))
                    .unwrap_or(false)
                || headers_contain(request, term)
                || body_contains(request, term)
        })
    }

    fn is_empty(&self) -> bool {
        self.raw_terms.is_empty()
            && self.method.is_none()
            && self.status.is_none()
            && self.resource_type.is_none()
            && self.domain.is_none()
            && self.url.is_none()
            && self.path.is_none()
            && self.mime.is_none()
            && self.header.is_none()
            && self.body.is_none()
            && self.request_body.is_none()
            && self.response_body.is_none()
            && self.has.is_empty()
            && self.duration.is_none()
            && self.size.is_none()
    }
}

impl Threshold {
    fn matches(self, value: i64) -> bool {
        match self.op {
            ThresholdOp::Gt => value > self.value,
            ThresholdOp::Gte => value >= self.value,
            ThresholdOp::Lt => value < self.value,
            ThresholdOp::Lte => value <= self.value,
            ThresholdOp::Eq => value == self.value,
        }
    }
}

fn parse_threshold(input: &str, multiplier: impl Fn(&str) -> Option<i64>) -> Option<Threshold> {
    let (op, value) = if let Some(value) = input.strip_prefix(">=") {
        (ThresholdOp::Gte, value)
    } else if let Some(value) = input.strip_prefix("<=") {
        (ThresholdOp::Lte, value)
    } else if let Some(value) = input.strip_prefix('>') {
        (ThresholdOp::Gt, value)
    } else if let Some(value) = input.strip_prefix('<') {
        (ThresholdOp::Lt, value)
    } else if let Some(value) = input.strip_prefix('=') {
        (ThresholdOp::Eq, value)
    } else {
        (ThresholdOp::Eq, input)
    };

    Some(Threshold {
        op,
        value: parse_threshold_value(value, multiplier)?,
    })
}

fn parse_threshold_value(input: &str, multiplier: impl Fn(&str) -> Option<i64>) -> Option<i64> {
    let input = input.trim().to_lowercase();
    let split = input
        .find(|ch: char| !ch.is_ascii_digit())
        .unwrap_or(input.len());
    let number = match input[..split].parse::<i64>() {
        Ok(number) => number,
        Err(_) => return None,
    };
    let suffix = input[split..].trim();
    let multiplier = if suffix.is_empty() {
        1
    } else {
        multiplier(suffix)?
    };
    Some(number * multiplier)
}

fn byte_multiplier(suffix: &str) -> Option<i64> {
    match suffix {
        "b" => Some(1),
        "kb" | "k" => Some(1024),
        "mb" | "m" => Some(1024 * 1024),
        _ => None,
    }
}

fn contains_lower(value: &str, needle: &str) -> bool {
    value.to_lowercase().contains(needle)
}

fn headers_contain(request: &RequestView, needle: &FilterPattern) -> bool {
    request
        .request
        .request_headers
        .iter()
        .chain(
            request
                .response
                .as_ref()
                .into_iter()
                .flat_map(|response| response.response_headers.iter()),
        )
        .any(|header| needle.matches(&header.name) || needle.matches(&header.value))
}

fn body_contains(request: &RequestView, needle: &FilterPattern) -> bool {
    request
        .request_body
        .as_deref()
        .map(|body| needle.matches(body))
        .unwrap_or(false)
        || request
            .response_body
            .as_deref()
            .map(|body| needle.matches(body))
            .unwrap_or(false)
}

fn matches_has_filter(request: &RequestView, filter: &str) -> bool {
    match filter {
        "body" => request.request_body.is_some() || request.response_body.is_some(),
        "reqbody" | "request-body" => request.request_body.is_some(),
        "resbody" | "response-body" => request.response_body.is_some(),
        "headers" => {
            !request.request.request_headers.is_empty()
                || request
                    .response
                    .as_ref()
                    .map(|response| !response.response_headers.is_empty())
                    .unwrap_or(false)
        }
        "replay" | "replays" => !request.replays.is_empty(),
        "error" => request
            .status_code()
            .map(|status| status >= 400)
            .unwrap_or(false),
        "pending" => request.response.is_none(),
        _ => false,
    }
}

fn matches_status_filter(status_code: Option<i64>, filter: &str) -> bool {
    let Some(status_code) = status_code else {
        return filter == "-";
    };
    if let Some(prefix) = filter.strip_suffix("xx") {
        return status_code.to_string().starts_with(prefix);
    }
    status_code.to_string().contains(filter)
}

pub(crate) fn formatted_response_body(request: &RequestView) -> String {
    let Some(body) = request.response_body.as_deref() else {
        return "No response body captured for this request.".to_string();
    };

    if looks_like_json(
        request
            .response
            .as_ref()
            .and_then(|response| response.mime_type.as_deref()),
        body,
    ) {
        serde_json::from_str::<serde_json::Value>(body)
            .and_then(|value| serde_json::to_string_pretty(&value))
            .unwrap_or_else(|_| body.to_string())
    } else {
        body.to_string()
    }
}

pub(crate) fn formatted_request_body(request: &RequestView) -> String {
    let Some(body) = request.request_body.as_deref() else {
        return "No request body captured for this request.".to_string();
    };

    if looks_like_json(
        request
            .request
            .request_headers
            .iter()
            .find(|header| header.name.eq_ignore_ascii_case("content-type"))
            .map(|header| header.value.as_str()),
        body,
    ) {
        serde_json::from_str::<serde_json::Value>(body)
            .and_then(|value| serde_json::to_string_pretty(&value))
            .unwrap_or_else(|_| body.to_string())
    } else {
        body.to_string()
    }
}

fn looks_like_json(mime_type: Option<&str>, body: &str) -> bool {
    mime_type.map(|mime| mime.contains("json")).unwrap_or(false)
        || body.trim_start().starts_with('{')
        || body.trim_start().starts_with('[')
}

fn push_json_tree_item(
    items: &mut Vec<BodyTreeItem>,
    collapsed: &HashSet<String>,
    key: String,
    label: String,
    value: &serde_json::Value,
    depth: usize,
) {
    let expandable = matches!(
        value,
        serde_json::Value::Object(_) | serde_json::Value::Array(_)
    );
    let value_label = json_tree_value_label(value);
    let is_collapsed = collapsed.contains(&key);
    items.push(BodyTreeItem {
        key: key.clone(),
        depth,
        label,
        value: value_label,
        expandable,
        collapsed: is_collapsed,
    });
    if !expandable || is_collapsed {
        return;
    }
    match value {
        serde_json::Value::Object(object) => {
            for (field, child) in object {
                push_json_tree_item(
                    items,
                    collapsed,
                    format!("{key}.{field}"),
                    field.clone(),
                    child,
                    depth + 1,
                );
            }
        }
        serde_json::Value::Array(array) => {
            for (index, child) in array.iter().enumerate() {
                push_json_tree_item(
                    items,
                    collapsed,
                    format!("{key}[{index}]"),
                    format!("[{index}]"),
                    child,
                    depth + 1,
                );
            }
        }
        _ => {}
    }
}

fn json_tree_value_label(value: &serde_json::Value) -> Option<String> {
    match value {
        serde_json::Value::Object(object) => Some(format!("{{{}}}", object.len())),
        serde_json::Value::Array(array) => Some(format!("[{}]", array.len())),
        serde_json::Value::String(value) => Some(format!("\"{}\"", compact_string(value, 90))),
        serde_json::Value::Number(value) => Some(value.to_string()),
        serde_json::Value::Bool(value) => Some(value.to_string()),
        serde_json::Value::Null => Some("null".to_string()),
    }
}

fn compact_string(value: &str, max: usize) -> String {
    if value.chars().count() <= max {
        return value.to_string();
    }
    value
        .chars()
        .take(max.saturating_sub(1))
        .collect::<String>()
        + "…"
}

fn html_body_tree_items(body: &str, collapsed: &HashSet<String>) -> Vec<BodyTreeItem> {
    let mut items = Vec::new();
    let mut depth = 0_usize;
    let mut cursor = 0_usize;
    let mut sequence = Vec::<usize>::new();
    let mut hidden_depth = None::<usize>;
    while cursor < body.len() {
        let remaining = &body[cursor..];
        let Some(tag_start_offset) = remaining.find('<') else {
            push_html_tree_text(&mut items, hidden_depth, depth, &mut sequence, remaining);
            break;
        };
        if tag_start_offset > 0 {
            push_html_tree_text(
                &mut items,
                hidden_depth,
                depth,
                &mut sequence,
                &remaining[..tag_start_offset],
            );
        }
        let tag_start = cursor + tag_start_offset;
        let Some(tag_end_offset) = body[tag_start..].find('>') else {
            push_html_tree_text(
                &mut items,
                hidden_depth,
                depth,
                &mut sequence,
                &body[tag_start..],
            );
            break;
        };
        let tag_end = tag_start + tag_end_offset + 1;
        let tag = body[tag_start..tag_end].trim();
        if tag.starts_with("</") {
            depth = depth.saturating_sub(1);
            if hidden_depth == Some(depth) {
                hidden_depth = None;
            }
            cursor = tag_end;
            continue;
        }
        let key = next_html_tree_key(&mut sequence, depth);
        let (name, attrs) = html_tree_tag_name_and_attrs(tag);
        let expandable = !html_tree_tag_is_self_closing(tag);
        if hidden_depth.is_none() {
            items.push(BodyTreeItem {
                key: key.clone(),
                depth,
                label: name,
                value: (!attrs.is_empty()).then_some(attrs),
                expandable,
                collapsed: expandable && collapsed.contains(&key),
            });
            if expandable && collapsed.contains(&key) {
                hidden_depth = Some(depth);
            }
        }
        if expandable {
            depth = depth.saturating_add(1).min(24);
        }
        cursor = tag_end;
    }
    items
}

fn push_html_tree_text(
    items: &mut Vec<BodyTreeItem>,
    hidden_depth: Option<usize>,
    depth: usize,
    sequence: &mut Vec<usize>,
    text: &str,
) {
    if hidden_depth.is_some() {
        return;
    }
    let text = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if text.is_empty() {
        return;
    }
    items.push(BodyTreeItem {
        key: next_html_tree_key(sequence, depth),
        depth,
        label: "text".to_string(),
        value: Some(compact_string(&text, 120)),
        expandable: false,
        collapsed: false,
    });
}

fn next_html_tree_key(sequence: &mut Vec<usize>, depth: usize) -> String {
    if sequence.len() <= depth {
        sequence.resize(depth + 1, 0);
    }
    sequence[depth] += 1;
    sequence.truncate(depth + 1);
    format!(
        "html:{}",
        sequence
            .iter()
            .map(usize::to_string)
            .collect::<Vec<_>>()
            .join(".")
    )
}

fn html_tree_tag_name_and_attrs(tag: &str) -> (String, String) {
    let inner = tag
        .trim()
        .trim_start_matches('<')
        .trim_end_matches('>')
        .trim_end_matches('/')
        .trim_start_matches('/')
        .trim();
    let mut parts = inner.splitn(2, char::is_whitespace);
    let name = parts.next().unwrap_or("").to_string();
    let attrs = parts
        .next()
        .map(|attrs| compact_string(attrs.trim(), 90))
        .unwrap_or_default();
    (name, attrs)
}

fn html_tree_tag_is_self_closing(tag: &str) -> bool {
    let (name, _) = html_tree_tag_name_and_attrs(tag);
    tag.ends_with("/>")
        || matches!(
            name.as_str(),
            "!doctype"
                | "area"
                | "base"
                | "br"
                | "col"
                | "embed"
                | "hr"
                | "img"
                | "input"
                | "link"
                | "meta"
                | "param"
                | "source"
                | "track"
                | "wbr"
        )
}

fn build_curl(request: &RequestView) -> String {
    format!(
        "curl {}",
        build_curl_args(request)
            .into_iter()
            .map(|arg| shell_quote(&arg))
            .collect::<Vec<_>>()
            .join(" ")
    )
}

fn build_curl_args(request: &RequestView) -> Vec<String> {
    let mut parts = vec![
        "-sS".to_string(),
        "-i".to_string(),
        "-X".to_string(),
        request.request.method.clone(),
        request.request.url.clone(),
    ];

    for header in &request.request.request_headers {
        parts.push("-H".to_string());
        parts.push(format!("{}: {}", header.name, header.value));
    }

    if let Some(body) = request.request_body.as_deref() {
        parts.push("--data-raw".to_string());
        parts.push(body.to_string());
    }

    parts
}

fn shell_quote(value: &str) -> String {
    if value.is_empty() {
        return "''".to_string();
    }
    format!("'{}'", value.replace('\'', "'\"'\"'"))
}

pub(crate) fn domain_for_url(url: &str) -> String {
    let without_scheme = url
        .strip_prefix("http://")
        .or_else(|| url.strip_prefix("https://"))
        .unwrap_or(url);
    without_scheme
        .split('/')
        .next()
        .unwrap_or(without_scheme)
        .to_string()
}

pub(crate) fn path_for_url(url: &str) -> String {
    let without_scheme = url
        .strip_prefix("http://")
        .or_else(|| url.strip_prefix("https://"))
        .unwrap_or(url);
    without_scheme
        .find('/')
        .map(|index| without_scheme[index..].to_string())
        .unwrap_or_else(|| "/".to_string())
}

fn request_tree_parts(request: &RequestView) -> Vec<String> {
    let mut parts = vec![domain_for_url(&request.request.url)];
    parts.extend(normalized_path_segments(&path_for_url(
        &request.request.url,
    )));
    parts
}

fn build_request_tree_metas(requests: &[RequestView]) -> Vec<RequestTreeMeta> {
    let mut descendant_counts = HashMap::new();
    for group in requests.iter().flat_map(request_group_keys) {
        *descendant_counts.entry(group).or_insert(0) += 1;
    }
    requests
        .iter()
        .map(|request| {
            let parts = request_tree_parts(request);
            let group_key = group_key_for_parts(&parts);
            let ancestor_keys = ancestor_keys_for_parts(&parts);
            let child_count = group_key
                .as_ref()
                .and_then(|key| descendant_counts.get(key).copied())
                .unwrap_or(0);
            RequestTreeMeta {
                depth: parts.len().saturating_sub(1),
                group_key,
                ancestor_keys,
                has_children: child_count > 0,
                child_count,
                collapsed: false,
            }
        })
        .collect()
}

fn normalized_path_segments(path: &str) -> Vec<String> {
    let path = path.split(['?', '#']).next().unwrap_or(path);
    path.split('/')
        .filter(|segment| !segment.is_empty())
        .map(normalize_path_segment)
        .collect()
}

fn normalize_path_segment(segment: &str) -> String {
    if is_dynamic_path_segment(segment) {
        ":id".to_string()
    } else {
        segment.to_string()
    }
}

fn is_dynamic_path_segment(segment: &str) -> bool {
    let trimmed = segment.trim_matches(|ch: char| ch == '-' || ch == '_');
    let hexish = trimmed
        .chars()
        .all(|ch| ch.is_ascii_hexdigit() || ch == '-');
    trimmed.chars().all(|ch| ch.is_ascii_digit())
        || (trimmed.len() >= 8 && hexish)
        || (trimmed.contains('-') && trimmed.len() >= 12 && hexish)
}

fn group_key_for_parts(parts: &[String]) -> Option<String> {
    (parts.len() > 1).then(|| parts.join("/"))
}

fn request_group_keys(request: &RequestView) -> Vec<String> {
    let parts = request_tree_parts(request);
    ancestor_keys_for_parts(&parts)
}

fn ancestor_keys_for_parts(parts: &[String]) -> Vec<String> {
    (2..parts.len()).map(|end| parts[..end].join("/")).collect()
}

fn group_label(group_key: &str) -> String {
    group_key
        .split('/')
        .next_back()
        .map(str::to_string)
        .unwrap_or_else(|| group_key.to_string())
}

fn route_label_for_group(group_key: &str) -> String {
    let mut parts = group_key.split('/');
    let Some(domain) = parts.next() else {
        return group_key.to_string();
    };
    let path = parts.collect::<Vec<_>>().join("/");
    if path.is_empty() {
        domain.to_string()
    } else {
        format!("{domain}/{path}")
    }
}

fn route_breadcrumb_for_group(group_key: &str) -> String {
    group_key.split('/').collect::<Vec<_>>().join(" / ")
}

fn parent_group_key(group_key: &str) -> Option<String> {
    let mut parts = group_key.split('/').collect::<Vec<_>>();
    (parts.len() > 2).then(|| {
        parts.pop();
        parts.join("/")
    })
}

fn group_path_segment_count(group_key: &str) -> usize {
    group_key.split('/').skip(1).count()
}

fn strip_route_segments(path: &str, segment_count: usize) -> String {
    if segment_count == 0 {
        return path.to_string();
    }
    let (path_only, suffix) = path
        .find(['?', '#'])
        .map(|index| (&path[..index], &path[index..]))
        .unwrap_or((path, ""));
    let segments = path_only
        .split('/')
        .filter(|segment| !segment.is_empty())
        .collect::<Vec<_>>();
    if segments.len() <= segment_count {
        return format!("/{suffix}");
    }
    format!("/{}{}", segments[segment_count..].join("/"), suffix)
}

fn extension_for_mime(mime: &str) -> &'static str {
    if mime.contains("json") {
        "json"
    } else if mime.contains("html") {
        "html"
    } else if mime.contains("css") {
        "css"
    } else if mime.contains("javascript") || mime.contains("ecmascript") {
        "js"
    } else if mime.contains("xml") {
        "xml"
    } else {
        "txt"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use faro_core::Header;

    type TestResult = anyhow::Result<()>;

    fn request_view() -> RequestView {
        let mut request = RequestRecord::started(
            "session".to_string(),
            Some("tab".to_string()),
            Some("run".to_string()),
            "POST",
            "http://localhost:5173/api/users?active=true",
        );
        request.resource_type = Some("fetch".to_string());
        request
            .request_headers
            .push(Header::new("content-type", "application/json"));
        request.request_headers.push(Header::new("x-debug", "yes"));

        let mut response = ResponseRecord::received(request.id.clone());
        response.status_code = Some(500);
        response.mime_type = Some("application/json".to_string());
        response
            .response_headers
            .push(Header::new("x-request-id", "abc-123"));
        response.body_size = Some(27);

        RequestView {
            request,
            response: Some(response),
            request_body: Some(r#"{"name":"Ada"}"#.to_string()),
            response_body: Some(r#"{"error":"database down"}"#.to_string()),
            replays: Vec::new(),
            details_loaded: true,
        }
    }

    fn console_log(level: ConsoleLevel, message: &str, source: Option<&str>) -> ConsoleLog {
        ConsoleLog::new(
            "session".to_string(),
            None,
            None,
            level,
            message.to_string(),
            source.map(str::to_string),
            None,
        )
    }

    #[test]
    fn request_filter_matches_extended_fields() {
        let mut request = request_view();
        request.request.completed_at = Some(request.request.started_at + 750);
        let Some(response) = request.response.as_mut() else {
            panic!("missing response");
        };
        response.body_size = Some(128 * 1024);

        assert!(RequestFilter::parse("method:post path:/api/users").matches(&request));
        assert!(RequestFilter::parse("mime:json header:abc-123").matches(&request));
        assert!(RequestFilter::parse("reqbody:ada resbody:database").matches(&request));
        assert!(RequestFilter::parse("body:database has:body has:error").matches(&request));
        assert!(RequestFilter::parse("status:5xx type:fetch domain:localhost").matches(&request));
        assert!(RequestFilter::parse("duration:>500 size:>100kb").matches(&request));
        assert!(RequestFilter::parse("path:/api/(users|teams)").matches(&request));
        assert!(RequestFilter::parse("method:^(post|put)$").matches(&request));
        assert!(RequestFilter::parse("/database|timeout/").matches(&request));
    }

    #[test]
    fn request_filter_rejects_missing_extended_fields() {
        let request = request_view();

        assert!(!RequestFilter::parse("path:/missing").matches(&request));
        assert!(!RequestFilter::parse("header:nope").matches(&request));
        assert!(!RequestFilter::parse("has:replay").matches(&request));
        assert!(!RequestFilter::parse("resbody:success").matches(&request));
        assert!(!RequestFilter::parse("duration:<1 size:>1mb").matches(&request));
    }

    #[test]
    fn cycles_filter_presets() -> TestResult {
        let store = Store::open_memory()?;
        let mut state = WorkbenchState::load(
            &store,
            std::path::Path::new("memory.db"),
            "http://localhost:5173",
            AppConfig::default(),
        )?;

        state.request_filter.clear();
        assert_eq!(state.request_filter, "");
        state.cycle_filter_preset();
        assert_eq!(state.request_filter, "has:error");
        assert_eq!(state.active_filter_preset_label(), Some("errors"));
        Ok(())
    }

    #[test]
    fn clear_request_filter_and_route_resets_breadcrumb() -> TestResult {
        let store = Store::open_memory()?;
        let mut state = WorkbenchState::load(
            &store,
            std::path::Path::new("memory.db"),
            "http://localhost:5173",
            AppConfig::default(),
        )?;

        state.request_filter = "status:5xx".to_string();
        state.sql_request_filter_ids = Some(std::collections::HashSet::new());
        state.sql_request_filter_query = Some("select id from requests".to_string());
        state.active_request_route_group = Some("localhost:5173/api/users".to_string());

        state.clear_request_filter_and_route();

        assert!(state.request_filter.is_empty());
        assert!(state.sql_request_filter_ids.is_none());
        assert!(state.sql_request_filter_query.is_none());
        assert!(state.active_request_route_group.is_none());
        Ok(())
    }

    #[test]
    fn console_filter_matches_level_source_kind_and_text() {
        let eval_log = console_log(
            ConsoleLevel::Info,
            "> document.title\n\"Faro\"",
            Some("faro-console"),
        );
        let error_log = console_log(
            ConsoleLevel::Error,
            "Unhandled token failure",
            Some("runtime"),
        );

        assert!(ConsoleFilter::parse("eval faro").matches(&eval_log));
        assert!(ConsoleFilter::parse("/faro|runtime/").matches(&eval_log));
        assert!(ConsoleFilter::parse("kind:eval source:faro").matches(&eval_log));
        assert!(ConsoleFilter::parse("level:error token").matches(&error_log));
        assert!(!ConsoleFilter::parse("level:warn").matches(&error_log));
        assert!(!ConsoleFilter::parse("kind:page").matches(&eval_log));
    }

    #[test]
    fn strips_expanded_route_segments_from_request_paths() {
        assert_eq!(
            strip_route_segments("/api/users/123/details?tab=profile", 3),
            "/details?tab=profile"
        );
        assert_eq!(strip_route_segments("/api/users/123", 3), "/");
        assert_eq!(strip_route_segments("/api/users", 1), "/users");
    }

    #[test]
    fn labels_expanded_route_with_domain_and_normalized_path() {
        assert_eq!(
            route_label_for_group("localhost:5173/api/users/:id"),
            "localhost:5173/api/users/:id"
        );
        assert_eq!(group_path_segment_count("localhost:5173/api/users/:id"), 3);
        assert_eq!(
            parent_group_key("localhost:5173/api/users/:id"),
            Some("localhost:5173/api/users".to_string())
        );
        assert_eq!(parent_group_key("localhost:5173/api"), None);
    }

    #[test]
    fn request_tree_meta_counts_descendant_children() -> TestResult {
        let mut parent = request_view();
        parent.request.url = "http://localhost:5173/api/users".to_string();
        let mut child = request_view();
        child.request.url = "http://localhost:5173/api/users/123".to_string();
        let requests = vec![parent, child];
        let metas = build_request_tree_metas(&requests);

        let Some(parent_meta) = metas.first() else {
            panic!("missing parent meta");
        };
        let Some(child_meta) = metas.get(1) else {
            panic!("missing child meta");
        };

        assert!(parent_meta.has_children);
        assert_eq!(parent_meta.child_count, 1);
        assert!(!child_meta.has_children);
        assert_eq!(child_meta.child_count, 0);

        let store = Store::open_memory()?;
        let mut state = WorkbenchState::load(
            &store,
            std::path::Path::new("memory.db"),
            "http://localhost:5173",
            AppConfig::default(),
        )?;
        state.requests = requests;
        state.request_tree_metas = metas;

        assert_eq!(state.request_open_route_child_count(0), Some((false, 1)));
        assert_eq!(state.request_open_route_child_count(1), Some((false, 1)));
        Ok(())
    }

    #[test]
    fn request_tree_meta_marks_visible_leaf_parent_with_query_as_having_children() -> TestResult {
        let mut parent = request_view();
        parent.request.url = "http://localhost:5173/api/users?limit=10".to_string();
        let mut child = request_view();
        child.request.url = "http://localhost:5173/api/users/123".to_string();
        let requests = vec![parent, child];
        let metas = build_request_tree_metas(&requests);
        let store = Store::open_memory()?;
        let mut state = WorkbenchState::load(
            &store,
            std::path::Path::new("memory.db"),
            "http://localhost:5173",
            AppConfig::default(),
        )?;
        state.requests = requests;
        state.request_tree_metas = metas;
        state.filtered_request_indices = vec![0, 1];

        let Some(parent_meta) = state.request_tree_meta(0) else {
            panic!("missing parent meta");
        };
        assert!(parent_meta.has_children);
        assert_eq!(parent_meta.child_count, 1);
        Ok(())
    }

    #[test]
    fn request_open_route_counts_shared_prefix_without_parent_row() -> TestResult {
        let mut first = request_view();
        first.request.url = "http://localhost:5173/api/users/123".to_string();
        let mut second = request_view();
        second.request.url = "http://localhost:5173/api/users/456".to_string();
        let requests = vec![first, second];
        let metas = build_request_tree_metas(&requests);
        let store = Store::open_memory()?;
        let mut state = WorkbenchState::load(
            &store,
            std::path::Path::new("memory.db"),
            "http://localhost:5173",
            AppConfig::default(),
        )?;
        state.requests = requests;
        state.request_tree_metas = metas;

        assert_eq!(state.request_open_route_child_count(0), Some((false, 2)));
        assert_eq!(state.request_open_route_child_count(1), Some((false, 2)));
        Ok(())
    }

    #[test]
    fn request_open_route_marks_filtered_siblings_as_drillable() -> TestResult {
        let mut first = request_view();
        first.request.url = "http://localhost:5173/gamma/api/v1/organizations/123".to_string();
        let mut second = request_view();
        second.request.url = "http://localhost:5173/gamma/api/v1/organizations/456".to_string();
        let requests = vec![first, second];
        let metas = build_request_tree_metas(&requests);
        let store = Store::open_memory()?;
        let mut state = WorkbenchState::load(
            &store,
            std::path::Path::new("memory.db"),
            "http://localhost:5173",
            AppConfig::default(),
        )?;
        state.requests = requests;
        state.request_tree_metas = metas;
        state.filtered_request_indices = vec![0, 1];

        assert!(state.request_open_route_child_count(0).is_some());
        assert!(state.request_open_route_child_count(1).is_some());
        Ok(())
    }
}
