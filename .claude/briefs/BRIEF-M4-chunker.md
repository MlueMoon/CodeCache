# BRIEF — M4 / chunker (AST boundaries + metadata enrichment + heuristic fallback)

- **Milestone:** M4 — chunker  ·  **Module(s):** chunker, types, fixtures
- **Owner (manager):** principal-engineering-manager  ·  **Created:** 2026-06-10
- **Status:** RED ▣  GREEN ▢  REVIEW ▢  DONE ▢
- **Links:** docs/ROADMAP.md#m4--chunker · docs/TEST_STRATEGY.md#chunker ·
  docs/plans/M4-chunker.md · docs/project_plan.md §3.2.1 / §4.3 · Decision Log D2 / D3

## Goal
Turn parser output into enriched `Chunk`s via `chunk(tree, source, lang) -> Result<Vec<Chunk>>`:
map AST definitions to chunks with in-bounds, disjoint-or-nested byte spans (M4.1); populate the
D3 enrichment fields `parent_symbol` / `file_docstring` / `imports` / `cross_references` (M4.2);
and when the parser's ERROR rate exceeds `HEURISTIC_FALLBACK_THRESHOLD`, fall back to a line
heuristic and flag those chunks with the new `is_heuristic = true` (M4.3, D2).

## Scope (in / out)
- In: `codecache::chunker::chunk`; the new `Chunk.is_heuristic: bool` field on `crate::types`;
  AST→Chunk mapping; D3 enrichment extraction; heuristic/line fallback path.
- Out: TS/Go enrichment (M9); storage/FTS5 column mapping for the new field (coordinate with M1
  §D3, but persistence lands at M5 indexer); criterion bench (M10 aggregate cold-index budget).

## Decision Log bindings
- **D2 (graceful degradation):** chunker emits `is_heuristic = true` chunks via a line heuristic
  when `error_rate(tree) >= HEURISTIC_FALLBACK_THRESHOLD`; never panics on malformed input.
- **D3 (metadata enrichment):** `parent_symbol`, `file_docstring`, `imports`, `cross_references`
  populated on the AST path; heuristic chunks may leave enrichment empty but keep the invariants.

## Definition of Done
- [ ] M4.1–M4.3 green incl. the non-overlap/in-bounds **property test**.
- [ ] All four enrichment fields populated on the AST path; heuristic path flags `is_heuristic`.
- [ ] Extended `Chunk` (with `is_heuristic`) recorded in `project_plan.md` §3.2.1/§4.3 first.
- [ ] clippy/fmt clean; reviewer APPROVED; `docs/TODO.md` Phase 4 + `src/chunker/CLAUDE.md` updated.

---
## RED — test lead (2026-06-10)

Added `tests/chunker_tests.rs` (10 integration tests) + `tests/chunker_proptest.rs` (3 proptest
properties) + one new committed fixture `tests/fixtures/python/enriched_module.py`, all **before**
any `src/chunker/` implementation. Reused existing fixtures where they fit
(`top_level_function.py`, `simple_class.py`, `malformed.py`). The high-error heuristic scenarios
use small in-memory source strings rather than a committed fixture.

### RED is established (right reason)
`export PATH="$HOME/.cargo/bin:$PATH"; cargo test --test chunker_tests --test chunker_proptest`
fails to **compile** for exactly the two expected reasons — the M4 API/field do not exist yet:

```
error[E0432]: unresolved import `codecache::chunker::chunk`
  --> tests\chunker_tests.rs:22:5   |  no `chunk` in `chunker`
error[E0432]: unresolved import `codecache::chunker::chunk`
  --> tests\chunker_proptest.rs:22:5 |  no `chunk` in `chunker`
error[E0609]: no field `is_heuristic` on type `&Chunk`
  --> tests\chunker_tests.rs:112:16  (and 247:19, 223:33, 227:41)
  = note: available fields are: `symbol_name`, `symbol_type`, `file_path`, `start_byte`,
          `end_byte` ... and 8 others
error: could not compile `codecache` (test "chunker_tests") due to 5 previous errors
error: could not compile `codecache` (test "chunker_proptest") due to 1 previous error
```

No other suite touched/weakened. The `chunker` module stub already exists (`src/chunker/mod.rs`,
doc comment + empty `#[cfg(test)] mod tests {}`) and is wired in `src/lib.rs` (`pub mod chunker;`).

### Tests written (names = `behavior_under_condition_expects_result`)
`tests/chunker_tests.rs`:
- **M4.1** — `single_symbol_file_yields_one_chunk`, `empty_file_yields_no_chunks`.
- **M4.2 (D3)** — `method_chunk_has_parent_symbol_set_to_class_name`,
  `top_level_function_has_no_parent_symbol`, `chunk_has_file_docstring_when_module_has_one`,
  `chunk_imports_lists_module_imports`, `cross_references_lists_called_symbol_names` (best-effort).
- **M4.3 (D2)** — `high_error_rate_input_produces_heuristic_flagged_chunks`,
  `heuristic_chunks_still_non_overlapping_and_in_bounds`, `malformed_input_never_panics`.

