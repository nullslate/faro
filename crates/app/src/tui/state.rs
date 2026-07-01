use crate::config::AppConfig;
use crate::query::{
    RequestQueryItem, RequestQueryMeta, RequestQueryOptions, RequestQueryResult,
    filter_console_indices, filter_depends_on_response, filter_websocket_indices,
    latest_responses_by_request, query_requests_iter,
};
use crate::services::{
    build_curl_args as service_build_curl_args, build_curl_command, session_summary,
};
use crate::tui::live_refresh::{LiveRefreshDelta, LiveRefreshRequest};
use anyhow::Context;
use faro_core::{
    ConsoleLevel, ConsoleLog, CookieEventRecord, CookieSnapshotRecord, ReplayRecord, RequestRecord,
    ResponseRecord, Session, StorageEventRecord, StorageSnapshotRecord, UnixMillis,
    WebSocketFrameDirection, WebSocketFrameRecord, now_ms,
};
use faro_store::{ScriptRecord, Store};
use ratatui::widgets::{ListState, TableState};
use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::time::Instant;

use super::layout::{DensityMode, LayoutMode, LayoutPreference, clamp_split_percent};

mod body;
mod chrome;
mod data;
mod detail;
mod lifecycle;
mod palette;
mod requests;
mod routes;
mod scripts;
mod streams;
mod types;

#[cfg(test)]
pub(crate) use detail::build_curl_args;
pub(crate) use detail::{formatted_request_body, formatted_response_body, looks_like_json};
use palette::{
    CONSOLE_FILTER_PRESETS, FILTER_PRESETS, PALETTE_ENTRIES, WEBSOCKET_FILTER_PRESETS,
    filter_preset_status, filter_query_for_preset_label, next_filter_preset, palette_matches,
};
pub(crate) use palette::{PaletteCommand, PaletteEntry};
pub(super) use requests::{
    apply_request_stats_replacement, compute_request_stats, request_stats_contribution,
};
use routes::{
    append_request_tree_metas, build_request_tree_metas, descendant_counts_for_metas, group_label,
    group_path_segment_count, parent_group_key, route_breadcrumb_for_group, route_label_for_group,
    strip_route_segments,
};
pub(crate) use routes::{domain_for_url, path_for_url};
pub(crate) use types::{
    BodyTreeCache, BodyTreeItem, CapturedFavicon, ConsoleDetailLineCache, ConsoleStats,
    CurrentCookieEntry, CurrentStorageEntry, DetailTab, FocusPane, InputMode, LayoutPreset,
    LiveWatermarks, PerfStats, ReplayView, RequestStats, RequestTreeMeta, RequestView,
    ResponseBodyLineCache, RouteSummary, SessionView, SortMode, SqlResultsView,
    WebSocketDetailLineCache, WebSocketStats, WorkbenchView,
};

pub(crate) type ReplayContext = (String, Option<String>, Option<String>, String, String);

