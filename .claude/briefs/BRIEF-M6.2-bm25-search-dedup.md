# BRIEF — M6 / M6.2 — BM25 search + determinism + dedup (skeleton)

- **Milestone:** M6 — retriever  ·  **Module(s):** `retriever` (uses `storage::search`)
- **Owner (manager):** principal-engineering-manager  ·  **Created:** 2026-06-10
- **Status:** RED ▢  GREEN ▢  REVIEW ▢  DONE ▢  (blocked by M6.1)
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

## GREEN — engineering lead

## Specialist / Perf notes

## REVIEW — code reviewer

## OUTCOME — manager
