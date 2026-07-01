pub(super) const SCHEMA: &str = r#"
CREATE TABLE IF NOT EXISTS sessions (
    id TEXT PRIMARY KEY,
    created_at INTEGER NOT NULL,
    name TEXT,
    root_url TEXT
);

CREATE TABLE IF NOT EXISTS tabs (
    id TEXT PRIMARY KEY,
    session_id TEXT NOT NULL REFERENCES sessions(id) ON DELETE CASCADE,
    created_at INTEGER NOT NULL,
    current_url TEXT,
    title TEXT
);

CREATE TABLE IF NOT EXISTS runs (
    id TEXT PRIMARY KEY,
    session_id TEXT NOT NULL REFERENCES sessions(id) ON DELETE CASCADE,
    tab_id TEXT NOT NULL REFERENCES tabs(id) ON DELETE CASCADE,
    started_at INTEGER NOT NULL,
    ended_at INTEGER,
    url TEXT NOT NULL,
    trigger TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS events (
    id TEXT PRIMARY KEY,
    session_id TEXT NOT NULL REFERENCES sessions(id) ON DELETE CASCADE,
    tab_id TEXT REFERENCES tabs(id) ON DELETE SET NULL,
    run_id TEXT REFERENCES runs(id) ON DELETE SET NULL,
    ts INTEGER NOT NULL,
    kind TEXT NOT NULL,
    payload_json TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_events_session_ts ON events(session_id, ts);
CREATE INDEX IF NOT EXISTS idx_events_kind_ts ON events(kind, ts);

CREATE TABLE IF NOT EXISTS requests (
    id TEXT PRIMARY KEY,
    session_id TEXT NOT NULL REFERENCES sessions(id) ON DELETE CASCADE,
    tab_id TEXT REFERENCES tabs(id) ON DELETE SET NULL,
    run_id TEXT REFERENCES runs(id) ON DELETE SET NULL,
    browser_request_id TEXT,
    started_at INTEGER NOT NULL,
    completed_at INTEGER,
    method TEXT NOT NULL,
    url TEXT NOT NULL,
    resource_type TEXT,
    initiator TEXT,
    request_headers_json TEXT,
    request_body_ref TEXT,
    status TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_requests_session_started ON requests(session_id, started_at);
CREATE INDEX IF NOT EXISTS idx_requests_session_completed ON requests(session_id, completed_at);
CREATE INDEX IF NOT EXISTS idx_requests_run_started ON requests(run_id, started_at);

CREATE TABLE IF NOT EXISTS responses (
    id TEXT PRIMARY KEY,
    request_id TEXT NOT NULL REFERENCES requests(id) ON DELETE CASCADE,
    received_at INTEGER NOT NULL,
    status_code INTEGER,
    status_text TEXT,
    mime_type TEXT,
    response_headers_json TEXT,
    body_ref TEXT,
    body_size INTEGER,
    body_truncated INTEGER NOT NULL DEFAULT 0
);

CREATE INDEX IF NOT EXISTS idx_responses_request_received ON responses(request_id, received_at, id);
CREATE INDEX IF NOT EXISTS idx_responses_received_request ON responses(received_at, request_id, id);

CREATE TABLE IF NOT EXISTS replays (
    id TEXT PRIMARY KEY,
    session_id TEXT NOT NULL REFERENCES sessions(id) ON DELETE CASCADE,
    tab_id TEXT REFERENCES tabs(id) ON DELETE SET NULL,
    run_id TEXT REFERENCES runs(id) ON DELETE SET NULL,
    source_request_id TEXT NOT NULL REFERENCES requests(id) ON DELETE CASCADE,
    ts INTEGER NOT NULL,
    command TEXT NOT NULL,
    exit_code INTEGER,
    status_code INTEGER,
    response_body_ref TEXT,
    output_path TEXT,
    error TEXT
);

CREATE INDEX IF NOT EXISTS idx_replays_request_ts ON replays(source_request_id, ts);
CREATE INDEX IF NOT EXISTS idx_replays_session_ts ON replays(session_id, ts);

CREATE TABLE IF NOT EXISTS bodies (
    id TEXT PRIMARY KEY,
    content_type TEXT,
    encoding TEXT,
    size INTEGER NOT NULL,
    sha256 TEXT NOT NULL,
    storage_kind TEXT NOT NULL,
    data BLOB
);

CREATE TABLE IF NOT EXISTS console_logs (
    id TEXT PRIMARY KEY,
    session_id TEXT NOT NULL REFERENCES sessions(id) ON DELETE CASCADE,
    tab_id TEXT REFERENCES tabs(id) ON DELETE SET NULL,
    run_id TEXT REFERENCES runs(id) ON DELETE SET NULL,
    ts INTEGER NOT NULL,
    level TEXT NOT NULL,
    message TEXT NOT NULL,
    source TEXT,
    line INTEGER,
    stack_json TEXT
);

CREATE INDEX IF NOT EXISTS idx_console_session_ts ON console_logs(session_id, ts);
CREATE INDEX IF NOT EXISTS idx_console_run_ts ON console_logs(run_id, ts);

CREATE TABLE IF NOT EXISTS storage_snapshots (
    id TEXT PRIMARY KEY,
    session_id TEXT NOT NULL REFERENCES sessions(id) ON DELETE CASCADE,
    tab_id TEXT REFERENCES tabs(id) ON DELETE SET NULL,
    run_id TEXT REFERENCES runs(id) ON DELETE SET NULL,
    ts INTEGER NOT NULL,
    origin TEXT NOT NULL,
    storage_type TEXT NOT NULL,
    data_json TEXT NOT NULL,
    sha256 TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS storage_events (
    id TEXT PRIMARY KEY,
    session_id TEXT NOT NULL REFERENCES sessions(id) ON DELETE CASCADE,
    tab_id TEXT REFERENCES tabs(id) ON DELETE SET NULL,
    run_id TEXT REFERENCES runs(id) ON DELETE SET NULL,
    ts INTEGER NOT NULL,
    origin TEXT NOT NULL,
    storage_type TEXT NOT NULL,
    operation TEXT NOT NULL,
    key TEXT,
    old_value TEXT,
    new_value TEXT,
    stack_json TEXT
);

CREATE TABLE IF NOT EXISTS cookie_snapshots (
    id TEXT PRIMARY KEY,
    session_id TEXT NOT NULL REFERENCES sessions(id) ON DELETE CASCADE,
    tab_id TEXT REFERENCES tabs(id) ON DELETE SET NULL,
    run_id TEXT REFERENCES runs(id) ON DELETE SET NULL,
    ts INTEGER NOT NULL,
    url TEXT,
    cookies_json TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS cookie_events (
    id TEXT PRIMARY KEY,
    session_id TEXT NOT NULL REFERENCES sessions(id) ON DELETE CASCADE,
    tab_id TEXT REFERENCES tabs(id) ON DELETE SET NULL,
    run_id TEXT REFERENCES runs(id) ON DELETE SET NULL,
    ts INTEGER NOT NULL,
    operation TEXT NOT NULL,
    name TEXT,
    domain TEXT,
    path TEXT,
    value TEXT,
    attributes_json TEXT
);

CREATE TABLE IF NOT EXISTS websocket_frames (
    id TEXT PRIMARY KEY,
    session_id TEXT NOT NULL REFERENCES sessions(id) ON DELETE CASCADE,
    tab_id TEXT REFERENCES tabs(id) ON DELETE SET NULL,
    run_id TEXT REFERENCES runs(id) ON DELETE SET NULL,
    browser_request_id TEXT NOT NULL,
    ts INTEGER NOT NULL,
    direction TEXT NOT NULL,
    opcode INTEGER NOT NULL,
    mask INTEGER NOT NULL,
    payload TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_websocket_frames_session_ts ON websocket_frames(session_id, ts);
CREATE INDEX IF NOT EXISTS idx_websocket_frames_request_ts ON websocket_frames(browser_request_id, ts);

CREATE TABLE IF NOT EXISTS scripts (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    body TEXT NOT NULL,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL,
    last_run_at INTEGER
);

CREATE INDEX IF NOT EXISTS idx_scripts_updated ON scripts(updated_at DESC, name ASC);
"#;
