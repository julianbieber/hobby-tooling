; Calls — helix's rust tags.scm captures no references.
(call_expression
  function: (identifier) @name) @reference.call

(call_expression
  function: (scoped_identifier
    name: (identifier) @name)) @reference.call

(call_expression
  function: (field_expression
    field: (field_identifier) @name)) @reference.call

(macro_invocation
  macro: (identifier) @name) @reference.call

; Imports (best-effort; Rust's module system makes path→file fuzzy).
(use_declaration
  (identifier) @import.module)

(use_declaration
  (scoped_identifier) @import.module)
