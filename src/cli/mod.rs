//! CLI: argument parsing, command dispatch, user-facing errors.
//!
//! Public API anchor: `project_plan.md` §3.2 / §7 (commands: init/index/update/query/status/
//! config/serve). Owner: `principal-engineering-lead`. Tests live in `tests/cli_tests.rs`;
//! scenarios in `docs/TEST_STRATEGY.md#cli`.
//!
//! M7.2: the `clap` derive surface (parsing + `--help`/`--version` + error → nonzero exit) mirrors
//! §7.1–§7.2 exactly. Command *handlers* are M7.3 — for now they are thin placeholders that print a
//! "not yet implemented" line and return `Ok(())`, never panicking. `Cli::parse()` auto-exits with
//! a nonzero code on a parse/validation error (bad type, out-of-set enum, missing required arg,
//! unknown subcommand), which is what the M7.2 RED tests pin.

use std::path::PathBuf;

use anyhow::Result;
use clap::{Parser, Subcommand, ValueEnum};

/// Default index database location, shared by every command's `--db-path` (§7.2).
const DEFAULT_DB_PATH: &str = ".codecache/index.db";

/// Top-level CLI: `codecache <COMMAND> [OPTIONS]` (§7.1).
#[derive(Debug, Parser)]
#[command(name = "codecache", version, about = "Local-first, AST-driven code-context retrieval engine", long_about = None)]
pub struct Cli {
    /// Enable verbose logging.
    #[arg(short, long, global = true)]
    pub verbose: bool,

    #[command(subcommand)]
    pub command: Command,
}

/// The seven documented subcommands (§7.1).
#[derive(Debug, Subcommand)]
pub enum Command {
    /// Initialize a new CodeCache index in the current directory.
    Init {
        /// Database location.
        #[arg(long, default_value = DEFAULT_DB_PATH)]
        db_path: PathBuf,
        /// Paths to index (can specify multiple).
        #[arg(long, default_values_t = [".".to_string()])]
        index_path: Vec<String>,
        /// Additional ignore patterns beyond .gitignore.
        #[arg(long)]
        ignore: Vec<String>,
        /// Languages to index.
        #[arg(long, value_delimiter = ',', default_values_t = ["python".to_string(), "typescript".to_string(), "go".to_string()])]
        languages: Vec<String>,
    },

    /// Build or rebuild the full index.
    Index {
        /// Force full re-index (ignore existing hashes).
        #[arg(long)]
        full: bool,
        /// Database location.
        #[arg(long, default_value = DEFAULT_DB_PATH)]
        db_path: PathBuf,
        /// Show progress bar.
        #[arg(long)]
        progress: bool,
    },

    /// Incrementally update the index for specific files.
    Update {
        /// Files to update (can use glob patterns).
        #[arg(value_name = "FILE", required = true)]
        files: Vec<PathBuf>,
        /// Database location.
        #[arg(long, default_value = DEFAULT_DB_PATH)]
        db_path: PathBuf,
    },

    /// Search the codebase and retrieve relevant code snippets.
    Query {
        /// Search query (free-form text).
        #[arg(value_name = "QUERY")]
        query: String,
        /// Maximum tokens in output.
        #[arg(long, default_value_t = 4000)]
        max_tokens: usize,
        /// Maximum number of results.
        #[arg(long, default_value_t = 20)]
        max_results: usize,
        /// Output format: toon|json|text.
        #[arg(long, value_enum, default_value_t = OutputFormat::Text)]
        format: OutputFormat,
        /// Restrict search to files matching glob.
        #[arg(long)]
        file_filter: Option<String>,
        /// Database location.
        #[arg(long, default_value = DEFAULT_DB_PATH)]
        db_path: PathBuf,
    },

    /// Show index statistics and health.
    Status {
        /// Database location.
        #[arg(long, default_value = DEFAULT_DB_PATH)]
        db_path: PathBuf,
    },

