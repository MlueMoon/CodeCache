# Verifying the CodeCache Research Track

A reviewer's guide to **independently verifying** CodeCache's research claims (tracks R1–R2). It
tells you what is and isn't being claimed, how to re-run every result, what output is expected, what
counts as pass/fail, and which scientific caveats to keep front of mind.

It is written for the skeptical reader: a program-committee reviewer, an internal research lead, or
a future maintainer who needs to trust the numbers before building on them.

> Companion docs: research harness internals — [`../research/r1_harness/README.md`](../research/r1_harness/README.md);
> research-track rules — [`../research/CLAUDE.md`](../research/CLAUDE.md); the milestone Decision Log
> (D12–D31) — [`ROADMAP.md`](ROADMAP.md); positioning + evaluation design —
> [`../project_overview.md`](../project_overview.md).

---

## 1. What is being claimed — and what is *not*

CodeCache's research track studies **retrieval interfaces for coding agents**: does giving an agent a
structured AST+BM25 index (as a tool, or as one-shot injected context) change how efficiently it
covers the code it needs, versus grep-only? The track is deliberately staged, and **scope discipline
is part of the design** ([`project_overview.md`](../project_overview.md) §7).

| Track | What it builds | What it claims | What it does **not** claim |
|---|---|---|---|
| **R1** | An eval harness: the same agent run in arms **A0** (grep only), **A1** (+ `codecache query` in-loop), **A4** (one-shot top-k injection), scored on Layer-1 (retrieval quality) + Layer-2 (token/turn economy) from trajectory logs. | The apparatus runs end-to-end, deterministically, and measures the right things. | **No arm winner.** Which interface "wins" is an **R3** determination, gated behind a separate (~$1K) API budget. |
| **R2** | Offline ablations reusing R1's scorer/corpus over a process boundary to the binary: NDCG@10 (R2.1), a BM25 weight sweep (R2.2), a chunker A/B vs a stub and vs the cAST/astchunk baseline (R2.3/R2.6), an external corpus loader (R2.5a), and a scoped real-corpus exit run over ContextBench-Lite (R2.7). | **Directional, proxy** signals + a validated apparatus. R2.2's headline is a **null result** ("the shipped default weights are not beaten on the micro-suite"). | No published weights finding, no full-benchmark result, no winner. R2 is explicitly "validate the apparatus + get a directional read," not a paper result. |

**The single most important thing to verify is that the harness honors this discipline** — that no
output silently overclaims. The code is written to make this checkable: every run entrypoint prints
a scope/honesty disclaimer, and the reported numbers are labelled "directional/proxy," not "result."

---

## 2. Verification philosophy (why you can trust the numbers)

Three properties make the research auditable; verifying them is most of the job.

1. **Two independent scorer implementations, cross-checked.** Layer-1 metrics (Recall/Precision/F1
   @k, file + block) exist twice: the Rust scorer in
   [`tests/retrieval_quality.rs`](../tests/retrieval_quality.rs) (the M10.2 "source of truth") and a
   Python port in [`research/r1_harness/r1harness/scorer.py`](../research/r1_harness/r1harness/scorer.py).
   The Python unit tests (`tests/test_scorer.py`) **mirror the Rust scorer's hand-computed metric
   tests**, so the two stay behaviorally identical (Decision Log **D21**). If a metric were wrong, the
   two implementations would have to be wrong *identically* to both stay green.

2. **Hermetic test suite.** The `pytest` suite never touches the network and never needs the live
   binary for metric correctness: scorer/trajectory/corpus/selector logic is pure and unit-tested
   with fixtures and mocks. Network/git/binary are exercised only by the **run entrypoints**, not the
   gates. So "are the metrics correct" is verifiable completely offline.

3. **One gold source.** Layer-1 gold contexts come from
   [`tests/fixtures/retrieval_quality/`](../tests/fixtures/retrieval_quality/) — the *same* fixture the
   Rust scorer uses. R2 "swaps in the real corpus, scorer unchanged" (D21).

---

## 3. Prerequisites

