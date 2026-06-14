# CodeCache R1 — eval harness

Research track **R1** (ROADMAP **D22**, ratified 2026-06-13). The smallest harness that
runs **one gold-labeled task end-to-end in arms A0/A1/A4** and computes the Layer-1 / Layer-2
metrics *from the trajectory logs* — the controlled, same-agent comparison of retrieval
*interfaces* that the repositioning (D12) and `project_overview.md` §4–§5 are built on.

This tree is **research-only and out-of-crate**: it ships in no release artifact, adds no Rust
dependency, and does not touch `Cargo.toml`. The harness is **Python**; the CodeCache core stays
**Rust**; they meet at a **process boundary** — the harness shells out to the built `codecache`
binary (no FFI/PyO3, no async bridge). This preserves the zero-dependency single-binary identity
(D12/D15).

## Arms (R1 scope)
| Arm | Retrieval interface | `codecache` in loop | One-shot inject |
|---|---|---|---|
| **A0** | grep/glob/read only (control) | no | no |
| **A1** | A0 + `codecache query` as a tool | yes | no |
| **A4** | one-shot top-k from the index, no loop access | no | yes |

A2 (D3-enrichment toggle), A3 (embedding tool), A5 (hybrid RRF) are **deferred to R2/R3** (D22).

## Layout
```
research/r1_harness/
├── r1harness/
│   ├── scorer.py          # Layer-1: Python port of the M10.2 protocol (Recall/Precision/F1 @k, file+block)
│   ├── trajectory.py      # JSONL turn-log schema + Layer-2 (tokens/turns-to-coverage)
│   ├── corpus.py          # materialise a micro-suite corpus to a real on-disk repo
│   ├── codecache_tool.py  # adapter: shell out to the codecache binary, parse §6.4.2 JSON
│   ├── extract.py         # action+observation → surfaced files/blocks (A1 JSON exact; A0 grep/cat heuristic)
│   ├── bash_env.py        # portable `bash -c` environment for mini (not cmd.exe on Windows)
│   ├── runner.py          # LoggingAgent over mini's DefaultAgent; deterministic OR live mode
│   ├── report.py          # pure trajectory scoring (mini-free)
│   └── arms.py            # A0/A1/A4 + Task definitions
├── tasks/auth_q1.json     # the R1 single task (gold mirrors the M10.2 fixture verbatim)
├── validate_offline.py    # run A0/A1/A4 offline (DeterministicModel) → runs/report.json
├── run_live.py            # run A0/A1/A4 against a live local model (Ollama via litellm) → runs/live/
├── tests/                 # pytest: scorer (mirrors retrieval_quality.rs), trajectory, corpus, extractor, …
├── requirements.txt       # mini-swe-agent==2.4.1 (runner only) + pytest
└── pyproject.toml
```

The Layer-1 scorer is a **Python port of the M10.2 protocol** pinned by
`tests/retrieval_quality.rs`; `tests/test_scorer.py` mirrors that file's hand-computed metric
tests so the two stay behaviourally identical. The R1 task's gold is loaded from the *same*
`tests/fixtures/retrieval_quality/micro_suite.json` the Rust scorer uses — one gold source, two
scorers (D21: "R2 swaps in the real corpus, scorer unchanged").

## Running the offline tests (no agent, no API, no network)
```bash
cd research/r1_harness
python -m pytest            # scorer + trajectory + corpus unit tests
```
The `codecache_tool` adapter is exercised against the **built binary** (`cargo build --release`
first, or set `$CODECACHE_BIN`).

## Running end-to-end (needs the mini-swe-agent venv — see `docs/TESTING_AND_USAGE.md` §3)
```bash
# Offline — mini's DeterministicModel; no API, no network:
PYTHONUTF8=1 python validate_offline.py                          # A0/A1/A4 → runs/report.json

# Live, zero-cost — a local Ollama model (ollama pull qwen2.5:7b):
PYTHONUTF8=1 python run_live.py                                  # qwen2.5:7b, native tool-calling
PYTHONUTF8=1 python run_live.py --model-class litellm_textbased  # bash-block mode (robust; llama3/phi3)
```

## Status
- **R1 DONE (2026-06-13)** — exit met **offline and live**. Offline (`validate_offline.py`,
  `DeterministicModel`): A0/A1/A4 each drive mini's loop on `auth_q1`, log a trajectory, and cover the
  gold block. Live (`run_live.py`, local Ollama `qwen2.5:7b`, temp 0, **zero cost**): all three arms cover
  the gold block — A1's in-loop `codecache query` returns the gold symbol at **rank 1 on turn 1**.
- **Findings (carried to R2/R3):** Ollama *native* tool-calling is fragile for this 7B model on the
  in-loop arm (empty responses → `RepeatedFormatError`); use `--model-class litellm_textbased` (the mode
  llama3/phi3 also need). Fixed a grep `./`-prefix measurement bug (+regression test; pytest 38→39).
- **Gated (separate, downstream):** the ~$1K **R3** API spend and any paid benchmark/API access — **not**
  authorised by R1.

## Scope discipline (`project_overview.md` §7)
R1 builds the outcome-agnostic *apparatus* only. **No arm-winner claim is made here** — which
interface wins is **R3**. A rigorous null result is itself a publishable outcome (§4.3).
