// Test code may unwrap (CONTRIBUTING.md, Code rules).
#![allow(clippy::unwrap_used, clippy::expect_used)]

//! The fixes of the C# construct errors (M1.5): applied, the file parses.

use mora_diag::codes;
use mora_span::{TextRange, TextSize};
use mora_syntax::{SyntaxFix, parse};

/// The text after applying the only fix `text` reports, which must parse
/// without errors.
fn fixed(text: &str) -> String {
    let parse = parse(text);
    let fixes: Vec<_> = parse
        .errors()
        .iter()
        .filter_map(|e| e.fix.as_ref())
        .collect();
    assert_eq!(fixes.len(), 1, "{:#?}", parse.errors());
    let out = fixes[0].apply(text);
    assert_eq!(
        parse_errors(&out),
        Vec::<String>::new(),
        "fixed text:\n{out}"
    );
    out
}

fn parse_errors(text: &str) -> Vec<String> {
    parse(text)
        .errors()
        .iter()
        .map(|e| format!("{} {:?}", e.code, e.range))
        .collect()
}

#[test]
fn block_namespace_becomes_file_scoped() {
    let text =
        "using Std;\n\nnamespace Shop.Orders\n{\n    class Order { }\n\n    void Ship() { }\n}\n";
    assert_eq!(
        fixed(text),
        "using Std;\n\nnamespace Shop.Orders;\n    class Order { }\n\n    void Ship() { }\n"
    );
}

#[test]
fn block_namespace_on_one_line_keeps_the_rest_of_the_line() {
    assert_eq!(
        fixed("namespace App { class A { } } // done\n"),
        "namespace App;  class A { }  // done\n"
    );
}

#[test]
fn block_namespace_fix_keeps_comments() {
    assert_eq!(
        fixed("namespace App // the app\n{ // types\n    class A { }\n} // end\n"),
        "namespace App; // the app\n // types\n    class A { }\n // end\n"
    );
}

#[test]
fn block_namespace_reports_on_its_name() {
    let text = "namespace App\n{\n}\n";
    let parse = parse(text);
    let [error] = parse.errors() else {
        panic!("{:#?}", parse.errors());
    };
    assert_eq!(error.code, codes::MR0120);
    assert_eq!(&text[error.range], "namespace App");
    assert!(error.note.is_some());
}

#[test]
fn nested_or_unclosed_namespaces_have_no_fix() {
    for text in [
        "namespace A { namespace B { } }",
        "namespace A.B;\nnamespace C { }",
        "namespace A {\nclass C { }\n",
    ] {
        let parse = parse(text);
        assert!(
            parse.errors().iter().any(|e| e.code == codes::MR0120),
            "{text}: {:#?}",
            parse.errors()
        );
        assert!(
            parse.errors().iter().all(|e| e.fix.is_none()),
            "{text}: {:#?}",
            parse.errors()
        );
    }
}

#[test]
fn fix_edits_apply_in_text_order() {
    let range = |a: u32, b: u32| TextRange::new(TextSize::new(a), TextSize::new(b));
    let fix = SyntaxFix {
        message: String::new(),
        edits: vec![
            (range(6, 6), "!".to_owned()),
            (range(0, 1), "J".to_owned()),
            (range(2, 4), String::new()),
        ],
    };
    assert_eq!(fix.apply("hello world"), "Jeo !world");
}

/// The codes `text` reports, each with the text it points at.
fn reported(text: &str) -> Vec<(String, &str)> {
    parse(text)
        .errors()
        .iter()
        .map(|e| (e.code.to_string(), &text[e.range]))
        .collect()
}

#[track_caller]
fn assert_reports(text: &str, expected: &[(&str, &str)]) {
    let expected: Vec<_> = expected
        .iter()
        .map(|(code, at)| ((*code).to_owned(), *at))
        .collect();
    assert_eq!(reported(text), expected, "{text}");
}

#[test]
fn each_construct_reports_one_error_and_recovers() {
    let cases: &[(&str, &[(&str, &str)])] = &[
        // A label is reported with its colon; the statement after it parses.
        ("done: Console.WriteLine(1);", &[("MR0106", "done:")]),
        // `goto` without `;` stops before the `}` of its block.
        ("void F() { goto done }\nint x = 1;", &[("MR0106", "goto")]),
        ("void F() { unsafe void G() { } }", &[("MR0107", "unsafe")]),
        ("void F() { unsafe { G(); } }", &[("MR0107", "unsafe")]),
        ("unsafe { G(); }", &[("MR0107", "unsafe")]),
        // An anonymous delegate is an expression, wherever it starts.
        (
            "void F() { delegate (int x) { G(); }; H(); }",
            &[("MR0105", "delegate")],
        ),
        (
            "let f = delegate (int x) { return x; };",
            &[("MR0105", "delegate")],
        ),
        (
            "class C { delegate void D(); int x; }",
            &[("MR0105", "delegate")],
        ),
        // `using` for disposal, against `using` directives.
        ("using File g = Open();", &[("MR0121", "using")]),
        ("using A = B.C;\nusing System.Text;", &[]),
        ("using Foo g;", &[("MR0110", "")]),
        ("void F() { using Foo; }", &[("MR0110", "using")]),
    ];
    for (text, expected) in cases {
        assert_reports(text, expected);
    }
}

#[test]
fn unsafe_functions_stay_declarations() {
    for text in ["unsafe void F() { }", "class C { unsafe void Raw() { } }"] {
        let tree = parse(text).syntax().debug_tree();
        assert!(tree.contains("FunctionDecl"), "{text}:\n{tree}");
        assert!(!tree.contains("LocalFunctionStmt"), "{text}:\n{tree}");
    }
}
