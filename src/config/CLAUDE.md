# src/config/ — CLAUDE.md

**Module:** `config` · **Owner:** `principal-engineering-lead` · **Milestone:** M1 (stub at M0).

## Purpose
Load and validate `.codecache/config.toml`: index paths, ignore patterns, language settings,
storage/retrieval/MCP sections; apply defaults for omitted fields.

## API anchor
`docs/project_plan.md` §7.3 (config schema).

## Tests / scenarios
`docs/TEST_STRATEGY.md#config` — valid TOML loads; defaults applied; invalid/missing → clear
error; ignore-pattern parsing.

## Shipped API (M1)
- `Config` (+ `StorageConfig`/`RetrievalConfig`/`McpConfig`) mirroring §7.3 keys.
- `Config::load(&Path) -> Result<Config, ConfigError>` — reads + parses TOML, applies documented
  defaults for omitted fields (`max_tokens=4000`, `max_results=20`, `bm25_k1=1.2`, `bm25_b=0.75`,
  `languages=[python,typescript,go]`, `db_path=.codecache/index.db`, `max_db_size_mb=500`,
  `transport=stdio`, `sse_port=3000`) via `#[serde(default = ...)]` + section `Default` impls.
- `ConfigError::{Io, Parse}` — typed (impl `std::error::Error`); missing/unreadable → `Io`,
  malformed TOML → `Parse`. No `unwrap`/`expect`/`panic`.

## Status
**M1: DONE (2026-06-10).** All four gates green on Rust 1.85.0.
