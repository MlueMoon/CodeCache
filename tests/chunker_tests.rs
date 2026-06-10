//! Integration tests for the `chunker` module (M4 — Python first).
//!
//! TDD RED: written before `src/chunker/mod.rs` exists. Scenarios from
//! `docs/plans/M4-chunker.md` (slices M4.1–M4.3) + `docs/TEST_STRATEGY.md#chunker`.
//!
//! The public API under test (`docs/plans/M4-chunker.md` §API contracts):
//! ```ignore
//! pub fn chunk(tree: &tree_sitter::Tree, source: &str, lang: Language) -> Result<Vec<Chunk>>;
//! ```
//! `chunk` turns parser output into enriched `Chunk`s: it maps AST definitions to chunks
//! (M4.1), populates the D3 enrichment fields `parent_symbol` / `file_docstring` / `imports` /
//! `cross_references` (M4.2), and — when the parser's ERROR rate exceeds
//! `HEURISTIC_FALLBACK_THRESHOLD` — falls back to a line heuristic and flags the resulting chunks
//! with `is_heuristic = true` (M4.3, D2).
//!
//! The tree is produced via the existing `Parser` (M3 seam) so this also exercises the
//! parser↔chunker boundary on real fixtures. Tests are deterministic and parallel-safe; fixtures
//! live under `tests/fixtures/python/` and are committed.

use std::path::{Path, PathBuf};

use codecache::chunker::chunk;
use codecache::parser::Parser;
use codecache::types::{Chunk, Language, SymbolType};

// ───────────────────────────── fixture helpers ─────────────────────────────

/// Absolute path to a committed Python fixture.
fn fixture_path(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("python")
        .join(name)
}

/// Load a committed Python fixture's bytes as a UTF-8 string, preserving its exact newlines.
fn load_fixture(name: &str) -> String {
    let path = fixture_path(name);
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read fixture {name}: {e}"))
}

/// Parse a fixture to a tree, then run the chunker over it — the full parser↔chunker seam.
fn chunks_of(name: &str) -> (String, Vec<Chunk>) {
    let mut parser = Parser::new().expect("Parser::new");
    let source = load_fixture(name);
    let path = fixture_path(name);
    let tree = parser
        .parse_file(&path, &source, Language::Python)
        .unwrap_or_else(|e| panic!("parse {name}: {e}"));
    let chunks =
        chunk(&tree, &source, Language::Python).unwrap_or_else(|e| panic!("chunk {name}: {e}"));
    (source, chunks)
}

/// Run the chunker over an in-memory source string (no committed fixture).
fn chunks_of_source(source: &str) -> Vec<Chunk> {
    let mut parser = Parser::new().expect("Parser::new");
    let tree = parser
        .parse_file(Path::new("mem.py"), source, Language::Python)
        .expect("parse in-memory source");
    chunk(&tree, source, Language::Python).expect("chunk in-memory source")
}

/// Find exactly one chunk with the given symbol name; panic with context otherwise.
fn one_named<'a>(chunks: &'a [Chunk], name: &str) -> &'a Chunk {
    let matches: Vec<&Chunk> = chunks.iter().filter(|c| c.symbol_name == name).collect();
    assert_eq!(
        matches.len(),
        1,
        "expected exactly one chunk named {name:?}, found {}: {:?}",
        matches.len(),
        chunks.iter().map(|c| &c.symbol_name).collect::<Vec<_>>()
    );
    matches[0]
}

/// The bedrock span guard, reused from the parser suite: every chunk's byte span must slice the
/// source to exactly its text and lie within `[0, file_len]`.
fn assert_span_in_bounds(source: &str, chunk: &Chunk) {
    assert!(
        chunk.start_byte < chunk.end_byte && chunk.end_byte <= source.len(),
        "span [{}, {}) must satisfy start < end <= {} for {:?}",
        chunk.start_byte,
        chunk.end_byte,
        source.len(),
        chunk.symbol_name
    );
    assert_eq!(
        &source[chunk.start_byte..chunk.end_byte],
        chunk.chunk_text,
        "chunk_text must equal source[start_byte..end_byte] for {:?}",
        chunk.symbol_name
    );
}

// ═══════════════ Slice M4.1 — AST → Chunk (boundary cases) ═══════════════

#[test]
fn single_symbol_file_yields_one_chunk() {
    // A file containing exactly one top-level definition yields exactly one chunk, spanning it.
    let (source, chunks) = chunks_of("top_level_function.py");
    assert_eq!(
        chunks.len(),
        1,
        "a single-symbol file must yield exactly one chunk, got {:?}",
        chunks.iter().map(|c| &c.symbol_name).collect::<Vec<_>>()
    );
    let f = &chunks[0];
    assert_eq!(f.symbol_name, "greet");
    assert_eq!(f.symbol_type, SymbolType::Function);
    assert!(!f.is_heuristic, "a well-formed AST chunk is not heuristic");
    assert_span_in_bounds(&source, f);
}

#[test]
fn empty_file_yields_no_chunks() {
    // An empty file is not an error: the chunker returns an empty vec, never panicking.
    let chunks = chunks_of_source("");
    assert!(
        chunks.is_empty(),
        "an empty file must yield no chunks, got {chunks:?}"
    );
}

// ═══════════════ Slice M4.2 — metadata enrichment (D3) ═══════════════

