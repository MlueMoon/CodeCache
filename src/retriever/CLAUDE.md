# src/retriever/ — CLAUDE.md

**Module:** `retriever` · **Owner:** `principal-engineering-lead` · **Milestone:** M6 (stub at M0).

## Purpose
Query execution: preprocess query → FTS5 BM25 search → snippet extraction → token counting →
greedy token-budget packing. Kept behind a trait so a `HybridRetriever` (embeddings) can wrap it
in v0.2 without churn (**Decision Log D1**).

## API anchor
`docs/project_plan.md` §3.2.3 (`Retriever`, `QueryOptions`, `QueryResult`) + §6.

## Tests / scenarios
`docs/TEST_STRATEGY.md#retriever` — deterministic BM25 ranking; `--max-tokens` never exceeded;
empty/no-match → well-formed empty result; dedup of overlapping snippets.

## Perf
Query latency budget **p95 < 500ms on 100K LOC, cold cache** (project_plan §1.3 / §11.2; warm
breakdown target <100ms: FTS5 <50ms, BM25 <10ms, snippet <20ms, tokens <10ms, format <10ms).
**M6.4 bench wired:** `benches/query_bench.rs` (registered in `Cargo.toml`, `harness=false`) seeds
`Storage` directly with ~100K-LOC of synthetic chunks **outside** the timed region and times **only**
`Retriever::query` (the full preprocess→FTS5→BM25→tie-break→dedup→token-budget-pack path). `max_results`
left at the §3.2.3 default 20 so in-flight chunks stay within the ~10MB cap (§11.3). p50 ≈ criterion
median; p95/p99 are read from criterion's raw `sample.json`. **Tracked baseline, not a hard CI gate** —
exceeding 500ms is a regression signal; the hard budget gate (assert/CI-fail) + full suite land at M10.
Actual measured p50/p95/p99 numbers are captured by the main-session `cargo bench --bench query_bench`
run (the manager subagent cannot run cargo).

