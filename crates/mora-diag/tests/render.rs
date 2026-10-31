// Helpers outside `#[test]` functions are not covered by `allow-unwrap-in-tests`.
#![allow(clippy::unwrap_used)]

use mora_diag::{Code, Diagnostic, Files, Fix, Label, TextEdit, render, to_json_line};
use mora_span::{FileId, Span, TextRange, TextSize};

const POOL: &str = "\
using Std.Collections;

namespace App;
public unique class Connection
{
}

public class ConnectionPool
{
    private _idle: List<Connection> = new();

    public Return(conn: Connection)
    {
        _idle.Add(conn);
    }
}
";

fn span_of(file: FileId, text: &str, needle: &str, nth: usize) -> Span {
    let start = text.match_indices(needle).nth(nth).unwrap().0;
    let start = TextSize::try_from(start).unwrap();
    let len = TextSize::try_from(needle.len()).unwrap();
    Span::new(file, TextRange::at(start, len))
}

/// The `MR0310` example of §17.6, built from a file whose lines 12 to 14
/// are the ones the spec shows.
fn mr0310(files: &mut Files) -> Diagnostic {
    let file = files.add("src/Pool.mr", POOL);
    let param = span_of(file, POOL, "conn: Connection", 0);
    let stored = span_of(file, POOL, "conn", 1);
    Diagnostic::error(
        Code::new(310),
        "cannot store `conn` because `Connection` is unique",
        Label::new(
            stored,
            "storing it would leave the caller with a connection it no longer owns",
        ),
    )
    .with_secondary(Label::new(param, "`conn` is only borrowed for this call"))
    .with_fix(Fix::new(
        "take ownership of the parameter",
        vec![TextEdit::insert(file, param.range.start(), "move ")],
    ))
}

/// The first code block of the given language in §17.6.
fn spec_example(language: &str) -> &'static str {
    let spec = include_str!("../../../docs/design/mora.md");
    let section = &spec[spec.find("### 17.6 Diagnostics").unwrap()..];
    let fence = format!("```{language}\n");
    let block = &section[section.find(&fence).unwrap() + fence.len()..];
    &block[..block.find("```").unwrap()]
}

#[test]
fn renders_the_spec_example_byte_for_byte() {
    let mut files = Files::new();
    let diagnostic = mr0310(&mut files);
    let rendered = render(&diagnostic, &files, false);
    assert_eq!(format!("{rendered}\n"), spec_example("text"));
}

#[test]
fn prints_the_spec_json_example() {
    let mut files = Files::new();
    let diagnostic = mr0310(&mut files);
    let line = to_json_line(&diagnostic, &files);
    assert_eq!(format!("{line}\n"), spec_example("json"));
}

#[test]
fn color_adds_escape_codes_only() {
    let mut files = Files::new();
    let diagnostic = mr0310(&mut files);
    let colored = render(&diagnostic, &files, true);
    assert!(colored.contains("\x1b["));
    assert_eq!(strip_ansi(&colored), render(&diagnostic, &files, false));
}

fn strip_ansi(text: &str) -> String {
    let mut out = String::new();
    let mut chars = text.chars();
    while let Some(c) = chars.next() {
        if c == '\x1b' {
            for c in chars.by_ref() {
                if c.is_ascii_alphabetic() {
                    break;
                }
            }
        } else {
            out.push(c);
        }
    }
    out
}

#[test]
fn warning_with_note_and_a_label_in_another_file() {
    let mut files = Files::new();
    let a = files.add("src/A.mr", "let x = 1;\n");
    let b = files.add("src/B.mr", "let x = 2;\n");
    let diagnostic = Diagnostic::warning(
        Code::new(2001),
        "`x` is declared twice",
        Label::new(span_of(a, "let x = 1;\n", "x", 0), "second declaration"),
    )
    .with_secondary(Label::new(
        span_of(b, "let x = 2;\n", "x", 0),
        "first declared here",
    ))
    .with_note("each file in a project shares one namespace");
    let expected = "\
warning[MR2001]: `x` is declared twice
 --> src/A.mr:1:5
  |
1 | let x = 1;
  |     ^ second declaration
  |
 ::: src/B.mr:1:5
  |
1 | let x = 2;
  |     - first declared here
  |
  = note: each file in a project shares one namespace";
    assert_eq!(render(&diagnostic, &files, false), expected);
}

#[test]
fn columns_count_characters_in_crlf_files() {
    let mut files = Files::new();
    let text = "a\r\nlet é = ;\r\n";
    let file = files.add("src/C.mr", text);
    let diagnostic = Diagnostic::error(
        Code::new(110),
        "expected an expression",
        Label::new(
            span_of(file, text, ";", 0),
            "expression expected before `;`",
        ),
    );
    let expected = "\
error[MR0110]: expected an expression
 --> src/C.mr:2:9
  |
2 | let é = ;
  |         ^ expression expected before `;`";
    assert_eq!(render(&diagnostic, &files, false), expected);
}

#[test]
fn spans_out_of_range_or_inside_a_character_do_not_panic() {
    let mut files = Files::new();
    let text = "let é = 1;";
    let file = files.add("src/D.mr", text);
    let span = |start: u32, end: u32| Span::new(file, TextRange::new(start.into(), end.into()));
    for (start, end) in [(5, 6), (6, 6), (5, 100), (100, 200), (0, u32::MAX)] {
        let diagnostic = Diagnostic::error(
            Code::new(9000),
            "bad span",
            Label::new(span(start, end), "here"),
        )
        .with_fix(Fix::new(
            "replace it",
            vec![TextEdit::replace(span(start, end), "x")],
        ));
        let rendered = render(&diagnostic, &files, false);
        assert!(
            rendered.starts_with("error[MR9000]: bad span"),
            "{rendered}"
        );
    }
}

#[test]
fn unknown_file_renders_the_header() {
    let files = Files::new();
    let span = Span::new(FileId::new(7), TextRange::new(3.into(), 5.into()));
    let diagnostic = Diagnostic::error(Code::new(9001), "lost file", Label::unlabeled(span));
    let rendered = render(&diagnostic, &files, false);
    assert!(
        rendered.starts_with("error[MR9001]: lost file"),
        "{rendered}"
    );
}
