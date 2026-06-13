# .github/ — CLAUDE.md

CI/CD workflows. **Owner agent:** `devops-release-engineer`. CI must mirror the local quality
gates exactly (`../docs/ENGINEERING_PLAN.md` §5).

## Layout
| Path | Role | Milestone |
|---|---|---|
| `workflows/ci.yml` | fmt-check → clippy `-D warnings` → `cargo test --all`, with cargo caching. | M0 (landed) |
| `workflows/bench.yml` | Scheduled criterion runs (weekly cron + `workflow_dispatch`; trend-not-gate). | M10.3 (landed) |
| `workflows/release.yml` | version bump, `v0.1.0` tag, crates.io publish, install smoke test. | M10.4 (pending) |

## CI parity contract (ENGINEERING_PLAN §5)
The `ci.yml` steps must use the **same flags** as the local hooks (`.claude/hooks/*.ps1`):
| Gate | Local hook | CI step |
|---|---|---|
| Format | `cargo fmt` on `.rs` edit | `cargo fmt --all -- --check` |
| Lint | `cargo clippy --all-targets -- -D warnings` at Stop | same |
| Tests | `cargo test` at Stop | `cargo test --all` |

Toolchain is pinned by `../rust-toolchain.toml` (1.85.0) so local == CI; bump them in lockstep.
Caching is mandatory — `rusqlite` `bundled` + tree-sitter grammars compile C (slow cold build).

## Rules
- When local hooks change, update `ci.yml` in the **same** change to keep gates identical
  (`../.claude/CLAUDE.md` conventions).
- Keep the toolchain channel here, `rust-toolchain.toml`, and any hook references in sync.

## bench.yml — trigger model + policy (M10.3)
- **Triggers:** `schedule` (cron `0 2 * * 1` — weekly Monday 02:00 UTC) + `workflow_dispatch`. NOT on push/pull_request (benches are slow and machine-variable; per-PR runs add noise without actionable signal).
- **OS:** `ubuntu-latest` only (single-OS trend tracking; cross-OS bench noise is not worth the extra CI minutes).
- **Caching:** identical key pattern to `ci.yml` (`Cargo.lock` + `rust-toolchain.toml`); mandatory because `rusqlite bundled` + tree-sitter grammars compile C from source.
- **Timeout:** 60 minutes (the 100K-LOC cold-index bench takes ~13–14 s per sample × 10 samples; 60 min is a generous ceiling for all three benches).
- **Trend-not-gate policy (Decision Log D20):** the two in-code hard asserts (index-size < 100MB in `indexing.rs`; hash-1K < 500ms in `hashing_bench.rs`) fail the job naturally if their budgets regress — that is correct and desired. All machine-variable timing benches (cold-index, query latency, incremental) are recorded via the uploaded `target/criterion/` artifact and are NOT asserted in CI. In particular, the cold-10K-LOC budget (< 5s, currently 6.04s p50 — D20 miss) is tracked-not-asserted so it does not permanently break the scheduled job.
- **Artifact:** `criterion-results-<run_id>` uploaded with `actions/upload-artifact@v4`, 90-day retention. Inspect `target/criterion/*/new/sample.json` across runs to track p95/p99 trends (criterion does not print them to stdout).

## Status
M0: `ci.yml` landed (single `gates` job, three steps).
M10.3: `bench.yml` landed (scheduled weekly trend-tracker; `release.yml` still pending at M10.4).
