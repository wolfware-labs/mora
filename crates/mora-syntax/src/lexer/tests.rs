use super::*;

/// The kinds and texts of the tokens of `text`, without whitespace.
fn tokens(text: &str) -> Vec<(TokenKind, &str)> {
    let lexed = lex(text);
    assert_lossless(text, &lexed);
    lexed
        .tokens
        .iter()
        .filter(|t| t.kind != TokenKind::Whitespace)
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

use TokenKind as K;

#[test]
fn every_keyword_and_punctuation_lexes_as_itself() {
    for &kind in TokenKind::KEYWORDS.iter().chain(TokenKind::PUNCTUATION) {
        let text = kind.text().unwrap();
        assert_eq!(tokens(text), [(kind, text)], "{text}");
        assert!(errors(text).is_empty(), "{text}");
    }
    assert_eq!(TokenKind::KEYWORDS.len(), 93);
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
}

#[test]
fn strings() {
    let text = r#""a\tb" "x"u8 @"C:\temp\""x""" "" @"""#;
    assert_eq!(
        tokens(text),
        [
            (K::String, r#""a\tb""#),
            (K::String, r#""x"u8"#),
            (K::VerbatimString, r#"@"C:\temp\""x""""#),
            (K::String, r#""""#),
            (K::VerbatimString, r#"@"""#),
        ]
    );
    assert!(errors(text).is_empty());
    assert_eq!(
        tokens(r#""a"u8x"#),
        [(K::String, r#""a""#), (K::Ident, "u8x")]
    );
    assert_eq!(errors("\"open\nx"), [(codes::MR0012, "\"open")]);
    assert_eq!(
        errors(r#""\x41 \z""#),
        [(codes::MR0011, r"\x41"), (codes::MR0013, r"\z")]
    );
    assert_eq!(errors("@\"open\n"), [(codes::MR0012, "@\"open\n")]);
    assert!(errors("@\"two\nlines\"").is_empty());
}

#[test]
fn raw_strings() {
    let text = "\"\"\"one \"line\"\"\"\" \"\"\"\n    a\n      b\n    \"\"\"u8";
    assert_eq!(
        tokens(text),
        [
            (K::RawString, "\"\"\"one \"line\"\"\"\""),
            (K::RawString, "\"\"\"\n    a\n      b\n    \"\"\"u8"),
        ]
    );
    // The first token closes with four quotes after opening with three.
    assert_eq!(errors(text), [(codes::MR0016, "\"\"\"\"")]);
    assert_eq!(errors("\"\"\"\n  a\n b\n  \"\"\""), [(codes::MR0016, " ")]);
    assert_eq!(errors("\"\"\"x\n  \"\"\""), [(codes::MR0016, "\"\"\"")]);
    assert_eq!(
        errors("\"\"\"\n  x\n  y \"\"\""),
        [(codes::MR0016, "\"\"\"")]
    );
    assert_eq!(errors("\"\"\"\n  x"), [(codes::MR0012, "\"\"\"\n  x")]);
    assert!(errors("\"\"\"\"\n  \"\"\" quoted\n  \"\"\"\"").is_empty());
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
        tokens(r#"$@"{a}""" $"" $"{x::Y}""#),
        [
            (K::InterpStart, "$@\""),
            (K::HoleOpen, "{"),
            (K::Ident, "a"),
            (K::HoleClose, "}"),
            (K::InterpText, "\"\""),
            (K::InterpEnd, "\""),
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
    assert_eq!(tokens(r#"@$"a""#)[0], (K::InterpStart, "@$\""));
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
    let text = "$$\"\"\"\n  {x} {{y}} {{{z}}}\n  \"\"\"";
    assert_eq!(
        tokens(text),
        [
            (K::InterpStart, "$$\"\"\""),
            (K::InterpText, "\n  {x} "),
            (K::HoleOpen, "{{"),
            (K::Ident, "y"),
            (K::HoleClose, "}}"),
            (K::InterpText, " {"),
            (K::HoleOpen, "{{"),
            (K::Ident, "z"),
            (K::HoleClose, "}}"),
            (K::InterpText, "}\n  "),
            (K::InterpEnd, "\"\"\""),
        ]
    );
    assert!(errors(text).is_empty());
    // A hole may span lines without breaking the indentation rule.
    assert!(errors("$\"\"\"\n  a{\nf(x)\n}\n  \"\"\"").is_empty());
    assert_eq!(errors("$\"\"\"\n  a\n b\n  \"\"\""), [(codes::MR0016, " ")]);
}

#[test]
fn interpolation_errors() {
    assert_eq!(errors(r#"$"a}b""#), [(codes::MR0016, "}")]);
    assert_eq!(errors("$\"a {x\n"), [(codes::MR0012, "$\"")]);
    assert_eq!(errors("$\"open\nx"), [(codes::MR0012, "$\"")]);
    assert_eq!(errors(r#"$$"{{x}}""#), [(codes::MR0016, "$$\"")]);
    assert_eq!(errors(r#"$"\q{x}""#), [(codes::MR0013, r"\q")]);
    assert_eq!(
        errors("$$\"\"\"{{{{x}}}}\"\"\""),
        [(codes::MR0016, "{{{{"), (codes::MR0016, "}}"),]
    );
}

#[test]
fn unknown_characters() {
    assert_eq!(
        errors("a ` b \u{a0} c"),
        [(codes::MR0017, "`"), (codes::MR0017, "\u{a0}")]
    );
    assert_eq!(tokens("`")[0].0, K::Unknown);
}

/// Every `mora` example in the spec and the error pages, and every `.mr`
/// file under `tests/`, lexes losslessly; examples that expect output lex
/// without errors.
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
        let expects_errors = text.starts_with("// error:");
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
/// the lexer panic or lose text. M1.4 adds a real fuzz target.
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
