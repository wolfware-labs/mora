// Test code may unwrap (CONTRIBUTING.md, Code rules).
#![allow(clippy::unwrap_used, clippy::expect_used)]

//! Tests that pin parser behavior found untested by `cargo mutants` (§20.5).

use mora_syntax::ast::{self, AstNode};
use mora_syntax::{SyntaxKind as K, SyntaxNode, parse};

/// The error messages, in the order of the text.
fn messages(text: &str) -> Vec<String> {
    parse(text)
        .errors()
        .iter()
        .map(|error| error.message.clone())
        .collect()
}

fn count(text: &str, kind: K) -> usize {
    parse(text)
        .syntax()
        .descendants()
        .filter(|n| n.kind() == kind)
        .count()
}

fn has(text: &str, kind: K) -> bool {
    count(text, kind) > 0
}

/// The first node of `kind`, which must be there.
fn node(text: &str, kind: K) -> SyntaxNode {
    parse(text)
        .syntax()
        .descendants()
        .find(|n| n.kind() == kind)
        .unwrap_or_else(|| panic!("no {kind:?} in {text:?}"))
}

/// Parses without errors and holds a node of `kind`.
fn assert_clean_with(text: &str, kind: K) {
    assert_eq!(messages(text), Vec::<String>::new(), "{text:?}");
    assert!(has(text, kind), "no {kind:?} in {text:?}");
}

/// A property of type `ty` parses without errors: the member's shape
/// needs `scan_type` to find the end of the type.
fn assert_property_type(ty: &str) -> String {
    let text = format!("class A {{ P: {ty} {{ get; }} }}");
    assert_clean_with(&text, K::PropertyDecl);
    text
}

// `ty`: `weak T` is a type of its own.

#[test]
fn a_weak_field_type_parses() {
    let text = "class A { Parent: weak Node?; }";
    assert_clean_with(text, K::WeakType);
    assert!(has(text, K::FieldDecl));
}

// `scan_type`: a splice type, a tuple type, an alias-qualified, nested or
// array type before `{` makes a property.

#[test]
fn a_spread_splice_type_makes_a_property() {
    assert_property_type("$..ts");
}

#[test]
fn a_parenthesized_splice_type_makes_a_property() {
    assert_property_type("$(T)");
    assert_property_type("$(f())");
}

#[test]
fn an_unclosed_splice_type_ends_at_the_end_of_the_file() {
    assert_eq!(
        messages("F() { $(a"),
        ["expected `)`", "expected `;`", "expected `}`"]
    );
}

#[test]
fn a_weak_or_unit_type_makes_a_property() {
    let text = assert_property_type("weak Node?");
    assert!(has(&text, K::WeakType));
    assert_property_type("()");
}

#[test]
fn a_tuple_type_makes_a_property() {
    assert_property_type("(a: int, b: string)");
    assert_property_type("(int, string)");
    assert_property_type("(int, string,)");
}

#[test]
fn an_alias_qualified_type_makes_a_property() {
    let text = assert_property_type("Alias::T");
    assert!(has(&text, K::AliasQualifier));
}

#[test]
fn a_nested_generic_type_makes_a_property() {
    assert_property_type("Outer<int>.Inner");
}

#[test]
fn an_array_type_makes_a_property() {
    let text = assert_property_type("int[]");
    assert!(has(&text, K::ArrayType));
}

#[test]
fn an_unclosed_index_is_not_an_array_type() {
    let text = "F() { T[0 x; }";
    assert!(!has(text, K::LocalDeclStmt));
    assert!(has(text, K::IndexExpr));
    assert_eq!(messages(text), ["expected `]`", "expected `;`"]);
}

// `child_between`: the parts of `a ? b : c` are found across whitespace.

