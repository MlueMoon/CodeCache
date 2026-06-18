# BRIEF — M6 / M6.2 — BM25 search + determinism + dedup (skeleton)

- **Milestone:** M6 — retriever  ·  **Module(s):** `retriever` (uses `storage::search`)
- **Owner (manager):** principal-engineering-manager  ·  **Created:** 2026-06-10
- **Status:** RED ✓  GREEN ✓  REVIEW ✓ (APPROVE)  DONE ✓ (gates green, main session)
- **Links:** docs/ROADMAP.md#m6--retriever · docs/plans/M6-retriever.md#slice-m62--bm25-search--determinism--dedup · docs/TEST_STRATEGY.md#retriever · project_plan.md §3.2.3 / §6.2
- **Routing:** test-lead (RED) → engineering-lead (GREEN) → **rust-treesitter-specialist** (FTS5/BM25 query-plan + weighting sanity, EXPLAIN QUERY PLAN baseline from M1) → code-reviewer.

## Goal
Execute `storage.search(fts_query, max_results)` from the M6.1 MATCH string, apply a stable
tie-break ordering, and dedup overlapping snippets (same file + overlapping byte span ⇒ keep one).
No-match/empty query ⇒ empty, well-formed result.

## Scope (in / out)
- **In:** the search-execution half of `Retriever::query` (no budget yet); `file_filter` post-filter
  (or SQL `file_path` filter — document the choice); dedup by `(file_path, overlapping span)`;
  deterministic order (BM25 asc, then stable key e.g. `file_path, start_byte`). **D1 trait** lands
  here if not in M6.1.
- **Out:** token-budget packing (`apply_token_budget`, `total_tokens`) → M6.3; latency bench → M6.4.

## Scenarios to cover (from plan §6.2 / TEST_STRATEGY#retriever)
- [ ] `relevant_chunk_ranks_above_irrelevant` (seed 2 chunks; query matches one strongly)
- [ ] `same_query_same_index_yields_identical_order` (determinism; stable tie-break)
- [ ] `no_match_query_returns_empty_result` (+ empty-query-from-M6.1 path → empty result)
- [ ] `overlapping_snippets_deduplicated` (same file, overlapping byte span ⇒ one)
- [ ] (file_filter) `file_filter_restricts_results_to_listed_files`
- [ ] cross-cutting: empty DB ⇒ empty result, no panic.

## Definition of Done
- [ ] Tests green · clippy -D warnings · fmt clean · API matches §3.2.3 · §6.2 BM25 (FTS5-native, no custom scorer)
- [ ] D1 trait in place; `file_filter` behavior documented · reviewer APPROVED
- [ ] docs/TODO.md + src/retriever/CLAUDE.md updated

---
## RED — test lead
Added `tests/retriever_tests.rs` (7 integration tests, seed `Storage` directly — M6 is
independent of the index chain). All reference the not-yet-existent
`codecache::retriever::{QueryOptions, Retrieve, Retriever}`, so the crate fails to compile = RED.

Tests:
- `relevant_chunk_ranks_above_irrelevant` — auth chunk (name match) ranks above a math chunk.
- `same_query_same_index_yields_identical_order` — 4 tied-score chunks; order identical across 5
  repeats AND equals `(file_path, start_byte)` ascending — pins the documented stable tie-break.
- `no_match_query_returns_empty_result` — terms absent ⇒ empty chunks, `total_results_found == 0`,
  `total_tokens == 0`.
- `empty_or_all_stopword_query_short_circuits_without_running_match` — `""`, `"   "`, `"find the"`
  all return Ok(empty); success proves the short-circuit (a literal `MATCH ""` would error).
- `empty_db_query_returns_empty_result_without_panic` — cross-cutting empty-index path.
- `overlapping_snippets_deduplicated` — same-file overlapping byte spans collapse to one; a
  non-overlapping same-file span and a same-span different-file chunk are kept (4 seeded ⇒ 3 out),
  plus a property assertion that no two survivors share a file and overlap.
- `file_filter_restricts_results_to_listed_files` — only `file_path`s in the filter set survive.

Note (contract, not weakening): the test budget is set to 1_000_000 tokens so M6.2 never trims —
token-budget packing is M6.3. `total_tokens` is asserted only as `0` for the empty paths, which is
budget-independent.

## GREEN — engineering lead
Implemented in `src/retriever/mod.rs` (no new files — kept ranking inline; `ranking.rs` from the
phase plan was not needed at this size and would be undriven surface).

