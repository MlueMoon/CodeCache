; TypeScript extraction queries (project_plan.md §5.3).
;
; These S-expression queries are the documented contract for what the TypeScript
; `LanguageConfig` extracts. They are compiled and validated against the grammar
; in `Parser::new` (a malformed query is a construction-time error), which proves
; the capture/field/node names below match the tree-sitter-typescript grammar.
;
; NOTE (extraction seam): like Python, extraction walks the tree with a
; `TreeCursor` rather than driving these queries through `QueryCursor`. The walk
; gives direct ancestor access, which is what method classification needs (a
; `method_definition` whose nearest definition ancestor is a `class_declaration`
; is a Method with `parent_symbol = <class name>`), and avoids the external
; `streaming-iterator` crate that tree-sitter 0.24's `QueryCursor::matches`
; requires. The queries are kept here, validated, and ready for richer
; query-driven enrichment (D3) in M4.

; Function declarations (name + params + body).
(function_declaration
  name: (identifier) @function.name
  parameters: (formal_parameters) @function.params
  body: (statement_block) @function.body) @function.definition

; Arrow functions assigned to a variable: the chunk is named by the declarator
; identifier and spans the `variable_declarator` (name + arrow value).
(variable_declarator
  name: (identifier) @function.name
  value: (arrow_function
    parameters: (_) @function.params
    body: (_) @function.body)) @function.definition

; Class declarations (name + body).
(class_declaration
  name: (type_identifier) @class.name
  body: (class_body) @class.body) @class.definition

; Methods (name + params + body); typed as Method when inside a class_declaration.
(method_definition
  name: (property_identifier) @method.name
  parameters: (formal_parameters) @method.params
  body: (statement_block) @method.body) @method.definition
