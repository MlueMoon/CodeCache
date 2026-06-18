# BRIEF — M1 / config + storage (types → config → storage, FTS5)

- **Milestone:** M1 — config + storage  ·  **Module(s):** `types`, `config`, `storage`
- **Owner (manager):** principal-engineering-manager  ·  **Created:** 2026-06-10
- **Status:** RED ✅  GREEN ✅  REVIEW ✅ (conditional)  DONE ▢ (awaiting gate execution)
- **Links:** docs/ROADMAP.md#m1--config--storage-sqlite-schema--fts5 · docs/TEST_STRATEGY.md#config · docs/TEST_STRATEGY.md#storage-sqlite--fts5 · docs/plans/M1-config-storage.md
- **Spec (source of truth):** project_plan.md §3.2.1/§4.3 (types), §7.3 (config), §3.2.2 + §4.1 (storage)

## Goal
Stand up the dependency-free `crate::types` core types, load+validate `.codecache/config.toml`
with documented defaults, and build the SQLite storage layer: FTS5 `symbols` table +
`files_metadata` + `index_state`, with idempotent schema/migration, CRUD round-trip, and BM25
`MATCH` search.

## Scope (in / out)
- **In:** `types` (`Chunk`, `Language`, `SymbolType`, `FileMeta`); `config` (`Config`, `load`,
  defaults, validation, typed errors); `storage` (`Storage::new`, `init_schema`, `insert_chunks`,
  `delete_chunks_for_file`, `search`, `get_file_hash`, `update_file_hash`, `SearchResult`).
- **Out:** hashing (M2), parsing/chunking that *produces* `Chunk`s (M3/M4), the indexer
  orchestration that *calls* storage (M5), retriever token budget (M6). Build `Chunk`s by hand
  in tests. `Arc<Mutex<Connection>>` is **designed in now** (D8) but multi-consumer sharing is
  only exercised at M8 — do not add MCP wiring here.

## Build order within the slice (honor ENGINEERING_PLAN §2, Decision D5)
1. `crate::types` FIRST (zero deps; storage depends on types, NOT on parser).
2. `config` (independent of storage — can be built in parallel; pair at milestone exit).
3. `storage` (depends on `types`).

## Ratified decisions to honor (do not re-litigate)
- **D5** — `Chunk`/`Language`/`SymbolType`/`FileMeta` live in `crate::types`, dependency-free.
- **D6** — `update_file_hash(file_path: &Path, meta: &FileMeta)` where
  `FileMeta { content_hash: String, mtime: u64, file_size: u64, language: Language, chunk_count: usize }`.
- **D7** — `symbols` gets `start_line`/`end_line` UNINDEXED; `Chunk` carries `start_line`/`end_line`
  (1-based, inclusive).
