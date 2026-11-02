use mora_diag::Diagnostic;
use mora_span::{Dependency, Project, ProjectKind};
use salsa::Setter;

use super::{TestDb, app, codes, file_named, item_named};
use crate::*;

/// The signature of the item `name` in the file at `path`.
fn signature_of(db: &TestDb, project: Project, path: &str, name: &str) -> Signature {
    let file = file_named(db, project, path);
    signature(db, item_named(db, project, file, name)).clone()
}

fn function(signature: Signature) -> FnSignature {
    match signature {
        Signature::Function(function) => function,
        other => panic!("not a function: {other:?}"),
    }
}

/// The types of the parameters of the function `F` in a file of its own.
fn param_types(text: &str) -> Vec<Ty> {
    let db = TestDb::new();
    let project = app(&db, &[("src/Program.mr", text)]);
    function(signature_of(&db, project, "src/Program.mr", "F"))
        .params
        .into_iter()
        .map(|param| param.ty)
        .collect()
}

fn unsupported(ty: &Ty) -> (Construct, usize) {
    match ty {
        Ty::Unsupported(it) => (it.construct, it.parts.len()),
        other => panic!("not unsupported: {other:?}"),
    }
}

fn messages(diagnostics: &[Diagnostic]) -> Vec<String> {
    diagnostics.iter().map(|d| d.message.clone()).collect()
}

#[test]
fn every_primitive_keyword_names_its_type() {
    let types = param_types(
        "F(a: bool, b: byte, c: sbyte, d: short, e: ushort, f: int, g: uint, h: long, \
         i: ulong, j: nint, k: nuint, l: float, m: double, n: char, o: string) {}\n",
    );
    let primitives = [
        Primitive::Bool,
        Primitive::Byte,
        Primitive::SByte,
        Primitive::Short,
        Primitive::UShort,
        Primitive::Int,
        Primitive::UInt,
        Primitive::Long,
        Primitive::ULong,
        Primitive::NInt,
        Primitive::NUInt,
        Primitive::Float,
        Primitive::Double,
        Primitive::Char,
    ];
    let mut expected: Vec<Ty> = primitives.into_iter().map(Ty::Primitive).collect();
    expected.push(Ty::String);
    assert_eq!(types, expected);
}

#[test]
fn tuple_forms_lower_to_tuples() {
    let types = param_types(
        "F(a: (), b: (int, string), c: (Id: int, Name: string), d: (int), e: (int,), \
         f: (Id: int), g: (int, (bool, char))) {}\n",
    );
    let elem = |name: Option<&str>, ty: Ty| TupleElem {
        name: name.map(str::to_owned),
        ty,
    };
    let int = Ty::Primitive(Primitive::Int);
    assert_eq!(types[0], Ty::UNIT);
    assert_eq!(
        types[1],
        Ty::Tuple(vec![elem(None, int.clone()), elem(None, Ty::String)])
    );
    assert_eq!(
        types[2],
        Ty::Tuple(vec![
            elem(Some("Id"), int.clone()),
            elem(Some("Name"), Ty::String)
        ])
    );
    // Parentheses around one type group it; a comma or a name makes a tuple.
    assert_eq!(types[3], int);
    assert_eq!(types[4], Ty::Tuple(vec![elem(None, int.clone())]));
    assert_eq!(types[5], Ty::Tuple(vec![elem(Some("Id"), int.clone())]));
    assert_eq!(
        types[6],
        Ty::Tuple(vec![
            elem(None, int),
            elem(
                None,
                Ty::Tuple(vec![
                    elem(None, Ty::Primitive(Primitive::Bool)),
                    elem(None, Ty::Primitive(Primitive::Char)),
                ])
            ),
        ])
    );
}

