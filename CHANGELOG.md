# Changelog

All notable changes to this project will be documented in this file.

## [Unreleased]

## [0.1.0] - 2026-04-30

Initial release.

### Added
- All 17 endpoints of the openarchieven.nl API v1.1: `archives`, `search`,
  `show`, `match`, `births`, `deaths`, `marriages`, `yearsago`, `census`,
  `weather`, and the seven `stats` subcommands.
- Three output formats (`json`, `table`, `markdown`) selected automatically by
  TTY/pipe context, overridable with `--output/-o`.
- `schema` subcommand emits a byte-stable, machine-readable contract for
  scripts and AI agents.
- On-disk response cache with advisory locking and per-endpoint TTLs;
  `cache info`, `cache prune`, and `cache clear --yes` for management.
- Built-in rate limiter (4 req/sec default) and retry with jittered backoff
  that respects `Retry-After`.
- Structured JSON error contract on stderr with stable `kind` enum.
- `--fields` projection for list and single-flat responses.
- Distribution: crates.io, PyPI wheel, and Homebrew tap, all published from
  one tag-driven release workflow.
