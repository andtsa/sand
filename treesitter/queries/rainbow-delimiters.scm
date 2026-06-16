; Rainbow delimiters for Sand (rainbow-delimiters.nvim).
;
; Each bracket pair is captured *inside* its container node, so nesting depth is
; read from the syntax tree: `@delimiter` marks the brackets, `@container` the
; enclosing node.

; ── braces `{ }` ─────────────────────────────────────────────────────────────
(block
  "{" @delimiter
  "}" @delimiter) @container

(match_expression
  "{" @delimiter
  "}" @delimiter) @container

; ── parentheses `( )` ────────────────────────────────────────────────────────
; tuples (expression / type / pattern position)
(tuple_expr
  "(" @delimiter
  ")" @delimiter) @container
(tuple_type
  "(" @delimiter
  ")" @delimiter) @container
(tuple_pattern
  "(" @delimiter
  ")" @delimiter) @container

; function parameter list + calls
(function_definition
  "(" @delimiter
  ")" @delimiter) @container
(function_call
  "(" @delimiter
  ")" @delimiter) @container
(external_function_call
  "(" @delimiter
  ")" @delimiter) @container

; (the lambda parameter list `fn (x: T)` is omitted: tree-sitter rejects
;  `(lambda_expr "(" …)` as an "impossible pattern" — a static-analysis quirk of
;  the `(` position after `fn`.)

; constructor / tag payloads (expression position)
(constructor_expr
  "(" @delimiter
  ")" @delimiter) @container
(external_constructor_expr
  "(" @delimiter
  ")" @delimiter) @container
(tag_expr
  "(" @delimiter
  ")" @delimiter) @container

; constructor / tag payloads (pattern position)
(constructor_pattern
  "(" @delimiter
  ")" @delimiter) @container
(tag_pattern
  "(" @delimiter
  ")" @delimiter) @container

; enum variant payload type
(enum_variant
  "(" @delimiter
  ")" @delimiter) @container
