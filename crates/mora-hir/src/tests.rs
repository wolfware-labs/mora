use std::sync::{Arc, Mutex};

use mora_diag::Diagnostic;
use mora_span::{Dependency, FileId, Project, ProjectKind, SourceFile};
use salsa::Setter;

use crate::*;

/// A database that records every query execution.
#[salsa::db]
#[derive(Clone)]
struct TestDb {
    storage: salsa::Storage<Self>,
    executed: Arc<Mutex<Vec<String>>>,
    next_file: Arc<Mutex<u32>>,
}

impl TestDb {
    fn new() -> Self {
        let executed = Arc::new(Mutex::new(Vec::new()));
        let log = Arc::clone(&executed);
        let storage = salsa::Storage::new(Some(Box::new(move |event: salsa::Event| {
            if let salsa::EventKind::WillExecute { database_key } = event.kind {
                log.lock().unwrap().push(format!("{database_key:?}"));
            }
        })));
        Self {
            storage,
            executed,
            next_file: Arc::default(),
        }
    }

    fn take_executed(&self) -> Vec<String> {
        std::mem::take(&mut *self.executed.lock().unwrap())
    }

    fn file(&self, path: &str, text: &str) -> SourceFile {
        let mut next = self.next_file.lock().unwrap();
        let id = FileId::new(*next);
        *next += 1;
        SourceFile::new(self, id, path.into(), text.into())
    }

    fn project(
        &self,
        name: &str,
        kind: ProjectKind,
        files: &[(&str, &str)],
        dependencies: Vec<Dependency>,
    ) -> Project {
        let files = files
            .iter()
            .map(|(path, text)| self.file(path, text))
            .collect();
        Project::new(
            self,
            name.into(),
            files,
            pascal_case(name).unwrap(),
            kind,
            "src/Program.mr".into(),
            dependencies,
        )
    }
}

#[salsa::db]
impl salsa::Database for TestDb {}

#[salsa::db]
impl mora_span::Db for TestDb {}

fn app(db: &TestDb, files: &[(&str, &str)]) -> Project {
    db.project("users-api", ProjectKind::App, files, Vec::new())
}

fn codes(diagnostics: &[Diagnostic]) -> Vec<String> {
    diagnostics.iter().map(|d| d.code.to_string()).collect()
}

fn path(text: &str) -> Vec<String> {
    text.split('.').map(str::to_owned).collect()
}

fn file_named(db: &TestDb, project: Project, path: &str) -> SourceFile {
    *project
        .files(db)
        .iter()
        .find(|file| file.path(db) == path)
        .unwrap()
}

/// The full name of what `text` resolves to from the file at `from`.
fn resolve(db: &TestDb, project: Project, from: &str, text: &str) -> Result<String, String> {
    let scope = Scope {
        project,
        file: file_named(db, project, from),
    };
    match resolve_path(db, scope, &path(text)) {
        Ok(Resolved::Namespace(namespace)) => {
            Ok(def_map(db, namespace.project).namespace_path(namespace.id))
        }
        Ok(Resolved::Item(item)) => Ok(item_name(db, item)),
        Ok(Resolved::Functions(items)) => Ok(format!("{} functions", items.len())),
        Ok(Resolved::External) => Ok("external".into()),
        Err(error) => Err(match error.kind {
            ResolveErrorKind::Unresolved { .. } => format!("unresolved at {}", error.segment),
            ResolveErrorKind::Ambiguous(candidates) => format!("ambiguous: {}", candidates.len()),
            ResolveErrorKind::NotVisible(item) => format!("hidden: {}", item_name(db, item)),
        }),
    }
}

fn item_name(db: &TestDb, item: ItemId) -> String {
    let map = def_map(db, item.project);
    let namespace = map.file_namespace(item.file).unwrap();
    format!(
        "{}.{}",
        map.namespace_path(namespace),
        item_tree(db, item.file).item(item.item).name
    )
}

fn item_named(db: &TestDb, project: Project, file: SourceFile, name: &str) -> ItemId {
    let (idx, _) = item_tree(db, file)
        .items()
        .find(|(_, item)| item.name == name)
        .unwrap();
    ItemId {
        project,
        file,
        item: idx,
    }
}

