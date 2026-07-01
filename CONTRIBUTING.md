# Contributing To Faro

Thanks for taking the time to improve Faro.

Faro is a Rust terminal app that captures browser debugging data into SQLite and exposes it through a TUI, CLI, SQL, and MCP. Contributions are welcome, especially around performance, browser capture reliability, agent workflows, documentation, and terminal UI polish.

## Development Setup

Prerequisites:

- Rust stable with edition 2024 support.
- A Chromium-family browser for manual capture testing.
- `curl` for replay features.
- Optional: `nvim`, `code --wait`, `zed --wait`, or another `$EDITOR` for editor handoff workflows.

Clone and run checks:

```sh
git clone https://github.com/nullslate/faro.git
cd faro
cargo fmt --check
cargo test
cargo clippy --all-targets --all-features -- -D warnings
rg -n "\.ok\(\)|\.unwrap\(\)|\.expect\(" crates
```

Run locally:

```sh
cargo run -- http://localhost:5173
cargo run -- capture http://localhost:5173 --for 10s --json
cargo run -- --db /tmp/faro.db mcp
```

## Pull Request Guidelines

- Keep changes focused and explain the user-visible behavior.
- Add tests for new behavior and regressions.
- Preserve existing style and architecture. Prefer extending current modules over creating parallel paths.
- Avoid `.unwrap()`, `.expect()`, and `.ok()` in `crates`.
- Do not include captured private data, auth headers, cookies, or real production payloads in fixtures, screenshots, or issues.
- Run `cargo fmt`, `cargo test`, and clippy before opening a PR.

## Performance Work

Faro can ingest large request streams, so performance changes should include a note about impact. The opt-in harnesses are:

```sh
scripts/perf-smoke.sh
cargo test large_session -- --ignored --nocapture
cargo test render_perf -- --ignored --nocapture
```

Treat harness numbers as regression signals, not portable benchmarks.

## Security And Privacy

Faro captures data that can include credentials, cookies, localStorage, request bodies, and response bodies. Please be careful with logs and examples. Report security issues privately using the process in [SECURITY.md](SECURITY.md).

## Code Of Conduct

This project follows the [Contributor Covenant Code of Conduct](CODE_OF_CONDUCT.md).
