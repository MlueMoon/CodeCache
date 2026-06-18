# BRIEF — M3 / parser (Python first)

- **Milestone:** M3 — parser (Python)  ·  **Module(s):** parser, fixtures
- **Owner (manager):** principal-engineering-manager  ·  **Created:** 2026-06-10
- **Status:** RED ▣  GREEN ▢  REVIEW ▢  DONE ▢
- **Links:** docs/ROADMAP.md#m3--parser-python-first · docs/TEST_STRATEGY.md#parser ·
  docs/plans/M3-parser-python.md · docs/project_plan.md §3.2.1 / §4.3 / §5.3

## Goal
Load `tree-sitter-python`, run `.scm` queries to extract function/class/method nodes with
**exact byte spans**, distinguish methods from free functions, include decorator lines in the
def span, handle nested/async/multibyte/CRLF, and detect ERROR-node rate so M4/M5 degrade
gracefully (D2). Unsupported language ⇒ typed error.

## Scope (in / out)
- In: `Parser::new`, `parse_file`, `extract_chunks`, `error_rate`, `should_fall_back`,
  `HEURISTIC_FALLBACK_THRESHOLD`; Python grammar + `queries/python.scm`.
- Out: TS/Go grammars (M9); the heuristic/regex chunker fallback itself (M4 owns the
  `heuristic` flag + chunk emission); criterion bench (M10 aggregate cold-index budget).

## Decision Log bindings
- **D2 (graceful degradation):** parser computes ERROR rate + fallback flag here; never panics
  on malformed input. M4 chunker emits heuristic-flagged chunks.
- **D3 (metadata enrichment):** `parent_symbol` filled for methods (enclosing class) and nested
  functions (enclosing function). Parser passes `Tree` + source forward so M4 can re-query.
- **D7:** `start_line`/`end_line` are 1-based inclusive.

## Definition of Done
- [ ] M3.1–M3.3 green; spans asserted by exact `&source[start..end]` slice equality.
- [ ] Nested/async/decorated/method/multibyte/CRLF fixtures covered.
- [ ] ERROR rate + fallback flag implemented; malformed input never panics.
- [ ] `error_rate` API recorded in `project_plan.md` §3.2.1 with/before implementation.
- [ ] clippy/fmt clean; reviewer APPROVED; `docs/TODO.md` Phase 3 + `src/parser/CLAUDE.md` updated.

---
## RED — test lead

Added `tests/parser_tests.rs` (14 tests) + 10 committed fixtures under
`tests/fixtures/python/`, all **before** any `src/parser/` implementation.

