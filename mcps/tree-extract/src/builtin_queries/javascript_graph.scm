; Calls + instantiation come free from helix `_javascript` tags. This adds
; imports + inheritance. Shared by `javascript` and `jsx`.

; Imports
(import_statement
  source: (string (string_fragment) @import.module))

; Inheritance: class Foo extends Bar {}
(class_heritage
  (identifier) @name) @reference.implementation
