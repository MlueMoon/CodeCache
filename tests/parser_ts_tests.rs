//! Integration tests for the `parser` module — M9.1 (TypeScript).
//!
//! TDD RED: written before `src/parser/typescript.rs` exists and before `Parser::new`
//! wires the TypeScript grammar. Scenarios from `BRIEF-M9-typescript-go.md` (slice M9.1)
//! + `docs/TEST_STRATEGY.md#parser-python--ts--go` + `docs/project_plan.md` §5.3.
//!
//! These mirror `parser_tests.rs` exactly (same helpers, same span-exactness discipline) but
//! drive `Language::TypeScript`. Until TS is wired they fail at `parse_file(.., TypeScript)`
//! returning `ParserError::UnsupportedLanguage(TypeScript)` — the correct RED state.
//!
//! The Python contract TS MUST match (non-negotiable, from M3):
//!  - byte-exact spans: `&source[start_byte..end_byte] == chunk_text`;
//!  - D7 1-based inclusive line numbers; trailing terminator does not advance `end_line`;
//!  - method-in-class ⇒ `SymbolType::Method` with `parent_symbol = <class>`;
//!  - D2 graceful degradation: `error_rate` + `should_fall_back` apply unchanged, never panic;
//!  - deterministic order (chunks sorted by `start_byte`).
//!
//! Span assertions compare `&source[start_byte..end_byte]` against the expected text (the
//! strongest off-by-one / byte-vs-char guard). Fixtures live under `tests/fixtures/typescript/`
//! and are committed (LF newlines). Tests are deterministic and parallel-safe.

use std::path::{Path, PathBuf};

use codecache::parser::{error_rate, should_fall_back, Parser, HEURISTIC_FALLBACK_THRESHOLD};
use codecache::types::{Chunk, Language, SymbolType};

// ───────────────────────────── fixture helpers ─────────────────────────────

/// Absolute path to a committed TypeScript fixture.
fn fixture_path(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("typescript")
        .join(name)
}

/// Load a committed TypeScript fixture's bytes as a UTF-8 string, preserving its exact newlines.
fn load_fixture(name: &str) -> String {
    let path = fixture_path(name);
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read fixture {name}: {e}"))
}

/// Parse a fixture to a tree (the M3.1 seam) and hand back the (source, tree) pair.
fn parse_fixture(parser: &mut Parser, name: &str) -> (String, tree_sitter::Tree) {
    let source = load_fixture(name);
    let path = fixture_path(name);
    let tree = parser
        .parse_file(&path, &source, Language::TypeScript)
        .unwrap_or_else(|e| panic!("parse {name}: {e}"));
    (source, tree)
}