- **D8** — `Storage` wraps `Arc<Mutex<rusqlite::Connection>>` (Connection isn't Clone); `Storage`
  is therefore cheaply `Clone`. Design the type this way now so M8 needs no rework.
- **D3** — `symbols` carries indexed FTS5 columns `parent_symbol`, `imports`, `cross_references`
  so M4 can populate without a migration. (For M1, tests may leave these empty/None; the
  *columns must exist and be searchable*.)
- **D9** — FTS5 ships in the bundled SQLite build (rusqlite 0.32, `features=["bundled"]`, no
  `fts5` feature). The first `CREATE VIRTUAL TABLE ... USING fts5` proves it.

## API contract (verbatim from project_plan §3.2.2 / §4.3 — implement exactly)
```rust
// crate::types
pub struct Chunk {
    pub symbol_name: String, pub symbol_type: SymbolType, pub file_path: PathBuf,
    pub start_byte: usize, pub end_byte: usize,
    pub start_line: usize, pub end_line: usize,        // D7: 1-based inclusive
    pub chunk_text: String, pub language: Language,
    pub parent_symbol: Option<String>, pub file_docstring: Option<String>,  // D3
    pub imports: Vec<String>, pub cross_references: Vec<String>,            // D3
}
pub enum SymbolType { Function, Class, Method, Struct }
pub enum Language { Python, TypeScript, Go }
pub struct FileMeta {  // D6
    pub content_hash: String, pub mtime: u64, pub file_size: u64,
    pub language: Language, pub chunk_count: usize,
}

// crate::storage
pub struct Storage { /* conn: Arc<Mutex<rusqlite::Connection>> (D8) */ }
impl Storage {
    pub fn new(db_path: &Path) -> Result<Self>;
    pub fn init_schema(&self) -> Result<()>;
    pub fn insert_chunks(&self, chunks: &[Chunk]) -> Result<()>;
    pub fn delete_chunks_for_file(&self, file_path: &Path) -> Result<()>;
    pub fn search(&self, query: &str, limit: usize) -> Result<Vec<SearchResult>>;
    pub fn get_file_hash(&self, file_path: &Path) -> Result<Option<String>>;
    pub fn update_file_hash(&self, file_path: &Path, meta: &FileMeta) -> Result<()>;
}
pub struct SearchResult { pub chunk: Chunk, pub bm25_score: f64 }
```
**SQL schema:** verbatim from project_plan §4.1 — `symbols` FTS5 (`symbol_name`, `symbol_type`,
`chunk_text`, `parent_symbol`, `imports`, `cross_references` indexed; `file_path`, `start_byte`,
`end_byte`, `start_line`, `end_line`, `language` UNINDEXED; `tokenize='unicode61
remove_diacritics 2'`); `files_metadata` (`file_path` PK, `content_hash`, `mtime`, `file_size`,
`language`, `chunk_count`, `indexed_at`) + `idx_files_mtime` + `idx_files_language`; `index_state`
(key/value) seeded with `version='0.1.0'` etc.

**`config` shape:** mirror project_plan §7.3 keys exactly. Defaults when omitted (§6/§7.3):
`max_tokens=4000`, `max_results=20`, `bm25_k1=1.2`, `bm25_b=0.75`,
`languages=["python","typescript","go"]`. Apply via `#[serde(default = ...)]`.

## Scenarios to cover (from TEST_STRATEGY + M1 plan slices)
### Slice M1.1 — config (tests/config_tests.rs)
- [ ] `valid_toml_loads_all_fields_expects_populated_config`
- [ ] `omitted_fields_expects_documented_defaults` (assert each default value above)
- [ ] `missing_file_expects_typed_error`
- [ ] `invalid_toml_expects_typed_error` (malformed TOML → typed error, no panic)
- [ ] `ignore_pattern_parsing_correct`
### Slice M1.2 — schema + idempotency + migration (tests/storage_tests.rs)
- [ ] `new_db_creates_all_tables_expects_symbols_files_index_state`
- [ ] `init_schema_twice_expects_no_error_idempotent`
- [ ] `older_version_db_expects_migration_to_current` (seed `index_state.version` < current)
- [ ] `corrupt_db_file_expects_typed_error_not_panic`
### Slice M1.3 — CRUD round-trip + delete-by-file
- [ ] `insert_then_search_returns_inserted_chunk_with_fields` (assert ALL columns survive incl.
      start_line/end_line, language, symbol_type)
- [ ] `bulk_insert_many_chunks_expects_all_present` (transaction batch)
- [ ] `delete_chunks_for_file_removes_only_that_files_chunks`
- [ ] `update_then_get_file_hash_round_trips_filemeta` (D6: verify hash via `get_file_hash`;
      file_size/chunk_count/language/mtime persisted into files_metadata)
- [ ] `empty_db_search_expects_empty_vec`
### Slice M1.4 — FTS5 MATCH + bm25 ordering
- [ ] `match_query_returns_rows_containing_term`
- [ ] `bm25_orders_more_relevant_chunk_first` (term repeated → ranks higher; assert ordering)
- [ ] `unindexed_columns_not_searchable` (term only in `file_path` ⇒ no match)
- [ ] `column_weighting_respected` (name match outranks body-only match if weights set)
### Cross-cutting (TEST_STRATEGY §Cross-cutting) where applicable
- [ ] UTF-8 multibyte identifier survives insert→search round-trip
- [ ] Determinism: same inserts ⇒ identical search ordering (stable tie-break)
- [ ] Isolate all DB state via `tempfile`; assert real values, never just `is_ok()`

## Engineering standards (enforced)
- No reachable `unwrap()/expect()/panic!` in production code — `Result` + `?`, typed errors.
- Keep FTS5 statements prepared/cached (forward-looking for M6 p95<500ms; FTS5 search <50ms).
- Match documented APIs exactly; if a divergence is needed, STOP and raise to manager
  (project_plan.md is updated first).

## Performance note (perf engineer, if warranted)
No hard budget gate at M1. Record `EXPLAIN QUERY PLAN` for the §6.1 search SQL in this brief so
M6's latency budget has a baseline. External-content FTS5 keeps the index under the <100MB budget.

## Definition of Done
- [ ] All M1.1–M1.4 slices green; assertions check real column values + ordering.
- [ ] Schema idempotent + migration path tested; corrupt/locked DB → typed error, no panic.
- [ ] `types` location = `crate::types` (D5); `update_file_hash` uses `&FileMeta` (D6); line
      columns present (D7); `Storage` wraps `Arc<Mutex<Connection>>` (D8); FTS5 proven (D9).
- [ ] `cargo clippy --all-targets -- -D warnings` clean · `cargo fmt --check` clean.
- [ ] API matches project_plan §3.2.2 / §4.1 / §4.3 (or plan updated first).
- [ ] reviewer APPROVED.
- [ ] docs/TODO.md Phase 1 + `src/types`/`src/config`/`src/storage` CLAUDE.md updated.
- [ ] Manager runs real gates with Rust 1.85.0 on PATH (handed back to human session) — done is
      NOT declared on inspection alone.

---
## RED — test lead
Status: RED ✅ (tests written first; do not compile until the impl exists — canonical Rust RED).

Tests added:
- `src/types/mod.rs` `#[cfg(test)] mod tests` (4 unit tests): pins `Chunk` full field set incl.
  D7 line range + D3 enrichment; `FileMeta` D6 bundle; `Language`/`SymbolType` `as_str()` +
  `from_str_lenient()` round-trips (needed because storage persists them as text in §4.1).
- `tests/config_tests.rs` (5 tests, M1.1): full load, documented defaults
  (4000/20/1.2/0.75/[py,ts,go]), missing-file typed error, malformed-TOML typed error,
  ignore-pattern parsing.
- `tests/storage_tests.rs` (M1.2–M1.4 + cross-cutting, 15 tests): table creation incl. FTS5
  proof (D9), idempotent re-init, version migration, corrupt-db typed error; insert→search full
  field round-trip (incl. start_line/end_line), bulk insert (50), delete-by-file isolation,
  `update_file_hash`→`get_file_hash`/`get_file_meta` D6 round-trip, empty-db→empty vec; FTS5
  MATCH, bm25 best-first ordering, UNINDEXED-not-searchable, column weighting; UTF-8 multibyte
  round-trip, deterministic ordering.

Impl must satisfy (surface the tests require beyond the public API contract):
- `Language::as_str()` / `Language::from_str_lenient(&str) -> Option<Language>`.
- `SymbolType::as_str()` / `SymbolType::from_str_lenient(&str) -> Option<SymbolType>`.
- `Chunk` and `FileMeta` derive `Clone`; `Language`/`SymbolType` derive `Clone+Copy+PartialEq+Debug`.
- Storage test helpers call `set_index_state`/`get_index_state` and `get_file_meta` — expose
  these as `pub` on `Storage` (they are natural read/write accessors for `index_state` and
  `files_metadata` and will be reused by M5/M7/`status`). Not in the §3.2.2 minimal list but
  additive, not divergent — recorded here for the reviewer.
- bm25 ordering: best match first (FTS5 `ORDER BY bm25(symbols)` ascending — more negative first).

## GREEN — engineering lead
Status: GREEN (implementation believed-complete; awaiting real gate execution by manager).

Implemented (types → config → storage, per D5 build order):
- `src/types/mod.rs`: `Chunk` (Debug/Clone/PartialEq/Eq), `SymbolType` & `Language`
  (Debug/Clone/Copy/PartialEq/Eq/Hash; `Language` also Serialize/Deserialize `rename_all =
  "lowercase"` for config), `FileMeta`. Added `as_str()`/`from_str_lenient()` on both enums
  (total, reversible, `None` on unknown — no panic) for text persistence in §4.1.
- `src/config/mod.rs`: `Config` + `StorageConfig`/`RetrievalConfig`/`McpConfig` mirroring §7.3;
  per-field `#[serde(default = ...)]` + section `Default` impls give the documented defaults
  (4000/20/1.2/0.75/[py,ts,go]/`.codecache/index.db`/500/stdio/3000). `Config::load` returns
  typed `ConfigError::{Io,Parse}` (impl `std::error::Error`) — missing/unreadable → `Io`,
  malformed TOML → `Parse`. No `unwrap`/`expect`/`panic`.
- `src/storage/{mod,schema,queries}.rs`: `Storage { conn: Arc<Mutex<Connection>> }` (D8),
  `#[derive(Clone)]`. Typed `StorageError::{Sqlite,LockPoisoned,CorruptRow}` (impl Error). Lock
  helper maps a poisoned mutex to a typed error (no panic). `init_schema` = idempotent
  `execute_batch` (`IF NOT EXISTS` + `INSERT OR IGNORE`) then `migrate()` (stamps version
  forward when stored ≠ current). `insert_chunks` batches in one transaction with a cached
  prepared stmt. `search` maps rows → `SearchResult`, deferring enum validation so a corrupt row
  becomes `CorruptRow` not a panic. `get_file_hash`/`get_file_meta`/`update_file_hash` (D6 upsert
  incl. file_size/chunk_count/language/indexed_at), `get/set_index_state`.

Plan deviations raised (manager to ratify in Decision Log):
- **§4.1 `content='symbols'` dropped.** In FTS5, `content=` names a *separate* external-content
  table; aiming it at the FTS5 table's own name is invalid. Used a default (contentful) FTS5
  table so all columns round-trip without a companion table. Index stays ~6MB at Django scale
  (§4.2) — well under the <100MB budget. **Proposed new decision D11.** (project_plan §4.1 to be
  annotated.)
- **List columns `imports`/`cross_references` stored as `\n`-joined text** in the single FTS5
  cell (FTS5 has no array type). `split_joined`/`join("\n")` round-trip; empty ⇒ empty vec.
- **Additive `Storage` methods** beyond the §3.2.2 minimal list: `get_file_meta`,
  `get_index_state`, `set_index_state`. Additive, not divergent; needed by tests now and by
  M5/M7/`status` later.

## Specialist / Perf notes
FTS5 (rust-treesitter-specialist):
- Tokenizer `unicode61 remove_diacritics 2` (matches §4.1): folds diacritics so `café_handler`
  is reachable; `_`-joined identifiers stay single tokens.
- Indexed (D3): symbol_name, symbol_type, chunk_text, parent_symbol, imports, cross_references.
  UNINDEXED (retrieval-only): file_path, start_byte, end_byte, start_line, end_line (D7),
  language — proven by `unindexed_columns_not_searchable`.
- Column weighting: `bm25(symbols, 10.0,1.0,1.0,5.0,2.0,2.0)` weights symbol_name highest
  (satisfies `column_weighting_respected`); `ORDER BY score ASC, rowid ASC` = best-first with a
  deterministic tie-break (satisfies the determinism test).
Perf: no M1 budget gate. `EXPLAIN QUERY PLAN` baseline deferred — to be captured during gate
execution and logged here for M6's p95<500ms / FTS5<50ms budget. Statements are `prepare_cached`.

## REVIEW — code reviewer
Verdict: **APPROVE (conditional on real gate execution)** — no blocking defects found on
inspection; one refactor applied during review to de-risk a clippy `-D warnings` failure.

Audit notes (severity — location — finding):
- [fixed] minor — `storage/mod.rs` `map_search_row` — original used an immediately-invoked
  closure `(|| {…})()` for `?` early-return, which `clippy::redundant_closure_call` could flag
  under `-D warnings`. Refactored to a `RawSearchRow` struct + named `build_search_result`
  helper. No behavior change.
- [ok] `bm25(symbols, …)` passes 6 weights for a 12-column table. FTS5 defaults unspecified
  trailing weights to 1.0 and UNINDEXED columns never contribute, so this is correct; the 6
  weights cover exactly the indexed columns in declared order.
- [ok] `insert_chunks` borrows `&Chunk` in the loop; `params!` auto-references (`&Option<String>`,
  `&i64` temporaries live to statement end), so no move-out-of-borrow. Transaction scopes the
  cached stmt in an inner block before `commit()`.
- [ok] No reachable `unwrap`/`expect`/`panic!` in production: poisoned `Mutex` → `LockPoisoned`;
  unknown stored enum → `CorruptRow`; all SQLite errors via `?`/`From`. `path_to_str` uses
  `to_string_lossy` (no panic on non-UTF-8).
- [ok] D5 (types in `crate::types`), D6 (`update_file_hash(&FileMeta)` + persisted bundle),
  D7 (`start_line`/`end_line` UNINDEXED + on `Chunk`), D8 (`Arc<Mutex<Connection>>`, `Clone`),
  D9 (FTS5 virtual table created) all honored. D11 raised + recorded (ROADMAP + §4.1 annotated).
- Caveat per manager's standing rule: APPROVE is **subject to** the manager running the real
  `build`/`test`/`clippy -D warnings`/`fmt --check` with Rust 1.85.0 on PATH. Inspection cannot
  fully substitute for compilation (M0 lesson).

## OUTCOME — manager
Aligned with project_plan §3.2.2/§4.1/§4.3 and the ratified decisions. One architecture
clarification surfaced and handled spec-first: **D11** (drop invalid `content='symbols'`; use a
contentful FTS5 table) — recorded in ROADMAP Decision Log and annotated in project_plan §4.1
before keeping the implementation. M1 plan reconciled to the D6/D7 `FileMeta`/line-column
contract up front.

Status: **implementation believed-complete; NOT marked DONE.** Per the standing rule (M0 taught
that inspection misses real compile errors), the slice is held at REVIEW-conditional until the
human/manager session runs the real gates with Rust 1.85.0 on PATH:
`cargo build` · `cargo test` · `cargo clippy --all-targets -- -D warnings` · `cargo fmt --check`.

TODO: Phase 1 items moved to `[~]` (believed-complete, awaiting gates), not `[x]`. Module
CLAUDE.md (`types`/`config`/`storage`) updated to shipped API. Final `[x]` flip + DONE checkbox
happen after gates pass.

Follow-ups on gate execution:
- Capture `EXPLAIN QUERY PLAN` for the SEARCH SQL; log it here as the M6 latency baseline.
- If any gate fails, reopen GREEN (do not weaken tests); record the failure here.

---
## REOPEN #1 — gate failure: missing `file_docstring` column (2026-06-10)

**Gate result that reopened the cycle:** `cargo build` FAILED on Rust 1.85.0 —
`error[E0063]: missing field file_docstring in initializer of Chunk` at `src/storage/mod.rs`
`build_search_result`. Root cause: a real correctness gap, not a typo. `crate::types::Chunk` has
`file_docstring: Option<String>` (D3), but the FTS5 `symbols` schema, the insert SQL, the search
projection, and `RawSearchRow` all omitted the column — so `build_search_result` could not
populate the field, and `insert_chunks` would have silently dropped docstrings on round-trip even
if it compiled. The reviewer's prior `[ok]` note on the bm25 weight count (6 weights / 6 indexed
columns) became stale with this fix.

