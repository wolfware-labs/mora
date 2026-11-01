//! UI tests: a `.mr` file whose lines carry `//~ ERROR MRnnnn` or
//! `//~ WARNING MRnnnn` where a diagnostic must be reported, next to a
//! `.stderr` snapshot of the rendered output.

use mora_diag::{Code, Severity, Sources, render};
use mora_span::FileId;

use crate::compiler::{Checked, Compiler};
use crate::diff::diff;

/// A diagnostic a line expects.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Annotation {
    /// The line the annotation is on, one-based.
    pub line: usize,
    /// `ERROR` or `WARNING`.
    pub severity: Severity,
    /// The code the diagnostic must have.
    pub code: Code,
}

/// The annotations of one line, in order. Text after the code is a comment.
/// A `//~` that is not followed by `ERROR` or `WARNING` and a code gives an
/// error naming what it found.
pub fn line_annotations(line: &str) -> Vec<Result<(Severity, Code), String>> {
    line.match_indices("//~")
        .map(|(at, _)| {
            let mut words = line[at + 3..].split_whitespace();
            let severity = match words.next() {
                Some("ERROR") => Severity::Error,
                Some("WARNING") => Severity::Warning,
                other => {
                    return Err(format!(
                        "`//~` must be followed by ERROR or WARNING, not `{}`",
                        other.unwrap_or("")
                    ));
                }
            };
            let Some(code) = words.next() else {
                return Err("`//~` needs a code after the severity".to_owned());
            };
            code.parse()
                .map(|code| (severity, code))
                .map_err(|error| format!("{error}"))
        })
        .collect()
}

/// The annotations of a file, or the problems with malformed ones.
pub fn annotations(text: &str) -> Result<Vec<Annotation>, Vec<String>> {
    let mut found = Vec::new();
    let mut problems = Vec::new();
    for (index, line) in text.lines().enumerate() {
        for annotation in line_annotations(line) {
            match annotation {
                Ok((severity, code)) => found.push(Annotation {
                    line: index + 1,
                    severity,
                    code,
                }),
                Err(problem) => problems.push(format!("line {}: {problem}", index + 1)),
            }
        }
    }
    if problems.is_empty() {
        Ok(found)
    } else {
        Err(problems)
    }
}

/// The result of one UI case.
#[derive(Debug)]
pub struct UiOutcome {
    /// Diagnostics that do not match the annotations, and malformed
    /// annotations; empty when they match.
    pub problems: Vec<String>,
    /// The rendered diagnostics, which the snapshot must equal.
    pub rendered: String,
}

/// Compiles `text` as `path` and matches the diagnostics against the
/// annotations of each file the case describes.
pub fn check(compiler: &dyn Compiler, path: &str, text: &str) -> UiOutcome {
    let checked = compiler.check(path, text);
    UiOutcome {
        problems: compare(&checked),
        rendered: rendered(&checked),
    }
}

/// Compares the rendered output with the snapshot, `None` when there is no
/// snapshot file. A case without diagnostics needs none. Trailing newlines
/// are not compared, so an editor that adds or strips one does not matter.
pub fn compare_snapshot(snapshot: Option<&str>, rendered: &str) -> Option<String> {
    match snapshot {
        Some(snapshot) if snapshot.trim_end_matches('\n') != rendered.trim_end_matches('\n') => {
            Some(format!(
                "the output differs from the snapshot (- snapshot, + actual):\n{}",
                diff(snapshot, rendered)
            ))
        }
        None if !rendered.is_empty() => Some(format!(
            "no snapshot; run with --bless to write it. The output is:\n{rendered}"
        )),
        _ => None,
    }
}

/// Each diagnostic rendered without color, separated by blank lines, with a
/// final newline; empty when there are none.
pub fn rendered(checked: &Checked) -> String {
    let mut out = String::new();
    for diagnostic in &checked.diagnostics {
        if !out.is_empty() {
            out.push('\n');
        }
        out.push_str(&render(diagnostic, &checked.files, false));
        out.push('\n');
    }
    out
}

/// Pairs reported diagnostics with the annotations of the files they are
/// in, by line, severity and code. With more than one file, each problem
/// names its file.
fn compare(checked: &Checked) -> Vec<String> {
    let several = checked.sources.len() > 1;
    let at = |file: FileId, line: usize| {
        if several {
            format!("{}:{line}", checked.files.path(file))
        } else {
            format!("line {line}")
        }
    };
    let mut unmatched: Vec<(FileId, Annotation)> = Vec::new();
    let mut problems = Vec::new();
    for &file in &checked.sources {
        match annotations(checked.files.text(file)) {
            Ok(found) => unmatched.extend(found.into_iter().map(|a| (file, a))),
            Err(malformed) if several => problems.extend(
                malformed
                    .into_iter()
                    .map(|problem| format!("{}: {problem}", checked.files.path(file))),
            ),
            Err(malformed) => problems.extend(malformed),
        }
    }
    for diagnostic in &checked.diagnostics {
        let span = diagnostic.primary.span;
        let severity = diagnostic.severity;
        let code = diagnostic.code;
        let what = format!("{} {code}: {}", name(severity), diagnostic.message);
        if !checked.sources.contains(&span.file) {
            let path = checked.files.path(span.file);
            problems.push(format!("unexpected {what} (reported in {path})"));
            continue;
        }
        let line = checked
            .files
            .line_index(span.file)
            .line_col(span.range.start())
            .map_or(1, |pos| pos.line as usize + 1);
        let wanted = Annotation {
            line,
            severity,
            code,
        };
        match unmatched
            .iter()
            .position(|&(f, a)| f == span.file && a == wanted)
        {
            Some(index) => {
                unmatched.remove(index);
            }
            None => problems.push(format!("{}: unexpected {what}", at(span.file, line))),
        }
    }
    for (file, missing) in unmatched {
        problems.push(format!(
            "{}: expected {} {}, not reported",
            at(file, missing.line),
            name(missing.severity),
            missing.code
        ));
    }
    problems
}

fn name(severity: Severity) -> &'static str {
    match severity {
        Severity::Error => "error",
        Severity::Warning => "warning",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Parsed, not `codes::MRnnnn`: `cargo xtask error-codes` counts those in
    // `src` as reported codes, which need a page.
    fn code(text: &str) -> Code {
        text.parse().unwrap()
    }

    #[test]
    fn annotations_sit_after_code_and_may_repeat() {
        let text = "let a = 1;\nlet b = a; //~ ERROR MR0300 moved\nf(b); //~ ERROR MR0320 //~ WARNING MR2002\n";
        let found = annotations(text).unwrap();
        let summary: Vec<_> = found.iter().map(|a| (a.line, a.severity, a.code)).collect();
        assert_eq!(
            summary,
            [
                (2, Severity::Error, code("MR0300")),
                (3, Severity::Error, code("MR0320")),
                (3, Severity::Warning, code("MR2002")),
            ]
        );
    }

    #[test]
    fn malformed_annotations_are_reported() {
        let problems =
            annotations("a //~ NOTE MR0300\nb //~ ERROR MR300\nc //~ ERROR\n").unwrap_err();
        assert_eq!(
            problems,
            [
                "line 1: `//~` must be followed by ERROR or WARNING, not `NOTE`",
                "line 2: `MR300` is not a diagnostic code like `MR0310`",
                "line 3: `//~` needs a code after the severity",
            ]
        );
    }
}
