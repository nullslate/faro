# Devbench

Devbench is a terminal-first browser debugging workbench for frontend and full-stack development.

It launches or attaches to a Chromium-family browser through the Chrome DevTools Protocol, captures browser observations into SQLite, and lets humans and agents inspect the result through a TUI, CLI, SQL, or MCP.

```text
Browser -> Chrome DevTools Protocol -> Devbench capture -> SQLite -> TUI / CLI / MCP
```

Devbench does not embed Chromium and does not render web pages in the terminal. It controls a real browser and keeps a durable local debugging database.

## Features

- Network request tree with route drilldown, filters, presets, response details, and replay history.
- Captured request/response headers, query params, request bodies, bounded text response bodies, SSE parsing, and small image previews where the terminal supports inline images.
- Console logs, page errors, and JavaScript scratch evaluation through `$EDITOR`.
- Storage and cookies views with captured snapshots and live mutation events.
- Copy any captured request as a full `curl` command.
- Replay captured requests with `curl` and persist replay status/body metadata.
- Read-only SQL editor in the TUI, plus CLI/MCP read-only SQL for agents.
- Agent-friendly CLI and MCP server for capture, inspection, replay, and SQL.
- TOML config in `~/.config/devbench/config.toml` or `$XDG_CONFIG_HOME/devbench/config.toml`.

## Quick Start

### Install From Source

Prerequisites:

- Rust stable, edition 2024 capable.
- A Chromium-family browser: Chromium, Chrome, Brave, etc.
- `curl` for request replay.
- Optional: `nvim` or another `$EDITOR` for body editing, SQL, and console evaluation.

```sh
git clone <repo-url> devbench
cd devbench
cargo install --path crates/app
```

Run the TUI against a local app:

```sh
devbench http://localhost:5173
```

Press `o` to launch the browser and start capture. Use eager launch when you want capture to start immediately:

```sh
devbench --launch-on-start http://localhost:5173
```

Open a previously captured database without launching a browser:

```sh
devbench tui ~/.config/devbench/devbench.db
```

### Run Without Installing

```sh
cargo run -- http://localhost:5173
cargo run -- --launch-on-start http://localhost:5173
```

### Attach To An Existing Browser

```sh
chromium --remote-debugging-port=9222 --user-data-dir=/tmp/devbench-profile
devbench --attach-port 9222 http://localhost:5173
```

Use `DEVBENCH_BROWSER=/path/to/chrome-or-chromium` to override browser discovery.

### Editor Handoff

Devbench opens `$EDITOR` for SQL, console evaluation, body viewing/editing, storage edits, cookie edits, and replay editing. Terminal editors usually work as-is:

```sh
export EDITOR=nvim
```

GUI editors should be configured to wait until the file is closed:

```sh
export EDITOR="code --wait"
export EDITOR="zed --wait"
```

Without a wait flag, GUI editors may return immediately and Devbench will resume before the file is saved.

## TUI Basics

Common keys:

```text
q/esc   quit
tab     switch focus
1-4     switch views: Network, Console, Storage, Cookies
j/k     move focused selection
enter   drill into a route / expand selected tree item
backspace go up one route level
h/l     switch request detail tab
p       open command palette
o       open browser and start capture
y       copy selected request as curl
w       save selected request/response exchange to /tmp
r       replay selected request with curl
R       edit selected request in $EDITOR, then replay
D       diff original response body against latest replay response body
s/S     cycle request sort / toggle sort direction
f       cycle quick network filter preset
e       open selected item in $EDITOR
u/d     scroll focused detail/body pane
g/G     jump to top/bottom in focused pane
/       filter requests
?       floating key/filter help
c       clear request filter
```

Request filters support plain text, structured fields, and case-insensitive regex patterns:

```text
method:post
status:2xx
status:404
type:fetch
domain:localhost
url:/api/users
path:/api
mime:json
header:x-request-id
body:error
reqbody:email
resbody:database
has:body
has:error
has:replay
duration:>500
size:>100kb
api/(users|teams)
path:/api/v[0-9]+
method:^(post|put)$
```

