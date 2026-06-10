//! Schema DDL, the seeded `index_state`, and version migration.
//!
//! Source of truth: `project_plan.md` §4.1. FTS5 design notes (rust-treesitter-specialist):
//!
//! * **Table form — default (contentful) FTS5, not `content='symbols'`.** §4.1's pseudo-DDL
//!   writes `content='symbols'`, but in FTS5 the `content=` option names a *separate* external
//!   content table; pointing it at the FTS5 table's own name is not valid. For M1 we use a
//!   plain (contentful) FTS5 table: FTS5 stores every column value and returns it on `SELECT`,
//!   so a `Chunk` round-trips through `insert` → `search` with no companion table. This keeps
//!   the schema correct and the round-trip tests honest; an external-content optimization can
//!   be revisited at M10 if the <100MB index budget (§1.3) is ever threatened (it is not at
//!   Django scale — §4.2 estimates ~6MB).
//! * **Indexed vs UNINDEXED.** Searchable columns (D3): `symbol_name`, `symbol_type`,
//!   `chunk_text`, `parent_symbol`, `imports`, `cross_references`, `file_docstring`. Retrieval-only
//!   columns (UNINDEXED): `file_path`, `start_byte`, `end_byte`, `start_line`, `end_line` (D7),
//!   `language`. `file_docstring` is the last indexed column (D3 follow-up, 2026-06-10) so a term
//!   appearing only in a module docstring is matchable, not merely stored.
//! * **Tokenizer** `unicode61 remove_diacritics 2` — matches §4.1; folds diacritics so
//!   `café_handler` is reachable, while unicode61 keeps `_`-joined identifiers as single tokens.

/// Current schema/index version, mirroring the seeded `index_state.version` (§4.1).
pub const CURRENT_VERSION: &str = "0.1.0";

/// Column order for the `symbols` FTS5 table. Indexed columns first, then UNINDEXED. The insert
/// and search SQL in `queries.rs` rely on this exact order.
pub const CREATE_SYMBOLS: &str = "\
CREATE VIRTUAL TABLE IF NOT EXISTS symbols USING fts5(
    symbol_name,
    symbol_type,
    chunk_text,
    parent_symbol,
    imports,
    cross_references,
    file_docstring,
    file_path UNINDEXED,
    start_byte UNINDEXED,
    end_byte UNINDEXED,
    start_line UNINDEXED,
    end_line UNINDEXED,
    language UNINDEXED,
    tokenize = 'unicode61 remove_diacritics 2'
);";

/// `files_metadata` table (§4.1) — one row per indexed file for incremental updates.
pub const CREATE_FILES_METADATA: &str = "\
CREATE TABLE IF NOT EXISTS files_metadata (
    file_path    TEXT PRIMARY KEY,
    content_hash TEXT NOT NULL,
    mtime        INTEGER NOT NULL,
    file_size    INTEGER NOT NULL,
    language     TEXT NOT NULL,
    chunk_count  INTEGER NOT NULL,
    indexed_at   INTEGER NOT NULL
);";

/// Secondary indexes on `files_metadata` (§4.1).
pub const CREATE_FILES_INDEXES: &str = "\
CREATE INDEX IF NOT EXISTS idx_files_mtime ON files_metadata(mtime);
CREATE INDEX IF NOT EXISTS idx_files_language ON files_metadata(language);";

/// `index_state` key/value table (§4.1).
pub const CREATE_INDEX_STATE: &str = "\
CREATE TABLE IF NOT EXISTS index_state (
    key   TEXT PRIMARY KEY,
    value TEXT NOT NULL
);";

/// Seed the global state rows the first time the table is created (§4.1). Uses
/// `INSERT OR IGNORE` so re-running `init_schema` is idempotent and never clobbers live values.
pub const SEED_INDEX_STATE: &str = "\
INSERT OR IGNORE INTO index_state (key, value) VALUES
    ('version', '0.1.0'),
    ('created_at', strftime('%s', 'now')),
    ('last_full_index', '0'),
    ('total_files', '0'),
    ('total_chunks', '0');";
