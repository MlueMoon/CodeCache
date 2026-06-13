//! M10.1 plan-capture tool: print `EXPLAIN QUERY PLAN` for the §6 retrieval `SEARCH` SQL against a
//! PERSISTENT on-disk fixture DB. Kept as a reproducible FTS5 query-plan baseline (R2/R3 reuse it).
//!
//! Why this exists: M6.4's `query_bench.rs` seeds a per-run tempfile that is torn down before the
//! plan can be read, so the FTS5 query plan was never captured. This harness seeds a persistent
//! DB at a fixed path using ONLY the public `Storage` API (same seed shape as `query_bench.rs`:
//! 5_000 synthetic chunks × ~20 LOC ≈ 100K LOC), then opens that same file with a fresh
//! `rusqlite` connection and runs `EXPLAIN QUERY PLAN <SEARCH>` with a representative MATCH term
//! (`authenticate OR user`, what the retriever preprocesses "authenticate user" into) + LIMIT 20.
//!
//! The SEARCH SQL string below is copied VERBATIM from `src/storage/queries.rs::SEARCH` (which is
//! private to the storage module). It is the exact statement `Storage::search` runs. If that query
//! changes, update this copy (and `benches/CLAUDE.md`'s recorded plan) in the same change.
//!
//! Run:  cargo run --release --example explain_query_plan
//!
//! Not run by `cargo test`; the seeded DB at the printed path is recreated each run.

use std::path::PathBuf;

use codecache::storage::Storage;
use codecache::types::{Chunk, Language, SymbolType};
use rusqlite::Connection;

const CHUNK_COUNT: usize = 5_000;
const LOC_PER_CHUNK: usize = 20;

/// Verbatim copy of `src/storage/queries.rs::SEARCH` (private there). Quoted here so the plan is
/// read against the exact statement production runs.
const SEARCH: &str = "\
SELECT
    symbol_name, symbol_type, chunk_text, parent_symbol, imports, cross_references, file_docstring,
    file_path, start_byte, end_byte, start_line, end_line, language,
    bm25(symbols, 10.0, 1.0, 1.0, 5.0, 2.0, 2.0, 2.0) AS score
FROM symbols
WHERE symbols MATCH ?1
ORDER BY score ASC, rowid ASC
LIMIT ?2;";

fn synthetic_body(i: usize) -> String {
    let domain = if i % 20 == 0 {
        "authenticate user validate credentials session token"
    } else {
        "compute transform aggregate serialize render dispatch"
    };
    let mut body = format!("def symbol_{i}(arg_{i}):\n    \"\"\"{domain} for {i}.\"\"\"\n");
    for line in 0..LOC_PER_CHUNK {
        body.push_str(&format!("    step_{line} = {domain} + {i}\n"));
    }
    body
}

fn synthetic_chunk(i: usize) -> Chunk {
    let body = synthetic_body(i);
    let start = i * 10_000;
    let end = start + body.len();
    Chunk {
        symbol_name: format!("symbol_{i}"),
        symbol_type: SymbolType::Function,
        file_path: PathBuf::from(format!("src/mod_{:05}.py", i / 50)),
        start_byte: start,
        end_byte: end,
        start_line: 1,
        end_line: LOC_PER_CHUNK + 2,
        chunk_text: body,
        language: Language::Python,
        parent_symbol: None,
        file_docstring: None,
        imports: Vec::new(),
        cross_references: Vec::new(),
        is_heuristic: false,
    }
}

fn main() {
    // Persistent on-disk DB at a fixed path (NOT a tempfile) so it survives the EXPLAIN run.
    let db_path = std::env::temp_dir().join("codecache_m10_explain_fixture.db");
    let _ = std::fs::remove_file(&db_path); // start clean if re-run
    println!("persistent fixture DB: {}", db_path.display());

    // Seed via the public Storage API — same seed shape as benches/query_bench.rs.
    {
        let storage = Storage::new(&db_path).expect("open storage");
        storage.init_schema().expect("init schema");
        const BATCH: usize = 500;
        let mut batch: Vec<Chunk> = Vec::with_capacity(BATCH);
        for i in 0..CHUNK_COUNT {
            batch.push(synthetic_chunk(i));
            if batch.len() == BATCH {
                storage.insert_chunks(&batch).expect("seed insert_chunks");
                batch.clear();
            }
        }
        if !batch.is_empty() {
            storage.insert_chunks(&batch).expect("seed tail");
        }
    } // storage dropped; file persists.

    // Re-open the SAME persistent file with a fresh connection and read the plan.
    let conn = Connection::open(&db_path).expect("reopen persistent db");

    let count: i64 = conn
        .query_row("SELECT count(*) FROM symbols;", [], |r| r.get(0))
        .expect("count symbols");
    println!("rows in symbols FTS5 table: {count}");

    let match_term = "authenticate OR user"; // retriever output for "authenticate user"
    let limit: i64 = 20;

    println!("\n--- SEARCH SQL (verbatim) ---\n{SEARCH}");
    println!("\n--- EXPLAIN QUERY PLAN (MATCH = {match_term:?}, LIMIT {limit}) ---");

    let explain = format!("EXPLAIN QUERY PLAN {SEARCH}");
    let mut stmt = conn.prepare(&explain).expect("prepare EXPLAIN");
    let rows = stmt
        .query_map(rusqlite::params![match_term, limit], |r| {
            // EXPLAIN QUERY PLAN columns: id, parent, notused, detail
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, i64>(1)?,
                r.get::<_, String>(3)?,
            ))
        })
        .expect("run EXPLAIN");
    for row in rows {
        let (id, parent, detail) = row.expect("read plan row");
        println!("id={id} parent={parent} | {detail}");
    }
}
