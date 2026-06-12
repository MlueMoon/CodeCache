//! M6 slice M6.2 — retriever (BM25 search + determinism + dedup) integration tests (RED first).
//!
//! Scenarios: docs/TEST_STRATEGY.md#retriever and docs/plans/M6-retriever.md (Slice M6.2).
//! API anchor: docs/project_plan.md §3.2.3 (`Retriever`/`QueryOptions`/`QueryResult`) + §6.2.
//!
//! These tests seed `Storage` directly (no real indexing needed — M6 is independent of the
//! M3→M4→M5 chain) and exercise `Retriever::query` end to end over the seeded FTS5 index:
//! BM25 relevance ordering, deterministic + stable tie-break, no-match / empty-query → empty
//! well-formed result, dedup of overlapping spans in the same file, and the `file_filter`.
//!
//! Token-budget packing is NOT exercised here (that is M6.3); `max_tokens` is set generously so
//! it never trims, isolating the search/dedup/ordering behavior this slice owns.

use std::path::PathBuf;

use codecache::retriever::{QueryOptions, Retrieve, Retriever};
use codecache::storage::Storage;
use codecache::types::{Chunk, Language, SymbolType};

/// Build a `Storage` over a fresh temp DB with the schema initialized.
/// Returns (dir, storage); keep `dir` alive for the test's duration.
fn fresh_storage() -> (tempfile::TempDir, Storage) {
    let dir = tempfile::tempdir().expect("temp dir");
    let db_path = dir.path().join("index.db");
    let storage = Storage::new(&db_path).expect("open/create db");
    storage.init_schema().expect("init schema");
    (dir, storage)
}

/// Construct a `Chunk` with explicit byte span, so dedup-by-overlap tests can control spans.
#[allow(clippy::too_many_arguments)]
fn chunk_at(
    file: &str,
    name: &str,
    body: &str,
    start_byte: usize,
    end_byte: usize,
    start_line: usize,
    end_line: usize,
) -> Chunk {
    Chunk {
        symbol_name: name.to_string(),
        symbol_type: SymbolType::Function,
        file_path: PathBuf::from(file),
        start_byte,
        end_byte,
        start_line,
        end_line,
        chunk_text: body.to_string(),
        language: Language::Python,
        parent_symbol: None,
        file_docstring: None,
        imports: Vec::new(),
        cross_references: Vec::new(),
        is_heuristic: false,
    }
}

/// A simple chunk at line 1..10, bytes 0..len.
fn chunk(file: &str, name: &str, body: &str) -> Chunk {
    chunk_at(file, name, body, 0, body.len(), 1, 10)
}

/// Generous options: large token budget (no trimming in this slice), default-ish caps.
fn opts() -> QueryOptions {
    QueryOptions {
        max_tokens: 1_000_000,
        max_results: 20,
        file_filter: None,
    }
}

// ───────────────────────── relevance: relevant ranks above irrelevant ─────────────────────────

#[test]
fn relevant_chunk_ranks_above_irrelevant() {
    // Seed two chunks: one whose symbol name matches the query strongly, one unrelated.
    // The query "authenticate user" must rank the auth chunk first (BM25 name weighting).
    let (_dir, storage) = fresh_storage();
    let relevant = chunk(
        "src/auth.py",
        "authenticate_user",
        "def authenticate_user(): validate the user credentials",
    );
    let irrelevant = chunk(
        "src/math.py",
        "compute_factorial",
        "def compute_factorial(n): return product of range",
    );
    storage
        .insert_chunks(&[irrelevant, relevant])
        .expect("seed chunks");

    let retriever = Retriever::new(storage);
    let result = retriever
        .query("authenticate user", opts())
        .expect("query succeeds");

    assert!(
        !result.chunks.is_empty(),
        "the relevant chunk must be retrieved"
    );
    assert_eq!(
        result.chunks[0].chunk.symbol_name, "authenticate_user",
        "the strongly-matching chunk must rank first"
    );
    // total_results_found reflects how many matched before any (future) budget trimming.
    assert!(
        result.total_results_found >= 1,
        "found count reflects matches"
    );
}