pub(crate) struct WorkbenchState {
    pub(crate) config: AppConfig,
    pub(crate) db_path: std::path::PathBuf,
    pub(crate) target_url: String,
    pub(crate) active_session_id: Option<String>,
    pub(crate) sessions: Vec<SessionView>,
    pub(crate) session_state: ListState,
    pub(crate) requests: Vec<RequestView>,
    pub(crate) request_indices_by_id: HashMap<String, usize>,
    pub(crate) request_tree_metas: Vec<RequestTreeMeta>,
    pub(crate) request_route_descendant_counts: HashMap<String, usize>,
    pub(crate) filtered_request_indices: Vec<usize>,
    pub(crate) filtered_request_rows: Vec<usize>,
    pub(crate) filtered_request_positions_by_id: HashMap<String, usize>,
    pub(crate) filtered_route_descendant_counts: HashMap<String, usize>,
    pub(crate) request_stats: RequestStats,
    pub(crate) live_watermarks: LiveWatermarks,
    pub(crate) live_requests_since_prune: usize,
    pub(crate) active_route_summary_cache: Option<RouteSummary>,
    pub(crate) collapsed_request_groups: HashSet<String>,
    pub(crate) active_request_route_group: Option<String>,
    pub(crate) sql_request_filter_ids: Option<HashSet<String>>,
    pub(crate) sql_request_filter_query: Option<String>,
    pub(crate) requests_hidden_before: Option<UnixMillis>,
    pub(crate) console_logs: Vec<ConsoleLog>,
    pub(crate) filtered_console_indices: Vec<usize>,
    pub(crate) filtered_console_positions_by_id: HashMap<String, usize>,
    pub(crate) console_hidden_before: Option<UnixMillis>,
    pub(crate) console_stats: ConsoleStats,
    pub(crate) console_detail_line_cache: RefCell<Option<ConsoleDetailLineCache>>,
    pub(crate) websocket_frames: Vec<WebSocketFrameRecord>,
    pub(crate) filtered_websocket_indices: Vec<usize>,
    pub(crate) filtered_websocket_positions_by_id: HashMap<String, usize>,
    pub(crate) websocket_state: ListState,
    pub(crate) websocket_detail_scroll: u16,
    pub(crate) websocket_detail_line_cache: RefCell<Option<WebSocketDetailLineCache>>,
    pub(crate) websocket_stats: WebSocketStats,
    pub(crate) websocket_connection_ids: HashSet<String>,
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
    pub(crate) selected_replay_index: usize,
    pub(crate) body_scroll: u16,
    pub(crate) body_tree_selected: usize,
    pub(crate) body_tree_selected_key: Option<String>,
    pub(crate) collapsed_body_nodes: HashSet<String>,
    pub(crate) body_tree_cache: RefCell<Option<BodyTreeCache>>,
    pub(crate) response_body_line_cache: RefCell<Option<ResponseBodyLineCache>>,
    pub(crate) captured_favicon_cache: RefCell<Option<Option<CapturedFavicon>>>,
    pub(crate) storage_scroll: u16,
    pub(crate) cookie_scroll: u16,
    pub(crate) input_mode: InputMode,
    pub(crate) layout_mode: LayoutMode,
    pub(crate) density_mode: DensityMode,
    pub(crate) requests_percent: u16,
    pub(crate) detail_percent: u16,
    pub(crate) palette_query: String,
    pub(crate) palette_selected: usize,
    pub(crate) body_search_query: String,
    pub(crate) show_help: bool,
    pub(crate) show_sessions: bool,
    pub(crate) show_theme_preview: bool,
    pub(crate) show_perf: bool,
    pub(crate) perf: PerfStats,
    pub(crate) sql_result: Option<SqlResultsView>,
    pub(crate) sql_row_scroll: usize,
    pub(crate) sql_col_scroll: usize,
    pub(crate) last_sql_query: String,
    pub(crate) request_filter: String,
    pub(crate) console_filter: String,
    pub(crate) websocket_filter: String,
    pub(crate) cdp_websocket_url: Option<String>,
    pub(crate) status: String,
    pub(crate) status_updated_at: Instant,
}