#[test]
fn an_edit_inside_a_body_does_not_rebuild_the_def_map() {
    let mut db = TestDb::new();
    let project = app(
        &db,
        &[
            ("src/Program.mr", "using UsersApi.Store;\nlet x = 1;\n"),
            (
                "src/Store/UserStore.mr",
                "internal class UserStore {\n    public Count(): int {\n        return 1;\n    }\n}\n",
            ),
        ],
    );
    assert_eq!(diagnostics(&db, project), &[]);
    db.take_executed();

    // The edit makes the file longer, so every range after it moves.
    let store = file_named(&db, project, "src/Store/UserStore.mr");
    store.set_text(&mut db).to(
        "internal class UserStore {\n    public Count(): int {\n        let n = 40 + 2;\n        return n;\n    }\n}\n"
            .into(),
    );
    assert_eq!(diagnostics(&db, project), &[]);
    let executed = db.take_executed();
    let ran = |query: &str| {
        executed
            .iter()
            .any(|key| key.starts_with(&format!("{query}(")))
    };
    assert!(ran("parse"), "{executed:?}");
    assert!(ran("item_tree"), "{executed:?}");
    assert!(!ran("def_map"), "{executed:?}");
    assert!(!ran("imports"), "{executed:?}");

    // A new declaration changes the item tree, so the def map is rebuilt.
    store
        .set_text(&mut db)
        .to("internal class UserStore {}\ninternal class Cache {}\n".into());
    def_map(&db, project);
    let executed = db.take_executed();
    assert!(
        executed.iter().any(|key| key.starts_with("def_map(")),
        "{executed:?}"
    );
}

#[test]
fn item_trees_have_no_positions() {
    let db = TestDb::new();
    let a = db.file("src/A.mr", "class A {\n    F() {}\n}\n");
    let b = db.file("src/B.mr", "\n\n  class A {   F() { let x = 1; }\n}");
    assert_eq!(item_tree(&db, a), item_tree(&db, b));
    assert_ne!(item_source_map(&db, a), item_source_map(&db, b));
}

#[test]
fn item_trees_keep_names_kinds_visibility_and_nesting() {
    let db = TestDb::new();
    let file = db.file(
        "src/A.mr",
        "global using static Std.Math;\nusing Json = Std.Json.JsonValue;\n\
         public struct Point {\n    private class Cache {}\n    static Main() {}\n    X: int;\n}\n\
         file enum Color { Red }\nconst Max: int = 5;\nArea(): int => 1;\nlet y = 2;\n",
    );
    let tree = item_tree(&db, file);
    let summary: Vec<_> = tree
        .items()
        .map(|(_, item)| {
            let parent = item.parent.map(|p| tree.item(p).name.as_str());
            (
                item.name.as_str(),
                item.kind,
                item.visibility,
                item.is_static,
                parent,
            )
        })
        .collect();
    assert_eq!(
        summary,
        [
            ("Point", ItemKind::Struct, Visibility::Public, false, None),
            (
                "Cache",
                ItemKind::Class,
                Visibility::Private,
                false,
                Some("Point")
            ),
            (
                "Main",
                ItemKind::Function,
                Visibility::Folder,
                true,
                Some("Point")
            ),
            ("Color", ItemKind::Enum, Visibility::File, false, None),
            ("Max", ItemKind::Field, Visibility::Folder, false, None),
            ("Area", ItemKind::Function, Visibility::Folder, false, None),
        ]
    );
    assert_eq!(tree.top_level().len(), 4);
    assert!(tree.has_statements());
    let usings = tree.usings();
    assert_eq!(
        usings[0],
        Using {
            global: true,
            is_static: true,
            alias: None,
            path: path("Std.Math"),
        }
    );
    assert_eq!(usings[1].alias.as_deref(), Some("Json"));
    assert_eq!(usings[1].path, path("Std.Json.JsonValue"));
}

#[test]
fn folders_give_namespaces() {
    let db = TestDb::new();
    let project = app(
        &db,
        &[
            ("src/Program.mr", ""),
            ("src/Store/UserStore.mr", ""),
            ("src/http-client/Retry.mr", ""),
            ("src/http-client/auth/Token.mr", ""),
        ],
    );
    let map = def_map(&db, project);
    let namespace = |path: &str| {
        let file = file_named(&db, project, path);
        map.namespace_path(map.file_namespace(file).unwrap())
    };
    assert_eq!(namespace("src/Program.mr"), "UsersApi");
    assert_eq!(namespace("src/Store/UserStore.mr"), "UsersApi.Store");
    assert_eq!(namespace("src/http-client/Retry.mr"), "UsersApi.HttpClient");
    assert_eq!(
        namespace("src/http-client/auth/Token.mr"),
        "UsersApi.HttpClient.Auth"
    );
    assert_eq!(map.folder_problems(), &[]);
}