// ───────────────────────── determinism + stable tie-break ─────────────────────────

#[test]
fn same_query_same_index_yields_identical_order() {
    // Several chunks that all match the same term equally (identical body text) so their BM25
    // scores tie; the retriever must apply a deterministic, stable tie-break so the order is
    // identical across repeated queries. Tie-break key documented as (file_path, start_byte).
    let (_dir, storage) = fresh_storage();
    let body = "handle request and return response";
    let chunks = vec![
        chunk_at("src/c.py", "c_handler", body, 0, body.len(), 1, 5),
        chunk_at("src/a.py", "a_handler", body, 0, body.len(), 1, 5),
        chunk_at("src/b.py", "b_handler", body, 0, body.len(), 1, 5),
        chunk_at(
            "src/a.py",
            "a_handler2",
            body,
            100,
            100 + body.len(),
            20,
            25,
        ),
    ];
    storage.insert_chunks(&chunks).expect("seed chunks");

    let retriever = Retriever::new(storage);
    let first = retriever
        .query("request response", opts())
        .expect("query 1");
    for _ in 0..5 {
        let again = retriever
            .query("request response", opts())
            .expect("query n");
        let order_first: Vec<_> = first
            .chunks
            .iter()
            .map(|r| (r.chunk.file_path.clone(), r.chunk.start_byte))
            .collect();
        let order_again: Vec<_> = again
            .chunks
            .iter()
            .map(|r| (r.chunk.file_path.clone(), r.chunk.start_byte))
            .collect();
        assert_eq!(
            order_first, order_again,
            "repeated identical queries must yield identical order"
        );
    }

    // The stable key is (file_path, start_byte): among tied scores, a.py(0) < a.py(100) < b.py < c.py.
    let order: Vec<(String, usize)> = first
        .chunks
        .iter()
        .map(|r| {
            (
                r.chunk.file_path.to_string_lossy().into_owned(),
                r.chunk.start_byte,
            )
        })
        .collect();
    assert_eq!(
        order,
        vec![
            ("src/a.py".to_string(), 0),
            ("src/a.py".to_string(), 100),
            ("src/b.py".to_string(), 0),
            ("src/c.py".to_string(), 0),
        ],
        "tied scores break by (file_path, start_byte) ascending"
    );
}

// ───────────────────────── no-match / empty query → empty well-formed result ─────────────────────────

#[test]
fn no_match_query_returns_empty_result() {
    // A query whose terms appear nowhere in the index returns an empty, well-formed result.
    let (_dir, storage) = fresh_storage();
    storage
        .insert_chunks(&[chunk("src/a.py", "alpha", "def alpha(): pass")])
        .expect("seed");

    let retriever = Retriever::new(storage);
    let result = retriever
        .query("nonexistentterm zzzqqq", opts())
        .expect("query succeeds even with no match");
    assert!(result.chunks.is_empty(), "no match ⇒ empty chunks");
    assert_eq!(result.total_results_found, 0, "no match ⇒ zero found");
    assert_eq!(result.total_tokens, 0, "no chunks ⇒ zero tokens");
}

#[test]
fn empty_or_all_stopword_query_short_circuits_without_running_match() {
    // Empty / whitespace / all-stopword queries reduce to no tokens after preprocessing. The
    // retriever must short-circuit to an empty, well-formed result WITHOUT ever issuing
    // `MATCH ""` (which FTS5 rejects). If it tried, the call would error — so success here proves
    // the short-circuit path.
    let (_dir, storage) = fresh_storage();
    storage
        .insert_chunks(&[chunk("src/a.py", "alpha", "def alpha(): pass")])
        .expect("seed");

    let retriever = Retriever::new(storage);
    for q in ["", "   ", "find the"] {
        let result = retriever
            .query(q, opts())
            .expect("empty/all-stopword query must not error (no MATCH \"\")");
        assert!(
            result.chunks.is_empty(),
            "no tokens ⇒ empty chunks for {q:?}"
        );
        assert_eq!(
            result.total_results_found, 0,
            "no tokens ⇒ zero found for {q:?}"
        );
        assert_eq!(result.total_tokens, 0, "no tokens ⇒ zero tokens for {q:?}");
    }
}

