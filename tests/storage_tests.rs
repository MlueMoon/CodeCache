//! M1 slices M1.2–M1.4 — storage (SQLite + FTS5) integration tests (RED first).
//!
//! Scenarios: docs/TEST_STRATEGY.md#storage-sqlite--fts5 and docs/plans/M1-config-storage.md.
//! API anchor: docs/project_plan.md §3.2.2 (API) + §4.1 (schema) + §4.3 (types).
//! All DB state isolated via `tempfile`; assertions check real column values and ordering.

use std::path::{Path, PathBuf};

use codecache::storage::Storage;
use codecache::types::{Chunk, FileMeta, Language, SymbolType};

/// Build a `Storage` over a fresh temp DB with the schema initialized.
/// Returns (dir, storage); keep `dir` alive for the test's duration.
fn fresh_storage() -> (tempfile::TempDir, Storage) {
    let dir = tempfile::tempdir().expect("temp dir");
    let db_path = dir.path().join("index.db");
    let storage = Storage::new(&db_path).expect("open/create db");
    storage.init_schema().expect("init schema");
    (dir, storage)
}

/// Construct a minimal-but-complete `Chunk` for a given file/symbol/body.
fn chunk(file: &str, name: &str, body: &str) -> Chunk {
    Chunk {
        symbol_name: name.to_string(),
        symbol_type: SymbolType::Function,
        file_path: PathBuf::from(file),
        start_byte: 0,
        end_byte: body.len(),
        start_line: 1,
        end_line: 10,
        chunk_text: body.to_string(),
        language: Language::Python,
        parent_symbol: None,
        file_docstring: None,
        imports: Vec::new(),
        cross_references: Vec::new(),
        is_heuristic: false,
    }
}

/// Construct a `Chunk` whose only distinguishing text lives in `file_docstring`, so a query that
/// matches must be matching the (indexed) docstring column. `symbol_name`/`chunk_text` are kept
/// free of the docstring term on purpose.
fn chunk_with_docstring(file: &str, name: &str, body: &str, docstring: &str) -> Chunk {
    let mut c = chunk(file, name, body);
    c.file_docstring = Some(docstring.to_string());
    c
}

// ───────────────────────── M1.2 — schema, idempotency, migration ─────────────────────────

#[test]
fn new_db_creates_all_tables_expects_symbols_files_index_state() {
    let dir = tempfile::tempdir().expect("temp dir");
    let db_path = dir.path().join("index.db");
    let storage = Storage::new(&db_path).expect("open db");

    // init_schema must succeed and create the three logical stores; a search on the freshly
    // created (empty) symbols table proves the FTS5 virtual table exists (D9).
    storage.init_schema().expect("init schema creates tables");
    let results = storage
        .search("anything", 10)
        .expect("search empty symbols table");
    assert!(results.is_empty(), "fresh db search returns empty");

    // files_metadata exists and is empty.
    let hash = storage
        .get_file_hash(Path::new("nope.py"))
        .expect("query files_metadata");
    assert_eq!(hash, None, "no metadata for unknown file");
}

#[test]
fn init_schema_twice_expects_no_error_idempotent() {
    let (_dir, storage) = fresh_storage();
    // Second init must be a no-op, not an error (CREATE ... IF NOT EXISTS).
    storage
        .init_schema()
        .expect("second init_schema is idempotent");
}

#[test]
fn older_version_db_expects_migration_to_current() {
    // Open a db, init schema, then simulate an older version by setting index_state.version
    // backwards. A subsequent init_schema must migrate it forward to the current version.
    let (_dir, storage) = fresh_storage();
    storage
        .set_index_state("version", "0.0.1")
        .expect("seed older version");

    storage.init_schema().expect("re-init triggers migration");

    let version = storage
        .get_index_state("version")
        .expect("read version")
        .expect("version present");
    assert_eq!(version, "0.1.0", "migration bumps version to current");
}

#[test]
fn corrupt_db_file_expects_typed_error_not_panic() {
    // A file that is not a valid SQLite database must surface a typed error, never a panic.
    let dir = tempfile::tempdir().expect("temp dir");
    let db_path = dir.path().join("corrupt.db");
    std::fs::write(&db_path, b"this is definitely not sqlite").expect("write garbage");

    let result = Storage::new(&db_path).and_then(|s| s.init_schema().map(|_| s));
    assert!(
        result.is_err(),
        "corrupt db file must yield a typed error, not a panic"
    );
}

// ───────────────────────── M1.3 — CRUD round-trip + delete-by-file ─────────────────────────