/// Parse + extract chunks from a fixture in one step.
fn chunks_of(name: &str) -> (String, Vec<Chunk>) {
    let mut parser = Parser::new().expect("Parser::new");
    let (source, tree) = parse_fixture(&mut parser, name);
    let chunks = parser
        .extract_chunks(&tree, &source, Language::TypeScript)
        .unwrap_or_else(|e| panic!("extract_chunks {name}: {e}"));
    (source, chunks)
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

/// The bedrock off-by-one guard: the chunk's byte span must slice the source to exactly its text.
fn assert_span_slices_to_text(source: &str, chunk: &Chunk) {
    assert!(
        chunk.start_byte <= chunk.end_byte && chunk.end_byte <= source.len(),
        "span [{}, {}) out of bounds for source of {} bytes ({:?})",
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

// ════════════════ M9.1 — exact byte spans + symbol typing (TS) ═══════════════

#[test]
fn extracts_function_declaration_with_exact_span() {
    let (source, chunks) = chunks_of("top_level_function.ts");
    let f = one_named(&chunks, "foo");
    assert_eq!(f.symbol_type, SymbolType::Function);
    assert_eq!(f.language, Language::TypeScript);
    // Whole `function foo(...) { ... }` body, exact bytes (incl. the trailing line terminator).
    assert_span_slices_to_text(&source, f);
    assert_eq!(
        f.chunk_text, "function foo(name: string): string {\n  return \"hi \" + name;\n}\n",
        "function span must cover the full declaration"
    );
    // 1-based inclusive line range (D7); the appended `\n` does not advance `end_line`.
    assert_eq!(f.start_line, 1, "function starts on line 1");
    assert_eq!(f.end_line, 3, "function ends on line 3 (closing brace)");
    assert_eq!(f.parent_symbol, None, "a top-level function has no parent");
}

#[test]
fn extracts_arrow_function_assigned_to_variable() {
    // §5.3: an arrow fn assigned to a `const`/`let`/`var` is extracted via the
    // `variable_declarator` (name: identifier, value: arrow_function) → a Function named by the
    // declarator identifier (`bar`). The span node is the `variable_declarator`, so it starts at
    // the identifier `bar` (excluding the `const ` keyword) and ends at the arrow body's `}`
    // (the trailing `;` and `\n` are NOT part of the declarator, so no terminator is appended).
    let (source, chunks) = chunks_of("arrow_function.ts");
    let f = one_named(&chunks, "bar");
    assert_eq!(
        f.symbol_type,
        SymbolType::Function,
        "an arrow fn assigned to a variable is a Function"
    );
    assert_eq!(f.language, Language::TypeScript);
    assert_span_slices_to_text(&source, f);
    assert_eq!(
        f.chunk_text, "bar = (x: number) => {\n  return x + 1;\n}",
        "arrow-fn span must cover the variable_declarator (name + arrow value), \
         excluding the `const ` keyword and the trailing `;`"
    );
    // The declarator starts on line 1 and its body closes on line 3 (D7).
    assert_eq!(f.start_line, 1, "arrow declarator starts on line 1");
    assert_eq!(f.end_line, 3, "arrow body closes on line 3");
}

#[test]
fn extracts_class_declaration_and_method_definition() {
    let (source, chunks) = chunks_of("class_with_method.ts");

    // The class itself ⇒ SymbolType::Class, spanning the whole `class Foo { ... }` block.
    let c = one_named(&chunks, "Foo");
    assert_eq!(c.symbol_type, SymbolType::Class);
    assert_eq!(c.language, Language::TypeScript);
    assert_span_slices_to_text(&source, c);
    assert!(
        c.chunk_text.starts_with("class Foo {"),
        "class span must start at the `class` keyword, got {:?}",
        c.chunk_text
    );
    assert!(
        c.chunk_text.contains("greet(name: string): string {"),
        "class span must include its method bodies, got {:?}",
        c.chunk_text
    );
    assert_eq!(c.start_line, 1, "class starts on line 1");
    assert_eq!(c.parent_symbol, None, "a top-level class has no parent");

    // The method ⇒ SymbolType::Method with parent_symbol = the enclosing class.
    let m = one_named(&chunks, "greet");
    assert_eq!(
        m.symbol_type,
        SymbolType::Method,
        "a method_definition inside a class_declaration must be typed as Method"
    );
    assert_eq!(
        m.parent_symbol.as_deref(),
        Some("Foo"),
        "method's parent_symbol must be its enclosing class (D3)"
    );
    assert_span_slices_to_text(&source, m);
    assert!(
        m.chunk_text.starts_with("greet(name: string): string {"),
        "method span must start at the method name, got {:?}",
        m.chunk_text
    );
}

#[test]
fn generics_handled() {
    // A generic function `identity<T>(x: T): T` must extract with an exact span — the type
    // parameter list must not break the byte span or the symbol name.
    let (source, chunks) = chunks_of("generics.ts");
    let f = one_named(&chunks, "identity");
    assert_eq!(f.symbol_type, SymbolType::Function);
    assert_eq!(f.language, Language::TypeScript);
    assert_span_slices_to_text(&source, f);
    assert_eq!(
        f.chunk_text, "function identity<T>(x: T): T {\n  return x;\n}\n",
        "generic function span must include the type parameters and reproduce exact text"
    );
    assert_eq!(f.start_line, 1);
    assert_eq!(f.end_line, 3);
}

#[test]
fn tsx_or_type_only_constructs_no_panic() {
    // v0.1 does NOT emit interfaces / type-aliases as chunks (§5.3 lists no queries for them);
    // they must merely parse without panic. The fixture also contains a real generic function
    // (`makePair`) and a class (`Circle`) which MUST be found. Every surviving chunk must still
    // satisfy the span invariant. Parsing + extraction must return Ok and never panic.
    let mut parser = Parser::new().expect("Parser::new");
    let source = load_fixture("type_only.ts");
    let path = fixture_path("type_only.ts");

    let tree = parser
        .parse_file(&path, &source, Language::TypeScript)
        .expect("type-only constructs must parse to a tree, not panic");

    let chunks = parser
        .extract_chunks(&tree, &source, Language::TypeScript)
        .expect("extract_chunks over a type-only file must return Ok");

    // The real function and class ARE extracted...
    let f = one_named(&chunks, "makePair");
    assert_eq!(f.symbol_type, SymbolType::Function);
    assert_span_slices_to_text(&source, f);

    let c = one_named(&chunks, "Circle");
    assert_eq!(c.symbol_type, SymbolType::Class);
    assert_span_slices_to_text(&source, c);

    // ...while interface/type-alias names are NOT emitted as chunks in v0.1.
    assert!(
        !chunks.iter().any(|c| c.symbol_name == "Shape"),
        "v0.1 must not emit a chunk for the `Shape` interface, got {:?}",
        chunks.iter().map(|c| &c.symbol_name).collect::<Vec<_>>()
    );
    assert!(
        !chunks.iter().any(|c| c.symbol_name == "Pair"),
        "v0.1 must not emit a chunk for the `Pair` type alias, got {:?}",
        chunks.iter().map(|c| &c.symbol_name).collect::<Vec<_>>()
    );

    // Whatever DID get extracted must satisfy the span invariant.
    for chunk in &chunks {
        assert_span_slices_to_text(&source, chunk);
    }
}

#[test]
fn high_error_rate_ts_file_flags_heuristic() {
    // D2 parity: a mostly-garbage TS file must exceed the threshold and flag for heuristic
    // fallback, and parse/extract must still return Ok without panic.
    let mut parser = Parser::new().expect("Parser::new");
    let source = load_fixture("high_error.ts");
    let path = fixture_path("high_error.ts");

    let tree = parser
        .parse_file(&path, &source, Language::TypeScript)
        .expect("malformed TS must still parse to a (possibly error-laden) tree, not panic");

    let rate = error_rate(&tree);
    assert!(
        (0.0..=1.0).contains(&rate),
        "error_rate must be a fraction in [0, 1], got {rate}"
    );
    assert!(
        rate >= HEURISTIC_FALLBACK_THRESHOLD,
        "high-error TS rate {rate} must meet/exceed threshold {HEURISTIC_FALLBACK_THRESHOLD}"
    );
    assert!(
        should_fall_back(rate),
        "rate {rate} at/above threshold must request heuristic fallback (D2 parity)"
    );

    // Extraction over the broken tree must also return Ok and never panic; survivors keep spans.
    let result = parser.extract_chunks(&tree, &source, Language::TypeScript);
    assert!(
        result.is_ok(),
        "extract_chunks over a malformed TS tree must return Ok, got {result:?}"
    );
    for chunk in result.expect("ok") {
        assert_span_slices_to_text(&source, &chunk);
    }
}

#[test]
fn async_function_extracted() {
    // An `async function fetchData(...)` is still a Function; the `async` keyword is inside span.
    let (source, chunks) = chunks_of("async_function.ts");
    let f = one_named(&chunks, "fetchData");
    assert_eq!(f.symbol_type, SymbolType::Function);
    assert_eq!(f.language, Language::TypeScript);
    assert_span_slices_to_text(&source, f);
    assert!(
        f.chunk_text.starts_with("async function fetchData("),
        "async function span must include the `async` keyword, got {:?}",
        f.chunk_text
    );
}