#[test]
fn bad_and_clashing_folders_are_reported() {
    let db = TestDb::new();
    let project = app(
        &db,
        &[
            ("src/2d/Shape.mr", ""),
            ("src/HttpClient/A.mr", ""),
            ("src/http-client/B.mr", ""),
        ],
    );
    let problems = def_map(&db, project).folder_problems();
    assert!(matches!(
        &problems[0],
        FolderProblem::Invalid { folder, .. } if folder == "src/2d"
    ));
    assert!(matches!(
        &problems[1],
        FolderProblem::Clash { folder, other, namespace, .. }
            if folder == "src/http-client"
                && other == "src/HttpClient"
                && namespace == "UsersApi.HttpClient"
    ));
    assert_eq!(codes(diagnostics(&db, project)), ["MR0121", "MR0121"]);
}

#[test]
fn visibility_follows_the_folder_by_default() {
    let db = TestDb::new();
    let project = app(
        &db,
        &[
            (
                "src/Store/UserStore.mr",
                "class UserStore {}\ninternal class Shared {}\nfile class Local {}\n",
            ),
            ("src/Store/Other.mr", ""),
            ("src/Program.mr", ""),
        ],
    );
    let scope = |path: &str| Scope {
        project,
        file: file_named(&db, project, path),
    };
    let program = scope("src/Program.mr");
    let other = scope("src/Store/Other.mr");
    let store = scope("src/Store/UserStore.mr");
    let item = |name: &str| item_named(&db, project, store.file, name);
    assert!(is_visible(&db, item("UserStore"), other));
    assert!(!is_visible(&db, item("UserStore"), program));
    assert!(is_visible(&db, item("Shared"), program));
    assert!(is_visible(&db, item("Local"), store));
    assert!(!is_visible(&db, item("Local"), other));
}

#[test]
fn a_nested_item_is_no_more_visible_than_its_type() {
    let db = TestDb::new();
    let project = db.project(
        "a",
        ProjectKind::Lib,
        &[(
            "src/A.mr",
            "class Outer {\n    public class Inner {}\n    private class Secret {}\n}\n\
             public class Open {\n    public class Inner {}\n}\nprivate class Top {}\n",
        )],
        Vec::new(),
    );
    let file = project.files(&db)[0];
    let tree = item_tree(&db, file);
    let inner: Vec<_> = tree
        .items()
        .filter(|(_, item)| item.name == "Inner")
        .map(|(idx, _)| ItemId {
            project,
            file,
            item: idx,
        })
        .collect();
    let item = |name: &str| item_named(&db, project, file, name);
    assert_eq!(effective_visibility(&db, inner[0]), Visibility::Folder);
    assert_eq!(effective_visibility(&db, inner[1]), Visibility::Public);
    assert_eq!(
        effective_visibility(&db, item("Secret")),
        Visibility::Private
    );
    // `private` at namespace level, where no type encloses it, means the file.
    assert_eq!(effective_visibility(&db, item("Top")), Visibility::File);
}

#[test]
fn lookup_goes_from_the_innermost_namespace_out() {
    let db = TestDb::new();
    let project = app(
        &db,
        &[
            (
                "src/Program.mr",
                "internal class Config {}\ninternal class Root {}\n",
            ),
            ("src/Store/Config.mr", "class Config {}\n"),
            ("src/Store/Cache/Entry.mr", "class Entry {}\n"),
        ],
    );
    let program = "src/Program.mr";
    let ok = |text: &str| Ok::<String, String>(text.into());
    // The innermost namespace wins over the root namespace.
    assert_eq!(
        resolve(&db, project, "src/Store/Config.mr", "Config"),
        ok("UsersApi.Store.Config")
    );
    assert_eq!(
        resolve(&db, project, program, "Config"),
        ok("UsersApi.Config")
    );
    // Enclosing namespaces are searched outward.
    assert_eq!(
        resolve(&db, project, "src/Store/Cache/Entry.mr", "Root"),
        ok("UsersApi.Root")
    );
    // A subfolder is reached through its name, and the root through its own.
    assert_eq!(
        resolve(&db, project, program, "Store.Cache"),
        ok("UsersApi.Store.Cache")
    );
    assert_eq!(
        resolve(&db, project, program, "UsersApi.Store"),
        ok("UsersApi.Store")
    );
    // An unmarked item of another folder is found but not visible.
    assert_eq!(
        resolve(&db, project, program, "Store.Config"),
        Err("hidden: UsersApi.Store.Config".into())
    );
    assert_eq!(
        resolve(&db, project, program, "Store.Nothing"),
        Err("unresolved at 1".into())
    );
    assert_eq!(
        resolve(&db, project, program, "Nothing"),
        Err("unresolved at 0".into())
    );
}

