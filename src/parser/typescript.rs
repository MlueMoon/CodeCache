//! TypeScript `LanguageConfig`: the tree-sitter-typescript grammar plus the
//! `.scm` extraction queries (project_plan.md §5.3).
//!
//! The query strings are embedded from `queries/typescript.scm` and validated
//! against the grammar in [`super::Parser::new`]; see that file's header for why
//! extraction walks the tree directly rather than driving the queries through
//! `QueryCursor`.
//!
//! v0.1 loads the **TypeScript** grammar (`LANGUAGE_TYPESCRIPT`); `.tsx`/JSX
//! discovery is deferred (manager decision, see `src/parser/CLAUDE.md`).

use super::LanguageConfig;

/// The TypeScript extraction queries (function/arrow/class/method), §5.3.
pub const TYPESCRIPT_QUERIES: &str = include_str!("queries/typescript.scm");

/// Build the TypeScript [`LanguageConfig`]: tree-sitter-typescript grammar + queries.
pub fn config() -> LanguageConfig {
    LanguageConfig {
        grammar: tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into(),
        queries: TYPESCRIPT_QUERIES,
    }
}
