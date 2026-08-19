; Only top-level main functions are executable entry points.
(module
  (function_definition
    name: (identifier) @run
    (#eq? @run "main")
    (#set! tag mojo-main)))

(module
  (decorated_definition
    definition: (function_definition
      name: (identifier) @run
      (#eq? @run "main")
      (#set! tag mojo-main))))
