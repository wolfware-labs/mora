// Helpers outside `#[test]` functions are not covered by `allow-unwrap-in-tests`.
#![allow(clippy::unwrap_used)]

use mora_diag::{
    Code, Diagnostic, Files, Fix, JsonDiagnostic, JsonSpan, Label, Severity, TextEdit, render,
    to_json_line,
};
use mora_span::{Span, TextRange};

fn sample(files: &mut Files) -> Diagnostic {
    let a = files.add("src/A.mr", "let é = x;\r\nlet y = 2;\n");
    let b = files.add("src/B.mr", "let x = 1;\n");
    let span =
        |file, start: u32, end: u32| Span::new(file, TextRange::new(start.into(), end.into()));
    Diagnostic::error(
        Code::new(201),
        "`x` is not in scope",
        Label::new(span(a, 9, 10), "not found"),
    )
    .with_secondary(Label::unlabeled(span(b, 4, 5)))
    .with_note("names from another file need a `using`")
    .with_fix(Fix::new(
        "import it",
        vec![
            TextEdit::insert(a, 0.into(), "using B;\n"),
            TextEdit::replace(span(b, 4, 5), "z"),
        ],
    ))
}

#[test]
fn round_trips_through_serde_json() {
    let mut files = Files::new();
    let line = to_json_line(&sample(&mut files), &files);
    assert!(!line.contains('\n'), "{line}");
    let parsed: JsonDiagnostic = serde_json::from_str(&line).unwrap();
    assert_eq!(
        parsed,
        JsonDiagnostic::new(&sample(&mut Files::new()), &files)
    );
    assert_eq!(serde_json::to_string(&parsed).unwrap(), line);
}

#[test]
fn has_every_field_of_the_spec() {
    let mut files = Files::new();
    let value: serde_json::Value =
        serde_json::from_str(&to_json_line(&sample(&mut files), &files)).unwrap();
    let keys = |v: &serde_json::Value| {
        let mut keys: Vec<String> = v.as_object().unwrap().keys().cloned().collect();
        keys.sort();
        keys
    };
    assert_eq!(
        keys(&value),
        [
            "code",
            "fixes",
            "message",
            "note",
            "primary",
            "secondary",
            "severity"
        ]
    );
    assert_eq!(value["code"], "MR0201");
    assert_eq!(value["severity"], "error");
    assert_eq!(value["note"], "names from another file need a `using`");
    assert_eq!(keys(&value["primary"]), ["label", "span"]);
    assert_eq!(
        keys(&value["primary"]["span"]),
        [
            "byte_end",
            "byte_start",
            "column_end",
            "column_start",
            "file",
            "line_end",
            "line_start"
        ]
    );
    assert_eq!(value["secondary"][0]["label"], serde_json::Value::Null);
    assert_eq!(keys(&value["fixes"][0]), ["edits", "message"]);
    assert_eq!(
        keys(&value["fixes"][0]["edits"][1]),
        ["replacement", "span"]
    );
    assert_eq!(value["fixes"][0]["edits"][1]["span"]["file"], "src/B.mr");
}

#[test]
fn positions_are_one_based_character_columns() {
    let mut files = Files::new();
    let json = JsonDiagnostic::new(&sample(&mut files), &files);
    // Eight characters but nine bytes precede `x`: column 9, where a byte column would be 10.
    assert_eq!(
        json.primary.span,
        JsonSpan {
            file: "src/A.mr".into(),
            byte_start: 9,
            byte_end: 10,
            line_start: 1,
            column_start: 9,
            line_end: 1,
            column_end: 10,
        }
    );
    assert_eq!(json.severity, Severity::Error);
    assert_eq!(json.fixes[0].edits[0].span.byte_end, 0);
}

#[test]
fn rejects_malformed_codes() {
    let mut files = Files::new();
    let line = to_json_line(&sample(&mut files), &files).replace("MR0201", "MR201");
    assert!(serde_json::from_str::<JsonDiagnostic>(&line).is_err());
}

#[test]
fn bad_spans_are_clamped_onto_the_text() {
    let mut files = Files::new();
    let file = files.add("src/C.mr", "é\n");
    let span = Span::new(file, TextRange::new(1.into(), 50.into()));
    let json = JsonDiagnostic::new(
        &Diagnostic::error(Code::new(9000), "bad", Label::unlabeled(span)),
        &files,
    );
    let s = json.primary.span;
    assert_eq!((s.byte_start, s.byte_end), (0, 3));
    assert_eq!(
        (s.line_start, s.column_start, s.line_end, s.column_end),
        (1, 1, 2, 1)
    );
}

#[test]
fn wide_characters_give_the_same_column_as_the_terminal_header() {
    // `日` and `本` are two columns wide on screen but count as one each.
    let mut files = Files::new();
    let text = "let 日本 = x;";
    let file = files.add("src/W.mr", text);
    let start = u32::try_from(text.find('x').unwrap()).unwrap();
    let span = Span::new(file, TextRange::new(start.into(), (start + 1).into()));
    let diagnostic = Diagnostic::error(Code::new(201), "not in scope", Label::unlabeled(span));
    let json = JsonDiagnostic::new(&diagnostic, &files);
    assert_eq!(
        (json.primary.span.line_start, json.primary.span.column_start),
        (1, 10)
    );
    assert!(render(&diagnostic, &files, false).contains("--> src/W.mr:1:10\n"));
}
