; Scopes: function bodies and blocks create local scope
(function_definition) @scope
(block) @scope

; Definitions: function name, parameter names, let/declaration names, assignment targets
(function_definition
  name: (identifier) @definition)

(parameter
  name: (identifier) @definition)

(declaration
  name: (identifier) @definition)

(assignment
  name: (identifier) @definition)

; References: any identifier that's not a definition can be considered a usage
(identifier) @reference
