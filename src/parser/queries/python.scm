; Python extraction queries (project_plan.md §5.3).
;
; These S-expression queries are the documented contract for what the Python
; `LanguageConfig` extracts. They are compiled and validated against the grammar
; in `Parser::new` (a malformed query is a construction-time error), which proves
; the capture names below match the tree-sitter-python grammar node kinds.
;
; NOTE (M3 implementation seam): extraction itself walks the tree with a
; `TreeCursor` rather than driving these queries through `QueryCursor`. The walk
; gives direct ancestor access, which is what the two pinned specialist decisions
; need: (1) wrapping a decorated def in its `decorated_definition` parent so the
; `@decorator` lines are inside the span, and (2) classifying a `function_definition`
; as a Method when its nearest definition ancestor is a `class_definition`. Driving
; `QueryCursor::matches` in tree-sitter 0.24 additionally requires the external
; `streaming-iterator` crate, which we deliberately avoid (keep Cargo.toml lean).
; The queries are kept here, validated, and ready for richer query-driven
; enrichment (D3) in M4.

; Function / method definitions (name + params + body).
(function_definition
  name: (identifier) @function.name
  parameters: (parameters) @function.params
  body: (block) @function.body) @function.definition

; Class definitions (name + body block).
(class_definition
  name: (identifier) @class.name
  body: (block) @class.body) @class.definition

; A decorated definition spans its `@decorator` lines together with the inner
; function/class definition (the `decorated_definition` node).
(decorated_definition) @decorated.definition
