# CodeCache — Phase Plans

Granular, per-milestone execution plans for v0.1. **Owner:** `principal-engineering-manager`.

These plans turn the milestones in [`../ROADMAP.md`](../ROADMAP.md) into concrete, sliceable
work the agent team can execute test-first. They are the durable reference each agent consults
before a slice; the per-slice hand-off blackboard remains `.claude/briefs/BRIEF-<m>-<slice>.md`.

## How to read these
- One file per milestone: `M0-scaffolding.md` … `M10-benchmarks-release.md`.
- Build order is **bottom-up** (`../ENGINEERING_PLAN.md` §2). Do not start a milestone before
  its dependency milestones are `[x]` in [`../TODO.md`](../TODO.md).
- Each plan is the *what/how* for that milestone; the *why* lives in
  [`../project_plan.md`](../project_plan.md), the *gates* in `../ENGINEERING_PLAN.md` §4–5.

## Plan structure (every file)
1. **Milestone ID & name** — from `../ROADMAP.md`.
2. **Goal / acceptance criteria** — the milestone exit criteria, made checkable.
3. **Modules & files** — create/modify, aligned to `../ENGINEERING_PLAN.md` §2 + `../project_plan.md` §3.1 / §10.4.
4. **Dependencies** — prior milestones + external crates that land here.
5. **Ordered slices** — each slice is one TDD cycle: RED (test-lead) → GREEN (eng-lead) →
   PERF (if applicable) → REVIEW (code-reviewer) → INTEGRATE (manager).
6. **API contracts / data structures** — from `../project_plan.md` §3.2 / §4.
7. **Performance budgets** — from `../project_plan.md` §5.4 / §11 + `../TEST_STRATEGY.md`.
8. **Decision Log bindings** — which Decision Log dispositions (D1–D16) this milestone must honor.
9. **Definition of Done** — the per-milestone checklist.

## Index
| Plan | Milestone | Modules | Depends on |
|---|---|---|---|
| [M0-scaffolding.md](M0-scaffolding.md) | M0 — Scaffolding & CI | project layout, CI | — |
| [M1-config-storage.md](M1-config-storage.md) | M1 — config + storage | `config`, `storage` | M0 |
| [M2-hasher.md](M2-hasher.md) | M2 — hasher | `hasher` | M1 |
| [M3-parser-python.md](M3-parser-python.md) | M3 — parser (Python) | `parser` | M0 |
| [M4-chunker.md](M4-chunker.md) | M4 — chunker | `chunker` | M3 |
| [M5-indexer.md](M5-indexer.md) | M5 — indexer | `indexer` | M1, M2, M3, M4 |
| [M6-retriever.md](M6-retriever.md) | M6 — retriever | `retriever` | M1 |
| [M7-formatter-cli.md](M7-formatter-cli.md) | M7 — formatter + cli | `formatter`, `cli` | M5, M6 |
| [M8-mcp-server.md](M8-mcp-server.md) | M8 — mcp_server | `mcp_server` | M6, M7 |
| [M9-typescript-go.md](M9-typescript-go.md) | M9 — TypeScript + Go | `parser` (TS/Go) | M3, M4, M5 |
| [M10-benchmarks-release.md](M10-benchmarks-release.md) | M10 — Benchmarks + Release | `benches/`, release | all |

**Replan 2026-06-11** ([`../../project_overview.md`](../../project_overview.md), ROADMAP D12–D16):
M7 adds agent-first output ordering (D13); M8 adds the `rmcp` entry spike (D15, slice M8.0),
`codecache_outline` (D13), and self-healing search (D14, slice M8.4); M10's token-reduction
benchmark is replaced by Layer-1 gold-context scoring (D16). The post-M8 **research track
(R1–R4)** is tabled in `../ROADMAP.md`; per-milestone R-plans will be written when R1 is briefed.

## Critical path
```
M0 ─► M1 ─► M2 ─┐
           │    ├─► M5 ─► M7 ─► M8
M0 ─► M3 ─► M4 ─┘         │
      M1 ─► M6 ───────────┘
M3,M4,M5 ─► M9
all ─► M10
```
`M3` (parser) can proceed in parallel with `M1`/`M2` since it only depends on M0 scaffolding.
`M6` (retriever) only needs `M1` storage and can be built in parallel with the M3→M4→M5 chain.

## Maintenance contract
- When a milestone completes, the manager flips its TODO items to `[x]` and notes the plan as
  executed (not deleted — plans stay as the historical record).
- If a slice forces an API change, update `../project_plan.md` §3.2 **first**, then the plan,
  then the brief — never diverge silently.
- New design decisions ⇒ append to the `../ROADMAP.md` Decision Log and cite it in the plan.
