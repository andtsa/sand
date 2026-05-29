; Indent inside any block node
(block) @indent.begin

; Indent inside match braces
(match_expression) @indent.begin

; Indent inside parameter lists
(parameters) @indent.begin
(parameters) @indent.end

; Align closing brackets
[
  "}"
  ")"
] @indent.branch

("}") @indent.end
(")") @indent.end