## Shipped API (M6.1 — query preprocessing)
Module-private, dependency-free string helpers (no `Storage` yet; M6.2's `query` calls them):
- `preprocess_query(&str) -> Vec<String>` — tokenize → lowercase (Unicode-aware) → drop
  `STOPWORDS` → FTS5-escape. Tokenizer splits on any char that is **not** alphanumeric / `_` /
  `"` (so `()`, `:`, `-`, whitespace separate; `"` stays in-token to be escaped). Empty / all-
  stopword input → `[]`. Total, deterministic, no `unwrap/expect/panic`.
- `build_match_expression(&[String]) -> String` — ` OR `-join into the FTS5 `MATCH` string
  (§6.1); `&[]` → `""` (caller maps to an empty result, never runs `MATCH ""`).
- `escape_fts5_token(&str) -> String` — a safe ASCII bareword (alnum/`_`) is emitted **unquoted**;
  any other token (non-ASCII like `café`, or one carrying a `"`) becomes an FTS5 **string literal**
  `"…"` with internal `"` doubled, so the joined expression is always syntactically valid.
- `STOPWORDS: &[&str]` — 21 natural-language filler words (`the`, `find`, `show`, `how`, …);
  **no programming keywords** (often the query target). Linear `.contains` (fine at this size).

## Shipped API (M6.2 — BM25 search + determinism + dedup)
The search-execution half of `query` (no token budget yet — that's M6.3):
- `trait Retrieve { fn query(&self, &str, QueryOptions) -> Result<QueryResult> }` — the **D1** seam,
  minimal on purpose so a future `HybridRetriever` implements the same trait without churn.
- `Retriever { storage: Storage }` + `Retriever::new(storage)`; implements `Retrieve`.
- `QueryOptions { max_tokens, max_results, file_filter, bm25_weights }` (+ `Default` =
  4000/20/None/None, §3.2.3). `bm25_weights: Option<[f64; 7]>` is the R2.2a/D24 per-column BM25
  override; `None` ⇒ default weights (default-identical), `Some(w)` ⇒ re-rank via
  `Storage::search_with_weights`. `query` threads it: `storage.search_with_weights(&expr,
  max_results, options.bm25_weights.as_ref())` (was `storage.search`).
- `QueryResult { chunks, total_tokens, total_results_found }`. `total_tokens` is `0` until M6.3;
  `total_results_found` is the post-filter + post-dedup (pre-budget) count.
- `RetrieverError::Storage(StorageError)` (impl Error/Display, `From<StorageError>`) + `Result<T>`.
- `query` pipeline: `preprocess_query` → **short-circuit if no tokens** (empty/all-stopword ⇒ empty
  `QueryResult`, never `MATCH ""`) → `build_match_expression` → `storage.search(&expr, max_results)`
  (expression bound to `symbols MATCH ?1` **parameterized**, not interpolated) → stable sort →
  `file_filter` post-filter → dedup → assemble.

### Ranking / dedup / filter semantics
- **Tie-break (deterministic):** `bm25_score` ascending via `f64::total_cmp` (total order, no NaN
  panic), then `(file_path, start_byte, end_byte)` ascending. Re-sorts the storage `bm25 ASC, rowid
  ASC` so order is reproducible independent of insertion order (`rowid` is an insertion artifact).
- **Dedup (`partial_overlap_or_equal`):** within one file, a later chunk is dropped iff its
  half-open byte span **partially crosses or exactly equals** a kept chunk's. **Strict containment
  is preserved** — the M4 chunker guarantees same-file chunks are disjoint OR strictly nested, so a
  class and a method inside it are distinct units and both survive. Different files never collide.
  Dedup runs after the SQL `LIMIT` (safety net; true crossing duplicates are rare given M4's invariant).
- **`file_filter`:** documented as a **post-filter** over `chunk.file_path` (exact `PathBuf` match),
  not a SQL predicate — keeps the FTS5 query simple; M7 CLI maps `--file-filter` glob to this list.

## Shipped API (M6.3 — token-budget packing)
The §6.3 greedy packer; `query` now trims to the budget instead of returning everything:
- `fn estimate_tokens(text: &str) -> usize` (module-private) — the §6.3 char heuristic
  `(text.len() / 4).max(1)`, **no tokenizer crate**. `text.len()` is the **byte** length (a
  multibyte identifier counts its UTF-8 bytes — a conservative over-estimate vs. chars). The
  `.max(1)` floor means even empty / 1–3-byte text costs ≥ 1 token. Callers pass `chunk.chunk_text`
  (full signature+body — the same text the M7 formatter emits, so the budget reflects bytes
  actually delivered to the agent).
- `Retriever::apply_token_budget(&self, results: Vec<SearchResult>, max_tokens: usize) ->
  Vec<SearchResult>` (the §3.2.3 surface) — greedy over the already-ranked/deduped list: keep each
  chunk whose `estimate_tokens` still fits the running total, **hard-stop** (`break`) at the first
  that would push over `max_tokens`. Returns the fitting prefix; total, no `unwrap/expect/panic`.
- `query` pipeline tail now: dedup → `total_results_found = deduped.len()` (**pre-budget**) →
  `apply_token_budget(deduped, max_tokens)` → `total_tokens = Σ estimate_tokens(packed)` → assemble.

### Budget semantics / decisions (pinned by tests)
- **Length basis:** `chunk.chunk_text` (signature+body). Documented for M7 so the formatter emits
  the same text the budget counted.
- **Greedy stop, not skip-and-continue:** once a chunk doesn't fit we stop — we do **not** skip it
  to squeeze a smaller later chunk in. Keeps the highest-ranked contiguous prefix (§6.3 `break`).
- **Oversized first chunk ⇒ empty pack** (`total_tokens = 0`), **not** a forced top-1: `max_tokens`
  is a hard ceiling the caller asked for, so the result never exceeds it. Pinned by
  `oversized_first_chunk_yields_empty_pack`.
- **`total_tokens <= max_tokens` always** (the pack is a fitting prefix); empty/no-token paths → 0.

## Decision Log bindings
- **D1 (trait):** `trait Retrieve` + `Retriever` landed at **M6.2**, driven by `new`/`query` RED.
  Minimal (`query` only); the future `HybridRetriever` (embeddings) implements the same trait.
- **D4 (transport-agnostic):** `query` returns a structured `QueryResult`; formatting + CLI/MCP
  transport live downstream, so the core stays adapter-agnostic.

## Status
- **M6.1 DONE (2026-06-11):** `preprocess_query` + `build_match_expression` + `escape_fts5_token`
  + `STOPWORDS`; 7 in-module unit tests; reviewer APPROVED; all four gates green.
- **M6.2 GREEN + APPROVED (2026-06-11):** `trait Retrieve` + `Retriever` + `query` (search/dedup/
  tie-break/file_filter); 7 integration tests in `tests/retriever_tests.rs` + 1 unit test; M6.1
  `#[allow(dead_code)]` removed. Gates verified green by main session. Token budget = M6.3.
- **M6.3 GREEN + APPROVED (2026-06-11):** `estimate_tokens` + `Retriever::apply_token_budget`
  wired into `query`; `--max-tokens` is a hard ceiling. 5 new integration tests + 1 unit test
  (`estimate_tokens_is_len_div_4_min_1`). Reviewer APPROVED. **Gates pending main-session
  verification** (manager subagent cannot run cargo).
- **M6.4 BENCH WIRED + APPROVED (2026-06-11):** `benches/query_bench.rs` + `Cargo.toml` `[[bench]]`.
  Synthetic ~100K-LOC seeded index (5 000 chunks ×20 LOC) built outside the timed closure; times only
  `Retriever::query`; `sample_size=100` (each sample = one query → p50≈median, p95/p99 from raw
  `sample.json`). p95<500ms tracked as a **baseline, not a CI gate** (hard gate = M10). Reviewer APPROVED.
  **Actual p50/p95/p99 + clippy/fmt/`cargo bench` + EXPLAIN QUERY PLAN baseline PENDING main-session run**
  (manager subagent cannot run cargo).
- **R2.2a / D24 GREEN (2026-06-14):** `QueryOptions.bm25_weights: Option<[f64; 7]>` added (+ `Default
  = None`); `query` routes it to `Storage::search_with_weights`. `None` keeps every existing retriever
  test's order (default-identical); `Some(custom)` changes ranking. +1 integration test
  (`bm25_weights_some_changes_ranking_vs_none`); all 13 retriever tests green; all four gates clean.
