# Testing & Trying CodeCache — detailed guide

How to **test** CodeCache (run the suites + quality gates) and how to **try** it (use the CLI, the
MCP server, and the R1 research harness). Two parts to the project:

1. **The Rust core** — the `codecache` binary/library (parser → index → retrieve → format/serve).
2. **The R1 research harness** — out-of-crate Python (`research/r1_harness/`) that runs the
   retrieval-interface ablation against the built binary (research track, ROADMAP D22).

---

## 0. Prerequisites

| For | Need |
|---|---|
| Rust core | **Rust 1.85.0** (pinned by `rust-toolchain.toml`; `rustup` auto-selects it). On Windows, the MSVC toolchain (VS 2022 Build Tools) for the bundled SQLite + Tree-sitter C deps. |
| R1 harness | **Python 3.10+**, and a **bash** (Git for Windows provides `bash`/`grep`/`cat`). |

Clone and enter the repo:
```bash
git clone <repo-url> CodeCache && cd CodeCache
```
The first `cargo build` compiles bundled C (SQLite/Tree-sitter) — expect ~1 min cold.

---

## 1. Testing the Rust core

### 1.1 The four quality gates (what hooks + CI enforce)
Run all four from the repo root; this is exactly what must pass before any change is "done":
```bash
cargo fmt --all -- --check                  # formatting
cargo clippy --all-targets -- -D warnings   # lint (warnings are errors)
cargo test --all                            # all tests (currently 230, 0 failures)
cargo build                                 # compiles clean
```
On Windows PowerShell, prepend cargo to PATH if needed:
`$env:Path = "$env:USERPROFILE\.cargo\bin;$env:Path"`.

### 1.2 What `cargo test` covers
Unit tests live in each module (`src/**/mod.rs` `#[cfg(test)]`); cross-module suites live in
`tests/`:

| Suite | Covers |
|---|---|
| `tests/parser_tests.rs`, `parser_ts_tests.rs`, `parser_go_tests.rs` | Python/TS/Go byte-exact spans, ERROR-node degradation (D2). |
| `tests/chunker_tests.rs`, `chunker_proptest.rs` | AST→chunk enrichment (D3); non-overlap property. |
| `tests/storage_tests.rs` | SQLite/FTS5 schema, CRUD round-trip, BM25 ordering, `symbols_for_path` (D19). |
| `tests/retriever_tests.rs` | BM25 ranking determinism, dedup, token-budget packing. |
| `tests/formatter_tests.rs` | TOON/JSON/text golden outputs; JSON round-trip; D13 ordering. |
| `tests/cli_tests.rs`, `e2e_cli.rs` | CLI parsing/exit codes + full `init→index→query` through the built binary. |
| `tests/mcp_tests.rs` | MCP JSON-RPC handshake, `tools/list`, `tools/call`, self-healing (D14). |
| `tests/e2e_multilang.rs` | Mixed Python/TS/Go repo indexes through the public surface. |
| `tests/retrieval_quality.rs` | **M10.2 Layer-1 scorer** (Recall/Precision/F1 @k, file+block) + metric unit tests. |

### 1.3 Useful subsets
```bash
cargo test --test storage_tests              # one integration suite
cargo test --test retrieval_quality -- --nocapture   # see the printed Layer-1 metrics report
cargo test recall_                           # tests whose name contains "recall_"
```

### 1.4 Benchmarks + query plan (perf budgets, M10)
```bash
cargo bench                                   # criterion: indexing / query / hashing vs §5.4 budgets
cargo run --release --example explain_query_plan   # FTS5 EXPLAIN QUERY PLAN baseline
```
Budgets and measured numbers (query p95 ≈ 0.5 ms, index 12 MB, etc.; the one tracked miss is
cold-index 10K, D20) are in [`benches/CLAUDE.md`](../benches/CLAUDE.md).

---

## 2. Trying the Rust core (CLI walkthrough)

