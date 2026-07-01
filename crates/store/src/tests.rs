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
fn reports_database_table_row_counts() -> TestResult {
    let store = Store::open_memory()?;
    let session = Session::new(None, Some("http://localhost:3000".to_string()));
    let request = RequestRecord::started(
        session.id.clone(),
        None,
        None,
        "GET",
        "http://localhost:3000/api",
    );

    store.insert_session(&session)?;
    store.insert_request(&request)?;

    let counts = store.table_row_counts()?;
    assert_eq!(table_count(&counts, "sessions"), Some(1));
    assert_eq!(table_count(&counts, "requests"), Some(1));
    assert_eq!(table_count(&counts, "responses"), Some(0));
    Ok(())
}

#[test]
fn reports_body_storage_stats() -> TestResult {
    let store = Store::open_memory()?;
    let inline = BodyRecord::inline_text(
        Some("application/json".to_string()),
        "{\"ok\":true}".to_string(),
        "inline-sha".to_string(),
    );
    let external = BodyRecord {
        id: "external-body".to_string(),
        content_type: Some("application/octet-stream".to_string()),
        encoding: "binary".to_string(),
        size: 4096,
        sha256: "external-sha".to_string(),
        storage_kind: "external".to_string(),
        data: Vec::new(),
    };

    store.insert_body(&inline)?;
    store.insert_body(&external)?;

    let stats = store.body_storage_stats()?;
    assert_eq!(stats.bodies, 2);
    assert_eq!(stats.total_bytes, inline.size as u64 + 4096);
    assert_eq!(stats.inline_bytes, inline.size as u64);
    assert_eq!(stats.external_bytes, 4096);
    Ok(())
}

#[test]
fn reports_top_session_storage_stats() -> TestResult {
    let store = Store::open_memory()?;
    let small = Session::new(None, Some("http://small.test".to_string()));
    let large = Session::new(None, Some("http://large.test".to_string()));
    store.insert_session(&small)?;
    store.insert_session(&large)?;

    let small_request =
        RequestRecord::started(small.id.clone(), None, None, "GET", "http://small.test/a");
    let small_body = BodyRecord::inline_text(None, "small".to_string(), "small-sha".to_string());
    let mut small_response = ResponseRecord::received(small_request.id.clone());
    small_response.body_ref = Some(small_body.id.clone());
    store.insert_body(&small_body)?;
    store.insert_request(&small_request)?;
    store.insert_response(&small_response)?;

    let large_request =
        RequestRecord::started(large.id.clone(), None, None, "GET", "http://large.test/a");
    let large_body = BodyRecord {
        id: "large-body".to_string(),
        content_type: Some("application/json".to_string()),
        encoding: "utf-8".to_string(),
        size: 8192,
        sha256: "large-sha".to_string(),
        storage_kind: "external".to_string(),
        data: Vec::new(),
    };
    let mut large_response = ResponseRecord::received(large_request.id.clone());
    large_response.body_ref = Some(large_body.id.clone());
    store.insert_body(&large_body)?;
    store.insert_request(&large_request)?;
    store.insert_response(&large_response)?;

    let stats = store.top_session_storage_stats(5)?;
    assert_eq!(stats.len(), 2);
    assert_eq!(stats[0].id, large.id);
    assert_eq!(stats[0].requests, 1);
    assert_eq!(stats[0].bodies, 1);
    assert_eq!(stats[0].body_bytes, 8192);
    assert_eq!(stats[1].id, small.id);
    Ok(())
}

