//! The target-name half of `cargo xtask layers` (spec §19.4 rule 1).
//!
//! Only `mora-target`, `mora-link` and the backend may branch on a target.
//! Elsewhere, a string literal that equals the name, triple, `os` or `arch`
//! of a built-in target is a violation, unless the file is in [`ALLOW`].
//! Literals are found by a small lexer, so comments never count, and
//! literals on a line with a Rust `cfg`, `cfg!` or `cfg_attr` are skipped:
//! `cfg` describes the host the code is built for, not a Mora target.

use std::fmt;
use std::io;
use std::path::Path;

use mora_target::{Target, builtin_names};

/// The crates that may match on target names, relative to the repository root.
pub const OWNERS: &[&str] = &[
    "crates/mora-target",
    "crates/mora-link",
    "crates/mora-codegen",
];

/// Files outside [`OWNERS`] that may name a target, each with the reason.
pub const ALLOW: &[(&str, &str)] = &[];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Hit {
    pub path: String,
    pub line: usize,
    pub word: String,
}

impl fmt::Display for Hit {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{}:{}: \"{}\" names a target; only mora-target, mora-link and mora-codegen may match on targets (§19.4)",
            self.path, self.line, self.word
        )
    }
}

/// The name, triple, `os` and `arch` of every built-in target, without duplicates.
pub fn words() -> Vec<String> {
    let mut words = Vec::new();
    for name in builtin_names() {
        let target = Target::builtin(name).expect("built-in targets parse");
        for word in [target.name, target.triple, target.os, target.arch] {
            if !words.contains(&word) {
                words.push(word);
            }
        }
    }
    words
}

/// The 1-based line numbers and words of the target names quoted in `text`.
pub fn scan(text: &str, words: &[String]) -> Vec<(usize, String)> {
    let lines: Vec<&str> = text.lines().collect();
    let is_cfg = |line: usize| {
        let code = lines.get(line - 1).copied().unwrap_or_default();
        ["cfg(", "cfg!(", "cfg_attr("]
            .iter()
            .any(|cfg| code.contains(cfg))
    };
    literals(text)
        .into_iter()
        .filter(|(line, literal)| words.contains(literal) && !is_cfg(*line))
        .collect()
}

/// The string literals of Rust source `text`, with the 1-based line each
/// starts on. Comments, nested block comments included, are skipped, and
/// character literals are told apart from lifetimes. Escapes are kept as
/// written, which is enough to compare against target names.
fn literals(text: &str) -> Vec<(usize, String)> {
    let chars: Vec<char> = text.chars().collect();
    let at = |i: usize| chars.get(i).copied().unwrap_or_default();
    let mut found = Vec::new();
    let mut line = 1;
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        let ident_before = i > 0 && (chars[i - 1].is_alphanumeric() || chars[i - 1] == '_');
        if c == '\n' {
            line += 1;
            i += 1;
        } else if c == '/' && at(i + 1) == '/' {
            while i < chars.len() && chars[i] != '\n' {
                i += 1;
            }
        } else if c == '/' && at(i + 1) == '*' {
            let mut depth = 0;
            while i < chars.len() {
                if chars[i] == '/' && at(i + 1) == '*' {
                    depth += 1;
                    i += 2;
                } else if chars[i] == '*' && at(i + 1) == '/' {
                    depth -= 1;
                    i += 2;
                    if depth == 0 {
                        break;
                    }
                } else {
                    line += usize::from(chars[i] == '\n');
                    i += 1;
                }
            }
        } else if c == 'r' && !ident_before && matches!(at(i + 1), '"' | '#') {
            // A raw string: r"...", r#"..."#, and so on.
            let mut j = i + 1;
            while at(j) == '#' {
                j += 1;
            }
            if at(j) != '"' {
                i += 1;
                continue;
            }
            let hashes = j - i - 1;
            let start = (line, j + 1);
            j += 1;
            while j < chars.len() {
                if chars[j] == '"' && (1..=hashes).all(|k| at(j + k) == '#') {
                    break;
                }
                line += usize::from(chars[j] == '\n');
                j += 1;
            }
            found.push((start.0, chars[start.1..j.min(chars.len())].iter().collect()));
            i = j + 1 + hashes;
        } else if c == '"' {
            let start = (line, i + 1);
            let mut j = i + 1;
            while j < chars.len() && chars[j] != '"' {
                if chars[j] == '\\' {
                    j += 1;
                }
                line += usize::from(at(j) == '\n');
                j += 1;
            }
            found.push((start.0, chars[start.1..j.min(chars.len())].iter().collect()));
            i = j + 1;
        } else if c == '\'' {
            // A character literal, or a lifetime, which has no closing quote.
            if at(i + 1) == '\\' {
                let mut j = i + 2;
                while j < chars.len() && chars[j] != '\'' {
                    j += 1;
                }
                i = j + 1;
            } else if at(i + 2) == '\'' {
                i += 3;
            } else {
                i += 1;
            }
        } else {
            i += 1;
        }
    }
    found
}

