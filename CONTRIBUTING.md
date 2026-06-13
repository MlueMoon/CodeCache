# Contributing to CodeCache

CodeCache is built **test-driven (TDD)** by a coordinated team of Claude Code agents with
human oversight. This document covers the development workflow, quality gates, and conventions
that all contributors — human or agent — must follow.

For the full architecture and product spec see [`docs/project_plan.md`](docs/project_plan.md).
For the team, module ownership, and build order see
[`docs/ENGINEERING_PLAN.md`](docs/ENGINEERING_PLAN.md).

---

## TDD Workflow (non-negotiable)

Every production line of code exists to satisfy a **previously failing test**. The cycle is:

1. **RED** — write a failing test that expresses the desired behavior. Commit or at least run
   it to confirm it fails for the right reason.
2. **GREEN** — write the minimum idiomatic Rust to make the test pass. No gold-plating.
3. **REFACTOR** — clean up while keeping the tests green.
4. **REVIEW** — an independent reviewer (human or the `code-reviewer` agent) gates the slice
   before it is merged. APPROVE or BLOCK with specific findings.
5. **INTEGRATE** — merge only after APPROVE; update `docs/TODO.md` in the same change.

**Never weaken or delete a test to make it pass.** If a test is wrong, fix the test _and_
record why. If a budget cannot be a hard assert (machine variance), record the number and track
it via the scheduled bench workflow — do not simply remove the assertion.

---

## Quality Gates

All four of the following must be clean before a slice is considered done. CI enforces the same
commands; "green locally" means "green in CI."

```bash
# Gate 1 — formatting (CI runs --check; locally cargo fmt rewrites in place)
cargo fmt --all

# Gate 2 — linting (zero warnings, all targets including benches and examples)
cargo clippy --all-targets -- -D warnings

# Gate 3 — tests
cargo test --all

# Gate 4 — build
cargo build
```

CI (`.github/workflows/ci.yml`) runs gates 1–3 on push and PR, on a matrix of ubuntu/macOS/
Windows, with the toolchain pinned by `rust-toolchain.toml`. Local hooks
(`.claude/hooks/check-on-stop.ps1`, `fmt-on-edit.ps1`) run the same commands so there are no
CI-only surprises.

---

## Minimum Supported Rust Version (MSRV)

**Rust 1.85.0** — pinned in `rust-toolchain.toml`. Do not use features introduced after 1.85.
When upgrading the MSRV, bump both `rust-toolchain.toml` and `Cargo.toml`'s `rust-version`
field, and update this document in the same change.

---

## Code Rules

- **No reachable `unwrap()`, `expect()`, or `panic!`** in any shipped runtime code (library or
  binary). Use `?` + `anyhow::Result` / typed errors. `expect` is acceptable in test and bench
  harness code where a panic would surface as a test failure.
- Match the documented APIs in `docs/project_plan.md` §3.2. If you need to diverge, update the
  plan _first_, log the decision in `docs/ROADMAP.md` (Decision Log), then implement.
- No new dependency without explicit manager sign-off recorded in the Decision Log. The approved
  dependency set is in `Cargo.toml` + `docs/project_plan.md` §10.3.
- Keep `Cargo.toml` lean. Dev-dependencies (test/bench only) are in `[dev-dependencies]`.

---

## Running Benchmarks

```bash
# Run all criterion benchmarks (may take a few minutes — 100K-LOC index bench is slow)
cargo bench

# Run a specific benchmark
cargo bench --bench indexing
cargo bench --bench query_bench
cargo bench --bench hashing_bench
```

Performance budgets are recorded in `benches/CLAUDE.md`. The scheduled CI job
(`.github/workflows/bench.yml`, weekly) uploads criterion results for trend tracking. Two
budgets have hard in-code asserts (index size < 100 MB; hash-1K < 500 ms); machine-variable
timing benches are tracked-not-asserted per Decision Log D20.

The `/bench` skill (`.claude/skills/bench/`) documents the full bench workflow for agent use.

---

## Module Ownership

Each module has a `CLAUDE.md` co-located with its code (e.g. `src/storage/CLAUDE.md`). These
are the canonical docs for the module's public API, test coverage, and status. Update the
relevant `CLAUDE.md` in every change that touches that module.

The agent team is documented in `.claude/agents/`. The orchestration manual is
`.claude/CLAUDE.md`.

---

## Commit Style

- One commit per slice (`M<milestone>.<slice>: <imperative summary>`).
- Update `docs/TODO.md` and the relevant module `CLAUDE.md` in the same commit.
- All four quality gates must be green before the commit lands.

---

## License

By contributing you agree that your contributions will be dual-licensed under
[MIT](LICENSE-MIT) and [Apache-2.0](LICENSE-APACHE), consistent with the existing codebase.
