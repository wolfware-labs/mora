// Test code may unwrap (CONTRIBUTING.md, Code rules).
#![allow(clippy::unwrap_used, clippy::expect_used)]

//! Tests that pin parser behavior found untested by `cargo mutants` (§20.5).

use mora_syntax::{SyntaxKind, SyntaxNode, parse};

/// A node as nested node kinds, without tokens: `BinExpr(PathExpr, LiteralExpr)`.
/// Paths in expressions are written `P`.
fn render(node: &SyntaxNode) -> String {
    let kind = format!("{:?}", node.kind());
    if node.kind() == SyntaxKind::PathExpr {
        return "P".into();
    }
    let children: Vec<String> = node
        .children()
        .filter(|c| c.kind() != SyntaxKind::ArgList)
        .map(|c| render(&c))
        .collect();
    if children.is_empty() {
        kind
    } else {
        format!("{kind}({})", children.join(", "))
    }
}

/// The expression of `let _ = <text>;`, which must parse without errors.
fn shape(text: &str) -> String {
    let source = format!("let _ = {text};");
    let parse = parse(&source);
    assert!(parse.errors().is_empty(), "{text}: {:?}", parse.errors());
    let declarator = parse
        .syntax()
        .descendants()
        .find(|n| n.kind() == SyntaxKind::VariableDeclarator)
        .unwrap();
    render(&declarator.children().nth(1).unwrap())
}

/// The pattern of `x is <text>`, which must parse without errors.
fn pattern(text: &str) -> String {
    let s = shape(&format!("x is {text}"));
    s.strip_prefix("IsExpr(P, ")
        .and_then(|s| s.strip_suffix(')'))
        .unwrap()
        .to_owned()
}

/// The whole file as nested node kinds, and the messages of its errors.
fn outline(text: &str) -> (String, Vec<String>) {
    let parse = parse(text);
    assert_eq!(parse.syntax().text(), text, "not lossless");
    let errors = parse.errors().iter().map(|e| e.message.clone()).collect();
    (render(&parse.syntax()), errors)
}

/// The messages of the errors in `text`.
fn errors(text: &str) -> Vec<String> {
    outline(text).1
}

// pattern_operand: a constant pattern stops before `as`, `<` and `==`.

#[test]
fn a_constant_pattern_stops_before_relational_operators() {
    assert_eq!(
        shape("x is 1 as T"),
        "AsExpr(IsExpr(P, ConstantPattern(LiteralExpr)), PathType(Path(PathSegment(NameRef))))"
    );
    assert_eq!(
        shape("x is 1 == y"),
        "BinExpr(IsExpr(P, ConstantPattern(LiteralExpr)), P)"
    );
}

// at_expr_start: a predefined type starts an expression statement.

#[test]
fn a_statement_can_start_with_a_predefined_type() {
    assert_eq!(
        outline("int.Parse(s);").0,
        "SourceFile(ExprStmt(CallExpr(MemberExpr(PredefinedTypeExpr(PredefinedType), NameRef))))"
    );
    assert_eq!(
        errors("void F() { return int.MaxValue; }"),
        Vec::<String>::new()
    );
}

// expr_bp: a range does not associate, on either side of its `..`.

#[test]
fn a_range_takes_one_dot_dot() {
    assert_eq!(errors("let _ = ..a..b;"), ["expected `;`"]);
    assert_eq!(errors("let _ = a..b..c;"), ["expected `;`"]);
    assert_eq!(shape("..a"), "RangeExpr(P)");
    assert_eq!(shape("a.."), "RangeExpr(P)");
}

// conditional_follows: `?` is a conditional only when a `:` follows at
// the same depth before the expression ends.

#[test]
fn a_conditional_needs_a_colon_at_its_own_depth() {
    assert_eq!(
        shape("f() ? 1 : 2"),
        "ConditionalExpr(CallExpr(P), LiteralExpr, LiteralExpr)"
    );
    assert_eq!(
        shape("c ? f(a, b) : d"),
        "ConditionalExpr(P, CallExpr(P), P)"
    );
    // A `;` ends the first expression; the `:` after it is another's.
    assert_eq!(
        outline("let a = r? + 1; let b = c ? 1 : 2;").0,
        "SourceFile(LocalDeclStmt(LocalDecl(VariableDeclarator(Name, BinExpr(PropagateExpr(P), LiteralExpr)))), LocalDeclStmt(LocalDecl(VariableDeclarator(Name, ConditionalExpr(P, LiteralExpr, LiteralExpr)))))"
    );
    // A `:` inside parentheses belongs to them.
    assert_eq!(
        shape("r? + g(n: 1)"),
        "BinExpr(PropagateExpr(P), CallExpr(P))"
    );
}

