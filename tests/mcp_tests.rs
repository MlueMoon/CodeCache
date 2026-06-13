//! M8.1 — MCP server: JSON-RPC framing over stdio + `initialize` handshake (RED, test-lead).
//!
//! These tests drive the (not-yet-implemented) `mcp_server` over an **in-memory** reader/writer
//! pair — feeding it `impl BufRead` and capturing `impl Write` — so the read/dispatch/write loop
//! is exercised WITHOUT spawning the real process or touching real stdin/stdout. This requires the
//! engineering lead to expose a generic, unit-testable server entry point (see the REQUIRED
//! SIGNATURE note below); these tests are written against that contract and so currently fail to
//! compile (the `mcp_server` module is an empty stub). That compile failure IS the RED state.
//!
//! ── PINNED PROTOCOL DECISIONS (M8.1 — honor these in GREEN) ──────────────────────────────────
//!
//! 1. FRAMING = **line-delimited JSON** (newline-framed): exactly one JSON-RPC object per line,
//!    each request and each response terminated by a single `\n`. (The plan §8 says
//!    "newline/length-framed"; we pick newline framing for v0.1 simplicity. No `Content-Length`
//!    headers.) A response written to the writer must therefore be one `\n`-terminated line that
//!    parses as a JSON-RPC 2.0 object.
//!
//! 2. protocolVersion advertised by the server = **"2024-11-05"** (the stable MCP protocol revision;
//!    the project plan does not pin one, so this is the M8.1 decision the eng-lead must match in the
//!    `initialize` result). If the eng-lead must change it, change THIS constant in lock-step and
//!    note it in the brief — the test is the contract.
//!
//! 3. ERROR CODES (JSON-RPC 2.0): parse error = -32700; method not found = -32601; invalid params
//!    = -32602. Every malformed/edge input returns a structured error object; the server NEVER
//!    panics / unwinds across the loop.
//!
//! ── REQUIRED ENTRY-POINT SIGNATURE (what GREEN must expose) ──────────────────────────────────
//!
//! A generic, reader/writer-injected server loop plus a storage-backed context constructor:
//!
//! ```ignore
//! // src/mcp_server/mod.rs
//! pub struct CodeCacheServer { /* holds storage; retriever/indexer wired in M8.3 */ }
//!
//! impl CodeCacheServer {
//!     /// Build a server over a shared `Storage` (D8: one Arc<Mutex<Connection>> lent onward).
//!     pub fn new(storage: codecache::storage::Storage) -> Self;
//! }
//!
//! /// The transport-agnostic (D4) read→dispatch→write loop. Reads line-delimited JSON-RPC
//! /// requests from `reader`, writes line-delimited JSON-RPC responses to `writer`. Returns
//! /// Ok(()) at clean EOF; never panics on malformed input. `R`/`W` are generic so tests can
//! /// inject in-memory pipes instead of real stdio.
//! pub fn serve<R: std::io::BufRead, W: std::io::Write>(
//!     reader: R,
//!     writer: W,
//!     server: CodeCacheServer,
//! ) -> anyhow::Result<()>;
//! ```
//!
//! M8.1 only exercises framing + `initialize` + error mapping; it does NOT call tools/list or
//! tools/call (those are M8.2–M8.4). The handshake path does not touch storage, but the
//! constructor takes `Storage` now so the same seam serves the later slices unchanged.

use std::io::Cursor;

use codecache::mcp_server::{serve, CodeCacheServer};
use codecache::storage::Storage;

/// The protocol revision the server must advertise in its `initialize` result. Pinned by M8.1;
/// change in lock-step with the implementation (see header decision #2).
const PROTOCOL_VERSION: &str = "2024-11-05";

// ── Test harness ────────────────────────────────────────────────────────────────────────────

