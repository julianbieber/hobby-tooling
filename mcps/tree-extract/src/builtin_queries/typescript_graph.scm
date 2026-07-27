; TypeScript inherits helix's `_typescript` (type/class refs) but NOT call refs
; (those live in `_javascript`, which typescript does not inherit). So we add
; calls here, plus imports + inheritance. Shared by `typescript` and `tsx`.

; Calls
(call_expression
  function: (identifier) @name) @reference.call

(call_expression
  function: (member_expression
    property: (property_identifier) @name)) @reference.call

(new_expression
  constructor: (identifier) @name) @reference.class

; Imports
(import_statement
  source: (string (string_fragment) @import.module))

; Inheritance
(class_heritage
  (extends_clause
    value: (identifier) @name)) @reference.implementation

(class_heritage
  (implements_clause
    (type_identifier) @name)) @reference.implementation
