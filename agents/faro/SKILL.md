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

1. Capture a page before inspecting it:

```text
capture_url({ "url": "https://example.com", "duration": "15s" })
```

2. List requests, usually filtered by route or status:

```text
list_requests({ "route": "/api", "filter": "status >= 400", "limit": 50 })
```

3. Fetch details and bodies:

```text
get_request({ "request_id": "...", "include_body": true })
get_response_body({ "request_id": "..." })
```

4. Check client-side failures:

```text
list_console_errors({})
get_storage_item({ "storage_type": "localStorage", "key": "auth" })
list_cookies({})
```

5. Reproduce or share requests:

```text
copy_request_as_curl({ "request_id": "..." })
replay_request({ "request_id": "..." })
```

6. Use SQL for ad hoc analysis. SQL must be read-only:

```text
run_readonly_sql({ "query": "select id, method, url, status_code from requests where status_code >= 500" })
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
- Prefer `copy_request_as_curl` when sharing a reproduction with a human.
- Prefer `replay_request` only when the user explicitly wants to send the request again.
- Do not run mutating SQL. Faro rejects writes, but tools should still ask for read-only queries.