### 2.1 Build the binary
```bash
cargo build --release        # -> target/release/codecache(.exe)
```
Either put it on PATH, or call it by path. Examples below use `codecache` for brevity.

### 2.2 End-to-end on a sample repo
From inside the repo you want to index (CodeCache writes a `.codecache/` dir there):
```bash
codecache init     # create .codecache/ + config.toml + the index DB (idempotent)
codecache index    # parse + chunk + index all supported files (Python/TS/Go)
codecache status   # show counts + DB size + per-language breakdown
```
Real output (a 1-file Python repo):
```
$ codecache index
Indexed 1 file(s), 2 chunk(s) in 53 ms

$ codecache status
CodeCache index status
  Version:   0.1.0
  Database:  .../.codecache/index.db (49152 bytes)
  Files:     1
  Chunks:    2
  Files by language:
    python: 1
```

### 2.3 Query in three formats
```bash
codecache query "authenticate user password" --max-results 3                 # text (default)
codecache query "verify password" --format json --max-results 1              # JSON (§6.4.2)
codecache query "authenticate" --format toon --max-results 3                 # TOON (locator-only)
```
- **text** (default) — agent-first (D13): symbol, qualified parent, `file:start-end`, signature,
  then body. Header shows result count + total tokens.
- **json** — `{query, total_results, total_tokens, chunks[]}`; each chunk carries `symbol_name`,
  `symbol_type`, `file_path`, `start_byte`/`end_byte`, `language`, `bm25_score`, `chunk_text`.
  (Scores are FTS5 BM25 — more negative = more relevant.)
- **toon** — one `file:start-end` line per hit; pipes straight to an editor/`cat`.

Flags (`codecache query --help`): `--max-tokens` (default 4000, hard ceiling), `--max-results`
(default 20), `--format {text|json|toon}`, `--file-filter <PATH>`.

### 2.4 Incremental update
```bash
codecache update src/auth.py     # re-index only the named file(s) — xxHash skips unchanged
```

### 2.5 Config
```bash
codecache config                 # print the resolved config
codecache config <KEY> <VALUE>   # set a key and persist to .codecache/config.toml
```

### 2.6 MCP server (Claude Code)
```bash
codecache serve                  # stdio JSON-RPC MCP server (Ctrl-C to stop)
```
Wire it into Claude Code (`--transport stdio` is the default; `sse` returns a clean
"unsupported in v0.1"):
```json
{
  "mcpServers": {
    "codecache": { "command": "codecache", "args": ["serve"], "cwd": "/path/to/your/project" }
  }
}
```
Tools exposed: `codecache_search`, `codecache_update`, `codecache_outline`. Full setup +
self-healing notes: [`CLAUDE_CODE_SETUP.md`](CLAUDE_CODE_SETUP.md).

---

## 3. Testing & trying the R1 research harness (`research/r1_harness/`)

Out-of-crate Python; talks to the built `codecache` binary over a process boundary. It runs the
same-agent retrieval-interface ablation — **A0** (grep only), **A1** (+ `codecache query`), **A4**
(one-shot top-k injection) — and scores Layer-1 (Recall/Precision/F1) + Layer-2 (tokens &
turns-to-coverage) from trajectory logs.

### 3.0 The research suite needs a venv with `requirements.txt` (R2.5a+/R2.6)
**The research test suite (`research/r1_harness`) requires a Python virtual environment with
`research/r1_harness/requirements.txt` installed.** As of R2.5a it depends on
`datasets`/`huggingface_hub` (pinned but fetch-entrypoint-only — the test suite is hermetic and
does not import them) and as of **R2.6** it depends on **`astchunk`** (MIT) and its Tree-sitter
transitives — the R2.6 astchunk tests **import `astchunk` at runtime** and **FAIL on a system
Python that lacks it**. Do **not** run the suite with the system `python3`.

