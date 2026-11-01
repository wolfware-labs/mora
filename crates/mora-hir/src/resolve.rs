use mora_span::{Db, Project, SourceFile};

use crate::defs::{ItemId, NamespaceRef, def_map};
use crate::items::{Item, ItemKind, Visibility, item_tree};

/// The name of the standard library's root namespace (§4.4).
pub(crate) const STD: &str = "Std";

/// Where a name is looked up from: a file of a project, at namespace level.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Scope {
    /// The project the file belongs to.
    pub project: Project,
    /// The file.
    pub file: SourceFile,
}

/// What a path names.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Resolved {
    /// A namespace of this project or of a dependency.
    Namespace(NamespaceRef),
    /// A type, a macro, or a constant or static.
    Item(ItemId),
    /// One function or a set of overloads.
    Functions(Vec<ItemId>),
    /// A name under `Std` while the standard library is not loaded, as in
    /// tests that compile files on their own. Nothing is known about it.
    External,
}

/// Why a path does not resolve.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ResolveError {
    /// The segment that failed, counting from zero.
    pub segment: usize,
    /// What went wrong there.
    pub kind: ResolveErrorKind,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ResolveErrorKind {
    /// No declaration has the name (`MR0127`). `within` is what the segments
    /// before it name, `None` for the first segment.
    Unresolved { within: Option<Resolved> },
    /// More than one declaration has it at the same level of the lookup (`MR0128`).
    Ambiguous(Vec<Resolved>),
    /// Only declarations that are not visible from the scope have it (`MR0129`).
    NotVisible(ItemId),
}

/// Looks `path` up from `scope`.
///
/// The first segment is looked up in order (§14.5): the enclosing
/// namespaces from the innermost out; then the global names, which are the
/// first segment of the project's root namespace, the dependencies' names
/// and `Std`; then what the file's `using` directives and the project's
/// `global using` directives open; then the prelude. The first level that
/// has a visible declaration with the name decides: two there are
/// ambiguous, unless all are functions, which are overloads. Each later
/// segment is looked up in what the path names so far. Locals, parameters
/// and members of enclosing types come before all of these; they belong to
/// bodies, which have their own lookup.
pub fn resolve_path(db: &dyn Db, scope: Scope, path: &[String]) -> Result<Resolved, ResolveError> {
    resolve(db, scope, path, true)
}

/// Looks up the path of a `using` directive: like [`resolve_path`], but
/// without the levels that `using` directives and the prelude add, so the
/// directives of a project never depend on each other.
pub(crate) fn resolve_using(
    db: &dyn Db,
    scope: Scope,
    path: &[String],
) -> Result<Resolved, ResolveError> {
    resolve(db, scope, path, false)
}

fn resolve(
    db: &dyn Db,
    scope: Scope,
    path: &[String],
    with_imports: bool,
) -> Result<Resolved, ResolveError> {
    let Some((first, rest)) = path.split_first() else {
        return Err(ResolveError {
            segment: 0,
            kind: ResolveErrorKind::Unresolved { within: None },
        });
    };
    let mut current = first_segment(db, scope, first, with_imports)
        .map_err(|kind| ResolveError { segment: 0, kind })?;
    for (index, name) in rest.iter().enumerate() {
        current = member(db, scope, &current, name).map_err(|kind| ResolveError {
            segment: index + 1,
            kind,
        })?;
    }
    Ok(current)
}

