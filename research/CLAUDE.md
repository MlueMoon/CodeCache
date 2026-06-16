# research/ — CLAUDE.md

Research track (R1–R4) artifacts. **Owner:** `research-harness-engineer` agent for **R2+** (ROADMAP
**D23**, adopted 2026-06-14 — sonnet; scope `research/`; gates **ruff + pytest**; process-boundary to the
binary). The main session drove **R1** (D22). The `principal-engineering-manager` stays gatekeeper
(scope/DoD/doc-sync) and the `code-reviewer` is the independent APPROVE/BLOCK gate.

## What lives here
- `r1_harness/` — the shared research harness (Python). Holds **both** tracks (one package, `r1harness/`):
  - **R1 eval harness** — a fork of mini-SWE-agent that runs the same-agent retrieval-interface ablation
    (arms A0/A1/A4) against the built `codecache` binary and scores Layer-1/Layer-2 metrics from trajectory
    logs. See `r1_harness/README.md`.
  - **R2 offline ablation apparatus** (D23) — pure, binary-via-process-boundary modules that reuse the same
    `corpus.py`/`scorer.py`/`codecache_tool.py`: `scorer.py` (NDCG@10, R2.1), `sweep.py`+`run_sweep.py` (BM25
    weight sweep, R2.2b), `chunkers.py`+`ab_runner.py`+`run_ab.py` (R2.3b stub chunker + native-vs-stub
    A/B plumbing over the D25 `codecache ingest` seam — holds storage/FTS5/retriever/enrichment constant so
    the chunker is the only ablated axis), `astchunk_chunker.py`+`run_ab_astchunk.py` (R2.6 cAST baseline —
    the astchunk/cAST chunker dropped into that same A/B plumbing, native-vs-astchunk; D28),
    `ablation_report.py`+`run_report.py` (R2.4 ablation-table reporter — aggregates sweep + A/B into a single
    Markdown view with n_queries-weighted A/B aggregation, directional top-config selection, and scope-honesty
    disclaimer; pure core + thin loaders + entrypoint), and `contextbench.py`+`fetch_contextbench.py`
    (R2.5a external-corpus loader — pure mapper core + thin fetch entrypoint for the ContextBench-Lite HF
    dataset; see "External-corpus provenance" below).
    Real-run outputs land under `r1_harness/runs/`.

## Rules (different from the Rust crate)
- **Out-of-crate, research-only.** Nothing here is a Rust dependency, ships in a release artifact,
  or touches `Cargo.toml`. The four Rust gates (fmt/clippy/test/build) do not apply; this is Python.
- **Process boundary only.** The harness talks to CodeCache by shelling out to the `codecache`
  binary — no FFI/PyO3. Preserves the zero-dependency single-binary identity (D12/D15).
- **One gold source.** Layer-1 gold contexts come from `tests/fixtures/retrieval_quality/`
  (shared with the Rust M10.2 scorer); the Python scorer ports the M10.2 protocol verbatim (D21).
- **No paid spend without a gate.** R1 runs offline (deterministic/local model). The ~$1K R3 API
  spend and any paid benchmark/API access are separate downstream human gates.
  **EXCEPTION (D26 ratified):** the `fetch_contextbench.py` entrypoint (R2.5a) makes a **one-time,
  cached, no-auth-token** download from HF (`Contextbench/ContextBench`) — zero paid spend, authorized
  for the research harness only. The **product (codecache binary) stays fully air-gapped**.
  The test suite remains hermetic — it never triggers a network call.
- **Scope discipline (`../project_overview.md` §7):** R1 builds outcome-agnostic apparatus; arm
  winners are an R3 determination, not R1.

