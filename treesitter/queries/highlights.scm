; ========= Comments =========
(comment) @comment

; ========= Literals =========
(number) @number
(boolean) @boolean

; ========= Identifiers =========
(identifier) @variable

(parameter
  name: (identifier) @variable.parameter)

(declaration
  name: (identifier) @variable.definition)

(assignment
  name: (identifier) @variable.assignment)

(function_definition
  name: (identifier) @function)

(function_call
  function: (identifier) @function.call)

(external_function_call
  module: (identifier) @module
  function: (identifier) @function.call)

(module_declaration
  name: (identifier) @module)

; ========= Types =========
; Built-in primitive types
"Int" @type.builtin
"Bool" @type.builtin
"Unit" @type.builtin

; Named enum type used as a type annotation
(function_definition
  return_type: (identifier) @type)

(parameter
  type: (identifier) @type)

(declaration
  type: (identifier) @type)

; Qualified type: mod::TypeName
(qualified_type
  module: (identifier) @module
  name: (identifier) @type)

; Ad-hoc tag union type: #ok | #err
(tag_type
  tag: (identifier) @type.tag)

; ========= Enum type declarations =========
(type_alias
  name: (identifier) @type.definition
  variant: (identifier) @constructor)

; ========= Constructors =========
; Light#Red
(constructor_expr
  type_name: (identifier) @type
  variant: (identifier) @constructor)

; mod::Light#Red
(external_constructor_expr
  module: (identifier) @module
  type_name: (identifier) @type
  variant: (identifier) @constructor)

; #Red (bare tag)
(tag_expr
  variant: (identifier) @constructor)

; ========= Match patterns =========
; Light#Red  (constructor pattern)
(constructor_pattern
  type_name: (identifier) @type
  variant: (identifier) @constructor)

; #gt  (bare tag pattern)
(tag_pattern
  tag: (identifier) @constructor)

; _  (wildcard)
(wildcard_pattern) @variable.special

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
  "type"
  "match"
  "mut"
] @keyword

"=>" @punctuation.special

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
  "¡"
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
  "#"
  "::"
] @punctuation.delimiter