#[test]
fn insert_then_search_returns_inserted_chunk_with_fields() {
    let (_dir, storage) = fresh_storage();
    let mut c = chunk(
        "src/auth/handlers.py",
        "authenticate_user",
        "def authenticate_user(username, password):\n    return verify(username, password)",
    );
    c.symbol_type = SymbolType::Function;
    c.start_byte = 12;
    c.end_byte = 90;
    c.start_line = 45;
    c.end_line = 67;
    c.language = Language::Python;

    storage.insert_chunks(&[c.clone()]).expect("insert chunk");

    let results = storage.search("authenticate_user", 10).expect("search");
    assert_eq!(results.len(), 1, "exactly one match");
    let got = &results[0].chunk;
    assert_eq!(got.symbol_name, "authenticate_user");
    assert_eq!(got.symbol_type, SymbolType::Function);
    assert_eq!(got.file_path, PathBuf::from("src/auth/handlers.py"));
    assert_eq!(got.start_byte, 12);
    assert_eq!(got.end_byte, 90);
    assert_eq!(got.start_line, 45, "D7: start_line round-trips");
    assert_eq!(got.end_line, 67, "D7: end_line round-trips");
    assert_eq!(got.language, Language::Python);
    assert_eq!(got.chunk_text, c.chunk_text);
}

#[test]
fn insert_then_search_round_trips_some_file_docstring() {
    // D3: file_docstring is a persisted+indexed enrichment column. A Some(..) value must survive
    // insert → search byte-for-byte. The docstring term is unique so the chunk is findable, but
    // we assert on the *reconstructed field*, not just the match.
    let (_dir, storage) = fresh_storage();
    let c = chunk_with_docstring(
        "src/payments/gateway.py",
        "charge",
        "def charge(amount):\n    pass",
        "Module docstring: orchestrates the stripe billing workflow.",
    );

    storage.insert_chunks(&[c.clone()]).expect("insert chunk");

    let results = storage.search("charge", 10).expect("search");
    assert_eq!(results.len(), 1, "exactly one match");
    assert_eq!(
        results[0].chunk.file_docstring,
        Some("Module docstring: orchestrates the stripe billing workflow.".to_string()),
        "Some(file_docstring) round-trips through insert→search"
    );
}

#[test]
fn insert_then_search_round_trips_none_file_docstring() {
    // The absence of a docstring must round-trip as None (not Some("")). `chunk()` sets None.
    let (_dir, storage) = fresh_storage();
    let c = chunk("src/util.py", "helper", "def helper():\n    pass");
    assert_eq!(
        c.file_docstring, None,
        "helper builds a None-docstring chunk"
    );

    storage.insert_chunks(&[c]).expect("insert chunk");

    let results = storage.search("helper", 10).expect("search");
    assert_eq!(results.len(), 1, "exactly one match");
    assert_eq!(
        results[0].chunk.file_docstring, None,
        "absent docstring round-trips as None, not Some(\"\")"
    );
}

#[test]
fn bulk_insert_many_chunks_expects_all_present() {
    let (_dir, storage) = fresh_storage();
    let chunks: Vec<Chunk> = (0..50)
        .map(|i| {
            chunk(
                "src/mod.py",
                &format!("widget_{i}"),
                &format!("def widget_{i}():\n    return shared_marker_term"),
            )
        })
        .collect();

    storage.insert_chunks(&chunks).expect("bulk insert");

    let results = storage
        .search("shared_marker_term", 100)
        .expect("search all");
    assert_eq!(results.len(), 50, "all 50 chunks present");
}

#[test]
fn delete_chunks_for_file_removes_only_that_files_chunks() {
    let (_dir, storage) = fresh_storage();
    storage
        .insert_chunks(&[
            chunk("a.py", "alpha", "def alpha():\n    common_term"),
            chunk("b.py", "beta", "def beta():\n    common_term"),
        ])
        .expect("insert two files");

    storage
        .delete_chunks_for_file(Path::new("a.py"))
        .expect("delete a.py chunks");

    let results = storage
        .search("common_term", 10)
        .expect("search after delete");
    assert_eq!(results.len(), 1, "only b.py remains");
    assert_eq!(results[0].chunk.file_path, PathBuf::from("b.py"));
}

#[test]
fn update_then_get_file_hash_round_trips_filemeta() {
    let (_dir, storage) = fresh_storage();
    let path = Path::new("src/auth/handlers.py");
    let meta = FileMeta {
        content_hash: "0123456789abcdef0123456789abcdef".to_string(),
        mtime: 1_700_000_000,
        file_size: 2048,
        language: Language::Python,
        chunk_count: 7,
    };

    storage
        .update_file_hash(path, &meta)
        .expect("write file meta");

    let hash = storage.get_file_hash(path).expect("read hash");
    assert_eq!(
        hash,
        Some("0123456789abcdef0123456789abcdef".to_string()),
        "content_hash round-trips"
    );

    // The full FileMeta must be persisted (D6) — verify via the read-back accessor.
    let stored = storage
        .get_file_meta(path)
        .expect("read meta")
        .expect("meta present");
    assert_eq!(stored.content_hash, meta.content_hash);
    assert_eq!(stored.mtime, meta.mtime);
    assert_eq!(stored.file_size, meta.file_size);
    assert_eq!(stored.language, meta.language);
    assert_eq!(stored.chunk_count, meta.chunk_count);
}

#[test]
fn empty_db_search_expects_empty_vec() {
    let (_dir, storage) = fresh_storage();
    let results = storage.search("authenticate", 20).expect("search empty db");
    assert!(results.is_empty(), "no rows ⇒ empty vec, not an error");
}

