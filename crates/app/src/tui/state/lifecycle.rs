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
        let selected_session_id = if let Some(session) =
            select_session(session_records, target_url, active_session_id)
        {
            store
                .prune_repeated_session_requests(
                    &session.id,
                    config.retention.max_repeated_requests_per_url,
                )
                .with_context(|| format!("prune repeated requests for session {}", session.id))?;
            store
                .prune_session_requests(&session.id, config.retention.max_requests_per_session)
                .with_context(|| format!("prune requests for session {}", session.id))?;
            store
                .prune_session_console_logs(
                    &session.id,
                    config.retention.max_console_logs_per_session,
                )
                .with_context(|| format!("prune console logs for session {}", session.id))?;
            store
                .prune_session_websocket_frames(
                    &session.id,
                    config.retention.max_websocket_frames_per_session,
                )
                .with_context(|| format!("prune websocket frames for session {}", session.id))?;
            Some(session.id)
        } else {
            None
        };
        let session_records = store.sessions()?;
        let sessions = build_session_views(store, &session_records)?;
        let session = selected_session_id
            .as_ref()
            .and_then(|selected_session_id| {
                session_records
                    .iter()
                    .find(|session| session.id == *selected_session_id)
            });
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
        let request_route_descendant_counts = descendant_counts_for_metas(&request_tree_metas);
        let request_indices_by_id = request_indices_by_id(&requests);
        let request_stats = compute_request_stats(&requests);
        let console_stats = compute_console_stats(&console_logs);
        let websocket_stats = compute_websocket_stats(&websocket_frames);
        let websocket_connection_ids = websocket_connection_ids_for_frames(&websocket_frames);
        let live_watermarks =
            live_watermarks_for_state(&requests, &console_logs, &websocket_frames);
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
            request_indices_by_id,
            request_tree_metas,
            request_route_descendant_counts,
            filtered_request_indices,
            filtered_request_rows: Vec::new(),
            filtered_request_positions_by_id: HashMap::new(),
            filtered_route_descendant_counts: HashMap::new(),
            request_stats,
            live_watermarks,
            live_requests_since_prune: 0,
            active_route_summary_cache: None,
            collapsed_request_groups: HashSet::new(),
            active_request_route_group: None,
            sql_request_filter_ids: None,
            sql_request_filter_query: None,
            requests_hidden_before: None,
            console_logs,
            filtered_console_indices,
            filtered_console_positions_by_id: HashMap::new(),
            console_hidden_before: None,
            console_stats,
            console_detail_line_cache: RefCell::new(None),
            websocket_frames,
            filtered_websocket_indices,
            filtered_websocket_positions_by_id: HashMap::new(),
            websocket_state,
            websocket_detail_scroll: 0,
            websocket_detail_line_cache: RefCell::new(None),
            websocket_stats,
            websocket_connection_ids,
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
            body_tree_cache: RefCell::new(None),
            response_body_line_cache: RefCell::new(None),
            captured_favicon_cache: RefCell::new(None),
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
        self.request_indices_by_id = loaded.request_indices_by_id;
        self.request_tree_metas = loaded.request_tree_metas;
        self.request_route_descendant_counts = loaded.request_route_descendant_counts;
        self.request_stats = loaded.request_stats;
        self.live_watermarks = loaded.live_watermarks;
        self.live_requests_since_prune = 0;
        self.active_route_summary_cache = loaded.active_route_summary_cache;
        self.body_tree_cache.replace(None);
        self.response_body_line_cache.replace(None);
        self.captured_favicon_cache.replace(None);
        self.console_detail_line_cache.replace(None);
        self.websocket_detail_line_cache.replace(None);
        self.filtered_route_descendant_counts = HashMap::new();
        self.filtered_request_rows = Vec::new();
        self.filtered_request_positions_by_id = HashMap::new();
        self.collapsed_request_groups = collapsed_request_groups;
        self.active_request_route_group = active_request_route_group;
        self.console_logs = loaded.console_logs;
        self.console_stats = loaded.console_stats;
        self.filtered_console_positions_by_id = HashMap::new();
        self.websocket_frames = loaded.websocket_frames;
        self.websocket_stats = loaded.websocket_stats;
        self.websocket_connection_ids = loaded.websocket_connection_ids;
        self.filtered_websocket_positions_by_id = HashMap::new();
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

    pub(crate) fn live_refresh_request(&self) -> Option<LiveRefreshRequest> {
        let session_id = self.active_session_id.clone()?;
        let prune_interval = self.config.retention.prune_interval_requests.max(1);

        Some(LiveRefreshRequest {
            db_path: self.db_path.clone(),
            session_id,
            retention: self.config.retention.clone(),
            max_started_at: self.live_watermarks.max_started_at,
            max_completed_at: self.live_watermarks.max_completed_at,
            max_response_at: self.live_watermarks.max_response_at,
            max_console_ts: self.live_watermarks.max_console_ts,
            max_websocket_ts: self.live_watermarks.max_websocket_ts,
            prune_retention: self.live_requests_since_prune >= prune_interval,
        })
    }

    pub(crate) fn apply_live_refresh_delta(&mut self, delta: LiveRefreshDelta) -> bool {
        if delta.is_empty() {
            return false;
        }
        let merge_started = Instant::now();
        let changed_request_count = delta.changed_requests.len();
        let retention_prune_ran = delta.retention_prune_ran;
        let selected_id = self
            .selected_request()
            .map(|request| request.request.id.clone());
        let mut live_watermarks = self.live_watermarks;
        live_watermarks.apply_delta(&delta);
        let requests_pruned = if let Some(retained_request_ids) = delta.retained_request_ids {
            let retained_ids = retained_request_ids.into_iter().collect::<HashSet<_>>();
            self.requests
                .retain(|request| retained_ids.contains(&request.request.id));
            let _ =
                self.prune_request_views_to_limit(self.config.retention.max_requests_per_session);
            self.rebuild_request_indices_by_id();
            self.body_tree_cache.replace(None);
            self.response_body_line_cache.replace(None);
            self.captured_favicon_cache.replace(None);
            true
        } else {
            false
        };
        let console_pruned = if let Some(retained_console_log_ids) = delta.retained_console_log_ids
        {
            let retained_ids = retained_console_log_ids.into_iter().collect::<HashSet<_>>();
            self.console_logs
                .retain(|log| retained_ids.contains(&log.id));
            true
        } else {
            false
        };
        let websockets_pruned =
            if let Some(retained_websocket_frame_ids) = delta.retained_websocket_frame_ids {
                let retained_ids = retained_websocket_frame_ids
                    .into_iter()
                    .collect::<HashSet<_>>();
                self.websocket_frames
                    .retain(|frame| retained_ids.contains(&frame.id));
                true
            } else {
                false
            };

        let requests_changed = requests_pruned || !delta.changed_requests.is_empty();
        let responses_changed = !delta.changed_responses.is_empty();
        let console_changed = console_pruned || !delta.new_console_logs.is_empty();
        let websockets_changed = websockets_pruned || !delta.new_websocket_frames.is_empty();

        let mut request_stats_requires_rebuild = requests_pruned;
        let mut appended_request_indices = Vec::new();
        if !delta.changed_requests.is_empty() {
            let merge_result = self.merge_request_rows(delta.changed_requests);
            request_stats_requires_rebuild |= merge_result.requires_stats_rebuild;
            appended_request_indices = merge_result.appended_indices.clone();
            let pruned_by_limit =
                self.prune_request_views_to_limit(self.config.retention.max_requests_per_session);
            if pruned_by_limit {
                self.body_tree_cache.replace(None);
                self.response_body_line_cache.replace(None);
                self.captured_favicon_cache.replace(None);
                request_stats_requires_rebuild = true;
            }
            if requests_pruned || merge_result.requires_tree_rebuild || pruned_by_limit {
                self.rebuild_request_tree_metas_timed();
            } else if !merge_result.appended_indices.is_empty() {
                let started = Instant::now();
                append_request_tree_metas(
                    &mut self.request_tree_metas,
                    &self.requests,
                    &merge_result.appended_indices,
                    &self.request_route_descendant_counts,
                );
                for index in &merge_result.appended_indices {
                    if let Some(meta) = self.request_tree_metas.get(*index) {
                        for group in &meta.ancestor_keys {
                            *self
                                .request_route_descendant_counts
                                .entry(group.clone())
                                .or_insert(0) += 1;
                        }
                    }
                }
                self.perf.last_tree_build_ms = started.elapsed().as_millis();
                self.perf.max_tree_build_ms = self
                    .perf
                    .max_tree_build_ms
                    .max(self.perf.last_tree_build_ms);
            }
        } else if requests_pruned {
            self.rebuild_request_tree_metas_timed();
        }
        if responses_changed {
            request_stats_requires_rebuild |= self.merge_response_rows(delta.changed_responses);
        }
        if requests_changed || responses_changed {
            if request_stats_requires_rebuild {
                self.request_stats = compute_request_stats(&self.requests);
            }
            self.active_route_summary_cache = self.compute_active_route_summary();
        }
        if console_changed {
            if console_pruned || self.console_hidden_before.is_some() {
                self.console_logs.extend(delta.new_console_logs);
                if let Some(hidden_before) = self.console_hidden_before {
                    self.console_logs.retain(|log| log.ts > hidden_before);
                }
                self.console_stats = compute_console_stats(&self.console_logs);
            } else {
                accumulate_console_stats(&mut self.console_stats, &delta.new_console_logs);
                self.console_logs.extend(delta.new_console_logs);
            }
            self.apply_console_filter();
        }
        if websockets_changed {
            if websockets_pruned {
                self.websocket_frames.extend(delta.new_websocket_frames);
                self.websocket_stats = compute_websocket_stats(&self.websocket_frames);
                self.websocket_connection_ids =
                    websocket_connection_ids_for_frames(&self.websocket_frames);
            } else {
                accumulate_websocket_stats(&mut self.websocket_stats, &delta.new_websocket_frames);
                for frame in &delta.new_websocket_frames {
                    self.websocket_connection_ids
                        .insert(frame.browser_request_id.clone());
                }
                self.websocket_stats.connections = self.websocket_connection_ids.len();
                self.websocket_frames.extend(delta.new_websocket_frames);
            }
            self.apply_websocket_filter();
        }
        self.live_watermarks = if requests_pruned || console_pruned || websockets_pruned {
            live_watermarks_for_state(&self.requests, &self.console_logs, &self.websocket_frames)
        } else {
            live_watermarks
        };
        self.perf.last_live_merge_ms = merge_started.elapsed().as_millis();
        self.perf.max_live_merge_ms = self
            .perf
            .max_live_merge_ms
            .max(self.perf.last_live_merge_ms);
        let response_updates_affect_filter =
            responses_changed && self.response_updates_affect_request_query();
        if requests_changed
            && !requests_pruned
            && !response_updates_affect_filter
            && self.can_apply_unfiltered_request_fast_path()
        {
            self.sync_unfiltered_request_filter_state(&appended_request_indices);
        } else if requests_changed || response_updates_affect_filter {
            self.apply_filter();
            if let Some(selected) =
                selected_id.and_then(|id| self.filtered_index_for_request_id(&id))
            {
                self.table_state.select(Some(selected));
            }
        }
        self.perf.last_db_refresh_ms = delta.duration_ms;
        self.perf.max_db_refresh_ms = self.perf.max_db_refresh_ms.max(delta.duration_ms);
        if retention_prune_ran {
            self.live_requests_since_prune = 0;
        } else {
            self.live_requests_since_prune = self
                .live_requests_since_prune
                .saturating_add(changed_request_count);
        }
        true
    }

    fn merge_request_rows(&mut self, requests: Vec<RequestRecord>) -> RequestMergeResult {
        let mut result = RequestMergeResult::default();
        for request in requests {
            if let Some(index) = self.request_indices_by_id.get(&request.id).copied() {
                if let Some(existing) = self.requests.get_mut(index) {
                    let before = request_stats_contribution(existing);
                    result.requires_tree_rebuild |= existing.request.url != request.url;
                    existing.request = request;
                    let after = request_stats_contribution(existing);
                    result.requires_stats_rebuild |= apply_request_stats_replacement(
                        &mut self.request_stats,
                        Some(before),
                        Some(after),
                    );
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
                let index = self.requests.len().saturating_sub(1);
                if let Some(request) = self.requests.get(index) {
                    let after = request_stats_contribution(request);
                    result.requires_stats_rebuild |=
                        apply_request_stats_replacement(&mut self.request_stats, None, Some(after));
                }
                self.request_indices_by_id.insert(id, index);
                result.appended_indices.push(index);
            }
        }
        result
    }

    fn merge_response_rows(&mut self, responses: Vec<ResponseRecord>) -> bool {
        let mut requires_stats_rebuild = false;
        for response in responses {
            if let Some(index) = self
                .request_indices_by_id
                .get(&response.request_id)
                .copied()
                && let Some(request) = self.requests.get_mut(index)
            {
                let before = request_stats_contribution(request);
                request.response = Some(response);
                let after = request_stats_contribution(request);
                requires_stats_rebuild |= apply_request_stats_replacement(
                    &mut self.request_stats,
                    Some(before),
                    Some(after),
                );
            }
        }
        requires_stats_rebuild
    }

    fn prune_request_views_to_limit(&mut self, max_requests: usize) -> bool {
        let excess = self.requests.len().saturating_sub(max_requests.max(1));
        if excess > 0 {
            self.requests.drain(0..excess);
            self.rebuild_request_indices_by_id();
            return true;
        }
        false
    }

    fn rebuild_request_indices_by_id(&mut self) {
        self.request_indices_by_id = request_indices_by_id(&self.requests);
    }

    fn rebuild_request_tree_metas_timed(&mut self) {
        let started = Instant::now();
        self.request_tree_metas = build_request_tree_metas(&self.requests);
        self.request_route_descendant_counts =
            descendant_counts_for_metas(&self.request_tree_metas);
        self.perf.last_tree_build_ms = started.elapsed().as_millis();
        self.perf.max_tree_build_ms = self
            .perf
            .max_tree_build_ms
            .max(self.perf.last_tree_build_ms);
    }
}

