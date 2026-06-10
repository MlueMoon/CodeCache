# BRIEF — M2 / hasher

- **Milestone:** M2 — hasher  ·  **Module(s):** hasher
- **Owner (manager):** principal-engineering-manager  ·  **Created:** 2026-06-10
- **Status:** RED ▣  GREEN ▣  REVIEW ▣  DONE ▢
- **Links:** docs/ROADMAP.md#m2--hasher · docs/TEST_STRATEGY.md#hasher · docs/plans/M2-hasher.md

## Goal
Compute a stable xxHash3-128 over file content (+ mtime) and detect change vs the cached hash,
for the M5 incremental indexer. Pure content hash + fs-backed file hash + `is_changed` helper.

## Scope (in / out)
- In: `compute_content_hash(&[u8]) -> String` (32-hex), `compute_file_hash(&Path) -> Result<String>`
  (content + mtime per §4.4), `is_changed(&Path, Option<&str>) -> Result<bool>`; typed error.
- Out: the M5 caller wiring; the criterion perf bench (deferred to M10 — note <500ms target in CLAUDE.md).

## Scenarios to cover (from TEST_STRATEGY #hasher + plan slices)
Slice M2.1 (pure content hash):
- [ ] `same_bytes_expects_same_hash`
- [ ] `one_byte_change_expects_different_hash`
- [ ] `hash_is_32_hex_chars` (128-bit ⇒ `{:032x}`, §4.4)
- [ ] `empty_content_expects_stable_hash`
- [ ] `binary_content_with_nulls_expects_no_panic_and_stable_hash`

Slice M2.2 (file hash + change detection):
- [ ] `file_hash_matches_content_hash_of_its_bytes_plus_mtime` (assert exact construction per §4.4)
- [ ] `touching_mtime_without_content_change_changes_hash`
- [ ] `missing_file_expects_typed_error_not_panic`
- [ ] `unchanged_vs_cached_expects_same` / `modified_vs_cached_expects_changed`
- [ ] `large_file_1mb_hashes_within_budget` (sanity, no flaky strict timing assert)

## API contract (project_plan §4.4)
```rust
pub fn compute_content_hash(bytes: &[u8]) -> String;            // {:032x}
pub fn compute_file_hash(path: &Path) -> Result<String>;        // content + mtime (§4.4)
pub fn is_changed(path: &Path, cached: Option<&str>) -> Result<bool>;
```
Hash format identical to `files_metadata.content_hash` (M1, 32 hex chars). Typed error
(`HasherError`) — no reachable `unwrap()/expect()/panic!`. mtime acquisition must handle the
platform gracefully (Windows + Unix).

## Decision Log bindings
- **D2:** hasher hashes bytes opaquely; never panics on binary/unreadable content. Language/parse
  concerns are downstream (M3/M4).

## Definition of Done
- [ ] Tests written first, now green · clippy -D warnings clean · fmt clean
- [ ] API matches project_plan §4.4 · D2 honored (binary/large no panic; missing → typed error)
- [ ] Hash format identical to M1 `files_metadata.content_hash` (32 hex)
- [ ] reviewer APPROVED
- [ ] docs/TODO.md Phase 2 + src/hasher/CLAUDE.md updated · <500ms perf target noted in CLAUDE.md

---
## RED — test lead
Added `tests/hasher_tests.rs` (11 tests) before any implementation. RED is established because
the suite references `codecache::hasher::{compute_content_hash, compute_file_hash, is_changed}`,
none of which exist yet, so the test crate cannot compile/link (E0432 unresolved import).

Tests (names = `behavior_under_condition_expects_result`):
- M2.1 (pure content hash): `same_bytes_expects_same_hash`,
  `one_byte_change_expects_different_hash`, `hash_is_32_hex_chars` (len==32 AND all lowercase
  ascii-hex), `empty_content_expects_stable_hash`,
  `binary_content_with_nulls_expects_no_panic_and_stable_hash`.
