# src/cli/ — CLAUDE.md

**Module:** `cli` · **Owner:** `principal-engineering-lead` · **Milestone:** M7 (stub at M0).

## Purpose
`clap`-based argument parsing and command dispatch: `init`, `index`, `update`, `query`,
`status`, `config`, `serve`. User-facing errors with helpful messages + nonzero exit.

## API anchor
`docs/project_plan.md` §7 (command structure + per-command specs).

## Shipped surface (M7.2 — parsing)
`clap` derive in `mod.rs`: `Cli` (global `-v/--verbose`, `-V/--version` from `CARGO_PKG_VERSION`,
`-h/--help`) + a `Command` subcommand enum mirroring §7.1–§7.2 EXACTLY:
- `init` — `--db-path` [default `.codecache/index.db`], `--index-path` (multi, default `.`),
  `--ignore` (multi), `--languages` (comma-delimited, default `python,typescript,go`)
- `index` — `--full`, `--db-path`, `--progress`
- `update <FILE>...` (required positional) — `--db-path`
- `query <QUERY>` — `--max-tokens 4000`, `--max-results 20`, `--format` toon|json|text [text],
  `--file-filter`, `--db-path`
- `status` — `--db-path`
- `config` — positional `KEY [VALUE]` + `--db-path` (minimal/forward-compatible; read/write
  semantics land in M7.3)
- `serve` — `--transport` stdio|sse [stdio], `--port 3000`, `--db-path`

Two clap `ValueEnum`s: `OutputFormat` (toon|json|text) and `Transport` (stdio|sse), so out-of-set
values produce clap's own nonzero parse error. `From<OutputFormat> for formatter::Format` is the
seam keeping clap concerns inside `cli`. All `--db-path` share `DEFAULT_DB_PATH`.

`run()` → `Cli::parse()` then `dispatch()`; errors return `anyhow::Result` (Err → nonzero exit via
`main`). No reachable `unwrap()/expect()/panic!`. **Handlers are inert M7.3 placeholders** at this
slice — real delegation to `app`/`Indexer`/`Retriever`/`Config`/`Storage` lands in M7.3; `serve` is
an M8 stub.

## Tests / scenarios
`tests/cli_tests.rs` (5 tests via `assert_cmd`/`predicates`, D17): documented-flag parsing,
query defaults, help/version, bad-args → nonzero, unknown-command → nonzero.
`docs/TEST_STRATEGY.md#cli` — E2E `init → index → query` through the built binary (M7.4).

## Status
M7.2 DONE (2026-06-12): clap parsing + error/exit-code mapping shipped + green; reviewer APPROVED.
Command handlers + `status` aggregates → M7.3; binary E2E → M7.4.
