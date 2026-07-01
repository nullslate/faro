use super::*;
use crate::config::AppConfig;
use crate::tui::state::{InputMode, SortMode};
use faro_core::{
    CookieEventRecord, CookieRecord, CookieSnapshotRecord, ReplayRecord, RequestRecord,
    ResponseRecord, StorageEntry, StorageEventRecord, StorageSnapshotRecord,
};
use faro_store::ScriptRecord;
use ratatui::widgets::{ListState, TableState};
use std::path::PathBuf;

fn state_with_storage(
    storage_snapshots: Vec<StorageSnapshotRecord>,
    storage_events: Vec<StorageEventRecord>,
) -> WorkbenchState {
    WorkbenchState {
        config: AppConfig::default(),
        db_path: PathBuf::from("/tmp/faro-test.db"),
        target_url: "http://localhost:5173".to_string(),
        active_session_id: None,
        sessions: Vec::new(),
        session_state: ListState::default(),
        requests: Vec::new(),
        request_tree_metas: Vec::new(),
        filtered_request_indices: Vec::new(),
        filtered_request_rows: Vec::new(),
        filtered_route_descendant_counts: std::collections::HashMap::new(),
        collapsed_request_groups: std::collections::HashSet::new(),
        active_request_route_group: None,
        sql_request_filter_ids: None,
        sql_request_filter_query: None,
        requests_hidden_before: None,
        console_logs: Vec::new(),
        filtered_console_indices: Vec::new(),
        console_hidden_before: None,
        websocket_frames: Vec::new(),
        filtered_websocket_indices: Vec::new(),
        websocket_state: ListState::default(),
        websocket_detail_scroll: 0,
        storage_events,
        storage_snapshots,
        storage_selected: 0,
        cookie_events: Vec::new(),
        cookie_snapshots: Vec::new(),
        cookie_selected: 0,
        scripts: Vec::new(),
        script_state: ListState::default(),
        script_output: Vec::new(),
        script_status: None,
        script_duration_ms: None,
        table_state: TableState::default(),
        console_state: ListState::default(),
        view: WorkbenchView::Network,
        focus: FocusPane::Requests,
        detail_tab: DetailTab::Overview,
        sort_mode: SortMode::Started,
        sort_descending: false,
        detail_scroll: 0,
        selected_replay_index: usize::MAX,
        body_scroll: 0,
        body_tree_selected: 0,
        body_tree_selected_key: None,
        collapsed_body_nodes: std::collections::HashSet::new(),
        storage_scroll: 0,
        cookie_scroll: 0,
        input_mode: InputMode::Normal,
        layout_mode: LayoutMode::Normal,
        density_mode: DensityMode::Compact,
        requests_percent: 48,
        detail_percent: 38,
        palette_query: String::new(),
        palette_selected: 0,
        body_search_query: String::new(),
        show_help: false,
        show_sessions: false,
        show_theme_preview: false,
        show_perf: false,
        perf: Default::default(),
        sql_result: None,
        sql_row_scroll: 0,
        sql_col_scroll: 0,
        last_sql_query: String::new(),
        request_filter: String::new(),
        console_filter: String::new(),
        websocket_filter: String::new(),
        cdp_websocket_url: None,
        status: String::new(),
        status_updated_at: std::time::Instant::now(),
    }
}

fn response_request(mime: &str, resource_type: &str, url: &str) -> RequestView {
    let mut request = RequestRecord::started(
        "session".to_string(),
        Some("tab".to_string()),
        Some("run".to_string()),
        "GET",
        url,
    );
    request.resource_type = Some(resource_type.to_string());
    let mut response = ResponseRecord::received(request.id.clone());
    response.status_code = Some(200);
    response.mime_type = Some(mime.to_string());
    RequestView {
        request,
        response: Some(response),
        request_body: None,
        response_body: None,
        replays: Vec::new(),
        details_loaded: true,
    }
}

fn state_with_cookies(
    cookie_snapshots: Vec<CookieSnapshotRecord>,
    cookie_events: Vec<CookieEventRecord>,
) -> WorkbenchState {
    WorkbenchState {
        cookie_events,
        cookie_snapshots,
        ..state_with_storage(Vec::new(), Vec::new())
    }
}