/// Build a `CodeCacheServer` over a fresh, schema-initialized in-temp-dir database. M8.1's
/// handshake path never reads storage, but constructing the real server (not a mock) keeps the
/// seam honest for M8.2–M8.4 and proves D8 storage-lending compiles.
fn test_server() -> (CodeCacheServer, tempfile::TempDir) {
    let tmp = tempfile::tempdir().expect("create temp dir");
    let db_path = tmp.path().join("index.db");
    let storage = Storage::new(&db_path).expect("open storage");
    storage.init_schema().expect("init schema");
    (CodeCacheServer::new(storage), tmp)
}

/// Drive the server with `input` as the entire (line-delimited) request stream and return the raw
/// bytes the server wrote. Uses in-memory `Cursor`s so nothing touches real stdio (the generic
/// reader/writer seam the eng-lead must provide).
fn run_server(input: &str) -> Vec<u8> {
    let (server, _tmp) = test_server();
    let reader = Cursor::new(input.as_bytes().to_vec());
    let mut output: Vec<u8> = Vec::new();
    // The loop must terminate at EOF and must not panic on any input.
    serve(reader, &mut output, server).expect("serve loop returns Ok at clean EOF");
    output
}

/// Parse the server's output as exactly one line-delimited JSON-RPC response object (framing
/// contract: one `\n`-terminated JSON object). Asserts the line really is `\n`-terminated and
/// that there is exactly one response line, then returns the parsed `Value`.
fn single_response(output: &[u8]) -> serde_json::Value {
    let text = std::str::from_utf8(output).expect("server output must be valid UTF-8");
    assert!(
        text.ends_with('\n'),
        "line-delimited framing: every response line must be \\n-terminated; got: {text:?}"
    );
    let mut lines = text.lines().filter(|l| !l.trim().is_empty());
    let first = lines
        .next()
        .expect("server must write at least one response line");
    let value: serde_json::Value =
        serde_json::from_str(first).expect("each response line must be parseable JSON-RPC");
    assert!(
        lines.next().is_none(),
        "exactly one response expected for a single request; got extra lines in: {text:?}"
    );
    value
}

/// A well-formed JSON-RPC 2.0 `initialize` request line (newline-framed). Mirrors what an MCP
/// client (e.g. Claude Code) sends to open the session.
fn initialize_request_line(id: i64) -> String {
    let req = serde_json::json!({
        "jsonrpc": "2.0",
        "id": id,
        "method": "initialize",
        "params": {
            "protocolVersion": PROTOCOL_VERSION,
            "capabilities": {},
            "clientInfo": { "name": "test-client", "version": "0.0.0" }
        }
    });
    format!(
        "{}\n",
        serde_json::to_string(&req).expect("serialize request")
    )
}

// ── 1. initialize handshake → server capabilities ────────────────────────────────────────────

/// A well-formed `initialize` request yields a JSON-RPC 2.0 response with the SAME `id`,
/// `jsonrpc: "2.0"`, and a `result` carrying the pinned protocolVersion, a `capabilities` object,
/// and `serverInfo` (name + version). Pins the handshake response shape.
#[test]
fn initialize_request_returns_server_capabilities() {
    let output = run_server(&initialize_request_line(1));
    let resp = single_response(&output);

    assert_eq!(
        resp.get("jsonrpc").and_then(|v| v.as_str()),
        Some("2.0"),
        "response must carry jsonrpc \"2.0\"; got: {resp}"
    );
    assert_eq!(
        resp.get("id").and_then(|v| v.as_i64()),
        Some(1),
        "response id must echo the request id; got: {resp}"
    );
    assert!(
        resp.get("error").is_none(),
        "a well-formed initialize must NOT produce an error object; got: {resp}"
    );

    let result = resp
        .get("result")
        .expect("initialize response must carry a `result`");

    assert_eq!(
        result.get("protocolVersion").and_then(|v| v.as_str()),
        Some(PROTOCOL_VERSION),
        "server must advertise the pinned protocolVersion; got: {result}"
    );
    assert!(
        result
            .get("capabilities")
            .map(|c| c.is_object())
            .unwrap_or(false),
        "initialize result must carry a `capabilities` object; got: {result}"
    );

    let server_info = result
        .get("serverInfo")
        .expect("initialize result must carry `serverInfo`");
    assert!(
        server_info
            .get("name")
            .and_then(|v| v.as_str())
            .map(|n| !n.is_empty())
            .unwrap_or(false),
        "serverInfo.name must be a non-empty string; got: {server_info}"
    );
    assert!(
        server_info
            .get("version")
            .and_then(|v| v.as_str())
            .is_some(),
        "serverInfo.version must be a string; got: {server_info}"
    );
}

