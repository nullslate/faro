# Security Policy

Faro captures browser debugging data, which may include sensitive headers, cookies, localStorage/sessionStorage values, request bodies, response bodies, and replay artifacts.

## Supported Versions

Faro is currently pre-1.0. Security fixes are made on `main` unless release branches are introduced later.

## Reporting A Vulnerability

Please do not open a public issue for a security vulnerability.

Report security issues privately by emailing the maintainer or by using GitHub private vulnerability reporting if it is enabled on the repository.

Include:

- A description of the issue and affected workflow.
- Steps to reproduce with sanitized data.
- Whether credentials, browser state, filesystem paths, or captured payloads can be exposed.
- Any suggested fix or mitigation.

Please avoid sending real cookies, tokens, auth headers, private URLs, or production payloads.

## Security Defaults

- MCP mutating/browser-driving tools require `--mcp-allow-mutation`.
- Sensitive curl export requires `--mcp-allow-sensitive` and an explicit tool argument.
- Share bundles redact common sensitive headers and JSON/text patterns.
- Body-returning MCP tools apply size caps.
- Security-relevant MCP actions are written to `audit.jsonl` in the Faro config directory.

## Local Data

By default, Faro stores captured data in the platform config directory:

- Linux: `~/.config/faro`
- macOS: `~/Library/Application Support/faro`
- Windows: `%APPDATA%\faro`

Delete old sessions with:

```sh
faro sessions nuke --yes --vacuum
```