/// Scans every `.rs` file under `crates/` outside [`OWNERS`] and [`ALLOW`].
pub fn check(root: &Path) -> io::Result<Vec<Hit>> {
    let words = words();
    let mut files = Vec::new();
    collect(root, &root.join("crates"), &mut files)?;
    files.sort();
    let mut hits = Vec::new();
    for path in files {
        let owned = OWNERS
            .iter()
            .any(|owner| path.starts_with(&format!("{owner}/")));
        if owned || ALLOW.iter().any(|(allowed, _)| *allowed == path) {
            continue;
        }
        let text = std::fs::read_to_string(root.join(&path))?;
        for (line, word) in scan(&text, &words) {
            hits.push(Hit {
                path: path.clone(),
                line,
                word,
            });
        }
    }
    Ok(hits)
}

/// Adds the `.rs` files under `dir` to `files`, as `/`-separated paths relative to `root`.
fn collect(root: &Path, dir: &Path, files: &mut Vec<String>) -> io::Result<()> {
    for entry in std::fs::read_dir(dir)? {
        let path = entry?.path();
        if path.is_dir() {
            collect(root, &path, files)?;
        } else if path.extension().is_some_and(|ext| ext == "rs") {
            let relative = path.strip_prefix(root).expect("under the root");
            let parts: Vec<_> = relative.iter().map(|part| part.to_string_lossy()).collect();
            files.push(parts.join("/"));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn words_cover_names_triples_os_and_arch() {
        let words = words();
        for word in [
            "x86_64-linux",
            "aarch64-pc-windows-gnullvm",
            "macos",
            "riscv64",
        ] {
            assert!(words.contains(&word.to_owned()), "{word}");
        }
        assert_eq!(words.iter().filter(|w| *w == "linux").count(), 1);
    }

    #[test]
    fn a_quoted_target_name_is_a_hit() {
        let text =
            "fn f(t: &str) -> bool {\n    t == \"x86_64-linux\" || t.ends_with(\"windows\")\n}\n";
        assert_eq!(
            scan(text, &words()),
            [(2, "x86_64-linux".to_owned()), (2, "windows".to_owned())]
        );
    }

    #[test]
    fn comments_cfg_and_partial_words_are_not_hits_but_later_lines_are() {
        let text = concat!(
            "// builds for \"x86_64-linux\"\n",
            "    /// on \"macos\"\n",
            "#[cfg(target_os = \"linux\")]\n",
            "if cfg!(target_os = \"windows\") {}\n",
            "#[cfg_attr(target_os = \"linux\", allow(dead_code))]\n",
            "/* one line \"linux\" */\n",
            "/*\n",
            " * on \"aarch64-macos\"\n",
            " */\n",
            "let s = \"linux-ish\";\n",
            "let t = \"my-linux\";\n",
            "let u = \"riscv64\";\n",
        );
        assert_eq!(scan(text, &words()), [(12, "riscv64".to_owned())]);
    }

    #[test]
    fn comments_are_skipped_but_code_beside_them_is_not() {
        let text = concat!(
            "let n = 0; // \"linux\"\n",
            "/* a */ let t = \"linux\";\n",
            "/* outer /* \"macos\" */ still \"windows\" */ let a = \"aarch64\";\n",
            "let c = '\"'; let w = \"windows\";\n",
            "fn f<'a>(s: &'a str) -> bool { s == r#\"x86_64\"# }\n",
            "let e = '\\''; let s = \"say \\\"linux\\\"\";\n",
            "let m = \"two\nlines\"; let r = \"riscv64\";\n",
        );
        assert_eq!(
            scan(text, &words()),
            [
                (2, "linux".to_owned()),
                (3, "aarch64".to_owned()),
                (4, "windows".to_owned()),
                (5, "x86_64".to_owned()),
                (8, "riscv64".to_owned()),
            ]
        );
    }

    #[test]
    fn allowed_files_exist() {
        let root = crate::root();
        for (path, reason) in ALLOW {
            assert!(root.join(path).is_file(), "{path}");
            assert!(!reason.is_empty(), "{path}");
        }
    }

    #[test]
    fn the_workspace_has_no_hits() {
        let hits = check(&crate::root()).unwrap();
        assert!(hits.is_empty(), "{hits:?}");
    }
}