#[test]
fn a_hidden_item_does_not_shadow_a_visible_one_further_out() {
    let db = TestDb::new();
    let project = app(
        &db,
        &[
            ("src/Program.mr", "internal class Config {}\n"),
            ("src/Store/Config.mr", "file class Config {}\n"),
            ("src/Store/Use.mr", ""),
        ],
    );
    assert_eq!(
        resolve(&db, project, "src/Store/Use.mr", "Config"),
        Ok("UsersApi.Config".into())
    );
}

#[test]
fn usings_come_after_namespaces_and_globals_and_before_the_prelude() {
    let db = TestDb::new();
    let std = db.project(
        "std",
        ProjectKind::Lib,
        &[
            (
                "src/Console.mr",
                "public class Console {}\npublic class Store {}\n",
            ),
            (
                "src/Collections/List.mr",
                "public class List {}\npublic class Console {}\n",
            ),
        ],
        Vec::new(),
    );
    let project = db.project(
        "users-api",
        ProjectKind::App,
        &[
            ("src/Program.mr", "using Std.Collections;\n"),
            ("src/Store/Thing.mr", "internal class Thing {}\n"),
            ("src/Other.mr", ""),
        ],
        vec![Dependency {
            name: "Std".into(),
            project: std,
        }],
    );
    let ok = |text: &str| Ok::<String, String>(text.into());
    // A `using` opens the namespace's items for its own file.
    assert_eq!(
        resolve(&db, project, "src/Program.mr", "List"),
        ok("Std.Collections.List")
    );
    assert_eq!(
        resolve(&db, project, "src/Other.mr", "List"),
        Err("unresolved at 0".into())
    );
    // The `using` level comes before the prelude: two `Console`s, no ambiguity.
    assert_eq!(
        resolve(&db, project, "src/Program.mr", "Console"),
        ok("Std.Collections.Console")
    );
    assert_eq!(
        resolve(&db, project, "src/Other.mr", "Console"),
        ok("Std.Console")
    );
    // Enclosing namespaces come before the prelude: the folder `Store` wins.
    assert_eq!(
        resolve(&db, project, "src/Other.mr", "Store"),
        ok("UsersApi.Store")
    );
    // The dependency's name is global.
    assert_eq!(
        resolve(&db, project, "src/Other.mr", "Std.Collections"),
        ok("Std.Collections")
    );
}

#[test]
fn two_usings_that_open_one_name_are_ambiguous() {
    let db = TestDb::new();
    let project = app(
        &db,
        &[
            ("src/Program.mr", "using UsersApi.A;\nusing UsersApi.B;\n"),
            (
                "src/A/Item.mr",
                "internal class Item {}\ninternal Run() {}\n",
            ),
            (
                "src/B/Item.mr",
                "internal class Item {}\ninternal Run(x: int) {}\n",
            ),
        ],
    );
    assert_eq!(
        resolve(&db, project, "src/Program.mr", "Item"),
        Err("ambiguous: 2".into())
    );
    // Functions with one name are overloads.
    assert_eq!(
        resolve(&db, project, "src/Program.mr", "Run"),
        Ok("2 functions".into())
    );
}

#[test]
fn global_usings_and_aliases_apply_to_every_file() {
    let db = TestDb::new();
    let project = app(
        &db,
        &[
            (
                "src/Program.mr",
                "global using UsersApi.Store;\nglobal using Users = UsersApi.Store.UserStore;\n",
            ),
            ("src/Store/UserStore.mr", "internal class UserStore {}\n"),
            ("src/Api/Handler.mr", ""),
        ],
    );
    let handler = "src/Api/Handler.mr";
    assert_eq!(
        resolve(&db, project, handler, "UserStore"),
        Ok("UsersApi.Store.UserStore".into())
    );
    assert_eq!(
        resolve(&db, project, handler, "Users"),
        Ok("UsersApi.Store.UserStore".into())
    );
}

