; TypeScript symbol extraction. Capture names match SymbolKind::as_str()
; exactly, so the extractor maps capture -> kind via `SymbolKind::from_str`.
;
; Covers NestJS out of the box: controllers/services/modules are just
; decorated classes with decorated methods, which tree-sitter parses as plain
; class/method nodes with `decorator` children — no framework-specific rules.
;
; ponytail: `export`-wrapped forms match too (queries ignore nesting), so no
; separate patterns for them. Nested/local functions are captured broadly and
; ranking sorts them out.

(function_declaration name: (identifier) @name) @function

(class_declaration name: (type_identifier) @name) @class
(abstract_class_declaration name: (type_identifier) @name) @class

(method_definition name: (property_identifier) @name) @method

; React components / hooks written as const arrows or function expressions:
;   const App = () => ...   /   const useThing = function () {...}
; Captured as functions, not constants, so they survive the UPPER_SNAKE filter.
(variable_declarator name: (identifier) @name value: (arrow_function)) @function
(variable_declarator name: (identifier) @name value: (function_expression)) @function

(interface_declaration name: (type_identifier) @name) @interface

(type_alias_declaration name: (type_identifier) @name) @type_alias

; const/let/var declarators; filtered to UPPER_SNAKE_CASE in the extractor.
(variable_declarator name: (identifier) @name) @constant
