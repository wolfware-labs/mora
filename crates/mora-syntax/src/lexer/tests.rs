use super::*;

/// The kinds and texts of the tokens of `text`, without whitespace.
fn tokens(text: &str) -> Vec<(SyntaxKind, &str)> {
    let lexed = lex(text);
    assert_lossless(text, &lexed);
    lexed
        .tokens
        .iter()
        .filter(|t| t.kind != SyntaxKind::Whitespace)
        .map(|t| (t.kind, &text[t.range]))
        .collect()
}

/// The codes and texts of the errors in `text`.
fn errors(text: &str) -> Vec<(Code, &str)> {
    let lexed = lex(text);
    assert_lossless(text, &lexed);
    lexed
        .errors
        .iter()
        .map(|e| (e.code, &text[e.range]))
        .collect()
}

fn assert_lossless(text: &str, lexed: &Lexed) {
    let mut end = TextSize::new(0);
    for token in &lexed.tokens {
        assert_eq!(
            token.range.start(),
            end,
            "gap or overlap before {token:?} in {text:?}"
        );
        assert!(!token.range.is_empty(), "empty {token:?} in {text:?}");
        end = token.range.end();
    }
    assert_eq!(end, size(text.len()), "tokens stop early in {text:?}");
}

use SyntaxKind as K;

#[test]
fn every_keyword_and_punctuation_lexes_as_itself() {
    for &kind in SyntaxKind::KEYWORDS.iter().chain(SyntaxKind::PUNCTUATION) {
        let text = kind.text().unwrap();
        assert_eq!(tokens(text), [(kind, text)], "{text}");
        assert!(errors(text).is_empty(), "{text}");
    }
    assert_eq!(SyntaxKind::KEYWORDS.len(), 94);
}

#[test]
fn contextual_keywords_are_identifiers() {
    for word in [
        "get", "set", "init", "value", "when", "and", "or", "not", "with", "record", "required",
        "file", "nameof", "global", "arena", "args", "code", "adds",
    ] {
        assert_eq!(tokens(word), [(K::Ident, word)]);
    }
}

#[test]
fn identifiers() {
    assert_eq!(
        tokens("_x9 @class café 𝑥 _"),
        [
            (K::Ident, "_x9"),
            (K::Ident, "@class"),
            (K::Ident, "café"),
            (K::Ident, "𝑥"),
            (K::Ident, "_"),
        ]
    );
    assert_eq!(ident_name("@class"), "class");
    // `é` precomposed and as `e` plus a combining acute accent.
    assert_eq!(tokens("cafe\u{301}"), [(K::Ident, "cafe\u{301}")]);
    assert_eq!(ident_name("cafe\u{301}"), ident_name("caf\u{e9}"));
    assert!(matches!(ident_name("caf\u{e9}"), Cow::Borrowed(_)));
}

#[test]
fn shift_operators_are_single_greater_thans() {
    assert_eq!(
        tokens("a >>= b >>> c"),
        [
            (K::Ident, "a"),
            (K::Gt, ">"),
            (K::Gt, ">"),
            (K::Eq, "="),
            (K::Ident, "b"),
            (K::Gt, ">"),
            (K::Gt, ">"),
            (K::Gt, ">"),
            (K::Ident, "c"),
        ]
    );
    let lexed = lex("List<List<int>> x");
    let kinds: Vec<_> = lexed.tokens.iter().map(|t| t.kind).collect();
    let first_gt = kinds.iter().position(|&k| k == K::Gt).unwrap();
    assert!(lexed.is_joined(first_gt));
    assert!(!lexed.is_joined(first_gt + 1));
    assert_eq!(
        tokens("x ? [1] : y?.z ?? w")[1..4],
        [(K::Question, "?"), (K::LBracket, "["), (K::IntNumber, "1"),]
    );
}

