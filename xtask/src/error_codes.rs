//! `cargo xtask error-codes`: checks the code registry against the spec,
//! the error pages and every place that uses a code (§17.6, §20.4).
//!
//! A code is *used* when a crate refers to `codes::MRnnnn`, a UI test
//! expects it with `//~ ERROR MRnnnn` or `//~ WARNING MRnnnn`, or a `mora`
//! example in the spec or a page starts with `// error: MRnnnn` or
//! `// warning: MRnnnn`. Every used code needs a registry entry and a page.

use std::fmt;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use mora_diag::Code;
use mora_diag::registry::{CodeRange, Entry};

/// Where the spec lives, relative to the repository root.
pub const SPEC: &str = "docs/design/mora.md";
/// Where the pages live, relative to the repository root.
pub const PAGES: &str = "docs/errors";

/// A code at a place in the repository.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Use {
    pub code: Code,
    /// `path:line`, relative to the repository root.
    pub at: String,
}

/// The parts of the repository the check reads.
#[derive(Debug, Default)]
pub struct Repo {
    pub spec: String,
    /// File name and text of every file in `docs/errors/`.
    pub pages: Vec<(String, String)>,
    pub uses: Vec<Use>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Problem {
    /// The range table of §17.6 and the registry's ranges differ.
    RangeTable {
        spec: Vec<String>,
        registry: Vec<String>,
    },
    Unsorted {
        code: Code,
    },
    OutOfRange {
        code: Code,
        at: String,
    },
    /// A code the spec or a use names, missing from the registry.
    Unregistered {
        code: Code,
        at: String,
    },
    /// A registered code the spec never mentions.
    NotInSpec {
        code: Code,
    },
    MissingPage {
        code: Code,
        at: String,
    },
    /// A file in `docs/errors/` that is not a page for a registered code.
    StrayPage {
        file: String,
    },
    BadPage {
        file: String,
        reason: String,
    },
}

impl fmt::Display for Problem {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Problem::RangeTable { spec, registry } => write!(
                f,
                "{SPEC}: the §17.6 range table does not match RANGES in crates/mora-diag/src/registry.rs\n  spec:     {}\n  registry: {}",
                spec.join("; "),
                registry.join("; ")
            ),
            Problem::Unsorted { code } => {
                write!(f, "registry: {code} is out of order or listed twice")
            }
            Problem::OutOfRange { code, at } => {
                write!(f, "{at}: {code} is outside every range of §17.6")
            }
            Problem::Unregistered { code, at } => {
                write!(f, "{at}: {code} is not in crates/mora-diag/src/registry.rs")
            }
            Problem::NotInSpec { code } => write!(
                f,
                "registry: {code} is registered but {SPEC} never mentions it"
            ),
            Problem::MissingPage { code, at } => {
                write!(
                    f,
                    "{at}: {code} is used but {PAGES}/{code}.md does not exist"
                )
            }
            Problem::StrayPage { file } => write!(
                f,
                "{PAGES}/{file}: not README.md or a page for a registered code"
            ),
            Problem::BadPage { file, reason } => write!(f, "{PAGES}/{file}: {reason}"),
        }
    }
}

