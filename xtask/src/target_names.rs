//! The target-name half of `cargo xtask layers` (spec §19.4 rule 1).
//!
//! Only `mora-target`, `mora-link` and the backend may branch on a target.
//! Elsewhere, a string literal that equals the name, triple, `os` or `arch`
//! of a built-in target is a violation, unless the file is in [`ALLOW`].
//! Comment lines, block comments that start a line, and Rust `cfg`,
//! `cfg!` and `cfg_attr` lines are skipped: comments are not code, and `cfg`
//! describes the host the code is built for, not a Mora target.

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
    let mut hits = Vec::new();
    let mut in_block_comment = false;
    for (i, line) in text.lines().enumerate() {
        let trimmed = line.trim_start();
        if in_block_comment {
            in_block_comment = !line.contains("*/");
            continue;
        }
        if trimmed.starts_with("/*") {
            in_block_comment = !line.contains("*/");
            continue;
        }
        if trimmed.starts_with("//")
            || ["cfg(", "cfg!(", "cfg_attr("]
                .iter()
                .any(|cfg| line.contains(cfg))
        {
            continue;
        }
        for word in words {
            if line.contains(&format!("\"{word}\"")) {
                hits.push((i + 1, word.clone()));
            }
        }
    }
    hits
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