**Plan/Decision-Log change (spec-first):** §4.1 DDL listed only 3 of D3's enrichment fields as
indexed and omitted `file_docstring`, contradicting the §4.3 `Chunk` struct and D3 ("indexed in
FTS5 to lift recall"). The DDL was the documentation bug. Fixed both §4.1 DDL blocks to add
`file_docstring` as the **last indexed column** (immediately before the UNINDEXED block, preserving
"indexed first, then UNINDEXED"). Recorded as a follow-up under ROADMAP Decision Log D11. No
schema-version bump — M1 is not yet released, so the pre-release schema is corrected in place.

**RED (test-lead role) — 3 new tests in `tests/storage_tests.rs`, existing 15 untouched:**
- `insert_then_search_round_trips_some_file_docstring` — `Some(..)` survives insert→search
  (asserts the reconstructed field, not just a match).
- `insert_then_search_round_trips_none_file_docstring` — absent docstring round-trips as `None`,
  not `Some("")`.
- `term_only_in_file_docstring_is_matchable` — a unique term living ONLY in the docstring is
  searchable, proving the column is **indexed**, not merely stored (the regression guard for this
  exact gate failure). Added helper `chunk_with_docstring(...)`.

**GREEN (eng-lead + FTS5-specialist roles):**
- `schema.rs` — added `file_docstring` as last indexed column in `CREATE_SYMBOLS`; updated the
  indexed-vs-UNINDEXED doc comment.
- `queries.rs` — `INSERT_CHUNK` now 13 columns / `?1..?13` (`file_docstring` after
  `cross_references`); `SEARCH` projection adds `file_docstring`; **bm25 weight arity 6→7**, new
  `file_docstring` weight `2.0` (enrichment tier). Chose explicit-per-indexed-column weights (not
  relying on FTS5's trailing-default-1.0) so the weight list stays self-documenting and matches the
  declared indexed-column order exactly.
- `mod.rs` — `insert_chunks` params add `c.file_docstring` (Option<String>, `params!`
  auto-references like `c.parent_symbol`); `RawSearchRow` gains `file_docstring`; `map_search_row`
  column indices shifted (docstring at 6, file_path..score now 7..13); `build_search_result`
  populates `Chunk.file_docstring`.

**Docs updated in the same change:** `src/storage/CLAUDE.md` (indexed-columns list now 7; bm25
weight note). `src/types/CLAUDE.md` unchanged (it already documented `file_docstring` on `Chunk`;
the gap was storage persistence, not the type).

**Status:** RED-before-GREEN preserved. Held at "ready for gate execution" — TODO Phase 1 stays
`[~]`, NOT flipped to `[x]`, until the human session confirms all four gates pass with Rust
1.85.0. Watch for further errors beyond the first (the build stopped at E0063).