/// Checks a repository against a registry.
pub fn check(repo: &Repo, ranges: &[CodeRange], codes: &[Entry]) -> Vec<Problem> {
    let mut problems = Vec::new();
    let registered = |code: Code| codes.iter().any(|entry| entry.code == code);
    let in_range = |code: Code| ranges.iter().any(|range| range.contains(code));

    let spec_ranges = range_rows(&repo.spec);
    let registry_ranges: Vec<String> = ranges
        .iter()
        .map(|r| format!("{}-{} {}", r.first, r.last, r.kind))
        .collect();
    if spec_ranges != registry_ranges {
        problems.push(Problem::RangeTable {
            spec: spec_ranges,
            registry: registry_ranges,
        });
    }

    for pair in codes.windows(2) {
        if pair[0].code >= pair[1].code {
            problems.push(Problem::Unsorted { code: pair[1].code });
        }
    }
    for entry in codes {
        if !in_range(entry.code) {
            problems.push(Problem::OutOfRange {
                code: entry.code,
                at: "registry".into(),
            });
        }
    }

    let mentions = spec_mentions(&repo.spec);
    for mention in &mentions {
        if !in_range(mention.code) {
            problems.push(Problem::OutOfRange {
                code: mention.code,
                at: mention.at.clone(),
            });
        } else if !registered(mention.code) {
            problems.push(Problem::Unregistered {
                code: mention.code,
                at: mention.at.clone(),
            });
        }
    }
    for entry in codes {
        if !mentions.iter().any(|m| m.code == entry.code) {
            problems.push(Problem::NotInSpec { code: entry.code });
        }
    }

    let page_codes: Vec<Code> = repo
        .pages
        .iter()
        .filter_map(|(file, _)| page_code(file))
        .collect();
    let mut uses = repo.uses.clone();
    uses.extend(examples(SPEC, &repo.spec));
    // The template in README.md holds placeholder examples, not uses.
    for (file, text) in repo.pages.iter().filter(|(file, _)| file != "README.md") {
        uses.extend(examples(&format!("{PAGES}/{file}"), text));
    }
    for used in &uses {
        if !registered(used.code) {
            problems.push(Problem::Unregistered {
                code: used.code,
                at: used.at.clone(),
            });
        } else if !page_codes.contains(&used.code) {
            problems.push(Problem::MissingPage {
                code: used.code,
                at: used.at.clone(),
            });
        }
    }

    for (file, text) in &repo.pages {
        if file == "README.md" {
            continue;
        }
        match page_code(file) {
            Some(code) if registered(code) => {
                if let Err(reason) = check_page(code, text) {
                    problems.push(Problem::BadPage {
                        file: file.clone(),
                        reason,
                    });
                }
            }
            _ => problems.push(Problem::StrayPage { file: file.clone() }),
        }
    }
    problems.dedup();
    problems
}

/// The code a page file name stands for: `MR0310.md` gives `MR0310`.
fn page_code(file: &str) -> Option<Code> {
    file.strip_suffix(".md")?.parse().ok()
}

/// The rows of the §17.6 range table as `MR0001-MR0099 Lexical`.
fn range_rows(spec: &str) -> Vec<String> {
    spec.lines()
        .filter_map(range_row)
        .map(|(first, last, kind)| format!("{first}-{last} {kind}"))
        .collect()
}

fn range_row(line: &str) -> Option<(Code, Code, &str)> {
    let rest = line.strip_prefix("| `")?;
    let (first, rest) = rest.split_once("`-`")?;
    let (last, rest) = rest.split_once("` | ")?;
    let kind = rest.strip_suffix(" |")?;
    Some((first.parse().ok()?, last.parse().ok()?, kind))
}

/// Every code the spec names outside the range table.
fn spec_mentions(spec: &str) -> Vec<Use> {
    let mut mentions = Vec::new();
    for (index, line) in spec.lines().enumerate() {
        if range_row(line).is_some() {
            continue;
        }
        for code in codes_in(line) {
            mentions.push(Use {
                code,
                at: format!("{SPEC}:{}", index + 1),
            });
        }
    }
    mentions
}

/// Every `MR` followed by exactly four digits, with no letter or digit
/// on either side.
pub fn codes_in(line: &str) -> Vec<Code> {
    let bytes = line.as_bytes();
    let mut codes = Vec::new();
    let mut start = 0;
    while let Some(found) = line[start..].find("MR") {
        let at = start + found;
        start = at + 2;
        let end = at + 6;
        let before_ok = at == 0 || !bytes[at - 1].is_ascii_alphanumeric();
        let after_ok = bytes.get(end).is_none_or(|b| !b.is_ascii_alphanumeric());
        if before_ok
            && after_ok
            && let Some(code) = line.get(at..end).and_then(|text| text.parse().ok())
        {
            codes.push(code);
        }
    }
    codes
}

/// The code a line marks as expected, after one of `markers`.
fn marked_code(line: &str, markers: &[&str]) -> Option<Code> {
    let line = line.trim();
    markers.iter().find_map(|marker| {
        line.strip_prefix(marker)
            .and_then(|rest| rest.split_whitespace().next())
            .and_then(|code| code.parse().ok())
    })
}

