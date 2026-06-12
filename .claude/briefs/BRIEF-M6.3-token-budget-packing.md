# BRIEF — M6 / M6.3 — token budget packing (skeleton)

- **Milestone:** M6 — retriever  ·  **Module(s):** `retriever`
- **Owner (manager):** principal-engineering-manager  ·  **Created:** 2026-06-10
- **Status:** RED ▢  GREEN ▢  REVIEW ▢  DONE ▢  (blocked by M6.2)
- **Links:** docs/ROADMAP.md#m6--retriever · docs/plans/M6-retriever.md#slice-m63--token-budget-packing · docs/TEST_STRATEGY.md#retriever · project_plan.md §3.2.3 / §6.3
- **Routing:** test-lead (RED) → engineering-lead (GREEN) → code-reviewer. (No FTS5/perf for this slice; perf is M6.4.)

## Goal
Greedily pack ranked results within `max_tokens` and assemble the final
`QueryResult { chunks, total_tokens, total_results_found }` (§6.3). Token estimate is the §6.3
char heuristic `(text.len() / 4).max(1)` — no tokenizer crate in v0.1.

## Scope (in / out)
- **In:** `apply_token_budget(results, max_tokens) -> Vec<SearchResult>`; `estimate_tokens(text)`;
  full `QueryResult` assembly; `total_results_found` = pre-budget count.
- **Out:** formatting/transport (M7); embeddings (D1, v0.2).

## Scenarios to cover (from plan §6.3 / TEST_STRATEGY#retriever)
- [ ] `packing_never_exceeds_max_tokens`
- [ ] `greedy_stops_at_budget_keeping_top_ranked` (top-first; stop when next won't fit)
- [ ] `total_tokens_reported_matches_sum_of_packed`
- [ ] `total_results_found_reflects_pre_budget_count`
- [ ] `estimate_tokens_is_len_div_4_min_1` (§6.3, incl. empty/short text → min 1)
- [ ] edge: a single chunk larger than the whole budget (first-result behavior — define + assert)

## Definition of Done
- [ ] Tests green · clippy -D warnings · fmt clean · API matches §3.2.3 / §6.3
- [ ] `--max-tokens` never exceeded (the headline correctness exit) · reviewer APPROVED
- [ ] docs/TODO.md + src/retriever/CLAUDE.md updated

---
## RED — test lead

## GREEN — engineering lead

## Specialist / Perf notes

## REVIEW — code reviewer

## OUTCOME — manager
