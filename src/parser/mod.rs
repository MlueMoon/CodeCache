//! Parser: Tree-sitter integration — load grammars, run `.scm` queries, extract AST nodes with
//! exact byte spans; ERROR-node detection (graceful degradation, Decision Log D2).
//!
//! API anchor: `project_plan.md` §3.2.1 / §5.3. Owner: `principal-engineering-lead` +
//! `rust-treesitter-specialist`. Scenarios: `docs/TEST_STRATEGY.md#parser-python--ts--go`.
//! M3 ships Python; TS/Go land at M9.
//!
//! ## Extraction model (M3)
//! [`Parser::parse_file`] returns a raw `tree_sitter::Tree`; [`Parser::extract_chunks`] walks it
//! with a `TreeCursor` and emits one [`Chunk`] per function/class/method with a **byte-exact**
//! span (`&source[start_byte..end_byte] == chunk_text`, UTF-8-boundary correct, CRLF preserved).
//!
//! Two pinned specialist decisions (see `BRIEF-M3-parser-python.md`):
//! - **Decorator inclusion:** when a `function_definition`/`class_definition` is wrapped in a
//!   `decorated_definition`, the chunk span is taken from the *wrapper* so the `@decorator` lines
//!   are inside the span.
//! - **Method vs function:** a `function_definition` whose nearest *definition* ancestor is a
//!   `class_definition` is a [`SymbolType::Method`] with `parent_symbol = <class name>`; a
//!   function nested in another function stays a [`SymbolType::Function`] with
//!   `parent_symbol = <enclosing fn name>`.
//!
//! ## Degradation seam (D2)
//! [`error_rate`] reports the `(ERROR + MISSING) / named-node` fraction and [`should_fall_back`]
//! compares it to [`HEURISTIC_FALLBACK_THRESHOLD`]. M3 only *reports*; the actual heuristic/regex
//! chunker fallback (and the `heuristic` chunk flag) is owned by the M4 chunker. `parse_file` and
//! `extract_chunks` never panic on malformed input — they return `Ok` (possibly empty).

use std::collections::HashMap;
use std::fmt;
use std::path::{Path, PathBuf};

use tree_sitter::{Node, Query, Tree};

use crate::types::{Chunk, Language, SymbolType};

mod python;

/// [`error_rate`] value at or above which a file should be routed to the M4 heuristic chunker
/// instead of trusting the AST (Decision Log D2). ~20% of named nodes broken.
pub const HEURISTIC_FALLBACK_THRESHOLD: f32 = 0.20;

/// Per-language Tree-sitter configuration: the compiled grammar plus the `.scm` extraction
/// queries (`project_plan.md` §3.2.1 / §5.3).
pub struct LanguageConfig {
    /// The Tree-sitter grammar for this language.
    grammar: tree_sitter::Language,
    /// The `.scm` extraction queries (function/class/method), validated in [`Parser::new`].
    queries: &'static str,
}

/// Errors the parser can surface. Implements [`std::error::Error`] with a real [`Error::source`]
/// so callers can introspect the underlying Tree-sitter failure without us reaching for
/// `unwrap`/`expect`/`panic!` on any library path.
#[derive(Debug)]
pub enum ParserError {
    /// The requested [`Language`] has no wired grammar yet (e.g. Go/TS at M3).
    UnsupportedLanguage(Language),
    /// Applying a grammar to the Tree-sitter parser failed.
    Language(tree_sitter::LanguageError),
    /// One of the embedded `.scm` queries failed to compile against the grammar.
    Query(tree_sitter::QueryError),
    /// Tree-sitter returned no tree for the given input (it never does for in-memory UTF-8, but
    /// we model it rather than `unwrap` the `Option`).
    ParseFailed {
        /// The file whose parse produced no tree.
        path: PathBuf,
    },
}

impl fmt::Display for ParserError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ParserError::UnsupportedLanguage(lang) => {
                write!(f, "unsupported language for parsing: {}", lang.as_str())
            }
            ParserError::Language(_) => write!(f, "failed to set Tree-sitter language"),
            ParserError::Query(_) => write!(f, "failed to compile Tree-sitter query"),
            ParserError::ParseFailed { path } => {
                write!(f, "Tree-sitter produced no tree for {}", path.display())
            }
        }
    }
}