/// The first segment, through the levels of the lookup in order.
fn first_segment(
    db: &dyn Db,
    scope: Scope,
    name: &str,
    with_imports: bool,
) -> Result<Resolved, ResolveErrorKind> {
    let mut hidden = None;
    let map = def_map(db, scope.project);
    let mut namespace = map.file_namespace(scope.file);
    while let Some(id) = namespace {
        let found = namespace_members(
            db,
            NamespaceRef {
                project: scope.project,
                id,
            },
            name,
            true,
        );
        if let Some(found) = pick(db, scope, found, &mut hidden)? {
            return Ok(found);
        }
        namespace = map.namespace(id).parent;
    }
    if let Some(found) = pick(
        db,
        scope,
        global_names(db, scope.project, name),
        &mut hidden,
    )? {
        return Ok(found);
    }
    if with_imports {
        let imports = imports(db, scope.project, scope.file);
        let mut found = Vec::new();
        for (alias, target) in &imports.aliases {
            if alias == name {
                found.push(target.clone());
            }
        }
        for &namespace in &imports.namespaces {
            found.extend(namespace_members(db, namespace, name, false));
        }
        for &ty in &imports.types {
            found.extend(type_members(db, ty, name));
        }
        if let Some(found) = pick(db, scope, found, &mut hidden)? {
            return Ok(found);
        }
        if let Some(std) = dependency(db, scope.project, STD) {
            let found = namespace_members(db, std, name, false);
            if let Some(found) = pick(db, scope, found, &mut hidden)? {
                return Ok(found);
            }
        }
    }
    Err(hidden.map_or(
        ResolveErrorKind::Unresolved { within: None },
        ResolveErrorKind::NotVisible,
    ))
}

/// A segment after the first, looked up in what the path names so far.
fn member(
    db: &dyn Db,
    scope: Scope,
    current: &Resolved,
    name: &str,
) -> Result<Resolved, ResolveErrorKind> {
    let found = match current {
        Resolved::Namespace(namespace) => namespace_members(db, *namespace, name, true),
        // Only types have members, so this is empty for any other item.
        Resolved::Item(item) => type_members(db, *item, name),
        Resolved::External => return Ok(Resolved::External),
        Resolved::Functions(_) => Vec::new(),
    };
    let mut hidden = None;
    match pick(db, scope, found, &mut hidden)? {
        Some(found) => Ok(found),
        None => Err(hidden.map_or_else(
            || ResolveErrorKind::Unresolved {
                within: Some(current.clone()),
            },
            ResolveErrorKind::NotVisible,
        )),
    }
}

/// The declarations of one level that are visible from `scope`: `None`
/// when there are none, an error when they are ambiguous. The first
/// declaration that is not visible is kept in `hidden`, for the error when
/// no level has a visible one.
fn pick(
    db: &dyn Db,
    scope: Scope,
    found: Vec<Resolved>,
    hidden: &mut Option<ItemId>,
) -> Result<Option<Resolved>, ResolveErrorKind> {
    let mut functions = Vec::new();
    let mut others: Vec<Resolved> = Vec::new();
    for candidate in found {
        match candidate {
            Resolved::Item(item) if !is_visible(db, item, scope) => {
                hidden.get_or_insert(item);
            }
            Resolved::Item(item) if item_data(db, item).kind == ItemKind::Function => {
                if !functions.contains(&item) {
                    functions.push(item);
                }
            }
            Resolved::Functions(items) => {
                for item in items {
                    if !is_visible(db, item, scope) {
                        hidden.get_or_insert(item);
                    } else if !functions.contains(&item) {
                        functions.push(item);
                    }
                }
            }
            other => {
                if !others.contains(&other) {
                    others.push(other);
                }
            }
        }
    }
    match (others.len(), functions.is_empty()) {
        (0, true) => Ok(None),
        (0, false) => Ok(Some(Resolved::Functions(functions))),
        (1, true) => Ok(others.pop()),
        _ => {
            if !functions.is_empty() {
                others.push(Resolved::Functions(functions));
            }
            Err(ResolveErrorKind::Ambiguous(others))
        }
    }
}

/// The declarations named `name` in a namespace: its items and, when
/// `children` is set, the namespace of a subfolder.
fn namespace_members(
    db: &dyn Db,
    namespace: NamespaceRef,
    name: &str,
    children: bool,
) -> Vec<Resolved> {
    let data = def_map(db, namespace.project).namespace(namespace.id);
    let mut found = Vec::new();
    if children && let Some(&child) = data.children.get(name) {
        found.push(Resolved::Namespace(NamespaceRef {
            project: namespace.project,
            id: child,
        }));
    }
    if let Some(items) = data.items.get(name) {
        found.extend(items.iter().copied().map(Resolved::Item));
    }
    found
}

