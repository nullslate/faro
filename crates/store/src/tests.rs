use super::*;
use faro_core::{
    ConsoleLevel, CookieEventRecord, CookieRecord, CookieSnapshotRecord, Header, ReplayRecord,
    RequestRecord, RequestStatus, ResponseRecord, RunTrigger, Session, StorageEntry,
    StorageSnapshotRecord, WebSocketFrameDirection, WebSocketFrameRecord, console_event,
    cookie_event_observed_event, cookie_observed_event, request_completed_event,
    request_replayed_event, request_started_event, response_received_event,
    storage_snapshot_created_event, websocket_frame_event,
};

type TestResult = std::result::Result<(), Box<dyn std::error::Error>>;

#[test]
fn persists_session_run_event_and_console_projection() -> TestResult {
    let store = Store::open_memory()?;
    let session = Session::new(
        Some("Smoke".to_string()),
        Some("http://localhost:3000".to_string()),
    );
    let tab = Tab::new(session.id.clone(), session.root_url.clone());
    let run = Run::new(
        session.id.clone(),
        tab.id.clone(),
        "http://localhost:3000".to_string(),
        RunTrigger::InitialLoad,
    );
    let log = ConsoleLog::new(
        session.id.clone(),
        Some(tab.id.clone()),
        Some(run.id.clone()),
        ConsoleLevel::Info,
        "hello from localhost".to_string(),
        Some("http://localhost:3000/main.js".to_string()),
        Some(42),
    );
    let error_log = ConsoleLog::new(
        session.id.clone(),
        Some(tab.id.clone()),
        Some(run.id.clone()),
        ConsoleLevel::Error,
        "boom".to_string(),
        Some("http://localhost:3000/main.js".to_string()),
        Some(50),
    );
    let event = console_event(&log);

    store.insert_session(&session)?;
    store.insert_tab(&tab)?;
    store.insert_run(&run)?;
    store.insert_console_log(&log)?;
    store.insert_console_log(&error_log)?;
    store.append_event(&event)?;

    assert!(store.session_exists(&session.id)?);
    assert_eq!(store.event_count()?, 1);

    let logs = store.console_logs_for_session(&session.id)?;
    assert_eq!(logs.len(), 2);
    assert!(logs.iter().any(|log| log.message == "hello from localhost"));
    let counts = store.session_summary_counts(&session.id)?;
    assert_eq!(counts.requests, 0);
    assert_eq!(counts.console_errors, 1);
    Ok(())
}

#[test]
fn deletes_session_and_cascades_children() -> TestResult {
    let store = Store::open_memory()?;
    let session = Session::new(None, Some("http://localhost:3000".to_string()));
    let tab = Tab::new(session.id.clone(), session.root_url.clone());
    let run = Run::new(
        session.id.clone(),
        tab.id.clone(),
        "http://localhost:3000".to_string(),
        RunTrigger::InitialLoad,
    );
    let request = RequestRecord::started(
        session.id.clone(),
        Some(tab.id.clone()),
        Some(run.id.clone()),
        "GET",
        "http://localhost:3000/api",
    );
    let body = BodyRecord::inline_text(
        Some("application/json".to_string()),
        "{}".to_string(),
        "test".to_string(),
    );
    let mut response = ResponseRecord::received(request.id.clone());
    response.body_ref = Some(body.id.clone());

    store.insert_session(&session)?;
    store.insert_tab(&tab)?;
    store.insert_run(&run)?;
    store.insert_body(&body)?;
    store.insert_request(&request)?;
    store.insert_response(&response)?;

    assert_eq!(store.requests_for_session(&session.id)?.len(), 1);
    assert_eq!(store.delete_session(&session.id)?, 1);
    assert!(!store.session_exists(&session.id)?);
    assert!(store.requests_for_session(&session.id)?.is_empty());
    assert!(store.response_body(&body.id)?.is_none());
    Ok(())
}

#[test]
fn deletes_all_sessions_and_cascades_children() -> TestResult {
    let store = Store::open_memory()?;
    let first = Session::new(None, Some("http://localhost:3000".to_string()));
    let second = Session::new(None, Some("http://localhost:4000".to_string()));
    let request = RequestRecord::started(
        first.id.clone(),
        None,
        None,
        "GET",
        "http://localhost:3000/api",
    );
    let body = BodyRecord::inline_text(
        Some("text/plain".to_string()),
        "ok".to_string(),
        "test".to_string(),
    );
    let mut response = ResponseRecord::received(request.id.clone());
    response.body_ref = Some(body.id.clone());

    store.insert_session(&first)?;
    store.insert_session(&second)?;
    store.insert_body(&body)?;
    store.insert_request(&request)?;
    store.insert_response(&response)?;

    assert_eq!(store.delete_all_sessions()?, 2);
    assert!(store.sessions()?.is_empty());
    assert!(store.requests_for_session(&first.id)?.is_empty());
    assert!(store.response_body(&body.id)?.is_none());
    Ok(())
}