const EXAMPLE_MARKERS: &[&str] = &["// error:", "// warning:"];
const UI_MARKERS: &[&str] = &["//~ ERROR", "//~ WARNING"];

/// Codes expected by the executable examples of a Markdown file: `mora`
/// blocks whose first line is `// error: MRnnnn` or `// warning: MRnnnn`.
fn examples(path: &str, text: &str) -> Vec<Use> {
    let mut uses = Vec::new();
    let mut lines = text.lines().enumerate();
    while let Some((_, line)) = lines.next() {
        if line.trim() != "```mora" {
            continue;
        }
        if let Some((index, first)) = lines.next()
            && let Some(code) = marked_code(first, EXAMPLE_MARKERS)
        {
            uses.push(Use {
                code,
                at: format!("{path}:{}", index + 1),
            });
        }
    }
    uses
}

/// The template sections of `docs/errors/README.md`, in order.
const SECTIONS: &[&str] = &["Coming from C#", "Failing example", "Fixed example"];

/// A page has a title naming its code, the sections of [`SECTIONS`] in
/// order, and an executable example in each example section.
fn check_page(code: Code, text: &str) -> Result<(), String> {
    let title = format!("# {code}: ");
    if !text
        .lines()
        .next()
        .is_some_and(|line| line.starts_with(&title))
    {
        return Err(format!("the first line must start with `{title}`"));
    }
    let sections = sections(text);
    let headings: Vec<&str> = sections.iter().map(|(heading, _)| *heading).collect();
    if headings != SECTIONS {
        return Err(format!(
            "the sections must be `## {}`, in that order",
            SECTIONS.join("`, `## ")
        ));
    }
    let first_line = |body: &str| -> Option<String> {
        let block = body.split("```mora\n").nth(1)?;
        block.lines().next().map(str::to_owned)
    };
    let failing = first_line(&sections[1].1);
    if failing
        .as_deref()
        .and_then(|line| marked_code(line, EXAMPLE_MARKERS))
        != Some(code)
    {
        return Err(format!(
            "the failing example must be a `mora` block whose first line is `// error: {code}` or `// warning: {code}`"
        ));
    }
    if !first_line(&sections[2].1).is_some_and(|line| line.starts_with("// expect:")) {
        return Err(
            "the fixed example must be a `mora` block whose first line is `// expect:`".into(),
        );
    }
    Ok(())
}

/// The `## ` sections of a Markdown text, with the text under each.
fn sections(text: &str) -> Vec<(&str, String)> {
    let mut sections: Vec<(&str, String)> = Vec::new();
    for line in text.lines() {
        if let Some(heading) = line.strip_prefix("## ") {
            sections.push((heading, String::new()));
        } else if let Some((_, body)) = sections.last_mut() {
            body.push_str(line);
            body.push('\n');
        }
    }
    sections
}

/// Reads the spec, the pages and the uses in crate sources and UI tests.
pub fn load(root: &Path) -> io::Result<Repo> {
    let spec = fs::read_to_string(root.join(SPEC))?;
    let mut pages = Vec::new();
    let pages_dir = root.join(PAGES);
    if pages_dir.is_dir() {
        for entry in fs::read_dir(&pages_dir)? {
            let path = entry?.path();
            if path.is_file()
                && let Some(name) = path.file_name().and_then(|n| n.to_str())
            {
                pages.push((name.to_owned(), fs::read_to_string(&path)?));
            }
        }
    }
    pages.sort();

    let mut uses = Vec::new();
    for file in files_with_extension(&root.join("crates"), "rs")? {
        if !file.components().any(|c| c.as_os_str() == "src") {
            continue;
        }
        scan(root, &file, &mut uses, |line| {
            line.match_indices("codes::")
                .filter_map(|(index, path)| {
                    let rest = &line[index + path.len()..];
                    let boundary = rest
                        .as_bytes()
                        .get(6)
                        .is_none_or(|b| !b.is_ascii_alphanumeric());
                    rest.get(..6).filter(|_| boundary)?.parse().ok()
                })
                .collect()
        })?;
    }
    for file in files_with_extension(&root.join("tests/ui"), "mr")? {
        scan(root, &file, &mut uses, |line| {
            line.find("//~")
                .and_then(|at| marked_code(&line[at..], UI_MARKERS))
                .into_iter()
                .collect()
        })?;
    }
    Ok(Repo { spec, pages, uses })
}