#[test]
fn derives_current_storage_from_snapshot_and_live_events() {
    let session_id = "session".to_string();
    let tab_id = Some("tab".to_string());
    let run_id = Some("run".to_string());
    let snapshot = StorageSnapshotRecord::new(
        session_id.clone(),
        tab_id.clone(),
        run_id.clone(),
        "http://localhost:5173".to_string(),
        "localStorage".to_string(),
        vec![
            StorageEntry::new("stale", "old"),
            StorageEntry::new("keep", "value"),
        ],
        "hash".to_string(),
    );
    let events = vec![
        StorageEventRecord::new(
            session_id.clone(),
            tab_id.clone(),
            run_id.clone(),
            "http://localhost:5173".to_string(),
            "localStorage".to_string(),
            "update".to_string(),
            Some("stale".to_string()),
            Some("old".to_string()),
            Some("new".to_string()),
        ),
        StorageEventRecord::new(
            session_id.clone(),
            tab_id.clone(),
            run_id.clone(),
            "http://localhost:5173".to_string(),
            "localStorage".to_string(),
            "remove".to_string(),
            Some("keep".to_string()),
            Some("value".to_string()),
            None,
        ),
        StorageEventRecord::new(
            session_id,
            tab_id,
            run_id,
            "http://localhost:5173".to_string(),
            "sessionStorage".to_string(),
            "set".to_string(),
            Some("token".to_string()),
            None,
            Some("abc".to_string()),
        ),
    ];

    let app = state_with_storage(vec![snapshot], events);
    let entries = app.current_storage_entries();

    assert_eq!(entries.len(), 2);
    assert!(entries.iter().any(|entry| {
        entry.storage_type == "localStorage" && entry.key == "stale" && entry.value == "new"
    }));
    assert!(entries.iter().any(|entry| {
        entry.storage_type == "sessionStorage" && entry.key == "token" && entry.value == "abc"
    }));
}

#[test]
fn storage_clear_only_affects_matching_origin_and_type() {
    let session_id = "session".to_string();
    let tab_id = Some("tab".to_string());
    let run_id = Some("run".to_string());
    let snapshot = StorageSnapshotRecord::new(
        session_id.clone(),
        tab_id.clone(),
        run_id.clone(),
        "http://localhost:5173".to_string(),
        "localStorage".to_string(),
        vec![StorageEntry::new("gone", "1")],
        "hash".to_string(),
    );
    let other_snapshot = StorageSnapshotRecord::new(
        session_id.clone(),
        tab_id.clone(),
        run_id.clone(),
        "http://localhost:5173".to_string(),
        "sessionStorage".to_string(),
        vec![StorageEntry::new("kept", "2")],
        "hash".to_string(),
    );
    let clear = StorageEventRecord::new(
        session_id,
        tab_id,
        run_id,
        "http://localhost:5173".to_string(),
        "localStorage".to_string(),
        "clear".to_string(),
        None,
        None,
        None,
    );

    let app = state_with_storage(vec![snapshot, other_snapshot], vec![clear]);
    let entries = app.current_storage_entries();

    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].storage_type, "sessionStorage");
    assert_eq!(entries[0].key, "kept");
}

#[test]
fn derives_current_cookies_from_snapshot_and_events() {
    let session_id = "session".to_string();
    let tab_id = Some("tab".to_string());
    let run_id = Some("run".to_string());
    let snapshot = CookieSnapshotRecord::new(
        session_id.clone(),
        tab_id.clone(),
        run_id.clone(),
        Some("http://localhost:5173".to_string()),
        vec![CookieRecord {
            name: "theme".to_string(),
            value: "light".to_string(),
            domain: "localhost".to_string(),
            path: "/".to_string(),
            expires: None,
            http_only: false,
            secure: false,
            same_site: Some("Lax".to_string()),
        }],
    );
    let event = CookieEventRecord::new(
        session_id,
        tab_id,
        run_id,
        "document.cookie",
        Some("theme".to_string()),
        Some("localhost".to_string()),
        Some("/".to_string()),
        Some("dark".to_string()),
        Some(serde_json::json!({"sameSite": "Strict"})),
    );

    let app = state_with_cookies(vec![snapshot], vec![event]);
    let cookies = app.current_cookie_entries();

    assert_eq!(cookies.len(), 1);
    assert_eq!(cookies[0].name, "theme");
    assert_eq!(cookies[0].value, "dark");
    assert!(cookies[0].flags.contains("sameSite"));
}