#[derive(Default)]
struct RequestMergeResult {
    appended_indices: Vec<usize>,
    requires_tree_rebuild: bool,
    requires_stats_rebuild: bool,
}

fn request_indices_by_id(requests: &[RequestView]) -> HashMap<String, usize> {
    requests
        .iter()
        .enumerate()
        .map(|(index, entry)| (entry.request.id.clone(), index))
        .collect()
}

fn live_watermarks_for_state(
    requests: &[RequestView],
    console_logs: &[ConsoleLog],
    websocket_frames: &[WebSocketFrameRecord],
) -> LiveWatermarks {
    let mut watermarks = LiveWatermarks::default();
    for request in requests {
        watermarks.max_started_at = watermarks.max_started_at.max(request.request.started_at);
        if let Some(completed_at) = request.request.completed_at {
            watermarks.max_completed_at = watermarks.max_completed_at.max(completed_at);
        }
        if let Some(response) = &request.response {
            watermarks.max_response_at = watermarks.max_response_at.max(response.received_at);
        }
    }
    for log in console_logs {
        watermarks.max_console_ts = watermarks.max_console_ts.max(log.ts);
    }
    for frame in websocket_frames {
        watermarks.max_websocket_ts = watermarks.max_websocket_ts.max(frame.ts);
    }
    watermarks
}