#[test]
fn std_is_unchecked_without_a_standard_library() {
    let db = TestDb::new();
    let project = app(
        &db,
        &[(
            "src/Program.mr",
            "using Std.Collections;\nusing static Std.Math;\n",
        )],
    );
    assert_eq!(
        resolve(&db, project, "src/Program.mr", "Std.Anything.At.All"),
        Ok("external".into())
    );
    assert_eq!(diagnostics(&db, project), &[]);
}

#[test]
fn only_public_items_of_a_dependency_are_visible() {
    let db = TestDb::new();
    let core = db.project(
        "core",
        ProjectKind::Lib,
        &[(
            "src/Text/Parser.mr",
            "public class Parser {}\ninternal class Lexer {}\n",
        )],
        Vec::new(),
    );
    let project = db.project(
        "users-api",
        ProjectKind::App,
        &[("src/Program.mr", "")],
        vec![Dependency {
            name: "Core".into(),
            project: core,
        }],
    );
    assert_eq!(
        resolve(&db, project, "src/Program.mr", "Core.Text.Parser"),
        Ok("Core.Text.Parser".into())
    );
    assert_eq!(
        resolve(&db, project, "src/Program.mr", "Core.Text.Lexer"),
        Err("hidden: Core.Text.Lexer".into())
    );
}

#[test]
fn entry_points() {
    let db = TestDb::new();
    let entry = |files: &[(&str, &str)]| {
        let project = app(&db, files);
        (
            def_map(&db, project).entry_point(),
            codes(diagnostics(&db, project)),
        )
    };
    let (point, reported) = entry(&[("src/Program.mr", "let x = 1;\nHelper() {}\n")]);
    assert!(matches!(point, EntryPoint::Statements(_)), "{point:?}");
    assert!(reported.is_empty());

    // Only a function counts, not a type named `Main`.
    let (point, _) = entry(&[("src/Program.mr", "let x = 1;\nclass Main {}\n")]);
    assert!(matches!(point, EntryPoint::Statements(_)), "{point:?}");

    let (point, reported) = entry(&[("src/App.mr", "class App {\n    static Main() {}\n}\n")]);
    assert!(matches!(point, EntryPoint::Main(_)), "{point:?}");
    assert!(reported.is_empty());

    // An instance method named `Main` is not an entry point.
    let (point, _) = entry(&[("src/App.mr", "class App {\n    Main() {}\n}\n")]);
    assert_eq!(point, EntryPoint::None);

    let (point, reported) = entry(&[("src/Program.mr", "let x = 1;\nMain() {}\n")]);
    assert_eq!(point, EntryPoint::Conflict);
    assert_eq!(reported, ["MR0124"]);

    let (_, reported) = entry(&[("src/A.mr", "Main() {}\n"), ("src/B.mr", "Main() {}\n")]);
    assert_eq!(reported, ["MR0124"]);

    let (_, reported) = entry(&[
        ("src/Program.mr", "let x = 1;\n"),
        ("src/Util.mr", "let y = 2;\n"),
    ]);
    assert_eq!(reported, ["MR0125"]);

    let lib = db.project(
        "lib",
        ProjectKind::Lib,
        &[("src/Lib.mr", "let x = 1;\n")],
        Vec::new(),
    );
    assert_eq!(def_map(&db, lib).entry_point(), EntryPoint::None);
    assert_eq!(codes(diagnostics(&db, lib)), ["MR0125"]);
}

#[test]
fn the_fix_for_a_hidden_item_marks_it_internal() {
    let db = TestDb::new();
    let project = app(
        &db,
        &[
            ("src/Program.mr", "using UsersApi.Store.UserStore;\n"),
            ("src/Store/UserStore.mr", "@Json\nclass UserStore {}\n"),
        ],
    );
    let reported = diagnostics(&db, project);
    assert_eq!(codes(reported), ["MR0129"]);
    let fix = &reported[0].fixes[0];
    assert_eq!(fix.message, "mark `UserStore` `internal`");
    let edit = &fix.edits[0];
    assert_eq!(edit.replacement, "internal ");
    assert_eq!(u32::from(edit.span.range.start()), 6);
}

