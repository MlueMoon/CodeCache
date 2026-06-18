//! M1 slice M1.1 — config load/validate integration tests (RED first).
//!
//! Scenarios: docs/TEST_STRATEGY.md#config and docs/plans/M1-config-storage.md (M1.1).
//! API anchor: docs/project_plan.md §7.3. All filesystem state is isolated via `tempfile`;
//! assertions check real values, never just `is_ok()`.

use std::io::Write;

use codecache::config::Config;
use codecache::types::Language;

/// Write `contents` into a fresh temp dir as `config.toml`; return (dir, path).
/// The dir guard must be kept alive for the duration of the test.
fn write_config(contents: &str) -> (tempfile::TempDir, std::path::PathBuf) {
    let dir = tempfile::tempdir().expect("create temp dir");
    let path = dir.path().join("config.toml");
    let mut f = std::fs::File::create(&path).expect("create config.toml");
    f.write_all(contents.as_bytes()).expect("write config.toml");
    (dir, path)
}

const FULL_CONFIG: &str = r#"
version = "0.1.0"
index_paths = ["src", "lib"]
ignore_patterns = ["**/*.test.py", "**/test_*.py", "**/*.spec.ts", "**/*_test.go"]
languages = ["python", "typescript", "go"]

[storage]
db_path = ".codecache/index.db"
max_db_size_mb = 500

[retrieval]
default_max_tokens = 4000
default_max_results = 20
bm25_k1 = 1.2
bm25_b = 0.75

[mcp]
transport = "stdio"
sse_port = 3000
"#;

#[test]
fn valid_toml_loads_all_fields_expects_populated_config() {
    let (_dir, path) = write_config(FULL_CONFIG);

    let cfg = Config::load(&path).expect("valid config should load");

    assert_eq!(cfg.index_paths, vec!["src", "lib"]);
    assert_eq!(
        cfg.ignore_patterns,
        vec![
            "**/*.test.py".to_string(),
            "**/test_*.py".to_string(),
            "**/*.spec.ts".to_string(),
            "**/*_test.go".to_string(),
        ]
    );
    assert_eq!(
        cfg.languages,
        vec![Language::Python, Language::TypeScript, Language::Go]
    );
    assert_eq!(cfg.storage.db_path, ".codecache/index.db");
    assert_eq!(cfg.storage.max_db_size_mb, 500);
    assert_eq!(cfg.retrieval.default_max_tokens, 4000);
    assert_eq!(cfg.retrieval.default_max_results, 20);
    assert!((cfg.retrieval.bm25_k1 - 1.2).abs() < f64::EPSILON);
    assert!((cfg.retrieval.bm25_b - 0.75).abs() < f64::EPSILON);
    assert_eq!(cfg.mcp.transport, "stdio");
    assert_eq!(cfg.mcp.sse_port, 3000);
}

#[test]
fn omitted_fields_expects_documented_defaults() {
    // Minimal config: omit everything that has a documented default (§6/§7.3).
    let (_dir, path) = write_config("version = \"0.1.0\"\n");

    let cfg = Config::load(&path).expect("minimal config should load with defaults");

    assert_eq!(cfg.retrieval.default_max_tokens, 4000, "default max_tokens");
    assert_eq!(cfg.retrieval.default_max_results, 20, "default max_results");
    assert!(
        (cfg.retrieval.bm25_k1 - 1.2).abs() < f64::EPSILON,
        "default k1"
    );
    assert!(
        (cfg.retrieval.bm25_b - 0.75).abs() < f64::EPSILON,
        "default b"
    );
    assert_eq!(
        cfg.languages,
        vec![Language::Python, Language::TypeScript, Language::Go],
        "default languages"
    );
}

#[test]
fn missing_file_expects_typed_error() {
    let dir = tempfile::tempdir().expect("temp dir");
    let missing = dir.path().join("does-not-exist.toml");

    let err = Config::load(&missing).expect_err("missing file must error");
    // Typed, not a panic; message should reference the path or "not found".
    let msg = err.to_string().to_lowercase();
    assert!(
        msg.contains("not found") || msg.contains("no such file") || msg.contains("does-not-exist"),
        "error should indicate the missing file, got: {msg}"
    );
}

#[test]
fn invalid_toml_expects_typed_error() {
    // Malformed TOML (unterminated array / bad syntax).
    let (_dir, path) = write_config("index_paths = [\"src\", \n");

    let err = Config::load(&path).expect_err("malformed TOML must error, not panic");
    assert!(!err.to_string().is_empty(), "error should carry a message");
}

#[test]
fn ignore_pattern_parsing_correct() {
    let (_dir, path) =
        write_config("version = \"0.1.0\"\nignore_patterns = [\"**/*.min.js\", \"vendor/**\"]\n");

    let cfg = Config::load(&path).expect("config with ignore patterns loads");
    assert_eq!(
        cfg.ignore_patterns,
        vec!["**/*.min.js".to_string(), "vendor/**".to_string()]
    );
}
