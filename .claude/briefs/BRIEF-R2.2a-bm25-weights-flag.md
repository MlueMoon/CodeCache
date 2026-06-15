# BRIEF — R2.2a / CLI-reachable per-column BM25 weight override (the crate flag)

- **Milestone:** R2 (offline ablations, D23) — slice **R2.2a** (crate flag only)  ·  **Module(s):** `cli` (query), `retriever`, `storage`
- **Owner (manager):** principal-engineering-manager  ·  **Created:** 2026-06-14
- **Status:** RED ✅  GREEN ✅  REVIEW ✅  DONE ✅ (staged — awaiting main-session commit)
- **Links:** docs/ROADMAP.md#D23 / #D24 · docs/TEST_STRATEGY.md#retriever · docs/TEST_STRATEGY.md#storage-sqlite--fts5 · docs/TEST_STRATEGY.md#cli · docs/project_plan.md §3.2.2 / §3.2.3 / §7.2 (`codecache query`)

## Goal
Expose the 7 per-column FTS5 BM25 weights (currently hardcoded in `src/storage/queries.rs::SEARCH`)
as a **CLI-reachable, per-invocation override** — `codecache query "<terms>" --bm25-weights
"10,1,1,5,2,2,2"` — so the R2 research harness can sweep ranking weights across `codecache query`
calls over the process boundary **without recompiling**. The flag is the *only* deliverable here;
the research-side sweep (R2.2b) is a separate follow-on.

