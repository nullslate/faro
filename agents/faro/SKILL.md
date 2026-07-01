# Faro Browser Debugging

Use Faro when you need to capture, inspect, replay, or query browser debugging data for a web app.

## MCP Setup

Start the Faro MCP server over stdio:

```json
{
  "mcpServers": {
    "faro": {
      "command": "faro",
      "args": ["mcp"]
    }
  }
}
```

By default, MCP is read-only. Start Faro with `--mcp-allow-mutation` only when the user explicitly wants agents to launch captures, delete sessions, replay requests, reload pages, or evaluate JavaScript. Start with `--mcp-allow-sensitive` only when the user explicitly wants raw credentials/request bodies in generated curl commands.

Body-returning MCP tools may truncate large bodies according to `redaction.mcp_body_limit_bytes` in Faro config. Check returned truncation metadata before assuming the body is complete.

Use a specific database if needed:

```json
{
  "mcpServers": {
    "faro": {
      "command": "faro",
      "args": ["--db", "/path/to/faro.db", "mcp"]
    }
  }
}
```

## Workflow

1. Capture a page before inspecting it. Prefer having the user start Faro normally. Use `capture_url` only when MCP was started with `--mcp-allow-mutation`:

```text
capture_url({ "url": "https://example.com", "duration": "15s" })
```

2. List sessions and pick the right capture when more than one exists:

```text
list_sessions({})
```

Pass the selected `session_id` to list tools when possible.

3. List requests, usually filtered by route or status:

```text
list_requests({ "session_id": "...", "route": "/api", "filter": "status >= 400", "limit": 50 })
```

4. Fetch details and bodies:

```text
get_request({ "request_id": "...", "include_body": true })
get_response_body({ "request_id": "..." })
```

5. Check client-side failures:

```text
list_console_errors({ "session_id": "..." })
list_storage_items({ "session_id": "...", "storage_type": "localStorage", "key_contains": "auth" })
get_storage_item({ "session_id": "...", "storage_type": "localStorage", "key": "auth" })
list_cookies({ "session_id": "..." })
list_websocket_frames({ "session_id": "...", "direction": "received", "limit": 50 })
```

6. Reproduce or share requests:

```text
copy_request_as_curl({ "request_id": "..." })
replay_request({ "request_id": "..." })
list_replays({ "session_id": "..." })
get_replay({ "replay_id": "...", "include_body": true })
```

`copy_request_as_curl` is redacted by default. Use `include_sensitive: true` only when MCP was started with `--mcp-allow-sensitive` and the user explicitly needs raw credentials/body. `replay_request` requires `--mcp-allow-mutation`.

7. Use SQL for ad hoc analysis. SQL must be read-only:

```text
run_readonly_sql({ "query": "select id, method, url, status_code from requests where status_code >= 500" })
```

8. When `capture_url` returns an attached `websocket_url`, live page actions are available only if MCP was started with `--mcp-allow-mutation`:

```text
evaluate_js({ "websocket_url": "...", "expression": "document.title" })
reload_page({ "websocket_url": "..." })
```

## CLI Fallback

If MCP is unavailable, use the CLI:

```bash
faro capture https://example.com --for 15s --json
faro requests --route /api --filter "status >= 400" --json
faro request get <request-id> --body --json
faro request curl <request-id>
faro console errors --json
faro storage get localStorage auth --json
faro cookies list --json
faro replay <request-id> --json
faro sql "select * from requests where status_code >= 500" --json
```

## Notes

- Route filters accept plain prefixes like `/api/users`, one-segment params like `/api/users/:id`, and wildcards like `/api/*`.
- Prefer `copy_request_as_curl` when sharing a reproduction with a human; it is redacted by default.
- Prefer `replay_request` only when the user explicitly wants to send the request again and MCP mutation is enabled.
- Do not run mutating SQL. Faro rejects writes, but tools should still ask for read-only queries.
- Mutating/browser MCP tools require `--mcp-allow-mutation`; sensitive curl output requires `--mcp-allow-sensitive`.
- Security-relevant MCP/TUI actions are appended to Faro's `audit.jsonl` in the config directory.
- Redaction rules are configurable in `config.toml` under `[redaction]`.
