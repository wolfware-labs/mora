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
    let text = "using Std;\n\nnamespace Shop.Orders\n{\n    class Order { }\n\n    Ship() { }\n}\n";
    assert_eq!(
        fixed(text),
        "using Std;\n\nnamespace Shop.Orders;\n    class Order { }\n\n    Ship() { }\n"
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
        ("F() { goto done }\nlet x: int = 1;", &[("MR0106", "goto")]),
        ("F() { unsafe G() { } }", &[("MR0107", "unsafe")]),
        ("F() { unsafe { G(); } }", &[("MR0107", "unsafe")]),
        ("unsafe { G(); }", &[("MR0107", "unsafe")]),
        ("fixed (int* p = &x) { G(); }", &[("MR0107", "fixed")]),
        // A reserved modifier before a tuple type still starts a declaration.
        (
            "virtual F(): (int, int) => (1, 2);",
            &[("MR0108", "virtual")],
        ),
        // An anonymous delegate is an expression, wherever it starts.
        (
            "F() { delegate (int x) { G(); }; H(); }",
            &[("MR0105", "delegate")],
        ),
        (
            "let f = delegate (int x) { return x; };",
            &[("MR0105", "delegate")],
        ),
        (
            "class C { delegate void D(); x: int; }",
            &[("MR0105", "delegate")],
        ),
        // `null` is read as `none`, in expressions, patterns and casts.
        ("let u: User? = null;", &[("MR0129", "null")]),
        ("let found = u is null;", &[("MR0129", "null")]),
        (
            "let v = u switch { null => 0, _ => 1 };",
            &[("MR0129", "null")],
        ),
        ("let w = (User?)null;", &[("MR0129", "null")]),
        ("let x: User? = none;", &[]),
        // `using` for disposal, against `using` directives.
        ("using var g: File = Open();", &[("MR0121", "using")]),
        ("using A = B.C;\nusing System.Text;", &[]),
        ("using Foo g;", &[("MR0110", "")]),
        ("F() { using Foo; }", &[("MR0110", "using")]),
    ];
    for (text, expected) in cases {
        assert_reports(text, expected);
    }
}

#[test]
fn unsafe_functions_stay_declarations() {
    for text in ["unsafe F() { }", "class C { unsafe Raw() { } }"] {
        let tree = parse(text).syntax().debug_tree();
        assert!(tree.contains("FunctionDecl"), "{text}:\n{tree}");
        assert!(!tree.contains("LocalFunctionStmt"), "{text}:\n{tree}");
    }
}

#[test]
fn null_fixes_to_none() {
    assert_eq!(fixed("let u: User? = null;\n"), "let u: User? = none;\n");
    let tree = parse("let found = u is null;").syntax().debug_tree();
    assert!(tree.contains("NoneKw"), "{tree}");
}

/// `text` with the fixes of every error applied at once; the result must
/// parse without errors.
fn all_fixed(text: &str) -> String {
    let edits = parse(text)
        .errors()
        .iter()
        .filter_map(|e| e.fix.as_ref())
        .flat_map(|fix| fix.edits.iter().cloned())
        .collect::<Vec<_>>();
    let mut ranges: Vec<_> = edits.iter().map(|(range, _)| *range).collect();
    ranges.sort_by_key(|range| (range.start(), range.end()));
    for pair in ranges.windows(2) {
        assert!(pair[0].end() <= pair[1].start(), "{text}: {pair:?} overlap");
    }
    let out = SyntaxFix {
        message: String::new(),
        edits,
    }
    .apply(text);
    assert_eq!(
        parse_errors(&out),
        Vec::<String>::new(),
        "fixed text:\n{out}"
    );
    out
}

