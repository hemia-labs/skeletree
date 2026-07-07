; Python symbol extraction.
; Capture names match SymbolKind::as_str() exactly, so the extractor maps
; capture -> kind via `SymbolKind::from_str` with no lookup table.
;
; ponytail: only module- and class-level definitions are captured; nested
; closures and nested classes are intentionally skipped (rarely useful as
; standalone symbols). Decorated forms are captured on the INNER definition
; node, so spans/signatures exclude decorator lines. Upgrade path: add
; nested patterns and capture the decorated_definition span if snippets need
; the decorators.

; Module-level functions (plain + decorated)
(module (function_definition name: (identifier) @name) @function)
(module (decorated_definition
  definition: (function_definition name: (identifier) @name) @function))

; Module-level classes (plain + decorated)
(module (class_definition name: (identifier) @name) @class)
(module (decorated_definition
  definition: (class_definition name: (identifier) @name) @class))

; Methods: functions inside a class body (plain + decorated)
(class_definition body: (block
  (function_definition name: (identifier) @name) @method))
(class_definition body: (block
  (decorated_definition
    definition: (function_definition name: (identifier) @name) @method)))

; Module-level assignments; filtered to UPPER_SNAKE_CASE in the extractor.
(module (expression_statement
  (assignment left: (identifier) @name) @constant))