#[test]
fn function_signatures_keep_names_modes_defaults_and_async() {
    let db = TestDb::new();
    let project = app(
        &db,
        &[(
            "src/Program.mr",
            "async F(a: int, in b: int, ref c: int, move d: int, params e: int, f: int = 1) {}\n\
             G(): string => \"\";\n",
        )],
    );
    let f = function(signature_of(&db, project, "src/Program.mr", "F"));
    assert!(f.is_async);
    assert_eq!(f.ret, Ty::UNIT);
    let shape: Vec<_> = f
        .params
        .iter()
        .map(|p| (p.name.as_str(), p.mode, p.is_params, p.has_default))
        .collect();
    assert_eq!(
        shape,
        [
            ("a", ParamMode::In, false, false),
            ("b", ParamMode::In, false, false),
            ("c", ParamMode::Ref, false, false),
            ("d", ParamMode::Move, false, false),
            ("e", ParamMode::In, true, false),
            ("f", ParamMode::In, false, true),
        ]
    );
    let g = function(signature_of(&db, project, "src/Program.mr", "G"));
    assert!(!g.is_async);
    assert_eq!(g.ret, Ty::String);
}

#[test]
fn structs_are_nominal_and_list_their_fields() {
    let db = TestDb::new();
    let project = app(
        &db,
        &[(
            "src/Program.mr",
            "struct Point {\n    public X: int;\n    private mut Y: Point;\n    file Z: float;\n    \
             internal W: char;\n    V: bool;\n    Length(): int => X;\n}\n\
             static Origin: Point = new Point {};\nF(p: Point) {}\n",
        )],
    );
    let file = file_named(&db, project, "src/Program.mr");
    let point = item_named(&db, project, file, "Point");
    let Signature::Struct(fields) = signature(&db, point) else {
        panic!("not a struct");
    };
    let shape: Vec<_> = fields
        .iter()
        .map(|f| (f.name.as_str(), f.visibility, f.ty.clone()))
        .collect();
    assert_eq!(
        shape,
        [
            ("X", Visibility::Public, Ty::Primitive(Primitive::Int)),
            ("Y", Visibility::Private, Ty::Struct(point)),
            ("Z", Visibility::File, Ty::Primitive(Primitive::Float)),
            ("W", Visibility::Internal, Ty::Primitive(Primitive::Char)),
            ("V", Visibility::Folder, Ty::Primitive(Primitive::Bool)),
        ]
    );
    assert_eq!(
        signature_of(&db, project, "src/Program.mr", "Origin"),
        Signature::Value(Ty::Struct(point))
    );
    assert_eq!(
        function(signature_of(&db, project, "src/Program.mr", "F")).params[0].ty,
        Ty::Struct(point)
    );
    assert_eq!(diagnostics(&db, project), &[]);
}

#[test]
fn later_constructs_are_unsupported_with_their_parts() {
    let db = TestDb::new();
    let project = app(
        &db,
        &[(
            "src/Program.mr",
            "class C {}\nunion U { A }\nenum E { A }\ninterface I {}\nstruct G<T> {}\n\
             F(a: int[], b: int?, c: (int, bool) => string, d: weak C, e: any I, \
             f: C, g: U, h: E, i: I, j: G, k: G<int>, l: S<int>, m: Std.Json.JsonValue) {}\n\
             struct S {}\n",
        )],
    );
    let types: Vec<_> = function(signature_of(&db, project, "src/Program.mr", "F"))
        .params
        .iter()
        .map(|param| unsupported(&param.ty))
        .collect();
    assert_eq!(
        types,
        [
            (Construct::Array, 1),
            (Construct::Optional, 1),
            (Construct::Function, 3),
            (Construct::Weak, 1),
            (Construct::Any, 1),
            (Construct::Class, 0),
            (Construct::Union, 0),
            (Construct::Enum, 0),
            (Construct::Interface, 0),
            (Construct::Generic, 0),
            (Construct::Generic, 1),
            (Construct::Generic, 1),
            (Construct::External, 0),
        ]
    );
    let class = function(signature_of(&db, project, "src/Program.mr", "F")).params[5]
        .ty
        .clone();
    let file = file_named(&db, project, "src/Program.mr");
    assert_eq!(
        class,
        Ty::Unsupported(Unsupported {
            construct: Construct::Class,
            item: Some(item_named(&db, project, file, "C")),
            parts: Vec::new(),
        })
    );
    for name in ["C", "U", "E", "I"] {
        assert_eq!(
            signature_of(&db, project, "src/Program.mr", name),
            Signature::None
        );
    }
    assert_eq!(diagnostics(&db, project), &[]);
}

