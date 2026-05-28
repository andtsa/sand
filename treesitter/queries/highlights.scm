; ========= Comments =========
(comment) @comment
; ========= Literals =========
(number) @number
(boolean) @boolean
; ========= Identifiers =========
(identifier) @variable

(parameter
  name: (identifier) @parameter)

(declaration
  name: (identifier) @variable.definition)

(assignment
  name: (identifier) @variable.assignment)

(function_definition
  name: (identifier) @function)

(function_call
  function: (identifier) @function.call)

(module_declaration
  name: (identifier) @module)

; ========= Keywords =========
[
  "if"
  "then"
  "else"
  "while"
  "do"
  "let"
  "def"
  "module"
] @keyword

; ========= Types =========
(type) @type

; ========= Operators =========
[
  "+"
  "-"
  "*"
  "/"
  "^"
  "!"
  "&"
  "|"
  "⊕"
  "#"
  ">"
  "<"
  ">="
  "<="
  "≥"
  "≤"
  "=="
  "!="
  "≠"
] @operator

; ========= Punctuation =========
[
  "("
  ")"
  "{"
  "}"
  ","
  ";"
  ":"
  ":="
] @punctuation.delimiter

