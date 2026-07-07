; Rust symbol extraction. Capture names match SymbolKind::as_str().
;
; ponytail: SymbolKind has no struct/enum/trait variants, so Rust maps onto the
; existing set: struct/enum/union -> class, trait -> interface, type -> type_alias.
; Free fns vs methods are told apart by parent context (module vs impl/trait),
; which avoids the double-capture a bare `function_item` pattern would cause.

; Free functions: crate root and inside modules
(source_file (function_item name: (identifier) @name) @function)
(mod_item body: (declaration_list
  (function_item name: (identifier) @name) @function))

; Methods: functions inside impl / trait blocks
(impl_item body: (declaration_list
  (function_item name: (identifier) @name) @method))
(trait_item body: (declaration_list
  (function_item name: (identifier) @name) @method))
; Trait method declarations without a body are `function_signature_item`.
(trait_item body: (declaration_list
  (function_signature_item name: (identifier) @name) @method))

; Types (structs, enums, unions read as "class"; traits as "interface")
(struct_item name: (type_identifier) @name) @class
(enum_item name: (type_identifier) @name) @class
(union_item name: (type_identifier) @name) @class
(trait_item name: (type_identifier) @name) @interface
(type_item name: (type_identifier) @name) @type_alias

; Constants and statics; filtered to UPPER_SNAKE_CASE in the extractor.
(const_item name: (identifier) @name) @constant
(static_item name: (identifier) @name) @constant