#[test]
fn type_parameters_of_the_item_and_its_types_are_in_scope() {
    let db = TestDb::new();
    let project = app(
        &db,
        &[(
            "src/Program.mr",
            "class Box<T> {\n    Get<U>(t: T, u: U, e: T.Enumerator, l: List<T>): V {}\n}\n\
             union Tree<K> {\n    Leaf;\n    Find(k: K) {}\n}\ninterface IMap<Q> {\n    Get(q: Q);\n}\n",
        )],
    );
    let get = function(signature_of(&db, project, "src/Program.mr", "Get"));
    let types: Vec<_> = get.params.iter().map(|p| unsupported(&p.ty)).collect();
    assert_eq!(
        types,
        [
            (Construct::TypeParam, 0),
            (Construct::TypeParam, 0),
            (Construct::TypeParam, 0),
            (Construct::External, 1),
        ]
    );
    // `V` is no type parameter.
    assert_eq!(get.ret, Ty::Error);
    let find = function(signature_of(&db, project, "src/Program.mr", "Find"));
    assert_eq!(unsupported(&find.params[0].ty), (Construct::TypeParam, 0));
    let get = function(signature_of(&db, project, "src/Program.mr", "Get"));
    assert_eq!(get.params.len(), 4);
    assert_eq!(codes(diagnostics(&db, project)), ["MR0127"]);
}

#[test]
fn missing_types_lower_to_the_error_type() {
    let db = TestDb::new();
    let project = app(
        &db,
        &[(
            "src/Program.mr",
            "F(a: , b: int[, c) {}\nstatic X = 1;\nG(a: Std.): () {}\n",
        )],
    );
    let f = function(signature_of(&db, project, "src/Program.mr", "F"));
    assert!(f.params.iter().any(|p| p.ty == Ty::Error), "{f:?}");
    assert_eq!(
        signature_of(&db, project, "src/Program.mr", "X"),
        Signature::Value(Ty::Error)
    );
    // The parser reports these; the signature adds nothing.
    assert_eq!(diagnostics(&db, project), &[]);
}

#[test]
fn names_that_are_not_types_are_reported() {
    let db = TestDb::new();
    let project = app(
        &db,
        &[(
            "src/Program.mr",
            "const Max: int = 1;\nmacro M(x: Expr) {\n}\nF() {}\nF(x: int) {}\n\
             G(a: Max, b: M, c: F) {}\n",
        )],
    );
    let g = function(signature_of(&db, project, "src/Program.mr", "G"));
    assert!(g.params[..3].iter().all(|p| p.ty == Ty::Error), "{g:?}");
    assert_eq!(
        messages(diagnostics(&db, project)),
        [
            "`Max` is not a type",
            "`M` is not a type",
            "`F` is not a type",
        ]
    );
}

#[test]
fn ambiguous_and_hidden_types_in_signatures_are_reported() {
    let db = TestDb::new();
    let project = app(
        &db,
        &[
            (
                "src/Program.mr",
                "class A {}\nstruct A {}\nF(a: A, b: UsersApi.Store.Hidden, c: List<Missing>) {}\n",
            ),
            ("src/Store/Hidden.mr", "file struct Hidden {}\n"),
        ],
    );
    assert_eq!(
        codes(diagnostics(&db, project)),
        ["MR0128", "MR0129", "MR0127"]
    );
}

#[test]
fn nested_types_resolve_inside_their_type_only() {
    let db = TestDb::new();
    let project = app(
        &db,
        &[(
            "src/Program.mr",
            "struct Outer {\n    private struct Inner {}\n    private Take(i: Inner): Outer.Inner {}\n    \
             Field: Inner;\n}\nF(i: Inner) {}\nG(i: Outer.Inner) {}\n",
        )],
    );
    let reported = diagnostics(&db, project);
    // `Field` is visible in the folder but `Inner` only inside `Outer`;
    // outside `Outer`, `Inner` is not found or not visible.
    assert_eq!(codes(reported), ["MR0123", "MR0127", "MR0129"]);
}