## Python environment decision (R2.5a, recorded 2026-06-15)
System Python (`/usr/bin/python3` 3.12.3) is externally managed (PEP 668 / Debian policy).
**Decision: use a project-local venv** at `research/r1_harness/.venv/` (gitignored).
Rationale: avoids `--break-system-packages`; keeps deps isolated; mirrors the Windows `C:\ccr1` venv pattern.
Create with:
```
python3 -m venv research/r1_harness/.venv
research/r1_harness/.venv/bin/pip install -r research/r1_harness/requirements.txt
```
Gate commands use the venv Python:
```
PYTHONUTF8=1 research/r1_harness/.venv/bin/pytest research/r1_harness/
research/r1_harness/.venv/bin/ruff check research/
research/r1_harness/.venv/bin/ruff format --check research/
```
Note: `datasets` and `huggingface_hub` are pinned in `requirements.txt` but are **fetch-entrypoint
only** (`fetch_contextbench.py`); the core mapper and test suite are hermetic and do NOT import
them, so the suite stays green whether or not they are installed. Install only when ready to run
the fetch entrypoint:
```
research/r1_harness/.venv/bin/pip install datasets==5.0.0 huggingface_hub==1.19.0
```

**Venv requirement (HARD, R2.6).** Running the test suite **requires the venv with
`requirements.txt` installed** — as of R2.6 the suite depends on **`astchunk`** (MIT) + its
Tree-sitter transitives, and the R2.6 astchunk tests **import `astchunk` at runtime** and **FAIL
on the system `python3`** (which lacks the dep). Always run via the venv Python:
```
PYTHONUTF8=1 research/r1_harness/.venv/bin/python -m pytest research/r1_harness/
```
Green baseline = **138 passed, 1 skipped** (the skip = the Windows-only path test).

## External-corpus provenance (R2.5a, D26)
**ContextBench-Lite** (`r1harness/contextbench.py`, `fetch_contextbench.py`):
- Source: HF dataset `Contextbench/ContextBench`, config `contextbench_verified` (500-task subset).
- License: **Apache-2.0** (confirmed: github.com/EuniAI/ContextBench). arXiv:2602.05892.
- Download: one-time cached to `r1_harness/cache/contextbench/` (gitignored — do NOT commit blobs).
- No auth token required. No paid spend.
- Attribution: EuniAI / ContextBench team.

**CodeRAG-Bench RepoEval** (R2.5b CUT — D27, 2026-06-15; qualitative published reference ONLY, no in-repo loader):
- Source: github.com/code-rag-bench/code-rag-bench, arXiv:2406.14497 (NAACL'25).
- **De-scoped (D27).** The R2.5b RepoEval BEIR loader is **CUT**; CodeCache does **not** load or reproduce
  CodeRAG-Bench data. We cite its published **BM25 NDCG@10 = 0.932** (paper Table 3) **qualitatively** as a
  reference number only. The real-corpus Layer-1 ablation uses **ContextBench-Lite (R2.5a)** instead. *Why cut:*
  RepoEval gold is a 20-line code window (not a symbol), so reproducing 0.932 validates CodeRAG-Bench's chunking,
  not CodeCache's AST-symbol chunking; the `code-rag-bench/repoeval` HF dataset is gated (401); and RepoEval gold
  has no symbol names. See ROADMAP **D27**.
- License: **confirmed CC-BY-SA-4.0** (HF Hub API `cardData.license` + `license:` tags + README front-matter
  across `code-rag-bench/{library-documentation,github-repos,github-repos-python}`). The GitHub repo's missing
  LICENSE file was a red herring — it governs code, not the HF data. (Moot now that the loader is cut, but
  recorded to close the prior open item.)
- RepoEval/RepoCoder underlying data: MIT.

## Update rule
Code change here ⇒ update `docs/TODO.md` (research-track section) in the same change, mirroring the
crate's golden rule.

**Run the suite from the venv** (`PYTHONUTF8=1 research/r1_harness/.venv/bin/python -m pytest
research/r1_harness/`), not the system `python3` — since R2.6 the suite depends on `astchunk` (MIT)
and FAILS without it. Green baseline = **138 passed, 1 skipped**. Full canonical run + the ruff
gates are in `docs/TESTING_AND_USAGE.md` §3.0.
