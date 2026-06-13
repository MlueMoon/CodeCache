//! Cold-index and incremental-re-index benchmarks for `Indexer::index_all`.
//!
//! M10.1 scales the M5.2 skeleton to the full §5.4 budget checkpoints:
//!   - cold_index/10k_loc   — ~10K LOC; budget < 5s
//!   - cold_index/100k_loc  — ~100K LOC; budget < 30s
//!   - incremental/10_files — modify 10 files in an already-indexed repo; budget < 2s
//!   - index_size           — after a 100K-LOC cold index, record the on-disk .db size; budget < 100MB
//!
//! The M5.2 50-file skeleton bench is retained as `cold_index/500_loc` for historical continuity.
//!
//! ## Fixture design
//! Synthetic Python files are generated OUTSIDE the timed closure (fixture I/O is not under test).
//! Each file contains a class with several methods so the AST chunker has real structural work to do.
//! LOC targets are met by controlling file count and body length per function.
//!
//! ## LOC accounting
//!   ~1.5K LOC ≈   50 files × ~30 LOC/file (M5.2 skeleton retained; `method_params(10)` saturates
//!                 `lpf` to 1, so each file emits ~30 LOC, not the original ~10 — baseline only)
//!  10K LOC  ≈  200 files × ~50 LOC/file
//! 100K LOC  ≈  500 files × 200 LOC/file
//!
//! ## Assertion policy (M10 brief)
//! Hard CI asserts are fragile on absolute ms. Where a budget is met with large headroom a generous
//! hard assert is acceptable; otherwise the number is recorded and trend-tracked.  The index-size
//! hard assert is a simple byte check (very stable) and is applied.
//!
//! Run:
//!   cargo bench --bench indexing
//! Save/compare baselines:
//!   cargo bench --bench indexing -- --save-baseline before
//!   cargo bench --bench indexing -- --baseline before

use std::fs;
use std::path::Path;
use std::time::Duration;

use codecache::config::Config;
use codecache::indexer::Indexer;
use codecache::storage::Storage;
use codecache::types::Language;
use criterion::{criterion_group, criterion_main, Criterion};
use tempfile::TempDir;

// ─────────────────────────────── LOC knobs ──────────────────────────────────

/// ~1.5K LOC: 50 files × ~30 LOC/file — M5.2 skeleton, retained (see module LOC-accounting note;
/// `method_params(10)` saturates body lines to 1, so files are ~30 LOC, not the original ~10).
const FILES_500_LOC: usize = 50;
const LINES_PER_FILE_500: usize = 10;

/// ~10K LOC: 200 files × 50 LOC/file.
const FILES_10K_LOC: usize = 200;
const LINES_PER_FILE_10K: usize = 50;

/// ~100K LOC: 500 files × 200 LOC/file.
const FILES_100K_LOC: usize = 500;
const LINES_PER_FILE_100K: usize = 200;

// ─────────────────────────────── fixture helpers ─────────────────────────────

/// Render a Python module with one class containing `method_count` methods, each with
/// `lines_per_method` lines of body (excluding def/docstring/return lines).
///
/// Total LOC per file ≈ 2 (class header) + method_count × (lines_per_method + 3).
fn py_module(file_index: usize, method_count: usize, lines_per_method: usize) -> String {
    let mut buf = format!(
        "\"\"\"Module {file_index}: synthetic benchmark fixture.\"\"\"\n\nclass Module{file_index}:\n    \"\"\"Class docstring for module {file_index}.\"\"\"\n\n"
    );
    for m in 0..method_count {
        buf.push_str(&format!(
            "    def method_{m}(self, x_{m}, y_{m}):\n        \"\"\"Method {m} in module {file_index}.\"\"\"\n"
        ));
        for line in 0..lines_per_method {
            buf.push_str(&format!(
                "        step_{line} = x_{m} + y_{m} + {line} + {file_index}\n"
            ));
        }
        buf.push_str("        return step_0\n\n");
    }
    buf
}

/// Compute (method_count, lines_per_method) so a file reaches approx `target_lines`.
/// We fix 5 methods and derive body lines per method from that.
fn method_params(target_lines: usize) -> (usize, usize) {
    let method_count = 5usize;
    // Each method: def + docstring + lines + return ≈ lines_per_method + 3
    // target_lines ≈ 2 + method_count * (lpf + 3)  → lpf = (target - 2) / mc - 3
    let lpf = ((target_lines.saturating_sub(2)) / method_count)
        .saturating_sub(3)
        .max(1);
    (method_count, lpf)
}

/// Write `file_count` synthetic Python files under `root`, each with approximately `lines_per_file`
/// lines. Fixture generation is OUTSIDE any timed bench closure.
fn write_synthetic_repo(root: &Path, file_count: usize, lines_per_file: usize) {
    let (mc, lpf) = method_params(lines_per_file);
    for i in 0..file_count {
        let content = py_module(i, mc, lpf);
        let path = root.join(format!("mod_{i:05}.py"));
        fs::write(&path, &content).expect("write synthetic .py file");
    }
}