#[test]
fn conditional_parts_skip_whitespace() {
    let cond = parse("var x = a ? b : c;")
        .syntax()
        .descendants()
        .find_map(ast::ConditionalExpr::cast)
        .unwrap();
    assert_eq!(cond.condition().unwrap().syntax().text(), "a");
    assert_eq!(cond.then().unwrap().syntax().text(), "b");
    assert_eq!(cond.r#else().unwrap().syntax().text(), "c");
}

// `describe`: how errors name an identifier and a string.

#[test]
fn a_missing_name_is_an_identifier() {
    assert_eq!(messages("class { }"), ["expected an identifier"]);
}

#[test]
fn a_missing_import_path_is_a_string() {
    assert_eq!(messages("import Core from ;"), ["expected a string"]);
}

// `nth_text`: `and` after a type in a pattern combines patterns.

#[test]
fn and_after_a_type_pattern_combines_patterns() {
    let text = "let b: bool = x is Foo and Bar;";
    assert_clean_with(text, K::BinPattern);
    assert!(!has(text, K::DeclarationPattern));
}

// `attached_comments`: comments directly above a declaration belong to it,
// indented or not, unless a blank line separates them.

#[test]
fn an_indented_comment_belongs_to_the_member_below() {
    let field = node("class A {\n    // doc\n    X: int;\n}", K::FieldDecl);
    assert!(field.text().starts_with("// doc"), "{}", field.text());
}

#[test]
fn a_blank_line_detaches_a_comment() {
    let text = "// a\n\nstruct S {}";
    assert!(node(text, K::StructDecl).text().starts_with("struct"));
    let first = parse(text).syntax().first_token().map(|token| token.kind());
    assert_eq!(first, Some(K::LineComment));
}

// `top_level`: `global` starts a `using` only before `using`.

#[test]
fn global_alone_is_a_name() {
    let text = "global = 1;";
    assert_clean_with(text, K::ExprStmt);
    assert!(!has(text, K::UsingDirective));
}

// `declaration`: what may follow attributes and modifiers.

#[test]
fn a_destructor_is_a_member_only() {
    let text = "static ~Foo() {}";
    assert!(!has(text, K::DestructorDecl));
    assert_eq!(
        messages(text)[0],
        "expected a declaration after the attributes and modifiers"
    );
}

#[test]
fn a_member_cannot_be_an_attributed_statement() {
    for text in ["class A { public 1; }", "class A { [X] 1; }"] {
        assert!(!has(text, K::AttributedStmt), "{text:?}");
        assert_eq!(
            messages(text),
            [
                "expected a declaration after the attributes and modifiers",
                "expected a member"
            ],
            "{text:?}"
        );
    }
}

#[test]
fn modifiers_do_not_make_an_attributed_statement() {
    let text = "static 1;";
    assert!(!has(text, K::AttributedStmt));
    assert_eq!(
        messages(text),
        ["expected a declaration after the attributes and modifiers"]
    );
}

// `at_declaration_start`: a variable at the top level is a local.

#[test]
fn a_top_level_variable_is_a_local() {
    let text = "let x: int = 1;";
    assert_clean_with(text, K::LocalDeclStmt);
    assert!(!has(text, K::FieldDecl));
}

#[test]
fn an_operator_is_a_member_only() {
    let text = "operator <(a: A, b: A): bool => true;";
    assert!(!has(text, K::OperatorDecl));
    assert_eq!(messages(text)[0], "expected a statement");
}

// `at_attribute`: a `[` opens an attribute only when it names something and
// a declaration or a statement follows.

#[test]
fn a_bracketed_literal_is_not_an_attribute() {
    let text = "[1] x;";
    assert!(!has(text, K::Attribute));
    assert_eq!(messages(text), ["expected `;`"]);
}

#[test]
fn brackets_at_the_end_of_the_file_are_not_an_attribute() {
    let text = "[A]";
    assert!(!has(text, K::Attribute));
    assert_eq!(messages(text), ["expected `;`"]);
}

#[test]
fn an_unclosed_attribute_ends_at_the_end_of_the_file() {
    assert_eq!(messages("[Json"), ["expected `]`", "expected `;`"]);
}

#[test]
fn an_attribute_before_a_keyword_declaration() {
    let text = "[Json] struct S {}";
    assert_clean_with(text, K::Attribute);
    assert!(node(text, K::StructDecl).text().starts_with("[Json]"));
}

#[test]
fn an_attribute_before_a_block_statement() {
    let text = "[Trace] do { } while (x);";
    assert_clean_with(text, K::AttributedStmt);
    assert!(node(text, K::AttributedStmt).text().starts_with("[Trace]"));
}

// `record`, `required` and `init` are names, not keywords.

#[test]
fn record_is_a_name() {
    assert_clean_with("record = 5;", K::ExprStmt);
}

// `at_contextual_modifier`: `file` before a keyword or a name is a modifier.

#[test]
fn file_is_a_modifier_before_a_declaration() {
    for text in ["file class A { }", "file Helper() { }"] {
        assert_clean_with(text, K::Modifier);
        assert_eq!(node(text, K::Modifier).text(), "file", "{text:?}");
    }
}

#[test]
fn file_alone_is_a_name() {
    assert_clean_with("file = 5;", K::ExprStmt);
    assert!(!has("file = 5;", K::Modifier));
}

// `statement_inner`: `unsafe` starts a statement only before a block.

#[test]
fn unsafe_takes_a_block() {
    assert_clean_with("F() { unsafe { Free(p); } }", K::UnsafeStmt);
    let text = "F() { unsafe x; }";
    assert!(!has(text, K::UnsafeStmt));
    assert!(!messages(text).is_empty());
}

#[test]
fn required_is_not_a_modifier() {
    let text = "class A { required X: Name; }";
    assert!(!has(text, K::Modifier));
    assert!(!messages(text).is_empty());
}

// `attribute`: trailing commas and arguments without a name.

#[test]
fn attribute_lists_take_a_trailing_comma() {
    assert_clean_with("[Json,] struct S {}", K::AttributeItem);
    assert_clean_with("[Cfg(a,)] struct S {}", K::AttributeArg);
}

#[test]
fn an_attribute_argument_without_a_name() {
    let text = "[Cfg(Debug)] struct S {}";
    assert_clean_with(text, K::AttributeArg);
    assert_eq!(node(text, K::AttributeArg).text(), "Debug");
}

// `union_decl`: an empty case list and a trailing comma.

#[test]
fn union_case_lists_can_be_empty_or_end_in_a_comma() {
    assert_clean_with("union U { }", K::UnionDecl);
    assert!(!has("union U { }", K::UnionCase));
    let text = "union U { A, }";
    assert_clean_with(text, K::UnionCase);
    assert_eq!(count(text, K::UnionCase), 1);
}

// `explicit_impl` and `indexer`.

#[test]
fn an_explicit_indexer_with_a_dotted_interface() {
    let text = "class A { Outer.IList<int>.this[i: int]: int => 0; }";
    assert_clean_with(text, K::IndexerDecl);
    assert_eq!(node(text, K::ExplicitImpl).text(), "Outer.IList<int>.");
}

#[test]
fn indexer_parameter_lists() {
    assert_clean_with("class A { this[]: int => 0; }", K::IndexerDecl);
    let text = "class A { this[a: int, b: int]: int => 0; }";
    assert_clean_with(text, K::IndexerDecl);
    assert_eq!(count(text, K::Param), 2);
}

// `type_param_list`: a trailing comma and variance.

#[test]
fn type_parameters_take_a_trailing_comma() {
    assert_clean_with("class A<T,> {}", K::TypeParam);
}

#[test]
fn type_parameters_take_variance() {
    let text = "interface IC<in T, out U> {}";
    assert_clean_with(text, K::TypeParamList);
    assert_eq!(node(text, K::TypeParamList).text(), "<in T, out U>");
}

// `where_clause`: a named type is a type constraint.

#[test]
fn a_named_constraint_is_a_type_constraint() {
    let text = "class A<T> where T : IFoo { }";
    assert_clean_with(text, K::TypeConstraint);
    assert!(!has(text, K::KindConstraint));
}

#[test]
fn allows_widens_with_move_or_ref_struct() {
    assert_clean_with("class A<T> where T : allows move { }", K::AllowsConstraint);
    assert_clean_with(
        "class A<T> where T : class, allows ref struct { }",
        K::AllowsConstraint,
    );
    // `allows` alone, or before anything else, is a type named `allows`.
    let text = "class A<T> where T : allows { }";
    assert!(has(text, K::TypeConstraint));
    assert!(!has(text, K::AllowsConstraint));
}

#[test]
fn move_and_ref_struct_are_not_constraints_without_allows() {
    for text in [
        "class A<T> where T : move { }",
        "class A<T> where T : ref struct { }",
        "class A<T> where T : new() { }",
    ] {
        assert!(!has(text, K::AllowsConstraint), "{text}");
        assert!(!messages(text).is_empty(), "{text}");
    }
}

// `declaration` and `shape`: indexers, operators and properties are members
// only; at the top level they are not declarations, whatever the order.

#[test]
fn an_indexer_is_a_member_only() {
    for text in [
        "static this[0];",
        "static this[i: int]: int => 0;",
        "static IList.this[i: int]: int => 0;",
    ] {
        assert!(!has(text, K::IndexerDecl), "{text:?}");
        assert!(!messages(text).is_empty(), "{text:?}");
    }
}

#[test]
fn a_property_is_a_member_only() {
    let text = "static X: int => 1;";
    assert!(!has(text, K::PropertyDecl));
}

#[test]
fn an_attributed_local_stays_a_statement() {
    let text = "[Cfg(Debug = true)] let x = 1;";
    assert_clean_with(text, K::AttributedStmt);
    assert!(!has(text, K::FieldDecl));
}

// `shape`: a dot is an explicit indexer only before `this`.

#[test]
fn an_explicit_indexer() {
    let text = "class A { IList<int>.this[i: int]: int => 0; }";
    assert_clean_with(text, K::IndexerDecl);
    assert_eq!(node(text, K::ExplicitImpl).text(), "IList<int>.");
}

// `shape`: a splice as the name of a member.

#[test]
fn a_spliced_member_name() {
    let text = "class A { $f(): int => 0; $g: int; }";
    assert_eq!(messages(text), Vec::<String>::new());
    assert_eq!(count(text, K::FunctionDecl), 1);
    assert_eq!(count(text, K::FieldDecl), 1);
    assert!(node(text, K::FunctionDecl).text().contains("$f"));
}

// `operator`: no return type, no annotation.

#[test]
fn an_operator_without_a_return_type() {
    let text = "class A { static operator ++(a: A) => a; }";
    assert_clean_with(text, K::OperatorDecl);
    assert!(has(text, K::ExprBody));
    assert!(!has(text, K::PredefinedType));
}

// `shape`: name first, a dot is an explicit indexer only before `this` and
// in a type, and a name alone is a field that lacks its type.

#[test]
fn a_name_first_dot_without_this_is_not_an_indexer() {
    assert!(!has("class A { X.; }", K::IndexerDecl));
    assert!(!has("static IList.this[i: int]: int => 0;", K::IndexerDecl));
}

#[test]
fn a_field_without_a_type() {
    let text = "class A { Count; }";
    assert!(has(text, K::FieldDecl));
    assert_eq!(messages(text), ["expected `:` and a type"]);
}

// `at_function`: parentheses left open at the end of the file are a call.

#[test]
fn an_unclosed_call_at_the_end_is_not_a_function() {
    let text = "F(a";
    assert!(!has(text, K::FunctionDecl));
    assert!(has(text, K::ExprStmt));
}

// `shape`: a field has one plain name where fields are allowed.

#[test]
fn a_field_needs_a_plain_name_and_a_place_for_fields() {
    // `IShape.` is skipped as an error; the field is `X` alone.
    assert_eq!(
        node("class A { IShape.X: int; }", K::FieldDecl).text(),
        "X: int;"
    );
    assert!(!has("[A] x: int;", K::FieldDecl));
}

// `at_any`: `any` starts a boxed interface type only before a name.

#[test]
fn any_alone_is_a_type_name() {
    for text in ["struct S { X: any; }", "struct S { X: List<any>; }"] {
        assert_clean_with(text, K::PathType);
        assert!(!has(text, K::AnyType), "{text}");
    }
    assert_clean_with("struct S { X: any IShape; }", K::AnyType);
}