#[test]
fn numbers() {
    assert_eq!(
        tokens("1 0x4D2 0b1001 10ul 1.5 1e9 1.5e-3f 2d 1..5 1.ToString()"),
        [
            (K::IntNumber, "1"),
            (K::IntNumber, "0x4D2"),
            (K::IntNumber, "0b1001"),
            (K::IntNumber, "10ul"),
            (K::RealNumber, "1.5"),
            (K::RealNumber, "1e9"),
            (K::RealNumber, "1.5e-3f"),
            (K::RealNumber, "2d"),
            (K::IntNumber, "1"),
            (K::DotDot, ".."),
            (K::IntNumber, "5"),
            (K::IntNumber, "1"),
            (K::Dot, "."),
            (K::Ident, "ToString"),
            (K::LParen, "("),
            (K::RParen, ")"),
        ]
    );
    // As in C#, a real needs a digit after the dot: `1._5` is member access.
    assert_eq!(
        tokens("1._5"),
        [(K::IntNumber, "1"), (K::Dot, "."), (K::Ident, "_5")]
    );
    assert!(errors("1._5").is_empty());
    assert!(errors("`\n  `").is_empty());
    assert_eq!(
        errors("18446744073709551616 1e400 0xZ 1.5m"),
        [
            (codes::MR0010, "18446744073709551616"),
            (codes::MR0010, "1e400"),
            (codes::MR0014, "0xZ"),
            (codes::MR0014, "1.5m"),
        ]
    );
}

#[test]
fn comments_and_trivia() {
    let text = "\u{feff}a // line\r\n/// doc\n//// not doc\n/* block\n */b\r c";
    assert_eq!(
        tokens(text),
        [
            (K::Ident, "a"),
            (K::LineComment, "// line"),
            (K::Newline, "\r\n"),
            (K::DocComment, "/// doc"),
            (K::Newline, "\n"),
            (K::LineComment, "//// not doc"),
            (K::Newline, "\n"),
            (K::BlockComment, "/* block\n */"),
            (K::Ident, "b"),
            (K::Ident, "c"),
        ]
    );
    assert_eq!(lex("\u{feff}").tokens[0].kind, K::Whitespace);
    assert_eq!(errors("a\u{feff}b"), [(codes::MR0017, "\u{feff}")]);
    assert_eq!(lex("\r").tokens[0].kind, K::Whitespace);
    // Block comments do not nest (§3.2).
    assert_eq!(
        tokens("/* a /* b */ c */"),
        [
            (K::BlockComment, "/* a /* b */"),
            (K::Ident, "c"),
            (K::Star, "*"),
            (K::Slash, "/"),
        ]
    );
    assert_eq!(errors("x /* open"), [(codes::MR0012, "/*")]);
}

#[test]
fn directives_start_a_line() {
    assert_eq!(
        tokens("#if1\n#region_x")[0..2],
        [(K::Hash, "#"), (K::Ident, "if1")]
    );
    assert_eq!(tokens("#region_x")[1], (K::Ident, "region_x"));
    let text = "#region Setup\n  #endregion\nx #region\n#if DEBUG\r\n#Log(x)";
    assert_eq!(
        tokens(text),
        [
            (K::Directive, "#region Setup"),
            (K::Newline, "\n"),
            (K::Directive, "#endregion"),
            (K::Newline, "\n"),
            (K::Ident, "x"),
            (K::Hash, "#"),
            (K::Ident, "region"),
            (K::Newline, "\n"),
            (K::Directive, "#if DEBUG"),
            (K::Newline, "\r\n"),
            (K::Hash, "#"),
            (K::Ident, "Log"),
            (K::LParen, "("),
            (K::Ident, "x"),
            (K::RParen, ")"),
        ]
    );
}

#[test]
fn characters() {
    assert_eq!(
        tokens(r"'a' '𝄞' '\n' '\u{1F600}' '\''"),
        [
            (K::Char, "'a'"),
            (K::Char, "'𝄞'"),
            (K::Char, r"'\n'"),
            (K::Char, r"'\u{1F600}'"),
            (K::Char, r"'\''"),
        ]
    );
    assert_eq!(
        errors(r"'' 'ab' '\x41' '\q' 'a"),
        [
            (codes::MR0015, "''"),
            (codes::MR0015, "'ab'"),
            (codes::MR0011, r"\x41"),
            (codes::MR0013, r"\q"),
            (codes::MR0012, "'a"),
        ]
    );
    assert_eq!(tokens("'\n"), [(K::Char, "'"), (K::Newline, "\n")]);
    // A lone carriage return does not end the line (§3.1).
    assert_eq!(errors("'a\rb'"), [(codes::MR0015, "'a\rb'")]);
}

