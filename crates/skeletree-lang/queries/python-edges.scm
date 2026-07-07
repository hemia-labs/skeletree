; Python edge extraction. Capture names are EdgeKind::as_str() values, so the
; extractor maps capture -> kind via `EdgeKind::from_str` with no lookup table.
;
; The captured node is always the callee/base *identifier*; the "from" symbol
; (the caller function, or the subclass) is found by walking ancestors to the
; nearest function/class definition in the extractor.
;
; ponytail: calls are resolved by name later (may hit false positives; ranking
; buries them). Module-level calls have no enclosing symbol and are skipped.
; imports and references are not extracted yet — add patterns here when path
; resolution lands.

; Direct calls: foo()
(call function: (identifier) @calls)
; Attribute/method calls: obj.foo() -> capture the method name
(call function: (attribute attribute: (identifier) @calls))
; Class inheritance: class Foo(Base, Other): -> one capture per base
(class_definition superclasses: (argument_list (identifier) @extends))