/// The text a diagnostic's primary span covers.
fn primary_text(db: &TestDb, project: Project, diagnostic: &Diagnostic) -> String {
    let file = project
        .files(db)
        .iter()
        .find(|file| file.id(db) == diagnostic.primary.span.file)
        .unwrap();
    file.text(db)[diagnostic.primary.span.range].to_owned()
}

#[test]
fn a_misplaced_statement_is_reported_where_it_starts() {
    let db = TestDb::new();
    let project = app(
        &db,
        &[
            ("src/Program.mr", "let x = 1;\n"),
            ("src/Util.mr", "class A {}\n\nSetup();\nlet y = 2;\n"),
        ],
    );
    let reported = diagnostics(&db, project);
    assert_eq!(codes(reported), ["MR0125"]);
    assert_eq!(primary_text(&db, project, &reported[0]), "Setup();");
}

#[test]
fn duplicate_declarations_are_named_with_their_kind() {
    let db = TestDb::new();
    let project = app(
        &db,
        &[
            ("src/Program.mr", "using UsersApi.Store.Item;\n"),
            ("src/Store/A.mr", "internal struct Item {}\n"),
            ("src/Store/B.mr", "internal class Item {}\n"),
        ],
    );
    let reported = diagnostics(&db, project);
    assert_eq!(codes(reported), ["MR0128"]);
    assert_eq!(reported[0].message, "`Item` is ambiguous");
    assert_eq!(
        reported[0].note.as_deref(),
        Some(
            "it could be the struct `UsersApi.Store.Item` or the class `UsersApi.Store.Item`; \
             rename one of them"
        )
    );
    assert_eq!(reported[0].secondary.len(), 2);
}

#[test]
fn an_unknown_member_names_what_it_was_looked_up_in() {
    let db = TestDb::new();
    let project = app(
        &db,
        &[
            (
                "src/Program.mr",
                "using UsersApi.Store.UserStore.Missing;\nusing UsersApi.Run.X;\n",
            ),
            (
                "src/Store/UserStore.mr",
                "internal class UserStore {\n    internal class Entry {}\n}\n",
            ),
            ("src/Run.mr", "Run() {}\n"),
        ],
    );
    let messages: Vec<_> = diagnostics(&db, project)
        .iter()
        .map(|d| d.message.as_str())
        .collect();
    assert_eq!(
        messages,
        [
            "cannot find `Missing` in `UsersApi.Store.UserStore`",
            "cannot find `X` in `UsersApi.Run`",
        ]
    );
    assert_eq!(
        resolve(&db, project, "src/Program.mr", "Store.UserStore.Entry"),
        Ok("UsersApi.Store.Entry".into())
    );
}

#[test]
fn using_static_opens_the_nested_types() {
    let db = TestDb::new();
    let project = app(
        &db,
        &[
            ("src/Program.mr", "using static UsersApi.Store.UserStore;\n"),
            (
                "src/Store/UserStore.mr",
                "internal class UserStore {\n    internal class Entry {}\n}\n",
            ),
        ],
    );
    assert_eq!(
        resolve(&db, project, "src/Program.mr", "Entry"),
        Ok("UsersApi.Store.Entry".into())
    );
}

#[test]
fn using_static_opens_static_functions_but_not_instance_methods() {
    let db = TestDb::new();
    let project = app(
        &db,
        &[
            ("src/Program.mr", "using static UsersApi.Store.UserStore;\n"),
            (
                "src/Store/UserStore.mr",
                "internal class UserStore {\n    internal static Create() {}\n    internal Add() {}\n}\n",
            ),
        ],
    );
    assert_eq!(
        resolve(&db, project, "src/Program.mr", "Create"),
        Ok("1 functions".into())
    );
    assert!(resolve(&db, project, "src/Program.mr", "Add").is_err());
}

#[test]
fn an_alias_to_overloads_keeps_only_the_visible_ones() {
    let db = TestDb::new();
    let project = app(
        &db,
        &[
            ("src/Program.mr", ""),
            ("src/A/Run.mr", "internal Run() {}\n"),
            (
                "src/A/Other.mr",
                "global using R = UsersApi.A.Run;\nglobal using H = UsersApi.A.Hide;\n\
                 file Run(x: int) {}\nfile Hide() {}\n",
            ),
        ],
    );
    assert_eq!(
        resolve(&db, project, "src/Program.mr", "R"),
        Ok("1 functions".into())
    );
    assert_eq!(
        resolve(&db, project, "src/Program.mr", "H"),
        Err("hidden: UsersApi.A.Hide".into())
    );
}

