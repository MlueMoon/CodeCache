//! Hasher: xxHash3-128 content hashing + change detection for incremental indexing.
//!
//! API anchor: `project_plan.md` §4.4. Owner: `principal-engineering-lead`. Scenarios:
//! `docs/TEST_STRATEGY.md#hasher`.
//!
//! A file's hash mixes its **content** and its **mtime** (modification time): `mtime` alone is
//! insufficient (a restore can reset it without changing bytes) and content alone misses a
//! touch-only change the downstream indexer may care about, so §4.4 hashes both. The output is a
//! 32-char lowercase hex string — the exact format stored in `files_metadata.content_hash` (M1
//! §4.1), so a hash round-trips through storage unchanged.
//!
//! Hashing is deliberately content-opaque (Decision Log **D2**): binary/non-UTF-8/unreadable
//! bytes are hashed without interpretation and never panic. Language and parse concerns are
//! downstream (M3/M4). No reachable `unwrap()/expect()/panic!` — every fallible step returns
//! [`HasherError`] via `?`.

use std::path::Path;
use std::time::UNIX_EPOCH;

use xxhash_rust::xxh3::Xxh3;

/// A typed hasher error. Wraps the I/O / clock failures that can occur while reading a file and
/// its modification time. Never panics.
#[derive(Debug)]
pub enum HasherError {
    /// The file could not be read or its metadata could not be obtained (missing, unreadable,
    /// permission denied, …). Carries the originating path for diagnostics.
    Io {
        /// The path that failed.
        path: std::path::PathBuf,
        /// The underlying I/O error.
        source: std::io::Error,
    },
    /// The file's modification time predates the Unix epoch (clock skew / exotic filesystem), so
    /// it cannot be expressed as epoch seconds.
    MtimeBeforeEpoch(std::path::PathBuf),
}

impl std::fmt::Display for HasherError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            HasherError::Io { path, source } => {
                write!(f, "failed to hash {}: {source}", path.display())
            }
            HasherError::MtimeBeforeEpoch(path) => {
                write!(
                    f,
                    "modification time of {} is before the Unix epoch",
                    path.display()
                )
            }
        }
    }
}

impl std::error::Error for HasherError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            HasherError::Io { source, .. } => Some(source),
            HasherError::MtimeBeforeEpoch(_) => None,
        }
    }
}

/// Convenience alias for hasher results.
pub type Result<T> = std::result::Result<T, HasherError>;

/// Compute a stable xxHash3-128 over `bytes`, formatted as 32 lowercase hex chars.
///
/// Pure (no filesystem): deterministic for identical input, sensitive to any single-byte change,
/// and content-opaque (embedded NULs / non-UTF-8 are fine — Decision Log **D2**). This is the
/// content half of [`compute_file_hash`] and is independently unit-testable.
pub fn compute_content_hash(bytes: &[u8]) -> String {
    let mut hasher = Xxh3::new();
    hasher.update(bytes);
    format!("{:032x}", hasher.digest128())
}

/// Compute the change-detection hash of the file at `path`: xxHash3-128 of its **content**
/// followed by its **mtime** (epoch seconds, little-endian), per `project_plan.md` §4.4.
///
/// Returns the same 32-char hex format as [`compute_content_hash`] / `files_metadata.content_hash`.
///
/// # Errors
/// Returns [`HasherError::Io`] if the file cannot be read or its metadata/mtime obtained (missing,
/// unreadable, …), and [`HasherError::MtimeBeforeEpoch`] if the mtime predates the Unix epoch.
/// Never panics, including on binary content.
pub fn compute_file_hash(path: &Path) -> Result<String> {
    let content = std::fs::read(path).map_err(|source| HasherError::Io {
        path: path.to_path_buf(),
        source,
    })?;
    let metadata = std::fs::metadata(path).map_err(|source| HasherError::Io {
        path: path.to_path_buf(),
        source,
    })?;
    let modified = metadata.modified().map_err(|source| HasherError::Io {
        path: path.to_path_buf(),
        source,
    })?;
    let mtime = modified
        .duration_since(UNIX_EPOCH)
        .map_err(|_| HasherError::MtimeBeforeEpoch(path.to_path_buf()))?
        .as_secs();

    let mut hasher = Xxh3::new();
    hasher.update(&content);
    hasher.update(&mtime.to_le_bytes());
    Ok(format!("{:032x}", hasher.digest128()))
}

/// Has the file at `path` changed relative to a previously cached hash?
///
/// Returns `true` when there is no cached hash (`None` — the file was never indexed) or when the
/// freshly computed [`compute_file_hash`] differs from `cached`. This is the predicate M5's
/// incremental indexer uses to skip unchanged files.
///
/// # Errors
/// Propagates any [`HasherError`] from [`compute_file_hash`] (e.g. the file became unreadable).
pub fn is_changed(path: &Path, cached: Option<&str>) -> Result<bool> {
    match cached {
        None => Ok(true),
        Some(cached) => {
            let current = compute_file_hash(path)?;
            Ok(current != cached)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn content_hash_is_deterministic_and_32_hex() {
        let a = compute_content_hash(b"unit-level check");
        let b = compute_content_hash(b"unit-level check");
        assert_eq!(a, b);
        assert_eq!(a.len(), 32);
        assert!(a
            .chars()
            .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase()));
    }

    #[test]
    fn is_changed_with_no_cache_is_true_without_touching_fs() {
        // `None` short-circuits before any filesystem access, so a bogus path is fine here.
        let changed = is_changed(Path::new("definitely/missing/path.py"), None)
            .expect("None cache must not error");
        assert!(changed);
    }
}
