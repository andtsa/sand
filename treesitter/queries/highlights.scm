; ========= Comments =========
(comment) @comment

; ========= Literals =========
(number) @number
(boolean) @boolean
(lifetime) @label

; ========= Identifiers =========
(identifier) @variable

(parameter
  name: (identifier) @variable.parameter)

(declaration
  name: (identifier) @variable.definition)

(assignment
  target: (identifier) @variable.assignment)

(function_definition
  name: (identifier) @function)

(typeclass_method
  name: (identifier) @function)

(function_call
  function: (identifier) @function.call)

(external_function_call
  module: (identifier) @module
  function: (identifier) @function.call)

(module_declaration
  name: (identifier) @module)

(use_path (identifier) @module)
(use_glob) @character.special

; ========= Type parameters / generics =========
(type_param
  name: (identifier) @type.parameter)
(variance_ann) @operator
(kind_atom) @type.builtin

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

; Generic instantiation: Option<Int>
(type_application
  name: (identifier) @type)

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
  variant: (enum_variant
    name: (identifier) @constructor))

(deriving_clause
  class: (identifier) @type)

; ========= Typeclasses & impls =========
(typeclass_declaration
  name: (identifier) @type.definition)

(requires_clause
  class: (identifier) @type)

(impl_declaration
  class: (identifier) @type)

(where_constraint
  class: (identifier) @type)

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

; let E#V(...) = ...
(let_constructor
  type_name: (identifier) @type
  variant: (identifier) @constructor)

(int_literal_pattern) @number
(bool_literal_pattern) @boolean

; ========= Binding patterns =========
(binding_pattern) @variable.definition

; ========= Lambdas =========
; fn (x: T) -> e
(lambda_param
  param: (identifier) @variable.parameter)

; Calling-mode of a function arrow: Owned | BorrowedMut | Borrowed
(arrow_kind) @type.builtin

; The function arrow itself (`->` or `-[k]>`)
(fn_arrow) @operator

; ========= Keywords =========
[
  "if"
  "then"
  "else"
  "while"
  "do"
  "let"
  "def"
  "fn"
  "module"
  "use"
  "type"
  "match"
  "mut"
  "extern"
  "typeclass"
  "impl"
  "for"
  "requires"
  "where"
  "deriving"
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
  "and"
  "or"
  "xor"
  "&&"
  "⊕"
  "&"
  "@"
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
  "|"
] @punctuation.delimiter