```bash
# From the repo root.
# (1) Build the binary the harness shells out to (process boundary — no FFI):
cargo build --release                     # → target/release/codecache  (or set $CODECACHE_BIN)

# (2) Create the research venv (system python is PEP-668 externally-managed; do NOT use it):
python3 -m venv research/r1_harness/.venv
research/r1_harness/.venv/bin/pip install -r research/r1_harness/requirements.txt
```

The venv is **required**: since R2.6 the suite imports `astchunk` (MIT) at runtime and fails on a
system Python that lacks it. `.venv/`, `cache/`, and `runs/` are gitignored — none are committed.

---

## 4. Layer 0 — the gates (what "green" means)

Run the two research gates. **This is the baseline every other claim builds on.**

```bash
cd research/r1_harness
.venv/bin/ruff check .                                   # lint gate
PYTHONUTF8=1 .venv/bin/python -m pytest -q                # test gate
```

**Expected (the green baseline):**

```
All checks passed!                 # ruff
166 passed, 1 skipped              # pytest
```

- **`1 skipped`** is by design — a Windows-only path test that is inapplicable on Linux/macOS.
- The **skip count is the signal**: a failure, an error, or a *changed skip count* is the real
  regression. A growing `passed` count is normal as the harness gains tests.
- Pass criterion: **0 failed, 0 errored, exactly 1 skipped, ruff clean.**

> The cloned ContextBench repos under `cache/contextbench_repos/` carry their own test files;
> `pyproject.toml`'s `norecursedirs` keeps pytest collecting only `tests/`, so the baseline is stable
> regardless of what has been materialized.

---

## 5. Verify the metric implementations directly

Beyond the suite passing, you can audit the metrics by hand — this is where a reviewer earns trust.

### 5.1 Read the definitions and confirm they're standard

[`scorer.py`](../research/r1_harness/r1harness/scorer.py) implements:

- **Recall@k** = |G ∩ R_k| / |G| (empty gold ⇒ 1.0)
- **Precision@k** = |G ∩ R_k| / min(k, |R|) (short lists not penalized; 0 if |R|=0)
- **F1@k** = 2·P·R / (P+R) (0 when P+R=0)
- **NDCG@k** = DCG@k / IDCG@k with binary relevance, DCG = Σ rel_i / log₂(i+1) (1-based rank);
  IDCG ranks all |gold| relevant items first; empty gold ⇒ 1.0. (NDCG is an **R2 extension** beyond
  the M10.2 protocol — the Rust scorer does not compute it; this is stated in the module docstring.)

Each is scored at two granularities — **file** (`file_path`) and **block** (`(file_path,
symbol_name)`) — at k ∈ {1, 5, 10}, then **macro-averaged** (mean of per-query metrics).

### 5.2 Run only the metric tests and spot-check the math

```bash
cd research/r1_harness
PYTHONUTF8=1 .venv/bin/python -m pytest tests/test_scorer.py -v
```

These mirror the Rust `retrieval_quality.rs` hand-computed cases. Cross-read the two files: the same
gold/retrieved vectors should produce the same recall/precision/F1 numbers in both languages.

### 5.3 (Optional) drive a value through by hand

```bash
cd research/r1_harness
.venv/bin/python - <<'PY'
from r1harness.scorer import ndcg_at_k, recall_at_k
gold = {"a", "b"}
ret  = ["x", "a", "b"]          # gold at ranks 2 and 3 (1-based)
# DCG = 1/log2(3) + 1/log2(4); IDCG = 1/log2(2) + 1/log2(3)
print("recall@3 =", recall_at_k(ret, gold, 3))   # 1.0
print("ndcg@3   =", round(ndcg_at_k(ret, gold, 3), 4))
PY
```

Confirm the printed NDCG equals your own pen-and-paper `(1/log2 3 + 1/log2 4)/(1/log2 2 + 1/log2 3)`
≈ `0.7039`. If it does, the order-sensitivity and normalization are correct.

---

## 6. Reproduce R1 — the end-to-end apparatus (offline, zero cost)

R1's exit is "all three arms run end-to-end, log trajectories, and cover the gold block" — **no
winner**. The offline validator drives mini-SWE-agent's loop with a *deterministic scripted model*,
so it costs nothing and is reproducible.

