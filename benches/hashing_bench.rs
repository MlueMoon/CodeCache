//! Hashing throughput benchmark for `hasher::{compute_file_hash, compute_content_hash}`.
//!
//! Budget (§5.4): hash 1K files (each ~500 LOC) in **< 500ms** total.
//!
//! ## What is measured
//! - `hash_1k_files/compute_file_hash` — hashes 1000 synthetic on-disk files back-to-back,
//!   simulating the M5 incremental-skip predicate calling `compute_file_hash` once per file.
//! - `hash_1k_files/compute_content_hash` — hashes 1000 synthetic byte slices in-memory
//!   (no filesystem); this isolates the xxHash3 throughput from disk I/O.
//!
//! ## Fixture design
//! 1000 files are written to a temp dir OUTSIDE the timed closure. Each file is ~500 LOC of
//! synthetic Python source (≈ 15KB bytes). File content varies by index so all 1000 are distinct
//! (deterministic, reproducible across runs — no timestamps in content).
//!
//! ## Assertion policy
//! `compute_content_hash` (pure in-memory xxHash3) should be well under budget with several
//! orders of magnitude headroom (~10 GB/s per §11.4 → 1K × 15KB ≈ 15MB → < 2ms). A hard assert
//! is NOT applied in the timed closure (criterion controls iteration; the "1K files in one call"
//! budget is validated by the `hash_1k_sequential` helper below which runs once outside criterion
//! and panics if the wall-clock exceeds 500ms). Criterion samples give stable p50/p95/p99.
//!
//! Run:
//!   cargo bench --bench hashing_bench
//! Save/compare baselines:
//!   cargo bench --bench hashing_bench -- --save-baseline before
//!   cargo bench --bench hashing_bench -- --baseline before

use std::fs;
use std::path::PathBuf;
use std::time::Instant;

use codecache::hasher::{compute_content_hash, compute_file_hash};
use criterion::{black_box, criterion_group, criterion_main, Criterion};
use tempfile::TempDir;

// ─────────────────────────────── scale knobs ─────────────────────────────────

/// Number of files to hash — the §5.4 budget target.
const FILE_COUNT: usize = 1_000;

/// Approximate lines per file. ~500 LOC ≈ 15KB per file for typical Python source.
const LINES_PER_FILE: usize = 500;

// ─────────────────────────────── fixture helpers ─────────────────────────────

/// Render ~[`LINES_PER_FILE`] lines of synthetic Python source for file `i`.
/// Content varies by `i` so each file has a distinct hash. No timestamps — deterministic.
fn synthetic_source(i: usize) -> Vec<u8> {
    let mut buf = format!(
        "\"\"\"Synthetic benchmark file {i}.\"\"\"\n\nclass Benchmark{i}:\n    \"\"\"Benchmark class {i}.\"\"\"\n\n"
    );
    // 5 methods, each with (LINES_PER_FILE / 5 - 3) body lines.
    let body_lines_per_method = (LINES_PER_FILE / 5).saturating_sub(3).max(1);
    for m in 0..5 {
        buf.push_str(&format!(
            "    def method_{m}_{i}(self, a, b):\n        \"\"\"Method {m} of file {i}.\"\"\"\n"
        ));
        for line in 0..body_lines_per_method {
            buf.push_str(&format!("        x_{line} = a + b + {i} + {m} + {line}\n"));
        }
        buf.push_str("        return x_0\n\n");
    }
    buf.into_bytes()
}

/// Write [`FILE_COUNT`] synthetic Python files to a temp dir. Returns the dir handle and
/// the list of absolute paths (in deterministic order). Fixture I/O runs OUTSIDE the bench closure.
fn setup_files() -> (TempDir, Vec<PathBuf>, Vec<Vec<u8>>) {
    let dir = tempfile::tempdir().expect("create temp dir for hashing bench");
    let mut paths = Vec::with_capacity(FILE_COUNT);
    let mut contents = Vec::with_capacity(FILE_COUNT);

    for i in 0..FILE_COUNT {
        let content = synthetic_source(i);
        let path = dir.path().join(format!("bench_{i:04}.py"));
        fs::write(&path, &content).expect("write synthetic bench file");
        paths.push(path);
        contents.push(content);
    }

    (dir, paths, contents)
}

// ─────────────────────────────── one-shot budget check ───────────────────────

/// Hash all [`FILE_COUNT`] files sequentially and assert the total wall-clock time is under 500ms.
/// Runs ONCE before criterion's sampling loop; panics on a budget miss (acceptable in bench code).
///
/// This validates the §5.4 "1K files < 500ms" budget as a hard assertion. The criterion loop below
/// then measures per-file latency for p50/p95/p99 trending.
fn assert_budget_hash_1k_files(paths: &[PathBuf]) {
    let start = Instant::now();
    for path in paths {
        let _ = compute_file_hash(path).expect("compute_file_hash must not error on bench files");
    }
    let elapsed = start.elapsed();
    let elapsed_ms = elapsed.as_millis();
    eprintln!(
        "\n[hashing_bench] hash_1k_sequential: {elapsed_ms} ms for {FILE_COUNT} files — budget < 500 ms"
    );
    const BUDGET_MS: u128 = 500;
    if elapsed_ms > BUDGET_MS {
        panic!(
            "HASH 1K FILES BUDGET MISSED: {elapsed_ms} ms > {BUDGET_MS} ms budget ({FILE_COUNT} files)"
        );
    }
}

// ─────────────────────────────── criterion benches ───────────────────────────

/// Per-file `compute_file_hash` latency: hash ONE on-disk file per criterion sample.
/// The per-sample cost × 1000 gives the total-1K-files cost without the per-iteration setup
/// overhead; p50/p95/p99 show the distribution of individual file hash latency.
fn bench_compute_file_hash(c: &mut Criterion) {
    let (_dir, paths, _contents) = setup_files();

    // Hard budget check before sampling.
    assert_budget_hash_1k_files(&paths);

    let mut group = c.benchmark_group("hash_1k_files");
    group.sample_size(100);

    group.bench_function("compute_file_hash_per_file", |b| {
        let mut idx = 0usize;
        b.iter(|| {
            // Cycle through files so we hash all 1000 across samples.
            let path = &paths[idx % FILE_COUNT];
            idx = idx.wrapping_add(1);
            let result = compute_file_hash(black_box(path)).expect("hash must succeed");
            black_box(result)
        });
    });

    group.finish();
}

/// Per-call `compute_content_hash` latency: hash ONE ~15KB byte slice per criterion sample.
/// This isolates xxHash3 CPU throughput from disk I/O.
fn bench_compute_content_hash(c: &mut Criterion) {
    let (_dir, _paths, contents) = setup_files();

    let mut group = c.benchmark_group("hash_1k_files");
    group.sample_size(100);

    group.bench_function("compute_content_hash_per_file", |b| {
        let mut idx = 0usize;
        b.iter(|| {
            let content = &contents[idx % FILE_COUNT];
            idx = idx.wrapping_add(1);
            let result = compute_content_hash(black_box(content.as_slice()));
            black_box(result)
        });
    });

    group.finish();
}

// ─────────────────────────────── criterion wiring ────────────────────────────

criterion_group!(benches, bench_compute_file_hash, bench_compute_content_hash,);
criterion_main!(benches);
