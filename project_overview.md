# CodeCache — Director's Assessment, Landscape Research & Revised Plan
**Prepared:** June 11, 2026 · **For:** solo developer building with Claude Code · **Status of project:** M0–M5 complete, M6 in progress

---

## 1. Executive Verdict

You asked three questions: is this realistic for a solo developer, is it worth doing as research, and what should the metrics and benchmarks be. Here are the direct answers, with the full reasoning in the sections below.

**Is the engineering realistic solo?** Yes — and you've already proven it. M0 through M5 shipped in roughly two days with disciplined TDD and a 96-test suite. The remaining engineering (M6–M10) is the easier half. The plan as written is sound; this report proposes only deltas, not a rewrite.

**Is the product positioning still valid?** Partially. The landscape has moved significantly since your spec was drafted, and the single biggest finding of this research is that **your original framing — "indexed retrieval replaces context dumping" — is fighting the last war.** The industry debate is now *agentic search (grep-in-a-loop) versus indexed retrieval*, and agentic search won the default position: Claude Code's creator has publicly stated that early versions used RAG with a local vector DB, but agentic search outperformed it decisively while avoiding staleness, security, and reliability problems. However — and this is the opportunity — the emerging consensus is *hybrid*: Cursor found that combining a trained embedding index with grep produces the best outcomes, and practitioner commentary explicitly identifies "light local indexing + model-driven query refinement" as the unexplored sweet spot. **CodeCache should be repositioned from "RAG preprocessor" to "a deterministic, zero-dependency index *tool inside the agent's search loop*."** Your architecture already supports this (the MCP server is the loop-tool interface); only the framing and the evaluation design need to change.

**Is it worth doing as research?** Yes, conditionally. The novelty claim "AST chunking improves code retrieval" is **already published** — the cAST paper (EMNLP 2025 Findings) showed AST-based chunking lifts Recall@5 by 4.3 points on RepoEval and Pass@1 by 2.67 on SWE-bench. You cannot claim that as a contribution. What *is* open, timely, and tractable for one person is the **empirical question nobody has answered rigorously: where do grep-only agentic search, local lexical indexes, and embedding indexes sit on the tokens-latency-accuracy Pareto frontier when used as tools *inside* an agent loop, under explicit token budgets?** New benchmarks released in early 2026 (ContextBench, with verified gold contexts and file/block/line-level retrieval metrics) exist precisely because, as the authors argue, outcome-only benchmarks like SWE-bench offer limited visibility into whether agents succeed by finding the right information or by compensating with expensive exploration. A careful study with an open artifact is publishable at a workshop (LLM4Code, AIware, MSR), strong as an arXiv preprint, and — judging by how much attention indie benchmarking efforts in this exact space have attracted — likely to be widely read regardless of venue.

**The realistic deliverable bundle, end of summer 2026:** (a) `codecache` v0.1 — a single static binary, zero-dependency local index for coding agents; (b) an evaluation harness and dataset of agent-retrieval trajectories; (c) a preprint: *"Grep, Index, or Embed? The Token Economics of Context Retrieval Interfaces for Coding Agents."*

---

## 2. What Changed: Landscape Research Findings (June 2026)

Your spec's risk register listed "Anthropic could build this natively into Claude Code" as a competitive risk. The reality that materialized is subtler and more instructive. Five findings matter.

### 2.1 Agentic search won the default — by explicit A/B testing, not ideology

Claude Code does not index codebases. It uses a three-tool hierarchy — Glob for path matching, Grep for content search, Read for loading files — and spawns read-only Explore subagents with isolated context windows so heavy search doesn't pollute the main conversation. This was not a shortcut: the team built RAG into early versions, tested it head-to-head, and an Anthropic engineer confirmed publicly that agentic search outperformed it "by a lot," which surprised them. Boris Cherny's stated reasons go beyond accuracy: simplicity, plus avoiding the security, privacy, staleness, and reliability problems of a maintained index. Claude Code has since added native LSP support (v2.0.74, December 2025) with goToDefinition, findReferences, and call-hierarchy operations across 11 languages — though user reports suggest the LSP trigger rate in practice is low and the agent usually finds what it needs through text search.