#[test]
fn a_private_member_is_visible_inside_the_type_of_its_innermost_private_parent() {
    let db = TestDb::new();
    let project = app(
        &db,
        &[(
            "src/Program.mr",
            "class A {\n    private class B {\n        class C {}\n    }\n    class D {}\n}\nclass E {}\n",
        )],
    );
    let file = file_named(&db, project, "src/Program.mr");
    let item = |name| item_named(&db, project, file, name);
    let inside = |name| Scope {
        project,
        file,
        within: Some(item(name).item),
    };
    let c = item("C");
    assert!(is_visible(&db, c, inside("A")));
    assert!(is_visible(&db, c, inside("B")));
    assert!(is_visible(&db, c, inside("D")));
    assert!(!is_visible(&db, c, inside("E")));
    assert!(!is_visible(&db, c, Scope::file(project, file)));
    let other = app(&db, &[("src/Program.mr", "")]);
    let elsewhere = Scope {
        project: other,
        file,
        within: Some(item("A").item),
    };
    assert!(!is_visible(&db, c, elsewhere));
}

#[test]
fn file_items_are_not_visible_from_another_project() {
    let db = TestDb::new();
    let project = app(&db, &[("src/Program.mr", "file class A {}\n")]);
    let file = file_named(&db, project, "src/Program.mr");
    let a = item_named(&db, project, file, "A");
    assert!(is_visible(&db, a, Scope::file(project, file)));
    let other = app(&db, &[]);
    assert!(!is_visible(&db, a, Scope::file(other, file)));
}

#[test]
fn without_the_standard_library_prelude_names_and_usings_of_std_are_external() {
    let db = TestDb::new();
    let project = app(
        &db,
        &[
            (
                "src/Program.mr",
                "using Std.Json;\nF(a: JsonValue, b: Console, c: Missing) {}\n",
            ),
            (
                "src/Other.mr",
                "G(a: JsonValue, b: Console, c: Option<int>, d: Hidden) {}\n",
            ),
            (
                "src/Store/Hidden.mr",
                "file class Hidden {}\nclass Console {}\n",
            ),
        ],
    );
    let f = function(signature_of(&db, project, "src/Program.mr", "F"));
    assert_eq!(unsupported(&f.params[0].ty), (Construct::External, 0));
    assert_eq!(unsupported(&f.params[1].ty), (Construct::External, 0));
    assert_eq!(unsupported(&f.params[2].ty), (Construct::External, 0));
    let g = function(signature_of(&db, project, "src/Other.mr", "G"));
    assert_eq!(g.params[0].ty, Ty::Error);
    assert_eq!(unsupported(&g.params[1].ty), (Construct::External, 0));
    assert_eq!(unsupported(&g.params[2].ty), (Construct::External, 1));
    assert_eq!(g.params[3].ty, Ty::Error);
    assert_eq!(
        messages(diagnostics(&db, project)),
        ["cannot find `JsonValue`", "cannot find `Hidden`"]
    );
}

