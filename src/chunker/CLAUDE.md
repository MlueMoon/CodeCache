# src/chunker/ — CLAUDE.md

**Module:** `chunker` · **Owner:** `principal-engineering-lead` + `rust-treesitter-specialist`
· **Milestone:** M4 (stub at M0).

## Purpose
Turn AST nodes into `Chunk`s with metadata enrichment (`parent_symbol`, `file_docstring`,
`imports`, `cross_references` — **Decision Log D3**); populate `start_line`/`end_line` (**D7**).
Chunks are non-overlapping and lie within file bounds. Flag heuristic chunks when degradation
(D2) triggered.

## API anchor
`docs/project_plan.md` §4.3 (`Chunk`).

## Tests / scenarios
`docs/TEST_STRATEGY.md#chunker` — property: chunks never overlap and lie within `[0, file_len)`;
enrichment fields populated; heuristic flag set under degradation.

## Shipped API (M4 — Python)
```rust
pub fn chunk(tree: &tree_sitter::Tree, source: &str, lang: Language) -> Result<Vec<Chunk>>;
// Result = chunker's own alias; Ok(vec![]) for empty input; never panics on malformed trees.
pub enum ChunkerError { Parser(parser::ParserError) }   // impl std::error::Error + source() chain
```
`chunk` routes on `parser::error_rate(tree)` vs `HEURISTIC_FALLBACK_THRESHOLD` (0.20, D2):
- **AST path** (`error_rate < threshold`): reuses the M3 `Parser::extract_chunks` cursor walk for
  byte-exact spans + `parent_symbol`/`SymbolType` classification (policy (a): emit **both** class
  and method; siblings disjoint, a method is contained in its class). `is_heuristic = false`.
- **Heuristic path** (`error_rate >= threshold`): flat line split on `def `/`class `/`async def `
  at column 0 (Python). Each chunk runs header-line → next header (or EOF) so siblings are
  pairwise disjoint and span the whole file. `is_heuristic = true`; enrichment left empty.

## Enrichment field semantics (D3, AST path; single pass over the tree)
| Field | Source |
|---|---|
| `parent_symbol` | Enclosing class/function — computed by the M3 walk, carried through. |
| `file_docstring` | First named child of the `module` (skipping comments) when it is an `expression_statement` wrapping a `string`; quotes stripped. Same value on every chunk of the file. |
| `imports` | Every top-level `import_statement` / `import_from_statement` node text, file order, trailing whitespace trimmed. |
| `cross_references` | Function names of `call` expressions whose span lies inside the chunk; bare `identifier` callees only (attribute calls like `os.urandom` are skipped). Deduped, first-seen order. |

## Non-overlap invariant
AST chunks inherit it from the parser: each chunk's span is a definition's byte range, so two
chunks are either disjoint (siblings) or strictly nested (method inside class) — never partially
overlapping. Heuristic chunks are cut at successive header offsets `[h_i, h_{i+1})`, so they are
pairwise disjoint by construction and `start_byte < end_byte <= source.len()` always holds.

## Storage-persistence seam (known follow-up — M5/M7)
M4 does **not** add an `is_heuristic` column or migrate the M1 schema. `storage` round-trips only
non-heuristic AST chunks today, so the row→`Chunk` path reconstructs `is_heuristic: false`. If the
M5 indexer or M7 formatter need the flag persisted/surfaced, add an UNINDEXED `is_heuristic` column
to the `symbols` FTS5 table and map it in `insert_chunks` + `build_search_result` then.

## Status
**M4: GREEN (2026-06-10).** Python only. 10 integration + 3 proptest + 2 unit tests pass; all four
gates green on Rust 1.85.0. TS/Go enrichment lands at M9; flag persistence at M5/M7 if needed.
