; ========= Scopes =========
; Function bodies and blocks create local variable scopes
(function_definition) @scope
(block) @scope
; Each match arm body is its own scope
(match_arm) @scope

; ========= Definitions =========
(function_definition
  name: (identifier) @definition)

(parameter
  name: (identifier) @definition)

(declaration
  name: (identifier) @definition)

(assignment
  name: (identifier) @definition)

; Type alias names are type-level definitions
(type_alias
  name: (identifier) @definition.type)

; ========= References =========
; Any identifier not captured above is a reference
(identifier) @reference
