use super::*;

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
        let session_records = store.sessions()?;
        let sessions = build_session_views(store, &session_records)?;
        let session = select_session(session_records, target_url, active_session_id);
        let selected_session_id = session.as_ref().map(|session| session.id.clone());
        if let Some(session) = &session {
            let mut responses_by_request = latest_responses_by_request(store, &session.id)?;
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
        let mut session_state = ListState::default();
        if !sessions.is_empty() {
            let selected = selected_session_id
                .as_deref()
                .and_then(|id| {
                    sessions
                        .iter()
                        .position(|entry| entry.session.id.as_str() == id)
                })
                .unwrap_or_else(|| sessions.len() - 1);
            session_state.select(Some(selected));
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
            sessions,
            session_state,
            requests,
            request_tree_metas,
            filtered_request_indices,
            filtered_request_rows: Vec::new(),
            filtered_route_descendant_counts: HashMap::new(),
            collapsed_request_groups: HashSet::new(),
            active_request_route_group: None,
            sql_request_filter_ids: None,
            sql_request_filter_query: None,
            requests_hidden_before: None,
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
            selected_replay_index: usize::MAX,
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
            body_search_query: String::new(),
            show_help: false,
            show_sessions: false,
            show_theme_preview: false,
            show_perf: false,
            perf: PerfStats::default(),
            sql_result: None,
            sql_row_scroll: 0,
            sql_col_scroll: 0,
            last_sql_query: String::new(),
            request_filter: initial_request_filter,
            console_filter: String::new(),
            websocket_filter: String::new(),
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
        self.sessions = loaded.sessions;
        self.session_state = loaded.session_state;
        self.requests = loaded.requests;
        self.request_tree_metas = loaded.request_tree_metas;
        self.filtered_route_descendant_counts = HashMap::new();
        self.filtered_request_rows = Vec::new();
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
                (!self.filtered_request_rows.is_empty())
                    .then(|| self.filtered_request_rows.len() - 1)
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

    pub(crate) fn refresh_live_data(&mut self) -> anyhow::Result<()> {
        let Some(session_id) = self.active_session_id.clone() else {
            return self.reload();
        };
        let selected_id = self
            .selected_request()
            .map(|request| request.request.id.clone());
        let max_started_at = self
            .requests
            .iter()
            .map(|request| request.request.started_at)
            .max()
            .unwrap_or(0);
        let max_completed_at = self
            .requests
            .iter()
            .filter_map(|request| request.request.completed_at)
            .max()
            .unwrap_or(0);
        let max_response_at = self
            .requests
            .iter()
            .filter_map(|request| {
                request
                    .response
                    .as_ref()
                    .map(|response| response.received_at)
            })
            .max()
            .unwrap_or(0);
        let max_console_ts = self
            .console_logs
            .iter()
            .map(|log| log.ts)
            .max()
            .unwrap_or(0);
        let max_websocket_ts = self
            .websocket_frames
            .iter()
            .map(|frame| frame.ts)
            .max()
            .unwrap_or(0);

        let store = Store::open(&self.db_path)
            .with_context(|| format!("open database {}", self.db_path.display()))?;
        let changed_requests = store
            .requests_for_session_changed_after(&session_id, max_started_at, max_completed_at)
            .with_context(|| format!("load changed requests for session {session_id}"))?;
        let changed_responses = store
            .responses_for_session_after(&session_id, max_response_at)
            .with_context(|| format!("load changed responses for session {session_id}"))?;
        let new_console_logs = store
            .console_logs_for_session_after(&session_id, max_console_ts)
            .with_context(|| format!("load console logs for session {session_id}"))?;
        let new_websocket_frames = store
            .websocket_frames_for_session_after(&session_id, max_websocket_ts)
            .with_context(|| format!("load websocket frames for session {session_id}"))?;

        let requests_changed = !changed_requests.is_empty();
        let responses_changed = !changed_responses.is_empty();
        let console_changed = !new_console_logs.is_empty();
        let websockets_changed = !new_websocket_frames.is_empty();
        if !(requests_changed || responses_changed || console_changed || websockets_changed) {
            return Ok(());
        }

        if requests_changed {
            self.merge_request_rows(changed_requests);
            self.request_tree_metas = build_request_tree_metas(&self.requests);
        }
        if responses_changed {
            self.merge_response_rows(changed_responses);
        }
        if console_changed {
            self.console_logs.extend(new_console_logs);
            if let Some(hidden_before) = self.console_hidden_before {
                self.console_logs.retain(|log| log.ts > hidden_before);
            }
            self.apply_console_filter();
        }
        if websockets_changed {
            self.websocket_frames.extend(new_websocket_frames);
            self.apply_websocket_filter();
        }
        if requests_changed || responses_changed {
            self.apply_filter();
            if let Some(selected) =
                selected_id.and_then(|id| self.filtered_index_for_request_id(&id))
            {
                self.table_state.select(Some(selected));
            }
        }
        Ok(())
    }

    fn merge_request_rows(&mut self, requests: Vec<RequestRecord>) {
        let mut indices_by_id = self
            .requests
            .iter()
            .enumerate()
            .map(|(index, entry)| (entry.request.id.clone(), index))
            .collect::<HashMap<_, _>>();
        for request in requests {
            if let Some(index) = indices_by_id.get(&request.id).copied() {
                if let Some(existing) = self.requests.get_mut(index) {
                    existing.request = request;
                }
            } else {
                let id = request.id.clone();
                self.requests.push(RequestView {
                    request,
                    response: None,
                    request_body: None,
                    response_body: None,
                    replays: Vec::new(),
                    details_loaded: false,
                });
                indices_by_id.insert(id, self.requests.len().saturating_sub(1));
            }
        }
    }

    fn merge_response_rows(&mut self, responses: Vec<ResponseRecord>) {
        let indices_by_id = self
            .requests
            .iter()
            .enumerate()
            .map(|(index, entry)| (entry.request.id.clone(), index))
            .collect::<HashMap<_, _>>();
        for response in responses {
            if let Some(index) = indices_by_id.get(&response.request_id).copied()
                && let Some(request) = self.requests.get_mut(index)
            {
                request.response = Some(response);
            }
        }
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

fn build_session_views(store: &Store, sessions: &[Session]) -> anyhow::Result<Vec<SessionView>> {
    sessions
        .iter()
        .map(|session| {
            let summary = session_summary(store, session.clone())?;
            Ok(SessionView {
                session: summary.session,
                request_count: summary.request_count,
                console_error_count: summary.console_error_count,
                replay_count: summary.replay_count,
                websocket_count: summary.websocket_count,
                storage_count: summary.storage_count,
                cookie_count: summary.cookie_count,
            })
        })
        .collect()
}