New public surface (matches §3.2.3):
- `trait Retrieve { fn query(&self, user_query: &str, options: QueryOptions) -> Result<QueryResult>; }`
  — **D1** seam, intentionally minimal (just `query`) so a future `HybridRetriever` implements the
  same trait without churning callers.
- `struct Retriever { storage: Storage }` with `Retriever::new(storage)`; implements `Retrieve`.
- `QueryOptions { max_tokens, max_results, file_filter }` + `Default` (4000 / 20 / None per §3.2.3).
- `QueryResult { chunks, total_tokens, total_results_found }`.
- `enum RetrieverError { Storage(StorageError) }` (impl Error/Display, `From<StorageError>`) +
  `pub type Result<T>`. No reachable `unwrap/expect/panic`.

`query` flow: `preprocess_query` → **if tokens empty, short-circuit** to an empty well-formed
result (never `MATCH ""`) → `build_match_expression` → `storage.search(&expr, max_results)` (the
expression is bound to `symbols MATCH ?1` **inside** `Storage::search` — parameterized, not
interpolated) → `stable_sort` → `apply_file_filter` → `dedup_overlapping` → assemble result.

- **Tie-break:** `bm25_score` ascending via `f64::total_cmp` (total order, no NaN panic), then
  `file_path`, then `start_byte`, then `end_byte`. Re-sorting here (storage already does
  `bm25 ASC, rowid ASC`) makes order reproducible independent of insertion order (`rowid`).
- **Dedup:** keep-first over the ranked list; a later result is dropped iff it shares `file_path`
  with a kept result AND their half-open byte spans **partially cross or are equal**. **Strict
  containment is preserved** (`partial_overlap_or_equal`): the M4 chunker guarantees same-file
  chunks are disjoint OR strictly nested (a method inside its class), so a class and its method
  are distinct retrieval units — collapsing one would destroy real signal. Only crossing partial
  overlaps / exact duplicates collapse; best-ranked survives. (Specialist-driven refinement.)
- **file_filter:** documented as a **post-filter** over `chunk.file_path` (exact `PathBuf` match),
  applied before dedup so the surviving set + `total_results_found` reflect the filtered view. Not
  a SQL predicate — keeps the FTS5 query simple; M7 CLI maps `--file-filter` glob to this list.
- **total_results_found:** post-filter + post-dedup count (pre-budget). Budget trimming + the
  `total_tokens` sum are **M6.3**; `total_tokens` is `0` here.

Removed the temporary `#[allow(dead_code)]` from `preprocess_query` / `build_match_expression` /
`escape_fts5_token` / `STOPWORDS` — all now consumed by `query`. Added one in-module unit test
`spans_overlap_is_half_open`. `lib.rs` already `pub use`s nothing from retriever and `pub mod
retriever` exposes the new items directly (`codecache::retriever::{Retrieve, Retriever, ...}`).

## Specialist / Perf notes
rust-treesitter-specialist (FTS5/BM25 query-plan + weighting sanity):
1. **Parameterized MATCH — correct.** `Storage::search` binds the expression to `symbols MATCH ?1`
   via `params![query, limit]`; the retriever passes `&match_expr` as that bound value. No
   user/derived text is interpolated into SQL — FTS5 syntax-injection surface is closed. The M6.1
   escaper (bareword vs `"…"`-literal) keeps the expression itself syntactically valid.
2. **BM25 weights unchanged — appropriate.** §6.2 mandates FTS5-native `bm25()`, no custom scorer.
   The storage `SEARCH` already weights `symbol_name` 10.0 / `parent_symbol` 5.0 / rest 1–2.0; the
   retriever does not re-weight (correct — that's storage's concern). `relevant_chunk_ranks_above_
   irrelevant` passes on name weighting alone.
3. **Re-sort vs storage `ORDER BY` — consistent, no double-work concern.** Storage emits
   `bm25 ASC, rowid ASC`; the retriever re-sorts `bm25 ASC, file_path, start_byte, end_byte`. Same
   primary key (score asc preserved); only the tie-break differs, replacing the insertion-order
   `rowid` artifact with a data-stable key. `total_cmp` avoids NaN-panic. No EXPLAIN-QUERY-PLAN
   regression — the SQL is untouched; sorting is in-memory over ≤ `max_results` (≤20 default) rows.
