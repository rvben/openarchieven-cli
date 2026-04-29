# CLAUDE.md

Guidance for Claude Code working in this repository.

## Build & Test

```bash
make check     # lint + test (CI runs this)
make build     # cargo build --release
make test      # cargo nextest run (or cargo test fallback)
make lint      # fmt --check + clippy -D warnings
make fmt       # auto-format
make install   # release build → ~/.local/bin/openarchieven
```

## Architecture

Binary `openarchieven` wraps the openarchieven.nl API per the design spec at
`docs/superpowers/specs/2026-04-29-openarchieven-cli-design.md`. Core flow:
`main.rs` parses args → command module → `Client::get()` (cache + rate limit
+ retry) → response shaped into `Renderable` → renderer (json/table/markdown)
→ stdout. Errors always go to stderr as JSON.

## Modules

- `client.rs`: HTTP, rate limiter, retry/backoff, `Retry-After` parsing.
- `cache.rs`: on-disk JSON cache with atomic writes and advisory locks.
- `error.rs`: `ErrorKind` enum and structured stderr emission.
- `output.rs`: `Renderable` shapes and three renderers.
- `schema_cmd.rs`: emits the machine-readable schema contract.
- `commands/<name>.rs`: one module per API endpoint.

## Conventions

- TDD throughout. Test failure paths explicitly.
- `cargo nextest run` preferred (fast, token-optimized via RTK).
- No live network in unit/integration tests — `wiremock` stands in.
- Schema output is byte-stable; `tests/schema.rs` snapshots it.
