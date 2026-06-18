# .github/ — CLAUDE.md

CI/CD workflows. **Owner agent:** `devops-release-engineer`. CI must mirror the local quality
gates exactly (`../docs/ENGINEERING_PLAN.md` §5).

## Layout
| Path | Role | Milestone |
|---|---|---|
| `workflows/ci.yml` | fmt-check → clippy `-D warnings` → `cargo test --all`, with cargo caching. | M0 (landed) |
| `workflows/bench.yml` | Scheduled criterion runs (weekly cron + `workflow_dispatch`; trend-not-gate). | M10.3 (landed) |
| `workflows/release.yml` | tag-triggered: fmt/clippy/test gates + smoke test (matrix: ubuntu/macos/windows) → cargo publish → platform binary assets. | M10.4 (authored — STAGED, not triggered; human pushes v0.1.0 tag to activate) |

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

## release.yml — trigger model + policy (M10.4)
- **Trigger:** `on.push.tags: ["v*"]` ONLY. NOT on push/PR. Tag push is the human go-ahead signal.
- **Jobs (in order):** `install-smoke-test` (matrix: ubuntu/macos/windows — fmt + clippy + test + build --release + init→index→query fixture) → `publish` (ubuntu; `cargo publish` gated on `CARGO_REGISTRY_TOKEN` secret) + `release-binaries` (matrix: upload platform binary to GitHub Release via `softprops/action-gh-release@v2`). Publish and release-binaries both `needs: install-smoke-test`; a broken binary never reaches crates.io.
- **Security:** `CARGO_REGISTRY_TOKEN` is a repository secret (set in Settings → Secrets → Actions). Binary upload uses the auto-provisioned `GITHUB_TOKEN` with `contents: write`. No plaintext secrets.
- **Repository URL — SET 2026-06-17 (was placeholder):** `Cargo.toml repository = "https://github.com/AdvancedUno/codecache"`, confirmed final by the human (blocker #2 resolved). crates.io records this permanently per version.
- **Name conflict — RESOLVED 2026-06-17 (D30):** `codecache` was taken on crates.io, so the package was renamed `codecache-rs` (the binary stays `codecache`). `cargo publish --dry-run` is green under the new name.

## Status
M0: `ci.yml` landed (single `gates` job, three steps).
M10.3: `bench.yml` landed (scheduled weekly trend-tracker).
M10.4: `release.yml` authored + dry-run verified. STAGED — awaiting human go-ahead (tag push).