#[test]
fn empty_db_query_returns_empty_result_without_panic() {
    // Cross-cutting: querying an empty index is a well-formed empty result, not a panic/error.
    let (_dir, storage) = fresh_storage();
    let retriever = Retriever::new(storage);
    let result = retriever
        .query("anything at all", opts())
        .expect("query empty db");
    assert!(result.chunks.is_empty());
    assert_eq!(result.total_results_found, 0);
}

// ───────────────────────── dedup overlapping snippets ─────────────────────────

#[test]
fn overlapping_snippets_deduplicated() {
    // Two chunks in the SAME file whose byte spans overlap must collapse to one in the result
    // (keep the better-ranked / first-encountered). A chunk in a different file, or a
    // non-overlapping span in the same file, is kept.
    let (_dir, storage) = fresh_storage();
    let body = "process payment and charge the card";
    // a.py: [0,50) and [40,90) overlap (40 < 50) ⇒ one survives.
    // a.py: [200,250) does NOT overlap the first cluster ⇒ kept.
    // b.py: [0,50) is a different file ⇒ kept even though byte span coincides.
    let chunks = vec![
        chunk_at("src/a.py", "process_payment", body, 0, 50, 1, 5),
        chunk_at("src/a.py", "process_payment_dup", body, 40, 90, 4, 9),
        chunk_at("src/a.py", "charge_card", body, 200, 250, 30, 35),
        chunk_at("src/b.py", "b_payment", body, 0, 50, 1, 5),
    ];
    storage.insert_chunks(&chunks).expect("seed");

    let retriever = Retriever::new(storage);
    let result = retriever.query("payment charge", opts()).expect("query");

    // 4 seeded, one overlapping pair collapses ⇒ 3 distinct results.
    assert_eq!(
        result.chunks.len(),
        3,
        "overlapping same-file spans collapse to one"
    );

    // No two surviving results share a file AND overlap in bytes.
    for i in 0..result.chunks.len() {
        for j in (i + 1)..result.chunks.len() {
            let a = &result.chunks[i].chunk;
            let b = &result.chunks[j].chunk;
            if a.file_path == b.file_path {
                let overlap = a.start_byte < b.end_byte && b.start_byte < a.end_byte;
                assert!(
                    !overlap,
                    "surviving results in the same file must not overlap: {:?} vs {:?}",
                    (a.start_byte, a.end_byte),
                    (b.start_byte, b.end_byte)
                );
            }
        }
    }
}

// ───────────────────────── file_filter ─────────────────────────

#[test]
fn file_filter_restricts_results_to_listed_files() {
    // With a file_filter, only chunks whose file_path is in the listed set survive.
    let (_dir, storage) = fresh_storage();
    let body = "load configuration from disk";
    let chunks = vec![
        chunk("src/keep.py", "load_config", body),
        chunk("src/drop.py", "load_config_other", body),
    ];
    storage.insert_chunks(&chunks).expect("seed");

    let retriever = Retriever::new(storage);
    let options = QueryOptions {
        max_tokens: 1_000_000,
        max_results: 20,
        file_filter: Some(vec![PathBuf::from("src/keep.py")]),
    };
    let result = retriever
        .query("configuration load", options)
        .expect("query");

    assert!(!result.chunks.is_empty(), "the kept file must still match");
    for r in &result.chunks {
        assert_eq!(
            r.chunk.file_path,
            PathBuf::from("src/keep.py"),
            "only listed files survive the filter"
        );
    }
}
