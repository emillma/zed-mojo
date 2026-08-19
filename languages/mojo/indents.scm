(_ "[" "]" @end) @indent
(_ "{" "}" @end) @indent
(_ "(" ")" @end) @indent

(function_definition) @start.def
(mlir_region_definition) @start.mlir_region
(struct_definition) @start.struct
(trait_definition) @start.trait
(extension_definition) @start.extension
(if_statement) @start.if
(for_statement) @start.for
(while_statement) @start.while
(with_statement) @start.with
(try_statement) @start.try
(elif_clause) @start.elif
(else_clause) @start.else
(except_clause) @start.except
(finally_clause) @start.finally
