//! Integration tests for the `hasher` module (M2).
//!
//! TDD RED: written before `src/hasher/mod.rs` is implemented. Scenarios from
//! `docs/plans/M2-hasher.md` + `docs/TEST_STRATEGY.md#hasher`. The public API under test
//! (`project_plan.md` §4.4):
//! ```ignore
//! pub fn compute_content_hash(bytes: &[u8]) -> String;       // 32 hex chars
//! pub fn compute_file_hash(path: &Path) -> Result<String>;   // content + mtime.to_le_bytes()
//! pub fn is_changed(path: &Path, cached: Option<&str>) -> Result<bool>;
//! ```
//! Filesystem state is isolated with `tempfile`; tests are deterministic and parallel-safe.

use std::fs::File;
use std::io::Write;
use std::time::{Duration, SystemTime};

use codecache::hasher::{compute_content_hash, compute_file_hash, is_changed};
use tempfile::tempdir;

fn is_32_lower_hex(s: &str) -> bool {
    s.len() == 32
        && s.chars()
            .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase())
}

// ---- Slice M2.1 — pure content hash (no filesystem) ----

#[test]
fn same_bytes_expects_same_hash() {
    let a = compute_content_hash(b"def authenticate_user():\n    pass\n");
    let b = compute_content_hash(b"def authenticate_user():\n    pass\n");
    assert_eq!(a, b, "identical bytes must hash identically");
}

#[test]
fn one_byte_change_expects_different_hash() {
    let a = compute_content_hash(b"hello world");
    let b = compute_content_hash(b"hello worle"); // last byte differs
    assert_ne!(a, b, "a single-byte change must change the hash");
}

#[test]
fn hash_is_32_hex_chars() {
    let h = compute_content_hash(b"some representative content");
    assert_eq!(h.len(), 32, "128-bit hash formatted {{:032x}} is 32 chars");
    assert!(
        is_32_lower_hex(&h),
        "hash must be exactly 32 lowercase hex digits, got {h:?}"
    );
}

#[test]
fn empty_content_expects_stable_hash() {
    let a = compute_content_hash(b"");
    let b = compute_content_hash(b"");
    assert_eq!(a, b, "empty input must hash deterministically");
    assert!(
        is_32_lower_hex(&a),
        "empty-input hash is still 32 hex chars"
    );
}

#[test]
fn binary_content_with_nulls_expects_no_panic_and_stable_hash() {
    // D2: hasher treats content opaquely — embedded NULs / non-UTF-8 bytes must not panic.
    let bytes: &[u8] = &[0x00, 0xFF, 0x00, 0x10, 0xDE, 0xAD, 0xBE, 0xEF, 0x00];
    let a = compute_content_hash(bytes);
    let b = compute_content_hash(bytes);
    assert_eq!(a, b, "binary content must hash deterministically");
    assert!(is_32_lower_hex(&a));
}

// ---- Slice M2.2 — file hash (content + mtime) and change detection ----

#[test]
fn file_hash_matches_content_hash_of_its_bytes_plus_mtime() {
    // §4.4 mixes content THEN mtime.to_le_bytes(), so we cannot reconstruct the value with
    // `compute_content_hash` (which hashes bytes only). Instead we assert the observable
    // contract: for a file whose content AND mtime are unchanged, `compute_file_hash` is
    // deterministic across calls and returns a 32-hex string in the M1 storage format.
    let dir = tempdir().expect("tempdir");
    let path = dir.path().join("sample.py");
    {
        let mut f = File::create(&path).expect("create");
        f.write_all(b"x = 1\n").expect("write");
    }
    let h1 = compute_file_hash(&path).expect("hash 1");
    let h2 = compute_file_hash(&path).expect("hash 2");
    assert_eq!(h1, h2, "same content + same mtime ⇒ identical file hash");
    assert!(
        is_32_lower_hex(&h1),
        "file hash uses the 32-hex storage format"
    );
}

#[test]
fn touching_mtime_without_content_change_changes_hash() {
    // §4.4 hashes mtime too: identical content at a different mtime must produce a different
    // hash. `File::set_modified` (stable since Rust 1.75) makes this deterministic.
    let dir = tempdir().expect("tempdir");
    let path = dir.path().join("touched.py");
    {
        let mut f = File::create(&path).expect("create");
        f.write_all(b"same content\n").expect("write");
    }
    let t1 = SystemTime::UNIX_EPOCH + Duration::from_secs(1_700_000_000);
    let t2 = SystemTime::UNIX_EPOCH + Duration::from_secs(1_700_000_500);

    File::options()
        .write(true)
        .open(&path)
        .expect("open")
        .set_modified(t1)
        .expect("set mtime t1");
    let h1 = compute_file_hash(&path).expect("hash t1");

    File::options()
        .write(true)
        .open(&path)
        .expect("open")
        .set_modified(t2)
        .expect("set mtime t2");
    let h2 = compute_file_hash(&path).expect("hash t2");

    assert_ne!(
        h1, h2,
        "same content but different mtime must change the file hash (§4.4)"
    );
}

#[test]
fn missing_file_expects_typed_error_not_panic() {
    let dir = tempdir().expect("tempdir");
    let path = dir.path().join("does_not_exist.py");
    let result = compute_file_hash(&path);
    assert!(
        result.is_err(),
        "hashing a missing file must return a typed Err, not panic"
    );
}

#[test]
fn unchanged_vs_cached_expects_same() {
    let dir = tempdir().expect("tempdir");
    let path = dir.path().join("unchanged.py");
    {
        let mut f = File::create(&path).expect("create");
        f.write_all(b"def f():\n    return 42\n").expect("write");
    }
    let cached = compute_file_hash(&path).expect("hash");
    let changed = is_changed(&path, Some(&cached)).expect("is_changed");
    assert!(!changed, "file matching its cached hash is unchanged");
}

#[test]
fn modified_vs_cached_expects_changed() {
    let dir = tempdir().expect("tempdir");
    let path = dir.path().join("modified.py");
    {
        let mut f = File::create(&path).expect("create");
        f.write_all(b"def f():\n    return 42\n").expect("write");
    }
    // A stale/wrong cached hash ⇒ changed.
    let wrong = "0".repeat(32);
    assert!(
        is_changed(&path, Some(&wrong)).expect("is_changed wrong"),
        "a file not matching its cached hash is changed"
    );
    // No cached hash at all ⇒ treat as changed (never indexed before).
    assert!(
        is_changed(&path, None).expect("is_changed none"),
        "a file with no cached hash is treated as changed"
    );
}

#[test]
fn large_file_1mb_hashes_within_budget() {
    // Sanity: a ~1MB file hashes without error and yields a 32-hex string. The real perf budget
    // (<500ms for 1K files, §5.4) is validated by the M10 criterion bench, not this test.
    let dir = tempdir().expect("tempdir");
    let path = dir.path().join("large.bin");
    {
        let mut f = File::create(&path).expect("create");
        let chunk = vec![0xABu8; 64 * 1024];
        for _ in 0..16 {
            f.write_all(&chunk).expect("write 64k");
        }
    }
    let h = compute_file_hash(&path).expect("hash large file");
    assert!(is_32_lower_hex(&h), "large file hash is still 32 hex chars");
}
