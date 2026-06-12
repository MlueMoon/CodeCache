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

## Decision Log bindings
- **D1 (trait):** the `Retriever` struct + minimal `trait Retrieve` are **deferred to M6.2**, where
  `new`/`query` RED tests drive them — M6.1 is pure string logic with no `Storage`, so landing the
  type now would be undriven production surface (TDD). The future `HybridRetriever` wraps the trait.

## Status
- **M6.1 DONE (2026-06-11):** `preprocess_query` + `build_match_expression` + `escape_fts5_token`
  + `STOPWORDS`; 7 in-module unit tests; reviewer APPROVED; all four gates green.
- **M6.2–M6.4:** pending — BM25 search/dedup (introduces the `Retriever` struct + D1 trait), token
  budget packing, latency bench.
