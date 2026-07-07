; TypeScript edge extraction. Capture names are EdgeKind::as_str() values.
; The captured node is the callee/base *identifier*; the "from" symbol (the
; calling function/method, or the subclass) is the nearest enclosing def.
;
; ponytail: calls resolve by name later (false positives buried by ranking).
; Member calls capture only the property name (`this.svc.find()` -> `find`),
; which is how NestJS services are invoked. imports/references not yet
; extracted — add patterns when module resolution lands.

; Direct calls: foo()
(call_expression function: (identifier) @calls)
; Member/method calls: obj.foo() / this.svc.foo() -> capture the method name
(call_expression function: (member_expression property: (property_identifier) @calls))

; Class inheritance: class B extends A / class C extends React.Component
(extends_clause (identifier) @extends)
(extends_clause (member_expression property: (property_identifier) @extends))

; JSX usage builds the component graph: <Header/> -> references Header.
; HTML tags (`<div>`) never match a symbol name, so they resolve to nothing.
(jsx_opening_element name: (identifier) @references)
(jsx_self_closing_element name: (identifier) @references)
