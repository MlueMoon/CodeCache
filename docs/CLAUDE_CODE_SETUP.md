# Setting Up CodeCache as an MCP Server in Claude Code

CodeCache exposes three MCP tools — `codecache_search`, `codecache_update`, and
`codecache_outline` — via a stdio JSON-RPC server. Once configured, Claude Code calls these
tools automatically when it needs to search your codebase for context.

---

## Prerequisites

- **Claude Code** (any recent version that supports MCP stdio servers).
- **Rust 1.85+** if building from source, or the pre-built binary from the GitHub Releases page.

---

## Step 1 — Install the `codecache` binary

**Option A: Install from crates.io (after v0.1.0 is published)**

```bash
cargo install codecache-rs   # the crate is `codecache-rs`; it installs a binary named `codecache`
```

**Option B: Build from source**

```bash
git clone https://github.com/AdvancedUno/codecache
cd codecache
cargo build --release
# The binary is at target/release/codecache (or target/release/codecache.exe on Windows).
# Copy it somewhere on your PATH, or use the full path in the MCP config below.
```

---

## Step 2 — Initialize and index your project

Run these commands once from your project root:

```bash
# Create the .codecache/ directory and index database.
# Paths to index are configured at init time (default: current directory).
codecache init

# Build the full index (Python, TypeScript, and Go files by default).
codecache index
```

The index is stored in `.codecache/index.db`. Add `.codecache/` to your `.gitignore` if you do
not want to commit it (the index is fully reproducible from source).

To update the index after editing files:

```bash
# Incremental update for specific files:
codecache update src/auth.py src/api/routes.py

# Or re-index everything:
codecache index
```

---

## Step 3 — Configure Claude Code to use CodeCache as an MCP server

### Option A: Edit `mcp.json` directly

Add the following to `~/.config/claude-code/mcp.json` (create the file if it does not exist):

```json
{
  "mcpServers": {
    "codecache": {
      "command": "codecache",
      "args": ["serve", "--transport", "stdio"],
      "cwd": "/path/to/your/project"
    }
  }
}
```

Replace `/path/to/your/project` with the absolute path to the project root where you ran
`codecache init`. The `cwd` tells the MCP server where to find `.codecache/index.db`.

If `codecache` is not on your PATH (e.g. you built from source), use the full binary path:

```json
{
  "mcpServers": {
    "codecache": {
      "command": "/home/you/.cargo/bin/codecache",
      "args": ["serve", "--transport", "stdio"],
      "cwd": "/path/to/your/project"
    }
  }
}
```

### Option B: Use the `claude mcp add` CLI (if your Claude Code version supports it)

```bash
claude mcp add codecache \
  --command codecache \
  --args "serve" "--transport" "stdio" \
  --cwd /path/to/your/project
```

---

## The Three MCP Tools

Once connected, Claude Code can call these tools automatically:

### `codecache_search`

Searches the index using BM25 full-text ranking and returns token-budgeted ranked snippets.

**Input:**
```json
{
  "query": "authenticate user credentials",
  "max_tokens": 4000,
  "max_results": 20
}
```

**Output:** A JSON array of ranked code snippets with file path, line range, symbol name,
symbol type, and chunk text. Results are ordered by BM25 relevance score (best first) and
truncated to `max_tokens`.

### `codecache_update`

Incrementally re-indexes a list of file paths. Call this after editing source files so the
index stays current. (Self-healing search in `codecache_search` also re-indexes stale files
transparently, but explicit `update` is faster for large batches.)

**Input:**
```json
{
  "files": ["src/auth.py", "src/api/routes.py"]
}
```

### `codecache_outline`

Returns all indexed symbols for a given file — the full "outline" of what CodeCache knows
about that file.

**Input:**
```json
{
  "path": "src/auth.py"
}
```

The argument is named `path` (not `file_path`), and it must match the file **as indexed**. CodeCache
stores absolute paths, and `codecache_search` returns absolute paths — so when chaining
search → outline, pass the absolute path back. A path that does not match an indexed file returns
`Found 0 symbols`.

**Output:** All chunks indexed for that file: function names, class names, line ranges, and
brief summaries.

---

## Transport Note

**v0.1 supports stdio transport only.** The `--transport sse` flag returns a clean error.
SSE transport is planned for v0.2 (Decision Log D4). For remote/multi-project use, run the
binary locally and connect via stdio.

---

## Troubleshooting

**"No results found" for a query I expect to match:**
- Check that `codecache init` and `codecache index` completed without errors.
- Run `codecache query "<your query>"` from the project root to test retrieval directly.
- Check that the file extension is one CodeCache v0.1 supports: `.py`, `.ts`, `.go` (`.tsx`/JSX
  discovery is deferred post-v0.1).
- BM25 is a lexical retriever — it matches query _terms_ against indexed text. Semantic
  queries ("error handling", "settings not found") may return no results if the vocabulary
  does not appear verbatim in the code. This is a known v0.1 limitation (Decision Log D1);
  hybrid embeddings are planned for v0.2.

**"Permission denied" or binary not found:**
- Ensure `codecache` (or the full path to the binary) is executable and on your PATH.
- On macOS, you may need to allow the binary through Gatekeeper the first time.

**The index is stale after I edited files:**
- Run `codecache update <files>` after each edit batch, or rely on self-healing search (the
  MCP server re-indexes stale files automatically before returning `codecache_search` results).

---

## See Also

- [`README.md`](../README.md) — project overview and quickstart.
- [`CONTRIBUTING.md`](../CONTRIBUTING.md) — development workflow and quality gates.
- [`docs/project_plan.md`](project_plan.md) §8.4 — MCP configuration spec.
- [`docs/ROADMAP.md`](ROADMAP.md) — milestone history and decision log.
