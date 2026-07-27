; Java tags.scm captures no references. This adds calls + imports + inheritance.

; Calls
(method_invocation
  name: (identifier) @name) @reference.call

(object_creation_expression
  type: (type_identifier) @name) @reference.call

; Imports
(import_declaration
  (scoped_identifier) @import.module)

; Inheritance — superclass/super_interfaces sit inside the class declaration's
; line range, so the enclosing definition resolves to the subclass.
(superclass
  (type_identifier) @name) @reference.implementation

(super_interfaces
  (type_list (type_identifier) @name)) @reference.implementation
