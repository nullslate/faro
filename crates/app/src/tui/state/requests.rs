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
        let started = Instant::now();
        let selected_id = self
            .selected_request()
            .map(|request| request.request.id.clone());
        let result = if self.can_apply_unfiltered_request_fast_path() {
            self.unfiltered_request_query_result()
        } else {
            query_requests_iter(
                self.request_query_items_iter(),
                &self.request_query_options(),
            )
        };
        self.filtered_request_indices = result.indices;
        self.filtered_request_rows = result.rows;
        self.rebuild_filtered_request_positions_by_id();
        self.filtered_route_descendant_counts = result.route_descendant_counts;
        self.active_route_summary_cache = self.compute_active_route_summary();

        let selected = selected_id
            .and_then(|id| self.filtered_index_for_request_id(&id))
            .or_else(|| (!self.filtered_request_rows.is_empty()).then_some(0));
        self.table_state.select(selected);
        self.reset_request_view_scroll();
        self.hydrate_selected_request_for_active_detail();
        self.perf.last_filter_ms = started.elapsed().as_millis();
        self.perf.max_filter_ms = self.perf.max_filter_ms.max(self.perf.last_filter_ms);
    }

    pub(super) fn sync_unfiltered_request_filter_state(&mut self, appended_indices: &[usize]) {
        let started = Instant::now();
        let should_select_first = self.table_state.selected().is_none()
            && self.filtered_request_rows.is_empty()
            && !appended_indices.is_empty();

        for index in appended_indices {
            let row_index = self.filtered_request_rows.len();
            self.filtered_request_indices.push(*index);
            self.filtered_request_rows.push(*index);
            if let Some(request) = self.requests.get(*index) {
                self.filtered_request_positions_by_id
                    .insert(request.request.id.clone(), row_index);
            }
        }

        if self.filtered_route_descendant_counts.is_empty()
            && !self.request_route_descendant_counts.is_empty()
            && self.filtered_request_rows.len() > appended_indices.len()
        {
            self.filtered_route_descendant_counts = self.request_route_descendant_counts.clone();
        } else {
            for index in appended_indices {
                if let Some(meta) = self.request_tree_metas.get(*index) {
                    for group in &meta.ancestor_keys {
                        *self
                            .filtered_route_descendant_counts
                            .entry(group.clone())
                            .or_insert(0) += 1;
                    }
                }
            }
        }
        self.active_route_summary_cache = None;
        if should_select_first {
            self.table_state.select(Some(0));
        }
        self.perf.last_filter_ms = started.elapsed().as_millis();
        self.perf.max_filter_ms = self.perf.max_filter_ms.max(self.perf.last_filter_ms);
    }

    pub(super) fn response_updates_affect_request_query(&self) -> bool {
        filter_depends_on_response(&self.request_filter)
            || matches!(
                self.sort_mode,
                SortMode::Status | SortMode::Duration | SortMode::Size
            )
    }

    pub(super) fn can_apply_unfiltered_request_fast_path(&self) -> bool {
        self.request_filter.is_empty()
            && self.sql_request_filter_ids.is_none()
            && self.requests_hidden_before.is_none()
            && self.active_request_route_group.is_none()
            && matches!(self.sort_mode, SortMode::Started)
            && !self.sort_descending
    }

    fn unfiltered_request_query_result(&self) -> RequestQueryResult {
        let indices = (0..self.requests.len()).collect::<Vec<_>>();
        let route_descendant_counts = if self.request_route_descendant_counts.is_empty()
            && !self.request_tree_metas.is_empty()
        {
            descendant_counts_for_metas(&self.request_tree_metas)
        } else {
            self.request_route_descendant_counts.clone()
        };
        RequestQueryResult {
            rows: indices.clone(),
            indices,
            route_descendant_counts,
        }
    }

    fn request_query_items_iter(&self) -> impl Iterator<Item = RequestQueryItem<'_>> {
        self.requests
            .iter()
            .enumerate()
            .map(|(index, request)| self.request_query_item(index, request))
    }

    fn request_query_item<'a>(
        &'a self,
        index: usize,
        request: &'a RequestView,
    ) -> RequestQueryItem<'a> {
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
    }

    #[cfg(test)]
    fn request_query_items(&self) -> Vec<RequestQueryItem<'_>> {
        self.request_query_items_iter().collect()
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
            query_requests_iter(query_items.iter().copied(), &self.request_query_options())
        };
        self.filtered_route_descendant_counts = result.route_descendant_counts;
        self.active_route_summary_cache = self.compute_active_route_summary();
    }

    #[cfg(test)]
    pub(super) fn rebuild_filtered_request_rows(&mut self) {
        self.filtered_request_rows = self.filtered_request_indices.clone();
        self.rebuild_filtered_request_positions_by_id();
    }

    pub(super) fn filtered_index_for_request_id(&self, request_id: &str) -> Option<usize> {
        self.filtered_request_positions_by_id
            .get(request_id)
            .copied()
    }

    fn rebuild_filtered_request_positions_by_id(&mut self) {
        self.filtered_request_positions_by_id = self
            .filtered_request_rows
            .iter()
            .enumerate()
            .filter_map(|(position, request_index)| {
                self.requests
                    .get(*request_index)
                    .map(|request| (request.request.id.clone(), position))
            })
            .collect();
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
        let Some(meta) = self.request_tree_metas.get(request_index) else {
            return false;
        };
        let group = meta
            .group_key
            .as_deref()
            .filter(|key| self.route_group_child_count(key) > 0)
            .or_else(|| {
                meta.ancestor_keys
                    .iter()
                    .rev()
                    .map(String::as_str)
                    .find(|key| self.route_group_child_count(key) > 0)
            });
        let Some(group) = group else {
            return false;
        };
        self.active_request_route_group
            .as_deref()
            .map(|active| group != active && route_group_is_descendant_of(group, active))
            .unwrap_or(true)
    }

    pub(super) fn route_group_child_count(&self, group: &str) -> usize {
        self.filtered_route_descendant_counts
            .get(group)
            .copied()
            .unwrap_or(0)
    }

    #[cfg(test)]
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
        self.active_route_summary_cache.clone()
    }

    pub(super) fn compute_active_route_summary(&self) -> Option<RouteSummary> {
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
        let active_group = self.active_request_route_group.as_deref()?;
        let meta = self.request_tree_metas.get(request_index)?;
        let in_active_group = meta.ancestor_keys.iter().any(|key| key == active_group);
        if !in_active_group {
            return None;
        }
        Some(strip_route_segments(
            &meta.path,
            group_path_segment_count(active_group),
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

pub(crate) fn compute_request_stats(requests: &[RequestView]) -> RequestStats {
    let mut stats = RequestStats::default();
    for request in requests {
        add_request_stats_contribution(&mut stats, request_stats_contribution(request));
    }
    sync_request_stats_average(&mut stats);

    stats
}

pub(crate) fn apply_request_stats_replacement(
    stats: &mut RequestStats,
    before: Option<RequestStatsContribution>,
    after: Option<RequestStatsContribution>,
) -> bool {
    let requires_rebuild = before
        .as_ref()
        .and_then(|contribution| contribution.duration_ms)
        .is_some_and(|duration| stats.max_duration_ms == Some(duration));
    if let Some(before) = before {
        subtract_request_stats_contribution(stats, before);
    }
    if let Some(after) = after {
        add_request_stats_contribution(stats, after);
    }
    sync_request_stats_average(stats);
    requires_rebuild
}

#[derive(Clone, Copy)]
pub(crate) struct RequestStatsContribution {
    status_code: Option<i64>,
    replayed: bool,
    body_size: Option<i64>,
    duration_ms: Option<i64>,
}

pub(crate) fn request_stats_contribution(request: &RequestView) -> RequestStatsContribution {
    RequestStatsContribution {
        status_code: request.status_code(),
        replayed: !request.replays.is_empty(),
        body_size: request
            .response
            .as_ref()
            .and_then(|response| response.body_size),
        duration_ms: request.duration_ms(),
    }
}

fn add_request_stats_contribution(
    stats: &mut RequestStats,
    contribution: RequestStatsContribution,
) {
    match contribution.status_code {
        Some(200..=299) => stats.ok += 1,
        Some(300..=399) => stats.redirect += 1,
        Some(400..=499) => stats.client += 1,
        Some(500..=599) => stats.server += 1,
        None => stats.pending += 1,
        Some(_) => {}
    }
    if contribution.replayed {
        stats.replayed += 1;
    }
    if let Some(size) = contribution.body_size {
        stats.total_size += size;
    }
    if let Some(duration) = contribution.duration_ms {
        stats.duration_total_ms += duration;
        stats.duration_count += 1;
        stats.max_duration_ms = Some(stats.max_duration_ms.unwrap_or(duration).max(duration));
        if duration >= 500 {
            stats.slow += 1;
        }
    }
}

fn subtract_request_stats_contribution(
    stats: &mut RequestStats,
    contribution: RequestStatsContribution,
) {
    match contribution.status_code {
        Some(200..=299) => stats.ok = stats.ok.saturating_sub(1),
        Some(300..=399) => stats.redirect = stats.redirect.saturating_sub(1),
        Some(400..=499) => stats.client = stats.client.saturating_sub(1),
        Some(500..=599) => stats.server = stats.server.saturating_sub(1),
        None => stats.pending = stats.pending.saturating_sub(1),
        Some(_) => {}
    }
    if contribution.replayed {
        stats.replayed = stats.replayed.saturating_sub(1);
    }
    if let Some(size) = contribution.body_size {
        stats.total_size = stats.total_size.saturating_sub(size);
    }
    if let Some(duration) = contribution.duration_ms {
        stats.duration_total_ms = stats.duration_total_ms.saturating_sub(duration);
        stats.duration_count = stats.duration_count.saturating_sub(1);
        if duration >= 500 {
            stats.slow = stats.slow.saturating_sub(1);
        }
    }
}

fn sync_request_stats_average(stats: &mut RequestStats) {
    stats.avg_duration_ms =
        (stats.duration_count > 0).then(|| stats.duration_total_ms / stats.duration_count as i64);
}

fn route_group_is_descendant_of(group: &str, active: &str) -> bool {
    group
        .strip_prefix(active)
        .and_then(|suffix| suffix.strip_prefix('/'))
        .is_some()
}
