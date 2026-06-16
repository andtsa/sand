; ========= Scopes =========
; Function bodies and blocks create local variable scopes
(function_definition) @scope
(block) @scope
; Each match arm body is its own scope
(match_arm) @scope
; A lambda body is its own scope, binding the lambda parameter
(lambda_expr) @scope

; ========= Definitions =========
(function_definition
  name: (identifier) @definition)

(parameter
  name: (identifier) @definition)

(lambda_param
  param: (identifier) @definition)

(declaration
  name: (identifier) @definition)

(assignment
  target: (identifier) @definition)

; Pattern bindings (in match arms and destructuring)
(binding_pattern) @definition

; Type alias + typeclass names are type-level definitions
(type_alias
  name: (identifier) @definition.type)

(typeclass_declaration
  name: (identifier) @definition.type)

; Generic type parameters
(type_param
  name: (identifier) @definition)

; ========= References =========
; Any identifier not captured above is a reference
(identifier) @reference
