; JavaScript / JSX symbol extraction. Capture names match SymbolKind::as_str().
; Covers React (`.jsx`) and Next.js JS pages. No interfaces/type aliases in JS.
;
; ponytail: JS class names are `identifier` (TS uses `type_identifier`), so JS
; needs its own query rather than sharing TypeScript's.

(function_declaration name: (identifier) @name) @function

(class_declaration name: (identifier) @name) @class

(method_definition name: (property_identifier) @name) @method

; Const arrow / function-expression components and hooks -> functions, so they
; survive the UPPER_SNAKE constant filter.
(variable_declarator name: (identifier) @name value: (arrow_function)) @function
(variable_declarator name: (identifier) @name value: (function_expression)) @function

; Other declarators; filtered to UPPER_SNAKE_CASE in the extractor.
(variable_declarator name: (identifier) @name) @constant
