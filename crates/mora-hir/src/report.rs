use mora_diag::{Diagnostic, Fix, Label, TextEdit, codes};
use mora_span::{Db, Project, ProjectKind, SourceFile, Span, TextRange, TextSize};

use crate::defs::{DefMap, FolderProblem, ItemId, def_map};
use crate::items::{ItemKind, Visibility, item_source_map, item_tree};
use crate::resolve::{ResolveErrorKind, Resolved, STD, Scope, effective_visibility, resolve_using};

/// The diagnostics of a project's folders (`MR0121`), `using` directives
/// (`MR0127`, `MR0128`, `MR0129`) and entry point (`MR0124`, `MR0125`).
/// Lexical and syntax errors come from [`crate::parse`].
#[salsa::tracked(returns(ref))]
pub fn diagnostics(db: &dyn Db, project: Project) -> Vec<Diagnostic> {
    let map = def_map(db, project);
    let mut out = Vec::new();
    for problem in map.folder_problems() {
        out.push(folder(db, problem));
    }
    for file in map.files() {
        usings(db, project, file, &mut out);
    }
    entry_point(db, project, map, &mut out);
    out
}

/// The start of a file: where a diagnostic about the file as a whole points.
fn file_start(db: &dyn Db, file: SourceFile) -> Span {
    Span::new(file.id(db), TextRange::empty(TextSize::new(0)))
}

fn folder(db: &dyn Db, problem: &FolderProblem) -> Diagnostic {
    match problem {
        FolderProblem::Invalid { folder, file } => {
            let name = folder.rsplit('/').next().unwrap_or(folder);
            Diagnostic::error(
                codes::MR0121,
                format!("the folder `{folder}` does not give a namespace name"),
                Label::new(file_start(db, *file), format!("this file is in `{folder}`")),
            )
            .with_note(format!(
                "a folder's name in PascalCase names its namespace, and `{name}` does not give an identifier"
            ))
        }
        FolderProblem::Clash {
            folder,
            other,
            namespace,
            file,
            other_file,
        } => Diagnostic::error(
            codes::MR0121,
            format!("the folders `{other}` and `{folder}` both give the namespace `{namespace}`"),
            Label::new(file_start(db, *file), format!("this file is in `{folder}`")),
        )
        .with_secondary(Label::new(
            file_start(db, *other_file),
            format!("this file is in `{other}`"),
        )),
    }
}

/// The errors in the paths of a file's `using` directives.
fn usings(db: &dyn Db, project: Project, file: SourceFile, out: &mut Vec<Diagnostic>) {
    let tree = item_tree(db, file);
    let map = item_source_map(db, file);
    let scope = Scope { project, file };
    for (index, using) in tree.usings().iter().enumerate() {
        let Err(error) = resolve_using(db, scope, &using.path) else {
            continue;
        };
        let name = &using.path[error.segment];
        let span = Span::new(file.id(db), map.using(index).segments[error.segment]);
        out.push(match error.kind {
            ResolveErrorKind::Unresolved { within } => unresolved(db, name, span, within),
            ResolveErrorKind::Ambiguous(candidates) => ambiguous(db, name, span, &candidates),
            ResolveErrorKind::NotVisible(item) => not_visible(db, scope, name, span, item),
        });
    }
}

fn unresolved(db: &dyn Db, name: &str, span: Span, within: Option<Resolved>) -> Diagnostic {
    let message = match within.as_ref().and_then(|within| full_name(db, within)) {
        Some(within) => format!("cannot find `{name}` in `{within}`"),
        None => format!("cannot find `{name}`"),
    };
    Diagnostic::error(codes::MR0127, message, Label::new(span, "not found"))
}

fn ambiguous(db: &dyn Db, name: &str, span: Span, candidates: &[Resolved]) -> Diagnostic {
    let names: Vec<Option<String>> = candidates
        .iter()
        .map(|candidate| full_name(db, candidate))
        .collect();
    let described: Vec<String> = candidates
        .iter()
        .zip(&names)
        .filter_map(|(candidate, name)| {
            Some(format!("{} `{}`", kind_name(db, candidate), name.as_ref()?))
        })
        .collect();
    let remedy = if names.windows(2).all(|pair| pair[0] == pair[1]) {
        "rename one of them"
    } else {
        "write more of the path to choose one"
    };
    let note = format!("it could be {}; {remedy}", described.join(" or "));
    let mut diagnostic = Diagnostic::error(
        codes::MR0128,
        format!("`{name}` is ambiguous"),
        Label::new(
            span,
            format!("`{name}` names {} declarations", candidates.len()),
        ),
    )
    .with_note(note);
    for candidate in candidates {
        let items = match candidate {
            Resolved::Item(item) => std::slice::from_ref(item),
            Resolved::Functions(items) => items.as_slice(),
            Resolved::Namespace(_) | Resolved::External => &[],
        };
        for &item in items {
            diagnostic =
                diagnostic.with_secondary(Label::new(item_name_span(db, item), "declared here"));
        }
    }
    diagnostic
}

