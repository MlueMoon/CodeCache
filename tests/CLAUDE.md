# tests/ — CLAUDE.md

Integration, end-to-end, and property tests for CodeCache. **Owner agent:**
`principal-test-engineering-lead`. Scenario matrix: [`../docs/TEST_STRATEGY.md`](../docs/TEST_STRATEGY.md).

## Purpose
Cross-module tests that exercise the crate from the outside (the `codecache` library + the
built binary). Per-module unit tests live in each module's `#[cfg(test)] mod tests`; this
directory holds the wider integration/E2E/property surface.

## Layout
| Path | Role | Milestone |
|---|---|---|
| `smoke_test.rs` | M0 smoke test: crate links; `codecache::VERSION == CARGO_PKG_VERSION`. | M0 |
| `parser_tests.rs` | M3 parser integration: exact byte spans, method/decorator/nested, ERROR-rate (D2). | M3 |
| `fixtures/` | Sample source trees / files used by integration + E2E tests (added as needed). | M3+ |

### `fixtures/python/` (M3 parser)
Minimal, purpose-built Python files loaded by `parser_tests.rs`. Span assertions compare
`&source[start_byte..end_byte]` to the expected text, so the exact bytes (incl. newlines) matter
— do not reformat these.

| File | Purpose | Newlines |
|---|---|---|
| `valid_module.py` | well-formed module: imports + free fn + class/method (parse-without-error). | LF |
| `top_level_function.py` | single free function `greet`. | LF |
| `simple_class.py` | `Greeter` class with `__init__` + `greet` methods. | LF |
| `nested_function.py` | `outer` free fn containing a nested `inner`. | LF |
| `async_def.py` | `async def fetch`. | LF |
| `decorated_function.py` | `@cache` + `@retry(3)` over `def compute` (decorator-in-span). | LF |
| `multibyte_identifier.py` | `def αβγ(τ)` — multibyte UTF-8 identifiers (byte-vs-char guard). | LF |
| `crlf_function.py` | `def crlf_fn` with CRLF endings (span preserves `\r\n`). | **CRLF** |
| `malformed.py` | one good fn + a broken `def broken(:` → some ERROR nodes (positive rate). | LF |
| `high_error.py` | mostly garbage → ERROR-rate above `HEURISTIC_FALLBACK_THRESHOLD`. | LF |

Integration tests for storage round-trips (M1), parser fixtures (M3), chunker non-overlap
property (M4), indexer idempotency (M5), retriever ranking/budget (M6), formatter goldens +
E2E `init→index→query` (M7), and MCP round-trip (M8) land in their milestones — one file or
module per concern, named after the behavior under test.

## Rules (TDD)
- Tests are written **first** (RED) before any production line they cover (`../docs/ENGINEERING_PLAN.md` §3).
- Never weaken or delete a test to make it pass.
- Property tests use `proptest` (declared in `[dev-dependencies]` from M0).
- Keep fixtures small and deterministic; stable ordering so assertions don't flake.

## Status
M0: only `smoke_test.rs` exists (the RED→GREEN gate for scaffolding). No fixtures yet.
