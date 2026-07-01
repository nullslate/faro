use super::*;

impl WorkbenchState {
    pub(crate) fn selected_request(&self) -> Option<&RequestView> {
        self.selected_request_index()
            .and_then(|index| self.requests.get(index))
    }

    pub(crate) fn select_request_position(&mut self, position: usize) {
        if position < self.filtered_request_rows.len() {
            self.table_state.select(Some(position));
            self.reset_request_view_scroll();
            self.sync_selected_replay_index();
            self.hydrate_selected_request_for_active_detail();
        }
    }

    pub(crate) fn enter_selected_request_group(&mut self) {
        let selected_group = self
            .selected_request_index()
            .and_then(|index| self.drilldown_group_key_for_request_index(index));
        let Some(group) = selected_group else {
            self.status = "no collapsible request branch".to_string();
            return;
        };
        if self.active_request_route_group.as_deref() == Some(group.as_str()) {
            self.status = "no collapsible request branch".to_string();
            return;
        }
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
        let selected_group = self
            .selected_request_index()
            .and_then(|index| self.collapsible_group_key_for_request_index(index));
        let Some(group) = selected_group else {
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

    pub(super) fn next_request(&mut self) {
        if self.filtered_request_rows.is_empty() {
            return;
        }
        let next = match self.table_state.selected() {
            Some(index) if index + 1 < self.filtered_request_rows.len() => index + 1,
            _ => 0,
        };
        self.table_state.select(Some(next));
        self.reset_request_view_scroll();
        self.sync_selected_replay_index();
        self.hydrate_selected_request_for_active_detail();
    }

    pub(super) fn previous_request(&mut self) {
        if self.filtered_request_rows.is_empty() {
            return;
        }
        let previous = match self.table_state.selected() {
            Some(0) | None => self.filtered_request_rows.len() - 1,
            Some(index) => index - 1,
        };
        self.table_state.select(Some(previous));
        self.reset_request_view_scroll();
        self.sync_selected_replay_index();
        self.hydrate_selected_request_for_active_detail();
    }

    pub(super) fn apply_filter(&mut self) {
        let selected_id = self
            .selected_request()
            .map(|request| request.request.id.clone());
        let result = if self.can_apply_unfiltered_request_fast_path() {
            self.unfiltered_request_query_result()
        } else {
            let query_items = self.request_query_items();
            query_requests(&query_items, &self.request_query_options())
        };
        self.filtered_request_indices = result.indices;
        self.filtered_request_rows = result.rows;
        self.filtered_route_descendant_counts = result.route_descendant_counts;

        let selected = selected_id
            .and_then(|id| self.filtered_index_for_request_id(&id))
            .or_else(|| (!self.filtered_request_rows.is_empty()).then_some(0));
        self.table_state.select(selected);
        self.reset_request_view_scroll();
        self.hydrate_selected_request_for_active_detail();
    }

    fn can_apply_unfiltered_request_fast_path(&self) -> bool {
        self.request_filter.is_empty()
            && self.sql_request_filter_ids.is_none()
            && self.requests_hidden_before.is_none()
            && self.active_request_route_group.is_none()
            && matches!(self.sort_mode, SortMode::Started)
            && !self.sort_descending
    }

    fn unfiltered_request_query_result(&self) -> RequestQueryResult {
        let indices = (0..self.requests.len()).collect::<Vec<_>>();
        let mut route_descendant_counts = HashMap::new();
        for meta in &self.request_tree_metas {
            for group in &meta.ancestor_keys {
                *route_descendant_counts.entry(group.clone()).or_insert(0) += 1;
            }
        }
        RequestQueryResult {
            rows: indices.clone(),
            indices,
            route_descendant_counts,
        }
    }

    fn request_query_items(&self) -> Vec<RequestQueryItem<'_>> {
        self.requests
            .iter()
            .enumerate()
            .map(|(index, request)| {
                let response_headers = request
                    .response
                    .as_ref()
                    .map(|response| response.response_headers.as_slice())
                    .unwrap_or(&[]);
                RequestQueryItem {
                    index,
                    id: &request.request.id,
                    method: &request.request.method,
                    url: &request.request.url,
                    resource_type: request.request.resource_type.as_deref(),
                    status_code: request.status_code(),
                    started_at: request.request.started_at,
                    completed_at: request.request.completed_at,
                    mime_type: request
                        .response
                        .as_ref()
                        .and_then(|response| response.mime_type.as_deref()),
                    body_size: request
                        .response
                        .as_ref()
                        .and_then(|response| response.body_size),
                    request_headers: &request.request.request_headers,
                    response_headers,
                    request_body: request.request_body.as_deref(),
                    response_body: request.response_body.as_deref(),
                    replay_count: request.replays.len(),
                    meta: self
                        .request_tree_metas
                        .get(index)
                        .map(|meta| RequestQueryMeta {
                            domain: &meta.domain,
                            path: &meta.path,
                            ancestor_keys: &meta.ancestor_keys,
                        }),
                }
            })
            .collect()
    }

    fn request_query_options(&self) -> RequestQueryOptions<'_> {
        RequestQueryOptions {
            filter: &self.request_filter,
            sql_request_filter_ids: self.sql_request_filter_ids.as_ref(),
            hidden_before: self.requests_hidden_before,
            active_route_group: self.active_request_route_group.as_deref(),
            sort: self.sort_mode.into(),
            sort_descending: self.sort_descending,
        }
    }

