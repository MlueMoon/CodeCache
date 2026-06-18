//! Python `LanguageConfig`: the tree-sitter-python grammar plus the `.scm`
//! extraction queries (project_plan.md §5.3).
//!
//! The query strings are embedded from `queries/python.scm` and validated against
//! the grammar in [`super::Parser::new`]; see that file's header for why M3 walks
//! the tree directly rather than driving the queries through `QueryCursor`.

use super::LanguageConfig;

/// The Python extraction queries (function/class/method + decorated defs), §5.3.
pub const PYTHON_QUERIES: &str = include_str!("queries/python.scm");

/// Build the Python [`LanguageConfig`]: tree-sitter-python grammar + queries.
pub fn config() -> LanguageConfig {
    LanguageConfig {
        grammar: tree_sitter_python::LANGUAGE.into(),
        queries: PYTHON_QUERIES,
    }
}
