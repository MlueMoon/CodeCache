//! Retriever: FTS5 BM25 search, snippet extraction, token counting, greedy token-budget packing.
//!
//! API anchor: `project_plan.md` §3.2.3 / §6. Kept behind a trait so a `HybridRetriever` can
//! wrap it in v0.2 (Decision Log D1). Owner: `principal-engineering-lead`. Scenarios:
//! `docs/TEST_STRATEGY.md#retriever`. M0: empty stub.
//!
//! **M6.1 (query preprocessing)** ships only the two module-private, dependency-free string
//! functions that the M6.2 `Retriever::query` will call: [`preprocess_query`] (tokenize →
//! lowercase → drop stopwords → FTS5-escape) and [`build_match_expression`] (` OR `-join into a
//! valid FTS5 `MATCH` string, §6.1). The `Retriever` struct + `trait Retrieve` (Decision Log D1)
//! land in **M6.2**, driven by the `query`/`new` tests — adding them now (with no `Storage` and no
//! `query` body) would be undriven production surface (TDD). See the M6.1 brief GREEN note.

/// Stopwords dropped during preprocessing (§6.1). Deliberately **small and code-search-oriented**:
/// only the few natural-language filler words an agent prefixes a query with (e.g. "find the
/// user", "show me how X works") carry no FTS5 signal and only dilute BM25. We do **not** strip
/// programming keywords (`if`, `for`, `class`, `type`, …) — those are often exactly what a code
/// query targets. Lowercase; matched after the token itself is lowercased.
//
// `dead_code`-allowed for this slice only: M6.1 ships preprocessing ahead of `Retriever::query`,
// which consumes these in M6.2 (the brief plans the type/query to land next). Tests exercise them
// now; the attributes come off when `query` calls them. See `src/retriever/CLAUDE.md`.
#[allow(dead_code)]
const STOPWORDS: &[&str] = &[
    "a", "an", "and", "the", "of", "to", "in", "is", "it", "for", "on", "with", "find", "show",
    "me", "how", "where", "what", "that", "this", "get",
];

/// Preprocess a raw user query into a normalized, FTS5-safe token list (§3.2.3 / §6.1).
///
/// Pipeline: split into tokens (maximal runs of alphanumeric / `_` / `"`; every other char —
/// whitespace, `()`, `:`, `-`, … — separates) → lowercase (Unicode-aware, never slices a char
/// boundary) → drop [`STOPWORDS`] → FTS5-escape each survivor. A safe ASCII bareword
/// (alphanumeric/`_` only) is left unquoted; any other token (non-ASCII like `café`, or one
/// carrying a `"`) is wrapped as an FTS5 string literal with internal `"` doubled, so the joined
/// expression is always syntactically valid. An empty or all-stopword query yields `[]` — the
/// caller maps that to an empty result downstream (never `MATCH ""`). Deterministic; total.
#[allow(dead_code)] // consumed by `Retriever::query` in M6.2; exercised by tests now.
fn preprocess_query(query: &str) -> Vec<String> {
    query
        .split(|c: char| !(c.is_alphanumeric() || c == '_' || c == '"'))
        .filter(|tok| !tok.is_empty())
        .map(str::to_lowercase)
        .filter(|tok| !STOPWORDS.contains(&tok.as_str()))
        .map(|tok| escape_fts5_token(&tok))
        .collect()
}

/// Join already-escaped tokens into an FTS5 `MATCH` expression with ` OR ` (§6.1).
///
/// An empty token slice yields `""` — the caller treats that as "no query" and returns an empty,
/// well-formed result rather than running `MATCH ""` (which FTS5 rejects).
#[allow(dead_code)] // consumed by `Retriever::query` in M6.2; exercised by tests now.
fn build_match_expression(tokens: &[String]) -> String {
    tokens.join(" OR ")
}