pub(crate) fn body_text_for_ref(
    store: &Store,
    body_id: Option<&str>,
) -> anyhow::Result<Option<String>> {
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

pub(crate) fn replay_view_for_record(
    store: &Store,
    record: ReplayRecord,
) -> anyhow::Result<ReplayView> {
    let body = body_text_for_ref(store, record.response_body_ref.as_deref())
        .with_context(|| format!("load replay body for {}", record.id))?;
    Ok(ReplayView { record, body })
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

fn adjusted_percent(value: u16, delta: i16) -> u16 {
    clamp_split_percent(value.saturating_add_signed(delta))
}

#[cfg(test)]
mod tests {
    use super::*;
    use faro_core::{Header, RequestStatus};

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

    fn synthetic_request_view(index: usize) -> RequestView {
        let method = if index.is_multiple_of(9) {
            "POST"
        } else {
            "GET"
        };
        let path = match index % 6 {
            0 => format!("/api/users/{index}/profile"),
            1 => format!("/api/organizations/{}/members", index % 400),
            2 => format!("/assets/vendor-{index:x}.js"),
            3 => format!("/assets/chunk-{index:x}.css"),
            4 => "/graphql".to_string(),
            _ => format!("/events/stream/{}", index % 40),
        };
        let mut request = RequestRecord::started(
            "session".to_string(),
            Some("tab".to_string()),
            Some("run".to_string()),
            method,
            format!("http://localhost:5173{path}?q={index}"),
        );
        request.id = format!("request-{index:06}");
        request.started_at = index as i64;
        request.completed_at = Some(index as i64 + (index % 750) as i64);
        request.status = RequestStatus::Complete;
        request.resource_type = Some(
            match index % 6 {
                0 | 1 => "fetch",
                2 => "script",
                3 => "stylesheet",
                4 => "xhr",
                _ => "eventsource",
            }
            .to_string(),
        );

        let mut response = ResponseRecord::received(request.id.clone());
        response.id = format!("response-{index:06}");
        response.received_at = request.completed_at.unwrap_or(request.started_at);
        response.status_code = Some(if index.is_multiple_of(97) {
            500
        } else if index.is_multiple_of(41) {
            404
        } else {
            200
        });
        response.mime_type = Some(
            match request.resource_type.as_deref() {
                Some("script") => "application/javascript",
                Some("stylesheet") => "text/css",
                Some("eventsource") => "text/event-stream",
                _ => "application/json",
            }
            .to_string(),
        );
        response.body_size = Some(((index % 200_000) + 128) as i64);

        RequestView {
            request,
            response: Some(response),
            request_body: None,
            response_body: None,
            replays: Vec::new(),
            details_loaded: false,
        }
    }

    fn load_synthetic_state(request_count: usize) -> anyhow::Result<WorkbenchState> {
        let store = Store::open_memory()?;
        let mut state = WorkbenchState::load(
            &store,
            std::path::Path::new("memory.db"),
            "http://localhost:5173",
            AppConfig::default(),
        )?;
        state.requests = (0..request_count).map(synthetic_request_view).collect();
        state.request_indices_by_id = state
            .requests
            .iter()
            .enumerate()
            .map(|(index, request)| (request.request.id.clone(), index))
            .collect();
        state.request_tree_metas = build_request_tree_metas(&state.requests);
        state.request_route_descendant_counts =
            descendant_counts_for_metas(&state.request_tree_metas);
        state.request_stats = compute_request_stats(&state.requests);
        state.apply_filter();
        Ok(state)
    }

    fn log_perf(label: &str, elapsed: std::time::Duration, rows: usize) {
        println!("{label}: {:?} ({rows} rows)", elapsed);
    }

    #[test]
    #[ignore = "large-session perf harness; run with cargo test large_session -- --ignored --nocapture"]
    fn large_session_filter_perf_harness() -> TestResult {
        let mut state = load_synthetic_state(25_000)?;

        let started = std::time::Instant::now();
        state.request_filter.clear();
        state.apply_filter();
        log_perf(
            "large_session apply_filter all",
            started.elapsed(),
            state.filtered_request_rows.len(),
        );

        let started = std::time::Instant::now();
        state.request_filter = "type:fetch status:2xx".to_string();
        state.apply_filter();
        log_perf(
            "large_session apply_filter fetch 2xx",
            started.elapsed(),
            state.filtered_request_rows.len(),
        );

        let started = std::time::Instant::now();
        state.request_filter = "has:error".to_string();
        state.apply_filter();
        log_perf(
            "large_session apply_filter errors",
            started.elapsed(),
            state.filtered_request_rows.len(),
        );

        Ok(())
    }

    #[test]
    #[ignore = "large-session perf harness; run with cargo test large_session -- --ignored --nocapture"]
    fn large_session_route_append_perf_harness() -> TestResult {
        let mut state = load_synthetic_state(25_000)?;
        let existing_count = state.requests.len();
        let appended = (existing_count..existing_count + 1_000)
            .map(synthetic_request_view)
            .collect::<Vec<_>>();
        let appended_indices =
            (existing_count..existing_count + appended.len()).collect::<Vec<_>>();
        state.requests.extend(appended);

        let started = std::time::Instant::now();
        append_request_tree_metas(
            &mut state.request_tree_metas,
            &state.requests,
            &appended_indices,
            &state.request_route_descendant_counts,
        );
        for index in &appended_indices {
            if let Some(meta) = state.request_tree_metas.get(*index) {
                for group in &meta.ancestor_keys {
                    *state
                        .request_route_descendant_counts
                        .entry(group.clone())
                        .or_insert(0) += 1;
                }
            }
        }
        state.sync_unfiltered_request_filter_state(&appended_indices);
        log_perf(
            "large_session append 1000 tree/filter rows",
            started.elapsed(),
            state.filtered_request_rows.len(),
        );

        Ok(())
    }

    #[test]
    fn curl_args_request_decoded_compressed_replay_output() {
        let request = request_view();
        let args = build_curl_args(&request);

        assert!(args.iter().any(|arg| arg == "--compressed"));
    }

    #[test]
    fn clear_visible_requests_keeps_active_filter_and_tracks_fresh_requests() -> TestResult {
        let store = Store::open_memory()?;
        let mut state = WorkbenchState::load(
            &store,
            std::path::Path::new("memory.db"),
            "http://localhost:5173",
            AppConfig::default(),
        )?;
        let mut old_fetch = request_view();
        old_fetch.request.started_at = now_ms().saturating_sub(1_000);
        let old_fetch_id = old_fetch.request.id.clone();
        let mut document = request_view();
        document.request.resource_type = Some("document".to_string());
        document.request.started_at = old_fetch.request.started_at;
        state.requests = vec![old_fetch, document];
        state.request_filter = "type:fetch".to_string();
        state.apply_filter();

        assert_eq!(state.filtered_request_indices.len(), 1);
        state.clear_visible_requests();

        assert_eq!(state.request_filter, "type:fetch");
        assert!(state.filtered_request_indices.is_empty());
        assert!(
            state
                .requests_hidden_before
                .is_some_and(|hidden_before| hidden_before >= state.requests[0].request.started_at)
        );

        let mut new_fetch = request_view();
        new_fetch.request.started_at = state
            .requests_hidden_before
            .unwrap_or_default()
            .saturating_add(1);
        let new_fetch_id = new_fetch.request.id.clone();
        state.requests.push(new_fetch);
        state.apply_filter();

        assert_eq!(state.filtered_request_indices.len(), 1);
        let visible = &state.requests[state.filtered_request_indices[0]].request.id;
        assert_eq!(visible, &new_fetch_id);
        assert_ne!(visible, &old_fetch_id);
        Ok(())
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
    fn cycles_filter_presets_for_active_view() -> TestResult {
        let store = Store::open_memory()?;
        let mut state = WorkbenchState::load(
            &store,
            std::path::Path::new("memory.db"),
            "http://localhost:5173",
            AppConfig::default(),
        )?;
        state.request_filter.clear();

        state.set_view(WorkbenchView::Console);
        state.cycle_filter_preset();
        assert_eq!(state.console_filter, "level:error");
        assert!(state.request_filter.is_empty());

        state.set_view(WorkbenchView::WebSockets);
        state.cycle_filter_preset();
        assert_eq!(state.websocket_filter, "sent");
        assert!(state.request_filter.is_empty());
        Ok(())
    }

    #[test]
    fn body_tree_respects_configured_item_limit() -> TestResult {
        let store = Store::open_memory()?;
        let mut state = WorkbenchState::load(
            &store,
            std::path::Path::new("memory.db"),
            "http://localhost:5173",
            AppConfig::default(),
        )?;
        state.config.ui.max_body_tree_items = 100;
        let mut request = request_view();
        request.response_body = Some(format!(
            r#"{{"items":[{}]}}"#,
            (0..150)
                .map(|index| format!(r#"{{"id":{index},"name":"item-{index}"}}"#))
                .collect::<Vec<_>>()
                .join(",")
        ));
        state.requests = vec![request];
        state.request_tree_metas = build_request_tree_metas(&state.requests);
        state.filtered_request_indices = vec![0];
        state.rebuild_filtered_request_rows();
        state.table_state.select(Some(0));

        let items = state.body_tree_items();
        assert!(items.len() <= 101);
        assert!(items.iter().any(|item| item.label == "truncated"));
        Ok(())
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
        state.filtered_request_indices = vec![0, 1];
        state.rebuild_filtered_route_descendant_counts();
        state.rebuild_filtered_request_rows();

        assert_eq!(
            state.collapsible_group_key_for_request_index(0).as_deref(),
            Some("localhost:5173/api/users")
        );
        assert_eq!(
            state.collapsible_group_key_for_request_index(1).as_deref(),
            Some("localhost:5173/api/users")
        );
        Ok(())
    }

    #[test]
    fn appending_request_tree_metas_matches_full_rebuild() {
        let mut parent = request_view();
        parent.request.url = "http://localhost:5173/api/users".to_string();
        let mut child = request_view();
        child.request.url = "http://localhost:5173/api/users/123".to_string();
        let mut sibling = request_view();
        sibling.request.url = "http://localhost:5173/api/projects/456".to_string();

        let mut requests = vec![parent];
        let mut incremental = build_request_tree_metas(&requests);
        let mut descendant_counts = descendant_counts_for_metas(&incremental);
        requests.push(child);
        requests.push(sibling);
        append_request_tree_metas(&mut incremental, &requests, &[1, 2], &descendant_counts);
        for index in [1, 2] {
            if let Some(meta) = incremental.get(index) {
                for group in &meta.ancestor_keys {
                    *descendant_counts.entry(group.clone()).or_insert(0) += 1;
                }
            }
        }

        let rebuilt = build_request_tree_metas(&requests);
        assert_eq!(incremental.len(), rebuilt.len());
        for (left, right) in incremental.iter().zip(rebuilt.iter()) {
            assert_eq!(left.domain, right.domain);
            assert_eq!(left.path, right.path);
            assert_eq!(left.depth, right.depth);
            assert_eq!(left.group_key, right.group_key);
            assert_eq!(left.ancestor_keys, right.ancestor_keys);
        }
        assert_eq!(descendant_counts, descendant_counts_for_metas(&rebuilt));
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
        state.rebuild_filtered_route_descendant_counts();
        state.rebuild_filtered_request_rows();

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
        state.filtered_request_indices = vec![0, 1];
        state.rebuild_filtered_route_descendant_counts();
        state.rebuild_filtered_request_rows();

        let Some(group) = state.collapsible_group_key_for_request_index(0) else {
            panic!("missing collapsible group");
        };
        assert_eq!(group, "localhost:5173/api/users");
        assert_eq!(state.route_group_child_count(&group), 2);
        assert_eq!(
            state.collapsible_group_key_for_request_index(1).as_deref(),
            Some(group.as_str())
        );
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
        state.rebuild_filtered_route_descendant_counts();
        state.rebuild_filtered_request_rows();

        assert!(state.request_can_drill_down(0));
        assert!(state.request_can_drill_down(1));
        state.active_request_route_group = state.collapsible_group_key_for_request_index(0);
        assert!(!state.request_can_drill_down(0));
        assert!(!state.request_can_drill_down(1));
        Ok(())
    }
}