4. **Dedup-after-LIMIT — acceptable, noted.** Dedup runs after the SQL `LIMIT max_results`, so in a
   pathological index with many crossing-overlap duplicates the distinct count could dip below
   `max_results`. In practice the M4 non-overlap invariant makes true crossing duplicates rare
   (only re-index races or heuristic/AST span drift produce them), so the post-LIMIT dedup is a
   correct safety net, not a recall risk for v0.1. If it ever matters, fetch `k*max_results` then
   trim — deferred (no evidence it's needed). **Containment preserved** is the right call: nested
   method/class spans are distinct units; collapsing them would silently drop signal.
5. **No `MATCH ""` ever issued** — empty/all-stopword short-circuits before `storage.search`.
   Verified against FTS5 (an empty MATCH string raises `fts5: syntax error near ""`).

Verdict: FTS5/BM25 usage is sound for M6.2. Latency bench (p95 < 500ms) is M6.4 — not gated here.

## REVIEW — code reviewer
**Verdict: APPROVE** (pending main-session gate run — see Outcome; the reviewer cannot run cargo).

Checked against Definition of Done + golden rules:
- **TDD order honored** — `tests/retriever_tests.rs` (RED) reference the new API before it existed;
  no test was weakened. The dedup-containment refinement is a *contract* refinement (preserves
  nested chunks) that the existing RED test still satisfies (partial-overlap pair still collapses).
- **API matches §3.2.3** — `Retriever { storage }`, `new`, `query`; `QueryOptions {max_tokens,
  max_results, file_filter}`; `QueryResult {chunks, total_tokens, total_results_found}`. The
  `preprocess`/`apply_token_budget` private methods in the §3.2.3 sketch are free fns here (M6.1
  decision, already shipped); `query` is exposed via `trait Retrieve` (D1) — a superset, not a
  divergence. Acceptable; plan's prose already anticipates the trait.
- **D1 trait present + minimal** — only `query`; future `HybridRetriever` implements the same trait.
- **No reachable `unwrap/expect/panic`** — `query` propagates `StorageError` via `?` into typed
  `RetrieverError`; `total_cmp` makes the f64 sort total (no `partial_cmp().unwrap()`). `.expect()`
  appears only in tests (allowed).
- **Parameterized MATCH** — expression bound inside `Storage::search`, never interpolated. ✓
- **Determinism** — `total_cmp` + span tie-break give a total, insertion-order-independent order.
- **`#[allow(dead_code)]` removed** from all four M6.1 items (now consumed by `query`). ✓
- **Token budget correctly deferred** — `total_tokens == 0`, no trimming; that's M6.3.

Minor (non-blocking) note for M6.3: `apply_file_filter`'s long fn signature line and the
`QueryResult`/`QueryOptions` field order will be normalized by `cargo fmt`; nothing semantic.
No blocking findings.

## OUTCOME — manager
RED → GREEN → specialist → review complete; reviewer APPROVED. Files changed this slice:
- `tests/retriever_tests.rs` (NEW) — 7 integration tests.
- `src/retriever/mod.rs` — added `trait Retrieve`, `Retriever`, `QueryOptions` (+`Default`),
  `QueryResult`, `RetrieverError`/`Result`, `query`, `stable_sort`, `dedup_overlapping`,
  `apply_file_filter`, `partial_overlap_or_equal`; removed 4 M6.1 `#[allow(dead_code)]`; added
  unit test `partial_overlap_or_equal_keeps_containment_drops_crossing`.
- `docs/TODO.md` — M6.2 → `[x]` (with gate caveat).
- `src/retriever/CLAUDE.md` — M6.2 shipped API + ranking/dedup/filter semantics + status.
- this brief.

**Gate verification (main session, 2026-06-11) — DONE, all green on Rust 1.85.0:**
- `cargo fmt --all` reflowed `apply_file_filter`'s signature + struct fields; `cargo fmt --all -- --check` clean.
- `cargo clippy --all-targets -- -D warnings` clean (no lints; `Default` is non-derivable, the
  `&Option<Vec<_>>` arg is not a default-gate lint — as the specialist predicted).
- `cargo test --all` **111 passed** (23 lib +7 retriever +3 chunker_proptest +10 chunker +5 config
  +4 e2e +11 hasher +15 indexer +14 parser +1 smoke +18 storage), 0 failed.
- `cargo bench` not run (M6.4 wires the query bench).

Committed by the main session. Status flipped to **DONE**.