## Scope (in / out)
- **In:**
  - `QueryOptions.bm25_weights: Option<[f64; 7]>` (new field; §3.2.3, already amended in the plan).
  - `Storage::search_with_weights(query, limit, weights: Option<&[f64; 7]>)` (new; §3.2.2, amended).
    Existing `Storage::search` delegates to it with `None` → **default path byte-identical**.
  - `Retriever::query` threads `options.bm25_weights` into the storage call (was `storage.search`).
  - CLI `query --bm25-weights <W>`: parse 7 comma-separated `f64` into `[f64; 7]`; absent ⇒ `None`.
  - Plan/docs already updated (this brief's manager did §3.2.2/§3.2.3/§7.2 first); finish TODO +
    storage/cli/retriever CLAUDE.md + ROADMAP D24 at OUTCOME.
- **Out (defer):**
  - The actual weight **sweep / ablation** over the harness → **R2.2b** (research-harness-engineer).
  - Glob support for `--file-filter` (unchanged; still exact-match v0.1).
  - Any change to `bm25_k1`/`bm25_b` (config) — these are **inert** w.r.t. FTS5 `bm25()` and are NOT
    this surface. Do not conflate. (`config/mod.rs:62-67`.)
  - A config-file key for weights (CLI flag is the hard requirement; a config key is optional and
    **out of scope** unless the team finds it free — bias to the smallest increment).
  - `Cargo.toml` changes / new deps — **forbidden**.

## The precise seam (pre-investigated — do not re-grep from scratch)
- `src/storage/queries.rs:34-42` — `pub const SEARCH` bakes the weights into the SQL literal:
  `bm25(symbols, 10.0, 1.0, 1.0, 5.0, 2.0, 2.0, 2.0) AS score ... ORDER BY score ASC, rowid ASC`.
- Weight order = indexed-column order (`schema::CREATE_SYMBOLS`): **symbol_name(10.0), symbol_type(1.0),
  chunk_text(1.0), parent_symbol(5.0), imports(2.0), cross_references(2.0), file_docstring(2.0)** —
  exactly 7 indexed columns. UNINDEXED columns never take a weight.
- `src/storage/mod.rs:184` — `search(&self, query, limit)` runs `conn.prepare_cached(queries::SEARCH)`
  and maps rows via `map_search_row`. `search_with_weights` mirrors this but with a weight-parameterized
  ranking expression.
- `src/retriever/mod.rs:251` — `let mut hits = self.storage.search(&match_expr, options.max_results)?;`
  is the single call site to re-route through `search_with_weights(.., options.bm25_weights.as_ref())`.
- `src/cli/query.rs:36-42` — `QueryOptions { max_tokens, max_results, file_filter }` is constructed
  here; add `bm25_weights`. The clap `Command::Query` variant (in `src/cli/mod.rs`) gets the new
  `--bm25-weights` arg; `dispatch` threads it to `query::run`.

## KEY TECHNICAL NOTE — FTS5 `bm25()` weights (eng-lead + rust-treesitter-specialist)
FTS5's `bm25()` weights are **auxiliary-function arguments**, not value positions — they **cannot**
be bound as `?` parameters. The 7 weights therefore must be **formatted into the SQL ranking
expression string**. This is safe **ONLY** because each weight is a validated `f64` parsed from the
constrained flag — **NEVER interpolate raw CLI text into SQL.** The specialist must confirm the
bind-vs-format conclusion and that the formatted `f64` (e.g. via `{:?}`/a finite-guarded format)
produces a valid `bm25(symbols, <w1>, …, <w7>)` for all 7 finite values. The `MATCH ?1` / `LIMIT ?2`
bindings stay parameterized exactly as today.

## Design (manager's default call — eng-lead may refine the storage shape, not the CLI contract)
- `[f64; 7]` (fixed-size array) makes **"exactly 7 weights" a compile-time invariant** for everything
  below the CLI parse boundary — the arity error can only occur at the single CLI parse site, where
  it must be a **typed error / clean nonzero exit, never a panic**.
- `search` delegates to `search_with_weights(.., None)`; `None` reuses the existing default SQL so the
  default path is provably unchanged (don't reformat the default weights into a new code path that
  could drift — either keep `SEARCH` as the `None` arm, or assert the formatted default equals it).
- Validation (CLI): split on `,`, parse each to `f64`, require exactly 7, reject non-finite
  (NaN/±inf) — surface a clear typed error → nonzero exit. Reasonable to allow negative/zero weights
  (FTS5 accepts them; the sweep may want them) — **document** whatever bound is chosen.

## Scenarios to cover (from TEST_STRATEGY — test-lead writes these RED first)
Storage (`tests/storage_tests.rs` or `src/storage` unit):
- [ ] happy: `search_with_weights(q, lim, Some(&w))` with a weight vector that **reorders** results
      vs the default returns the expected re-ranked order (e.g. zero out `symbol_name` weight so a
      body-only match is no longer outranked by a name match) — proves the weights actually take effect.
- [ ] default-identical: `search(q, lim)` and `search_with_weights(q, lim, None)` and
      `search_with_weights(q, lim, Some(&[10.,1.,1.,5.,2.,2.,2.]))` all return **identical** results
      for the same seed (the documented default round-trips).
- [ ] determinism: ordering still `bm25 ASC` then the documented span tie-break under custom weights.
- [ ] edge: weights with a zero / a negative entry don't error at the storage layer (FTS5 accepts them).
Retriever (`tests/retriever_tests.rs`):
- [ ] `QueryOptions { bm25_weights: None, .. }` is byte-identical to today (every existing retriever
      test stays green — they construct `QueryOptions` without the field, so update the struct
      literals; the **behavior** must not change).
- [ ] `QueryOptions { bm25_weights: Some(custom), .. }` propagates to storage and changes ranking.
CLI (`tests/cli_tests.rs`, assert_cmd):
- [ ] `--bm25-weights "10,1,1,5,2,2,2"` parses and runs (exit 0 on an indexed fixture).
- [ ] **malformed input is a clean error, never a panic** → nonzero exit + stderr message:
      wrong arity (`"1,2,3"` — 3 values; `"1,2,3,4,5,6,7,8"` — 8 values), non-numeric (`"a,b,c,…"`),
      empty (`--bm25-weights ""`). Assert nonzero exit and no panic string.
- [ ] absent flag ⇒ default behavior unchanged (existing query CLI tests stay green).

## Definition of Done (manager enforces)
- [ ] Tests written first, now green · `cargo clippy --all-targets -- -D warnings` clean · `cargo fmt --all -- --check` clean
- [ ] API matches project_plan §3.2.2/§3.2.3/§7.2 (amended pre-code) · D24 recorded in ROADMAP
- [ ] **DEFAULT-IDENTICAL**: flag absent ⇒ byte-identical to today (10,1,1,5,2,2,2); every existing
      storage/retriever/cli test stays green
- [ ] No reachable `unwrap()/expect()/panic!`; arity/parse errors are typed → clean nonzero exit
- [ ] Deterministic ordering preserved (`bm25 ASC` + documented span tie-break)
- [ ] `Cargo.toml` UNTOUCHED · reviewer APPROVED
- [ ] docs/TODO.md (R2.2 row) + storage/cli/retriever CLAUDE.md updated in the same change

## Gates are OFF — run them EXPLICITLY and paste real output
The Stop/SubagentStop cargo hooks are disabled in an uncommitted `.claude/settings.json`. The
eng-lead and reviewer MUST run and paste:
- `cargo fmt --all -- --check`
- `cargo clippy --all-targets -- -D warnings`
- `cargo test`
Leave `.claude/settings.json` untouched and out of staging.

---
## RED — test lead (2026-06-14)

### Status flip
`Status:` line above → **RED ✅** (tests written, fail for the right reason). GREEN/REVIEW/DONE still ▢.

### Tests added (file · name · one-line intent)

**`tests/storage_tests.rs`** — new R2.2a block (constants `DEFAULT_WEIGHTS`, `REORDER_WEIGHTS`;
helpers `seed_name_vs_body`, `names_of`; reuses existing `fresh_storage`/`chunk`):
- `search_with_weights_custom_reorders_ranking_vs_default` — custom weights flip the default
  name-match-first order to body-match-first; asserts BOTH orders + `assert_ne!`.
- `search_with_weights_none_is_identical_to_default` — `search(q,lim)` == `search_with_weights(.., None)`
  == `search_with_weights(.., Some(&DEFAULT_WEIGHTS))` (identical `Vec<SearchResult>`).
- `search_with_weights_custom_is_deterministic_and_bm25_ordered` — repeated custom-weighted calls
  identical; scores stay non-decreasing (bm25 ASC).
- `search_with_weights_zero_and_negative_do_not_error` — weight vector `[0.0,1,1,5,2,2,-1.0]` ⇒ `Ok`,
  both rows still returned (FTS5 accepts zero/negative).

**`tests/retriever_tests.rs`** — updated ALL 7 existing `QueryOptions` literals (incl. the `opts()`
helper) to add `bm25_weights: None` (default-identical; not a behavior change), then added:
- `bm25_weights_some_changes_ranking_vs_none` — same seed/query, `None` vs `Some(REORDER_WEIGHTS)`
  yield DIFFERENT orderings through `Retriever::query` (const `REORDER_WEIGHTS` mirrors storage).

**`tests/retrieval_quality.rs`** — extended its single `QueryOptions` literal (+ the module doc
example) with `bm25_weights: None` so the research-quality suite keeps compiling (default-identical).

**`tests/cli_tests.rs`** — new R2.2a block (reuses `temp_project`/`cc_in`/`cc`):
- `query_help_lists_bm25_weights_flag` — `query --help` advertises `--bm25-weights` (RED: flag absent).
- `query_accepts_bm25_weights_flag` — `--bm25-weights "10,1,1,5,2,2,2"` runs to exit 0 on the indexed
  enriched fixture, still surfaces `hash_password` (RED: clap rejects the unknown flag, exit 2).
- `query_bm25_weights_malformed_exits_nonzero_without_panic` — wrong arity (`"1,2,3"`,
  `"1,2,3,4,5,6,7,8"`), non-numeric (`"a,b,c,d,e,f,g"`), empty (`""`) ⇒ `.failure()` + non-empty
  stderr + no `panicked` on either stream.

### RED output (captured)
Lib + binary build clean (no production code touched). Three test crates fail to COMPILE on the
missing API (the intended RED); the CLI tests compile and fail at runtime on the unknown flag.

```
$ cargo test   (error summary, grouped)
  10 error[E0560]: struct `QueryOptions` has no field named `bm25_weights`
   7 error[E0599]: no method named `search_with_weights` found for struct `codecache::storage::Storage`
   could not compile `codecache` (test "storage_tests")     — 7 errors
   could not compile `codecache` (test "retriever_tests")   — 9 errors
   could not compile `codecache` (test "retrieval_quality") — 1 error
```
- `storage_tests`: 7× `E0599 no method named search_with_weights` (lines 702/711/736/739/760/764/792).
- `retriever_tests`: 9× `E0560 no field bm25_weights` (7 updated literals + 2 in the new test).
- `retrieval_quality`: 1× `E0560 no field bm25_weights` (line 441).

CLI (compiles; runtime RED):
```
$ cargo test --test cli_tests bm25_weights
  query_accepts_bm25_weights_flag   FAILED — code=2, stderr: "error: unexpected argument '--bm25-weights' found"
  query_help_lists_bm25_weights_flag FAILED — help omits --bm25-weights
  query_bm25_weights_malformed_..    PASSED (see note below)
  test result: FAILED. 1 passed; 2 failed.
```

### What the impl MUST satisfy (pinned by these tests — don't reverse-engineer)
1. **Signature:** `Storage::search_with_weights(&self, query: &str, limit: usize,
   weights: Option<&[f64; 7]>) -> storage::Result<Vec<SearchResult>>`; `search` delegates to it with
   `None`. `QueryOptions.bm25_weights: Option<[f64; 7]>`. CLI `query --bm25-weights "<7 csv f64>"`.
2. **Weight order = `schema::CREATE_SYMBOLS` indexed-column order:**
   `[symbol_name, symbol_type, chunk_text, parent_symbol, imports, cross_references, file_docstring]`.
3. **The exact vectors the tests pin** (do not change — the seed makes the flip deterministic):
   - `DEFAULT_WEIGHTS = [10.0, 1.0, 1.0, 5.0, 2.0, 2.0, 2.0]` (must equal what `queries::SEARCH` bakes in).
   - `REORDER_WEIGHTS = [0.0, 1.0, 5.0, 1.0, 1.0, 1.0, 1.0]` (symbol_name→0, chunk_text→5).
   - Edge vector `[0.0, 1.0, 1.0, 5.0, 2.0, 2.0, -1.0]` ⇒ `Ok`.
4. **Reorder seed (verified empirically against real FTS5):** two chunks sharing the term `session` —
   `a.py` symbol_name=`session` (term in NAME), `b.py` symbol_name=`helper` body `"... session and the
   session again"` (term in BODY only), both bodies `def …(): …`. DEFAULT ⇒ `[session, helper]`;
   REORDER ⇒ `[helper, session]`. Scores DIFFER under both weightings (no tie), so the flip is decided
   purely by bm25 and the retriever's `(file_path, start_byte)` tie-break never engages.
5. **`None` byte-identical:** keep `queries::SEARCH` as the `None` arm (or prove the formatted default
   equals it) — `search`/`None`/`Some(&DEFAULT_WEIGHTS)` must return identical `Vec<SearchResult>`.
6. **FTS5 weights can't be bound:** format the 7 validated `f64` into the `bm25(symbols, …)` ranking
   expression; keep `MATCH ?1` / `LIMIT ?2` parameterized. Never interpolate raw CLI text.
7. **Ordering invariant unchanged:** `bm25 ASC, rowid ASC` (storage) / `(bm25, file_path, start_byte,
   end_byte)` (retriever).
8. **CLI malformed ⇒ typed error → nonzero, NEVER panic.** Validate: split on `,`, parse each to `f64`,
   require exactly 7, reject empty/non-numeric (consider rejecting NaN/±inf per the design note). No
   reachable `unwrap/expect/panic`.

### NOTE for eng-lead — the malformed CLI test currently PASSES (and that's expected)
`query_bm25_weights_malformed_exits_nonzero_without_panic` is GREEN-on-arrival **today** only because
clap rejects `--bm25-weights` as an *unknown flag* (already nonzero, non-empty stderr, no panic). Once
you ADD the flag, malformed values will reach YOUR parser instead of clap's unknown-arg path — the test
then becomes the real lock-in that your parse returns a typed error (nonzero + stderr, no panic) rather
than `unwrap()`-panicking on `""` / non-numeric / wrong arity. Do not "fix" it by making the parser
panic; keep it nonzero+clean.

### Impl-side `QueryOptions` sites to update when adding the field (not the test-lead's to touch)
- `src/retriever/mod.rs` — the struct def + its `Default` impl (add `bm25_weights: None`).
- `src/cli/query.rs:36` — struct literal (thread the parsed flag in).
- `src/mcp_server/handlers.rs:50` — uses `..Default::default()` ⇒ inherits `None` automatically (MCP
  stays default-weighted; CLI-only surface per scope). `benches/query_bench.rs` uses `::default()` ⇒
  also fine. No other literal sites.

## GREEN — engineering lead (2026-06-14)

`Status:` → **GREEN ✅** (RED tests now pass; all three gates clean). REVIEW/DONE still ▢.

### Files touched (production only — docs/tests were already in place from the manager/test-lead)
- `src/storage/queries.rs` — added `pub fn search_with_weights_sql(&[f64; 7]) -> String` (clones the
  `SEARCH` SQL but formats the 7 weights into `bm25(symbols, …)`) + private `fmt_weight(f64) -> String`
  (finite-f64 → SQL numeric literal via `{:?}`). `SEARCH` const itself **unchanged** (the `None`/default
  arm).
- `src/storage/mod.rs` — added `Storage::search_with_weights(&self, &str, usize, Option<&[f64; 7]>) ->
  Result<Vec<SearchResult>>`; `search` now delegates to it with `None`. Added typed
  `StorageError::NonFiniteWeight(f64)` (Display + the match arm). Reuses `map_search_row`/`RawSearchRow`
  unchanged.
- `src/retriever/mod.rs` — `QueryOptions.bm25_weights: Option<[f64; 7]>` (+ `Default = None`); `query`
  calls `storage.search_with_weights(&expr, max_results, options.bm25_weights.as_ref())` (was
  `storage.search`).
- `src/cli/mod.rs` — `Command::Query` gains `#[arg(long, value_name = "W")] bm25_weights:
  Option<String>`; `dispatch` threads `bm25_weights.as_deref()` into `query::run`.
- `src/cli/query.rs` — new `run` param `bm25_weights: Option<&str>`; private `parse_bm25_weights(&str)
  -> anyhow::Result<[f64; 7]>` (+ `BM25_WEIGHT_COUNT = 7`); parsed **before** opening storage; threaded
  into the `QueryOptions` literal. Added a `#[cfg(test)] mod tests` with 4 parser unit tests.
- CLAUDE.md updated: `src/storage`, `src/retriever`, `src/cli`. `docs/TODO.md` R2.2a row ticked.

### `search_with_weights` signature + the f64→SQL formatting approach
```rust
pub fn search_with_weights(
    &self, query: &str, limit: usize, weights: Option<&[f64; 7]>,
) -> storage::Result<Vec<SearchResult>>
```
- `None` ⇒ `conn.prepare_cached(queries::SEARCH)` verbatim (the default path is provably the same
  cached statement as before — **byte-identical**). `search(q, lim)` is now exactly
  `search_with_weights(q, lim, None)`.
- `Some(w)` ⇒ first reject any non-finite entry (`StorageError::NonFiniteWeight`), then
  `conn.prepare(&queries::search_with_weights_sql(w))`. `MATCH ?1` / `LIMIT ?2` stay **bound**; only the
  7 weights are formatted in. **Formatting = `format!("{w:?}")` per weight** — Rust's `f64` `Debug`
  always emits a decimal point and is locale-independent, so `10.0`→`10.0` (a `REAL` literal, not a
  bareword), `0.0`→`0.0`, `-1.0`→`-1.0` (unary-minus on a numeric literal — valid SQLite), `1e-10`
  stays `1e-10` (SQLite accepts scientific notation). Verified empirically with a throwaway `rustc`
  check. The weighted SQL is dynamic per vector, so `prepare` (not `prepare_cached`, which keys on a
  constant string).

### CLI parse/validation behavior (and the negatives/zeros decision)
`parse_bm25_weights(raw)`: `raw.split(',')` → require **exactly 7** fields (else typed error naming the
7 columns + the actual count) → `field.trim().parse::<f64>()` each (non-numeric → typed error w/ the
"invalid float literal" cause) → reject `!is_finite()` (NaN/±inf → typed error). **Decision: zeros and
negatives ARE allowed** (per the brief — FTS5 `bm25()` honors them and the R2 sweep wants them; only
non-finite / non-numeric / wrong-arity are rejected). The empty string `""` is one empty field ⇒ arity
1 ≠ 7 ⇒ rejected by the arity check. Parsing happens **before** `Storage::new`, so a malformed flag
fails fast. Returns `anyhow::Result` → `main` maps `Err` to a clean **nonzero exit**; no reachable
`unwrap/expect/panic`. The helper is module-private and unit-tested (4 tests).

### How each RED test now passes
- `storage_tests` (4): `search_with_weights_custom_reorders_ranking_vs_default` — DEFAULT vs REORDER
  on the `session` name-vs-body seed flips `[session,helper]`→`[helper,session]` (custom weights reach
  `bm25()`). `…_none_is_identical_to_default` — `search` / `…(None)` / `…(Some(&DEFAULT_WEIGHTS))` are
  identical (`None` reuses `SEARCH`; the formatted default `10.0,1.0,…` produces the same ranking).
  `…_custom_is_deterministic_and_bm25_ordered` — repeated custom calls identical, scores non-decreasing.
  `…_zero_and_negative_do_not_error` — `[0,1,1,5,2,2,-1]` ⇒ `Ok`, both rows returned (the finite-guard
  only blocks NaN/inf).
- `retriever_tests` (8 literals + 1 new): all 7 prior literals + `opts()` now carry `bm25_weights: None`
  (default-identical — orders unchanged). `bm25_weights_some_changes_ranking_vs_none` — `None` vs
  `Some(REORDER_WEIGHTS)` through `Retriever::query` yields different orderings.
- `retrieval_quality` (1 literal): carries `bm25_weights: None` ⇒ compiles, quality unchanged.
- `cli_tests` (3): `query_help_lists_bm25_weights_flag` — clap help now advertises `--bm25-weights`.
  `query_accepts_bm25_weights_flag` — `"10,1,1,5,2,2,2"` parses + runs to exit 0, still surfaces
  `hash_password`. `query_bm25_weights_malformed_exits_nonzero_without_panic` — wrong arity (3, 8),
  non-numeric, empty all hit **my** `parse_bm25_weights` (not clap's unknown-flag path) and return a
  typed error → `.failure()` + non-empty stderr + no `panicked`. (Per the test-lead's note: the flag now
  exists, so this test became the real typed-error lock-in; I did NOT make the parser panic.)

### Plan deviations
**None.** Signatures match `docs/project_plan.md` §3.2.2 (`search_with_weights`), §3.2.3
(`QueryOptions.bm25_weights: Option<[f64; 7]>`), §7.2 (`--bm25-weights <W>`) exactly. One additive,
in-scope extra beyond the literal brief: a typed `StorageError::NonFiniteWeight` storage-level guard —
the brief explicitly called this "acceptable and preferred" (map non-finite to a typed error rather
than emitting `inf`/`NaN`). Also added 4 CLI parser unit tests (the brief asked the helper be
"unit-testable"); these are new *tests by the implementer for a pure helper*, not modifications to any
RED test.

### Gate output (pasted, run from repo root, Rust 1.85)
```
$ cargo fmt --all -- --check
fmt: CLEAN (exit 0)            # no diff

$ cargo clippy --all-targets -- -D warnings
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 5.47s   # exit 0, zero warnings

$ cargo test     # aggregate across all binaries
TOTAL passed=208 failed=0
  # incl. storage_tests 25/25, retriever_tests 13/13, retrieval_quality 15/15,
  #       cli_tests 14/14, lib unittests 33/33 (4 = cli::query::parse_bm25_weights)
```
`Cargo.toml` untouched (`git diff --name-only` shows no `Cargo.toml`). `.claude/settings.json` left as
the pre-existing uncommitted hook-disable (not touched by me; out of staging).

### Built-binary spot check (real typed-error path, post-flag)
`query foo --bm25-weights ""` / `"1,2,3"` / `"1,2,3,4,5,6,7,8"` / `"a,b,c,d,e,f,g"` / `"inf,1,1,5,2,2,2"`
each → `Error: …` on stderr + exit 1, no panic. `"0,1,1,5,2,2,-1"` parses (only the absent-DB error
follows), confirming the valid negative/zero vector clears validation.

## Specialist / Perf notes (FTS5 bind-vs-format — eng-lead, rust-treesitter-specialist concern)

**(a) Can `bm25()` weights be bound as `?` params? No.** FTS5's `bm25(tbl, w1, w2, …)` weights are
*auxiliary-function arguments*, evaluated by the FTS5 extension over the full match set, not value
positions in the SQL value space. SQLite only binds `?` parameters in value contexts; an auxiliary
function's trailing weight args must be literals present at prepare time. Empirically confirmed: the
`Some` path formats them in and the reorder/edge tests pass against the real bundled FTS5; binding was
never attempted because the column list is the function's own argument syntax. So **format, don't bind**
— exactly as the brief's KEY TECHNICAL NOTE states.

**(b) Safest finite-f64 → SQL literal rendering: `format!("{w:?}")`.** Rust's `f64` `Debug` guarantees
(i) a decimal point or exponent so the token is always a numeric (`REAL`) literal — never a bareword
SQLite could misread as an identifier — and (ii) locale-independence (always `.`, never `,`). Checked
the load-bearing values: `10.0`, `1.0`, `5.0`, `0.0`, `-0.0`, `-1.0`, `2.5`, `100.0`, `0.1`, `1e10`
(→`10000000000.0`), `1e-10` (→`1e-10`) — all valid SQLite numeric literals. The one hazard is
non-finite: `{:?}` renders `f64::INFINITY`→`inf`, `NaN`→`NaN`, which are *not* valid SQL — so finiteness
is enforced **twice**: at the CLI (`parse_bm25_weights` rejects them up front) and defensively at the
storage layer (`StorageError::NonFiniteWeight`), so `inf`/`NaN` can never reach the SQL string. (Note:
`"inf"/"nan"` *do* parse via `str::parse::<f64>()` in Rust, so the explicit `is_finite()` check — not
the parse — is what blocks them; covered by `parse_bm25_weights_rejects_non_numeric_and_non_finite`.)

**(c) Do 0.0 / negative weights change FTS5 ranking validity? No — they're valid, just re-weighting.**
Per SQLite's FTS5 `bm25()` docs, a column weight of 0.0 removes that column's contribution to the score,
and negatives are accepted (they invert that column's contribution). They do **not** error and do **not**
break the ranking — the score stays a finite real and `ORDER BY score ASC, rowid ASC` is unchanged.
`search_with_weights_zero_and_negative_do_not_error` pins exactly this: `[0,1,1,5,2,2,-1]` ⇒ `Ok`, both
seeded rows still returned. (The reorder test's `[0,…]` on `symbol_name` is the same mechanism: zeroing
a column drops its boost, which is how the name-vs-body flip is engineered.)

**Perf:** the `None`/default path keeps `prepare_cached(SEARCH)` (warm, zero extra allocation vs before).
The `Some` path costs one `format!` (a ~430-byte SQL string) + a `prepare` (uncached) per weighted
query. That is the correct trade — the weighted SQL is intrinsically dynamic, so caching keyed on a
constant cannot apply; and the override is an opt-in research-sweep surface, not the hot default path
(MCP + default CLI both stay on the cached default). No change to the M6.4 query-latency budget posture.

## REVIEW — code reviewer (2026-06-14)

**VERDICT: APPROVE.** 0 blockers, 0 majors, 0 minors. All three gates clean (run independently below);
208 tests pass. The SQL-injection-safety crux, the default-identical guarantee, and the no-panic
malformed-input contract are all verified — by reading the code AND by two throwaway `rustc` checks +
a built-binary spot-check (not by trusting prior claims).

### Focus-area findings (the six things that mattered for this slice)
1. **SQL-injection safety (the crux) — SAFE.** `MATCH ?1` / `LIMIT ?2` stay bound; only the 7 weights
   are formatted in. Verified on the generated SQL: structurally identical to `SEARCH`, zero `?`
   placeholders inside `bm25(...)` (`storage/queries.rs:54-73`). `fmt_weight` (`queries.rs:81-83`) is
   `format!("{w:?}")`; I checked f64 `Debug` across the awkward range (integers, negatives, `-0.0`,
   `MIN`/`MAX`, `MIN_POSITIVE`, `EPSILON`, subnormal `5e-324`, `1e300`/`1e-300`) — every finite value
   renders with a `.` or exponent (always a valid SQLite REAL literal, never a bareword) and
   round-trips; SQLite accepts the scientific-notation forms. The ONLY hazard is non-finite
   (`inf`/`-inf`/`NaN` → barewords), and it is blocked TWICE: CLI `parse_bm25_weights` `is_finite()`
   (`cli/query.rs:51-53`) and the storage `NonFiniteWeight` guard (`storage/mod.rs:230-234`) which runs
   BEFORE `search_with_weights_sql(w)` is called (`mod.rs:235`). No raw CLI text ever reaches SQL.
2. **DEFAULT-IDENTICAL — HOLDS.** `search` → `search_with_weights(q, lim, None)` → the `None` arm
   (`storage/mod.rs:222-228`) runs `prepare_cached(queries::SEARCH)` verbatim — the unchanged const,
   same cached statement as before. `None` never enters the format path (no drift). Proven by
   `search_with_weights_none_is_identical_to_default` (full `Vec<SearchResult>` equality, incl. scores,
   across `search` / `None` / `Some(&DEFAULT_WEIGHTS)`).
3. **No reachable unwrap/expect/panic — CONFIRMED.** The only fallible parse is the single CLI site
   (`parse_bm25_weights`, `cli/query.rs:36-57`): `bail!`/`with_context` → `anyhow::Result` → nonzero
   exit; no `unwrap/expect/panic`. The `[f64; 7]` array makes arity a compile-time invariant below the
   CLI. Built-binary spot-check (all six malformed spellings + valid negative/zero): every malformed
   case exit=1, non-empty `Error:` stderr, zero "panicked"; `0,1,1,5,2,2,-1` clears validation.
4. **Determinism — PRESERVED.** Storage ranking expr is unchanged except the weight literals;
   `ORDER BY score ASC, rowid ASC` intact in the generated SQL. Retriever tie-break `(bm25 via
   total_cmp, file_path, start_byte, end_byte)` is on the unchanged post-search path. Pinned by
   `search_with_weights_custom_is_deterministic_and_bm25_ordered` (5 repeats identical + non-decreasing
   scores).
5. **Test adequacy — STRONG (real reorder, not "runs").** Storage `…_custom_reorders_ranking_vs_default`
   asserts BOTH concrete orders (`[session,helper]` default vs `[helper,session]` reorder) + `assert_ne!`
   on the same seed; retriever `bm25_weights_some_changes_ranking_vs_none` does the same through
   `Retriever::query` (1M-token budget so packing never masks the order). Default-identical is tested
   (focus 2). CLI malformed test covers arity both directions + non-numeric + empty against a REAL
   indexed fixture (so values hit the real parser, not clap's unknown-flag path), asserting
   `.failure()` + non-empty stderr + no "panicked" on either stream. `Some(&[10,1,1,5,2,2,2])`
   reproduces the default ordering (test lead's round-trip).
6. **Idiomatic / hygiene / scope — CLEAN.** clippy `-D warnings` and fmt `--check` both exit 0.
   `Cargo.toml`/`Cargo.lock` NOT in the working-tree change set (git status confirms). `.claude/settings.json`
   is the pre-existing uncommitted hook-disable, NOT staged (`git diff --cached` empty) and not part of
   this slice's logic. APIs match plan §3.2.2/§3.2.3/§7.2 exactly. `NonFiniteWeight` is an in-scope,
   brief-sanctioned additive guard. `prepare` (not `prepare_cached`) for the dynamic `Some` SQL is correct.

### Minor observations (NON-blocking, no action required — recorded for the manager)
- `fmt_weight` is `pub(self)`-effectively private (`fn`, not `pub fn`) — good; not part of the public
  surface. The brief's GREEN note said "`pub fn search_with_weights_sql`"; it is indeed `pub` (crate-
  internal, reused by `storage/mod.rs`), which is fine.
- `parse_bm25_weights` trims each field before parse, so `" 10 , 1 ,…"` is accepted (tested). That is a
  reasonable, documented ergonomic for the R2 sweep; not a correctness concern.
- The finite check lives at BOTH layers by design (defense in depth). The storage guard is unreachable
  from the CLI today, but is the correct contract for any non-CLI caller of `search_with_weights` — keep it.

### Gate output (run by the reviewer from repo root, Rust 1.85, cwd-stable)
```
$ cargo fmt --all -- --check
FMT_EXIT=0            # clean, no diff

$ cargo clippy --all-targets -- -D warnings
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 1.63s
CLIPPY_EXIT=0        # zero warnings

$ cargo test
TOTAL passed=208 failed=0   # incl. storage 25/25, retriever 13/13,
                            #       retrieval_quality 15/15, cli 14/14, lib unit 33/33
```
Independent verifications beyond the gates: (a) f64 `Debug` → SQL-literal validity across integers/
negatives/-0.0/MIN/MAX/MIN_POSITIVE/EPSILON/5e-324/1e±300 (all valid, all round-trip); (b) generated
SQL keeps `MATCH ?1`/`LIMIT ?2` bound with 0 `?` inside `bm25(...)`; (c) built-binary malformed
spot-check (empty / 3 / 8 / non-numeric / inf / NaN) all exit=1, no panic, clean `Error:`; valid
`0,1,1,5,2,2,-1` clears validation. Slice is ready to hand back for commit.

## OUTCOME — manager (2026-06-14)

`Status:` → **RED ✅  GREEN ✅  REVIEW ✅  DONE ✅** (verification-complete; staged for the
main-session commit — the manager does NOT own the commit boundary for this slice).

**Aligned — yes.** The shipped surface matches the plan amended *before* code (§3.2.2
`search_with_weights`, §3.2.3 `QueryOptions.bm25_weights: Option<[f64; 7]>`, §7.2 `--bm25-weights`).
The hard requirement (CLI-reachable per-invocation override of the 7 weights, `Cargo.toml`
untouched) is met. Default-identical is proven (full `Vec<SearchResult>` equality across
`search` / `None` / `Some(&DEFAULT_WEIGHTS)`); the injection-safety crux is double-guarded (CLI
`is_finite()` + storage `NonFiniteWeight`, both before the format) with `MATCH`/`LIMIT` still bound;
no reachable `unwrap/expect/panic`; ordering invariant preserved. The `[f64; 7]` array makes
"exactly 7 weights" a compile-time invariant so the only fallible parse is the single CLI site.

**Definition of Done — all met:** tests-first → green (208, +12); `cargo fmt --all -- --check` +
`cargo clippy --all-targets -- -D warnings` + `cargo test` clean (run independently by eng-lead AND
reviewer); reviewer **APPROVE, 0 findings**; `Cargo.toml`/`Cargo.lock` not in the change set
(reviewer-confirmed via git); `.claude/settings.json` untouched and unstaged.

**Docs reconciled (same change):**
- `docs/project_plan.md` §3.2.2/§3.2.3/§7.2 — amended pre-code (manager).
- `docs/ROADMAP.md` — **Decision Log D24** added (the per-column BM25 override surface).
- `docs/TODO.md` — R2.2 → `[~]`; **R2.2a ✅** (reviewer-APPROVED, staged); **R2.2b** added as the
  explicit next research-sweep step.
- `src/storage/CLAUDE.md`, `src/retriever/CLAUDE.md`, `src/cli/CLAUDE.md`, `docs/TEST_STRATEGY.md` —
  updated by the eng-lead/test-lead in-slice.

**Follow-ups (not blocking this slice):**
- **R2.2b** (research-harness-engineer, main session) — drive `--bm25-weights` across the
  `codecache_tool.py` process boundary to sweep ~3 weight settings (D23 ablation axis), scored by the
  R2.1 NDCG@10 / Layer-1 scorer. Pure `research/`, zero crate change.
- None outstanding on the crate side; the MCP `codecache_search` path intentionally stays
  default-weighted (out of scope; revisit only if a future tool wants per-call weights).

**Hand-back:** ready for the main session to verify + commit. The manager did not commit/push.
