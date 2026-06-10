# src/parser/ — CLAUDE.md

**Module:** `parser` · **Owner:** `principal-engineering-lead` + `rust-treesitter-specialist`
· **Milestone:** M3 (Python), M9 (TypeScript + Go) · stub at M0.

## Purpose
Tree-sitter integration: load grammars, run `.scm` queries to extract function/class/method
nodes with **exact** byte spans; detect ERROR-node rate and route high-error files to heuristic
fallback (**Decision Log D2** — indexing never hard-fails on malformed input).

## API anchor
`docs/project_plan.md` §3.2.1 (`Parser`, `LanguageConfig`) + §5.3 (per-language queries).

## Tests / scenarios
`docs/TEST_STRATEGY.md#parser-python--ts--go` — exact spans on nested/async/decorated symbols;
ERROR-node rate computed; heuristic fallback exercised; unsupported language → error.

## Shipped API (M3 — Python)
```rust
pub struct Parser { /* ts_parser, language_configs */ }
impl Parser {
    pub fn new() -> Result<Self>;                  // wires Python; validates `.scm` vs grammar
    pub fn parse_file(&mut self, path: &Path, content: &str, lang: Language)
        -> Result<tree_sitter::Tree>;              // unsupported lang ⇒ ParserError::UnsupportedLanguage
    pub fn extract_chunks(&self, tree: &tree_sitter::Tree, source: &str, lang: Language)
        -> Result<Vec<Chunk>>;                      // deterministic, sorted by start_byte
}
pub fn error_rate(tree: &tree_sitter::Tree) -> f32;   // (ERROR+MISSING)/named-nodes, in [0,1]
pub fn should_fall_back(rate: f32) -> bool;           // rate >= HEURISTIC_FALLBACK_THRESHOLD
pub const HEURISTIC_FALLBACK_THRESHOLD: f32;          // 0.20 (D2)
pub enum ParserError { UnsupportedLanguage(Language), Language(..), Query(..), ParseFailed{path} }
// ParserError: std::error::Error with source() chaining the underlying tree-sitter error.
```
Files: `mod.rs` (the above), `python.rs` (`LanguageConfig` = grammar + queries),
`queries/python.scm` (function/class/method/decorated S-expression queries, §5.3).

## Design notes
- **Extraction = `TreeCursor` walk, not `QueryCursor`.** The `.scm` queries are compiled and
  validated in `new` (a bad query is a construction error), but extraction walks the tree
  directly. This (a) gives ancestor access for the two pinned decisions and (b) avoids the
  external `streaming-iterator` crate that tree-sitter 0.24's `QueryCursor::matches` requires —
  keeping `Cargo.toml` lean. M4 can drive the queries for D3 enrichment.
- **Decorator inclusion:** spans come from the `decorated_definition` wrapper when present, so
  `@decorator` lines are inside the span and `start_line` is the first decorator line.
- **Method vs function:** nearest *definition* ancestor decides — `class_definition` ⇒ `Method`
  (parent = class); `function_definition` ⇒ nested `Function` (parent = enclosing fn).
- **Span exactness:** byte spans satisfy `&source[start..end] == chunk_text`; the span is
  extended to include the single trailing line terminator (`\n` / `\r\n`, CRLF preserved) that
  closes the def's last line; multibyte identifiers stay on UTF-8 boundaries. `start_line`/
  `end_line` are 1-based inclusive (D7); the appended terminator does not advance `end_line`.
- **error_rate denominator** is *named* nodes (anonymous literal tokens dilute the signal); the
  numerator counts every ERROR/MISSING node. `valid == 0.0`, malformed `> 0.0`, clamped to [0,1].

## Degradation seam (D2)
The parser only **reports** `error_rate` + `should_fall_back`; it never panics on malformed input
(`parse_file`/`extract_chunks` return `Ok`, possibly empty). The actual heuristic/regex chunker
fallback and the `heuristic` chunk flag are **owned by M4** (chunker), enforced again at M5.

## Status
**M3: GREEN (2026-06-10).** Python only. 14 integration + 3 unit tests pass; all four gates green
on Rust 1.85.0. TS/Go land at M9.