### RED is established (right reason)
`cargo test --test parser_tests` fails to compile with **E0432 unresolved imports** — the M3
API does not exist yet:
```
error[E0432]: unresolved imports `codecache::parser::error_rate`,
  `codecache::parser::should_fall_back`, `codecache::parser::Parser`,
  `codecache::parser::HEURISTIC_FALLBACK_THRESHOLD`
  --> tests\parser_tests.rs:27:25
   | no `error_rate` / `should_fall_back` / `Parser` / `HEURISTIC_FALLBACK_THRESHOLD` in `parser`
error: could not compile `codecache` (test "parser_tests") due to 1 previous error
```
All other suites (hasher/storage/config/smoke) still compile and pass (18 storage tests green;
no other module's tests weakened).

### Tests written (names = `behavior_under_condition_expects_result`)
M3.1 — load grammar + parse to a tree:
- `parse_valid_python_expects_tree_without_errors` (no `has_error`, `error_rate == 0.0`)
- `parse_empty_file_expects_empty_tree_no_panic` (parses, extracts to `[]`)
- `unsupported_language_expects_typed_error` (`Language::Go` at M3 ⇒ `Err`)

M3.2 — exact byte spans + symbol typing (each asserts `&source[start..end] == chunk_text`):
- `extracts_top_level_function_with_exact_span` (Function; full def; lines 1–2)
- `extracts_class_with_exact_span` (Class; span covers whole block incl. methods)
- `extracts_method_inside_class_as_method_type` (`greet`/`__init__` ⇒ Method, parent `Greeter`)
- `nested_function_extracted_with_correct_parent_context` (`outer` Function/no parent; `inner`
  Function with parent `outer`)
- `async_def_extracted` (span includes `async` keyword)
- `decorated_function_span_includes_decorator` (span starts at first `@decorator` line; line 1)
- `multibyte_identifier_span_is_byte_correct` (Greek `αβγ`/`τ`; byte-boundary slice)
- `crlf_file_spans_correct` (`\r\n` preserved inside span)

M3.3 — ERROR-node detection + degradation (D2):
- `error_node_rate_computed_for_malformed_file` (`error_rate` in [0,1] and > 0; threshold sane)
- `high_error_file_above_threshold_flags_for_heuristic_fallback` (`rate >= threshold`,
  `should_fall_back(rate)`, `!should_fall_back(0.0)`)
- `malformed_file_never_panics_returns_result` (parse + extract return `Ok`; surviving chunks
  still satisfy the span invariant)

### Fixtures (committed, minimal, purpose-built) — `tests/fixtures/python/`
| File | Purpose | Newlines |
|---|---|---|
| `valid_module.py` | well-formed module: imports + free fn + class/method | LF |
| `top_level_function.py` | single free function `greet` | LF |
| `simple_class.py` | `Greeter` class with `__init__` + `greet` methods | LF |
| `nested_function.py` | `outer` free fn containing nested `inner` | LF |
| `async_def.py` | `async def fetch` | LF |
| `decorated_function.py` | `@cache` + `@retry(3)` over `def compute` | LF |
| `multibyte_identifier.py` | `def αβγ(τ)` — multibyte UTF-8 names | LF |
| `crlf_function.py` | `def crlf_fn` with CRLF endings | CRLF |
| `malformed.py` | one good fn + one broken `def broken(:` → some ERROR nodes | LF |
| `high_error.py` | mostly garbage → ERROR-rate above threshold | LF |

### API surface the engineering lead MUST implement (in `src/parser/`)
Per `project_plan.md` §3.2.1 + the M3-introduced ERROR-rate API. Imported by the tests as:
```rust
use codecache::parser::{error_rate, should_fall_back, Parser, HEURISTIC_FALLBACK_THRESHOLD};

pub struct Parser { /* ts_parser: tree_sitter::Parser, language_configs */ }
impl Parser {
    pub fn new() -> Result<Self>;                          // wires the Python LanguageConfig
    pub fn parse_file(&mut self, path: &Path, content: &str, lang: Language)
        -> Result<tree_sitter::Tree>;                       // unsupported lang ⇒ typed Err
    pub fn extract_chunks(&self, tree: &tree_sitter::Tree, source: &str, lang: Language)
        -> Result<Vec<Chunk>>;                              // deterministic order by start_byte
}
pub fn error_rate(tree: &tree_sitter::Tree) -> f32;        // (ERROR+MISSING nodes) / total nodes
pub fn should_fall_back(rate: f32) -> bool;                // rate >= HEURISTIC_FALLBACK_THRESHOLD
pub const HEURISTIC_FALLBACK_THRESHOLD: f32;               // ~0.20 (D2); must be in [0,1)
```
Notes the tests pin:
- `error_rate(valid) == 0.0` exactly; `error_rate(malformed) > 0.0`; result always in `[0,1]`.
- `should_fall_back(0.0) == false`; `should_fall_back(rate>=threshold) == true`.
- `extract_chunks` over an empty file or a broken tree returns `Ok` (possibly empty) — never panics.
- Every emitted `Chunk` must satisfy `&source[start_byte..end_byte] == chunk_text` (byte-exact,
  UTF-8-boundary correct, CRLF-preserving).
- `Chunk.symbol_name`/`symbol_type`/`language`/`start_line`/`end_line`/`parent_symbol` populated
  per the assertions above.

### Two specialist decisions encoded by these tests (route via rust-treesitter-specialist)
1. **Decorator inclusion:** a decorated def's span **INCLUDES** the `@decorator` lines (span
   starts at the first decorator; `decorated_function_span_includes_decorator` asserts
   `start_line == 1` and `chunk_text` begins `"@cache\n@retry(3)\ndef compute("`). Use the
   `decorated_definition` node (not the inner `function_definition`) as the span source.
2. **Method vs function detection:** a `function_definition` whose ancestor is a
   `class_definition` body ⇒ `SymbolType::Method` with `parent_symbol = <class name>`; otherwise
   `SymbolType::Function`. A function nested inside another *function* stays `Function` with
   `parent_symbol = <enclosing fn name>` (`nested_function_extracted_with_correct_parent_context`).

## GREEN — engineering lead (2026-06-10)

All 14 parser tests green; full suite **61 passed** (parser 14 integration + 3 new in-module unit
tests; existing 44 hasher/storage/config/types/smoke unchanged). Four gates green on Rust 1.85.0:
`cargo build`, `cargo test --all`, `cargo clippy --all-targets -- -D warnings`, `cargo fmt --all -- --check`.