#[test]
fn method_chunk_has_parent_symbol_set_to_class_name() {
    // A method's `parent_symbol` is the enclosing class name (D3).
    let (_source, chunks) = chunks_of("simple_class.py");
    let m = one_named(&chunks, "greet");
    assert_eq!(
        m.symbol_type,
        SymbolType::Method,
        "a function defined in a class body is a Method"
    );
    assert_eq!(
        m.parent_symbol.as_deref(),
        Some("Greeter"),
        "a method's parent_symbol must be its enclosing class"
    );
}

#[test]
fn top_level_function_has_no_parent_symbol() {
    // A free, top-level function has no enclosing definition ⇒ parent_symbol is None.
    let (_source, chunks) = chunks_of("top_level_function.py");
    let f = one_named(&chunks, "greet");
    assert_eq!(f.symbol_type, SymbolType::Function);
    assert_eq!(
        f.parent_symbol, None,
        "a top-level function must have no parent_symbol"
    );
}

#[test]
fn chunk_has_file_docstring_when_module_has_one() {
    // A module-level docstring is enrichment-attached to the file's chunks (D3).
    let (_source, chunks) = chunks_of("enriched_module.py");
    assert!(
        !chunks.is_empty(),
        "enriched_module.py must yield at least one chunk"
    );
    let f = one_named(&chunks, "hash_password");
    assert_eq!(
        f.file_docstring.as_deref(),
        Some("Module docstring: user service helpers."),
        "the module docstring must be extracted onto the chunk (D3)"
    );
}

#[test]
fn chunk_imports_lists_module_imports() {
    // The file's import statements are listed in each chunk's `imports` (D3).
    let (_source, chunks) = chunks_of("enriched_module.py");
    let f = one_named(&chunks, "hash_password");
    assert!(
        f.imports.iter().any(|i| i.contains("os")),
        "module imports must include the `import os` statement, got {:?}",
        f.imports
    );
    assert!(
        f.imports.iter().any(|i| i.contains("typing")),
        "module imports must include the `from typing import List` statement, got {:?}",
        f.imports
    );
}

#[test]
fn cross_references_lists_called_symbol_names() {
    // Best-effort: identifiers called inside a chunk's body land in `cross_references` (D3).
    // `register` calls `hash_password`, so that name must appear in its cross_references.
    let (_source, chunks) = chunks_of("enriched_module.py");
    let m = one_named(&chunks, "register");
    assert!(
        m.cross_references.iter().any(|r| r == "hash_password"),
        "cross_references must list called symbol names (best-effort), got {:?}",
        m.cross_references
    );
}

// ═══════════════ Slice M4.3 — heuristic fallback (D2) ═══════════════

#[test]
fn high_error_rate_input_produces_heuristic_flagged_chunks() {
    // When the parser's ERROR rate exceeds the threshold, the chunker falls back to a line
    // heuristic and flags the resulting chunks with `is_heuristic = true` (D2).
    let source = "\
def alpha():
    @@@ ??? !!!
)))( {{{ ]][[

def beta():
    === +++ ***
";
    let chunks = chunks_of_source(source);
    assert!(
        !chunks.is_empty(),
        "heuristic fallback must still emit chunks for a high-error file"
    );
    assert!(
        chunks.iter().all(|c| c.is_heuristic),
        "every chunk from a high-error file must be flagged is_heuristic = true, got {:?}",
        chunks
            .iter()
            .map(|c| (&c.symbol_name, c.is_heuristic))
            .collect::<Vec<_>>()
    );
}

#[test]
fn heuristic_chunks_still_non_overlapping_and_in_bounds() {
    // Even on the heuristic path the invariants hold: every span is in bounds and slices to its
    // text, and sibling heuristic chunks never overlap.
    let source = "\
def alpha():
    @@@ ??? !!!
)))( {{{ ]][[

def beta():
    === +++ ***
";
    let mut chunks = chunks_of_source(source);
    assert!(!chunks.is_empty(), "expected heuristic chunks");
    for c in &chunks {
        assert!(c.is_heuristic, "this path must be heuristic");
        assert_span_in_bounds(source, c);
    }
    // Heuristic chunks are flat (no nesting), so they must be pairwise disjoint once sorted.
    chunks.sort_by_key(|c| c.start_byte);
    for pair in chunks.windows(2) {
        assert!(
            pair[0].end_byte <= pair[1].start_byte,
            "heuristic sibling chunks must not overlap: [{}, {}) vs [{}, {})",
            pair[0].start_byte,
            pair[0].end_byte,
            pair[1].start_byte,
            pair[1].end_byte
        );
    }
}

#[test]
fn malformed_input_never_panics() {
    // D2: malformed input must degrade gracefully — `chunk` returns a Result and never panics,
    // even on a tree riddled with ERROR/MISSING nodes. Surviving chunks keep the span invariant.
    let source = load_fixture("malformed.py");
    let mut parser = Parser::new().expect("Parser::new");
    let path = fixture_path("malformed.py");
    let tree = parser
        .parse_file(&path, &source, Language::Python)
        .expect("malformed input still parses to a tree, not a panic");

    let result = chunk(&tree, &source, Language::Python);
    assert!(
        result.is_ok(),
        "chunk over a malformed tree must return Ok, got {result:?}"
    );
    for c in result.expect("ok") {
        assert_span_in_bounds(&source, &c);
    }
}