#[test]
fn reports_top_repeated_request_groups() -> TestResult {
    let store = Store::open_memory()?;
    let session = Session::new(None, Some("http://localhost:3000".to_string()));
    store.insert_session(&session)?;

    for index in 0..3 {
        let mut request = RequestRecord::started(
            session.id.clone(),
            None,
            None,
            "GET",
            "http://localhost:3000/api/poll?active=1",
        );
        request.id = format!("poll-{index}");
        request.started_at = index;
        request.resource_type = Some("fetch".to_string());
        let mut response = ResponseRecord::received(request.id.clone());
        response.id = format!("poll-response-{index}");
        response.status_code = Some(if index == 2 { 500 } else { 200 });
        response.body_size = Some(100);
        store.insert_request(&request)?;
        store.insert_response(&response)?;
    }

    let mut singleton = RequestRecord::started(
        session.id.clone(),
        None,
        None,
        "GET",
        "http://localhost:3000/api/once",
    );
    singleton.id = "once".to_string();
    singleton.resource_type = Some("fetch".to_string());
    store.insert_request(&singleton)?;

    let groups = store.top_repeated_request_groups(5)?;
    assert_eq!(groups.len(), 1);
    assert_eq!(groups[0].session_id, session.id);
    assert_eq!(groups[0].method, "GET");
    assert_eq!(groups[0].resource_type.as_deref(), Some("fetch"));
    assert_eq!(groups[0].url, "http://localhost:3000/api/poll?active=1");
    assert_eq!(groups[0].requests, 3);
    assert_eq!(groups[0].error_responses, 1);
    assert_eq!(groups[0].body_bytes, 300);
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
fn prunes_session_requests_to_newest_rows() -> TestResult {
    let store = Store::open_memory()?;
    let session = Session::new(None, Some("http://localhost:3000".to_string()));
    store.insert_session(&session)?;

    let mut requests = Vec::new();
    for index in 0..5 {
        let mut request = RequestRecord::started(
            session.id.clone(),
            None,
            None,
            "GET",
            format!("http://localhost:3000/items/{index}"),
        );
        request.started_at = index;
        let body = BodyRecord::inline_text(
            Some("text/plain".to_string()),
            format!("body-{index}"),
            format!("body-{index}"),
        );
        let mut response = ResponseRecord::received(request.id.clone());
        response.body_ref = Some(body.id.clone());
        store.insert_body(&body)?;
        store.insert_request(&request)?;
        store.append_event(&request_started_event(&request))?;
        store.insert_response(&response)?;
        requests.push((request, body));
    }

    assert_eq!(store.prune_session_requests(&session.id, 3)?, 2);
    let retained = store.requests_for_session(&session.id)?;
    assert_eq!(retained.len(), 3);
    assert_eq!(retained[0].url, "http://localhost:3000/items/2");
    assert!(store.response_body(&requests[0].1.id)?.is_none());
    assert!(store.response_body(&requests[4].1.id)?.is_some());
    assert_eq!(request_event_count(&store, &session.id)?, 3);
    Ok(())
}

#[test]
fn prunes_repeated_session_requests_per_method_url_and_type() -> TestResult {
    let store = Store::open_memory()?;
    let session = Session::new(None, Some("http://localhost:3000".to_string()));
    store.insert_session(&session)?;

    for index in 0..5 {
        let mut request = RequestRecord::started(
            session.id.clone(),
            None,
            None,
            "GET",
            "http://localhost:3000/poll",
        );
        request.started_at = index;
        request.resource_type = Some("XHR".to_string());
        store.insert_request(&request)?;
    }
    let mut distinct = RequestRecord::started(
        session.id.clone(),
        None,
        None,
        "POST",
        "http://localhost:3000/poll",
    );
    distinct.started_at = 6;
    distinct.resource_type = Some("XHR".to_string());
    store.insert_request(&distinct)?;

    assert_eq!(store.prune_repeated_session_requests(&session.id, 2)?, 3);
    let retained = store.requests_for_session(&session.id)?;
    assert_eq!(retained.len(), 3);
    assert_eq!(
        retained
            .iter()
            .filter(|request| request.method == "GET")
            .count(),
        2
    );
    assert!(retained.iter().any(|request| request.method == "POST"));
    Ok(())
}

#[test]
#[ignore = "large-session perf harness; run with cargo test large_session -- --ignored --nocapture"]
fn large_session_store_query_perf_harness() -> TestResult {
    let store = Store::open_memory()?;
    let session = Session::new(None, Some("http://localhost:5173".to_string()));
    store.insert_session(&session)?;

    let started = std::time::Instant::now();
    for index in 0usize..25_000 {
        let mut request = RequestRecord::started(
            session.id.clone(),
            None,
            None,
            if index.is_multiple_of(9) {
                "POST"
            } else {
                "GET"
            },
            format!(
                "http://localhost:5173/{}/{}?q={index}",
                if index.is_multiple_of(3) {
                    "api/users"
                } else {
                    "assets"
                },
                index % 1_000
            ),
        );
        request.id = format!("request-{index:06}");
        request.started_at = index as i64;
        request.completed_at = Some((index + (index % 500)) as i64);
        request.status = RequestStatus::Complete;
        request.resource_type = Some(
            if index.is_multiple_of(3) {
                "fetch"
            } else if index % 3 == 1 {
                "script"
            } else {
                "stylesheet"
            }
            .to_string(),
        );
        let mut response = ResponseRecord::received(request.id.clone());
        response.id = format!("response-{index:06}");
        response.received_at = request.completed_at.unwrap_or(request.started_at);
        response.status_code = Some(if index.is_multiple_of(97) { 500 } else { 200 });
        response.body_size = Some(((index % 200_000) + 128) as i64);

        store.insert_request(&request)?;
        store.insert_response(&response)?;
    }
    println!("large_session store seed 25000: {:?}", started.elapsed());

    let started = std::time::Instant::now();
    let requests = store.requests_for_session(&session.id)?;
    println!(
        "large_session store load requests: {:?} ({} rows)",
        started.elapsed(),
        requests.len()
    );

    let started = std::time::Instant::now();
    let changed_requests = store.requests_for_session_changed_after(&session.id, 24_500, 24_500)?;
    println!(
        "large_session store changed requests: {:?} ({} rows)",
        started.elapsed(),
        changed_requests.len()
    );

    let started = std::time::Instant::now();
    let changed_responses = store.responses_for_session_after(&session.id, 24_500)?;
    println!(
        "large_session store changed responses: {:?} ({} rows)",
        started.elapsed(),
        changed_responses.len()
    );

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

fn request_event_count(store: &Store, session_id: &str) -> Result<usize> {
    let count = store.conn.query_row(
        "SELECT count(*)
         FROM events
         WHERE session_id = ?1
           AND kind IN ('request_started', 'response_received', 'request_completed')",
        params![session_id],
        |row| row.get::<_, i64>(0),
    )?;
    Ok(rows::sqlite_count_to_usize(count))
}

fn stream_event_count(store: &Store, session_id: &str, kind: &str) -> Result<usize> {
    let count = store.conn.query_row(
        "SELECT count(*)
         FROM events
         WHERE session_id = ?1
           AND kind = ?2",
        params![session_id, kind],
        |row| row.get::<_, i64>(0),
    )?;
    Ok(rows::sqlite_count_to_usize(count))
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
fn prunes_console_logs_to_newest_rows() -> TestResult {
    let store = Store::open_memory()?;
    let session = Session::new(None, Some("http://localhost:3000".to_string()));
    store.insert_session(&session)?;

    for index in 0..5 {
        let mut log = ConsoleLog::new(
            session.id.clone(),
            None,
            None,
            ConsoleLevel::Info,
            format!("log-{index}"),
            None,
            None,
        );
        log.ts = index;
        store.insert_console_log(&log)?;
        store.append_event(&console_event(&log))?;
    }

    assert_eq!(store.prune_session_console_logs(&session.id, 2)?, 3);
    let logs = store.console_logs_for_session(&session.id)?;
    assert_eq!(logs.len(), 2);
    assert_eq!(logs[0].message, "log-3");
    assert_eq!(logs[1].message, "log-4");
    assert_eq!(
        stream_event_count(&store, &session.id, "console_logged")?,
        2
    );
    Ok(())
}

#[test]
fn prunes_websocket_frames_to_newest_rows() -> TestResult {
    let store = Store::open_memory()?;
    let session = Session::new(None, Some("http://localhost:3000".to_string()));
    store.insert_session(&session)?;

    for index in 0..5 {
        let mut frame = WebSocketFrameRecord::new(
            session.id.clone(),
            None,
            None,
            "ws-1".to_string(),
            WebSocketFrameDirection::Received,
            1,
            false,
            format!("frame-{index}"),
        );
        frame.ts = index;
        store.insert_websocket_frame(&frame)?;
        store.append_event(&websocket_frame_event(&frame))?;
    }

    assert_eq!(store.prune_session_websocket_frames(&session.id, 2)?, 3);
    let frames = store.websocket_frames_for_session(&session.id)?;
    assert_eq!(frames.len(), 2);
    assert_eq!(frames[0].payload, "frame-3");
    assert_eq!(frames[1].payload, "frame-4");
    assert_eq!(
        stream_event_count(&store, &session.id, "websocket_frame")?,
        2
    );
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

fn table_count(counts: &[TableRowCount], table: &str) -> Option<usize> {
    counts
        .iter()
        .find(|count| count.table == table)
        .map(|count| count.rows)
}
