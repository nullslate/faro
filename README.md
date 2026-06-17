# Devbench TUI

Terminal-first frontend development workbench.

Devbench TUI does not embed Chromium and does not render web pages. It launches or
attaches to an external Chromium-based browser through the Chrome DevTools Protocol
and persists captured observations into SQLite.

```text
Browser -> CDP -> Capture -> SQLite -> Ratatui
```

The archived CEF/GTK prototype lives at `../devbench`.

## Run

```sh
cargo run -- http://localhost:5173
cargo run -- --db /tmp/devbench.db http://localhost:5173
cargo run -- --cdp-port 9223 http://localhost:5173
cargo run -- --attach-port 9222 http://localhost:5173
cargo run -- tui /tmp/devbench.db
```

Default URL mode opens the TUI without launching a browser. Press `o` to open a
Chromium-family browser and start CDP capture. Use `--launch-on-start` if you
want eager launch. Use `--cdp-port` only when you need a stable launch port. Use
`--attach-port` with an already-running browser, for example:

```sh
chromium --remote-debugging-port=9222 --user-data-dir=/tmp/devbench-profile
cargo run -- --attach-port 9222 http://localhost:5173
```

Keys:

```text
q/esc   quit
tab     switch focus between Requests, Detail, Body, Console, Storage, and Cookies
1-4     switch views: Network, Console, Storage, Cookies
j/k     move focused selection
h/l     switch request detail tab
m       toggle persisted focused-pane layout
ctrl+arrows resize persisted Network splits
p       open command palette
o       open browser and start capture
y       copy selected request as curl
w       save selected request/response exchange to /tmp
r       replay selected request with curl, persist the replay, and write output to /tmp
R       edit selected request in $EDITOR, then replay and persist it
D       diff original response body against latest replay response body
s/S     cycle request sort / toggle sort direction
f       cycle quick network filter preset
e       open selected body in $EDITOR, falling back to nvim
u/d     scroll focused detail/body pane
g/G     jump to top/bottom in focused pane
/       filter requests
?       show floating key/filter help
c       clear request filter
```

In the Console view (`2`), `e` opens a JavaScript scratch file in `$EDITOR`
and evaluates it in the inspected page through CDP. The result is persisted back
into the Console log.

Request filters support plain text plus structured tokens:

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
```

Plain text terms search across method, URL, type, status, MIME, headers, and
captured request/response bodies. Plain terms and structured values also accept
case-insensitive regex patterns:

```text
api/(users|teams)
path:/api/v[0-9]+
method:^(post|put)$
/graphql|rest/
```

Visible request-list matches are highlighted. Press `f` to cycle quick presets:
all, errors, JSON, fetch, XHR, SSE, images, scripts, styles, documents, with
body, slow, large, and replayed.

SSE responses (`text/event-stream`) are shown as parsed event entries in the
response body panes.

Image responses can be previewed inline when small image bodies are captured and
the terminal supports Kitty or iTerm image protocols. Other terminals show image
metadata and capture status instead.

The default view is Network. Console, Storage, and Cookies are available as
dedicated full-screen views instead of always consuming Network space.

Request detail tabs include overview, query params, request/response headers,
request/response bodies, timing, and replay history.

Current vertical slice:

1. Launches an external Chromium-family browser.
2. Connects through CDP on a local debugging port.
3. Enables Network and reloads the page for capture.
4. Persists requests, request bodies, responses, bounded text response bodies, console logs, live storage events, live cookie events, and reconciliation snapshots.
5. Persists replay attempts with exit/status/body metadata.
6. Displays captured requests, request detail, response body, replay status, console output, storage, and cookies in Ratatui.

Set `DEVBENCH_BROWSER=/path/to/chrome-or-chromium` to override browser discovery.

Current limitations:

- Storage mutation tracking is CDP DOMStorage-based; snapshots are only baseline/reconciliation.
- Cookie mutation tracking uses HTTP `Set-Cookie` observation plus a source-free `document.cookie` page agent.

## Current Workspace

- `devbench-core`: retained domain models.
- `devbench-store`: retained SQLite event store and projections.
- `devbench-capture`: retained source-neutral ingestion pipeline.
- `devbench-cdp`: new CDP/browser control plane.
- `devbench`: CLI/TUI bootstrap.
