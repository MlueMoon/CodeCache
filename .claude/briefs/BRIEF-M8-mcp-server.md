# BRIEF — M8 — mcp_server / M8.0 (D15 entry evaluation)

- **Milestone:** M8 — mcp_server  ·  **Module(s):** mcp_server, cli/serve
- **Owner (manager):** principal-engineering-manager  ·  **Created:** 2026-06-12
- **Status:** D15 EVAL ✓  ·  DEP DECISION: **RESOLVED (2026-06-12) — HAND-ROLL JSON-RPC over stdio; serde/serde_json only, no new runtime dep. Human-ratified.**  ·  M8.1 RED ✓ GREEN ✓ REVIEW ✓ DONE ✓ (149) · M8.2 RED ✓ GREEN ✓ REVIEW ✓ DONE ✓ (154 tests green) · M8.3 RED ▢
- **Links:** docs/ROADMAP.md#m8--mcp_server (D8/D13/D14/D15) · docs/plans/M8-mcp-server.md · docs/project_plan.md §8, §10.2, §10.3 · project_overview.md §2.5

## Goal
Expose CodeCache as an MCP server over stdio JSON-RPC with three tools (`codecache_search`,
`codecache_update`, `codecache_outline` — D13) plus self-healing search (D14), wired to
`codecache serve`. **This slice (M8.0) is the D15 entry gate ONLY:** decide `rmcp` vs hand-rolled
JSON-RPC and pin the choice before any RED/GREEN. No code lands this turn.

---
## M8.0 — D15 evaluation (rmcp vs hand-rolled JSON-RPC over stdio)

### Facts on `rmcp` as it stands (verified 2026-06-12)
- **Version / maturity:** latest `1.7.0` (2026-05-13). Post-1.0 (1.0.0 = 2026-03-03), biweekly minor
  cadence, ~12.4M total downloads, officially maintained by the modelcontextprotocol org. MIT/Apache-2.0.
- **Protocol:** targets MCP spec `2025-11-25`.
- **MSRV vs our 1.85.0 pin — the decisive friction.** rmcp declares **no `rust-version` (MSRV)**
  field, but uses **`edition = "2024"`**, so Rust **1.85 is the absolute hard floor** (edition 2024
  stabilized in 1.85). Secondary signals point *above* 1.85: the repo's `rust-toolchain.toml` pins
  **1.92**, DeepWiki's prerequisites state **1.90 minimum**, docs.rs built 1.7.0 on a 1.97 nightly.
  Net: **1.85.0 is unverified and probably too old**; with no declared MSRV and an active 1.92 dev
  toolchain, a `cargo update` can pull rmcp (or its deps) past 1.85 at any time — directly fighting
  our deliberate D10 MSRV contract.
- **Async runtime:** **tokio-required, async-only.** `tokio ^1` is a hard (non-optional) dep
  (`sync,macros,rt,time`); `serve()` is `async`, tool methods are `async fn`, transports are
  `AsyncRead`/`AsyncWrite`. Server must run under a tokio runtime.
- **stdio transport:** built-in via `transport-io` feature (`stdio()` → (stdin,stdout) pair).
  One-line server stand-up: `let svc = MyService::new().serve(stdio()).await?; svc.waiting().await?;`
- **Tool registration:** attribute macros `#[tool_router]` / `#[tool(description=...)]` /
  `#[tool_handler]`; **inputSchema is derived from a param struct via `schemars::JsonSchema`** (not
  hand-written serde_json). Hand-written schemas are possible but off the blessed path.
- **Dep weight (minimal stdio server `["server","transport-io"(,"macros")]`):** floor is
  **tokio + serde(+derive) + serde_json + async-trait + schemars 1.0 + pastey + rmcp-macros
  (syn/quote/proc-macro2) + base64**. hyper/axum/reqwest/tower/uuid are optional (NOT pulled for
  stdio). Estimated **~40–70 transitive crates** (not exactly verified — needs `cargo tree`).
  Cannot drop tokio; cannot drop schemars if using the macros.
- **Health:** active (last commit 2026-05-13), ~36 open issues / ~12 PRs, maintained 1.x migration guide.

### Hand-rolled JSON-RPC over stdio (the incumbent §10.2 plan)
- **Deps:** `serde` + `serde_json` only — **already in the tree**. Zero new runtime crates.
- **Scope is genuinely modest:** stdio + JSON-RPC 2.0 framing + `initialize` handshake + `tools/list`
  + `tools/call` for exactly 3 tools + strict error mapping (-32700/-32601/-32602). No SSE/HTTP (D4
  deferred). Estimated ~250–450 LOC of framing/dispatch under `mcp_server/`, fully under test.
- **Sync Storage fits natively:** D8's `Arc<Mutex<Connection>>` `Storage` is synchronous; a blocking
  stdin read-loop calls `Retriever`/`Indexer` directly with no runtime bridging. No tokio, no
  `block_on`, no async colouring of our otherwise-sync codebase.