    /// Manage configuration.
    ///
    /// M7.2 surface is deliberately minimal/forward-compatible: an optional `KEY [VALUE]`
    /// positional pair (read when only `KEY` is given, write when both are given). M7.3 defines
    /// the handler semantics on top of this shape.
    Config {
        /// Configuration key to read or write (omit to operate on the whole config).
        #[arg(value_name = "KEY")]
        key: Option<String>,
        /// New value to set for `KEY` (omit to read the current value).
        #[arg(value_name = "VALUE")]
        value: Option<String>,
        /// Database location.
        #[arg(long, default_value = DEFAULT_DB_PATH)]
        db_path: PathBuf,
    },

    /// Start an MCP server (for Claude Code integration).
    Serve {
        /// Transport type: stdio|sse.
        #[arg(long, value_enum, default_value_t = Transport::Stdio)]
        transport: Transport,
        /// Port for SSE transport.
        #[arg(long, default_value_t = 3000)]
        port: u16,
        /// Database location.
        #[arg(long, default_value = DEFAULT_DB_PATH)]
        db_path: PathBuf,
    },
}

/// CLI-local `--format` value set (`toon|json|text`, §7.2). Kept distinct from
/// [`crate::formatter::Format`] so clap concerns stay in `cli`; mapped over via [`From`] at the
/// handler boundary (M7.3).
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum OutputFormat {
    /// Compact, locator-only `file:start-end` list.
    Toon,
    /// Programmatic JSON.
    Json,
    /// Human-readable text (the default).
    Text,
}

impl From<OutputFormat> for crate::formatter::Format {
    fn from(value: OutputFormat) -> Self {
        match value {
            OutputFormat::Toon => crate::formatter::Format::Toon,
            OutputFormat::Json => crate::formatter::Format::Json,
            OutputFormat::Text => crate::formatter::Format::Text,
        }
    }
}

/// CLI-local `--transport` value set (`stdio|sse`, §7.2).
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum Transport {
    /// Standard-IO JSON-RPC transport (the default; for Claude Code).
    Stdio,
    /// Server-Sent-Events transport (for web clients).
    Sse,
}

/// Entry point invoked by `main`. Parses argv (clap auto-exits nonzero on a parse error, prints
/// `--help`/`--version`) and dispatches to a thin per-command handler. Handlers are M7.3 stubs for
/// now; nothing here panics or `unwrap`s on a reachable path.
pub fn run() -> Result<()> {
    let cli = Cli::parse();
    dispatch(cli)
}

/// Route a parsed [`Cli`] to its command handler. Split out from [`run`] so M7.3 can grow real
/// handlers (and unit-test dispatch) without re-touching argv parsing.
fn dispatch(cli: Cli) -> Result<()> {
    match cli.command {
        Command::Init { .. } => not_yet_implemented("init"),
        Command::Index { .. } => not_yet_implemented("index"),
        Command::Update { .. } => not_yet_implemented("update"),
        Command::Query { .. } => not_yet_implemented("query"),
        Command::Status { .. } => not_yet_implemented("status"),
        Command::Config { .. } => not_yet_implemented("config"),
        Command::Serve { .. } => {
            println!("serve: the MCP server is not implemented yet (M8).");
            Ok(())
        }
    }
}

/// Placeholder for an M7.3 command handler: prints a clean notice and succeeds (no panic).
fn not_yet_implemented(command: &str) -> Result<()> {
    println!("{command}: not yet implemented (M7.3).");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::CommandFactory;

    #[test]
    fn cli_definition_is_valid() {
        // Catches clap-derive construction errors (duplicate flags, bad defaults) at test time.
        Cli::command().debug_assert();
    }

    #[test]
    fn output_format_maps_to_formatter_format() {
        assert_eq!(
            crate::formatter::Format::from(OutputFormat::Toon),
            crate::formatter::Format::Toon
        );
        assert_eq!(
            crate::formatter::Format::from(OutputFormat::Json),
            crate::formatter::Format::Json
        );
        assert_eq!(
            crate::formatter::Format::from(OutputFormat::Text),
            crate::formatter::Format::Text
        );
    }
}