**Implication for CodeCache:** any pitch that begins "agents waste tokens dumping files" is attacking a strawman in 2026 — competent agents don't dump files, they grep iteratively. The honest critique of agentic search is the one its own practitioners concede: it pays for its accuracy in tokens and latency, re-deriving the same codebase knowledge every session. That is the gap a cheap, deterministic, always-fresh local index can attack — *as a tool the agent calls*, not as a replacement for the loop.

### 2.2 The frontier moved to hybrid — and to trained retrieval models

Cursor's published position is that semantic search is currently necessary for the best results on large codebases, that their agent uses grep heavily as well, and that the combination wins — with semantic search improving accuracy by about 12.5% over grep alone on large codebases. Critically, their embedding model is trained on *agent session traces*: they observe what an agent eventually found useful, have an LLM rank what should have been retrieved earlier, and align the embedder to those rankings. Cognition went further with SWE-grep: small models RL-trained specifically for retrieval, executing up to 8 parallel tool calls per turn over a maximum of 4 turns using only grep/read/glob, matching frontier-model retrieval quality at an order of magnitude lower latency (the mini variant serves at 2,800+ tokens/sec). These ship today inside Windsurf's Fast Context subagent.

**Implication:** retrieval-for-agents is now an arms race of *trained* components. A solo project cannot compete on model training. It can compete on the axis the big players have abandoned: **deterministic, local-first, zero-model, zero-cloud, single-binary infrastructure** — and on *measurement*, since the trained systems are closed and their claims are largely unaudited.

### 2.3 Your exact product idea has shipped — twice — with different trade-offs

Two open-source MCP servers now occupy adjacent positions. **Zilliz's claude-context** is an MCP plugin doing hybrid BM25 + dense vector search over a Milvus/Zilliz Cloud index, and its controlled evaluation claims roughly 40% token reduction at equivalent retrieval quality — almost exactly your spec's headline target. Its cost: it requires a vector database (cloud account or Docker'd Milvus) and an embedding API (OpenAI or Voyage). **Serena** provides symbol-level retrieval and editing via language servers (LSP) over MCP across 20–30+ languages, is free and local, and is widely praised — but it requires per-language language-server toolchains and carries the operational weight of running them. Meanwhile, AST chunking itself is commoditized: the cAST authors ship `astchunk` on PyPI, and Supermemory released a `code-chunk` library built on the same paper.

**Implication:** the open niche is real but narrow: **"the SQLite of code context."** No vector DB, no API key, no embedding model, no language server, no Node/Python runtime — one Rust binary, one `.db` file, sub-second cold start, deterministic results, works air-gapped. claude-context can't follow you there (it's structurally cloud/embedding-dependent); Serena can't either (structurally LSP-dependent). That is a defensible product wedge *and* it is exactly the configuration that makes your research reproducible by anyone.

### 2.4 The evaluation infrastructure you need now exists

In your spec, validation meant "benchmark against Claude Code on 5 real tasks" — a sample size that would convince nobody. Since then: **ContextBench** (Feb 2026) provides verified gold contexts for hard tasks sampled from SWE-bench Verified, Multi-SWE-bench, SWE-PolyBench, and SWE-bench Pro, and evaluates recall/precision/F1 at the file, block, and line level alongside Pass@1 — its headline result is that state-of-the-art agents still struggle with context retrieval, which prior end-to-end benchmarking masked. **SWE-ContextBench** (1,476 tasks, 51 repos, 9 languages, with a 300-task Lite split) measures whether agents can reuse context across related tasks and whether that reuse reduces cost. **CodeRAG-Bench** supplies retrieval pools and NDCG/precision-recall protocols across BM25, dense, and proprietary retrievers. And independent work confirms the questions are live in both directions: one 2026 study found embedding-based retrieval beating AST-based retrieval for research-code agents (41.7% vs 33.3% pass rate), while an indie 60-task benchmark found tuned grep beating a bespoke index on F1 while the index won 62× on token economy — coining "tokens per correct answer" as the load-bearing metric for agents.

**Implication:** you no longer need to invent an evaluation methodology — you need to *apply* the new ones, which is exactly the right scope for a solo researcher. The conflicting results above (embeddings beat AST here, grep beats index there, index wins on tokens everywhere) are the signature of a field that has not been mapped systematically. Mapping it is the contribution.