```bash
cd research/r1_harness
# Needs the mini-swe-agent venv (see docs/TESTING_AND_USAGE.md §3.2 for the short-path note on Windows).
PYTHONUTF8=1 .venv/bin/python validate_offline.py
```

**Expected shape** (the numbers are deterministic-script artifacts — *not* an arm-winner claim):

```
arm   R@1 file  R@1 blk  F1@10 blk  turns→cov   tok→cov  tot tok
A0        1.00     1.00       0.67          1       126      613
A1        1.00     1.00       0.40          1       161     1037
A4        1.00     1.00       0.40          1       162      462
OK: all three arms ran end-to-end, logged trajectories, and covered the gold block.
```

**Pass criteria:**
- Exit code 0 and the final `OK:` line.
- Every arm's `R@1 blk` (block recall@1) reaches `1.00` — i.e. each arm covered the gold block.
- Outputs land in `runs/<arm>/trajectory.jsonl` + `runs/report.json` (gitignored).

**What this proves / does not prove:** it proves the plumbing (agent loop → bash/`codecache`
actions → trajectory extraction → Layer-1/Layer-2 scoring) is correct and deterministic. It does
**not** rank the arms — the token/turn differences above are scripted, not measured behavior.

A live, still-zero-cost variant (`run_live.py`, local Ollama) exists for observation only; see
[`docs/TESTING_AND_USAGE.md`](TESTING_AND_USAGE.md) §3.4. It is not part of any claim.

---

## 7. Reproduce R2.2 — the BM25 weight sweep (a null result)

R2.2 sweeps 6 per-column BM25 weight vectors over the 15-query micro-suite and macro-averages
Layer-1 + NDCG@10 into one row per vector.

```bash
cd research/r1_harness
PYTHONUTF8=1 .venv/bin/python run_sweep.py        # → runs/sweep/report.json + a ranked table
```

**Expected finding (directional, PROXY — explicitly *not* a published result):** the shipped default
`10,1,1,5,2,2,2` is **not beaten** — `default`/`flat`/`body_heavy`/`name_strong`/`enrich_heavy` tie at
NDCG@10 (block) ≈ **0.822**; only the degenerate `name_only` (`10,0,0,0,0,0,0`) degrades (≈ 0.672).

**How to verify this is honest, not cherry-picked:**
- The grid is fixed and visible in [`sweep.py`](../research/r1_harness/r1harness/sweep.py) (`DEFAULT_GRID`).
- The flag *is* applied — raw `bm25` scores differ across vectors — but the **gold blocks order the
  same**, and Recall@10 saturates because the top-10 ≈ the whole ≤9-chunk micro-corpus. That
  saturation is *why* the micro-suite can't separate reasonable weightings, and it is the stated
  empirical motivation for the gated real-corpus run (R2.5–R2.7).
- The README and the run output both label this "validates the apparatus; **not** a weights finding."

A null result that is correctly scoped is a legitimate outcome. The thing to confirm is that the doc
and the output **say** it's a null/proxy result — they do.

---

## 8. Reproduce R2.7 — the scoped real-corpus exit (needs network + git)

R2.7 is the real-corpus test of the apparatus: materialize a small, deterministic slice of
ContextBench-Lite (≤3 repos, ≤15 py/ts tasks), then run two ablations — a BM25 weight sweep (native
chunker) and a chunker A/B (native vs astchunk) — at file-level NDCG@10/F1@10/Recall@10.

```bash
cd research/r1_harness
# One-time: fetch the ContextBench-Lite slice (one cached, no-auth-token HF download — D26 envelope).
.venv/bin/pip install datasets huggingface_hub
PYTHONUTF8=1 .venv/bin/python fetch_contextbench.py --force --n-records 500
# Then the exit run (clones each task's repo at its base_commit; zero paid spend):
PYTHONUTF8=1 .venv/bin/python run_contextbench_exit.py
```