Canonical run (Linux/WSL; mirrors the Windows `C:\ccr1` pattern):
```bash
python3 -m venv research/r1_harness/.venv          # one-time (.venv is gitignored)
research/r1_harness/.venv/bin/pip install -r research/r1_harness/requirements.txt
PYTHONUTF8=1 research/r1_harness/.venv/bin/python -m pytest research/r1_harness/
```
Green baseline: **168 passed, 1 skipped** (the skip = the Windows-only path test). The ruff gates:
```bash
research/r1_harness/.venv/bin/ruff check research/
research/r1_harness/.venv/bin/ruff format --check research/
```

### 3.1 Pure unit tests (no agent, no API, no network)
The scorer / trajectory / corpus / extractor tests need only the stdlib + pytest (the
astchunk/R2.6 tests additionally need `astchunk` from the venv above — run them from the venv):
```bash
cd research/r1_harness
.venv/bin/python -m pytest   # scorer (mirrors retrieval_quality.rs), trajectory, corpus,
                             # codecache_tool parsing, extractor, sweep, A/B, NDCG,
                             # contextbench loader, astchunk chunker (R2.6)
```
The `codecache_tool` and end-to-end paths use the built binary (build it first, or set
`$CODECACHE_BIN`).

### 3.2 Offline end-to-end validation (mini-SWE-agent, deterministic — still no API)
This runs the **full pipeline** (agent loop → bash/`codecache` actions → trajectory → scoring)
using a scripted deterministic model, so it costs nothing.

**One-time setup** — install mini-SWE-agent into a **short-path** venv (see Troubleshooting for
why the path must be short on Windows):
```bash
python -m venv C:/ccr1
C:/ccr1/Scripts/python -m pip install -r requirements.txt    # mini-swe-agent==2.4.1 + pytest
```
Build the release binary if you haven't (`cargo build --release`), then run:
```bash
PYTHONUTF8=1 C:/ccr1/Scripts/python validate_offline.py
```
Expected (numbers are deterministic-script artifacts — **not** an arm-winner claim, which is R3):
```
arm   R@1 file  R@1 blk  F1@10 blk  turns→cov   tok→cov  tot tok
A0        1.00     1.00       0.67          1       126      613
A1        1.00     1.00       0.40          1       161     1037
A4        1.00     1.00       0.40          1       162      462
OK: all three arms ran end-to-end, logged trajectories, and covered the gold block.
```
Outputs land in `research/r1_harness/runs/` (gitignored): `runs/<arm>/trajectory.jsonl` (the
per-turn log) + `runs/report.json` (the full Layer-1/Layer-2 report). The task and its gold come
from `tasks/auth_q1.json`, whose gold mirrors `tests/fixtures/retrieval_quality/micro_suite.json`.

### 3.3 What each piece is
| File | Role |
|---|---|
| `r1harness/scorer.py` | Layer-1 metrics — a verbatim port of the M10.2 protocol (`tests/retrieval_quality.rs`). |
| `r1harness/trajectory.py` | JSONL turn-log schema + Layer-2 (tokens/turns-to-coverage). |
| `r1harness/corpus.py` | Materialise a micro-suite corpus into a real on-disk repo. |
| `r1harness/codecache_tool.py` | Shell out to the binary; parse §6.4.2 JSON; relativise paths to gold. |
| `r1harness/bash_env.py` | Portable `bash -c` environment for mini (not cmd.exe on Windows). |
| `r1harness/extract.py` | Map an action+observation → surfaced files/blocks (A1 JSON exact; A0 grep/cat heuristic). |
| `r1harness/runner.py` | `LoggingAgent` over mini's `DefaultAgent`; runs an arm (deterministic *or* live), logs the trajectory. |
| `r1harness/report.py` | Pure scoring of a trajectory (mini-free). |
| `validate_offline.py` | Runs A0/A1/A4 on the task with `DeterministicModel` (offline) and writes the report. |
| `run_live.py` | Runs A0/A1/A4 against a **live local model** (Ollama via litellm); writes `runs/live/`. |