fn compute_console_stats(logs: &[ConsoleLog]) -> ConsoleStats {
    let mut stats = ConsoleStats::default();
    accumulate_console_stats(&mut stats, logs);
    stats
}

fn accumulate_console_stats(stats: &mut ConsoleStats, logs: &[ConsoleLog]) {
    for log in logs {
        match log.level {
            ConsoleLevel::Error | ConsoleLevel::Fatal => stats.errors += 1,
            ConsoleLevel::Warning => stats.warnings += 1,
            _ => {}
        }
        if log.source.as_deref() == Some("faro-console") {
            stats.evals += 1;
        }
    }
}

fn compute_websocket_stats(frames: &[WebSocketFrameRecord]) -> WebSocketStats {
    let mut stats = WebSocketStats::default();
    accumulate_websocket_stats(&mut stats, frames);
    stats.connections = websocket_connection_ids_for_frames(frames).len();
    stats
}

fn accumulate_websocket_stats(stats: &mut WebSocketStats, frames: &[WebSocketFrameRecord]) {
    for frame in frames {
        match frame.direction {
            WebSocketFrameDirection::Sent => stats.sent += 1,
            WebSocketFrameDirection::Received => stats.received += 1,
        }
        stats.bytes = stats.bytes.saturating_add(frame.payload.len());
    }
}

fn websocket_connection_ids_for_frames(frames: &[WebSocketFrameRecord]) -> HashSet<String> {
    frames
        .iter()
        .map(|frame| frame.browser_request_id.clone())
        .collect()
}

impl LiveWatermarks {
    fn apply_delta(&mut self, delta: &LiveRefreshDelta) {
        for request in &delta.changed_requests {
            self.max_started_at = self.max_started_at.max(request.started_at);
            if let Some(completed_at) = request.completed_at {
                self.max_completed_at = self.max_completed_at.max(completed_at);
            }
        }
        for response in &delta.changed_responses {
            self.max_response_at = self.max_response_at.max(response.received_at);
        }
        for log in &delta.new_console_logs {
            self.max_console_ts = self.max_console_ts.max(log.ts);
        }
        for frame in &delta.new_websocket_frames {
            self.max_websocket_ts = self.max_websocket_ts.max(frame.ts);
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