// ───────────────────────── M1.4 — FTS5 MATCH + bm25 ordering ─────────────────────────

#[test]
fn match_query_returns_rows_containing_term() {
    let (_dir, storage) = fresh_storage();
    storage
        .insert_chunks(&[
            chunk(
                "auth.py",
                "login",
                "def login():\n    authenticate the request",
            ),
            chunk("math.py", "add", "def add(a, b):\n    return a + b"),
        ])
        .expect("insert");

    let results = storage.search("authenticate", 10).expect("match search");
    assert_eq!(results.len(), 1, "only the matching row");
    assert_eq!(results[0].chunk.symbol_name, "login");
}

#[test]
fn bm25_orders_more_relevant_chunk_first() {
    let (_dir, storage) = fresh_storage();
    // One chunk repeats the term many times → higher relevance; one mentions it once.
    let dense = chunk("dense.py", "dense", "token token token token token here");
    let sparse = chunk("sparse.py", "sparse", "token appears once here");
    storage
        .insert_chunks(&[sparse, dense])
        .expect("insert both");

    let results = storage.search("token", 10).expect("ranked search");
    assert_eq!(results.len(), 2, "both match");
    assert_eq!(
        results[0].chunk.symbol_name, "dense",
        "more relevant chunk ranks first"
    );
    // FTS5 bm25() returns more-negative for better matches; lower (more negative) sorts first.
    assert!(
        results[0].bm25_score <= results[1].bm25_score,
        "scores are ordered best-first ({} <= {})",
        results[0].bm25_score,
        results[1].bm25_score
    );
}

#[test]
fn unindexed_columns_not_searchable() {
    let (_dir, storage) = fresh_storage();
    // The unique term lives only in the file_path (UNINDEXED) — must NOT match.
    storage
        .insert_chunks(&[chunk(
            "src/uniquepathtoken/m.py",
            "handler",
            "def handler():\n    pass",
        )])
        .expect("insert");

    let results = storage
        .search("uniquepathtoken", 10)
        .expect("search unindexed term");
    assert!(
        results.is_empty(),
        "terms only in UNINDEXED columns are not searchable"
    );
}

#[test]
fn term_only_in_file_docstring_is_matchable() {
    // D3: file_docstring is an INDEXED column. A term that appears only in the docstring (not in
    // symbol_name or chunk_text) must be matchable — this distinguishes "indexed" from merely
    // "stored" and is the regression guard for the missing-column gate failure.
    let (_dir, storage) = fresh_storage();
    storage
        .insert_chunks(&[chunk_with_docstring(
            "src/telemetry/mod.py",
            "emit",
            "def emit(event):\n    pass",
            "This module handles distributedtracingspans for the collector.",
        )])
        .expect("insert");

    let results = storage
        .search("distributedtracingspans", 10)
        .expect("search docstring-only term");
    assert_eq!(
        results.len(),
        1,
        "a term living only in the indexed file_docstring is searchable"
    );
    assert_eq!(results[0].chunk.symbol_name, "emit");
}

#[test]
fn column_weighting_respected() {
    let (_dir, storage) = fresh_storage();
    // "session" appears as the symbol_name of one chunk and only in the body of another.
    let name_match = chunk("a.py", "session", "def session():\n    pass");
    let body_match = chunk(
        "b.py",
        "helper",
        "def helper():\n    open the session and the session again",
    );
    storage
        .insert_chunks(&[body_match, name_match])
        .expect("insert");

    let results = storage.search("session", 10).expect("weighted search");
    assert_eq!(results.len(), 2);
    assert_eq!(
        results[0].chunk.symbol_name, "session",
        "symbol_name (weighted) match outranks body-only match"
    );
}

// ───────────────────────── Cross-cutting (TEST_STRATEGY) ─────────────────────────

#[test]
fn utf8_multibyte_identifier_round_trips_through_search() {
    let (_dir, storage) = fresh_storage();
    storage
        .insert_chunks(&[chunk(
            "héllo.py",
            "café_handler",
            "def café_handler():\n    return münchen",
        )])
        .expect("insert multibyte");

    let results = storage
        .search("café_handler", 10)
        .expect("search multibyte");
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].chunk.symbol_name, "café_handler");
    assert_eq!(results[0].chunk.file_path, PathBuf::from("héllo.py"));
}

#[test]
fn same_inserts_expect_deterministic_ordering() {
    let build = || {
        let dir = tempfile::tempdir().expect("temp dir");
        let storage = Storage::new(&dir.path().join("index.db")).expect("open");
        storage.init_schema().expect("schema");
        storage
            .insert_chunks(&[
                chunk("a.py", "one", "term term other"),
                chunk("b.py", "two", "term something"),
                chunk("c.py", "three", "term term term"),
            ])
            .expect("insert");
        let names: Vec<String> = storage
            .search("term", 10)
            .expect("search")
            .into_iter()
            .map(|r| r.chunk.symbol_name)
            .collect();
        (dir, names)
    };
    let (_d1, first) = build();
    let (_d2, second) = build();
    assert_eq!(first, second, "identical inserts ⇒ identical ordering");
}
