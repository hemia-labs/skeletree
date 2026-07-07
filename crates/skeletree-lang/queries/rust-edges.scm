; Rust edge extraction. Capture names are EdgeKind::as_str() values.
;
; ponytail: `impl Trait for Type` doesn't fit the "from = nearest enclosing
; symbol" model (an impl block has no name symbol), so it's skipped for now.
; Supertrait bounds DO fit — `trait A: B` encloses in trait A — so those become
; extends edges. Calls are the main connective tissue, resolved by name later.

; Direct calls: foo()
(call_expression function: (identifier) @calls)
; Method calls: obj.foo() -> capture the method name
(call_expression function: (field_expression field: (field_identifier) @calls))
; Path calls: Foo::bar() / module::foo() -> capture the final segment
(call_expression function: (scoped_identifier name: (identifier) @calls))

; Supertraits: trait A: B + C -> A extends B, A extends C
(trait_item bounds: (trait_bounds (type_identifier) @extends))
