(comment)+ @comment.around

(function_definition
  body: (block) @function.inside) @function.around

(mlir_region
  body: (block) @function.inside) @function.around

[
  (class_definition
    body: (block) @class.inside)
  (trait_definition
    body: (block) @class.inside)
  (extension_definition
    body: (block) @class.inside)
] @class.around