#[test]
fn syntax_body_lines_highlights_json() -> anyhow::Result<()> {
    let lines = syntax_body_lines(serde_json::to_string_pretty(&serde_json::json!({
        "name": "faro",
        "count": 3,
        "ok": true,
        "empty": null
    }))?);

    let spans = lines
        .iter()
        .flat_map(|line| line.spans.iter())
        .collect::<Vec<_>>();
    assert!(spans.iter().any(|span| span.content.as_ref() == "\"name\""));
    assert!(spans.iter().any(|span| span.content.as_ref() == "\"faro\""));
    assert!(spans.iter().any(|span| span.content.as_ref() == "3"));
    assert!(spans.iter().any(|span| span.content.as_ref() == "true"));
    assert!(spans.iter().any(|span| span.content.as_ref() == "null"));
    Ok(())
}

#[test]
fn syntax_body_lines_leaves_plain_text_plain() {
    let lines = syntax_body_lines("not-json: true".to_string());

    assert_eq!(lines.len(), 1);
    assert_eq!(lines[0].spans[0].content.as_ref(), "not-json: true");
}

#[test]
fn syntax_body_lines_strips_terminal_controls() {
    let lines = syntax_body_lines("ok\u{1b}[31mred\u{1b}[0m\u{7}done".to_string());

    assert_eq!(lines.len(), 1);
    assert_eq!(lines[0].spans[0].content.as_ref(), "okreddone");
}