impl std::error::Error for ParserError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            ParserError::Language(e) => Some(e),
            ParserError::Query(e) => Some(e),
            ParserError::UnsupportedLanguage(_) | ParserError::ParseFailed { .. } => None,
        }
    }
}

impl From<tree_sitter::LanguageError> for ParserError {
    fn from(e: tree_sitter::LanguageError) -> Self {
        ParserError::Language(e)
    }
}

impl From<tree_sitter::QueryError> for ParserError {
    fn from(e: tree_sitter::QueryError) -> Self {
        ParserError::Query(e)
    }
}

/// Crate-local result alias for parser operations.
type Result<T> = std::result::Result<T, ParserError>;

/// Tree-sitter front-end: holds one reusable `tree_sitter::Parser` and the per-language configs.
///
/// `parse_file` swaps the active grammar; `extract_chunks` is read-only over a produced tree, so
/// the borrow checker lets a caller hold a `&Tree` while extracting. See `project_plan.md` §3.2.1.
pub struct Parser {
    ts_parser: tree_sitter::Parser,
    language_configs: HashMap<Language, LanguageConfig>,
}

impl Parser {
    /// Build a parser with the Python grammar wired (M3). TS/Go are added at M9.
    ///
    /// The embedded `.scm` queries are compiled here so a malformed query is a construction-time
    /// error rather than a per-file surprise.
    pub fn new() -> Result<Self> {
        let mut language_configs = HashMap::new();

        let py = python::config();
        // Validate the queries against the grammar up front (proves capture/node names match).
        Query::new(&py.grammar, py.queries)?;
        language_configs.insert(Language::Python, py);

        Ok(Self {
            ts_parser: tree_sitter::Parser::new(),
            language_configs,
        })
    }

    /// Parse `content` for `lang` into a Tree-sitter tree.
    ///
    /// Returns [`ParserError::UnsupportedLanguage`] for a language without a wired grammar.
    /// Malformed input still parses (to an error-laden tree) and never panics.
    pub fn parse_file(&mut self, path: &Path, content: &str, lang: Language) -> Result<Tree> {
        let config = self
            .language_configs
            .get(&lang)
            .ok_or(ParserError::UnsupportedLanguage(lang))?;
        self.ts_parser.set_language(&config.grammar)?;
        self.ts_parser
            .parse(content, None)
            .ok_or_else(|| ParserError::ParseFailed {
                path: path.to_path_buf(),
            })
    }

    /// Extract function/class/method chunks from `tree` over its `source`, ordered by
    /// `start_byte` (deterministic). Returns `Ok(vec)` even for empty or malformed trees.
    pub fn extract_chunks(&self, tree: &Tree, source: &str, lang: Language) -> Result<Vec<Chunk>> {
        // We only know how to interpret a tree for a language we can parse.
        if !self.language_configs.contains_key(&lang) {
            return Err(ParserError::UnsupportedLanguage(lang));
        }

        let file_path = PathBuf::new();
        let mut chunks = Vec::new();
        let root = tree.root_node();
        // Recurse from the root; `parent_symbol` is threaded down the definition stack.
        collect_chunks(root, source, lang, &file_path, None, &mut chunks);

        chunks.sort_by_key(|c| c.start_byte);
        Ok(chunks)
    }
}

