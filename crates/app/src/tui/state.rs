use crate::config::AppConfig;
use crate::query::{
    RequestQueryItem, RequestQueryMeta, RequestQueryOptions, RequestQueryResult,
    filter_console_indices, filter_websocket_indices, latest_responses_by_request, query_requests,
};
use crate::services::{
    build_curl_args as service_build_curl_args, build_curl_command, session_summary,
};
use anyhow::Context;
use faro_core::{
    ConsoleLog, CookieEventRecord, CookieSnapshotRecord, ReplayRecord, RequestRecord,
    ResponseRecord, Session, StorageEventRecord, StorageSnapshotRecord, UnixMillis,
    WebSocketFrameRecord, now_ms,
};
use faro_store::{ScriptRecord, Store};
use ratatui::widgets::{ListState, TableState};
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
use routes::{
    build_request_tree_metas, group_label, group_path_segment_count, parent_group_key,
    route_breadcrumb_for_group, route_label_for_group, strip_route_segments,
};
pub(crate) use routes::{domain_for_url, path_for_url};
pub(crate) use types::{
    BodyTreeItem, CurrentCookieEntry, CurrentStorageEntry, DetailTab, FocusPane, InputMode,
    LayoutPreset, PerfStats, ReplayView, RequestTreeMeta, RequestView, RouteSummary, SessionView,
    SortMode, SqlResultsView, WorkbenchView,
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
    pub(crate) request_tree_metas: Vec<RequestTreeMeta>,
    pub(crate) filtered_request_indices: Vec<usize>,
    pub(crate) filtered_request_rows: Vec<usize>,
    pub(crate) filtered_route_descendant_counts: HashMap<String, usize>,
    pub(crate) collapsed_request_groups: HashSet<String>,
    pub(crate) active_request_route_group: Option<String>,
    pub(crate) sql_request_filter_ids: Option<HashSet<String>>,
    pub(crate) sql_request_filter_query: Option<String>,
    pub(crate) requests_hidden_before: Option<UnixMillis>,
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
    pub(crate) selected_replay_index: usize,
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