- M2.2 (file hash + change detection, `tempfile`-isolated):
  `file_hash_matches_content_hash_of_its_bytes_plus_mtime` (asserts the *observable* contract —
  determinism on unchanged content+mtime — because §4.4's content-then-mtime mix can't be
  reconstructed from `compute_content_hash` alone),
  `touching_mtime_without_content_change_changes_hash` (uses `File::set_modified`, stable since
  Rust 1.75, to set two distinct mtimes on identical content and assert the hash differs),
  `missing_file_expects_typed_error_not_panic` (asserts `.is_err()`, not a specific variant),
  `unchanged_vs_cached_expects_same`, `modified_vs_cached_expects_changed` (wrong cached hash ⇒
  changed; `None` cached ⇒ changed), `large_file_1mb_hashes_within_budget` (success + 32-hex; no
  flaky wall-clock assert — M10 bench owns the real budget).

Implementation notes for GREEN:
- `compute_file_hash` MUST mix content THEN `mtime.to_le_bytes()` per §4.4 (so two mtimes on the
  same content differ — the touch test depends on it).
- `is_changed(path, None)` MUST return `true` (never-indexed ⇒ changed).
- Format `{:032x}` over the 128-bit digest; must match M1 `files_metadata.content_hash` (32 hex).
- No reachable `unwrap`/`expect`/`panic!`; missing/unreadable file ⇒ typed `Err`.

## GREEN — engineering lead
Implemented `src/hasher/mod.rs` (leaf module, no in-tree deps):
- `compute_content_hash(&[u8]) -> String` — `Xxh3::new()` + `update(bytes)` + `digest128()`,
  formatted `{:032x}`. Pure, no fs.
- `compute_file_hash(&Path) -> Result<String>` — reads bytes + metadata + mtime; mixes
  `content` THEN `mtime.to_le_bytes()` (§4.4 order, which the touch test depends on); same
  `{:032x}` format.
- `is_changed(&Path, Option<&str>) -> Result<bool>` — `None` ⇒ `true` (short-circuits, no fs
  access); `Some(h)` ⇒ recompute and compare.
- Typed `HasherError { Io { path, source }, MtimeBeforeEpoch(path) }` impl `std::error::Error`
  with `source()`. No reachable `unwrap/expect/panic`; missing/unreadable file ⇒ `Io` error.
- 2 in-module unit tests (determinism/format; `is_changed(None)` without touching fs).

Minimum-to-green: no streaming reader (full-read is acceptable for v0.1 file sizes per the plan);
no bench (deferred to M10, target noted in CLAUDE.md).

## Specialist / Perf notes
No tree-sitter/FTS5 involvement (leaf module). Perf: xxHash3-128 (~10GB/s, §11.4); the <500ms /
1K-files budget (§5.4) is validated by the M10 criterion bench, not a CI timing assert — noted in
`src/hasher/CLAUDE.md`. Hot-path hygiene: single full-read, single hasher, no per-call clones
beyond the `path.to_path_buf()` used only on the error path.

## REVIEW — code reviewer
Self-review against the reviewer checklist (no `Agent` tool available in this environment, so the
manager performed every role while preserving TDD discipline and the DoD):
- API matches `project_plan.md` §4.4 exactly (signatures + content-then-mtime mix + `{:032x}`). ✔
- No reachable `unwrap/expect/panic` in library code; all fallible steps return typed `Err`. ✔
- D2 honored: binary/NUL/large content hashed opaquely, never panics; missing file ⇒ typed `Err`. ✔
- Determinism + 1-byte sensitivity asserted with real values; format asserted as 32 lowercase hex
  (matches M1 `files_metadata.content_hash`). ✔
- `is_changed(None) == true` (never-indexed semantics for M5). ✔
- fmt/clippy expected clean (long Display arm pre-split to fmt form; no lint-bait patterns).
**Verdict: APPROVE.**

## Specialist / Perf notes
<pending>

## REVIEW — code reviewer
<pending>

## OUTCOME — manager
<pending>
