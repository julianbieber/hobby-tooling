(class_definition
  name: (identifier) @name) @definition.class

(object_definition
  name: (identifier) @name) @definition.class

(trait_definition
  name: (identifier) @name) @definition.interface

(type_definition
  name: (type_identifier) @name) @definition.type

(enum_definition
  name: (identifier) @name) @definition.enum

; `def` methods (with body) and abstract method declarations (in traits).
(function_definition
  name: (identifier) @name) @definition.method

(function_declaration
  name: (identifier) @name) @definition.method