#[test]
fn the_fix_replaces_a_narrower_modifier() {
    let db = TestDb::new();
    let project = app(
        &db,
        &[
            (
                "src/Program.mr",
                "using UsersApi.Store.Open.Inner;\nusing UsersApi.Store.Open.Local;\n",
            ),
            (
                "src/Store/Types.mr",
                "internal class Open {\n    class Inner {}\n    file class Local {}\n}\n",
            ),
        ],
    );
    let reported = diagnostics(&db, project);
    assert_eq!(codes(reported), ["MR0129", "MR0129"]);
    let edit = |index: usize| {
        let edit = &reported[index].fixes[0].edits[0];
        (edit.replacement.as_str(), edit.span.range.len().into())
    };
    assert_eq!(edit(0), ("internal ", 0u32));
    assert_eq!(edit(1), ("internal", 4u32));
    assert_eq!(
        reported[0].message,
        "`Inner` is visible only in `UsersApi.Store`, not in `UsersApi`"
    );
    assert_eq!(
        reported[1].message,
        "`Local` is visible only in `src/Store/Types.mr`"
    );
}

#[test]
fn a_dependency_item_that_is_not_public_gets_a_public_fix() {
    let db = TestDb::new();
    let core = db.project(
        "core",
        ProjectKind::Lib,
        &[("src/Text/Lexer.mr", "internal class Lexer {}\n")],
        Vec::new(),
    );
    let project = db.project(
        "users-api",
        ProjectKind::App,
        &[("src/Program.mr", "using Core.Text.Lexer;\n")],
        vec![Dependency {
            name: "Core".into(),
            project: core,
        }],
    );
    let reported = diagnostics(&db, project);
    assert_eq!(codes(reported), ["MR0129"]);
    assert_eq!(
        reported[0].message,
        "`Lexer` is not public, so `UsersApi` cannot see it"
    );
    assert_eq!(reported[0].fixes[0].message, "mark `Lexer` `public`");
    assert_eq!(reported[0].fixes[0].edits[0].replacement, "public");
}

#[test]
fn a_dotted_root_namespace_is_a_chain() {
    let db = TestDb::new();
    let file = db.file("src/Store/A.mr", "internal class A {}\n");
    let project = Project::new(
        &db,
        "users".into(),
        vec![file],
        "Acme.Users".into(),
        ProjectKind::Lib,
        "src/Program.mr".into(),
        Vec::new(),
    );
    let map = def_map(&db, project);
    assert_eq!(map.namespace(map.outermost()).name, "Acme");
    assert_eq!(map.namespace_path(map.root()), "Acme.Users");
    assert_eq!(
        resolve(&db, project, "src/Store/A.mr", "Acme.Users.Store.A"),
        Ok("Acme.Users.Store.A".into())
    );
}

#[test]
fn the_standard_library_names_itself() {
    let db = TestDb::new();
    let std = db.project(
        "std",
        ProjectKind::Lib,
        &[("src/Text/Builder.mr", "public class Builder {}\n")],
        Vec::new(),
    );
    assert_eq!(
        resolve(&db, std, "src/Text/Builder.mr", "Std.Text.Builder"),
        Ok("Std.Text.Builder".into())
    );
}

#[test]
fn item_trees_keep_every_kind_of_declaration() {
    let db = TestDb::new();
    let file = db.file(
        "src/A.mr",
        "union Shape { Circle(r: float), Square(side: float); static Unit(): Shape => Circle(1.0); }\n\
         interface IShape {\n    Area(): float;\n}\n\
         macro Log(x: Expr) {\n}\n\
         extension (s: Shape) {\n}\n\
         class Square {\n    IShape.Area(): float => 1.0;\n    X: int;\n}\n",
    );
    let tree = item_tree(&db, file);
    let summary: Vec<_> = tree
        .items()
        .map(|(_, item)| (item.name.as_str(), item.kind))
        .collect();
    assert_eq!(
        summary,
        [
            ("Shape", ItemKind::Union),
            ("Unit", ItemKind::Function),
            ("IShape", ItemKind::Interface),
            ("Area", ItemKind::Function),
            ("Log", ItemKind::Macro),
            ("Square", ItemKind::Class),
        ]
    );
}