Quick presets include all, errors, JSON, fetch, XHR, SSE, images, scripts, styles, documents, with body, slow, large, and replayed.

## CLI

The CLI is designed for humans and agents that want to inspect Devbench without opening the TUI.

Capture a page without the TUI:

```sh
devbench capture https://example.com --for 15s --json
```

Inspect captured requests:

```sh
devbench requests --route /api --filter "status >= 400" --json
devbench request get <request-id> --body --json
devbench request curl <request-id>
```

Inspect browser state:

```sh
devbench console errors --json
devbench storage get localStorage auth --json
devbench cookies list --json
```

Replay and query:

```sh
devbench replay <request-id> --json
devbench sql "select * from requests where status_code >= 500" --json
```

Route filters accept:

- `/api/users`: exact route and descendants.
- `/api/users/:id`: one dynamic path segment.
- `/api/*`: wildcard for the rest of the path.

Use `--db <path>` with any command to target a specific SQLite database.

## MCP And Agent Integration

Devbench includes a stdio MCP server:

```sh
devbench mcp
```

Example MCP config:

```json
{
  "mcpServers": {
    "devbench": {
      "command": "devbench",
      "args": ["mcp"]
    }
  }
}
```

Use a specific database:

```json
{
  "mcpServers": {
    "devbench": {
      "command": "devbench",
      "args": ["--db", "/path/to/devbench.db", "mcp"]
    }
  }
}
```

Available MCP tools:

- `capture_url`
- `list_requests`
- `get_request`
- `get_response_body`
- `list_console_errors`
- `get_storage_item`
- `list_cookies`
- `copy_request_as_curl`
- `replay_request`
- `run_readonly_sql`

The importable agent package lives in:

```text
agents/devbench/
```

It contains:

- `SKILL.md`: workflow instructions for agent tools.
- `mcp.json`: ready-to-copy MCP server config.

## Configuration

On first run, Devbench creates:

```text
$XDG_CONFIG_HOME/devbench/config.toml
```

or:

```text
~/.config/devbench/config.toml
```

The default database path is `devbench.db` relative to the config directory, so the default DB is usually:

```text
~/.config/devbench/devbench.db
```

Important config fields:

```toml
[app]
db_path = "devbench.db"
launch_on_start = false

[ui]
bottom_fade_rows = 3

[theme]
text = "#d4be98"
muted = "#928374"
accent = "#89b482"
panel_title = "#d8a657"
panel_border = "#3c3836"
active_border = "#89b482"
```

The default theme is Gruvbox-inspired and can be customized with hex colors or supported terminal color names.

## Architecture

Workspace crates:

- `devbench-core`: domain models and event types.
- `devbench-store`: SQLite event store, projections, and read-only SQL guardrails.
- `devbench-capture`: source-neutral ingestion pipeline.
- `devbench-cdp`: Chrome DevTools Protocol capture/control plane.
- `devbench`: CLI, TUI, and MCP entrypoint.

Captured data is persisted in SQLite so the TUI, CLI, SQL, and MCP all inspect the same source of truth.

## Development

Run the standard checks:

```sh
cargo fmt --check
cargo test
cargo clippy --all-targets --all-features -- -D warnings
rg -n "\.ok\(\)|\.unwrap\(\)|\.expect\(" crates
```

Run locally:

```sh
cargo run -- http://localhost:5173
cargo run -- capture http://localhost:5173 --for 10s --json
cargo run -- --db /tmp/devbench.db mcp
```

## Current Limitations

- Devbench currently targets Chromium-family browsers through CDP.
- Response body capture is bounded to avoid unbounded database growth.
- Storage mutation tracking is CDP DOMStorage-based; snapshots are used for baseline and reconciliation.
- Cookie mutation tracking uses HTTP `Set-Cookie` observation plus a page-side `document.cookie` observer.
- MCP support is intentionally narrow and DB-first; live page evaluation is still CLI/TUI-oriented.

## License

MIT