### 2.5 One spec assumption is now wrong in your favor

Your spec lists MCP as "Custom (no SDK yet) — implement manually." There is now an official MCP Rust SDK (the `rmcp` crate, under the modelcontextprotocol GitHub org). Evaluate it before hand-rolling JSON-RPC in M8 — it likely cuts that milestone's effort substantially and de-risks protocol drift. Verify current API stability before committing; pin the version either way.

---

## 3. Repositioning: What CodeCache Should Claim to Be

The product sentence changes from *"a context retrieval engine that replaces file dumping"* to:

> **CodeCache is a zero-dependency, deterministic code index that coding agents call as a tool — replacing N rounds of grep with one structured lookup, with no embedding model, vector database, language server, or cloud account.**

Three consequences follow.

**The agent is the user, not the human.** Optimize the MCP tool descriptions, output format, and result granularity for what makes an *agent's next action* cheap: symbol name, qualified parent, file:line-range, and a one-line signature first; full bodies only within the token budget. Your TOON format and D7 (stored line numbers) already anticipate this. Add one tool you haven't specced: `codecache_outline` (return the symbol skeleton of a file or directory — the single cheapest way for an agent to orient, and what aider's repo-map proved valuable years ago).

**Freshness is the killer objection — kill it structurally.** The strongest argument against indexes is staleness (Cherny's stated reason for abandoning RAG). Your xxHash incremental design answers it, but only if re-indexing is invisible. Make `codecache_search` *self-healing*: before answering, hash-check the files implicated by the top results (cheap, you store hashes) and transparently re-index any that changed. A correct-by-construction index converts the best anti-index argument into your differentiator. This is a small M6/M8 work item with outsized strategic value.

**Don't compete with grep — compose with it.** Position the benchmark and the README honestly: grep is excellent, and the question is when a structured index *saves the agent turns*. This humility is also what makes the research credible.

---

## 4. Is This Worth Doing as Research? — Yes, With This Framing

### 4.1 What is *not* publishable

"AST chunking helps retrieval" (cAST published it), "BM25 over FTS5 is fast" (engineering, not science), "our tool reduces tokens 40%" as a standalone claim (claude-context already markets the same number; an unaudited self-benchmark is marketing). A pure systems-description paper about CodeCache itself would be a demo-track item at best.

### 4.2 The open question you are unusually well-positioned to answer

The field has three retrieval *interfaces* for coding agents and no controlled comparison of them under the metric that actually matters for agents — cost-to-correct-context:

1. **Agentic lexical search** — grep/glob/read in a reasoning loop (Claude Code default).
2. **Indexed lexical/structural search as a tool** — AST symbols + BM25, called from the same loop (CodeCache).
3. **Embedding/hybrid search as a tool** — dense or BM25+dense (claude-context-style), called from the same loop.

Every published comparison so far holds confounds: Cursor's +12.5% conflates a trained embedder with the hybrid interface; the agentic-vs-RAG experiments compare a *loop* against a *one-shot preprocessor* (interface and iteration confounded); ContextBench evaluates agents, not retrieval interfaces, and SWE-grep's evaluation set is internal. Because CodeCache is open, deterministic, and dependency-free, you can run the *same agent, same model, same prompts, same budget* with only the retrieval tool swapped — the clean ablation nobody has shipped.

**Research questions:**

- **RQ1 (efficiency):** At matched context recall, how many fewer tokens and tool turns does an agent need with an indexed structural search tool versus grep-only search? How does the gap scale with repository size (10K → 100K → 1M LOC)?
- **RQ2 (sufficiency of lexical):** How much of the embedding interface's quality advantage is recovered by metadata-enriched lexical retrieval (your D3 fields: parent symbol, imports, docstrings, cross-references) at zero model cost? (Literature anchor: on Python, BM25 is nearly at parity with dense retrievers — 0.64 vs 0.61 NDCG in one 2025 study — precisely because Python carries rich natural-language signal; the gap should widen for terser languages, which your M9 TS/Go support lets you test.)
- **RQ3 (interface shape):** One-shot top-k injection vs tool-in-loop with the *same* index: how do they trade off as the token budget sweeps 1K → 8K? (Hypothesis: one-shot wins at tiny budgets, tool-in-loop wins at moderate budgets, grep-only catches up only at large budgets.)
- **RQ4 (freshness, stretch):** Quantify the staleness penalty — agent outcomes with a deliberately stale index vs self-healing index vs live grep, on tasks immediately following synthetic edits.

### 4.3 Contribution statement (what the paper claims)

(1) The first controlled, same-agent comparison of retrieval *interfaces* (not retrievers) for coding agents under explicit token budgets, on public benchmarks with gold contexts; (2) a cost model — tokens-per-correct-context and tokens-per-resolved-task — that reframes the grep-vs-index debate as a budget-dependent frontier rather than a winner-take-all; (3) an open, reproducible artifact (single binary + harness + trajectories) enabling replication, which none of the commercial systems permit.

### 4.4 Venue reality check (solo author, no lab)

A top-tier main-track paper (ICSE/FSE/NeurIPS) is unlikely solo — those now expect multiple models, multiple seeds, human studies, or training contributions. Realistic and worthwhile targets, in order of effort: arXiv preprint with full artifact (baseline; do this regardless); workshop submission — LLM4Code @ ICSE, AIware, MSR technical/registered-report tracks, or FSE Demonstrations for the tool itself; and an industry-credible technical report in the style that Cursor/Cognition publish, which in this field circulates as widely as papers. The indie 60-task benchmark cited above demonstrates that careful solo measurement in this space gets read and cited; rigor and reproducibility, not venue, are what compound.

---

## 5. Evaluation Design: Benchmarks, Metrics, and the Experiment Matrix

### 5.1 Benchmarks to adopt (do not invent your own first)

| Benchmark | What it gives you | How you use it | Cost profile |
|---|---|---|---|
| **ContextBench / Lite** (2026) | Verified gold contexts; file/block/line-level retrieval metrics; sampled from SWE-bench Verified, Multi-SWE-bench, SWE-PolyBench, SWE-bench Pro | Primary retrieval-quality benchmark: score each interface's retrieved context against gold, *without* needing full agent runs for most ablations | Low (offline scoring) |
| **SWE-bench Verified subset** (Python; 50–100 tasks) | End-to-end Pass@1 with real repos and test suites | Downstream validation for the top 2–3 configurations only | High (LLM API spend) |
| **SWE-ContextBench Lite** (300+99 tasks) | Context *reuse* across related tasks; cost-efficiency framing | Stretch: tests whether a persistent index amortizes across related tasks — the thesis of an index | Medium |
| **CodeRAG-Bench** (RepoEval slice) | Standardized retrieval pools; NDCG protocol; BM25/dense baselines already implemented | Sanity-check your retriever against published BM25/dense numbers; comparability with cAST results | Low |
| **Your own micro-suite** (5 repos × ~15 queries, hand-verified) | Realistic developer queries (the kind FTS5 sees in practice) on Django-scale code | Latency/index-size budget validation (your existing §1.3 targets) and qualitative error analysis | Low |

### 5.2 Metric system (three layers — report all three, lead with the middle one)

**Layer 1 — Retrieval quality** (offline, against gold contexts): Recall@k, Precision@k, F1 at file / block(function) / line granularity (ContextBench protocol); NDCG@10 (CodeRAG-Bench protocol). These establish you're not trading quality for cost.

**Layer 2 — Token & turn economy** (the headline; agent-in-the-loop): *tokens-to-correct-context* — cumulative prompt+completion tokens consumed from task start until the agent's context first covers the gold set at ≥ some recall threshold; *tokens-per-resolved-task*; *tool turns to coverage*; *tokens per correct answer* on Q&A-style tasks. This layer is where an index should win and where the field currently argues with anecdotes.

**Layer 3 — Systems costs** (your existing §1.3 budgets, kept): query p50/p95/p99 latency (<500ms p95 @ 100K LOC); cold index time; incremental update (<2s / 10 files); index size (<100MB @ Django scale); plus two new ones — *time-to-first-relevant-result* (end-to-end, what SWE-grep optimizes) and *staleness window* (max age of any indexed chunk during a self-healing query).

**Reporting discipline:** every Layer-2 number with N, mean, and bootstrap CI; fixed model + temperature + prompt across arms; ≥3 runs per (task, config); publish raw trajectories. Paired bootstrap with multiple-comparison correction is the current standard in retrieval benchmarking papers — match it.

### 5.3 Experiment matrix

Arms (same agent harness, same model, only the retrieval tool swapped):

| Arm | Retrieval interface | Implements |
|---|---|---|
| A0 | grep/glob/read only (control) | Claude Code-style agentic search |
| A1 | A0 + `codecache_search` (AST+BM25, plain) | Index-as-tool, no enrichment |
| A2 | A1 with D3 metadata enrichment ON | Your recall hypothesis (RQ2) |
| A3 | A0 + embedding search tool (off-the-shelf code embedder over the *same chunks*) | Embedding interface, chunking held constant |
| A4 | One-shot top-k injection from A2's index (no loop access) | Classic RAG baseline (RQ3) |
| A5 (stretch) | A2 + A3 hybrid via reciprocal-rank fusion | The Cursor-shaped hybrid, untrained |

Sweeps: token budget {1K, 2K, 4K, 8K}; repo scale {~10K, ~100K, ~1M LOC}; language {Python now; TS/Go after M9}. Ablations within A2: chunking strategy (your function-level vs cAST split-merge vs fixed 32-line windows — `astchunk` exists, reuse it for the baseline) and per-column BM25 weights.

### 5.4 Budget estimate (the constraint that shapes everything)

Offline Layer-1 scoring is nearly free. The expensive part is Layer-2 agent runs: roughly (100 tasks × 6 arms × 3 runs) ≈ 1,800 trajectories; at a realistic $0.50–$2.00 per trajectory with a mid-tier model, that's **$900–$3,600**. Control it by: running the full matrix on ContextBench-Lite-sized subsets (30–50 tasks) first; promoting only arms that separate beyond CI overlap to the full set; using a cheaper model for the retrieval-phase agent (defensible — SWE-grep proves retrieval needs less model than repair); and capping turns. Plan for ~$1K of API spend as a real line item; it is the actual cost of the research.

---

## 6. Revised Plan (deltas only — your M0–M10 stand)

Your engineering plan, decision log, and TDD discipline are genuinely strong — better than most funded teams produce. Keep all of it. The changes are three insertions and one reordering.

| Change | What | When | Why |
|---|---|---|---|
| **Δ1** | Add `codecache_outline` tool + agent-first output ordering (signature/skeleton before bodies) | M7/M8 | §3: the agent is the user; cheapest orientation primitive |
| **Δ2** | Self-healing search (hash-check + transparent re-index of result files at query time) | M6 follow-up or M8 | §3: structurally kills the staleness objection; small (you already store hashes) |
| **Δ3** | Evaluate official MCP Rust SDK (`rmcp`) vs hand-rolled JSON-RPC | M8 entry | §2.5: spec's "no SDK" assumption is stale |
| **Δ4** | Replace M10's "5 real tasks" success criterion with §5's benchmark suite; the ≥40% token-reduction target becomes "Layer-2 dominance over A0 at matched Layer-1 recall, with CIs" | M10 | §4: 5 tasks convinces nobody; the field has real benchmarks now |

**New research track (after M8; M9 can interleave or follow):**

| Milestone | Work | Exit criteria | Est. effort |
|---|---|---|---|
| **R1 — Harness** | Minimal agent loop (or mini-SWE-agent fork) with pluggable retrieval tools; trajectory logging; ContextBench gold-context scorer | One task runs end-to-end in all of A0/A1/A4; metrics computed from logs | 2–3 wks |
| **R2 — Offline ablations** | Layer-1 sweeps: chunking × ranking × enrichment on ContextBench-Lite + RepoEval slice | Reproduce published BM25 baselines within tolerance; pick top configs | 1–2 wks |
| **R3 — Agent-in-loop study** | Full matrix on 30–50 tasks; promote winners to 100; budget/scale sweeps | All RQ1–RQ3 plots with CIs; raw trajectories published | 3–4 wks + ~$1K API |
| **R4 — Write-up & release** | Preprint + artifact (binary, harness, data); blog distillation; submit to one workshop | arXiv live; artifact reproduces headline figure from a clean machine | 2 wks |

**Realistic total timeline:** M6–M10 ≈ 2–4 weeks at your demonstrated pace; R1–R4 ≈ 8–11 weeks. Working solo with Claude Code, end-to-end completion by **late September 2026** is aggressive-but-achievable; budget slack to November.

---

## 7. Risks, Honest Trade-offs, and Kill Criteria

**The null result is live.** It is genuinely possible that A1/A2 do *not* beat grep-only for a strong model — Anthropic's internal testing found exactly that for one-shot RAG, and frontier models keep getting better at search. Protect yourself two ways: (a) a rigorous null result on this question is *itself publishable and useful* ("when does indexing pay?" answered with "rarely, and here's the boundary" still maps the frontier); (b) the index's clearest theoretical edge — amortization across queries and sessions, and small-model agents that can't drive grep skillfully — should be explicit arms, not afterthoughts. If the index can't beat grep even for a cheap model on a 1M-LOC repo at a 2K budget, that's your kill criterion for the *product* thesis (the research deliverable survives regardless).

**Crowding risk is real but bounded.** Serena (LSP) and claude-context (vector) bracket you; your wedge is zero-dependency determinism. If either ships a no-dependency local mode, your product differentiation narrows to performance + the research credibility — acceptable, since the research is the durable asset.

**Scope discipline is your biggest personal risk.** The existing decision log shows excellent restraint (D1 deferring embeddings, the `is_heuristic` persistence deferral). Apply the same restraint to the research: RQ4 and SWE-ContextBench are stretch goals; cut them first. One clean answer to RQ1–RQ3 beats four muddy ones.

**Benchmark contamination & validity:** SWE-bench-derived tasks are heavily targeted by model training; mitigate by reporting retrieval metrics (which contamination affects less than patch generation), including your hand-verified micro-suite on post-cutoff repo snapshots, and noting model/version in every figure.

---

## 8. Bottom Line

Build it, but build it as two coupled artifacts: a tool with one ruthless differentiator (zero-dependency determinism, self-healing freshness) and a measurement study the field actually lacks (retrieval *interfaces* for agents, compared clean, costed in tokens). The engineering is on track and the hard thinking — your decision log — is already at a professional standard. The single most important change this report asks for is strategic, not technical: stop benchmarking against "context dumping," start benchmarking against grep-in-a-loop, and let the data decide where your index earns its keep.

---

## 9. Key Sources

- Boris Cherny / Anthropic on abandoning RAG for agentic search (Latent Space, May 2025; HN engineer confirmation): vadim.blog/claude-code-no-indexing; zerofilter.medium.com (Apr 2026)
- Claude Code LSP support v2.0.74 & grep-backbone analysis: yage.ai/share/why-coding-agents-still-use-grep-en-20260327
- Cursor, "Improving agent with semantic search" (Nov 2025): cursor.com/blog/semsearch
- Cognition, "Introducing SWE-grep" (Oct 2025): cognition.ai/blog/swe-grep; docs.windsurf.com/context-awareness/fast-context
- Zilliz claude-context (hybrid BM25+dense MCP, ~40% token-reduction claim): github.com/zilliztech/claude-context; milvus.io blog (Aug 2025)
- Serena (LSP-based symbol retrieval/editing MCP): github.com/oraios/serena
- Sourcegraph, "Agentic Coding in 2026" (SCIP deterministic code intel via MCP): sourcegraph.com/blog/agentic-coding
- cAST: Structural chunking via AST, EMNLP 2025 Findings (arXiv:2506.15655); `astchunk` on PyPI; Supermemory code-chunk
- ContextBench: gold-context retrieval benchmark for coding agents (arXiv:2602.05892, Feb 2026)
- SWE-ContextBench: context reuse benchmark (arXiv:2602.08316, Feb 2026)
- CodeRAG-Bench: github.com/code-rag-bench/code-rag-bench
- Practical Code RAG at Scale (task-aware retrieval design; BM25 vs dense NDCG by language): arXiv:2510.20609
- Embedding vs AST retrieval for research-code agents (41.7% vs 33.3%): arXiv:2506.19724
- Indie 60-task retrieval benchmark; "tokens per correct answer": sverklo.com/blog/i-benchmarked-code-retrieval-for-ai-agents
- Agentic search overview & CodeSearchEval: morphllm.com/agentic-search