/// Recursively walk `node`, emitting a [`Chunk`] for each `function_definition` / `class_definition`.
///
/// `parent` is the name of the nearest enclosing *definition* (class or function), used both for
/// `parent_symbol` and to decide Method vs Function. Decorated defs are spanned from their
/// `decorated_definition` wrapper so the `@decorator` lines are included.
fn collect_chunks(
    node: Node,
    source: &str,
    lang: Language,
    file_path: &Path,
    parent: Option<&str>,
    out: &mut Vec<Chunk>,
) {
    // Iterate children; recurse with an updated `parent` when we enter a definition.
    let mut walk = node.walk();
    let children: Vec<Node> = node.children(&mut walk).collect();
    for child in children {
        match child.kind() {
            "function_definition" => {
                if let Some(name) = field_text(child, "name", source) {
                    let symbol_type = if parent_is_class(child) {
                        SymbolType::Method
                    } else {
                        SymbolType::Function
                    };
                    // Span source: the decorated wrapper if present, else the def itself.
                    let span_node = span_node_for(child);
                    if let Some(chunk) = build_chunk(
                        span_node,
                        name,
                        symbol_type,
                        lang,
                        file_path,
                        parent,
                        source,
                    ) {
                        out.push(chunk);
                    }
                    // Recurse into the body; this def's name becomes the children's parent.
                    collect_chunks(child, source, lang, file_path, Some(name), out);
                } else {
                    collect_chunks(child, source, lang, file_path, parent, out);
                }
            }
            "class_definition" => {
                if let Some(name) = field_text(child, "name", source) {
                    let span_node = span_node_for(child);
                    if let Some(chunk) = build_chunk(
                        span_node,
                        name,
                        SymbolType::Class,
                        lang,
                        file_path,
                        parent,
                        source,
                    ) {
                        out.push(chunk);
                    }
                    collect_chunks(child, source, lang, file_path, Some(name), out);
                } else {
                    collect_chunks(child, source, lang, file_path, parent, out);
                }
            }
            // Any other node (module, decorated_definition wrapper, blocks, statements, ERROR …):
            // recurse without changing the parent context so nested defs are still found.
            _ => {
                collect_chunks(child, source, lang, file_path, parent, out);
            }
        }
    }
}

/// The node whose byte span the chunk should use: the enclosing `decorated_definition` (so the
/// `@decorator` lines are included) when present, otherwise the definition node itself.
fn span_node_for(def: Node) -> Node {
    match def.parent() {
        Some(p) if p.kind() == "decorated_definition" => p,
        _ => def,
    }
}

/// True if `def`'s nearest enclosing *definition* is a `class_definition` (⇒ Method), as opposed
/// to a `function_definition` (⇒ nested Function) or module level. We climb past structural nodes
/// (`block`, `decorated_definition`, ERROR, …) and stop at the first definition ancestor.
fn parent_is_class(def: Node) -> bool {
    let mut cur = def.parent();
    while let Some(node) = cur {
        match node.kind() {
            "class_definition" => return true,
            "function_definition" => return false,
            _ => cur = node.parent(),
        }
    }
    false
}

/// Read the UTF-8 text of `node`'s `field` child, if present and valid UTF-8.
fn field_text<'a>(node: Node, field: &str, source: &'a str) -> Option<&'a str> {
    let child = node.child_by_field_name(field)?;
    node_text(child, source)
}

/// Slice `node`'s exact bytes out of `source` (byte-exact, UTF-8-boundary safe).
fn node_text<'a>(node: Node, source: &'a str) -> Option<&'a str> {
    source.get(node.start_byte()..node.end_byte())
}

/// Build a [`Chunk`] from a span node (already chosen to include decorators where relevant).
///
/// Returns `None` only if the span isn't a valid UTF-8 slice of `source` (e.g. truncated/broken
/// tree) — i.e. we drop a degenerate chunk rather than emit one that violates the span invariant.
fn build_chunk(
    span_node: Node,
    name: &str,
    symbol_type: SymbolType,
    lang: Language,
    file_path: &Path,
    parent: Option<&str>,
    source: &str,
) -> Option<Chunk> {
    let start_byte = span_node.start_byte();
    // Tree-sitter ends a definition at the last *content* byte, before the trailing newline. We
    // extend the span to include the single line terminator that closes the def's last line so a
    // chunk reads as a whole, self-contained source block (and CRLF `\r\n` is preserved verbatim).
    let end_byte = extend_to_line_end(source, span_node.end_byte());
    let text = source.get(start_byte..end_byte)?;
    Some(Chunk {
        symbol_name: name.to_string(),
        symbol_type,
        file_path: file_path.to_path_buf(),
        start_byte,
        end_byte,
        // Tree-sitter rows are 0-based; D7 line numbers are 1-based inclusive. The trailing
        // newline we appended belongs to the def's last content line, so the line range is
        // unchanged by the byte extension.
        start_line: span_node.start_position().row + 1,
        end_line: span_node.end_position().row + 1,
        chunk_text: text.to_string(),
        language: lang,
        parent_symbol: parent.map(str::to_string),
        // Enrichment (D3) beyond parent_symbol is filled by the M4 chunker.
        file_docstring: None,
        imports: Vec::new(),
        cross_references: Vec::new(),
    })
}