#[test]
fn deletes_session_requests_before_cutoff_and_orphan_bodies() -> TestResult {
    let store = Store::open_memory()?;
    let session = Session::new(None, Some("http://localhost:3000".to_string()));
    let mut old_request = RequestRecord::started(
        session.id.clone(),
        None,
        None,
        "GET",
        "http://localhost:3000/old",
    );
    old_request.started_at = 10;
    let mut new_request = RequestRecord::started(
        session.id.clone(),
        None,
        None,
        "GET",
        "http://localhost:3000/new",
    );
    new_request.started_at = 20;
    let old_body = BodyRecord::inline_text(
        Some("text/plain".to_string()),
        "old".to_string(),
        "old".to_string(),
    );
    let new_body = BodyRecord::inline_text(
        Some("text/plain".to_string()),
        "new".to_string(),
        "new".to_string(),
    );
    let mut old_response = ResponseRecord::received(old_request.id.clone());
    old_response.body_ref = Some(old_body.id.clone());
    let mut new_response = ResponseRecord::received(new_request.id.clone());
    new_response.body_ref = Some(new_body.id.clone());

    store.insert_session(&session)?;
    store.insert_body(&old_body)?;
    store.insert_body(&new_body)?;
    store.insert_request(&old_request)?;
    store.insert_request(&new_request)?;
    store.insert_response(&old_response)?;
    store.insert_response(&new_response)?;

    assert_eq!(store.delete_session_requests_before(&session.id, 10)?, 1);
    let requests = store.requests_for_session(&session.id)?;
    assert_eq!(requests.len(), 1);
    assert_eq!(requests[0].id, new_request.id);
    assert!(store.response_body(&old_body.id)?.is_none());
    assert!(store.response_body(&new_body.id)?.is_some());
    Ok(())
}

#[test]
fn persists_network_projection_and_matching_events() -> TestResult {
    let store = Store::open_memory()?;
    let session = Session::new(None, Some("http://localhost:3000".to_string()));
    let tab = Tab::new(session.id.clone(), session.root_url.clone());
    let run = Run::new(
        session.id.clone(),
        tab.id.clone(),
        "http://localhost:3000".to_string(),
        RunTrigger::InitialLoad,
    );
    store.insert_session(&session)?;
    store.insert_tab(&tab)?;
    store.insert_run(&run)?;

    let mut request = RequestRecord::started(
        session.id.clone(),
        Some(tab.id.clone()),
        Some(run.id.clone()),
        "GET",
        "http://localhost:3000/api/todos",
    );
    request.browser_request_id = Some("cef-1".to_string());
    request.request_headers = vec![Header::new("accept", "application/json")];

    let mut response = ResponseRecord::received(request.id.clone());
    response.status_code = Some(200);
    response.mime_type = Some("application/json".to_string());
    response.body_size = Some(27);

    store.insert_request(&request)?;
    store.append_event(&request_started_event(&request))?;
    store.insert_response(&response)?;
    store.append_event(&response_received_event(
        session.id.clone(),
        Some(tab.id.clone()),
        Some(run.id.clone()),
        &response,
    ))?;

    request.completed_at = Some(faro_core::now_ms());
    request.status = RequestStatus::Complete;
    store.complete_request(&request)?;
    store.append_event(&request_completed_event(&request))?;

    assert_eq!(store.event_count()?, 3);

    let requests = store.requests_for_session(&session.id)?;
    assert_eq!(requests.len(), 1);
    assert_eq!(requests[0].status, RequestStatus::Complete);
    assert_eq!(requests[0].request_headers[0].name, "accept");

    let responses = store.responses_for_request(&request.id)?;
    assert_eq!(responses.len(), 1);
    assert_eq!(responses[0].status_code, Some(200));

    let mut replay = ReplayRecord::new(
        session.id.clone(),
        Some(tab.id.clone()),
        Some(run.id.clone()),
        request.id.clone(),
        "curl -X GET http://localhost:3000/api/todos".to_string(),
    );
    replay.exit_code = Some(0);
    replay.status_code = Some(200);
    store.insert_replay(&replay)?;
    store.append_event(&request_replayed_event(&replay))?;

    let replays = store.replays_for_request(&request.id)?;
    assert_eq!(replays.len(), 1);
    assert_eq!(replays[0].status_code, Some(200));
    assert_eq!(store.replays_for_session(&session.id)?.len(), 1);
    Ok(())
}

