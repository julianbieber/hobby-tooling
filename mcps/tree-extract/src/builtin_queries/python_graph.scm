; Calls come free from helix python tags.scm. This adds imports + inheritance.

; Imports
(import_statement
  name: (dotted_name) @import.module)

(import_statement
  name: (aliased_import (dotted_name) @import.module))

(import_from_statement
  module_name: (dotted_name) @import.module)

(import_from_statement
  module_name: (relative_import) @import.module)

; Inheritance: class Foo(Bar):
(class_definition
  superclasses: (argument_list (identifier) @name)) @reference.implementation
