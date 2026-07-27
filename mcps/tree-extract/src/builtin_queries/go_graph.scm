; Calls + type references come free from helix go tags.scm. This adds imports.
; Go has no class inheritance (structural interfaces), so no implements edges.

(import_spec
  path: (interpreted_string_literal) @import.module)

(import_spec
  name: (package_identifier) @import.symbol
  path: (interpreted_string_literal) @import.module)