#[test]
fn broken_usings_keep_what_they_name() {
    let db = TestDb::new();
    let file = db.file("src/A.mr", "using;\nusing A.;\nusing B.1;\nusing C;\n");
    let paths: Vec<_> = item_tree(&db, file)
        .usings()
        .iter()
        .map(|using| using.path.join("."))
        .collect();
    assert_eq!(paths, ["A", "B", "C"]);
}

#[test]
fn an_empty_path_does_not_resolve() {
    let db = TestDb::new();
    let project = app(&db, &[("src/Program.mr", "")]);
    let scope = Scope {
        project,
        file: file_named(&db, project, "src/Program.mr"),
    };
    let error = resolve_path(&db, scope, &[]).unwrap_err();
    assert_eq!(error.segment, 0);
    assert_eq!(error.kind, ResolveErrorKind::Unresolved { within: None });
}

#[test]
fn duplicates_in_the_own_namespace_are_ambiguous() {
    let db = TestDb::new();
    let project = app(&db, &[("src/Program.mr", "class A {}\nstruct A {}\n")]);
    assert_eq!(
        resolve(&db, project, "src/Program.mr", "A"),
        Err("ambiguous: 2".into())
    );
}

#[test]
fn usings_that_fail_or_name_the_wrong_kind_open_nothing() {
    let db = TestDb::new();
    let project = app(
        &db,
        &[
            (
                "src/Program.mr",
                "using UsersApi.Missing;\nusing UsersApi.Store.Box;\nusing static UsersApi.Store;\n",
            ),
            ("src/Store/Box.mr", "internal class Box {}\n"),
        ],
    );
    assert_eq!(codes(diagnostics(&db, project)), ["MR0127"]);
    assert_eq!(
        resolve(&db, project, "src/Program.mr", "Box"),
        Err("unresolved at 0".into())
    );
}

#[test]
fn every_kind_is_named_in_an_ambiguity() {
    let db = TestDb::new();
    let project = app(
        &db,
        &[
            (
                "src/Program.mr",
                "using UsersApi.K.U;\nusing UsersApi.K.I;\nusing UsersApi.K.C;\nusing UsersApi.K.F;\n",
            ),
            (
                "src/K/A.mr",
                "internal union U { A }\ninternal interface I {}\ninternal const C: int = 1;\n\
                 internal class F {}\n",
            ),
            (
                "src/K/B.mr",
                "internal enum U { A }\ninternal macro I(x: Expr) {\n}\ninternal class C {}\n\
                 internal F() {}\n",
            ),
        ],
    );
    let notes: Vec<_> = diagnostics(&db, project)
        .iter()
        .map(|d| d.note.clone().unwrap())
        .collect();
    let prefix = |note: &str| note.split(';').next().unwrap().to_owned();
    assert_eq!(
        notes.iter().map(|note| prefix(note)).collect::<Vec<_>>(),
        [
            "it could be the union `UsersApi.K.U` or the enum `UsersApi.K.U`",
            "it could be the interface `UsersApi.K.I` or the macro `UsersApi.K.I`",
            "it could be the value `UsersApi.K.C` or the class `UsersApi.K.C`",
            "it could be the class `UsersApi.K.F` or the function `UsersApi.K.F`",
        ]
    );
}

#[test]
fn different_declarations_ask_for_more_of_the_path() {
    let db = TestDb::new();
    let core = db.project(
        "core",
        ProjectKind::Lib,
        &[("src/Text/Parser.mr", "")],
        Vec::new(),
    );
    // A dependency bound to the project's own root name; the manifest
    // rejects this (MR0501), but resolution must still say something.
    let project = db.project(
        "users-api",
        ProjectKind::App,
        &[("src/Program.mr", "using UsersApi.Text;\n")],
        vec![Dependency {
            name: "UsersApi".into(),
            project: core,
        }],
    );
    let reported = diagnostics(&db, project);
    assert_eq!(codes(reported), ["MR0128"]);
    assert_eq!(
        reported[0].note.as_deref(),
        Some(
            "it could be the namespace `UsersApi` or the namespace `Core`; \
             write more of the path to choose one"
        )
    );
}
