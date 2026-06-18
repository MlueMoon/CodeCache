# BRIEF — R2.3b / harness stub chunker + A/B plumbing

- **Milestone:** R2.3 (research track — chunker ablation) · slice **R2.3b** · **Module(s):** `research/r1_harness/` (pure Python; NO crate touch)
- **Owner (manager):** principal-engineering-manager  ·  **Created:** 2026-06-15
- **Status:** RED ▣  GREEN ▣  REVIEW ▣  DONE ▣  (DONE 2026-06-15 — see OUTCOME)
- **Links:** docs/ROADMAP.md#R2 (D23 chunker-ablation axis, **D25** ingest seam) · docs/TODO.md (R2.3 row) ·
  docs/TEST_STRATEGY.md#research-harness · `.claude/briefs/BRIEF-R2.3a-chunk-ingestion-seam.md` (the D25 seam this consumes)
- **Owner agent:** **research-harness-engineer** (sonnet; scope `research/`; gates **ruff + pytest**; process-boundary to the binary).

## Goal
The harness-side **stub chunker + A/B plumbing** over the hidden `codecache ingest <CHUNKS_JSON>` seam (D25,
shipped in R2.3a). Let the R2 chunker ablation **vary the chunker while holding storage + FTS5-BM25 +
retriever + enrichment constant** by running two arms over the *same* micro-suite corpus, scored by the
*same* Layer-1/NDCG scorer:
- **native arm** — `init → index → query` (CodeCache's real Tree-sitter chunker over the materialized files).
- **stub arm** — `init → ingest <chunks.json> → query`, where `<chunks.json>` is produced by an in-harness
  **stub chunker** that consumes the micro-suite gold chunk records (via `corpus.py`) and emits D25-schema
  `ingest` records.

The "stub" is the simplest swappable chunker (gold chunk boundaries → D25 records). astchunk/cAST is the
gated **R2.6** replacement that drops into the SAME plumbing — R2.3b's job is to prove the plumbing.

## Why (research framing — D23/D25; overview §7)
R2 builds **outcome-agnostic apparatus**. The chunker is the only variable we want to ablate; D25 gave us a
CLI-reachable seam (`ingest`) that bypasses discover→parse→chunk so an external chunker's output flows
through CodeCache's *same* storage + FTS5-BM25 + retriever. R2.3b is the pure-`research/` follow-on that
(1) writes the stub chunker that turns a `Corpus` into D25 records and (2) wires an A/B runner that produces
both arms' retrieved lists over the same corpus and feeds the existing scorer. **Whether stub beats native
is an OUTCOME, not a gate** (overview §7) — do NOT assert a winner; assert the *plumbing* works.

## Scope (in / out)
- **In:**
  - A **stub chunker** (new module, e.g. `r1harness/chunkers.py`): `Corpus → list[dict]` of **valid D25 ingest
    records** with **truthful, materialize-consistent** byte/line offsets (see the offset contract below).
  - An **A/B runner** (new module, e.g. `r1harness/ab_runner.py` + a thin `run_ab.py` real-run entrypoint
    mirroring `run_sweep.py`): for one corpus, produces the **native** and **stub** arms' retrieved lists over
    the *same* materialized repo and feeds the **existing** scorer (`scorer.score_query` / `macro_average`),
    yielding **one comparable scored row per arm**.
  - A thin `CodeCacheIndex.ingest(chunks_path)` adapter method on the existing class in `codecache_tool.py`
    (shells `codecache ingest <path>`, raises on nonzero like `init`/`index`) — **research-harness-engineer's
    call** whether a method or a free helper reads cleaner; the constraint is only: process-boundary, reuse
    the existing `_run`.
  - Tests-first (pytest) + ruff clean; an **equivalence sanity check** proving the stub arm (gold chunk
    boundaries) is retrievable/scorable through `ingest` and the A/B harness yields one row per arm.
- **Out (defer):**
  - astchunk/cAST baseline chunker → **R2.6** (gated: astchunk dep).
  - The formal ablation **table** {chunking × weights × enrichment} + top-config selection → **R2.4**
    (`report.py`-pattern reporter). R2.3b emits the raw per-arm rows the way `sweep.py` emits per-vector rows;
    the polished table is R2.4.
  - External corpus → **R2.5** (gated). R2.3b runs on the micro-suite PROXY only.
  - **Any crate / `Cargo.toml` change.** R2.3a shipped the only crate touch. If a crate change appears needed,
    that is a **scope error — escalate to the manager**, do not make it.
  - Re-ingest / incremental semantics (each arm `init`s a fresh DB — same non-goal as R2.3a).

## The offset-synthesis contract (the stub chunker's real work — get this exactly right)
The micro-suite chunk records **LACK** byte/line offsets and `parent_symbol`/`file_docstring`. Each record
carries only `{file_path, symbol_name, symbol_type, language, chunk_text, imports, cross_references}`
(verified in `tests/fixtures/retrieval_quality/micro_suite.json`). D25 **requires**
`start_byte, end_byte, start_line, end_line`. So the stub chunker must **synthesize** ranges, and they must
be **consistent with `corpus.py`'s `materialize()`** so the ingested ranges are truthful against the on-disk
file the native arm also sees.

`materialize()` (verified, `r1harness/corpus.py:53-70`): chunks sharing a `file_path` are concatenated **in
fixture order** with `"".join(chunk_texts)`; files are written in **first-seen `file_path` order**. The
synthesized offsets MUST point into that exact reconstructed content:
- **`start_byte`** = running sum of the **UTF-8 byte lengths** (`len(text.encode("utf-8"))`) of all preceding
  `chunk_text` for the **same `file_path`**, in fixture order. The first chunk in a file starts at byte `0`.
- **`end_byte`** = `start_byte + len(this chunk_text.encode("utf-8"))`. (D25/§4.3 byte ranges are
  **half-open** in the crate's storage round-trip — confirm against the crate's `Chunk` semantics; the
  native parser appends a trailing line terminator into the span, but for the **stub** the `chunk_text` IS the
  exact bytes, so `[start_byte, end_byte)` over the concatenation is the truthful range. Pin whatever the test
  asserts; the load-bearing property is that `reconstructed_file[start_byte:end_byte] == chunk_text` byte-wise.)
- **`start_line`** = 1-based line number of `start_byte` in the reconstructed file = `1 + (count of "\n" in
  all preceding chunk_text for that file)`.
- **`end_line`** = 1-based **inclusive** last line of the chunk (D7). Since the micro-suite `chunk_text` ends
  with a trailing `"\n"`, the inclusive last *content* line is `start_line + (count of "\n" in chunk_text) - 1`.
  (Pin the exact formula in a hand-computed RED test against `authenticate.py` — two chunks,
  `authenticate_user` then `verify_password` — so the second chunk's `start_line`/`start_byte` are non-zero
  and provably equal the first chunk's text length. This is the single most important correctness test.)
- **Enrichment / optional fields:** map `imports` and `cross_references` straight through (they exist in the
  record). `parent_symbol` and `file_docstring` are absent in the micro-suite → emit `null` (D25 default).
  `is_heuristic` → `false` (D25 default; storage drops it anyway). `symbol_type`/`language` pass through
  (micro-suite uses `function`/`python`, both valid D25 enums).

**Why truthful offsets matter even though BM25 indexes `chunk_text`:** retrieval ranks on the FTS5 columns
(text + enrichment), so a wrong offset would not change *which* chunk is retrieved — BUT R2.3b's contract is
that the stub arm is a **faithful** chunker over the real on-disk file (so R2.6's astchunk drop-in is judged
on the same honest basis, and so `status`/outline surfaces and any future offset-dependent metric are
truthful). Emit truthful offsets; a test pins the `authenticate.py` two-chunk case exactly.

## Equivalence sanity check (the GREEN gate — prove plumbing, not a winner)
- **Stub-arm retrievability:** after `init → ingest <stub chunks.json> → query "<a gold term>"`, the gold
  symbol for at least one micro-suite query is retrieved (e.g. `authenticate_user` for the session-token
  query) — proves the stub records flow through ingest → FTS5 → retriever and are scorable.
- **A/B comparability:** the A/B runner returns exactly **two** scored rows for one corpus (one per arm),
  each a `MetricAtK`-shaped Layer-1 result over the **same** gold from `load_suite`/the corpus's queries —
  i.e. both arms are scored by the identical scorer against the identical gold. **Do NOT assert native vs
  stub ordering or that one wins** (outcome-agnostic, overview §7).
- **Enrichment held constant:** both arms carry the same enrichment (the native parser enriches; the stub
  passes the micro-suite `imports`/`cross_references` through) so the chunker is the only varied axis. A test
  may assert the stub records carry the corpus's `imports`/`cross_references` (not dropped).

## Scenarios to cover (tests-first; from TEST_STRATEGY #research-harness)
- [ ] **stub chunker happy path (unit, binary-free):** `Corpus(auth_module) → stub_chunk()` emits one D25
      record per gold chunk, each with all 9 required fields present + correct enum values; record count ==
      corpus chunk count. (Pure logic — no binary.)
- [ ] **offset truthfulness (unit, binary-free — the critical test):** for `src/auth/authenticate.py` (two
      chunks), assert the **second** chunk's `start_byte` == UTF-8 byte length of the first chunk's text,
      `end_byte == start_byte + len(chunk2_text.encode())`, and `start_line`/`end_line` match the
      hand-computed 1-based inclusive lines; assert `reconstructed_file[start_byte:end_byte] == chunk_text`
      for **every** chunk in the corpus (the materialize-consistency invariant). Reuse `materialize()` /
      reconstruct the same `"".join(...)` to compare.
- [ ] **valid D25 JSON (unit):** the emitted list serializes to a top-level JSON array; `null` defaults for
      `parent_symbol`/`file_docstring`; `imports`/`cross_references` arrays preserved; `is_heuristic` absent or
      `false`. (If practical, validate the shape the R2.3a DTO accepts — top-level array, required keys present.)
- [ ] **stub arm is retrievable through ingest (integration, NEEDS BINARY):** `init → ingest stub.json →
      query "<gold term>"` returns the expected gold symbol (equivalence sanity check #1). Mark/skip cleanly
      if the binary is absent (mirror existing binary-dependent tests' skip pattern).
- [ ] **A/B runner yields one row per arm (integration, NEEDS BINARY):** for `auth_module`, the runner
      produces a native row and a stub row, both scored by the existing scorer over the same gold; assert
      **two** rows, each well-formed Layer-1 — **NOT** a winner. (Equivalence sanity check #2.)
- [ ] **edge — empty corpus / degenerate:** stub chunker over an empty chunk list emits `[]` (a valid D25
      no-op per R2.3a); the A/B runner handles a corpus with zero queries without crashing (or is documented
      as requiring ≥1 query — pin the chosen behavior).
- [ ] **determinism:** stub chunker output is deterministic for a fixed corpus (fixture order ⇒ stable record
      order + stable offsets); two runs produce byte-identical JSON.

## Constraints (hard — restated for the agent)
- **Pure `research/`, tests-first, ruff + pytest gates.** Do **not** touch the Rust crate, `Cargo.toml`, or
  any `src/`/`tests/` Rust file. Reuse `corpus.py` (`load_corpus`/`Corpus`/`materialize`), `codecache_tool.py`
  (`CodeCacheIndex`, `find_codecache_binary`, `build_query_args`, `parse_query_json`, `QueryResult`),
  `scorer.py` (Layer-1 + NDCG@10), and `sweep.py`'s `load_suite`/`SweepQuery` shape where it fits.
- **Process boundary only** — shell to the built binary; no FFI/PyO3 (D12/D15).
- **One gold source** — Layer-1 gold stays `tests/fixtures/retrieval_quality/` (D21). The stub chunker reads
  the corpus; it does not invent gold.
- **`.claude/settings.json` stays UNTOUCHED and OUT of staging.** Gates run **explicitly** (ruff + pytest);
  the Rust gates do not apply to `research/`.
- **Binary build (READ THIS — platform gotcha):** the harness `find_codecache_binary` finds
  `target/{release,debug}/codecache` (Linux) / `codecache.exe` (Windows). **This session runs under WSL2
  (Linux).** The only built artifacts currently present are **Windows** ones (`target/{debug,release}/codecache.exe`
  + `.pdb`/`.lib`, from a prior Windows session) — they will NOT run under WSL2 Linux, and the Linux path
  `target/debug/codecache` does **not** yet exist. A debug Linux build was kicked off in the background
  (manager task `bfbev2pd6`). **Therefore:** (1) the **pure-logic** tests (stub chunker, the offset-truthfulness
  invariant, determinism, valid-JSON, empty-corpus) MUST be the green-standalone backbone — they need **no**
  binary and must pass on their own; (2) the **binary-dependent** integration tests (stub-arm retrievability,
  A/B one-row-per-arm) MUST **skip cleanly** (`pytest.skip`/`importorskip`-style guard via
  `find_codecache_binary` raising `FileNotFoundError`) when no runnable Linux binary is found — not error.
  Before the validation run, confirm `target/debug/codecache` exists (Linux build done) or set `$CODECACHE_BIN`
  to a runnable binary; only then will the integration tests execute rather than skip.

## Definition of Done
- [ ] Tests written **first** (RED captured in this brief), now green · **ruff clean** (`ruff check` +
      `ruff format --check` per the research tree's gate).
- [ ] Stub chunker turns a micro-suite `Corpus` into valid D25 ingest records with **truthful,
      materialize-consistent** byte/line offsets (the `reconstructed[start:end] == chunk_text` invariant
      holds for every chunk).
- [ ] A/B runner produces both arms' retrieved lists over the same corpus and feeds the existing scorer →
      one comparable scored row per arm; equivalence sanity checks pass. **No winner asserted.**
- [ ] **NO crate / `Cargo.toml` / `src/` / Rust-`tests/` change** (escalate if one seems needed).
- [ ] code-reviewer **APPROVED**.
- [ ] `docs/TODO.md` (R2.3 row → R2.3b done, R2.4 next) + `research/CLAUDE.md` ("what lives here", if it
      changes) updated in the **same** change. Decision Log only if a genuine design fork arises (likely none
      — D23/D25 cover this).
- [ ] Committed locally (**no push**; **do not stage `.claude/settings.json`**). Manager reports the commit +
      green pytest/ruff summary.

---
## RED — test lead (research-harness-engineer)

**Status: RED captured 2026-06-15.** Tests written first; production modules (`r1harness/chunkers.py`,
`r1harness/ab_runner.py`, `CodeCacheIndex.ingest`) do NOT exist yet. Both new test files fail at
collection time with `ModuleNotFoundError` — exactly the correct RED. `ruff check` + `ruff format
--check` pass on both new files. Pre-existing suite: 55/56 passed (the 1 pre-existing failure,
`test_normalize_relative_path_backslashes_to_posix`, is a WSL2 platform issue pre-dating this
slice — confirmed by running the suite on a clean stash before our files existed).

### Files added

1. **`research/r1_harness/tests/test_chunkers.py`** (NEW) — 11 pure-logic, binary-free tests for the
   stub chunker (`r1harness.chunkers.stub_chunk`). Covers: happy-path count + required fields + enum
   values + field pass-through, the critical offset-truthfulness tests (second-chunk `start_byte`,
   second-chunk `start_line`/`end_line`, and the full materialize-consistency invariant over every
   chunk), valid-D25-JSON shape (serialisation, null defaults, array preservation, `is_heuristic`),
   edge (empty corpus → `[]`), and determinism (byte-identical JSON across two runs; stable key order).

2. **`research/r1_harness/tests/test_ab_runner.py`** (NEW) — 7 tests for the A/B runner + the
   `CodeCacheIndex.ingest` adapter. Pure tests: `ingest` method existence on `CodeCacheIndex`,
   enrichment hold-constant (imports/cross_references preserved). Binary-dependent (skip cleanly when
   no Linux binary): stub arm retrievable through ingest (equivalence sanity #1), A/B runner yields
   one row per arm (equivalence sanity #2), both arms scored against same gold, empty-queries
   no-crash. Binary skip uses a `pytest.fixture(scope="module")` that calls `find_codecache_binary()`
   and calls `pytest.skip(...)` in the `except FileNotFoundError` branch — skips the whole module
   cleanly when the binary is absent.

### Test → scenario map

| test file | test name | scenario (brief) | binary needed? |
|---|---|---|---|
| test_chunkers.py | `test_stub_chunk_happy_path_count_and_required_fields` | happy path — count + 9 required fields | no |
| test_chunkers.py | `test_stub_chunk_happy_path_enum_values` | happy path — valid D25 enum strings | no |
| test_chunkers.py | `test_stub_chunk_happy_path_matches_corpus_chunks` | happy path — field pass-through | no |
| test_chunkers.py | `test_offset_truthfulness_second_chunk_start_byte` | offset truthfulness — byte offsets | no |
| test_chunkers.py | `test_offset_truthfulness_second_chunk_line_numbers` | offset truthfulness — line numbers | no |
| test_chunkers.py | `test_offset_materialize_consistency_invariant` | offset truthfulness — invariant over ALL chunks | no |
| test_chunkers.py | `test_stub_chunk_json_is_top_level_array` | valid D25 JSON — top-level array | no |
| test_chunkers.py | `test_stub_chunk_null_defaults_for_optional_scalar_fields` | valid D25 JSON — null defaults | no |
| test_chunkers.py | `test_stub_chunk_imports_and_cross_references_preserved` | valid D25 JSON — arrays preserved | no |
| test_chunkers.py | `test_stub_chunk_is_heuristic_absent_or_false` | valid D25 JSON — is_heuristic | no |
| test_chunkers.py | `test_stub_chunk_empty_corpus_returns_empty_list` | edge — empty corpus | no |
| test_chunkers.py | `test_stub_chunk_is_deterministic` | determinism — byte-identical JSON | no |
| test_chunkers.py | `test_stub_chunk_field_order_stable_across_runs` | determinism — stable key order | no |
| test_ab_runner.py | `test_codecache_index_has_ingest_method` | CodeCacheIndex.ingest adapter exists | no |
| test_ab_runner.py | `test_stub_arm_retrievable_through_ingest` | stub arm retrievable (equivalence sanity #1) | YES — skip |
| test_ab_runner.py | `test_ab_runner_yields_one_row_per_arm` | A/B runner one row per arm (equivalence sanity #2) | YES — skip |
| test_ab_runner.py | `test_ab_runner_both_arms_use_same_gold` | both arms use same gold | YES — skip |
| test_ab_runner.py | `test_ab_runner_empty_queries_does_not_crash` | edge — empty queries, no crash | YES — skip |
| test_ab_runner.py | `test_stub_records_carry_imports_and_cross_references` | enrichment held constant | no |

Total new tests: **19** (13 in `test_chunkers.py`, 6 in `test_ab_runner.py`).

### Offset formula the GREEN agent MUST satisfy

For `src/auth/authenticate.py` (two chunks in fixture order):

```
chunk1 = authenticate_user
  start_byte = 0
  end_byte   = 398  [= len(chunk1_text.encode("utf-8"))]
  start_line = 1    [= 1, first chunk in file]
  end_line   = 9    [= 1 + 9_newlines_in_chunk1 - 1]

chunk2 = verify_password
  start_byte = 398  [= len(chunk1_text.encode("utf-8")) — running sum of preceding chunks]
  end_byte   = 587  [= 398 + 189 = 398 + len(chunk2_text.encode("utf-8"))]
  start_line = 10   [= 1 + 9_newlines_in_chunk1 — count of "\n" in all preceding chunk_texts]
  end_line   = 13   [= 10 + 4_newlines_in_chunk2 - 1]
```

General formula:
- `start_byte[i]` = `sum(len(c["chunk_text"].encode("utf-8")) for c in same_file_chunks[:i])`
- `end_byte[i]` = `start_byte[i] + len(chunk_text.encode("utf-8"))`
- `start_line[i]` = `1 + sum(c["chunk_text"].count("\n") for c in same_file_chunks[:i])`
- `end_line[i]` = `start_line[i] + chunk_text.count("\n") - 1`
  (chunk_texts in the micro-suite all end with `"\n"`, so `end_line` is always `>= start_line`)

**Byte-range convention:** `[start_byte, end_byte)` half-open over the UTF-8 bytes of the
reconstructed file (`"".join(chunk_texts).encode("utf-8")`). The test asserts
`reconstructed_file[start_byte:end_byte] == chunk_text.encode("utf-8")` (Python half-open slice).
This matches the brief's statement "for the stub the `chunk_text` IS the exact bytes."

### Binary-skip mechanism

`test_ab_runner.py` uses a `pytest.fixture(scope="module")` named `codecache_binary`:
```python
@pytest.fixture(scope="module")
def codecache_binary():
    try:
        return find_codecache_binary()
    except FileNotFoundError as exc:
        pytest.skip(f"codecache binary not found (...): {exc}")
```
All binary-dependent tests take this fixture as a parameter → the whole module skips cleanly when
no runnable binary is present (WSL2 with only Windows `.exe` artifacts). The `test_ab_runner_empty_queries_does_not_crash` test (which is not a module-fixture user) does its own inline `try/except FileNotFoundError: pytest.skip(...)` guard.

### Captured RED output

```
============================= test session starts ==============================
platform linux -- Python 3.12.3, pytest-9.1.0, pluggy-1.6.0
rootdir: /mnt/c/Users/ehlee/workspace/projects/CodeCache/research/r1_harness
configfile: pyproject.toml
collected 56 items / 2 errors

==================================== ERRORS ====================================
ERROR collecting tests/test_ab_runner.py
ImportError … research/r1_harness/tests/test_ab_runner.py:30: in <module>
    from r1harness.ab_runner import run_ab
ModuleNotFoundError: No module named 'r1harness.ab_runner'

ERROR collecting tests/test_chunkers.py
ImportError … research/r1_harness/tests/test_chunkers.py:39: in <module>
    from r1harness.chunkers import stub_chunk
ModuleNotFoundError: No module named 'r1harness.chunkers'

[pre-existing suite with --continue-on-collection-errors:]
FAILED test_codecache_tool.py::test_normalize_relative_path_backslashes_to_posix  [pre-existing WSL2 issue]
1 failed, 55 passed, 2 errors in 1.30s
```

### Contract questions the GREEN agent MUST resolve

1. **`run_ab` signature:** the tests call `run_ab(corpus, binary, work_dir, queries=None)` where
   omitting `queries` means "load from the micro-suite's queries for this corpus_id" and
   `queries=[]` means "score against empty list". GREEN must match this signature (or document an
   equivalent).

2. **A/B runner row shape:** tests assert `row["arm"]`, `row["macro_all"]`, `row["n_queries"]`
   keys on each returned dict. `macro_all` is a `{k: MetricAtK}` dict as returned by
   `macro_average()`. GREEN must emit this exact shape.

3. **`CodeCacheIndex.ingest(chunks_path: Path | str)` raises on nonzero** like `init`/`index`
   (shells `codecache ingest <path>`, raises `RuntimeError` if returncode != 0).

4. **`end_byte` is half-open** (Python slice convention): `reconstructed_file[start_byte:end_byte]
   == chunk_text.encode("utf-8")`. This is what the test pins. Do not make it inclusive.

5. **`is_heuristic` can be absent (key not in dict) or `False`** — either is acceptable.
   The test checks `rec.get("is_heuristic", False) is False`.

## GREEN — engineering lead (research-harness-engineer)

**Status: GREEN 2026-06-15.** All 19 new tests pass; both equivalence sanity checks RAN (Linux
binary built); ruff clean across all new/edited files and the full `research/` tree.

### Files added / edited

| File | Change |
|---|---|
| `research/r1_harness/r1harness/chunkers.py` | NEW — `stub_chunk(corpus) -> list[dict]` + `dump_chunks(records, path)` |
| `research/r1_harness/r1harness/ab_runner.py` | NEW — `run_ab(corpus, binary, work_dir, queries=None) -> list[dict]` |
| `research/r1_harness/run_ab.py` | NEW — thin real-run entrypoint (mirrors `run_sweep.py`) |
| `research/r1_harness/r1harness/codecache_tool.py` | EDIT — added `CodeCacheIndex.ingest(chunks_path)` method |
| `docs/TODO.md` | EDIT — R2.3 row marked done; R2.3b completion recorded; R2.4 flagged as next |

### How tests pass

**`r1harness/chunkers.py` (`stub_chunk`)**
- Per-file running counters (`file_byte_offset`, `file_newline_count`) advance in fixture order,
  matching `materialize()`'s concatenation order exactly.
- `start_byte[i] = sum(len(text_j.encode("utf-8")) for j < i, same file)`;
  `end_byte[i] = start_byte[i] + len(text_i.encode("utf-8"))` — half-open `[start, end)`.
- `start_line[i] = 1 + sum(text_j.count("\n") for j < i, same file)`;
  `end_line[i] = start_line[i] + text_i.count("\n") - 1`.
- Verified against hand-computed pinned values: `authenticate_user` → (0, 398, 1, 9);
  `verify_password` → (398, 587, 10, 13). All 13 chunker tests green.
- `parent_symbol=None`, `file_docstring=None`, `is_heuristic=False` emitted explicitly;
  `imports`/`cross_references` passed through as lists.

**`CodeCacheIndex.ingest(chunks_path)`**
- Minimal adapter: `self._run("ingest", str(chunks_path))`; raises `RuntimeError` on non-zero
  return code, matching the `init`/`index` pattern exactly. No new imports needed.

**`r1harness/ab_runner.py` (`run_ab`)**
- `queries=None` → calls `_load_queries_for_corpus(corpus.id)` to load from the micro-suite.
- `queries=[]` → passes the empty list through; `macro_average([])` returns all-zero metrics
  and `n_queries=0`. The test `test_ab_runner_empty_queries_does_not_crash` passes (no crash).
- Row shape: `{"arm": ..., "macro_all": {k: MetricAtK, ...}, "n_queries": int}` — exactly as
  the tests assert.
- Both arms share the same `arm_queries` list, so `n_queries` is equal between rows (same-gold
  test passes).

### Equivalence sanity check result — BOTH RAN

The Linux debug binary built successfully from the crate during this session:
`target/debug/codecache` (ELF 64-bit, x86-64). This was the only runnable binary on this WSL2
machine (the Windows `.exe` artifacts cannot run under Linux). All 4 binary-dependent tests
executed and passed (not skipped):

- `test_stub_arm_retrievable_through_ingest`: `init → ingest stub.json → query "authenticate
  user credentials"` returned `("src/auth/authenticate.py", "authenticate_user")` in the
  retrieved blocks — equivalence sanity #1 confirmed.
- `test_ab_runner_yields_one_row_per_arm`: two rows returned (`"native"` + `"stub"`), each with
  `macro_all[10]` carrying `recall_block` and `f1_block` attributes — equivalence sanity #2 confirmed.
- `test_ab_runner_both_arms_use_same_gold`: both rows have identical `n_queries`.
- `test_ab_runner_empty_queries_does_not_crash`: `queries=[]` → two rows, each `n_queries=0`.

### Final pytest summary

```
platform linux -- Python 3.12.3, pytest-9.1.0
collected 75 items
74 passed, 1 failed (pre-existing: test_normalize_relative_path_backslashes_to_posix WSL2 issue)
```

Confirmed: zero new failures introduced.

### ruff status

`ruff check research/` → All checks passed.
`ruff format --check research/` → 27 files already formatted.

### Plan deviations

None. The `run_ab.py` entrypoint is a straightforward mirror of `run_sweep.py`. The `ingest`
adapter is the minimal one-method edit to `codecache_tool.py` as specified. No crate changes
were made or needed.

## Specialist / Perf notes
<none expected — pure Python over existing seams; offsets are arithmetic over chunk_text bytes>

## REVIEW — code reviewer

**APPROVE 2026-06-15 — 0 blockers, 0 majors, 2 nits.** Independently re-ran the research gates on WSL2 with the
Linux debug binary (`target/debug/codecache`, ELF x86-64, present): `ruff check research/` → All checks passed;
`ruff format --check research/` → 27 files already formatted; full `research/r1_harness` pytest → **74 passed,
1 failed**. The two new test files ran in isolation: **19 passed** (`test_chunkers.py` 13, `test_ab_runner.py` 6) —
all 4 binary-dependent tests RAN (not skipped) and passed against the Linux binary.

**Offset invariant — CONFIRMED.** Recomputed the offsets directly from `micro_suite.json`: the pinned
`authenticate.py` values are exact — `authenticate_user` → (start_byte=0, end_byte=398, start_line=1, end_line=9)
and `verify_password` → (398, 587, 10, 13). The materialize-consistency invariant
`reconstructed_file[start_byte:end_byte] == chunk_text.encode("utf-8")` holds for **all 9** auth_module chunks.
Per-file running counters (`file_byte_offset`/`file_newline_count`) reset on first-seen of each `file_path`
(`chunkers.py:47-49`), matching `corpus.materialize()`'s per-file `"".join` concatenation in first-seen order —
so a multi-file corpus does not carry one file's offset into the next. UTF-8 byte length (not str length) and
1-based inclusive lines (D7) are used correctly.

**D25 schema — CONFIRMED.** Every record carries all 9 required fields; `symbol_type`/`language` pass through
the micro-suite's valid enums (`function`/`python`); `parent_symbol`/`file_docstring` emit explicit `None`
(absent in the fixture → correct null defaults); `imports`/`cross_references` pass through as fresh lists
(`list(chunk.get(...))`); `is_heuristic=False`. Serialises to a top-level JSON array (`dump_chunks` →
`json.dumps(records, ...)`).

**A/B plumbing + outcome-agnosticism — CONFIRMED.** `run_ab` materialises the corpus into two fresh dirs, runs
native (`init → index → query`) and stub (`init → ingest <stub.json> → query`) over the SAME corpus, scores
BOTH with the existing scorer (`score_query`/`macro_average`/`dedup_first`) against the SAME gold loaded once via
`_load_queries_for_corpus` (shared `arm_queries`), and returns exactly two rows `{arm, macro_all, n_queries}`.
The stub arm uses `init → ingest` (NOT `index`) — it exercises the D25 seam, not the native chunker
(`ab_runner.py:114-117`). **No winner is asserted** anywhere — the only `>`/"higher"/"wins" matches in code or
tests are the comments explicitly stating no winner is asserted (`test_ab_runner.py:108,138`).

**Test adequacy — CONFIRMED real, not theater.** Offsets are pinned to exact integers; the invariant is asserted
over every chunk; the A/B tests assert exactly two rows, both arms present, MetricAtK-shaped `macro_all[10]`, and
equal `n_queries`. Binary skip is clean: a module-scoped `codecache_binary` fixture calls `pytest.skip(...)` in the
`except FileNotFoundError` branch, and `test_ab_runner_empty_queries_does_not_crash` has its own inline
`try/except FileNotFoundError: pytest.skip(...)` — so the module skips (not errors) when no Linux binary exists.
Here they RAN: equivalence sanity #1 (gold `authenticate_user` retrieved through ingest) and #2 (two rows, same gold).

**Scope discipline — CONFIRMED.** `git diff HEAD` touches only `research/r1_harness/r1harness/codecache_tool.py`
(+13: the minimal `ingest` adapter), `docs/TODO.md`, and `.claude/settings.json`. **No** `src/` / `Cargo.toml` /
`Cargo.lock` / Rust `tests/` change. `CodeCacheIndex.ingest` is a minimal process-boundary adapter
(`self._run("ingest", str(chunks_path))`; raises `RuntimeError` on non-zero) consistent with `init`/`index` — no
FFI. The single pytest failure is the **pre-existing** `test_normalize_relative_path_backslashes_to_posix` WSL2
issue: its test file is unchanged vs HEAD and `normalize_path` is untouched by this slice — NOT introduced by R2.3b.

### Findings
- **minor — `.claude/settings.json` (working tree) — modified but MUST stay untouched + out of staging.** The
  working tree shows `M .claude/settings.json` (Stop/SubagentStop hooks removed). This was already present at
  session start, is unrelated to any R2.3b code, and is currently **NOT staged** (correct). **Fix:** do NOT
  `git add` it; commit only the three R2.3b paths (`chunkers.py`, `ab_runner.py`, `run_ab.py`, the
  `codecache_tool.py` edit, the two test files, `docs/TODO.md`, this brief). Manager to confirm the settings
  revert is handled in its own change, per the brief constraint. Not a code defect — flagged for commit hygiene.
- **nit — `ab_runner.py:25` — `from typing import Sequence`** is the deprecated alias (py310 target); prefer
  `from collections.abc import Sequence` (as `sweep.py` already does). ruff passes (UP035 not enabled), so
  non-blocking.
- **nit — `codecache_tool.py:138` — `chunks_path: "Path | str"`** quotes the annotation though the file has
  `from __future__ import annotations` (line 14), making the quotes redundant; `__init__` uses unquoted
  `Path | None`. Cosmetic; ruff clean.

**Verdict: APPROVE.** The two nits are non-blocking. The settings.json finding is commit hygiene, not a code
defect — the file is already out of staging, which satisfies the brief; the manager must keep it out of the
R2.3b commit.

## OUTCOME — manager
**DONE 2026-06-15.** RED → GREEN → code-reviewer **APPROVE** (0 blockers, 0 majors, 2 cosmetic nits) complete.
Aligned with D23 (chunker-ablation axis) + D25 (the `ingest` seam this consumes); pure `research/`, zero crate
touch (verified: `git diff` shows only `research/r1_harness/r1harness/codecache_tool.py` +13 `ingest` adapter,
the 3 new modules, the 2 new test files, plus the docs). No new Decision Log entry — D23/D25 already cover
this; no design fork arose.

**What shipped:** `r1harness/chunkers.py` (`stub_chunk` synthesizes materialize-consistent UTF-8 byte /
1-based-inclusive line offsets — invariant `reconstructed_file[start:end] == chunk_text.encode()` holds for
every chunk; `dump_chunks` emits a top-level D25 JSON array) + `r1harness/ab_runner.py` (`run_ab`: native
init→index→query vs stub init→**ingest**→query over the SAME materialized corpus + SAME scorer + SAME gold →
one comparable row per arm, **no winner asserted**) + `run_ab.py` entrypoint + `CodeCacheIndex.ingest` adapter.
19 new tests (15 pure-logic backbone + 4 binary-dependent that **RAN**, not skipped, because a Linux debug
binary was built from the crate — the only Linux-runnable binary on this WSL2 machine). Equivalence sanity
#1 (gold `authenticate_user` retrieved through the D25 ingest seam) + #2 (two rows per arm) both passed.

**Gates (reviewer-reproduced):** `ruff check research/` clean · `ruff format --check research/` clean (27
files) · full `research/r1_harness` pytest **74 passed / 1 failed**. The single failure is the **pre-existing**
`test_normalize_relative_path_backslashes_to_posix` WSL2 platform issue — confirmed (by the test lead via a
clean-stash run and by the reviewer via inspecting that `normalize_path` + its test are untouched by this
slice) to **predate R2.3b**; no new failure introduced.

**Doc-sync (golden rule, same change):** `docs/TODO.md` R2.3b line → DONE + reviewer-APPROVED, R2.3 parent →
COMPLETE, R2.4 flagged next; `research/CLAUDE.md` "What lives here" now documents the R2 ablation apparatus
(scorer NDCG / sweep / chunkers+ab_runner) sharing the `r1_harness/` package alongside the R1 harness.

**Commit-hygiene (enforced):** `.claude/settings.json` (cargo hooks disabled by the user; modified in the
working tree, unrelated to R2.3b) was **kept OUT of the commit** per the brief — committed via an explicit
pathspec, never `git add -A`.

**Follow-ups (no new work created — already on the roadmap):**
- **R2.4** — ablation-table reporter ({chunking × weights × enrichment} + top-config selection); the A/B
  rows R2.3b emits + the R2.2b sweep rows are its inputs. → research-harness-engineer (NEXT).
- **R2.6** — astchunk/cAST baseline drops into this exact A/B plumbing (gated: astchunk dep).
- **Pre-existing (not R2.3b):** the `test_normalize_relative_path_backslashes_to_posix` WSL2 failure should be
  fixed or platform-guarded by the research-harness owner in a future research slice so the suite is green on
  Linux too (it predates this slice; left untouched here to keep R2.3b scoped).