// at_cast: `(int)` is a cast only when an operand follows.

#[test]
fn a_predefined_type_in_parentheses_casts_only_an_operand() {
    assert_eq!(
        shape("(int)"),
        "ParenExpr(PredefinedTypeExpr(PredefinedType))"
    );
    assert_eq!(outline("let _ = (int)").1, ["expected `;`"]);
    // A cast can open the file.
    assert_eq!(
        outline("(int)x + 1;"),
        (
            "SourceFile(ExprStmt(BinExpr(CastExpr(PredefinedType, P), LiteralExpr)))".into(),
            vec![]
        )
    );
}

// at_lambda: an unclosed `(` at the end of the file is not a lambda.

#[test]
fn an_unclosed_parenthesis_at_the_end_is_not_a_lambda() {
    assert_eq!(errors("let _ = (a"), ["expected `)`", "expected `;`"]);
}

// lambda: capture lists.

#[test]
fn a_capture_list_takes_every_capture() {
    assert_eq!(
        shape("[weak a, move b] () => 0"),
        "LambdaExpr(CaptureList(Capture(NameRef), Capture(NameRef)), LambdaParamList, LiteralExpr)"
    );
    assert_eq!(
        errors("let f = [weak a,"),
        [
            "expected `]`",
            "expected an identifier",
            "expected `=>`",
            "expected an expression",
            "expected `;`"
        ]
    );
}

// primary: a splice is an expression.

#[test]
fn a_splice_is_an_expression() {
    assert_eq!(shape("$x"), "SpliceExpr(Splice(NameRef))");
}

// quote: what `code(...)` holds is a type only when it cannot be an expression.

#[test]
fn a_quote_holds_a_type_only_when_it_cannot_be_an_expression() {
    assert_eq!(shape("code(int)"), "QuoteExpr(PredefinedType)");
    assert_eq!(shape("code(x)"), "QuoteExpr(P)");
    assert_eq!(
        shape("code(int.MaxValue)"),
        "QuoteExpr(MemberExpr(PredefinedTypeExpr(PredefinedType), NameRef))"
    );
}

// array_initializer and new_expr: initializers, empty or not.

#[test]
fn array_initializers_take_every_element() {
    assert_eq!(
        shape("stackalloc int[2] { 1, 2 }"),
        "StackallocExpr(PredefinedType, LiteralExpr, ArrayInitializer(LiteralExpr, LiteralExpr))"
    );
    assert_eq!(
        shape("stackalloc int[0] { }"),
        "StackallocExpr(PredefinedType, LiteralExpr, ArrayInitializer)"
    );
    assert_eq!(
        shape("new int[] { }"),
        "NewArrayExpr(PredefinedType, ArrayInitializer)"
    );
}

// object_initializer: every member.

#[test]
fn an_object_initializer_takes_every_member() {
    assert_eq!(
        shape("new P { A = 1, B = 2 }"),
        "NewExpr(PathType(Path(PathSegment(NameRef))), ObjectInitializer(MemberInit(NameRef, LiteralExpr), MemberInit(NameRef, LiteralExpr)))"
    );
}

// postfix: an index stops at the end of the file.

#[test]
fn an_unclosed_index_stops_at_the_end() {
    assert_eq!(errors("let _ = a[1,"), ["expected `]`", "expected `;`"]);
}

// switch_expr_body: a switch expression may have no arms.

#[test]
fn a_switch_expression_may_be_empty() {
    assert_eq!(shape("x switch { }"), "SwitchExpr(P)");
}

// at_decl_expr: `out a` is a declaration only when a name follows the type.

#[test]
fn out_with_one_name_passes_a_variable() {
    assert_eq!(
        outline("g(f(out a));").0,
        "SourceFile(ExprStmt(CallExpr(P)))"
    );
    assert_eq!(errors("g(f(out a));"), Vec::<String>::new());
}

// interp_string: an unclosed hole skips to its `}`.

