//! Property-based tests for the `chunker` module (M4.1 invariants).
//!
//! TDD RED: written before `src/chunker/mod.rs` exists. Encodes the M4 nesting/overlap policy
//! **(a)** from `docs/plans/M4-chunker.md` §M4.1 — emit both the parent (class) and child
//! (method) chunks, with overlap *relaxed to* "siblings never overlap + a child is fully
//! contained in its parent". The invariants asserted for every generated Python file:
//!
//!   1. **In-bounds span:** `start_byte < end_byte <= file_len` for every chunk, and the byte
//!      span slices the source back to exactly `chunk_text`.
//!   2. **Containment hierarchy:** for any two chunks A, B their byte ranges are either disjoint
//!      or one strictly contains the other — never partially overlapping. (Disjoint siblings;
//!      a method fully contained in its class.)
//!   3. **Parent containment:** a chunk whose `parent_symbol` names another emitted chunk must
//!      be fully contained within that parent's span.
//!
//! Inputs are assembled from small, syntactically valid Python building blocks so the AST path
//! (not the heuristic fallback) is exercised; this keeps the property meaningful and the
//! generated programs well-formed.

use std::path::Path;

use codecache::chunker::chunk;
use codecache::parser::Parser;
use codecache::types::Chunk;
use codecache::types::Language;
use proptest::prelude::*;

/// Half-open byte range helpers.
fn disjoint(a: &Chunk, b: &Chunk) -> bool {
    a.end_byte <= b.start_byte || b.end_byte <= a.start_byte
}

/// True iff `inner`'s span is fully contained within `outer`'s span.
fn contained(inner: &Chunk, outer: &Chunk) -> bool {
    outer.start_byte <= inner.start_byte && inner.end_byte <= outer.end_byte
}

/// A free function definition with a unique name.
fn gen_function() -> impl Strategy<Value = String> {
    "[a-z][a-z0-9_]{0,7}".prop_map(|name| format!("def fn_{name}(x):\n    return x + 1\n"))
}

/// A class with one method, exercising the parent/child containment relationship.
fn gen_class() -> impl Strategy<Value = String> {
    ("[A-Z][a-z0-9]{0,6}", "[a-z][a-z0-9_]{0,6}").prop_map(|(cls, method)| {
        format!("class Cls_{cls}:\n    def m_{method}(self):\n        return self\n")
    })
}

/// A whole program: a sequence of top-level functions and classes joined by blank lines.
fn gen_program() -> impl Strategy<Value = String> {
    let unit = prop_oneof![gen_function(), gen_class()];
    prop::collection::vec(unit, 0..6).prop_map(|units| units.join("\n"))
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(128))]

    /// Every chunk's byte span lies within `[0, file_len)` and slices back to its own text.
    #[test]
    fn every_chunk_span_is_in_bounds(source in gen_program()) {
        let mut parser = Parser::new().expect("Parser::new");
        let tree = parser
            .parse_file(Path::new("prop.py"), &source, Language::Python)
            .expect("parse generated program");
        let chunks = chunk(&tree, &source, Language::Python).expect("chunk generated program");

        for c in &chunks {
            prop_assert!(
                c.start_byte < c.end_byte && c.end_byte <= source.len(),
                "span [{}, {}) must satisfy start < end <= {}",
                c.start_byte, c.end_byte, source.len()
            );
            prop_assert_eq!(
                &source[c.start_byte..c.end_byte],
                c.chunk_text.as_str(),
                "chunk_text must equal source[start_byte..end_byte]"
            );
        }
    }

    /// Policy (a): any two chunks are either disjoint or nested (one contains the other) — never
    /// partially overlapping. Siblings are disjoint; a method is contained in its class.
    #[test]
    fn chunks_are_disjoint_or_nested(source in gen_program()) {
        let mut parser = Parser::new().expect("Parser::new");
        let tree = parser
            .parse_file(Path::new("prop.py"), &source, Language::Python)
            .expect("parse generated program");
        let chunks = chunk(&tree, &source, Language::Python).expect("chunk generated program");

        for (i, a) in chunks.iter().enumerate() {
            for b in &chunks[i + 1..] {
                let nested = contained(a, b) || contained(b, a);
                prop_assert!(
                    disjoint(a, b) || nested,
                    "chunks must be disjoint or nested, never partially overlapping: \
                     [{}, {}) {:?} vs [{}, {}) {:?}",
                    a.start_byte, a.end_byte, a.symbol_name,
                    b.start_byte, b.end_byte, b.symbol_name
                );
            }
        }
    }

    /// A child chunk (e.g. a method) whose `parent_symbol` names another emitted chunk must be
    /// fully contained inside that parent's span.
    #[test]
    fn child_is_contained_in_named_parent(source in gen_program()) {
        let mut parser = Parser::new().expect("Parser::new");
        let tree = parser
            .parse_file(Path::new("prop.py"), &source, Language::Python)
            .expect("parse generated program");
        let chunks = chunk(&tree, &source, Language::Python).expect("chunk generated program");

        for child in &chunks {
            let Some(parent_name) = child.parent_symbol.as_deref() else {
                continue;
            };
            // The parent name should resolve to an emitted chunk that strictly contains the child.
            let containing_parent = chunks.iter().find(|p| {
                p.symbol_name == parent_name && contained(child, p) && p.start_byte != child.start_byte
            });
            prop_assert!(
                containing_parent.is_some(),
                "child {:?} (parent_symbol = {:?}) must be contained in a chunk named {:?}",
                child.symbol_name, child.parent_symbol, parent_name
            );
        }
    }
}
