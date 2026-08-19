; General identifiers are refined by the more specific captures below.
(identifier) @variable

((identifier) @variable.builtin
  (#eq? @variable.builtin "self"))

((identifier) @constructor
  (#match? @constructor "^[A-Z]"))

((identifier) @constant
  (#match? @constant "^_*[A-Z][A-Z0-9_]*$"))

(decorator) @function

(call
  function: (attribute attribute: (identifier) @function.method))

(call
  function: (identifier) @function)

(function_definition
  name: (identifier) @function)

(attribute attribute: (identifier) @property)
(type (identifier) @type)
(generic_type (identifier) @type)

((identifier) @type.builtin
  (#eq? @type.builtin "Self"))

; Auto-imported names from the current Modular stdlib prelude. Keep this after
; soft naming conventions so builtin types win capture precedence.
((identifier) @type.builtin
  (#match? @type.builtin "^(Absable|AddressSpace|AnyOrigin|AnyType|Array|BFloat16|Bool|Boolable|Byte|Codepoint|Comparable|Copyable|DType|Defaultable|Deinitable|Dict|Equatable|Error|FileDescriptor|FileHandle|Float8_e4m3fn|Float8_e4m3fnuz|Float8_e5m2|Float8_e5m2fnuz|Float8_e8m0fnu|Float16|Float32|Float64|FloatLiteral|Floatable|FloatableRaising|Hashable|Identifiable|ImmOpaquePointer|ImmOrigin|ImmPointer|ImmSpan|ImmStaticOrigin|ImmStringSlice|ImmStringSpan|ImmUnsafeAnyOrigin|ImmUntrackedOrigin|ImmutAnyOrigin|ImplicitlyCopyable|Indexer|Int|Int8|Int16|Int32|Int64|Int128|Int256|IntLiteral|Intable|IntableRaising|Iterable|IterableOwned|Iterator|KeyElement|List|Movable|MutAnyOrigin|MutOpaquePointer|MutOrigin|MutPointer|MutSpan|MutStringSlice|MutStringSpan|MutUnsafeAnyOrigin|MutUntrackedOrigin|Never|NoneType|OpaquePointer|Optional|OptionalPointer|Origin|OriginSet|ParameterList|Pointer|Powable|RegisterPassable|ReversibleRange|Roundable|SIMD|SIMDLength|Scalar|Sized|SizedRaising|Slice|Some|SomeTypeList|Span|StaticString|StopIteration|String|StringLiteral|StringSlice|StringSpan|TrivialRegisterPassable|Tuple|TypeList|UInt|UInt8|UInt16|UInt32|UInt64|UInt128|UInt256|UnsafeAnyOrigin|UnsafePointer|UntrackedOrigin|VariadicList|VariadicPack|Writable|Writer)$"))

((call
  function: (identifier) @function.builtin)
  (#match? @function.builtin "^(abs|all|alloc|any|ascii|atof|atol|bin|breakpoint|chr|debug_assert|divmod|enumerate|hash|hex|index|input|iter|len|map|materialize|max|min|next|oct|open|ord|partition|pow|print|range|rebind|rebind_var|reflect|repr|reversed|round|slice|sort|swap|zip)$"))

; These MLIR intrinsics also appear as subscript and attribute heads, so match
; the identifiers directly instead of limiting them to ordinary calls.
((identifier) @function.builtin
  (#any-of? @function.builtin "__mlir_attr" "__mlir_op"))

((identifier) @type.builtin
  (#eq? @type.builtin "__mlir_type"))

((decorator
  (identifier) @attribute.builtin)
  (#match? @attribute.builtin "^(__allow_legacy_custom_self_type|__copy_capture|__nonmaterializable|__parameter|__unsafe_nested_origins_read_only|align|always_inline|deprecated|doc_hidden|explicit_destroy|export|fieldwise_init|implicit|no_inline|noinline|nonmaterializable|parameter|register_passable|stable|staticmethod|unavailable|value)$"))

((decorator
  (call function: (identifier) @attribute.builtin))
  (#match? @attribute.builtin "^(__allow_legacy_custom_self_type|__copy_capture|__nonmaterializable|__parameter|__unsafe_nested_origins_read_only|align|always_inline|deprecated|doc_hidden|explicit_destroy|export|fieldwise_init|implicit|no_inline|noinline|nonmaterializable|parameter|register_passable|stable|staticmethod|unavailable|value)$"))

[
  (true)
  (false)
] @boolean

(none) @constant.builtin
(ellipsis) @constant.builtin

[
  (integer)
  (float)
] @number

[
  "."
  ","
  ":"
] @punctuation.delimiter

[
  "("
  ")"
  "["
  "]"
  "{"
  "}"
] @punctuation.bracket

(comment) @comment
(string) @string
(escape_sequence) @string.escape

(interpolation
  "{" @punctuation.special
  "}" @punctuation.special) @embedded

; First string expressions are docstrings at module/declaration scope.
(module
  . (string) @string.doc)

(function_definition
  body: (block
    . (string) @string.doc))

[
  (class_definition
    body: (block
      . (string) @string.doc))
  (struct_definition
    body: (block
      . (string) @string.doc))
  (trait_definition
    body: (block
      . (string) @string.doc))
]

[
  "-"
  "-="
  "!="
  "*"
  "**"
  "**="
  "*="
  "@="
  "/"
  "//"
  "//="
  "/="
  "&"
  "&="
  "%"
  "%="
  "^"
  "^="
  "+"
  "->"
  "+="
  "<"
  "<<"
  "<<="
  "<="
  "<>"
  "="
  ":="
  "=="
  ">"
  ">="
  ">>"
  ">>="
  "|"
  "|="
  "~"
  "and"
  "in"
  "is"
  "not"
  "not in"
  "or"
  "is not"
] @operator

[
  "as"
  "assert"
  "async"
  "await"
  "break"
  "comptime"
  "continue"
  "def"
  "del"
  "elif"
  "else"
  "except"
  "finally"
  "for"
  "from"
  "global"
  "if"
  "import"
  "lambda"
  "nonlocal"
  "pass"
  "raise"
  "return"
  "struct"
  "trait"
  "try"
  "type"
  "var"
  "while"
  "with"
  "yield"
] @keyword

(raises_clause "raises" @keyword)
(argument_convention) @keyword.modifier
(capturing_clause "capturing" @keyword.modifier)
(abi_clause "abi" @keyword.modifier)
(ref_type "ref" @keyword.modifier)
(generator_type "__generator_type" @type.builtin)
(where_clause "where" @keyword)
(where_expression "where" @keyword)
(extension_definition "__extension" @keyword)
(mlir_region_definition "__mlir_region" @keyword)
(function_definition "thin" @keyword.modifier)
(function_type "thin" @keyword.modifier)
(lambda "thin" @keyword.modifier)