#[test]
fn an_unclosed_hole_skips_to_its_brace() {
    assert_eq!(
        outline("let s = $\"{x y}\"; let t = 1;"),
        (
            "SourceFile(LocalDeclStmt(LocalDecl(VariableDeclarator(Name, InterpString(InterpHole(P))))), LocalDeclStmt(LocalDecl(VariableDeclarator(Name, LiteralExpr))))".into(),
            vec!["expected the `}` that closes the hole".into()]
        )
    );
}

// primary_pattern and is_pattern_word: `and`, `or` and `when` after a type
// continue the pattern rather than name a binding.

#[test]
fn a_pattern_word_after_a_type_is_not_a_binding() {
    assert_eq!(
        outline("x is A.B and C;").0,
        "SourceFile(ExprStmt(IsExpr(P, BinPattern(ConstantPattern(MemberExpr(P, NameRef)), ConstantPattern(P)))))"
    );
    assert_eq!(
        pattern("A or B"),
        "BinPattern(ConstantPattern(P), ConstantPattern(P))"
    );
}

// is_plain_path: a case pattern starts with a plain path.

#[test]
fn a_generic_type_before_parentheses_is_not_a_case_pattern() {
    assert_eq!(
        outline("x is List<int>(a);"),
        (
            "SourceFile(ExprStmt(IsExpr(P, TypePattern(PathType(Path(PathSegment(NameRef, GenericArgList(PredefinedType))))))), ExprStmt(ParenExpr(P)))".into(),
            vec!["expected `;`".into()]
        )
    );
}

// tuple_designation: stops at the end of the file.

#[test]
fn an_unclosed_tuple_designation_stops_at_the_end() {
    assert_eq!(
        errors("let (a,"),
        [
            "expected `)`",
            "expected `=`",
            "expected an expression",
            "expected `;`"
        ]
    );
}

// paren_or_positional and single_pattern_in_parens.

#[test]
fn parentheses_hold_one_pattern_or_fields() {
    assert_eq!(pattern("(A)"), "ParenPattern(ConstantPattern(P))");
    assert_eq!(
        pattern("(X: 0)"),
        "PositionalPattern(PatternField(NameRef, ConstantPattern(LiteralExpr)))"
    );
    assert_eq!(
        pattern("([1, 2])"),
        "ParenPattern(ListPattern(ConstantPattern(LiteralExpr), ConstantPattern(LiteralExpr)))"
    );
    assert_eq!(pattern("()"), "PositionalPattern");
    assert_eq!(errors("let _ = x is (1"), ["expected `)`", "expected `;`"]);
}

// list_pattern: may be empty.

#[test]
fn a_list_pattern_may_be_empty() {
    assert_eq!(pattern("[]"), "ListPattern");
}

// statement_inner: `switch` starts a statement only before `(`.

#[test]
fn switch_without_parentheses_is_not_a_statement() {
    assert_eq!(
        outline("switch x { }"),
        (
            "SourceFile(Error, ExprStmt(P), Block)".into(),
            vec!["expected a statement".into(), "expected `;`".into()]
        )
    );
}

// return_stmt: a missing `;` is the only error.

#[test]
fn return_without_a_semicolon_reports_only_the_semicolon() {
    assert_eq!(errors("void F() { return }"), ["expected `;`"]);
}

// switch_stmt and switch_section.

#[test]
fn switch_statements_take_their_sections() {
    assert_eq!(errors("switch (x) { }"), Vec::<String>::new());
    assert_eq!(
        outline("switch (x) { case 1: break; }").0,
        "SourceFile(SwitchStmt(P, SwitchSection(CaseLabel(ConstantPattern(LiteralExpr)), BreakStmt)))"
    );
    // `default` without `:` starts an expression, not a label.
    assert_eq!(
        outline("switch (x) { case 1: default(int).ToString(); break; }").0,
        "SourceFile(SwitchStmt(P, SwitchSection(CaseLabel(ConstantPattern(LiteralExpr)), ExprStmt(CallExpr(MemberExpr(DefaultExpr(PredefinedType), NameRef))), BreakStmt)))"
    );
}

#[test]
fn statements_before_the_first_label_are_one_error() {
    assert_eq!(
        outline("switch (x) { f(); g(); case 1: break; }"),
        (
            "SourceFile(SwitchStmt(P, Error(ExprStmt(CallExpr(P)), ExprStmt(CallExpr(P))), SwitchSection(CaseLabel(ConstantPattern(LiteralExpr)), BreakStmt)))".into(),
            vec!["expected `case` or `default`".into()]
        )
    );
}