- **Cost:** we own protocol-version drift (must hand-track MCP spec changes) and write/maintain the
  framing + schema JSON ourselves. Risk is bounded — the v0.1 surface is frozen at 3 tools + stdio.

### Sync/async Storage boundary (D8) under each path
- **Hand-roll:** no boundary — blocking loop → sync `Storage`/`Retriever`/`Indexer`. Clean.
- **rmcp:** forces a tokio runtime over a **synchronous** `Arc<Mutex<Connection>>`. Each async tool
  handler must call sync, blocking SQLite work — correct usage requires `spawn_blocking` (or accept
  blocking the async worker) to avoid stalling the reactor. Adds async/sync bridging complexity to a
  codebase that is otherwise deliberately synchronous. Net friction, not a fit.

### RECOMMENDATION (manager) — **HAND-ROLL JSON-RPC over stdio for v0.1; do NOT adopt rmcp now.**
Decisive reasons, in priority order:
1. **MSRV conflict with the deliberate 1.85.0 pin (D10).** rmcp has no declared MSRV, is developed on
   1.92, and is documented at 1.90 minimum. Adopting it either breaks the 1.85 contract or forces a
   pin bump chasing the ecosystem — exactly the whack-a-mole D10 rejected.
2. **Zero-dependency identity (D12 / §10.3).** rmcp drags in tokio + schemars + async-trait +
   proc-macro trees (~dozens of crates). CodeCache's one durable wedge is "zero-dependency,
   deterministic, single static binary, air-gapped." A heavy async SDK on the *one* optional surface
   is the wrong trade for v0.1.
3. **Async-over-sync friction (D8).** rmcp forces tokio onto a synchronous SQLite core; correct use
   needs `spawn_blocking` bridging. Hand-roll has zero boundary.
4. **Modest, frozen scope.** stdio + 3 tools + handshake is ~250–450 LOC over serde_json we already
   ship — well within our TDD discipline and cheaper to own than to bridge.

**Re-evaluate `rmcp` at v0.2**, when SSE/HTTP transports (D4) and richer protocol features make the
SDK's transport/codegen breadth pay for itself, and when an MSRV bump can be a deliberate decision
rather than forced. Keep `mcp_server` behind the D4 transport-agnostic seam so swapping in rmcp later
is an adapter change, not a refactor. **If the human prefers rmcp**, the entry condition is: a verified
`cargo +1.85.0 build` of rmcp 1.7 (or an agreed MSRV bump), acceptance of the tokio/schemars dep set
in §10.3, and a `spawn_blocking` boundary spec for D8.

> **DEP DECISION STATUS: RESOLVED (2026-06-12) — HAND-ROLL.** Human ratified hand-rolling JSON-RPC
> over stdio for v0.1 (serde/serde_json only; no new runtime dep; no `rmcp`). ROADMAP D15 flipped to
> RESOLVED; `project_plan.md` §10.2 updated; §10.3 confirmed needs no new runtime dep. RED/GREEN may
> proceed on the slice plan below.

### Proposed M8 slice breakdown (same shape either path; framing layer differs)
- **M8.1 — JSON-RPC framing + handshake.** `initialize` → server capabilities; malformed → -32700;
  unknown method → -32601; missing param → -32602; no panic. *(Hand-roll: we write the loop. rmcp:
  collapses into SDK; tests target our `ServerHandler`.)*
- **M8.2 — tool registration (tools/list).** All three tools with exact §8.2 inputSchema
  (search: query/max_tokens/file_filter · update: files[] · outline: path/max_tokens — D13).
- **M8.3 — tools/call round-trip.** `handle_search`→`Retriever::query`→agent-first markdown (D13);
  `handle_update`→`Indexer::update_files`→stats; `handle_outline`→storage symbol lookup by path
  prefix→skeleton from stored start/end lines (zero source reads — D7); bad args → -32602.
- **M8.4 — self-healing search (D14).** hash-check result files (`hasher::is_changed` vs
  `files_metadata`) → re-index changed → re-run query once → format; clean files = no writes; deleted
  file dropped, no panic; record staleness-window metric hook.
- **Cross-cutting (resolve before M8.1 GREEN):** D8 storage ownership (`Arc<Mutex<Connection>>` lent to
  retriever+indexer); `serve --transport stdio` replaces M7 stub; `--transport sse`/`--port` parse but
  return "unsupported in v0.1" (D4 seam kept).

---
## Definition of Done (M8 phase — enforced by manager)
- [ ] M8.0 D15 decision recorded in ROADMAP; dep pinned or manual path confirmed (BLOCKED on human signoff).
- [ ] M8.1–M8.4 green vs mock client; handshake + tools/list + tools/call round-trip.
- [ ] All three tool schemas match §8.2 exactly (D13); search output agent-first ordered.
- [ ] Self-healing search proven (D14 staleness tests).
- [ ] Malformed/unknown/invalid-params → correct JSON-RPC error codes, no panic.
- [ ] D8 ownership resolved; serve stub replaced; SSE/HTTP cleanly unsupported.
- [ ] clippy/fmt clean; reviewer APPROVED; docs/TODO.md Phase 8 + src/mcp_server/CLAUDE.md updated.