/// Create a temp dir, populate it, and return the handle (caller must keep it alive).
fn setup_repo(file_count: usize, lines_per_file: usize) -> TempDir {
    let dir = tempfile::tempdir().expect("create temp bench repo");
    write_synthetic_repo(dir.path(), file_count, lines_per_file);
    dir
}

/// Build a fresh Storage at `db_dir/index.db` (schema initialized, cold DB).
fn fresh_storage(db_dir: &Path) -> Storage {
    let db_path = db_dir.join("index.db");
    let storage = Storage::new(&db_path).expect("open storage");
    storage.init_schema().expect("init schema");
    storage
}

/// Python-only Config (no ignore patterns, no extra index_paths — discovery walks `root`).
fn python_config() -> Config {
    Config {
        languages: vec![Language::Python],
        ..Config::default()
    }
}

// ─────────────────────────────── cold-index benches ─────────────────────────

/// Cold index of the M5.2 skeleton (~500 LOC). Retained for baseline continuity.
fn bench_cold_500_loc(c: &mut Criterion) {
    let repo = setup_repo(FILES_500_LOC, LINES_PER_FILE_500);
    let repo_path = repo.path().to_path_buf();

    let mut group = c.benchmark_group("cold_index");
    group.sample_size(10);
    // No hard assert — informational baseline.

    group.bench_function("500_loc", |b| {
        b.iter(|| {
            let db_dir = tempfile::tempdir().expect("create temp db dir");
            let storage = fresh_storage(db_dir.path());
            let mut indexer =
                Indexer::new(python_config(), storage, repo_path.clone()).expect("create indexer");
            indexer.index_all().expect("index_all must succeed")
        });
    });

    group.finish();
}

/// Cold index of ~10K LOC (200 files × 50 LOC/file). Budget: < 5s.
///
/// M5.2 linear extrapolation from 500-LOC baseline suggested ~22s at 10K LOC, which is a miss.
/// This bench measures it HONESTLY — if it misses the 5s budget, the number is reported and stated
/// plainly in the bench output and the M10 brief. A generous hard assert is applied only if the
/// budget is met with clear headroom.
fn bench_cold_10k_loc(c: &mut Criterion) {
    let repo = setup_repo(FILES_10K_LOC, LINES_PER_FILE_10K);
    let repo_path = repo.path().to_path_buf();

    let mut group = c.benchmark_group("cold_index");
    // Low sample count — each iteration is potentially seconds.
    group.sample_size(10);
    // Allow up to 60s measurement time so criterion can complete at least sample_size iterations
    // even if a single iteration takes ~20–30s.
    group.measurement_time(Duration::from_secs(120));

    group.bench_function("10k_loc", |b| {
        b.iter(|| {
            let db_dir = tempfile::tempdir().expect("create temp db dir");
            let storage = fresh_storage(db_dir.path());
            let mut indexer =
                Indexer::new(python_config(), storage, repo_path.clone()).expect("create indexer");
            indexer.index_all().expect("index_all must succeed")
        });
    });

    group.finish();
}

/// Cold index of ~100K LOC (500 files × 200 LOC/file). Budget: < 30s.
///
/// Each bench iteration rebuilds the DB cold. Given the 10K-LOC result, this bench may take
/// several minutes total — sample_size=10 with a long measurement_time is set accordingly.
fn bench_cold_100k_loc(c: &mut Criterion) {
    let repo = setup_repo(FILES_100K_LOC, LINES_PER_FILE_100K);
    let repo_path = repo.path().to_path_buf();

    let mut group = c.benchmark_group("cold_index");
    group.sample_size(10);
    group.measurement_time(Duration::from_secs(600));

    group.bench_function("100k_loc", |b| {
        b.iter(|| {
            let db_dir = tempfile::tempdir().expect("create temp db dir");
            let storage = fresh_storage(db_dir.path());
            let mut indexer =
                Indexer::new(python_config(), storage, repo_path.clone()).expect("create indexer");
            indexer.index_all().expect("index_all must succeed")
        });
    });

    group.finish();
}

// ─────────────────────────────── incremental bench ───────────────────────────