/// Extend `end` (a byte offset on a UTF-8 / char boundary) to include the line terminator that
/// immediately follows it: a lone `\n`, or a `\r\n` pair (CRLF preserved). If `end` is not at a
/// line break (e.g. EOF without a trailing newline), it is returned unchanged. Operating on raw
/// bytes here is safe because `\r` (0x0D) and `\n` (0x0A) are single-byte ASCII and never appear
/// inside a multibyte UTF-8 sequence.
fn extend_to_line_end(source: &str, end: usize) -> usize {
    let bytes = source.as_bytes();
    match bytes.get(end) {
        Some(b'\n') => end + 1,
        Some(b'\r') if bytes.get(end + 1) == Some(&b'\n') => end + 2,
        _ => end,
    }
}

/// Syntactic error density of `tree` in `[0, 1]` (Decision Log D2): the count of `ERROR` +
/// `MISSING` nodes over the count of **named** nodes.
///
/// Rationale for the *named-node* denominator: tree-sitter materializes every literal token
/// (`(`, `)`, `:`, `+`, `def`, …) as an anonymous node. Those carry no independent syntactic
/// meaning, and including them in the denominator dilutes the error signal so heavily that even a
/// file that is almost entirely garbage scores well under any sane fallback threshold. Named nodes
/// are the meaningful syntactic units, so the ratio "broken units / meaningful units" is the
/// honest measure of how much of the file tree-sitter could not understand. The numerator counts
/// every `ERROR`/`MISSING` node (named or not, so a single `MISSING` anonymous token — e.g. an
/// unclosed paren — still yields a positive rate). The result is clamped into `[0, 1]`.
///
/// `error_rate(valid) == 0.0`; a malformed file reports `> 0.0`. Walks every node once via a
/// `TreeCursor` (no recursion-depth limit, no per-node allocation, never panics).
pub fn error_rate(tree: &Tree) -> f32 {
    let mut named: u64 = 0;
    let mut bad: u64 = 0;

    let mut cursor = tree.walk();
    loop {
        let node = cursor.node();
        if node.is_named() {
            named += 1;
        }
        if node.is_error() || node.is_missing() {
            bad += 1;
        }

        // Depth-first traversal without recursion.
        if cursor.goto_first_child() {
            continue;
        }
        loop {
            if cursor.goto_next_sibling() {
                break;
            }
            if !cursor.goto_parent() {
                // Back at the root with no more siblings: done.
                return ratio(bad, named);
            }
        }
    }
}

/// `bad / named` clamped into `[0, 1]`; `0.0` when there are no named nodes (defensive — a real
/// tree always has at least the named `module`/`source_file` root).
fn ratio(bad: u64, named: u64) -> f32 {
    if named == 0 {
        return 0.0;
    }
    (bad as f32 / named as f32).clamp(0.0, 1.0)
}

/// Whether a file with the given ERROR `rate` should be routed to the M4 heuristic chunker (D2).
/// `should_fall_back(0.0) == false`; true at or above [`HEURISTIC_FALLBACK_THRESHOLD`].
pub fn should_fall_back(rate: f32) -> bool {
    rate >= HEURISTIC_FALLBACK_THRESHOLD
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn threshold_is_a_sane_fraction() {
        assert!((0.0..1.0).contains(&HEURISTIC_FALLBACK_THRESHOLD));
    }

    #[test]
    fn should_fall_back_respects_threshold() {
        assert!(!should_fall_back(0.0));
        assert!(should_fall_back(HEURISTIC_FALLBACK_THRESHOLD));
        assert!(should_fall_back(1.0));
    }

    #[test]
    fn queries_compile_against_grammar() {
        // `Parser::new` validates the embedded `.scm`; surfacing it as a test documents the seam.
        assert!(Parser::new().is_ok());
    }
}