fn scan(
    root: &Path,
    file: &Path,
    uses: &mut Vec<Use>,
    codes: impl Fn(&str) -> Vec<Code>,
) -> io::Result<()> {
    let text = fs::read_to_string(file)?;
    let shown = file
        .strip_prefix(root)
        .unwrap_or(file)
        .display()
        .to_string();
    let shown = shown.replace('\\', "/");
    for (index, line) in text.lines().enumerate() {
        for code in codes(line) {
            uses.push(Use {
                code,
                at: format!("{shown}:{}", index + 1),
            });
        }
    }
    Ok(())
}

/// Every file under `dir` with the extension, skipping `target`; none when
/// `dir` does not exist.
fn files_with_extension(dir: &Path, extension: &str) -> io::Result<Vec<PathBuf>> {
    let mut files = Vec::new();
    if !dir.is_dir() {
        return Ok(files);
    }
    let mut pending = vec![dir.to_path_buf()];
    while let Some(dir) = pending.pop() {
        for entry in fs::read_dir(&dir)? {
            let path = entry?.path();
            if path.is_dir() {
                if path.file_name().is_some_and(|name| name != "target") {
                    pending.push(path);
                }
            } else if path.extension().is_some_and(|ext| ext == extension) {
                files.push(path);
            }
        }
    }
    files.sort();
    Ok(files)
}

#[cfg(test)]
mod tests {
    use mora_diag::registry::{CODES, RANGES};

    use super::*;

    const LEXICAL: &[CodeRange] = &[CodeRange::new(1, 99, "Lexical")];

    const MR0001: Entry = Entry {
        code: Code::new(1),
        summary: "a source file is not UTF-8",
    };

    const SPEC_TEXT: &str = "\
| `MR0001`-`MR0099` | Lexical |

A file that is not UTF-8 is error `MR0001`.
";

    const PAGE: &str = include_str!("../fixtures/error-codes/unregistered/docs/errors/MR0001.md");

    fn repo(spec: &str, pages: &[(&str, &str)], uses: &[(u16, &str)]) -> Repo {
        Repo {
            spec: spec.into(),
            pages: pages
                .iter()
                .map(|(file, text)| ((*file).into(), (*text).into()))
                .collect(),
            uses: uses
                .iter()
                .map(|(code, at)| Use {
                    code: Code::new(*code),
                    at: (*at).into(),
                })
                .collect(),
        }
    }

