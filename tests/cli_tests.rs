//! M7.2 — CLI parsing + errors + exit codes (RED, test-lead).
//!
//! Pins the clap surface from `docs/project_plan.md` §7.1–§7.2 by driving the BUILT
//! `codecache` binary as a subprocess (`assert_cmd`) and matching stdout/stderr/exit
//! codes (`predicates`). These tests live at the *parsing* layer — they only invoke
//! `--help` (which clap handles before any command handler runs) or trigger clap's own
//! arg/enum/required-arg validation. No command handler logic is exercised here; handler
//! behavior is M7.3 and is verified end-to-end in M7.4.
//!
//! RED rationale: the current M0 stub (`src/cli/mod.rs::run`) ignores all args and just
//! prints `codecache <VERSION>` then exits 0. So `<cmd> --help` will NOT exit 0 with the
//! documented flag text, an unknown command will NOT error, and a missing required arg
//! will NOT be rejected. Every assertion below fails against that stub for the right
//! reason: clap parsing is not implemented yet. The file COMPILES (assert_cmd drives a
//! subprocess — there is no not-yet-existing library API to import), so the failures are
//! purely behavioral.

use assert_cmd::Command;
use predicates::prelude::*;
use predicates::str::contains;

/// The seven documented subcommands (§7.1).
const SUBCOMMANDS: [&str; 7] = [
    "init", "index", "update", "query", "status", "config", "serve",
];

/// Fresh handle to the built binary for each invocation (parallel-safe: no shared state).
fn cc() -> Command {
    Command::cargo_bin("codecache").expect("binary `codecache` should build")
}

// ---------------------------------------------------------------------------
// 1. Each command parses its documented flags (§7.2).
//    `<cmd> --help` exits 0 and the help text names that command's flags. This
//    pins flag NAMES at the parsing layer without running any handler.
// ---------------------------------------------------------------------------

#[test]
fn each_command_parses_its_documented_flags() {
    // init — §7.2: --db-path, --index-path, --ignore, --languages
    cc().args(["init", "--help"])
        .assert()
        .success()
        .stdout(contains("--db-path"))
        .stdout(contains("--index-path"))
        .stdout(contains("--ignore"))
        .stdout(contains("--languages"));

    // index — §7.2: --full, --db-path, --progress
    cc().args(["index", "--help"])
        .assert()
        .success()
        .stdout(contains("--full"))
        .stdout(contains("--db-path"))
        .stdout(contains("--progress"));

    // update <FILE>... — §7.2: --db-path (positional FILE arg shown in usage)
    cc().args(["update", "--help"])
        .assert()
        .success()
        .stdout(contains("--db-path"))
        .stdout(contains("FILE"));

    // query <QUERY> — §7.2: --max-tokens, --max-results, --format, --file-filter, --db-path
    cc().args(["query", "--help"])
        .assert()
        .success()
        .stdout(contains("--max-tokens"))
        .stdout(contains("--max-results"))
        .stdout(contains("--format"))
        .stdout(contains("--file-filter"))
        .stdout(contains("--db-path"))
        .stdout(contains("QUERY"));

    // status — §7.2: --db-path
    cc().args(["status", "--help"])
        .assert()
        .success()
        .stdout(contains("--db-path"));

    // config — §7.2 gives no detailed flag spec; M7.3 defines the handler. RED-minimal:
    // assert it is a recognized subcommand whose `--help` parses and exits 0.
    cc().args(["config", "--help"]).assert().success();

    // serve — §7.2: --transport, --port, --db-path
    cc().args(["serve", "--help"])
        .assert()
        .success()
        .stdout(contains("--transport"))
        .stdout(contains("--port"))
        .stdout(contains("--db-path"));
}

// ---------------------------------------------------------------------------
// 2. Query defaults match the spec (§7.2): --max-tokens 4000, --max-results 20,
//    --format text, value set toon|json|text. Pinned via help output, not by
//    executing the query handler (that is M7.3).
// ---------------------------------------------------------------------------

#[test]
fn query_defaults_match_spec() {
    let assert = cc().args(["query", "--help"]).assert().success();

    // Defaults are advertised in clap help as `[default: <value>]`.
    assert
        .stdout(contains("4000"))
        .stdout(contains("20"))
        // Default format is text.
        .stdout(contains("text"))
        // The accepted format value set is toon|json|text.
        .stdout(contains("toon"))
        .stdout(contains("json"));
}

// ---------------------------------------------------------------------------
// 3. Help & version flags work (§7.1 global options).
//    --help/-h list all 7 subcommands; --version/-V print the crate version;
//    global -v/--verbose is accepted (surfaces in top-level help).
// ---------------------------------------------------------------------------

#[test]
fn help_and_version_flags_work() {
    // `--help` exits 0 and lists every subcommand.
    let long = cc().arg("--help").assert().success();
    let mut long = long;
    for sub in SUBCOMMANDS {
        long = long.stdout(contains(sub));
    }

    // `-h` is the short alias and behaves the same (lists subcommands).
    let short = cc().arg("-h").assert().success();
    let mut short = short;
    for sub in SUBCOMMANDS {
        short = short.stdout(contains(sub));
    }

    // Global verbose flag is advertised in top-level help (both spellings).
    cc().arg("--help")
        .assert()
        .success()
        .stdout(contains("--verbose"))
        .stdout(contains("-v"));

    // `--version` and `-V` exit 0 and print the crate version (env!("CARGO_PKG_VERSION")).
    let version = env!("CARGO_PKG_VERSION");
    cc().arg("--version")
        .assert()
        .success()
        .stdout(contains(version));
    cc().arg("-V").assert().success().stdout(contains(version));
}

// ---------------------------------------------------------------------------
// 4. Bad args exit nonzero with a stderr message — true at the *parsing* layer
//    (clap type/enum/required-arg validation), independent of handler logic.
// ---------------------------------------------------------------------------

#[test]
fn bad_args_exit_nonzero_with_message() {
    // Non-numeric value for an integer flag: clap's type validation rejects it.
    cc().args(["query", "needle", "--max-tokens", "notanumber"])
        .assert()
        .failure()
        .stderr(predicate::str::is_empty().not());

    // Invalid enum value for --transport (value set is stdio|sse): clap rejects it.
    cc().args(["serve", "--transport", "bogus"])
        .assert()
        .failure()
        .stderr(predicate::str::is_empty().not());

    // Missing the required positional <QUERY> arg: clap errors before any handler runs.
    cc().arg("query")
        .assert()
        .failure()
        .stderr(predicate::str::is_empty().not());
}

// ---------------------------------------------------------------------------
// 5. Unknown command errors cleanly — nonzero exit, stderr names the bad
//    subcommand, no panic (no "panicked at" in output).
// ---------------------------------------------------------------------------

#[test]
fn unknown_command_errors_cleanly() {
    cc().arg("frobnicate")
        .assert()
        .failure()
        // clap reports the unrecognized subcommand by name.
        .stderr(contains("frobnicate"))
        // A clean parse error, never a Rust panic.
        .stderr(contains("panicked").not())
        .stdout(contains("panicked").not());
}