// ── 2. malformed JSON → parse error (-32700), no panic ───────────────────────────────────────

/// Unparseable input on a request line maps to a JSON-RPC error object with `code == -32700`
/// (Parse error) and `jsonrpc: "2.0"`. The server must not panic on garbage bytes.
#[test]
fn malformed_json_returns_parse_error() {
    // Not valid JSON at all.
    let output = run_server("this is not json at all{{{\n");
    let resp = single_response(&output);

    assert_eq!(
        resp.get("jsonrpc").and_then(|v| v.as_str()),
        Some("2.0"),
        "even an error response must carry jsonrpc \"2.0\"; got: {resp}"
    );
    let error = resp
        .get("error")
        .expect("malformed JSON must produce a JSON-RPC `error` object");
    assert_eq!(
        error.get("code").and_then(|v| v.as_i64()),
        Some(-32700),
        "parse error code must be -32700; got: {error}"
    );
    assert!(
        error.get("message").and_then(|v| v.as_str()).is_some(),
        "JSON-RPC error must carry a `message` string; got: {error}"
    );
}

// ── 3. unknown method → method not found (-32601) ────────────────────────────────────────────

/// A structurally valid JSON-RPC envelope naming a method the server does not implement maps to
/// `code == -32601` (Method not found), echoing the request id.
#[test]
fn unknown_method_returns_method_not_found() {
    let req = serde_json::json!({
        "jsonrpc": "2.0",
        "id": 7,
        "method": "definitely_not_a_real_method",
        "params": {}
    });
    let line = format!(
        "{}\n",
        serde_json::to_string(&req).expect("serialize request")
    );

    let output = run_server(&line);
    let resp = single_response(&output);

    assert_eq!(
        resp.get("id").and_then(|v| v.as_i64()),
        Some(7),
        "error response must echo the request id; got: {resp}"
    );
    let error = resp
        .get("error")
        .expect("unknown method must produce a JSON-RPC `error` object");
    assert_eq!(
        error.get("code").and_then(|v| v.as_i64()),
        Some(-32601),
        "method-not-found code must be -32601; got: {error}"
    );
}

// ── 4. missing required param → invalid params (-32602) ──────────────────────────────────────

/// A method that REQUIRES params, called with the required param(s) missing, maps to
/// `code == -32602` (Invalid params). `initialize` requires `protocolVersion` in its params;
/// omitting params entirely must be rejected as invalid params (not a panic, not a generic OK).
#[test]
fn missing_required_param_returns_invalid_params() {
    // `initialize` with NO params object at all — the required `protocolVersion` is absent.
    let req = serde_json::json!({
        "jsonrpc": "2.0",
        "id": 3,
        "method": "initialize"
    });
    let line = format!(
        "{}\n",
        serde_json::to_string(&req).expect("serialize request")
    );

    let output = run_server(&line);
    let resp = single_response(&output);

    assert_eq!(
        resp.get("id").and_then(|v| v.as_i64()),
        Some(3),
        "error response must echo the request id; got: {resp}"
    );
    let error = resp
        .get("error")
        .expect("missing required param must produce a JSON-RPC `error` object");
    assert_eq!(
        error.get("code").and_then(|v| v.as_i64()),
        Some(-32602),
        "invalid-params code must be -32602; got: {error}"
    );
}

// ── 5. framing discipline: one request → one parseable, \n-terminated response line ──────────