#[test]
fn with_the_standard_library_the_prelude_is_its_root_namespace() {
    let db = TestDb::new();
    let std = db.project(
        "std",
        ProjectKind::Lib,
        &[
            ("src/Console.mr", "public class Console {}\n"),
            ("src/Json/JsonValue.mr", "public struct JsonValue {}\n"),
            (
                "src/Statics.mr",
                "public class Math {\n    public static struct Pi {}\n    Hidden() {}\n}\n",
            ),
        ],
        Vec::new(),
    );
    // The standard library itself has no `Std` to stand in for.
    let own = std.files(&db)[0];
    assert!(resolve_path(&db, Scope::file(std, own), &["Option".to_owned()]).is_err());
    let project = db.project(
        "users-api",
        ProjectKind::App,
        &[(
            "src/Program.mr",
            "using Std.Json;\nusing J = Std.Json;\nusing static Std.Math;\n\
             F(a: Consol, b: Option, c: JsonValu, d: J.JsonValue, e: Pie) {}\n",
        )],
        vec![Dependency {
            name: "Std".into(),
            project: std,
        }],
    );
    let reported = diagnostics(&db, project);
    assert_eq!(
        messages(reported),
        [
            "cannot find `Consol`",
            "cannot find `Option`",
            "cannot find `JsonValu`",
            "cannot find `Pie`",
        ]
    );
    let fixes: Vec<_> = reported
        .iter()
        .map(|d| d.fixes.first().map(|fix| fix.message.clone()))
        .collect();
    assert_eq!(
        fixes,
        [
            Some("did you mean `Console`?".to_owned()),
            None,
            Some("did you mean `JsonValue`?".to_owned()),
            Some("did you mean `Pi`?".to_owned()),
        ]
    );
}

#[test]
fn suggestions_come_from_what_the_path_names_so_far() {
    let db = TestDb::new();
    let project = app(
        &db,
        &[
            (
                "src/Program.mr",
                "struct Outer {\n    struct Inner {}\n    private struct Secret {}\n}\n\
                 F(a: Outer.Inne, b: UsersApi.Stor, c: Outer.Secre, d: Std.Lst.X, e: F.G) {}\n",
            ),
            ("src/Store/A.mr", ""),
        ],
    );
    let fixes: Vec<_> = diagnostics(&db, project)
        .iter()
        .map(|d| {
            (
                d.message.clone(),
                d.fixes.first().map(|fix| fix.message.clone()),
            )
        })
        .collect();
    assert_eq!(
        fixes,
        [
            (
                "cannot find `Inne` in `UsersApi.Outer`".to_owned(),
                Some("did you mean `Inner`?".to_owned())
            ),
            (
                "cannot find `Stor` in `UsersApi`".to_owned(),
                Some("did you mean `Store`?".to_owned())
            ),
            // `Secret` is private to `Outer`, so it is not suggested.
            ("cannot find `Secre` in `UsersApi.Outer`".to_owned(), None),
            ("cannot find `G` in `UsersApi.F`".to_owned(), None),
        ]
    );
}

#[test]
fn an_edit_inside_a_body_does_not_recompute_signatures() {
    let mut db = TestDb::new();
    let project = app(
        &db,
        &[
            ("src/Program.mr", "let x = 1;\n"),
            (
                "src/Store/UserStore.mr",
                "internal struct User {\n    public Id: int;\n}\n\
                 internal Find(id: int): User {\n    return new User { Id = id };\n}\n",
            ),
        ],
    );
    let store = file_named(&db, project, "src/Store/UserStore.mr");
    let find = item_named(&db, project, store, "Find");
    let before = signature(&db, find).clone();
    assert_eq!(diagnostics(&db, project), &[]);
    db.take_executed();

    // The edit makes the body longer, so every range after it moves.
    store.set_text(&mut db).to(
        "internal struct User {\n    public Id: int;\n}\n\
         internal Find(id: int): User {\n    let user = new User { Id = id + 1 };\n    return user;\n}\n"
            .into(),
    );
    assert_eq!(diagnostics(&db, project), &[]);
    assert_eq!(signature(&db, find), &before);
    let executed = db.take_executed();
    let ran = |query: &str| {
        executed
            .iter()
            .any(|key| key.starts_with(&format!("{query}(")))
    };
    assert!(ran("lowered"), "{executed:?}");
    assert!(!ran("item_signature"), "{executed:?}");

    // A change to the signature computes it again.
    store
        .set_text(&mut db)
        .to("internal struct User {\n    public Id: int;\n}\n\
         internal Find(id: long): User {\n    return new User { Id = 0 };\n}\n"
            .into());
    let after = function(signature(&db, find).clone());
    assert_eq!(after.params[0].ty, Ty::Primitive(Primitive::Long));
    let executed = db.take_executed();
    assert!(
        executed
            .iter()
            .any(|key| key.starts_with("item_signature(")),
        "{executed:?}"
    );
}
