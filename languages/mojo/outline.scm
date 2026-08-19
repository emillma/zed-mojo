(decorator) @annotation

(struct_definition
  "struct" @context
  name: (identifier) @name) @item

(trait_definition
  "trait" @context
  name: (identifier) @name) @item

(extension_definition
  "__extension" @context
  type: (_) @name) @item

(function_definition
  "async"? @context
  "def" @context
  name: (identifier) @name) @item

(mlir_region_definition
  "__mlir_region" @context
  name: (identifier) @name) @item

(comptime_declaration
  "comptime" @context
  name: (identifier) @name) @item