/// The nested types and functions named `name` in a type; none for an
/// item that is not a type.
fn type_members(db: &dyn Db, ty: ItemId, name: &str) -> Vec<Resolved> {
    let tree = item_tree(db, ty.file);
    tree.item(ty.item)
        .members
        .iter()
        .filter(|&&member| tree.item(member).name == name)
        .map(|&member| Resolved::Item(ItemId { item: member, ..ty }))
        .collect()
}

/// The global names that match: the first segment of the project's root
/// namespace, a dependency, or `Std` while the standard library is not loaded.
fn global_names(db: &dyn Db, project: Project, name: &str) -> Vec<Resolved> {
    let map = def_map(db, project);
    let mut found = Vec::new();
    if map.namespace(map.outermost()).name == name {
        found.push(Resolved::Namespace(NamespaceRef {
            project,
            id: map.outermost(),
        }));
    }
    if let Some(namespace) = dependency(db, project, name) {
        found.push(Resolved::Namespace(namespace));
    } else if name == STD {
        found.push(Resolved::External);
    }
    found
}

/// The root namespace of the dependency bound to `name`.
fn dependency(db: &dyn Db, project: Project, name: &str) -> Option<NamespaceRef> {
    project
        .dependencies(db)
        .iter()
        .find(|dependency| dependency.name == name)
        .map(|dependency| NamespaceRef {
            project: dependency.project,
            id: def_map(db, dependency.project).root(),
        })
}

/// What the `using` directives that apply to a file open.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct Imports {
    /// `using Name = Target;`, by name.
    aliases: Vec<(String, Resolved)>,
    /// `using Namespace;`.
    namespaces: Vec<NamespaceRef>,
    /// `using static Type;`.
    types: Vec<ItemId>,
}

/// The file's own directives and every `global using` of the project. A
/// directive whose path does not resolve opens nothing; [`crate::diagnostics`]
/// reports it.
#[salsa::tracked(returns(ref))]
pub(crate) fn imports(db: &dyn Db, project: Project, file: SourceFile) -> Imports {
    let mut imports = Imports::default();
    for other in def_map(db, project).files() {
        for using in item_tree(db, other).usings() {
            if other != file && !using.global {
                continue;
            }
            let scope = Scope {
                project,
                file: other,
            };
            let Ok(target) = resolve_using(db, scope, &using.path) else {
                continue;
            };
            match (&using.alias, using.is_static, target) {
                (Some(alias), _, target) => imports.aliases.push((alias.clone(), target)),
                (None, false, Resolved::Namespace(namespace)) => {
                    imports.namespaces.push(namespace);
                }
                // An item that is not a type has no members, so it opens nothing.
                (None, true, Resolved::Item(ty)) => imports.types.push(ty),
                _ => {}
            }
        }
    }
    imports
}

pub(crate) fn item_data(db: &dyn Db, item: ItemId) -> &Item {
    item_tree(db, item.file).item(item.item)
}

/// The visibility of an item after its enclosing types narrow it (§4.5).
/// `private` at namespace level, where no type encloses the item, means
/// the declaring file.
pub fn effective_visibility(db: &dyn Db, item: ItemId) -> Visibility {
    let tree = item_tree(db, item.file);
    let mut visibility = Visibility::Public;
    let mut at = Some(item.item);
    while let Some(idx) = at {
        let data = tree.item(idx);
        let own = match (data.visibility, data.parent) {
            (Visibility::Private, None) => Visibility::File,
            (own, _) => own,
        };
        visibility = visibility.min(own);
        at = data.parent;
    }
    visibility
}

/// Whether code at namespace level in `from` can see `item` (§4.5). A
/// `private` member is visible only inside its type, so never from there.
pub fn is_visible(db: &dyn Db, item: ItemId, from: Scope) -> bool {
    let same_project = item.project == from.project;
    match effective_visibility(db, item) {
        Visibility::Public => true,
        Visibility::Internal => same_project,
        Visibility::Folder => {
            let map = def_map(db, item.project);
            same_project && map.file_namespace(item.file) == map.file_namespace(from.file)
        }
        Visibility::File => item.file == from.file,
        Visibility::Private => false,
    }
}