    fn fixture() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures/error-codes/unregistered")
    }

    #[test]
    fn fixture_with_an_unregistered_code_fails() {
        let repo = load(&fixture()).unwrap();
        let problems: Vec<String> = check(&repo, LEXICAL, &[MR0001])
            .iter()
            .map(ToString::to_string)
            .collect();
        assert_eq!(
            problems,
            [
                "docs/design/mora.md:7: MR0004 is not in crates/mora-diag/src/registry.rs",
                "tests/ui/nested/unknown.mr:2: MR0042 is not in crates/mora-diag/src/registry.rs",
            ]
        );
    }

    #[test]
    fn fixture_uses_come_from_sources_and_ui_tests_only() {
        let repo = load(&fixture()).unwrap();
        let uses: Vec<String> = repo
            .uses
            .iter()
            .map(|u| format!("{} {}", u.at, u.code))
            .collect();
        assert_eq!(
            uses,
            [
                "crates/demo/src/lib.rs:2 MR0001",
                "tests/ui/encoding.mr:1 MR0001",
                "tests/ui/nested/unknown.mr:2 MR0042",
            ]
        );
    }

    #[test]
    fn clean_repo_passes() {
        let repo = repo(SPEC_TEXT, &[("MR0001.md", PAGE)], &[(1, "src/lib.rs:3")]);
        assert_eq!(check(&repo, LEXICAL, &[MR0001]), []);
    }

    #[test]
    fn used_code_without_a_page_fails() {
        let repo = repo(SPEC_TEXT, &[], &[(1, "src/lib.rs:3")]);
        assert_eq!(
            check(&repo, LEXICAL, &[MR0001]),
            [Problem::MissingPage {
                code: Code::new(1),
                at: "src/lib.rs:3".into()
            }]
        );
    }

    #[test]
    fn example_in_the_spec_counts_as_a_use() {
        let spec = format!("{SPEC_TEXT}\n```mora\n// error: MR0001\nlet x = 1;\n```\n");
        let problems = check(&repo(&spec, &[], &[]), LEXICAL, &[MR0001]);
        assert_eq!(
            problems,
            [Problem::MissingPage {
                code: Code::new(1),
                at: "docs/design/mora.md:6".into()
            }]
        );
    }

    #[test]
    fn page_without_a_registered_code_fails() {
        let repo = repo(
            SPEC_TEXT,
            &[
                ("MR0001.md", PAGE),
                ("MR0002.md", PAGE),
                ("notes.md", ""),
                ("README.md", ""),
            ],
            &[],
        );
        assert_eq!(
            check(&repo, LEXICAL, &[MR0001]),
            [
                Problem::StrayPage {
                    file: "MR0002.md".into()
                },
                Problem::StrayPage {
                    file: "notes.md".into()
                },
            ]
        );
    }

    #[test]
    fn code_outside_its_range_fails() {
        let spec = format!("{SPEC_TEXT}Unknown escape is `MR0150`.\n");
        let stray = Entry {
            code: Code::new(150),
            summary: "x",
        };
        let problems = check(&repo(&spec, &[], &[]), LEXICAL, &[MR0001, stray]);
        assert_eq!(
            problems,
            [
                Problem::OutOfRange {
                    code: Code::new(150),
                    at: "registry".into()
                },
                Problem::OutOfRange {
                    code: Code::new(150),
                    at: "docs/design/mora.md:4".into()
                },
            ]
        );
    }

    #[test]
    fn registry_and_spec_must_agree() {
        let unsorted = [
            MR0001,
            Entry {
                code: Code::new(5),
                summary: "x",
            },
            MR0001,
        ];
        let problems = check(
            &repo("| `MR0001`-`MR0050` | Lexical |\n", &[], &[]),
            LEXICAL,
            &unsorted,
        );
        assert!(
            matches!(problems[0], Problem::RangeTable { .. }),
            "{problems:?}"
        );
        assert!(problems.contains(&Problem::Unsorted { code: Code::new(1) }));
        assert!(problems.contains(&Problem::NotInSpec { code: Code::new(5) }));
    }

    #[test]
    fn malformed_pages_are_reported() {
        let cases = [
            (PAGE.replace("# MR0001:", "# MR0002:"), "first line"),
            (PAGE.replace("## Coming from C#", "## In C#"), "sections"),
            (
                PAGE.replace("// error: MR0001", "// note: MR0001"),
                "failing example",
            ),
            (PAGE.replace("// expect:", "// output:"), "fixed example"),
        ];
        for (page, expected) in cases {
            let problems = check(
                &repo(SPEC_TEXT, &[("MR0001.md", &page)], &[]),
                LEXICAL,
                &[MR0001],
            );
            let [Problem::BadPage { reason, .. }] = problems.as_slice() else {
                panic!("{expected}: {problems:?}");
            };
            assert!(reason.contains(expected), "{reason}");
        }
    }

    #[test]
    fn codes_need_word_boundaries() {
        let found: Vec<String> = codes_in("MR0001, `MR0310`; xMR0002 MR00030 MR12 MR9999")
            .iter()
            .map(ToString::to_string)
            .collect();
        assert_eq!(found, ["MR0001", "MR0310", "MR9999"]);
    }

    #[test]
    fn repository_passes() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
        let repo = load(&root).unwrap();
        let problems = check(&repo, RANGES, CODES);
        assert!(problems.is_empty(), "{problems:#?}");
    }
}
