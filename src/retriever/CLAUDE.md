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
Query latency budget p95 < 500ms on 100K LOC (project_plan §11.2). Bench wired by
`performance-bench-engineer` at M6.4 / M10.

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
- `QueryOptions { max_tokens, max_results, file_filter }` (+ `Default` = 4000/20/None, §3.2.3).
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
  `#[allow(dead_code)]` removed. **Gates pending main-session verification** (manager subagent
  cannot run cargo). Token budget = M6.3.
- **M6.3–M6.4:** pending — token-budget packing, latency bench.