### 3.4 Live-model run (`run_live.py`, zero-cost via Ollama)
`run_live.py` runs the **same pipeline** as the offline validator but swaps `DeterministicModel` for a
real litellm-backed model. The default backend is a **local Ollama** model — no API key, no paid spend
(this is *not* the ~$1K R3 budget; that stays a separate gate).

**One-time setup** — install [Ollama](https://ollama.com), then pull a small model:
```bash
ollama pull qwen2.5:7b      # advertises native tool-calling; also: llama3, phi3 (text-based only)
```
Ollama serves on `localhost:11434` automatically. Build the release binary, then run (from
`research/r1_harness/`):
```bash
# qwen2.5:7b via native tool-calling (litellm tools=[bash]):
PYTHONUTF8=1 C:/ccr1/Scripts/python.exe run_live.py

# text-based mode — the model writes ```mswea_bash_command``` blocks; robust for small models
# and REQUIRED for models without native tools (llama3, phi3):
PYTHONUTF8=1 C:/ccr1/Scripts/python.exe run_live.py --model-class litellm_textbased
PYTHONUTF8=1 C:/ccr1/Scripts/python.exe run_live.py --model ollama_chat/llama3 --model-class litellm_textbased
```
Flags: `--model` (litellm id, default `ollama_chat/qwen2.5:7b`), `--model-class`
(`litellm` native tools | `litellm_textbased`), `--steps` (per-arm budget, default 8), `--wall`
(per-arm seconds), `--temperature` (default 0.0). The first model call cold-loads the model into Ollama
(~1 min). Outputs land in `runs/live/` (gitignored): per-arm `trajectory.jsonl` (scored turns) +
`mini_trajectory.json` (full message log) + `report.json`. The run prints the live Layer-1/Layer-2 table
side-by-side with the deterministic baseline.

**What to expect (observation, not an arm-winner claim — that is R3).** On `qwen2.5:7b` @ temp 0 all
three arms cover the gold block: A1's in-loop `codecache query` returns the gold symbol at rank 1 on the
first turn; A0 finds it after decomposing the query into keywords; A4 via injection. Note: this small
model is **unreliable under Ollama native tool-calling on the in-loop arm** (it can emit empty responses →
`RepeatedFormatError`); prefer `--model-class litellm_textbased` for dependable local runs.

---

## 4. Troubleshooting

| Symptom | Cause / fix |
|---|---|
| `pip install mini-swe-agent` fails with `No such file or directory: ...litellm...long...path...` | Windows `MAX_PATH` (260). Install the venv at a **short root** like `C:\ccr1` (not under the deep repo path). |
| `UnicodeEncodeError: 'charmap' ... \U0001f44b` when importing minisweagent | mini prints a 👋 banner the cp1252 console can't encode. **Set `PYTHONUTF8=1`** for any process importing it. |
| `codecache binary not found` (harness) | Build it (`cargo build --release`) or set `$CODECACHE_BIN` to its path. |
| `bash not found` (harness) | Install Git for Windows (provides `bash`/`grep`/`cat`) or pass `bash_path` to `BashEnvironment`. |
| `cargo: command not found` | Install via `rustup`; PowerShell: `$env:Path = "$env:USERPROFILE\.cargo\bin;$env:Path"`. |
| clippy/test fail after an edit | Hooks run fmt-on-edit + clippy/test on stop; fix the reported issue (never weaken a test). |

---

## 5. See also
- [`README.md`](../README.md) — overview + quickstart.
- [`CONTRIBUTING.md`](../CONTRIBUTING.md) — TDD workflow, gates, MSRV, no-reachable-panic rule.
- [`CLAUDE_CODE_SETUP.md`](CLAUDE_CODE_SETUP.md) — MCP configuration + the three tools.
- [`research/r1_harness/README.md`](../research/r1_harness/README.md) — the harness in depth.
- [`docs/ROADMAP.md`](ROADMAP.md) / [`docs/TODO.md`](TODO.md) — milestones, decision log, what's next.
