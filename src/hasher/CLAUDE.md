# src/hasher/ — CLAUDE.md

**Module:** `hasher` · **Owner:** `principal-engineering-lead` · **Milestone:** M2 (stub at M0).

## Purpose
Compute xxHash3-128 of file content (+ mtime) and compare against the cached hash for change
detection driving incremental indexing (M5).

## API anchor
`docs/project_plan.md` §4.4 (`compute_file_hash` → 32-hex string).

## Tests / scenarios
`docs/TEST_STRATEGY.md#hasher` — deterministic for identical content; differs on 1-byte change;
unchanged ⇒ "same", modified ⇒ "changed"; binary & large files. Covered by
`tests/hasher_tests.rs` (11 integration tests) + 2 in-module unit tests.

## Shipped API (M2)
- `compute_content_hash(bytes: &[u8]) -> String` — pure, no fs; `Xxh3` over `bytes`, formatted
  `{:032x}` (32 lowercase hex). Deterministic; 1-byte-sensitive; content-opaque (D2).
- `compute_file_hash(path: &Path) -> Result<String>` — hashes file **content then
  `mtime.to_le_bytes()`** (§4.4 order; mtime alone is insufficient). Same 32-hex format as
  `files_metadata.content_hash` (M1 §4.1), so a hash round-trips through storage unchanged.
- `is_changed(path: &Path, cached: Option<&str>) -> Result<bool>` — `None` ⇒ `true`
  (never-indexed); `Some(h)` ⇒ recompute and compare. M5's incremental-skip predicate.
- `HasherError { Io { path, source }, MtimeBeforeEpoch(path) }` — typed, `impl std::error::Error`
  (with `source()`). No reachable `unwrap()/expect()/panic!`; missing/unreadable ⇒ typed `Err`.

## Decision Log bindings
- **D2:** hashes bytes opaquely (binary/NUL/non-UTF-8 never panic); language/parse concerns are
  downstream (M3/M4).

## Performance
- Budget: hash 1K files (~500 LOC each) **< 500ms** (§5.4); xxHash3-128 ~10GB/s (§11.4). Validated
  rigorously by the **M10** criterion bench, not by a flaky timing assert in unit tests. v0.1 uses
  a single full-read per file (acceptable for v0.1 sizes); avoid double-hashing/re-reading in M5.

## Status
**M2: DONE (2026-06-10).** Leaf module, no in-tree deps. First consumer is the M5 indexer.