fn not_visible(db: &dyn Db, scope: Scope, name: &str, span: Span, item: ItemId) -> Diagnostic {
    let map = def_map(db, item.project);
    let declared_in = map
        .file_namespace(item.file)
        .map(|id| map.namespace_path(id))
        .unwrap_or_default();
    let here = def_map(db, scope.project);
    let used_in = here
        .file_namespace(scope.file)
        .map(|id| here.namespace_path(id))
        .unwrap_or_default();
    let same_project = item.project == scope.project;
    let (message, note) = match effective_visibility(db, item) {
        _ if !same_project => (
            format!("`{name}` is not public, so `{used_in}` cannot see it"),
            format!(
                "only `public` declarations of `{declared_in}` are visible to the projects that depend on it"
            ),
        ),
        Visibility::Folder => (
            format!("`{name}` is visible only in `{declared_in}`, not in `{used_in}`"),
            format!(
                "a declaration without a visibility modifier is visible only in its folder; mark it `internal` to use it from `{used_in}`"
            ),
        ),
        Visibility::File => (
            format!("`{name}` is visible only in `{}`", item.file.path(db)),
            "a `file` declaration is visible only in the file that declares it".to_owned(),
        ),
        Visibility::Private | Visibility::Internal | Visibility::Public => (
            format!("`{name}` is private to the type that declares it"),
            "a `private` member is visible only inside its type".to_owned(),
        ),
    };
    let wanted = if same_project {
        Visibility::Internal
    } else {
        Visibility::Public
    };
    let keyword = wanted.keyword().unwrap_or_default();
    Diagnostic::error(codes::MR0129, message, Label::new(span, "not visible here"))
        .with_secondary(Label::new(item_name_span(db, item), "declared here"))
        .with_note(note)
        .with_fix(Fix::new(
            format!("mark `{name}` `{keyword}`"),
            vec![widen(db, item, wanted)],
        ))
}

/// The edit that gives `item` the visibility `wanted`. Only the item needs
/// it: a type around it that the use cannot see fails the lookup first.
fn widen(db: &dyn Db, item: ItemId, wanted: Visibility) -> TextEdit {
    let source = item_source_map(db, item.file).item(item.item);
    let file = item.file.id(db);
    let keyword = wanted.keyword().unwrap_or_default();
    match source.visibility {
        Some(range) => TextEdit::replace(Span::new(file, range), keyword),
        None => TextEdit::insert(file, source.modifiers_start, format!("{keyword} ")),
    }
}

fn item_name_span(db: &dyn Db, item: ItemId) -> Span {
    Span::new(
        item.file.id(db),
        item_source_map(db, item.file).item(item.item).name,
    )
}

/// What kind of declaration a path resolved to, for messages.
fn kind_name(db: &dyn Db, resolved: &Resolved) -> &'static str {
    let item = match resolved {
        Resolved::Namespace(_) | Resolved::External => return "the namespace",
        Resolved::Functions(_) => return "the function",
        Resolved::Item(item) => *item,
    };
    match item_tree(db, item.file).item(item.item).kind {
        ItemKind::Struct => "the struct",
        ItemKind::Class => "the class",
        ItemKind::Union => "the union",
        ItemKind::Enum => "the enum",
        ItemKind::Interface => "the interface",
        ItemKind::Function => "the function",
        ItemKind::Macro => "the macro",
        ItemKind::Field => "the value",
    }
}

/// The full name of what a path resolved to, such as `UsersApi.Store.UserStore`.
fn full_name(db: &dyn Db, resolved: &Resolved) -> Option<String> {
    match resolved {
        Resolved::Namespace(namespace) => {
            Some(def_map(db, namespace.project).namespace_path(namespace.id))
        }
        Resolved::Item(item) => Some(item_full_name(db, *item)),
        Resolved::Functions(items) => items.first().map(|&item| item_full_name(db, item)),
        Resolved::External => Some(STD.to_owned()),
    }
}

fn item_full_name(db: &dyn Db, item: ItemId) -> String {
    let tree = item_tree(db, item.file);
    let mut segments = Vec::new();
    let mut at = Some(item.item);
    while let Some(idx) = at {
        segments.push(tree.item(idx).name.clone());
        at = tree.item(idx).parent;
    }
    let map = def_map(db, item.project);
    if let Some(namespace) = map.file_namespace(item.file) {
        segments.push(map.namespace_path(namespace));
    }
    segments.reverse();
    segments.join(".")
}

/// `MR0124` and `MR0125` (§4.6).
fn entry_point(db: &dyn Db, project: Project, map: &DefMap, out: &mut Vec<Diagnostic>) {
    let entry = project.entry(db);
    let first_statement = |file: SourceFile| {
        let range = item_source_map(db, file)
            .first_statement()
            .unwrap_or_default();
        Span::new(file.id(db), range)
    };
    for &file in map.statement_files() {
        let message = match project.kind(db) {
            ProjectKind::Lib => "a library cannot have top-level statements".to_owned(),
            ProjectKind::App if Some(file) == map.entry_file() => continue,
            ProjectKind::App => {
                format!("top-level statements are allowed only in the entry file `{entry}`")
            }
        };
        out.push(Diagnostic::error(
            codes::MR0125,
            message,
            Label::new(first_statement(file), "a top-level statement"),
        ));
    }
    if project.kind(db) == ProjectKind::Lib {
        return;
    }
    let statements = map
        .entry_file()
        .filter(|file| map.statement_files().contains(file));
    let mains = map.mains();
    let (rest, first) = match statements {
        Some(file) => (
            mains,
            Label::new(first_statement(file), "the top-level statements start here"),
        ),
        None => match mains.split_first() {
            Some((&first, rest)) => (
                rest,
                Label::new(item_name_span(db, first), "the first `Main` is here"),
            ),
            None => return,
        },
    };
    for &main in rest {
        let message = if statements.is_some() {
            "`Main` and top-level statements are both entry points"
        } else {
            "more than one `Main` function"
        };
        out.push(
            Diagnostic::error(
                codes::MR0124,
                message,
                Label::new(item_name_span(db, main), "a second entry point"),
            )
            .with_secondary(first.clone())
            .with_note("an app has exactly one entry point: top-level statements in the entry file, or one `Main`"),
        );
    }
}