    #[cfg(test)]
    pub(super) fn rebuild_filtered_route_descendant_counts(&mut self) {
        let result = {
            let query_items = self.request_query_items();
            query_requests(&query_items, &self.request_query_options())
        };
        self.filtered_route_descendant_counts = result.route_descendant_counts;
    }

    #[cfg(test)]
    pub(super) fn rebuild_filtered_request_rows(&mut self) {
        self.filtered_request_rows = self.filtered_request_indices.clone();
    }

    pub(super) fn filtered_index_for_request_id(&self, request_id: &str) -> Option<usize> {
        self.filtered_request_rows
            .iter()
            .position(|index| self.requests[*index].request.id == request_id)
    }

    pub(super) fn selected_request_index(&self) -> Option<usize> {
        self.table_state
            .selected()
            .and_then(|index| self.filtered_request_rows.get(index))
            .copied()
    }

    pub(super) fn collapsible_group_key_for_request_index(
        &self,
        request_index: usize,
    ) -> Option<String> {
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

    fn drilldown_group_key_for_request_index(&self, request_index: usize) -> Option<String> {
        let active_group = self.active_request_route_group.as_deref();
        let group = self.collapsible_group_key_for_request_index(request_index)?;
        let can_drill = active_group
            .map(|active| group != active && group.starts_with(&format!("{active}/")))
            .unwrap_or(true);
        can_drill.then_some(group)
    }

    pub(crate) fn request_can_drill_down(&self, request_index: usize) -> bool {
        self.drilldown_group_key_for_request_index(request_index)
            .is_some()
    }

    pub(super) fn route_group_child_count(&self, group: &str) -> usize {
        self.filtered_route_descendant_counts
            .get(group)
            .copied()
            .unwrap_or(0)
    }

    pub(crate) fn request_tree_meta(&self, request_index: usize) -> Option<RequestTreeMeta> {
        let mut meta = self.request_tree_metas.get(request_index)?.clone();
        if let Some(group) = &meta.group_key {
            let visible_child_count = self.route_group_child_count(group);
            if visible_child_count > 0 {
                meta.has_children = true;
                meta.child_count = visible_child_count;
            } else {
                meta.has_children = false;
                meta.child_count = 0;
            }
        }
        meta.collapsed = meta
            .group_key
            .as_deref()
            .map(|key| self.collapsed_request_groups.contains(key))
            .unwrap_or(false);
        Some(meta)
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

    pub(super) fn reset_request_view_scroll(&mut self) {
        self.detail_scroll = 0;
        self.selected_replay_index = usize::MAX;
        self.body_scroll = 0;
        self.body_tree_selected = 0;
        self.body_tree_selected_key = None;
        self.collapsed_body_nodes.clear();
    }
}
