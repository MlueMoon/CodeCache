//! `codecache serve` handler (M8.1).
//!
//! Starts the MCP server over stdio JSON-RPC. Only `--transport stdio` is supported in v0.1; `sse`
//! returns a clean `anyhow::Error` (D4 transport seam) that surfaces as a nonzero exit — never a
//! panic. The stdio path resolves the db, opens `Storage`, and hands `stdin`/`stdout` locks to the
//! transport-agnostic [`mcp_server::serve`] loop.

use std::io::{stdin, stdout};
use std::path::Path;

use anyhow::{bail, Context, Result};

use crate::mcp_server::{serve, CodeCacheServer};
use crate::storage::Storage;

use super::paths;
use super::Transport;

/// Start the MCP server. `stdio` runs the blocking JSON-RPC loop; `sse` is unsupported in v0.1.
pub fn run(transport: Transport, db_path: &Path) -> Result<()> {
    match transport {
        Transport::Stdio => run_stdio(db_path),
        Transport::Sse => bail!("SSE transport is not supported in v0.1 (stdio only)"),
    }
}

/// Resolve the db, open `Storage`, and drive the stdio JSON-RPC loop until EOF.
fn run_stdio(db_path: &Path) -> Result<()> {
    let root =
        std::env::current_dir().context("could not resolve the current working directory")?;
    let resolved_db = paths::resolve(&root, db_path);
    let storage = Storage::new(&resolved_db)
        .map_err(anyhow::Error::new)
        .with_context(|| format!("could not open index database at {}", resolved_db.display()))?;

    let server = CodeCacheServer::new(storage);
    serve(stdin().lock(), stdout().lock(), server)
}