`tests/chunker_proptest.rs` (`proptest`, 128 cases each; programs built from valid `def`/`class`
building blocks so the AST path — not the fallback — is exercised):
- `every_chunk_span_is_in_bounds` — `start_byte < end_byte <= file_len` and
  `&source[start..end] == chunk_text` for every chunk.
- `chunks_are_disjoint_or_nested` — any two chunks are disjoint or one strictly contains the
  other; never partial overlap.
- `child_is_contained_in_named_parent` — a chunk whose `parent_symbol` names another emitted
  chunk is fully contained inside that parent's span.

### Fixtures (committed) — `tests/fixtures/python/`
| File | Purpose | Newlines |
|---|---|---|
| `enriched_module.py` (NEW) | module docstring + `import os` / `from typing import List` + class `UserService.register` calling free fn `hash_password` — drives D3 docstring/imports/cross_references | LF |
| `top_level_function.py` (reused) | single free fn `greet` — single-symbol + no-parent cases | LF |
| `simple_class.py` (reused) | `Greeter` w/ `__init__`+`greet` — method parent_symbol case | LF |
| `malformed.py` (reused) | one good fn + broken `def broken(:` — never-panic case | LF |

### API surface the engineering lead MUST implement (in `src/chunker/`)
Imported by the tests as `use codecache::chunker::chunk;`:
```rust
// crate::types::Chunk gains ONE field (see checklist below):
//   pub is_heuristic: bool,
// chunker entry point:
pub fn chunk(
    tree: &tree_sitter::Tree,
    source: &str,
    lang: Language,
) -> Result<Vec<Chunk>>;  // Result = chunker's own error alias; Ok([]) for empty/never panics
```
Behaviors the tests pin:
- Empty file ⇒ `Ok(vec![])`. Single top-level def ⇒ exactly one chunk; `is_heuristic == false`.
- AST-path chunks satisfy `&source[start_byte..end_byte] == chunk_text`, `start_byte < end_byte
  <= source.len()`, and `is_heuristic == false`.
- D3 enrichment on the AST path: method `parent_symbol = <class name>`; top-level fn
  `parent_symbol == None`; `file_docstring = Some("Module docstring: user service helpers.")`
  (the literal module-docstring text, quotes stripped) attached to the file's chunks; `imports`
  contains substrings `"os"` and `"typing"` (it lists module import statements — exact form is
  the impl's choice as long as those tokens appear); `cross_references` for `register` contains
  the exact string `"hash_password"` (best-effort: names called/referenced in the chunk body).
- Heuristic path (when `parser::error_rate(tree) >= HEURISTIC_FALLBACK_THRESHOLD`): emit ≥1
  chunk, **every** chunk `is_heuristic == true`, spans still in-bounds + slice to text, and
  sibling heuristic chunks are pairwise disjoint (flat, no nesting). Recommended heuristic: split
  on `def `/`class ` at column 0 (per plan M4.3).
- `chunk` over a malformed tree returns `Ok` (possibly empty), never panics; surviving chunks
  keep the span invariant.

### Nesting / overlap policy encoded (PLAN POLICY (a))
The property test encodes **policy (a)** from `docs/plans/M4-chunker.md` §M4.1: emit **both** the
parent (class) and child (method) chunks. Overlap is **relaxed** from "no two chunks overlap" to
"any two chunks are disjoint OR one strictly contains the other" (siblings disjoint; a method is
fully contained in its class). A child whose `parent_symbol` names an emitted chunk must be
contained in that parent. The engineering lead must NOT emit leaf-only (policy (b)) — that would
fail `method_chunk_has_parent_symbol_set_to_class_name` / `extracts_class_with_exact_span`-style
recall expectations.

### ⚠ CROSS-CUTTING: adding `is_heuristic` breaks existing `Chunk { .. }` literals
Adding `pub is_heuristic: bool` to `crate::types::Chunk` will break compilation of **every**
existing struct literal. The GREEN phase MUST add `is_heuristic: false` (AST-extracted and
storage-reconstructed chunks are non-heuristic) to all of these IN THE SAME CHANGE so the existing
61 tests keep passing:

- [ ] `src/types/mod.rs:150` — the in-module unit-test literal
      (`chunk_carries_all_documented_fields_...`).
- [ ] `src/parser/mod.rs:300` — `build_chunk(...)`'s `Chunk { .. }` (AST path ⇒ `false`).
- [ ] `src/storage/mod.rs:315` — the row→`Chunk` reconstruction (storage path ⇒ `false`).
- [ ] `tests/storage_tests.rs` — helper `chunk()` (~line 24) and `chunk_with_docstring()`
      (~line 44) literals.

The test lead deliberately did NOT modify those files (RED scope is new failing tests only). The
field default is **not** a Rust `Default` — every literal must spell `is_heuristic: false`.
Also: record the extended `Chunk` in `project_plan.md` §3.2.1/§4.3 **before** implementing (doc
contract), and confirm the FTS5 column mapping for the new field with M1 §D3 (persistence at M5).

## GREEN — engineering lead (2026-06-10)

**Status:** all four gates green on Rust 1.85.0. 10 chunker integration + 3 proptest pass; the
61 pre-existing tests still pass (lib 14, smoke 1, parser 14, storage 18, config 11, hasher 5,
types in-lib); +2 new chunker in-module unit tests (`strip_string_literal`, `header_symbol`).
`cargo build`, `cargo test --all`, `cargo clippy --all-targets -- -D warnings`, `cargo fmt --all
-- --check` all clean.

### Files
- `src/types/mod.rs` — added `pub is_heuristic: bool` to `Chunk`; fixed the in-module test literal.
- `src/parser/mod.rs` — `build_chunk` literal sets `is_heuristic: false` (AST path).
- `src/storage/mod.rs` — row→`Chunk` reconstruction sets `is_heuristic: false` (seam below).
- `tests/storage_tests.rs` — `chunk()` helper literal gets `is_heuristic: false`
  (`chunk_with_docstring` mutates a `chunk()` value, no literal there).
- `src/chunker/mod.rs` — the M4 implementation (`chunk` + `ChunkerError` + enrichment/heuristic).
- `docs/project_plan.md` §3.2.1 + §4.3 — recorded `is_heuristic` on `Chunk` (doc contract, first).
- `docs/TODO.md` Phase 4 + `src/chunker/CLAUDE.md` — updated in the same change.

### Enrichment extraction (D3, AST path, single pass)
The AST path calls `Parser::new()?.extract_chunks(tree, source, lang)?` to reuse the M3 cursor
walk — byte-exact spans, `parent_symbol`, and Method-vs-Function classification come for free (no
re-parse; the tree is borrowed). Then, once per file, over the **named children of the `module`
root**:
- `file_docstring`: first named child that isn't a `comment`; if it's an `expression_statement`
  wrapping a `string`, take that string's text and strip the quotes (`strip_string_literal`
  handles triple/single quotes + an `r`/`b`/`f` prefix). Cloned onto every chunk of the file.
