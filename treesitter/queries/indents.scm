
; Indent inside any block node 
(block) @indent.begin

; Indent inside parameter lists
(parameters) @indent.begin
(parameters) @indent.end

; Align closing brackets 
; (if punctuation tokens exist as leaves)
[
  "}"
  ")"
] @indent.branch

("}") @indent.end
(")") @indent.end