#[test]
fn type_first_declarations_fix_to_name_first() {
    let cases = [
        (
            "public mut decimal Balance;",
            "public mut Balance: decimal;",
        ),
        ("const int Max = 5, Min;", "const Max: int = 5, Min: int;"),
        ("public static int N = 0;", "public static N: int = 0;"),
        ("int x = 5;", "var x: int = 5;"),
        ("List<int> xs;", "let xs: List<int>;"),
        ("let int y = 1;", "let y: int = 1;"),
        ("var string s = \"a\";", "var s: string = \"a\";"),
        ("void Tick() { }", "Tick() { }"),
        ("async void Run() { }", "async Run() { }"),
        (
            "T Max<T>(T a, T b) where T : IComparable<T> => a;",
            "Max<T>(a: T, b: T): T where T : IComparable<T> => a;",
        ),
        (
            "static string Shout(this string s, int times = 1) => s;",
            "static Shout(this s: string, times: int = 1): string => s;",
        ),
        (
            "foreach (int n in xs) { }",
            "foreach (let n: int in xs) { }",
        ),
        (
            "foreach (let int n in xs) { }",
            "foreach (let n: int in xs) { }",
        ),
        (
            "let f = (int a, ref int b) => a;",
            "let f = (a: int, ref b: int) => a;",
        ),
        ("F(out int z);", "F(out let z: int);"),
        (
            "let p: (int x, string y) = t;",
            "let p: (x: int, y: string) = t;",
        ),
        (
            "class C { public int Count { get; } public T this[int i] => i; }",
            "class C { public Count: int { get; } public this[i: int]: T => i; }",
        ),
        (
            "class V { public static V operator -(V a) => a; float IShape.Area() => 0; }",
            "class V { public static operator -(a: V): V => a; IShape.Area(): float => 0; }",
        ),
        (
            "union U { Circle(double Radius) }",
            "union U { Circle(Radius: double) }",
        ),
        ("macro M(TypeInfo t) { }", "macro M(t: TypeInfo) { }"),
        (
            "[Import(\"c\")] static extern int puts(ReadOnlySpan<byte> s);",
            "[Import(\"c\")] static extern puts(s: ReadOnlySpan<byte>): int;",
        ),
    ];
    for (text, expected) in cases {
        assert!(
            parse(text).errors().iter().all(|e| e.code == codes::MR0135),
            "{text}: {:#?}",
            parse(text).errors()
        );
        assert_eq!(all_fixed(text), expected, "{text}");
    }
}

/// A type written first that holds declarations written first itself: the
/// fix that moves the type moves them fixed, and every error is still
/// reported. Applied together, the fixes give the name-first text.
#[test]
fn nested_type_first_declarations_fix_together() {
    let cases = [
        (
            "class C { List<(int x, string y)> xs; }",
            "class C { xs: List<(x: int, y: string)>; }",
            3,
        ),
        (
            "class C { Dictionary<string, (int a, int b)> Pairs() => d; }",
            "class C { Pairs(): Dictionary<string, (a: int, b: int)> => d; }",
            3,
        ),
        (
            "F(List<(int a, int b)> xs) { }",
            "F(xs: List<(a: int, b: int)>) { }",
            3,
        ),
        (
            "void G((int a, int b) p) { }",
            "G(p: (a: int, b: int)) { }",
            4,
        ),
        (
            "class C { public (int a, int b) P { get; } public (int a, int b) this[(int a, int b) k] => k; }",
            "class C { public P: (a: int, b: int) { get; } public this[k: (a: int, b: int)]: (a: int, b: int) => k; }",
            9,
        ),
        (
            "class V { public static (int a, V b) operator +(V x, V y) => x; }",
            "class V { public static operator +(x: V, y: V): (a: int, b: V) => x; }",
            5,
        ),
        (
            "List<(int a, int b)> xs = d;",
            "var xs: List<(a: int, b: int)> = d;",
            3,
        ),
        (
            "foreach (List<(int a, int b)> p in ps) { }",
            "foreach (let p: List<(a: int, b: int)> in ps) { }",
            3,
        ),
        (
            "let f = ((int a, int b) p) => p;",
            "let f = (p: (a: int, b: int)) => p;",
            3,
        ),
        (
            "F(out (int a, int b) p);",
            "F(out let p: (a: int, b: int));",
            3,
        ),
        (
            "List<(List<(int a, int b)> x, int y)> z;",
            "let z: List<(x: List<(a: int, b: int)>, y: int)>;",
            5,
        ),
    ];
    for (text, expected, errors) in cases {
        let parse = parse(text);
        assert!(
            parse.errors().iter().all(|e| e.code == codes::MR0135),
            "{text}: {:#?}",
            parse.errors()
        );
        assert_eq!(
            parse.errors().len(),
            errors,
            "{text}: {:#?}",
            parse.errors()
        );
        assert_eq!(all_fixed(text), expected, "{text}");
    }
}