- `imports`: each top-level `import_statement` / `import_from_statement` node text, in file order.
- `cross_references`: a single `TreeCursor` depth-first walk collects `call` nodes whose span is
  inside each chunk's `[start_byte, end_byte)`; the callee is read via the `function` field and
  kept only when it's a bare `identifier` (so `hash_password(...)` ⇒ `"hash_password"`, while
  `os.urandom(...)` is skipped). Deduped, first-seen order — deterministic. (Node kinds used:
  `module`, `expression_statement`, `string`, `import_statement`, `import_from_statement`, `call`,
  `identifier`. No new `.scm` was needed; the existing `queries/python.scm` already validates the
  grammar, and walking node kinds directly mirrors the M3 extraction seam.)

### Heuristic path (D2)
When `parser::error_rate(tree) >= HEURISTIC_FALLBACK_THRESHOLD` the tree is untrusted, so
`heuristic_chunks` ignores it and splits `source` by line: `split_inclusive('\n')` tracks byte
offsets, and a line whose column 0 begins `def `/`async def `/`class ` is a header. Each chunk
runs from its header offset to the next header offset (or EOF). `is_heuristic = true`; enrichment
empty; `symbol_name`/`symbol_type` parsed from the header line. Never indexes/borrows the broken
tree, so it can't panic on malformed nodes.

### Non-overlap guarantee
- AST path: every chunk span is a definition's byte range from the parser, so any two are disjoint
  (siblings) or strictly nested (method in class) — never partial overlap. The proptest
  `chunks_are_disjoint_or_nested` + `child_is_contained_in_named_parent` pin this; policy (a).
- Heuristic path: chunks are cut at successive header offsets `[h_i, h_{i+1})`, pairwise disjoint
  by construction; `start < end <= source.len()` holds (degenerate `start >= end` defensively
  skipped, `source.get(..)` guards the slice).

### Storage-persistence seam (decided, documented)
No `is_heuristic` schema column / no M1 migration in M4. `storage` round-trips only AST chunks, so
row→`Chunk` reconstructs `is_heuristic: false`. Documented in `src/chunker/CLAUDE.md` and
`docs/TODO.md` as a known M5/M7 follow-up (add an UNINDEXED column + map it in `insert_chunks` /
`build_search_result`) if the indexer/formatter need the flag persisted. Keeps M1's 18 storage
tests green with only the mechanical literal field-addition.

### No plan deviations
`chunk` signature matches the brief/plan API exactly. `Cargo.toml` unchanged (no new deps). One
note for the reviewer: the typed `ChunkerError` currently has a single `Parser(ParserError)`
variant (the only reachable failure is constructing the reuse `Parser` / unsupported language);
it impls `std::error::Error` with `source()` chaining per the hard rules.

## Specialist / Perf notes
<pending — rust-treesitter-specialist owns the D3 enrichment `.scm` captures (imports, call
expressions, module docstring) per plan M4.2>

## REVIEW — code reviewer
<pending>

## OUTCOME — manager
<pending>
