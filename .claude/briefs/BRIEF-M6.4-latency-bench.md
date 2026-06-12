# BRIEF — M6 / M6.4 — query-latency bench (skeleton)

- **Milestone:** M6 — retriever  ·  **Module(s):** `benches/query_bench.rs`
- **Owner (manager):** principal-engineering-manager  ·  **Created:** 2026-06-10
- **Status:** PERF ▢  REVIEW ▢  DONE ▢  (blocked by M6.3 — needs the full `query` path)
- **Links:** docs/ROADMAP.md#m6--retriever · docs/plans/M6-retriever.md#slice-m64--latency-bench-perf · project_plan.md §1.3 / §11.2
- **Routing:** **performance-bench-engineer** (PERF) → code-reviewer. Manager verifies budget tracking.

## Goal
Wire a criterion latency bench over a synthetic ~100K-LOC-scale seeded index; measure p50/p95/p99
(§11.2) for the full `Retriever::query` path. Track the headline budget **p95 < 500ms** (§1.3).
Carry the FTS5 EXPLAIN QUERY PLAN baseline from M1. The full budget gate is finalized at M10; M6.4
wires + tracks (skeleton), it does not yet hard-fail CI on the budget.

## Scope (in / out)
- **In:** `benches/query_bench.rs` skeleton; synthetic-index seeding helper; p50/p95/p99 reporting;
  documented current numbers vs the 500ms p95 budget; note the §11.2 warm breakdown targets.
- **Out:** full criterion suite + hard budget gate → M10; token-reduction benchmark → M10.

## Scenarios / measurements
- [ ] cold-cache p95 over 100K-LOC synthetic index < 500ms (track; record actual)
- [ ] warm breakdown sanity vs §11.2 targets (FTS5 <50ms, BM25 <10ms, snippet <20ms, tokens <10ms)
- [ ] `max_results` bounded (default 20) so in-flight chunks stay ~10MB cap (§11.3)

## Definition of Done
- [ ] Bench compiles + runs via `/bench`; p95 recorded vs budget · clippy/fmt clean
- [ ] reviewer APPROVED · docs/TODO.md Phase 6 (M6.4) + src/retriever/CLAUDE.md (perf line) updated
- [ ] M10 follow-up noted: promote to hard budget gate in the full suite

---
## PERF — performance-bench-engineer

## REVIEW — code reviewer

## OUTCOME — manager