#[test]
fn type_first_reports_the_type_and_the_name() {
    assert_reports(
        "public decimal Deposit(decimal amount) { }",
        &[("MR0135", "decimal Deposit"), ("MR0135", "decimal amount")],
    );
    assert_reports("int x = 1, y;", &[("MR0135", "int x")]);
    // A C# construct reported whole is not reported again.
    assert_reports("delegate int Transform(int x);", &[("MR0105", "delegate")]);
    assert_reports("class B { event Action Clicked; }", &[("MR0105", "event")]);
    // Only what the event contains is withdrawn, not what came before it.
    assert_reports(
        "class B { int X; int Y; event Action Clicked; }",
        &[
            ("MR0135", "int X"),
            ("MR0135", "int Y"),
            ("MR0105", "event"),
        ],
    );
    assert_reports("using File f = Open();", &[("MR0121", "using")]);
    // Type patterns keep the C# order.
    assert_reports("let ok = s is Circle c;", &[]);
    assert_reports("switch (s) { case Circle c: break; }", &[]);
}

#[test]
fn type_first_keeps_the_declaration_nodes() {
    let tree = parse("class C { int X; int F(int a) => a; }")
        .syntax()
        .debug_tree();
    for kind in ["FieldDecl", "VariableDeclarator", "FunctionDecl", "Param"] {
        assert!(tree.contains(kind), "{kind}:\n{tree}");
    }
    let tree = parse("void F() { int x = 1; }").syntax().debug_tree();
    assert!(tree.contains("LocalDeclStmt"), "{tree}");
}

#[test]
fn void_as_a_type_is_reported() {
    assert_reports("Tick(): void { }", &[("MR0136", ": void")]);
    assert_eq!(fixed("Tick(): void { }\n"), "Tick() { }\n");
    assert_eq!(
        fixed("async Run() : void => Go();\n"),
        "async Run() => Go();\n"
    );
    assert_eq!(fixed("let u: void = ();\n"), "let u: () = ();\n");
    // `void?` and `void[]` are not `void` alone.
    assert_reports("let v: void? = none;", &[]);
}

#[test]
fn positional_struct_is_not_an_error() {
    assert_eq!(
        parse_errors("struct Point(X: int, Y: int) { public Sum: int => X + Y; }\n"),
        Vec::<String>::new()
    );
}

#[test]
fn primary_constructor_fixes_to_fields_and_a_constructor() {
    assert_eq!(
        fixed("class Point(x: int, y: int) { }\n"),
        "class Point {\n    x: int;\n    y: int;\n    public Point(x: int, y: int) { this.x = x; this.y = y; }\n}\n"
    );
    assert_eq!(
        fixed("    class Point(x: int);\n"),
        "    class Point\n    {\n        x: int;\n        public Point(x: int) { this.x = x; }\n    }\n"
    );
    assert_eq!(
        fixed(
            "class Bag<T>(items: Map<string, T>, n: int = Max(1, 2)) : IBag\n{\n    public Count: int => n;\n}\n"
        ),
        "class Bag<T> : IBag\n{\n    items: Map<string, T>;\n    n: int;\n    public Bag(items: Map<string, T>, n: int = Max(1, 2)) { this.items = items; this.n = n; }\n    public Count: int => n;\n}\n"
    );
    assert_eq!(fixed("class Unit() { }\n"), "class Unit { }\n");
}

#[test]
fn primary_constructor_with_modifiers_has_no_fix() {
    for text in [
        "class Point(ref x: int) { }",
        "class Point([Json] x: int) { }",
        "class Point(int x) { }",
        "class Point(x: int { }",
    ] {
        let parse = parse(text);
        let error = parse
            .errors()
            .iter()
            .find(|e| e.code == codes::MR0128)
            .unwrap_or_else(|| panic!("{text}: {:#?}", parse.errors()));
        assert!(error.fix.is_none(), "{text}");
    }
}

#[test]
fn record_fixes_to_struct() {
    assert_eq!(fixed("record User(Id: int);\n"), "struct User(Id: int);\n");
    assert_eq!(
        fixed("public record class Tag(Value: string) { }\n"),
        "public struct Tag(Value: string) { }\n"
    );
    assert_eq!(
        fixed("record struct Money(Cents: long);\n"),
        "struct Money(Cents: long);\n"
    );
}

#[test]
fn required_and_init_fix_away() {
    assert_eq!(
        fixed("class A { public required Host: string; }\n"),
        "class A { public Host: string; }\n"
    );
    assert_eq!(
        fixed("class A { public Port: int { get; init; } }\n"),
        "class A { public Port: int { get; } }\n"
    );
    assert_eq!(
        fixed("class A { public Port: int { init => x = value; get; } }\n"),
        "class A { public Port: int { get; } }\n"
    );
    assert_eq!(
        fixed("class A { public Port: int { init; } }\n"),
        "class A { public Port: int { get; } }\n"
    );
}