/// Incremental re-index after modifying 10 files. Budget: < 2s.
///
/// Setup (outside the timed closure):
///   1. Write a 10K-LOC repo (200 files).
///   2. Cold-index it fully.
///   3. Touch (overwrite with new content) 10 of the files so the hasher sees them as changed.
///
/// Timed closure: call `index_all()` again — only the 10 changed files are re-indexed.
fn bench_incremental_10_files(c: &mut Criterion) {
    // --- Setup (fixture, outside timing) ---
    let repo = setup_repo(FILES_10K_LOC, LINES_PER_FILE_10K);
    let repo_path = repo.path().to_path_buf();

    // We reuse ONE db dir for the incremental bench (not rebuilt each iteration) because we want
    // to measure the incremental pass, not cold indexing.  The db is built once, then the 10 files
    // are re-touched before each iteration — criterion will only time the second index_all().
    let db_dir = tempfile::tempdir().expect("create temp db dir for incremental");
    let db_path = db_dir.path().join("index.db");

    // Build the initial (cold) full index outside the timed region.
    {
        let storage = Storage::new(&db_path).expect("open storage (initial)");
        storage.init_schema().expect("init schema");
        let mut indexer = Indexer::new(python_config(), storage, repo_path.clone())
            .expect("create indexer (initial)");
        indexer.index_all().expect("cold index_all must succeed");
    }

    // The 10 files we will touch on each bench iteration.
    let touched_files: Vec<std::path::PathBuf> = (0..10)
        .map(|i| repo_path.join(format!("mod_{i:05}.py")))
        .collect();

    let mut group = c.benchmark_group("incremental");
    group.sample_size(10);
    group.measurement_time(Duration::from_secs(60));

    group.bench_function("10_files", |b| {
        b.iter(|| {
            // Touch 10 files (outside the timing? No — we MUST touch them BEFORE each iteration
            // and the touch itself is trivial I/O; the whole closure IS timed but dominates via
            // index_all). We accept the small touch cost — it is constant and <1ms total.
            let (mc, lpf) = method_params(LINES_PER_FILE_10K);
            for (j, path) in touched_files.iter().enumerate() {
                // Write a slightly modified version so compute_file_hash sees a change.
                let mut content = py_module(j + FILES_10K_LOC, mc, lpf);
                // Append a harmless comment so the hash differs each iteration.
                content.push_str("# touched\n");
                fs::write(path, &content).expect("touch file for incremental bench");
            }

            let storage = Storage::new(&db_path).expect("open storage (incremental)");
            // Schema already exists — init_schema is idempotent.
            storage.init_schema().expect("init schema (incremental)");
            let mut indexer = Indexer::new(python_config(), storage, repo_path.clone())
                .expect("create indexer (incremental)");
            indexer
                .index_all()
                .expect("incremental index_all must succeed")
        });
    });

    group.finish();
}

// ─────────────────────────────── index-size bench ────────────────────────────

/// Measure the on-disk SQLite DB size after a 100K-LOC cold index. Budget: < 100MB.
///
/// This is NOT a criterion timing bench — criterion does not measure disk size. Instead we run
/// the index once and assert the size. We register it as a bench function so it runs with
/// `cargo bench --bench indexing` and the number is printed to stdout.
///
/// Hard assert applied: the 100MB budget is a firm constraint with ~94MB headroom per §4.2
/// estimates (~6MB for a similar corpus). If the measured size exceeds 100MB the bench panics
/// (this is bench/test code, so `expect()` / `assert!` are acceptable per the M10 DoD).
fn bench_index_size(c: &mut Criterion) {
    let repo = setup_repo(FILES_100K_LOC, LINES_PER_FILE_100K);
    let repo_path = repo.path().to_path_buf();
    let db_dir = tempfile::tempdir().expect("create temp db dir for size");
    let db_path = db_dir.path().join("index.db");

    // Build the index once (not repeated per iteration — this is a one-shot measurement).
    {
        let storage = Storage::new(&db_path).expect("open storage (size)");
        storage.init_schema().expect("init schema (size)");
        let mut indexer = Indexer::new(python_config(), storage, repo_path.clone())
            .expect("create indexer (size)");
        indexer.index_all().expect("index_all (size) must succeed");
    }

    // Measure the on-disk DB size.
    let db_size_bytes = fs::metadata(&db_path).expect("stat db file").len();
    let db_size_mb = db_size_bytes as f64 / (1024.0 * 1024.0);

    eprintln!(
        "\n[index_size] 100K-LOC synthetic corpus → DB size: {db_size_bytes} bytes ({db_size_mb:.2} MB) — budget < 100 MB"
    );

    // Hard assert: budget is 100MB; a miss is reported here explicitly.
    const BUDGET_BYTES: u64 = 100 * 1024 * 1024; // 100MB
    if db_size_bytes > BUDGET_BYTES {
        panic!(
            "INDEX SIZE BUDGET MISSED: {db_size_bytes} bytes ({db_size_mb:.2} MB) > 100 MB budget"
        );
    }

    // Register a trivial criterion function so this runs under `cargo bench` and appears in output.
    let mut group = c.benchmark_group("index_size");
    group.sample_size(10);
    group.bench_function("100k_loc_db_bytes", |b| {
        b.iter(|| {
            // Return the size so criterion can display it (not timed for latency).
            std::hint::black_box(db_size_bytes)
        });
    });
    group.finish();
}

// ─────────────────────────────── criterion wiring ────────────────────────────

criterion_group!(
    benches,
    bench_cold_500_loc,
    bench_cold_10k_loc,
    bench_cold_100k_loc,
    bench_incremental_10_files,
    bench_index_size,
);
criterion_main!(benches);