#[test]
fn syntax_body_lines_highlights_html_response() {
    let request = response_request("text/html", "document", "https://example.test/");
    let lines =
        syntax_body_lines_for_request(&request, r#"<main class="shell">Hello</main>"#.to_string());
    let spans = lines
        .iter()
        .flat_map(|line| line.spans.iter())
        .collect::<Vec<_>>();

    assert!(spans.iter().any(|span| span.content.as_ref() == "main"));
    assert!(spans.iter().any(|span| span.content.as_ref() == "class"));
    assert!(
        spans
            .iter()
            .any(|span| span.content.as_ref() == r#""shell""#)
    );
}

#[test]
fn syntax_body_lines_highlights_css_response() {
    let request = response_request("text/css", "stylesheet", "https://example.test/app.css");
    let lines = syntax_body_lines_for_request(
        &request,
        ".shell { color: #d4be98; margin: 12px; }".to_string(),
    );
    let spans = lines
        .iter()
        .flat_map(|line| line.spans.iter())
        .collect::<Vec<_>>();

    assert!(spans.iter().any(|span| span.content.as_ref() == "color"));
    assert!(spans.iter().any(|span| span.content.as_ref() == "#d4be98"));
    assert!(spans.iter().any(|span| span.content.as_ref() == "12px"));
}

#[test]
fn syntax_body_lines_highlights_javascript_response() {
    let request = response_request(
        "application/javascript",
        "script",
        "https://example.test/app.js",
    );
    let lines =
        syntax_body_lines_for_request(&request, "const title = document.title;".to_string());
    let spans = lines
        .iter()
        .flat_map(|line| line.spans.iter())
        .collect::<Vec<_>>();

    assert!(spans.iter().any(|span| span.content.as_ref() == "const"));
    assert!(spans.iter().any(|span| span.content.as_ref() == "document"));
    assert!(spans.iter().any(|span| span.content.as_ref() == "title"));
}

#[test]
fn response_body_content_lines_adds_line_number_gutter_when_active() {
    let mut request = response_request("application/json", "fetch", "https://example.test/api");
    request.response_body = Some("{\n  \"ok\": true\n}".to_string());

    let lines = response_body_content_lines(&request, true);

    assert_eq!(lines[0].spans[0].content.as_ref(), "  1 ");
    assert_eq!(lines[0].spans[1].content.as_ref(), "│ ");
    assert!(
        lines[1]
            .spans
            .iter()
            .any(|span| span.content.as_ref() == r#""ok""#)
    );
}

#[test]
fn console_stack_lines_formats_call_frames() {
    let stack = serde_json::json!({
        "callFrames": [{
            "functionName": "loadUser",
            "url": "https://example.test/app.js",
            "lineNumber": 41,
            "columnNumber": 7
        }]
    });

    let text = console_stack_lines(&stack)
        .iter()
        .flat_map(|line| line.spans.iter())
        .map(|span| span.content.as_ref())
        .collect::<String>();

    assert!(text.contains("loadUser"));
    assert!(text.contains("https://example.test/app.js"));
    assert!(text.contains(":42:8"));
}

#[test]
fn script_output_lines_highlight_selected_script_source() {
    let mut app = state_with_storage(Vec::new(), Vec::new());
    app.scripts.push(ScriptRecord::new(
        "Example",
        "let failed = faros.requests.filter(#{ status: #{ gte: 400 } });",
    ));
    app.script_state.select(Some(0));
    let lines = script_output_lines(&app);
    let spans = lines
        .iter()
        .flat_map(|line| line.spans.iter())
        .collect::<Vec<_>>();

    assert!(spans.iter().any(|span| span.content.as_ref() == "source "));
    assert!(spans.iter().any(|span| span.content.as_ref() == "let"));
    assert!(spans.iter().any(|span| span.content.as_ref() == "faros"));
    assert!(spans.iter().any(|span| span.content.as_ref() == "400"));
}

#[test]
fn response_body_syntax_applies_when_inactive_too() {
    let request = response_request("text/css", "stylesheet", "https://example.test/app.css");
    let body = ".shell { color: #d4be98; }";
    let mut active_request = request;
    active_request.response_body = Some(body.to_string());

    let active = response_body_content_lines(&active_request, true);
    let inactive = response_body_content_lines(&active_request, false);

    assert!(
        active[0]
            .spans
            .iter()
            .any(|span| span.content.as_ref() == "color")
    );
    assert!(
        inactive[0]
            .spans
            .iter()
            .any(|span| span.content.as_ref() == "color")
    );
    assert_eq!(active[0].spans[0].content.as_ref(), "  1 ");
    assert_ne!(inactive[0].spans[0].content.as_ref(), "  1 ");
}

#[test]
fn view_tabs_include_websockets_with_matching_shortcuts() {
    let app = state_with_storage(Vec::new(), Vec::new());
    let text = view_tabs_line(&app)
        .spans
        .iter()
        .map(|span| span.content.as_ref())
        .collect::<String>();

    for expected in [
        "1",
        "Net",
        "2 Console",
        "3 WS",
        "4 Scripts",
        "5 Storage",
        "6 Cookies",
    ] {
        assert!(text.contains(expected));
    }
}

#[test]
fn detail_tabs_only_show_interactive_pills() {
    let app = state_with_storage(Vec::new(), Vec::new());
    let text = detail_tab_lines(&app, 120)
        .into_iter()
        .flat_map(|line| line.spans)
        .map(|span| span.content)
        .collect::<String>();

    for expected in [
        "overview", "req hdr", "req body", "res hdr", "res body", "timing", "replay",
    ] {
        assert!(text.contains(expected));
    }
    for label in ["meta", "request", "response", "tools"] {
        assert!(!text.contains(label));
    }
}

#[test]
fn detail_tabs_wrap_and_compact_when_narrow() {
    let app = state_with_storage(Vec::new(), Vec::new());
    let lines = detail_tab_lines(&app, 44);

    assert!(lines.len() >= 2);
    for line in lines {
        assert!(line_width(&line) <= 43);
    }
}

#[test]
fn replay_lines_show_history_and_selected_body() {
    let mut app = state_with_storage(Vec::new(), Vec::new());
    let mut request = response_request("application/json", "fetch", "https://example.test/api");
    let mut first = ReplayRecord::new(
        "session".to_string(),
        None,
        None,
        request.request.id.clone(),
        "curl first".to_string(),
    );
    first.status_code = Some(500);
    first.exit_code = Some(0);
    let mut second = ReplayRecord::new(
        "session".to_string(),
        None,
        None,
        request.request.id.clone(),
        "curl second".to_string(),
    );
    second.status_code = Some(200);
    second.exit_code = Some(0);
    request.replays = vec![
        ReplayView {
            record: first,
            body: Some(r#"{"ok":false}"#.to_string()),
        },
        ReplayView {
            record: second,
            body: Some(r#"{"ok":true}"#.to_string()),
        },
    ];
    app.requests.push(request);
    app.filtered_request_indices = vec![0];
    app.filtered_request_rows = vec![0];
    app.table_state.select(Some(0));
    app.selected_replay_index = 0;

    let Some(selected_request) = app.selected_request() else {
        panic!("request missing");
    };
    let text = replay_lines(&app, selected_request, 100)
        .iter()
        .flat_map(|line| line.spans.iter())
        .map(|span| span.content.as_ref())
        .collect::<String>();

    assert!(text.contains("history 2"));
    assert!(text.contains("selected 1/2"));
    assert!(text.contains("replay history"));
    assert!(text.contains("hidden in replay view"));
    assert!(!text.contains("\"ok\""));
}

#[test]
fn request_tree_marker_shows_dot_only_for_rows_with_children() {
    let theme = Theme::default();
    let parent = RequestTreeMeta {
        domain: "localhost".to_string(),
        path: "/api".to_string(),
        depth: 1,
        group_key: None,
        ancestor_keys: Vec::new(),
        has_children: true,
        child_count: 2,
        collapsed: false,
    };
    let leaf = RequestTreeMeta {
        has_children: false,
        ..parent.clone()
    };
    let deep_parent = RequestTreeMeta {
        depth: 7,
        has_children: true,
        ..parent.clone()
    };

    let parent_text = request_tree_marker(0, 2, Some(&parent), true, RowFade::Full, &theme)
        .spans
        .iter()
        .map(|span| span.content.as_ref())
        .collect::<String>();
    let leaf_text = request_tree_marker(1, 2, Some(&leaf), false, RowFade::Full, &theme)
        .spans
        .iter()
        .map(|span| span.content.as_ref())
        .collect::<String>();
    let deep_parent_text =
        request_tree_marker(1, 2, Some(&deep_parent), true, RowFade::Full, &theme)
            .spans
            .iter()
            .map(|span| span.content.as_ref())
            .collect::<String>();
    let non_drillable_parent_text =
        request_tree_marker(1, 2, Some(&deep_parent), false, RowFade::Full, &theme)
            .spans
            .iter()
            .map(|span| span.content.as_ref())
            .collect::<String>();

    assert_eq!(parent_text, "›");
    assert_eq!(leaf_text, "");
    assert!(deep_parent_text.starts_with("›"));
    assert!(!non_drillable_parent_text.contains('›'));
}

#[test]
fn parse_sse_events_groups_fields() {
    let events = parse_sse_events(
        "id: 1\nevent: patch\ndata: {\"ok\":true}\n\nretry: 5000\ndata: heartbeat\n\n",
    );

    assert_eq!(events.len(), 2);
    assert_eq!(events[0].id.as_deref(), Some("1"));
    assert_eq!(events[0].event.as_deref(), Some("patch"));
    assert_eq!(events[0].data, vec![r#"{"ok":true}"#]);
    assert_eq!(events[1].retry.as_deref(), Some("5000"));
    assert_eq!(events[1].data, vec!["heartbeat"]);
}

#[test]
fn console_eval_lines_render_prompt_and_result_separately() {
    let log = ConsoleLog::new(
        "session".to_string(),
        None,
        None,
        ConsoleLevel::Info,
        "> const value = await fetch('/api')\n{\"ok\":true}".to_string(),
        Some("faro-console".to_string()),
        None,
    );

    let lines = console_log_lines(&log);
    let rendered = lines
        .iter()
        .map(|line| {
            line.spans
                .iter()
                .map(|span| span.content.as_ref())
                .collect::<String>()
        })
        .collect::<Vec<_>>();

    assert!(rendered.iter().any(|line| line.starts_with("> ")));
    assert!(rendered.iter().any(|line| line.starts_with("< ")));
    assert!(rendered.iter().any(|line| line.contains("\"ok\"")));
}

#[test]
fn console_log_lines_preserve_multiline_messages() {
    let log = ConsoleLog::new(
        "session".to_string(),
        None,
        None,
        ConsoleLevel::Error,
        "first line\nsecond line".to_string(),
        Some("page".to_string()),
        None,
    );

    let lines = console_log_lines(&log);

    assert_eq!(lines.len(), 2);
    assert!(
        lines[0]
            .spans
            .iter()
            .any(|span| span.content.as_ref() == "first line")
    );
    assert!(
        lines[1]
            .spans
            .iter()
            .any(|span| span.content.as_ref() == "second line")
    );
}