### Files shipped
- `src/parser/mod.rs` — `Parser{ts_parser, language_configs}`, `new/parse_file/extract_chunks`,
  free `error_rate`/`should_fall_back`, `HEURISTIC_FALLBACK_THRESHOLD = 0.20`, typed
  `ParserError` (impls `std::error::Error` with `source()` chaining the underlying TS error; `From`
  for `LanguageError`/`QueryError`). No reachable `unwrap/expect/panic` on library paths.
- `src/parser/python.rs` — Python `LanguageConfig` (`tree_sitter_python::LANGUAGE.into()` +
  `include_str!("queries/python.scm")`).
- `src/parser/queries/python.scm` — function/class/method + `decorated_definition` queries (§5.3).

### How each pinned decision was implemented
- **`.scm` query approach:** queries are compiled+validated against the grammar in `Parser::new`
  (`Query::new` ⇒ `ParserError::Query` on mismatch — proves capture/node names are correct), but
  **extraction is a `TreeCursor`/`node.children()` walk, not `QueryCursor::matches`.** Reason:
  tree-sitter 0.24's `matches` returns a `StreamingIterator` that needs the external
  `streaming-iterator` crate; the brief mandates no new deps, and the walk also gives the ancestor
  access the two decisions below need. Confirmed node kinds against
  `tree-sitter-python-0.23.6/src/node-types.json`.
- **Decorator node:** `span_node_for(def)` returns `def.parent()` when its kind is
  `decorated_definition`, else `def`. The wrapper's `start_byte` is the first `@decorator`, so the
  span begins at `@cache` and `start_line == 1` — matches `decorated_function_span_includes_decorator`.
- **Method detection traversal:** `parent_is_class(def)` climbs `def.parent()` past structural
  nodes (`block`, `decorated_definition`, ERROR…) and returns at the first *definition* ancestor:
  `class_definition ⇒ true` (Method, parent = class name via the threaded `parent` arg);
  `function_definition ⇒ false` (nested Function, parent = enclosing fn). `parent_symbol` is
  threaded down the recursion as the nearest enclosing def name.
- **Span exactness / trailing newline:** tree-sitter ends a `function_definition` at the last
  *content* byte (before the trailing `\n`), but the tests assert the chunk includes it. Added
  `extend_to_line_end(source, end)` to append a lone `\n` or a `\r\n` pair (byte-level; `\r`/`\n`
  are single-byte ASCII so multibyte UTF-8 is safe). CRLF preserved verbatim; `end_line` is taken
  from the node's content end row (+1) so the appended terminator doesn't advance the line. All
  spans verified by `&source[start..end] == chunk_text`.
- **error-rate walk:** iterative depth-first `TreeCursor` walk (no recursion-depth limit, no
  per-node allocation, never panics). Numerator = every `is_error()||is_missing()` node;
  denominator = **named** nodes. Empirically across the 10 fixtures: all 8 valid = 0.000,
  `malformed.py` = 0.083 (>0, from a `MISSING` `)` ), `high_error.py` = 0.667 (≥0.20). The
  named-node denominator is required: an all-nodes denominator scores `high_error.py` at only
  0.125 (< threshold) because anonymous literal tokens dilute the signal; named-only-but-counting
  only named-bad would score `malformed.py` at 0.000 (its sole defect is an anonymous MISSING).
  Counting all ERROR/MISSING over named nodes satisfies every pinned constraint. Recorded the
  metric + `HEURISTIC_FALLBACK_THRESHOLD` in `project_plan.md` §3.2.1.
- **Degradation seam:** documented in code + `src/parser/CLAUDE.md` that M3 only *reports*; the M4
  chunker owns heuristic emission and the `heuristic` flag.

### Spec/docs edits
- `docs/project_plan.md` §3.2.1: added the ERROR-rate API (`error_rate`/`should_fall_back`/
  `HEURISTIC_FALLBACK_THRESHOLD`), the `ParserError` surface, and the chunk-span conventions
  (decorator inclusion, method-vs-function, trailing-terminator/CRLF/D7).
- `src/parser/CLAUDE.md`: shipped API + design/degradation notes; status → GREEN.
- `docs/TODO.md` Phase 3: both items checked with the GREEN summary + gate results.

<!-- prior placeholder retained below for history -->

## Specialist / Perf notes
<pending>

## REVIEW — code reviewer
<pending>

## OUTCOME — manager
<pending>