#[test]
fn persists_websocket_frames() -> TestResult {
    let store = Store::open_memory()?;
    let session = Session::new(None, Some("http://localhost:3000".to_string()));
    let tab = Tab::new(session.id.clone(), session.root_url.clone());
    let run = Run::new(
        session.id.clone(),
        tab.id.clone(),
        "http://localhost:3000".to_string(),
        RunTrigger::InitialLoad,
    );
    store.insert_session(&session)?;
    store.insert_tab(&tab)?;
    store.insert_run(&run)?;

    let frame = WebSocketFrameRecord::new(
        session.id.clone(),
        Some(tab.id.clone()),
        Some(run.id.clone()),
        "ws-1".to_string(),
        WebSocketFrameDirection::Received,
        1,
        false,
        "{\"type\":\"hello\"}".to_string(),
    );
    store.insert_websocket_frame(&frame)?;
    store.append_event(&websocket_frame_event(&frame))?;

    let frames = store.websocket_frames_for_session(&session.id)?;
    assert_eq!(frames.len(), 1);
    assert_eq!(frames[0].browser_request_id, "ws-1");
    assert_eq!(frames[0].direction, WebSocketFrameDirection::Received);
    assert_eq!(frames[0].payload, "{\"type\":\"hello\"}");
    Ok(())
}

#[test]
fn persists_storage_and_cookie_snapshots() -> TestResult {
    let store = Store::open_memory()?;
    let session = Session::new(None, Some("http://localhost:3000".to_string()));
    let tab = Tab::new(session.id.clone(), session.root_url.clone());
    let run = Run::new(
        session.id.clone(),
        tab.id.clone(),
        "http://localhost:3000".to_string(),
        RunTrigger::InitialLoad,
    );
    store.insert_session(&session)?;
    store.insert_tab(&tab)?;
    store.insert_run(&run)?;

    let storage = StorageSnapshotRecord::new(
        session.id.clone(),
        Some(tab.id.clone()),
        Some(run.id.clone()),
        "http://localhost:3000".to_string(),
        "localStorage".to_string(),
        vec![StorageEntry::new("token", "abc")],
        "sha".to_string(),
    );
    store.insert_storage_snapshot(&storage)?;
    store.append_event(&storage_snapshot_created_event(&storage))?;

    let cookies = CookieSnapshotRecord::new(
        session.id.clone(),
        Some(tab.id.clone()),
        Some(run.id.clone()),
        Some("http://localhost:3000".to_string()),
        vec![CookieRecord {
            name: "sid".to_string(),
            value: "123".to_string(),
            domain: "localhost".to_string(),
            path: "/".to_string(),
            expires: None,
            http_only: true,
            secure: false,
            same_site: Some("Lax".to_string()),
        }],
    );
    store.insert_cookie_snapshot(&cookies)?;
    store.append_event(&cookie_observed_event(&cookies))?;
    let cookie_event = CookieEventRecord::new(
        session.id.clone(),
        Some(tab.id.clone()),
        Some(run.id.clone()),
        "document.cookie",
        Some("sid".to_string()),
        Some("localhost".to_string()),
        Some("/".to_string()),
        Some("123".to_string()),
        None,
    );
    store.insert_cookie_event(&cookie_event)?;
    store.append_event(&cookie_event_observed_event(&cookie_event))?;

    let storage_snapshots = store.storage_snapshots_for_session(&session.id)?;
    assert_eq!(storage_snapshots.len(), 1);
    assert_eq!(storage_snapshots[0].entries[0].key, "token");

    let cookie_snapshots = store.cookie_snapshots_for_session(&session.id)?;
    assert_eq!(cookie_snapshots.len(), 1);
    assert_eq!(cookie_snapshots[0].cookies[0].name, "sid");
    let cookie_events = store.cookie_events_for_session(&session.id)?;
    assert_eq!(cookie_events.len(), 1);
    assert_eq!(cookie_events[0].operation, "document.cookie");
    assert_eq!(store.event_count()?, 3);
    Ok(())
}

#[test]
fn validates_readonly_sql_queries() -> TestResult {
    validate_readonly_sql(
        "
            -- recent requests
            SELECT method, url
            FROM requests
            WHERE url LIKE '%api%';
            ",
    )?;

    assert!(validate_readonly_sql("SELECT 1; SELECT 2").is_err());
    assert!(validate_readonly_sql("UPDATE requests SET method = 'GET'").is_err());
    assert!(validate_readonly_sql("PRAGMA writable_schema = 1").is_err());
    Ok(())
}

#[test]
fn executes_readonly_sql_query() -> TestResult {
    let db_path = std::env::temp_dir().join(format!("faro-query-test-{}.db", uuid::Uuid::new_v4()));
    let session = Session::new(None, Some("https://example.test".to_string()));
    {
        let store = Store::open(&db_path)?;
        store.insert_session(&session)?;
    }

    let result = Store::query_readonly(
        &db_path,
        "SELECT root_url, name FROM sessions ORDER BY created_at DESC",
    )?;
    assert_eq!(result.columns, vec!["root_url", "name"]);
    assert_eq!(result.rows.len(), 1);
    assert_eq!(result.rows[0][0], "https://example.test");
    assert_eq!(result.rows[0][1], "NULL");
    std::fs::remove_file(db_path)?;
    Ok(())
}
