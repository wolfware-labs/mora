// Test code may unwrap (CONTRIBUTING.md, Code rules).
#![allow(clippy::unwrap_used, clippy::expect_used)]

//! Parser snapshot tests (M1.3). Each `tests/parser/ok/*.mr` and
//! `tests/parser/err/*.mr` file sits next to a `.rast` file holding its tree
//! and its errors. `ok` files must parse without errors. To update the
//! snapshots after a deliberate change, run
//! `MORA_BLESS=1 cargo test -p mora-syntax --test parser` and review the diff.

use std::path::Path;

use mora_syntax::ast::{self, AstNode};
use mora_syntax::parse;

fn render(text: &str) -> String {
    let parse = parse(text);
    let mut out = parse.syntax().debug_tree();
    for error in parse.errors() {
        out.push_str(&format!(
            "error {} {:?}: {}\n",
            error.code, error.range, error.message
        ));
    }
    out
}

#[test]
fn snapshots() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/parser");
    let bless = std::env::var_os("MORA_BLESS").is_some();
    let mut failures = Vec::new();
    let mut count = 0;
    for group in ["ok", "err"] {
        let mut files: Vec<_> = std::fs::read_dir(dir.join(group))
            .unwrap()
            .map(|entry| entry.unwrap().path())
            .filter(|path| path.extension().is_some_and(|ext| ext == "mr"))
            .collect();
        files.sort();
        for path in files {
            count += 1;
            let text = std::fs::read_to_string(&path).unwrap();
            let parse = parse(&text);
            assert_eq!(
                parse.syntax().text(),
                text,
                "{}: not lossless",
                path.display()
            );
            let has_errors = !parse.errors().is_empty();
            if (group == "ok") == has_errors {
                failures.push(format!(
                    "{}: {}",
                    path.display(),
                    if has_errors {
                        "unexpected errors"
                    } else {
                        "expected errors"
                    }
                ));
            }
            let rendered = render(&text);
            let snapshot = path.with_extension("rast");
            if bless {
                std::fs::write(&snapshot, &rendered).unwrap();
            } else if std::fs::read_to_string(&snapshot).ok().as_deref() != Some(rendered.as_str())
            {
                failures.push(format!(
                    "{}: snapshot differs; run MORA_BLESS=1 cargo test -p mora-syntax --test parser\n{rendered}",
                    path.display()
                ));
            }
        }
    }
    assert!(count >= 10, "found only {count} parser tests");
    assert!(failures.is_empty(), "{}", failures.join("\n\n"));
}

fn class_members(text: &str) -> Vec<String> {
    let file = parse(text).tree();
    let Some(ast::TopLevelItem::Item(ast::Item::ClassDecl(class))) = file.top_level_items().next()
    else {
        panic!("no class in {text}");
    };
    class
        .member_list()
        .unwrap()
        .members()
        .map(|member| {
            let syntax = member.syntax();
            let name = syntax
                .descendants()
                .find(|n| n.kind() == mora_syntax::SyntaxKind::Name)
                .map(|n| n.text())
                .unwrap_or_default();
            format!("{:?} {name}", syntax.kind())
        })
        .collect()
}

#[test]
fn a_missing_semicolon_keeps_the_next_members() {
    let text = "class A { int X int Y; void M() { } }";
    assert_eq!(
        class_members(text),
        ["FieldDecl X", "FieldDecl Y", "FunctionDecl M"]
    );
}

#[test]
fn a_missing_brace_keeps_the_next_members() {
    let text = "class B {\n public void First() {\n Work();\n public void Second() { }\n}";
    assert_eq!(
        class_members(text),
        ["FunctionDecl First", "FunctionDecl Second"]
    );
}

#[test]
fn garbage_between_members_is_skipped() {
    let text = "class C { int X; ) ) ; int Y; }";
    assert_eq!(class_members(text), ["FieldDecl X", "FieldDecl Y"]);
}

#[test]
fn every_spec_example_parses_without_panicking() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let spec = std::fs::read_to_string(root.join("docs/design/mora.md")).unwrap();
    let mut block: Option<String> = None;
    let mut count = 0;
    for line in spec.lines() {
        match &mut block {
            None if line.trim_start() == "```mora" => block = Some(String::new()),
            Some(text) if line.trim_start() == "```" => {
                assert_eq!(parse(text).syntax().text(), *text);
                count += 1;
                block = None;
            }
            Some(text) => {
                text.push_str(line);
                text.push('\n');
            }
            None => {}
        }
    }
    assert!(count >= 20, "{count}");
}