/// Escape one (already lowercased, non-empty) token for safe inclusion in an FTS5 `MATCH`
/// expression. A token that is a plain ASCII bareword (alphanumeric / `_`) is returned as-is;
/// anything else is wrapped in double quotes with internal `"` doubled, producing a valid FTS5
/// string literal that can never introduce a syntax error.
#[allow(dead_code)] // consumed by `Retriever::query` in M6.2; exercised by tests now.
fn escape_fts5_token(token: &str) -> String {
    let is_safe_bareword = token.chars().all(|c| c.is_ascii_alphanumeric() || c == '_');
    if is_safe_bareword {
        token.to_string()
    } else {
        format!("\"{}\"", token.replace('"', "\"\""))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── M6.1: query preprocessing (tokenize, lowercase, stopwords, FTS5 escaping) ──────────────

    #[test]
    fn preprocess_tokenizes_and_lowercases() {
        // §6.1: "Authenticate User" → ["authenticate", "user"]
        assert_eq!(
            preprocess_query("Authenticate User"),
            vec!["authenticate".to_string(), "user".to_string()]
        );
    }

    #[test]
    fn preprocess_builds_or_match_expression() {
        // §6.1: tokens join with " OR " into the FTS5 MATCH string.
        let tokens = preprocess_query("authenticate user");
        assert_eq!(build_match_expression(&tokens), "authenticate OR user");
    }

    #[test]
    fn preprocess_removes_stopwords() {
        // "find the user" → stopwords `find`/`the` dropped → ["user"].
        assert_eq!(preprocess_query("find the user"), vec!["user".to_string()]);
    }

    #[test]
    fn empty_query_after_stopword_removal_handled() {
        // Empty input and all-stopword input both degrade to an empty token list — no panic,
        // and the MATCH expression is empty (downstream M6.2 maps this to an empty QueryResult,
        // never `MATCH ""`).
        assert_eq!(preprocess_query(""), Vec::<String>::new());
        assert_eq!(preprocess_query("   "), Vec::<String>::new());
        assert_eq!(preprocess_query("find the"), Vec::<String>::new());

        let empty: Vec<String> = Vec::new();
        assert_eq!(build_match_expression(&empty), "");
    }

    #[test]
    fn preprocess_escapes_fts5_special_chars() {
        // FTS5 safety: special chars (parens, colon, quote) must not produce a MATCH syntax
        // error. A safe ASCII bareword stays unquoted; non-bareword tokens are wrapped as an FTS5
        // string literal with internal double-quotes doubled, so the joined expression is valid.
        // `foo()` → the `()` are separators; only the safe bareword `foo` survives.
        let tokens = preprocess_query("foo()");
        assert_eq!(tokens, vec!["foo".to_string()]);
        assert_eq!(build_match_expression(&tokens), "foo");

        // `user:name` → two safe barewords (colon is a separator) → unquoted, OR-joined.
        let tokens = preprocess_query("user:name");
        assert_eq!(build_match_expression(&tokens), "user OR name");

        // An embedded double-quote is the one in-token special char: it is kept and escaped by
        // doubling it inside the literal, so the expression stays balanced/valid (no dangling
        // quote → no FTS5 syntax error).
        let tokens = preprocess_query("sa\"y");
        assert_eq!(tokens, vec!["\"sa\"\"y\"".to_string()]);
    }

    #[test]
    fn preprocess_is_deterministic() {
        // Same input ⇒ identical token order/output across repeated calls.
        let input = "Parse the Config and Validate User Input";
        let first = preprocess_query(input);
        for _ in 0..5 {
            assert_eq!(preprocess_query(input), first);
        }
    }

    #[test]
    fn preprocess_handles_utf8_multibyte() {
        // Multibyte identifiers survive lowercasing without slicing a char boundary (no panic).
        assert_eq!(preprocess_query("Café"), vec!["\"café\"".to_string()]);
        assert_eq!(preprocess_query("Naïve"), vec!["\"naïve\"".to_string()]);
        // An identifier already lowercase + ASCII stays a bareword (no needless quoting).
        assert_eq!(preprocess_query("naive"), vec!["naive".to_string()]);
    }
}