/// Proves the read loop correctly FRAMES a single request and emits a single, line-delimited
/// (`\n`-terminated) response that parses as JSON-RPC 2.0. Asserts on the RAW bytes written: the
/// output is exactly one line, it ends with `\n`, contains no embedded newline before the
/// terminator, and round-trips through `serde_json`. This is the line-delimited framing contract.
#[test]
fn response_is_a_single_newline_terminated_json_line() {
    let output = run_server(&initialize_request_line(42));

    let text = std::str::from_utf8(&output).expect("output must be valid UTF-8");
    assert!(
        text.ends_with('\n'),
        "response must be newline-terminated (line-delimited framing); got: {text:?}"
    );
    // Exactly one line of content (one request ⇒ one response line).
    let content_lines: Vec<&str> = text.lines().filter(|l| !l.trim().is_empty()).collect();
    assert_eq!(
        content_lines.len(),
        1,
        "one request must yield exactly one framed response line; got: {text:?}"
    );
    // The single line has no embedded newline (the only newline is the frame terminator).
    let line = content_lines[0];
    assert!(
        !line.contains('\n'),
        "a framed response line must not contain an embedded newline; got: {line:?}"
    );
    // And it round-trips as a JSON-RPC 2.0 object with the echoed id.
    let value: serde_json::Value =
        serde_json::from_str(line).expect("the framed line must be parseable JSON");
    assert_eq!(value.get("jsonrpc").and_then(|v| v.as_str()), Some("2.0"));
    assert_eq!(value.get("id").and_then(|v| v.as_i64()), Some(42));
}

// ── 6. no panic ever: a batch of malformed/edge lines each yields a structured error ─────────

/// The loop must survive a stream of adversarial lines without unwinding: empty line, blank
/// whitespace, a bare JSON array, a JSON value that is not an object, a truncated object, and a
/// valid-but-unknown method — interleaved with a good `initialize`. Every NON-blank line that the
/// server chooses to answer must produce a parseable JSON-RPC object (never a panic, never a torn
/// half-line). The good `initialize` in the middle must still get a success result, proving the
/// loop recovers after errors.
#[test]
fn malformed_stream_never_panics_and_each_response_is_structured() {
    let mut input = String::new();
    input.push_str("not json\n");
    input.push_str("[1, 2, 3]\n"); // valid JSON, but not a JSON-RPC request object
    input.push_str("12345\n"); // valid JSON scalar, not an object
    input.push_str("{\"jsonrpc\":\"2.0\",\"id\":99,\"method\":\"nope\"}\n"); // unknown method
    input.push_str(&initialize_request_line(100)); // a good one, after the bad ones
    input.push_str("{\"jsonrpc\":\"2.0\",\"id\":\n"); // truncated object

    let (server, _tmp) = test_server();
    let reader = Cursor::new(input.into_bytes());
    let mut output: Vec<u8> = Vec::new();
    // Must return Ok at EOF — no panic, no Err — despite the malformed lines.
    serve(reader, &mut output, server).expect("serve must survive a malformed stream and end Ok");

    let text = std::str::from_utf8(&output).expect("output must be valid UTF-8");
    // Every non-blank response line must be independently parseable JSON (no torn frames).
    let mut saw_initialize_result = false;
    for line in text.lines().filter(|l| !l.trim().is_empty()) {
        let value: serde_json::Value = serde_json::from_str(line).unwrap_or_else(|_| {
            panic!("every emitted response line must be valid JSON; got: {line:?}")
        });
        assert_eq!(
            value.get("jsonrpc").and_then(|v| v.as_str()),
            Some("2.0"),
            "every response must carry jsonrpc \"2.0\"; got: {line:?}"
        );
        // The good initialize (id 100) must have produced a success result, not an error.
        if value.get("id").and_then(|v| v.as_i64()) == Some(100) {
            assert!(
                value.get("result").is_some() && value.get("error").is_none(),
                "the valid initialize amid the noise must succeed; got: {line:?}"
            );
            saw_initialize_result = true;
        }
    }
    assert!(
        saw_initialize_result,
        "the loop must recover after malformed lines and still answer the good initialize; output: {text:?}"
    );
}