---
## RED — test lead

**Slice M8.1 — JSON-RPC framing over stdio + `initialize` handshake.** Dep sign-off RESOLVED
(hand-roll, serde/serde_json only). Tests written **first**; both files confirmed RED for the
right reason (compile error / unexpected success), not a typo.

### Files
- `tests/mcp_tests.rs` — NEW. 6 integration tests driving the server over an **in-memory**
  `serve(reader, writer, server)` seam (in-memory `Cursor`/`Vec<u8>`, no real stdio, no subprocess).
- `tests/e2e_cli.rs` — appended 1 cross-cutting `assert_cmd` E2E (test #6) for the serve transport.

### Pinned decisions (eng-lead + reviewer MUST honor — the tests are the contract)
1. **Framing = line-delimited JSON (newline-framed).** Exactly one JSON-RPC object per line, each
   request and response terminated by a single `\n`. No `Content-Length` headers. (Plan §8 says
   "newline/length-framed"; we pick newline for v0.1 simplicity.) Tests assert on the **raw bytes**:
   output ends with `\n`, one request ⇒ exactly one response line, no embedded newline in a frame.
2. **protocolVersion = `"2024-11-05"`** (stable MCP revision; plan §8 pins none, so this is the
   M8.1 decision). Hard-coded as `PROTOCOL_VERSION` in `mcp_tests.rs`; the `initialize` result must
   echo it. Change in lock-step if ever revised.
3. **Error codes:** parse error `-32700`, method-not-found `-32601`, invalid-params `-32602`.
   Every malformed/edge input → a structured JSON-RPC `error` object; the loop **never panics** and
   returns `Ok(())` at clean EOF.
4. **`initialize` result shape:** `{ protocolVersion, capabilities: {object}, serverInfo: { name:
   non-empty string, version: string } }` under `result`; response carries `jsonrpc:"2.0"` + echoed
   `id`; no `error` on the happy path.
5. **D8 storage seam:** `CodeCacheServer::new(Storage)` takes one shared `Storage`
   (`Arc<Mutex<Connection>>` clone) — proven to compile in the harness (`test_server()`). The
   handshake path itself does not read storage; the constructor takes it now so M8.2–M8.4 reuse the
   same seam unchanged. (This is the D8 confirmation — a dedicated redundant test is not needed; the
   harness constructing the real server is the structural proof.)

### REQUIRED entry-point signature (GREEN target — make these exist so `mcp_tests.rs` compiles)
```rust
// src/mcp_server/mod.rs
pub struct CodeCacheServer { /* holds Storage; Retriever/Indexer wired in M8.3 */ }

impl CodeCacheServer {
    /// D8: one shared Storage (Arc<Mutex<Connection>>) lent onward to Retriever/Indexer later.
    pub fn new(storage: codecache::storage::Storage) -> Self;
    // (intra-crate this is `crate::storage::Storage`; the test imports `codecache::storage::Storage`)
}

/// Transport-agnostic (D4) read→dispatch→write loop. Reads line-delimited JSON-RPC requests from
/// `reader`, writes line-delimited (`\n`-terminated) JSON-RPC responses to `writer`. Returns
/// Ok(()) at clean EOF; NEVER panics on malformed input. Generic R/W so tests inject in-memory
/// pipes; the real `serve` CLI handler calls `serve(stdin.lock(), stdout.lock(), server)`.
pub fn serve<R: std::io::BufRead, W: std::io::Write>(
    reader: R,
    writer: W,
    server: CodeCacheServer,
) -> anyhow::Result<()>;
```
Both `serve` and `CodeCacheServer` must be `pub` and re-exported from `mcp_server` (the test does
`use codecache::mcp_server::{serve, CodeCacheServer};`). Do **not** read tools/list, tools/call, or
self-healing in this slice — those are M8.2–M8.4.

### Tests (all RED now)
`tests/mcp_tests.rs`:
1. `initialize_request_returns_server_capabilities` — handshake → `result` with pinned
   protocolVersion + `capabilities` object + `serverInfo{name,version}`, echoed id, jsonrpc 2.0.
2. `malformed_json_returns_parse_error` — garbage line → `error.code == -32700`, jsonrpc 2.0, has
   `message`; no panic.
3. `unknown_method_returns_method_not_found` — valid envelope, unknown `method` → `-32601`, echoed id.
4. `missing_required_param_returns_invalid_params` — `initialize` with no `params` → `-32602`, echoed id.
5. `response_is_a_single_newline_terminated_json_line` — **framing**: raw bytes are one
   `\n`-terminated line, no embedded newline, round-trips as JSON-RPC 2.0 with echoed id 42.
6. `malformed_stream_never_panics_and_each_response_is_structured` — adversarial stream (non-json,
   bare array, scalar, unknown method, a good `initialize`, truncated object): `serve` returns Ok,
   every emitted line is independently parseable JSON-RPC, and the good `initialize` (id 100) still
   yields a success `result` — proving recovery after errors (the **no-panic-ever** guarantee).

`tests/e2e_cli.rs` (cross-cutting):
7. `e2e_serve_unsupported_transport_sse_errors_cleanly` — `serve --transport sse` on an initialized
   project exits **NONZERO** with a clean stderr naming the v0.1 limitation ("unsupported"/"not
   supported"), no `panicked` on either stream. (Chosen at the binary level via `assert_cmd` —
   precedent D17 / `e2e_cli.rs` — because exit-code + stderr is the contract under test, and it is
   lighter than asserting the blocking stdio path. The stdio happy-path is covered by the in-memory
   seam in `mcp_tests.rs`, which can't block on real stdin.)

### Confirmed RED output (Rust 1.85, this session)
- `cargo test --test mcp_tests --no-run`:
  ```
  error[E0432]: unresolved imports `codecache::mcp_server::serve`,
  `codecache::mcp_server::CodeCacheServer`
   --> tests\mcp_tests.rs:57:29
     | no `serve` in `mcp_server`  /  no `CodeCacheServer` in `mcp_server`
  error: could not compile `codecache` (test "mcp_tests") due to 1 previous error
  ```
  → correct reason: the M0 stub exports neither symbol yet (the GREEN target).
- `cargo test --test e2e_cli e2e_serve_unsupported_transport_sse_errors_cleanly`:
  ```
  test e2e_serve_unsupported_transport_sse_errors_cleanly ... FAILED
  panicked at ...: Unexpected success
  command=`...codecache.exe "serve" "--transport" "sse"`  code=0
  stdout="serve: the MCP server is not implemented yet (M8).\n"  stderr=""
  ```
  → correct reason: the M7 serve stub exits 0; GREEN must reject non-stdio transports nonzero.

### Run command
`cargo test --test mcp_tests` (and `cargo test --test e2e_cli` for the cross-cutting one).

### Notes / open items handed to eng-lead
- The `serve` CLI handler (`src/cli/serve.rs`) currently takes no args and ignores `--transport`.
  GREEN must thread `transport`/`db_path` through `dispatch` (`Command::Serve { transport, port,
  db_path }` → `serve::run(transport, port, &db_path)`): `stdio` → build `CodeCacheServer` from the
  resolved db + `serve(stdin.lock(), stdout.lock(), server)`; `sse` (or `port` set) → return a clean
  `anyhow::Error` "unsupported in v0.1" (D4 seam). No reachable `unwrap/expect/panic` in the handler.
- `notifications/initialized` and other post-handshake notifications are **out of scope** for M8.1
  (no test pins them); add when a later slice needs them.
- Did not need new fixtures or new deps (serde_json + tempfile + assert_cmd already in the tree).

## GREEN — engineering lead

**Slice M8.1 GREEN (2026-06-12).** Hand-rolled JSON-RPC 2.0 over a generic reader/writer per the
RED pin. serde/serde_json/anyhow only — no new deps, no rmcp, no tokio. All five gates green.

### Files changed
- `src/mcp_server/mod.rs` — implemented the server (was an empty stub). ~170 LOC incl. one unit test.
- `src/cli/serve.rs` — replaced the M7 stub: `run(transport, db_path)` → stdio loop or clean SSE error.
- `src/cli/mod.rs` — `dispatch` now threads `Command::Serve { transport, port: _, db_path }`
  through to `serve::run(transport, &db_path)` (was `serve::run()` dropping all args).
- `src/lib.rs` — no change needed; `pub mod mcp_server;` already declared, so
  `codecache::mcp_server::{serve, CodeCacheServer}` resolve.

### Final public API (matches the RED signature EXACTLY)
```rust
// src/mcp_server/mod.rs
pub struct CodeCacheServer { /* holds Storage (D8); Retriever/Indexer wired in M8.3 */ }
impl CodeCacheServer {
    pub fn new(storage: codecache::storage::Storage) -> Self;
}
pub fn serve<R: std::io::BufRead, W: std::io::Write>(
    reader: R, writer: W, server: CodeCacheServer,
) -> anyhow::Result<()>;
```
`CodeCacheServer` holds `Storage` behind `#[allow(dead_code)]` (handshake never reads it; the
constructor freezes the D8 seam for M8.2–M8.4 unchanged).

### Framing / protocol constants
- `PROTOCOL_VERSION = "2024-11-05"` (matches `mcp_tests.rs::PROTOCOL_VERSION`).
- `SERVER_NAME = "codecache"`; `serverInfo.version = crate::VERSION` (= `env!("CARGO_PKG_VERSION")`).
- Error codes: `PARSE_ERROR -32700`, `METHOD_NOT_FOUND -32601`, `INVALID_PARAMS -32602`.
- Framing: `serve` iterates `reader.lines()`; blank/whitespace lines are skipped (no frame
  emitted); each answered line writes exactly one `\n`-terminated JSON object via `write_frame`
  (`serde_json::to_string` + `push('\n')` + `write_all`). EOF → `writer.flush()` → `Ok(())`.

### How each RED test passes
1. `initialize_request_returns_server_capabilities` — `handle_initialize` returns
   `{ protocolVersion, capabilities:{}, serverInfo:{name,version} }` under `result`; envelope echoes
   `id` and carries `jsonrpc:"2.0"`, no `error`.
2. `malformed_json_returns_parse_error` — `serde_json::from_str` Err → `error_response(Null, -32700, …)`.
3. `unknown_method_returns_method_not_found` — `dispatch` default arm → `-32601`, id echoed.
4. `missing_required_param_returns_invalid_params` — `initialize` with no `params` →
   `-32602` (also rejects a `params` object missing `protocolVersion`).
5. `response_is_a_single_newline_terminated_json_line` — `write_frame` emits exactly one
   `\n`-terminated line, no embedded newline; round-trips with id 42.
6. `malformed_stream_never_panics_and_each_response_is_structured` — non-json → -32700; bare array
   / scalar → `as_object()` guard → -32700; unknown method → -32601; good `initialize` (id 100) →
   success `result`; truncated object → -32700. No reachable unwrap/expect/panic; loop returns
   `Ok(())` at EOF. The only `?`-propagated errors are reader/writer IO errors (real EOF is not an
   error — `lines()` ends the iterator).

### Cross-cutting (e2e #7)
`e2e_serve_unsupported_transport_sse_errors_cleanly` — `serve::run` matches `Transport::Sse` →
`bail!("SSE transport is not supported in v0.1 (stdio only)")` → nonzero exit, clean stderr, no
panic. `Transport::Stdio` resolves db (`paths::resolve` + `Storage::new`), builds the server, and
calls `serve(stdin().lock(), stdout().lock(), server)`.

### Deviations / notes
- **`--port` is NOT used to reject** in this slice. The brief body mentioned "sse (or a non-default
  port intent)", but `--port` has a clap default of 3000 (always present) and no test pins port
  behavior; rejecting on the default would be wrong. Only `--transport sse` errors (the exact e2e
  contract). `port` is bound as `port: _` in dispatch — available for a future SSE slice. Flagging
  for manager visibility; no plan/spec change made.
- `tests/mcp_tests.rs` (test-lead's untracked RED file) is NOT `cargo fmt`-clean as committed; I did
  not touch it (TDD: tests are the contract). All **production** files I changed are fmt-clean —
  `cargo fmt --check` shows diffs only in `tests/mcp_tests.rs`. Heads-up for the manager/CI: either
  the test lead reformats that file or CI's fmt gate will flag it independently of this slice.

### Gates (all green)
- `cargo test --test mcp_tests` → 6/6 pass.
- `cargo test --test e2e_cli` → 6/6 pass (incl. the SSE cross-cutting test).
- `cargo clippy --all-targets -- -D warnings` → clean.
- `cargo build` → clean.
- `cargo test` (full suite) → **149 passed, 0 failed** (27 lib unit + 122 integration).
- `cargo fmt --check` → production files clean; only `tests/mcp_tests.rs` (test-lead's file) differs.

## Specialist / Perf notes
_(framing overhead must be < few ms; search call bounded by M6 p95 < 500ms budget)_

## REVIEW — code reviewer

**VERDICT: BLOCK** (reviewed 2026-06-12, Rust 1.85). One blocker: `cargo fmt --check` is NOT
clean. Correctness, no-panic, dependency, and D4-seam properties all verified and good — the
block is hygiene-only and a one-command fix.

### Gate results
- `cargo build` → clean (exit 0).
- `cargo clippy --all-targets -- -D warnings` → clean (exit 0).
- `cargo test` → **149 passed, 0 failed** (27 lib unit + 122 integration; mcp_tests 6/6, e2e_cli 6/6).
- `cargo fmt --check` → **FAILS**: 5 unformatted hunks remain, all in `tests/mcp_tests.rs`
  (lines 96, 119, 176, 222, 255). Production `src/` is fmt-clean.

### Findings
- **blocker — tests/mcp_tests.rs:96,119,176,222,255 — `cargo fmt --check` is not clean.**
  The REVIEW prompt stated the manager applied "two fmt-only line-wrapping fixes" to make this
  file clean, but the on-disk file still has 5 rustfmt diffs (re-wrapping `.expect(...)` chains
  and `format!(...)` calls). The root CLAUDE.md hygiene gate and CI both require
  `cargo fmt --check` clean across the whole tree; CI would reject this as-is. The GREEN note
  itself flagged the file as not fmt-clean. **Fix:** run `cargo fmt` (formats the 5 hunks; no
  assertion text changes — verified the pending diff only re-wraps existing calls), then re-run
  `cargo fmt --check` to confirm clean. This touches only test scaffolding (`.expect`/`format!`
  layout), not any assertion, so it does not weaken the contract.

### What I verified GOOD (no change needed)
- **Framing/dispatch/write loop correctness** (mod.rs:89-106): reads line-delimited via
  `reader.lines()` (strips `
`/`
`), skips blank/whitespace lines without emitting a frame,
  writes exactly one `
`-terminated JSON object per answered line, flushes, returns `Ok(())` at
  clean EOF. Loop recovers after a malformed line (per-line `handle_line` never aborts the stream).
- **Error mapping exact** (mod.rs:27-29,110-136): parse/non-object → -32700; missing `method` →
  -32602; unknown method → -32601 (dispatch default arm); `initialize` missing `params` OR
  `params` lacking string `protocolVersion` → -32602. Matches the pinned contract.
- **`initialize` result shape** (mod.rs:73-80): `{ protocolVersion:"2024-11-05", capabilities:{},
  serverInfo:{ name:"codecache", version: crate::VERSION } }`; envelope echoes `id` verbatim
  (mod.rs:126, `.cloned().unwrap_or(Value::Null)` — correct null fallback) and carries
  `jsonrpc:"2.0"`. PROTOCOL_VERSION matches `mcp_tests.rs::PROTOCOL_VERSION`.
- **No reachable unwrap/expect/panic in production paths.** Scanned src/mcp_server/mod.rs and
  src/cli/serve.rs — none. Only `?`-propagated IO errors (line read / write_all / flush) and
  serde serialization error in `write_frame`. serve.rs maps `StorageError` via
  `anyhow::Error::new` (+`with_context`) — `StorageError` implements `std::error::Error`, so this
  is sound; lock-poison is `StorageError::LockPoisoned`, not a panic.
- **No new dependencies.** Cargo.toml `[dependencies]` unchanged; serde/serde_json/anyhow only.
  No rmcp, no tokio, no async. Honors D15 RESOLVED.
- **D4 transport seam clean** (serve.rs:20-25): `Transport::Sse` → `bail!` clean anyhow error →
  nonzero exit, no panic (e2e #7 asserts stderr names the limitation, no "panicked" on either
  stream). `serve` core is generic over `BufRead`/`Write` (D4); CLI passes `stdin/stdout` locks.
- **Tests not weakened.** The 6 mcp_tests assertions and the e2e test are intact; the pending
  rustfmt changes only re-wrap call layout, no assertion/expected-value edits.
- **Idiomatic Rust / clippy clean.** `let-else` guards, `ok_or_else`, borrowing not cloning on the
  hot path (only the necessarily-owned `id` is cloned), `#[allow(dead_code)]` on `storage` is
  justified (freezes the D8 seam for M8.2-M8.4).

### Re-review
Run `cargo fmt`, confirm `cargo fmt --check` is clean, then this is an **APPROVE** — no other
findings. (Recommend the manager also note the `--port` non-rejection deviation in M8.2+ planning;
it is correct for this slice since no test pins port behavior, so it is not a block.)


## OUTCOME — manager
D15 evaluation complete. Recommendation: **HAND-ROLL** (do not adopt rmcp for v0.1). Awaiting human
dep sign-off before sequencing RED. No code, no Cargo.toml, no ROADMAP disposition change this turn.

---
## RED — test lead (M8.2 — `tools/list` returns all three tools with exact §8.2 schemas, D13)

**Slice M8.2.** 5 new integration tests appended to `tests/mcp_tests.rs` (the M8.1 file). The
M8.1 harness (`test_server`, `run_server`, `single_response`) is REUSED unchanged; the six M8.1
tests are untouched and still pass. RED confirmed for the right reason: the server returns
`-32601 method not found: tools/list` (no `tools/list` handler yet).

### Files
- `tests/mcp_tests.rs` — appended an M8.2 section: 4 helpers + 5 tests (tests #7–#11). New helpers:
  `tools_list_request_line(id)`, `tools_list(id)`, `tools_array(resp)`, `find_tool(resp, name)`,
  `input_schema_properties(tool)`, `input_schema_required(tool)`. No edits to M8.1 code.

### Tests added (all RED now)
7. `tools_list_returns_all_three_tools` — `result.tools` is an array of length 3; name set is
   EXACTLY {`codecache_search`, `codecache_update`, `codecache_outline`}; each tool has a non-empty
   `description` and an `inputSchema` of `type:"object"`; id echoed, jsonrpc 2.0.
8. `tools_list_includes_codecache_search_with_input_schema` — `query` (string), `max_tokens`
   (integer, `default` JSON number `4000`), `file_filter` (string, `default` JSON `null`);
   `required == ["query"]`.
9. `tools_list_includes_codecache_update_with_input_schema` — `files` (array, `items.type ==
   "string"`); `required == ["files"]`.
10. `tools_list_includes_codecache_outline_with_input_schema` (D13) — `path` (string),
    `max_tokens` (integer, `default` JSON number `2000`); `required == ["path"]`.
11. `tools_list_tool_order_is_stable_and_deterministic` — id echoed, jsonrpc 2.0, tools emitted in
    the FIXED order [search, update, outline], identical across two `tools/list` calls.

### Pinned decisions the eng-lead MUST honor (the tests are the contract)
1. **`tools/list` request:** `{ "jsonrpc":"2.0", "id":N, "method":"tools/list" }` — `params` is
   optional/absent (MCP allows it; the test omits it). Dispatch must accept `tools/list` with no
   `params` and NOT reject it as invalid-params.
2. **Result shape the eng-lead must emit:**
   ```json
   { "jsonrpc":"2.0", "id":N,
     "result": { "tools": [ { "name", "description", "inputSchema" }, ... ] } }
   ```
   i.e. `result.tools` is an ARRAY of tool objects, each `{ name, description, inputSchema }`.
3. **Exactly 3 tools**, names `codecache_search`, `codecache_update`, `codecache_outline`. Each
   `description` non-empty; each `inputSchema.type == "object"`.
4. **`default` representation = JSON value of the property's own type.** `max_tokens` defaults are
   JSON NUMBERS (`4000` / `2000`), asserted via both `as_i64()` and `is_number()` — emitting them
   as strings (`"4000"`) FAILS. `file_filter`'s default is JSON `null` (`is_null()`), not the
   string `"null"` and not an omitted key.
5. **`required` arrays exact** (order asserted as written): search `["query"]`, update `["files"]`,
   outline `["path"]`.
6. **TOOL ORDER is fixed and deterministic: [`codecache_search`, `codecache_update`,
   `codecache_outline`].** Test #11 asserts this order AND that it is identical across two calls.
   The eng-lead must emit the tools in this stable order (e.g. a fixed array / `IndexMap`, not a
   `HashMap` iteration). §8.2 lists them Tool 1=search, Tool 2=update, Tool 3=outline — that is the
   pinned order.

### §8.2 schema fields asserted (verbatim from project_plan.md §8.2, lines ~1331–1427)
- **codecache_search.inputSchema.properties:** `query{type:string}`,
  `max_tokens{type:integer, default:4000}`, `file_filter{type:string, default:null}`;
  `required:["query"]`. (Property `description` strings are NOT asserted — only types/defaults/required.)
- **codecache_update.inputSchema.properties:** `files{type:array, items:{type:string}}`;
  `required:["files"]`.
- **codecache_outline.inputSchema.properties:** `path{type:string}`,
  `max_tokens{type:integer, default:2000}`; `required:["path"]`.

### Contract clarifications / what is NOT pinned (eng-lead latitude)
- **Per-property `description` strings are NOT asserted** by these tests (only tool-level
  `description` non-emptiness is). The eng-lead SHOULD still emit the §8.2 description text for
  client UX, but a wording change won't break M8.2 tests. Tool-level `description` MUST be non-empty.
- **No `additionalProperties` / `$schema` assertions.** The eng-lead may add them; the tests use
  `.get(...)` navigation (not strict equality on the whole schema), so extra keys are tolerated.
- **`tools/call` is OUT of scope** (M8.3). These tests only enumerate tools; they never invoke one.

### Confirmed RED output (Rust 1.85, this session)
`cargo test --test mcp_tests`:
```
test result: FAILED. 6 passed; 5 failed; 0 ignored
failures:
  tools_list_returns_all_three_tools
  tools_list_includes_codecache_search_with_input_schema
  tools_list_includes_codecache_update_with_input_schema
  tools_list_includes_codecache_outline_with_input_schema
  tools_list_tool_order_is_stable_and_deterministic
panicked: a well-formed tools/list must NOT produce an error object; got:
  {"error":{"code":-32601,"message":"method not found: tools/list"},"id":N,"jsonrpc":"2.0"}
```
→ correct reason: `dispatch`'s default arm returns -32601 for `tools/list`; no handler exists yet
(the GREEN target). The 6 M8.1 tests still pass (untouched, not weakened).

`cargo fmt --check` → clean (whole tree; `tests/mcp_tests.rs` formatted). The M8.1 fmt blocker is
not repeated.

### GREEN target for the eng-lead
Add a `"tools/list"` arm to `CodeCacheServer::dispatch` returning `Ok(json!({ "tools": [ … ] }))`
with the three tool objects in the fixed [search, update, outline] order and the exact §8.2
schemas above. No new deps; serde_json `json!` only. Keep `serve`/framing untouched.

## GREEN — engineering lead (M8.2)

**Slice M8.2 GREEN (2026-06-12).** `tools/list` now lists the three D13 tools with the exact
§8.2 inputSchemas in the pinned [search, update, outline] order. serde_json `json!` only — no
new deps, no rmcp, no tokio. All five gates green.

### Files changed
- `src/mcp_server/tools.rs` — **NEW.** Holds the three tool schemas as hand-written `json!`
  values mirroring §8.2 verbatim (incl. the real description text). `pub(crate) fn
  tool_definitions() -> Vec<Value>` returns them in the fixed order `vec![search_tool(),
  update_tool(), outline_tool()]` (a `Vec`, never `HashMap` iteration — guarantees determinism).
  One `fn` per tool keeps each schema readable.
- `src/mcp_server/mod.rs` — added `mod tools;`; added a `"tools/list"` arm to `dispatch` →
  `Ok(self.handle_tools_list())`; new `handle_tools_list(&self) -> Value` returns
  `json!({ "tools": tools::tool_definitions() })`. `serve`/framing/`initialize` untouched.

### Result shape emitted
```json
{ "jsonrpc":"2.0", "id":N,
  "result": { "tools": [
    { "name":"codecache_search",  "description":"…", "inputSchema":{…} },
    { "name":"codecache_update",  "description":"…", "inputSchema":{…} },
    { "name":"codecache_outline", "description":"…", "inputSchema":{…} } ] } }
```

### How each M8.2 test passes
7. `tools_list_returns_all_three_tools` — `tool_definitions()` returns exactly 3 objects, names
   {search, update, outline}, each with non-empty §8.2 `description` and `inputSchema.type ==
   "object"`. Envelope echoes `id`, `jsonrpc:"2.0"`, no `error` (dispatch returns `Ok`).
8. `..._codecache_search_...` — `query{string}`, `max_tokens{integer, default:4000}` (JSON
   number via `json!` literal `4000`), `file_filter{string, default:null}` (JSON `null`);
   `required:["query"]`.
9. `..._codecache_update_...` — `files{array, items:{type:"string"}}`; `required:["files"]`.
10. `..._codecache_outline_...` — `path{string}`, `max_tokens{integer, default:2000}` (JSON
    number); `required:["path"]`.
11. `tools_list_tool_order_is_stable_and_deterministic` — fixed `Vec` order [search, update,
    outline]; identical across two calls because `tool_definitions()` is a pure constructor.

### Deviations / notes
- None. `params` on `tools/list` is accepted-and-ignored (the arm takes no params); absent
  `params` is NOT rejected as invalid-params. `tools/call` execution remains out of scope (M8.3).
- Module split: schemas live in `src/mcp_server/tools.rs` (the plan names this file for
  schemas+handlers). `tool_definitions` is `pub(crate)`; only `mod.rs` consumes it.

### Gates (all green)
- `cargo test --test mcp_tests` → **11/11** (6 M8.1 + 5 M8.2).
- `cargo test` (full suite) → **154 passed, 0 failed** (was 149; +5 M8.2).
- `cargo clippy --all-targets -- -D warnings` → clean.
- `cargo fmt --check` → clean (whole tree).
- `cargo build` → clean.

## REVIEW — code reviewer (M8.2)

**VERDICT: APPROVE** (reviewed 2026-06-12, Rust 1.85). The three §8.2 tool schemas match the
plan EXACTLY; tool order is deterministic via `Vec`; `tools/list` accepts absent params and
echoes id; no reachable panic/unwrap/expect; no new deps; all four gates green.

### Gate results
- `cargo fmt --check` → clean (whole tree; the M8.1 fmt blocker is NOT repeated).
- `cargo clippy --all-targets -- -D warnings` → clean (exit 0).
- `cargo test` → **154 passed, 0 failed** (27 lib unit + 127 integration; mcp_tests 11/11).
- `cargo build` → clean (exit 0).

### Schema fidelity to §8.2 (the crux) — verified EXACTLY, char-by-char
- **codecache_search**: `query{type:"string"}`; `max_tokens{type:"integer", default:4000}`
  (JSON number); `file_filter{type:"string", default:null}` (JSON null); `required:["query"]`.
- **codecache_update**: `files{type:"array", items:{type:"string"}}`; `required:["files"]`.
- **codecache_outline (D13)**: `path{type:"string"}`; `max_tokens{type:"integer", default:2000}`
  (JSON number); `required:["path"]`.
- All three tool-level `description`s and all six property `description`s are the real §8.2 text
  verbatim (string-equality checked against project_plan.md — no placeholders).

### What I verified GOOD
- **Deterministic order via Vec** (tools.rs:15-17): `tool_definitions()` returns
  `vec![search_tool(), update_tool(), outline_tool()]` — fixed `[search, update, outline]`, never
  HashMap iteration. Test #11 confirms identical order across two calls.
- **tools/list accepts absent params** (mod.rs:51, 89-91): dispatch arm takes no params and is
  NOT routed through invalid-params; id echoed via the shared `handle_line` path; result shape is
  `{ tools: [...] }`. Minimal 3-line diff to mod.rs; framing/`initialize` untouched.
- **No reachable panic/unwrap/expect** in tools.rs or the mod.rs additions; pure `json!`
  constructors, no IO, no fallible calls.
- **No new deps**: `git diff HEAD -- Cargo.toml` empty; serde_json `json!` only.
- **Tests not weakened**: all 6 M8.1 + 5 M8.2 present and meaningful (assert on types, defaults
  via both `as_i64()` and `is_number()`, null via `is_null()`, exact `required`, stable order).

### Minor (non-blocking — manager close-out, brief protocol step 6)
- `docs/TODO.md:216` still shows M8.2 as `[ ]`, and `src/mcp_server/CLAUDE.md:42,47` still read
  "M8.2–M8.4 pending" / "149 tests". The root golden rule ties doc updates to the code change;
  these should be flipped to DONE / "154 tests" at manager close-out. Not a code-correctness
  block — the source+test contract is complete and correct.
