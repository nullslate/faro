use serde_json::{Map, Value, json};

pub(super) fn tools() -> Value {
    json!([
        tool(
            "capture_url",
            "Launch or attach Chrome, capture a URL for a bounded duration, and persist observations in the Faro DB.",
            object_schema(
                &[
                    ("url", "string", "URL to open and capture."),
                    (
                        "duration",
                        "string",
                        "Optional duration such as 10s, 500ms, or 1m."
                    ),
                    (
                        "duration_ms",
                        "number",
                        "Optional capture duration in milliseconds."
                    )
                ],
                &["url"]
            )
        ),
        tool(
            "list_sessions",
            "List Faro capture sessions with summary counts. Use this first when multiple sessions may exist.",
            object_schema(&[], &[])
        ),
        tool(
            "get_session",
            "Get one Faro capture session and summary counts. Defaults to latest when session_id is omitted.",
            object_schema(
                &[(
                    "session_id",
                    "string",
                    "Optional Faro session id. Omit to use the latest session."
                )],
                &[]
            )
        ),
        tool(
            "get_db_stats",
            "Get Faro database maintenance stats including body storage totals, heavy sessions, repeated request groups, and table row counts.",
            object_schema(
                &[
                    (
                        "session_limit",
                        "number",
                        "Maximum heavy sessions to include, capped at 25."
                    ),
                    (
                        "repeated_limit",
                        "number",
                        "Maximum repeated request groups to include, capped at 50."
                    )
                ],
                &[]
            )
        ),
        tool(
            "list_heavy_sessions",
            "List sessions ordered by captured body bytes and request volume.",
            object_schema(
                &[(
                    "limit",
                    "number",
                    "Maximum sessions to return, capped at 100."
                )],
                &[]
            )
        ),
        tool(
            "list_repeated_requests",
            "List repeated method/type/url groups that are likely to make a session noisy or large.",
            object_schema(
                &[(
                    "limit",
                    "number",
                    "Maximum groups to return, capped at 200."
                )],
                &[]
            )
        ),
        tool(
            "prune_session",
            "Prune one session using retention limits. Requires confirm=true and --mcp-allow-mutation.",
            object_schema(
                &[
                    ("session_id", "string", "Faro session id to prune."),
                    ("confirm", "boolean", "Must be true to prune session data."),
                    ("max_requests", "number", "Keep only the newest N requests."),
                    (
                        "max_repeated",
                        "number",
                        "Keep only newest N requests per method/url/type group."
                    ),
                    (
                        "max_console",
                        "number",
                        "Keep only the newest N console logs."
                    ),
                    (
                        "max_ws",
                        "number",
                        "Keep only the newest N WebSocket frames."
                    ),
                    (
                        "vacuum",
                        "boolean",
                        "Checkpoint WAL and vacuum after pruning."
                    )
                ],
                &["session_id", "confirm"]
            )
        ),
        tool(
            "delete_all_sessions",
            "Delete all Faro sessions and cascaded captured data. Requires confirm=true.",
            object_schema(
                &[("confirm", "boolean", "Must be true to delete all sessions.")],
                &["confirm"]
            )
        ),
        tool(
            "list_requests",
            "List captured network requests from a session. Defaults to the latest session.",
            object_schema(
                &[
                    (
                        "session_id",
                        "string",
                        "Optional Faro session id. Omit to use the latest session."
                    ),
                    (
                        "route",
                        "string",
                        "Optional route filter, e.g. /api/users, /api/users/:id, or /api/*."
                    ),
                    (
                        "filter",
                        "string",
                        "Optional expression filter, e.g. status >= 400."
                    ),
                    ("limit", "number", "Maximum rows to return, capped at 500.")
                ],
                &[]
            )
        ),
        tool(
            "get_request",
            "Get a captured request and response by request id.",
            object_schema(
                &[
                    ("request_id", "string", "Faro request id."),
                    (
                        "include_body",
                        "boolean",
                        "Include captured request/response body text."
                    )
                ],
                &["request_id"]
            )
        ),
        tool(
            "get_response_body",
            "Get the captured response body for a request id.",
            object_schema(
                &[("request_id", "string", "Faro request id.")],
                &["request_id"]
            )
        ),
        tool(
            "list_replays",
            "List replay records by session or request. Defaults to the latest session.",
            object_schema(
                &[
                    (
                        "session_id",
                        "string",
                        "Optional Faro session id. Ignored when request_id is provided."
                    ),
                    ("request_id", "string", "Optional Faro request id."),
                    ("limit", "number", "Maximum rows to return, capped at 500.")
                ],
                &[]
            )
        ),
        tool(
            "get_replay",
            "Get one replay record and optionally its captured response body.",
            object_schema(
                &[
                    ("replay_id", "string", "Faro replay id."),
                    (
                        "include_body",
                        "boolean",
                        "Include stored replay response body text. Defaults to true."
                    )
                ],
                &["replay_id"]
            )
        ),
        tool(
            "list_websocket_frames",
            "List captured WebSocket frames for a session. Defaults to the latest session.",
            object_schema(
                &[
                    (
                        "session_id",
                        "string",
                        "Optional Faro session id. Omit to use the latest session."
                    ),
                    (
                        "direction",
                        "string",
                        "Optional direction: sent or received."
                    ),
                    ("opcode", "number", "Optional WebSocket opcode filter."),
                    ("limit", "number", "Maximum rows to return, capped at 1000.")
                ],
                &[]
            )
        ),
        tool(
            "list_console_errors",
            "List console error and fatal logs from a session. Defaults to the latest session.",
            object_schema(
                &[(
                    "session_id",
                    "string",
                    "Optional Faro session id. Omit to use the latest session."
                )],
                &[]
            )
        ),
        tool(
            "list_storage_items",
            "List current localStorage/sessionStorage items for a session. Useful for discovering auth/session keys.",
            object_schema(
                &[
                    (
                        "session_id",
                        "string",
                        "Optional Faro session id. Omit to use the latest session."
                    ),
                    (
                        "storage_type",
                        "string",
                        "Optional storage type: localStorage or sessionStorage."
                    ),
                    (
                        "key_contains",
                        "string",
                        "Optional case-insensitive substring filter for keys."
                    ),
                    ("limit", "number", "Maximum rows to return, capped at 1000.")
                ],
                &[]
            )
        ),
        tool(
            "get_storage_item",
            "Get current localStorage/sessionStorage values for a key.",
            object_schema(
                &[
                    (
                        "session_id",
                        "string",
                        "Optional Faro session id. Omit to use the latest session."
                    ),
                    ("storage_type", "string", "localStorage or sessionStorage."),
                    ("key", "string", "Storage key.")
                ],
                &["storage_type", "key"]
            )
        ),
        tool(
            "list_cookies",
            "List cookies from a session's latest cookie snapshot. Defaults to the latest session.",
            object_schema(
                &[(
                    "session_id",
                    "string",
                    "Optional Faro session id. Omit to use the latest session."
                )],
                &[]
            )
        ),
        tool(
            "copy_request_as_curl",
            "Build a shareable curl command for a captured request. Redacts sensitive headers and omits body by default.",
            object_schema(
                &[
                    ("request_id", "string", "Faro request id."),
                    (
                        "include_sensitive",
                        "boolean",
                        "Include credentials and request body. Requires --mcp-allow-sensitive."
                    )
                ],
                &["request_id"]
            )
        ),
        tool(
            "replay_request",
            "Replay a captured request with curl and persist the replay record.",
            object_schema(
                &[("request_id", "string", "Faro request id.")],
                &["request_id"]
            )
        ),
        tool(
            "evaluate_js",
            "Evaluate JavaScript through an attached CDP websocket URL returned by capture_url.",
            object_schema(
                &[
                    (
                        "websocket_url",
                        "string",
                        "CDP page websocket URL from a capture_url attached event."
                    ),
                    ("expression", "string", "JavaScript expression to evaluate.")
                ],
                &["websocket_url", "expression"]
            )
        ),
        tool(
            "reload_page",
            "Reload a page through an attached CDP websocket URL returned by capture_url.",
            object_schema(
                &[(
                    "websocket_url",
                    "string",
                    "CDP page websocket URL from a capture_url attached event."
                )],
                &["websocket_url"]
            )
        ),
        tool(
            "run_readonly_sql",
            "Run a read-only SQL query against the Faro SQLite database.",
            object_schema(&[("query", "string", "Read-only SQL query.")], &["query"])
        )
    ])
}

fn tool(name: &str, description: &str, input_schema: Value) -> Value {
    json!({
        "name": name,
        "description": description,
        "inputSchema": input_schema
    })
}

fn object_schema(properties: &[(&str, &str, &str)], required: &[&str]) -> Value {
    let mut props = Map::new();
    for (name, kind, description) in properties {
        props.insert(
            (*name).to_string(),
            json!({
                "type": kind,
                "description": description
            }),
        );
    }
    json!({
        "type": "object",
        "properties": props,
        "required": required
    })
}