#[test]
fn strings() {
    let text = r#""a\tb" "x"u8 """#;
    assert_eq!(
        tokens(text),
        [
            (K::String, r#""a\tb""#),
            (K::String, r#""x"u8"#),
            (K::String, r#""""#),
        ]
    );
    assert!(errors(text).is_empty());
    assert_eq!(
        tokens(r#""a"u8x"#),
        [(K::String, r#""a""#), (K::Ident, "u8x")]
    );
    assert_eq!(errors("\"open\nx"), [(codes::MR0012, "\"open")]);
    // A backslash before the line break does not carry the string onto the next line.
    assert_eq!(
        tokens("\"a\\\nb\""),
        [
            (K::String, "\"a\\"),
            (K::Newline, "\n"),
            (K::Ident, "b"),
            (K::String, "\""),
        ]
    );
    assert_eq!(
        errors("'\\\n"),
        [(codes::MR0013, "\\"), (codes::MR0012, "'\\")]
    );
    assert_eq!(
        errors(r#""\x41 \z""#),
        [(codes::MR0011, r"\x41"), (codes::MR0013, r"\z")]
    );
}

#[test]
fn raw_strings() {
    let text = "`C:\\data` `one \"line\"` ``a`b```c`` `\n    a\n      b\n    `u8";
    assert_eq!(
        tokens(text),
        [
            (K::RawString, "`C:\\data`"),
            (K::RawString, "`one \"line\"`"),
            (K::RawString, "``a`b```c``"),
            (K::RawString, "`\n    a\n      b\n    `u8"),
        ]
    );
    assert!(errors(text).is_empty());
    assert_eq!(
        tokens("`a``b` ``"),
        [(K::RawString, "`a``b`"), (K::RawString, "``")]
    );
    assert_eq!(errors("`\n  a\n b\n  `"), [(codes::MR0016, " ")]);
    assert_eq!(errors("`x\n  `"), [(codes::MR0016, "`")]);
    assert_eq!(errors("`\n  x\n  y `"), [(codes::MR0016, "`")]);
    assert_eq!(errors("`\n  x"), [(codes::MR0012, "`\n  x")]);
    assert_eq!(errors("x = ``a`"), [(codes::MR0012, "``a`")]);
    assert!(errors("``\n  ` quoted\n  ``").is_empty());
}

#[test]
fn interpolated_strings() {
    assert_eq!(
        tokens(r#"$"x = {x}, {{y}} {a + b,5:F2}!""#),
        [
            (K::InterpStart, "$\""),
            (K::InterpText, "x = "),
            (K::HoleOpen, "{"),
            (K::Ident, "x"),
            (K::HoleClose, "}"),
            (K::InterpText, ", {{y}} "),
            (K::HoleOpen, "{"),
            (K::Ident, "a"),
            (K::Plus, "+"),
            (K::Ident, "b"),
            (K::Comma, ","),
            (K::IntNumber, "5"),
            (K::HoleFormat, ":F2"),
            (K::HoleClose, "}"),
            (K::InterpText, "!"),
            (K::InterpEnd, "\""),
        ]
    );
    assert_eq!(
        tokens(r#"$`{a}"` $"" $"{x::Y}""#),
        [
            (K::InterpStart, "$`"),
            (K::HoleOpen, "{"),
            (K::Ident, "a"),
            (K::HoleClose, "}"),
            (K::InterpText, "\""),
            (K::InterpEnd, "`"),
            (K::InterpStart, "$\""),
            (K::InterpEnd, "\""),
            (K::InterpStart, "$\""),
            (K::HoleOpen, "{"),
            (K::Ident, "x"),
            (K::ColonColon, "::"),
            (K::Ident, "Y"),
            (K::HoleClose, "}"),
            (K::InterpEnd, "\""),
        ]
    );
    // As in C#, a top-level `:` in a hole starts the format; a conditional
    // goes in parentheses.
    assert_eq!(
        tokens(r#"$"{x ? 1 : 2}""#)[4..6],
        [(K::IntNumber, "1"), (K::HoleFormat, ": 2")]
    );
    assert!(tokens(r#"$"{(x ? 1 : 2)}""#).contains(&(K::Colon, ":")));
}

#[test]
fn holes_nest() {
    let text = r#"$"a{f($"b{new[] { 1 }[0]}c", "d")}e""#;
    assert_eq!(
        tokens(text),
        [
            (K::InterpStart, "$\""),
            (K::InterpText, "a"),
            (K::HoleOpen, "{"),
            (K::Ident, "f"),
            (K::LParen, "("),
            (K::InterpStart, "$\""),
            (K::InterpText, "b"),
            (K::HoleOpen, "{"),
            (K::NewKw, "new"),
            (K::LBracket, "["),
            (K::RBracket, "]"),
            (K::LBrace, "{"),
            (K::IntNumber, "1"),
            (K::RBrace, "}"),
            (K::LBracket, "["),
            (K::IntNumber, "0"),
            (K::RBracket, "]"),
            (K::HoleClose, "}"),
            (K::InterpText, "c"),
            (K::InterpEnd, "\""),
            (K::Comma, ","),
            (K::String, "\"d\""),
            (K::RParen, ")"),
            (K::HoleClose, "}"),
            (K::InterpText, "e"),
            (K::InterpEnd, "\""),
        ]
    );
    assert!(errors(text).is_empty());
}

#[test]
fn raw_interpolated_strings() {
    let text = "$$`\n  {x} {{y}} {{{z}}}\n  `";
    assert_eq!(
        tokens(text),
        [
            (K::InterpStart, "$$`"),
            (K::InterpText, "\n  {x} "),
            (K::HoleOpen, "{{"),
            (K::Ident, "y"),
            (K::HoleClose, "}}"),
            (K::InterpText, " {"),
            (K::HoleOpen, "{{"),
            (K::Ident, "z"),
            (K::HoleClose, "}}"),
            (K::InterpText, "}\n  "),
            (K::InterpEnd, "`"),
        ]
    );
    assert!(errors(text).is_empty());
    assert_eq!(
        tokens("$``a`{b}\"``"),
        [
            (K::InterpStart, "$``"),
            (K::InterpText, "a`"),
            (K::HoleOpen, "{"),
            (K::Ident, "b"),
            (K::HoleClose, "}"),
            (K::InterpText, "\""),
            (K::InterpEnd, "``"),
        ]
    );
    // A hole may span lines without breaking the indentation rule.
    assert!(errors("$`\n  a{\nf(x)\n}\n  `").is_empty());
    assert!(errors("$`\n{x\n}\n`").is_empty());
    // A line that starts with a hole still needs the indentation.
    assert_eq!(errors("$`\n{x}\n  `"), [(codes::MR0016, "{")]);
    assert_eq!(errors("$`\n  a\n b\n  `"), [(codes::MR0016, " ")]);
    assert_eq!(errors("$`a {x}"), [(codes::MR0012, "$`")]);
}

#[test]
fn interpolation_errors() {
    assert_eq!(errors(r#"$"a}b""#), [(codes::MR0016, "}")]);
    assert_eq!(errors("$\"a {x\n"), [(codes::MR0012, "$\"")]);
    assert_eq!(errors("$\"open\nx"), [(codes::MR0012, "$\"")]);
    assert_eq!(errors(r#"$$"{{x}}""#), [(codes::MR0016, "$$\"")]);
    assert_eq!(errors(r#"$"\q{x}""#), [(codes::MR0013, r"\q")]);
    assert_eq!(
        errors("$$`{{{{x}}}}`"),
        [(codes::MR0016, "{{{{"), (codes::MR0016, "}}"),]
    );
}

#[test]
fn unknown_characters() {
    assert_eq!(
        errors("a \\ b \u{a0} c"),
        [(codes::MR0017, "\\"), (codes::MR0017, "\u{a0}")]
    );
    assert_eq!(tokens("\\")[0].0, K::Unknown);
    assert_eq!(tokens("`")[0].0, K::RawString);
}

/// Every `mora` example in the spec and the error pages, and every `.mr`
/// file under `tests/`, lexes losslessly; those that expect no error (no
/// `// error:` first line, no `//~ ERROR` annotation) lex without errors.
#[test]
fn repository_sources_lex_losslessly() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let mut sources = Vec::new();
    let mut docs = vec![root.join("docs/design/mora.md")];
    if let Ok(pages) = std::fs::read_dir(root.join("docs/errors")) {
        docs.extend(pages.map(|p| p.unwrap().path()));
    }
    for doc in docs {
        let text = std::fs::read_to_string(&doc).unwrap();
        let mut block: Option<String> = None;
        for line in text.lines() {
            match &mut block {
                None if line.trim_start() == "```mora" => block = Some(String::new()),
                Some(b) if line.trim_start() == "```" => {
                    sources.push((doc.display().to_string(), b.clone()));
                    block = None;
                }
                Some(b) => {
                    b.push_str(line);
                    b.push('\n');
                }
                None => {}
            }
        }
    }
    let mut dirs = vec![root.join("tests")];
    while let Some(dir) = dirs.pop() {
        for entry in std::fs::read_dir(dir).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                dirs.push(path);
            } else if path.extension().is_some_and(|e| e == "mr") {
                sources.push((
                    path.display().to_string(),
                    std::fs::read_to_string(&path).unwrap(),
                ));
            }
        }
    }
    assert!(sources.len() >= 20, "found only {} sources", sources.len());
    for (origin, text) in &sources {
        let lexed = lex(text);
        assert_lossless(text, &lexed);
        let expects_errors = text.starts_with("// error:") || text.contains("//~ ERROR");
        if !expects_errors {
            let problems: Vec<_> = lexed
                .errors
                .iter()
                .map(|e| (&e.message, &text[e.range]))
                .collect();
            assert!(problems.is_empty(), "{origin}: {problems:?}\n{text}");
        }
    }
}

/// Random strings over the characters that switch lexer modes never make
/// the lexer panic or lose text. `fuzz/` runs the whole parser longer, nightly.
#[test]
fn random_inputs_are_lossless() {
    const PIECES: &[&str] = &[
        "$", "@", "{", "}", "\"", "'", "\\", "/", "*", "\n", "\r", "#", "x", "u8", "0", "1e", ".",
        "_", ":", ",", " ", "é", "𝄞", "\u{301}", "region", "\"\"\"", "\\u{", "0x", "`",
    ];
    let mut state: u64 = 0x9E37_79B9_7F4A_7C15;
    let mut next = move || {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        state
    };
    for _ in 0..20_000 {
        let len = next() % 24;
        let text: String = (0..len)
            .map(|_| PIECES[usize::try_from(next() % PIECES.len() as u64).unwrap()])
            .collect();
        let lexed = lex(&text);
        assert_lossless(&text, &lexed);
        for error in &lexed.errors {
            assert!(error.range.end() <= size(text.len()), "{text:?}: {error:?}");
        }
    }
}

// Behavior found untested by cargo mutants (§20.5).

#[test]
fn hole_format_stops_at_newline_only_in_plain_strings() {
    assert_eq!(
        tokens("$\"{x:F2\n"),
        [
            (K::InterpStart, "$\""),
            (K::HoleOpen, "{"),
            (K::Ident, "x"),
            (K::HoleFormat, ":F2"),
            (K::Newline, "\n"),
        ]
    );
    assert_eq!(
        tokens("$@\"{x:a\nb}\""),
        [
            (K::InterpStart, "$@\""),
            (K::HoleOpen, "{"),
            (K::Ident, "x"),
            (K::HoleFormat, ":a\nb"),
            (K::HoleClose, "}"),
            (K::InterpEnd, "\""),
        ]
    );
    assert_eq!(
        tokens("$\"\"\"{x:a\nb}\"\"\""),
        [
            (K::InterpStart, "$\"\"\""),
            (K::HoleOpen, "{"),
            (K::Ident, "x"),
            (K::HoleFormat, ":a\nb"),
            (K::HoleClose, "}"),
            (K::InterpEnd, "\"\"\""),
        ]
    );
}

#[test]
fn hole_format_stops_at_a_quote_except_in_raw_strings() {
    assert_eq!(
        tokens("$\"{x:F2\""),
        [
            (K::InterpStart, "$\""),
            (K::HoleOpen, "{"),
            (K::Ident, "x"),
            (K::HoleFormat, ":F2"),
            (K::String, "\""),
        ]
    );
    assert_eq!(
        tokens("$\"\"\"{x:a\"b}\"\"\""),
        [
            (K::InterpStart, "$\"\"\""),
            (K::HoleOpen, "{"),
            (K::Ident, "x"),
            (K::HoleFormat, ":a\"b"),
            (K::HoleClose, "}"),
            (K::InterpEnd, "\"\"\""),
        ]
    );
    assert_eq!(
        tokens("$`{x:a\"b\nc}`"),
        [
            (K::InterpStart, "$`"),
            (K::HoleOpen, "{"),
            (K::Ident, "x"),
            (K::HoleFormat, ":a\"b\nc"),
            (K::HoleClose, "}"),
            (K::InterpEnd, "`"),
        ]
    );
}

#[test]
fn a_string_ends_before_a_crlf_but_not_at_a_lone_cr() {
    assert_eq!(
        tokens("\"ab\r\nc"),
        [(K::String, "\"ab"), (K::Newline, "\r\n"), (K::Ident, "c")]
    );
    assert_eq!(errors("\"ab\r\nc"), [(codes::MR0012, "\"ab")]);
    assert_eq!(tokens("\"a\rb\""), [(K::String, "\"a\rb\"")]);
    assert!(errors("\"a\rb\"").is_empty());
}

#[test]
fn verbatim_string_takes_a_u8_suffix() {
    assert_eq!(
        tokens("@\"a\"u8;"),
        [(K::VerbatimString, "@\"a\"u8"), (K::Semicolon, ";")]
    );
}

#[test]
fn backslashes_are_text_in_verbatim_and_raw_interpolated_strings() {
    assert!(errors("$`\\q`").is_empty());
    assert_eq!(errors("$@\"\\q\""), [(codes::MR0018, "$@\"")]);
    assert_eq!(errors("$\"\"\"\\q\"\"\""), [(codes::MR0018, "$\"\"\"")]);
    assert_eq!(errors("$\"\\q\""), [(codes::MR0013, "\\q")]);
}

#[test]
fn a_newline_ends_a_plain_interpolated_string() {
    assert_eq!(
        tokens("$\"a\nb"),
        [
            (K::InterpStart, "$\""),
            (K::InterpText, "a"),
            (K::Newline, "\n"),
            (K::Ident, "b"),
        ]
    );
    assert_eq!(errors("$\"a\nb"), [(codes::MR0012, "$\"")]);
}

#[test]
fn raw_interpolated_string_error_ranges() {
    assert_eq!(
        errors("x = $\"\"\"ab\"\"\"\""),
        [(codes::MR0016, "\"\"\"\""), (codes::MR0018, "$\"\"\"")]
    );
    assert_eq!(errors("x = $`ab{{x}`"), [(codes::MR0016, "{{")]);
}

#[test]
fn raw_string_lines_after_a_hole_are_checked() {
    let text = "$`\n    {x}\n  bad\n    `";
    assert_eq!(errors(text), [(codes::MR0016, "  ")]);
}

#[test]
fn raw_string_opening_line_may_end_in_whitespace() {
    assert!(errors("x = `   \n  a\n  `").is_empty());
}

#[test]
fn raw_string_checks_a_short_last_line() {
    assert_eq!(errors("`\n  a\nb\n  `"), [(codes::MR0016, "b")]);
}

/// `text` with the fix of its last error, an `MR0018`, applied.
fn legacy_fixed(text: &str) -> String {
    let lexed = lex(text);
    let error = lexed.errors.last().expect("an error");
    assert_eq!(error.code, codes::MR0018, "{text}");
    error.fix.as_ref().expect("a fix").apply(text)
}

#[test]
fn legacy_strings_are_errors_with_a_backtick_fix() {
    for (text, fixed) in [
        (r#""""a "b" c""""#, r#"`a "b" c`"#),
        (
            "\"\"\"\n    a\n      b\n    \"\"\"",
            "`\n    a\n      b\n    `",
        ),
        ("\"\"\"a\"\"\"u8", "`a`u8"),
        ("\"\"\"`a\"\"\"", "``\n`a\n``"),
        ("\"\"\"a``b\"\"\"", "```a``b```"),
        ("\"\"\"\"a\"\"\"b\"\"\"\"", "`a\"\"\"b`"),
        (r#"@"C:\temp\""x""""#, r#"`C:\temp\"x"`"#),
        ("@\"\"", "`\n`"),
        ("@\"a\n  b\"", "`\na\n  b\n`"),
        ("@\"a\"u8", "`a`u8"),
        ("@\"{{a}}\"", "`{{a}}`"),
        ("@\"a`\"", "``\na`\n``"),
        ("@\"a\r\"", "`a\r`"),
    ] {
        assert_eq!(legacy_fixed(text), fixed, "{text}");
        assert!(errors(fixed).is_empty(), "{fixed}");
        assert_eq!(
            literal::string_value(fixed),
            literal::string_value(text),
            "{text}"
        );
    }
}

#[test]
fn legacy_interpolated_strings_are_errors_with_a_backtick_fix() {
    for (text, fixed) in [
        ("$\"\"\"a {x} b\"\"\"", "$`a {x} b`"),
        ("$$\"\"\"{{x}} {y}\"\"\"", "$$`{{x}} {y}`"),
        ("$\"\"\"\n  {x}\n  \"\"\"", "$`\n  {x}\n  `"),
        (r#"$@"{a}""x""""#, r#"$`{a}"x"`"#),
        ("@$\"{a} {{b}}\"", "$$`{{a}} {b}`"),
        ("$@\"{{{a}}}\"", "$$`{{{a}}}`"),
        ("$@\"{a}{b}{{\"", "$$`{{a}}{{b}}{`"),
        ("$@\"x{a,5:F2}`\"", "$``\nx{a,5:F2}`\n``"),
        ("$@\"{x:a\nb}\"", "$`\n{x:a\nb}\n`"),
        ("$@\"\"", "$`\n`"),
    ] {
        assert_eq!(legacy_fixed(text), fixed, "{text}");
        assert!(errors(fixed).is_empty(), "{fixed}");
        assert_eq!(holes(fixed), holes(text), "{text} -> {fixed}");
    }
}

/// The holes of an interpolated string: their tokens, without the text
/// between them and with hole braces counted once, however many there are.
fn holes(text: &str) -> Vec<(SyntaxKind, &str)> {
    tokens(text)
        .into_iter()
        .filter(|(kind, _)| !matches!(kind, K::InterpStart | K::InterpText | K::InterpEnd))
        .map(|(kind, text)| match kind {
            K::HoleOpen | K::HoleClose => (kind, ""),
            _ => (kind, text),
        })
        .collect()
}

#[test]
fn legacy_strings_nested_in_holes_get_fixes_of_their_own() {
    let text = "$@\"{{{F($@\"{y}\"\"\")}\"";
    assert_eq!(
        errors(text),
        [(codes::MR0018, "$@\""), (codes::MR0018, "$@\"")]
    );
    let outer = legacy_fixed(text);
    assert_eq!(outer, "$$`{{{F($@\"{y}\"\"\")}}`");
    assert_eq!(legacy_fixed(&outer), "$$`{{{F($`{y}\"`)}}`");
    assert!(errors(&legacy_fixed(&outer)).is_empty());
}

#[test]
fn legacy_strings_report_what_mora_uses() {
    let lexed = lex("x = \"\"\"a\"\"\" + @\"b\" + $\"\"\"c\"\"\" + @$\"d\";");
    let messages: Vec<_> = lexed.errors.iter().map(|e| e.message.as_str()).collect();
    assert_eq!(
        messages,
        [
            "`\"\"\"` strings are written with backticks in Mora",
            "`@\"...\"` strings are written with backticks in Mora",
            "`$\"\"\"` strings are written with backticks in Mora",
            "`@$\"...\"` strings are written with backticks in Mora",
        ]
    );
    let error = &lexed.errors[0];
    assert!(error.note.as_ref().unwrap().contains("between backticks"));
    assert_eq!(
        error.fix.as_ref().unwrap().message,
        "write it with backticks"
    );
}

#[test]
fn legacy_strings_with_other_errors_get_no_fix() {
    for text in [
        "\"\"\"a\"\"\"\"",
        "\"\"\"a\n  \"\"\"",
        "$$@\"{x}\"",
        "$@\"a}\"",
        "$@\"{\"\\q\"}\"",
        "@\"\n\r\"",
    ] {
        let lexed = lex(text);
        let error = lexed.errors.last().unwrap();
        assert_eq!(error.code, codes::MR0018, "{text}");
        assert_eq!(error.fix, None, "{text}");
    }
    // A fix may follow a nested string's own `MR0018`, but no other error.
    assert!(lex("$@\"{@\"a\"}\"").errors[1].fix.is_some());
}

#[test]
fn unclosed_legacy_strings_are_only_unclosed() {
    assert_eq!(errors("@\"open\n"), [(codes::MR0012, "@\"open\n")]);
    assert_eq!(errors("\"\"\"\n  x"), [(codes::MR0012, "\"\"\"\n  x")]);
    assert_eq!(errors("$@\"a"), [(codes::MR0012, "$@\"")]);
}
