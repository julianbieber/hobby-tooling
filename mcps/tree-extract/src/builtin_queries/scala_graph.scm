; Scala uses a built-in tags query (definitions only). This adds calls +
; imports + inheritance. Node names validated against the installed
; tree-sitter-scala grammar by the compile test.

; Calls
(call_expression
  function: (identifier) @name) @reference.call

(call_expression
  function: (field_expression
    field: (identifier) @name)) @reference.call

; Imports
(import_declaration
  path: (_) @import.module)

; Inheritance: extends_clause inside a class/object/trait definition.
(extends_clause
  (type_identifier) @name) @reference.implementation