**Network boundary:** the run does a one-time `git clone` per repo (GitHub git egress) and one cached
HF dataset download. This is the research-harness-only **D26 envelope — zero paid spend**. The product
(codecache binary) stays fully air-gapped; only the harness clones. Cloned repos and blobs go to
`cache/` (gitignored) and must **never** be committed.

**Pass / honesty criteria** (this run is *outcome-agnostic* by design):
- Exit 0; two Markdown tables (Table 1: weight sweep; Table 2: chunker A/B) + a `runs/contextbench_exit/report.json`.
- The header and `scope_note` say "Scoped/directional exit — NOT full-500 ContextBench-Lite. No winner asserted."
- **Determinism to verify:** the task selection is a pure function — filter py/ts → stable sort
  `(repo, instance_id)` → greedy repo-cap → task-cap (see
  [`contextbench_corpus.py`](../research/r1_harness/r1harness/contextbench_corpus.py) `select_tasks`).
  Re-running with the same flags selects the **same tasks**. Materialization is idempotent (reuses
  clones/worktrees), and each task is pinned to its `base_commit`, so the corpus does not drift.
- **A/B fairness to verify:** in Run 2, both arms operate on the *same* enumerated source-file set
  (primary extension, 500-file cap) copied into *separate* scratch dirs with separate `.codecache/`
  DBs (read the `_score_task_ab` docstring). The shared worktree is never indexed during the A/B run.
  Documented confounds are disclosed in the table footnote (the astchunk lead is python-driven; the
  `.ts`-only filter excludes `.tsx`/`.vue` — a coverage artifact, not a chunker signal).

If a repo fails to clone, the run logs `SKIPPED` and continues (typed `CorpusMaterializeError`); it
never crashes. A missing cache exits non-zero with instructions.

---

## 9. Scientific caveats a reviewer must keep in mind

These are stated by the project itself; verify the docs match them, and don't let any downstream
summary forget them.

1. **The micro-suite is a 15-query proxy.** Recall saturates on its ≤9-chunk corpora, so it cannot
   separate reasonable rankings/chunkers. Any micro-suite number is *directional only*.
2. **R2.7 is scoped (n≈10 tasks), not the full benchmark.** It is a feasibility/apparatus test on real
   repos, not an evaluation. n is small; differences within noise are not winners.
3. **No arm-winner anywhere in R1/R2.** That determination is **R3** and is gated behind an explicit,
   un-spent ~$1K API budget. Treat any "A1 beats A0" reading as out of scope until R3.
4. **Block-level metrics are excluded from the R2.7 headline** because ContextBench gold has no
   symbol names (and astchunk synthesizes names), which would make block scores meaningless for that
   arm. File-level is the headline. This is the correct, conservative choice.
5. **The token estimate is `len/4` (byte-based), not a real tokenizer.** Layer-2 token economy is an
   approximation (deliberately conservative); it is a relative measure within the harness, not an
   absolute token count.

---

## 10. A reviewer's verification checklist

- [ ] Build the binary; create the venv from `requirements.txt`.
- [ ] `ruff check .` clean; `pytest` shows **166 passed, 1 skipped** (skip count == 1).
- [ ] Read `scorer.py`; confirm the four metric definitions are standard; run `pytest tests/test_scorer.py -v`.
- [ ] Cross-read `scorer.py` ↔ `tests/retrieval_quality.rs`: the two implementations agree on the hand-computed cases.
- [ ] `validate_offline.py` exits 0 with the `OK:` line; every arm covers the gold block (R@1 blk = 1.00).
- [ ] `run_sweep.py` reproduces the null result (default not beaten; `name_only` degrades); output labels it "proxy / not a weights finding."
- [ ] (If running R2.7) selection is deterministic across re-runs; A/B arms are isolated; the report's `scope_note` asserts no winner; `cache/` and `runs/` are not committed (`git ls-files research/r1_harness/{cache,runs}` is empty).
- [ ] Every entrypoint's printed disclaimer matches the claims table in §1 — nothing overclaims.

If all boxes check, the research apparatus is sound and its (deliberately modest) claims are
reproducible. The open scientific question — *which retrieval interface actually saves an agent
tokens and turns* — remains correctly deferred to R3.
