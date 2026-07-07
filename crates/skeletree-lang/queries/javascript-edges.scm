; JavaScript / JSX edge extraction. Capture names are EdgeKind::as_str() values.
;
; ponytail: JS puts the base expression directly under `class_heritage` (TS
; wraps it in `extends_clause`), so the inheritance patterns differ from TS.

; Direct calls: foo()
(call_expression function: (identifier) @calls)
; Member/method calls: obj.foo() -> capture the method name
(call_expression function: (member_expression property: (property_identifier) @calls))

; Class inheritance: class B extends A / class C extends React.Component
(class_heritage (identifier) @extends)
(class_heritage (member_expression property: (property_identifier) @extends))

; JSX usage builds the component graph: <Header/> -> references Header.
(jsx_opening_element name: (identifier) @references)
(jsx_self_closing_element name: (identifier) @references)
