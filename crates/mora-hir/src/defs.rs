use std::collections::BTreeMap;

use la_arena::{Arena, Idx};
use mora_span::{Db, Project, ProjectKind, SourceFile};

use crate::items::{ItemIdx, ItemKind, item_tree};
use crate::name::pascal_case;

/// An index into the namespaces of a [`DefMap`].
pub type NamespaceId = Idx<Namespace>;

/// An item of a project: its file and its index in the file's item tree.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ItemId {
    /// The project the file belongs to.
    pub project: Project,
    /// The file that declares it.
    pub file: SourceFile,
    /// Its index in [`crate::item_tree`] of the file.
    pub item: ItemIdx,
}

/// A namespace of a project.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct NamespaceRef {
    /// The project whose [`DefMap`] holds it.
    pub project: Project,
    /// Its index there.
    pub id: NamespaceId,
}

/// The namespaces of a project and what each declares (§4.2).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DefMap {
    /// Every namespace, the root namespace's enclosing ones first.
    namespaces: Arena<Namespace>,
    /// The first segment of the root namespace: the name the project's code
    /// reaches it by.
    outermost: NamespaceId,
    /// The root namespace, where the files directly under `src/` are.
    root: NamespaceId,
    /// Each file and its namespace, by path.
    files: Vec<(SourceFile, NamespaceId)>,
    /// Folders that give no namespace name, or the same one as a sibling.
    folder_problems: Vec<FolderProblem>,
    /// Every `Main` function, by file path and then in the order of the text.
    mains: Vec<ItemId>,
    /// The files with top-level statements, by path.
    statement_files: Vec<SourceFile>,
    /// Whether the project is an app or a library.
    kind: ProjectKind,
    /// The entry file, when the project has it.
    entry_file: Option<SourceFile>,
}

/// One namespace: a folder under `src/`, or a segment of the root namespace.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Namespace {
    /// Its last segment.
    pub name: String,
    /// The namespace it is in; `None` for the first segment of the root namespace.
    pub parent: Option<NamespaceId>,
    /// The namespaces of its subfolders, by name.
    pub children: BTreeMap<String, NamespaceId>,
    /// The items its files declare at namespace level, by name. Several
    /// items with one name are overloads or a conflict.
    pub items: BTreeMap<String, Vec<ItemId>>,
}

/// A folder `MR0121` reports (§4.2).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FolderProblem {
    /// The folder's name in PascalCase is not an identifier.
    Invalid {
        /// The folder, such as `src/2d`.
        folder: String,
        /// The first file in it, where the diagnostic points.
        file: SourceFile,
    },
    /// The folder gives the same namespace as a sibling that comes first.
    Clash {
        /// The folder, such as `src/http-client`.
        folder: String,
        /// The sibling, such as `src/HttpClient`.
        other: String,
        /// The namespace both give.
        namespace: String,
        /// The first file in `folder`.
        file: SourceFile,
        /// The first file in `other`.
        other_file: SourceFile,
    },
}

/// What a project starts from (§4.6).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EntryPoint {
    /// A library, or an app with neither top-level statements nor `Main`.
    None,
    /// The top-level statements of the entry file.
    Statements(SourceFile),
    /// The one `Main` function.
    Main(ItemId),
    /// More than one candidate: `MR0124`.
    Conflict,
}

impl DefMap {
    pub fn namespace(&self, id: NamespaceId) -> &Namespace {
        &self.namespaces[id]
    }

    /// The root namespace, where the files directly under `src/` are.
    pub fn root(&self) -> NamespaceId {
        self.root
    }

    /// The namespace named by the first segment of the root namespace.
    pub fn outermost(&self) -> NamespaceId {
        self.outermost
    }

    /// The namespace of a file of the project.
    pub fn file_namespace(&self, file: SourceFile) -> Option<NamespaceId> {
        self.files
            .iter()
            .find_map(|&(it, namespace)| (it == file).then_some(namespace))
    }

    /// The files of the project, by path.
    pub fn files(&self) -> impl Iterator<Item = SourceFile> + '_ {
        self.files.iter().map(|&(file, _)| file)
    }

    /// The full name of a namespace, such as `UsersApi.Store`.
    pub fn namespace_path(&self, id: NamespaceId) -> String {
        path_of(&self.namespaces, id)
    }

    pub fn folder_problems(&self) -> &[FolderProblem] {
        &self.folder_problems
    }

    /// Every `Main` function: one at namespace level, or a `static` one in a
    /// type that is not an interface.
    pub fn mains(&self) -> &[ItemId] {
        &self.mains
    }

    /// The files with top-level statements, by path.
    pub fn statement_files(&self) -> &[SourceFile] {
        &self.statement_files
    }

    pub fn entry_file(&self) -> Option<SourceFile> {
        self.entry_file
    }

    /// The entry point, or why there is not exactly one.
    pub fn entry_point(&self) -> EntryPoint {
        if self.kind == ProjectKind::Lib {
            return EntryPoint::None;
        }
        let statements = self
            .entry_file
            .filter(|file| self.statement_files.contains(file));
        match (statements, self.mains.as_slice()) {
            (None, []) => EntryPoint::None,
            (Some(file), []) => EntryPoint::Statements(file),
            (None, [main]) => EntryPoint::Main(*main),
            _ => EntryPoint::Conflict,
        }
    }
}

/// The def map of a project. It reads the files' paths and item trees,
/// never their text, so an edit that leaves every item tree equal does not
/// run it again.
#[salsa::tracked(returns(ref))]
pub fn def_map(db: &dyn Db, project: Project) -> DefMap {
    let mut namespaces = Arena::new();
    let mut segments = project.root_namespace(db).split('.');
    let namespace = |name: &str, parent| Namespace {
        name: name.to_owned(),
        parent,
        children: BTreeMap::new(),
        items: BTreeMap::new(),
    };
    // `split` yields at least one segment.
    let outermost = namespaces.alloc(namespace(segments.next().unwrap_or_default(), None));
    let mut root = outermost;
    for segment in segments {
        let id = namespaces.alloc(namespace(segment, Some(root)));
        namespaces[root].children.insert(segment.to_owned(), id);
        root = id;
    }
    let mut builder = Builder {
        namespaces,
        folders: BTreeMap::new(),
        folder_problems: Vec::new(),
    };

    let mut files: Vec<SourceFile> = project.files(db).clone();
    files.sort_by(|a, b| a.path(db).cmp(b.path(db)));
    let mut map_files = Vec::new();
    let mut mains = Vec::new();
    let mut statement_files = Vec::new();
    for file in files {
        let path = file.path(db);
        let namespace = builder.namespace_of(root, path, file);
        map_files.push((file, namespace));
        let tree = item_tree(db, file);
        for &idx in tree.top_level() {
            let item = tree.item(idx);
            builder.namespaces[namespace]
                .items
                .entry(item.name.clone())
                .or_default()
                .push(ItemId {
                    project,
                    file,
                    item: idx,
                });
        }
        for (idx, item) in tree.items() {
            if item.kind != ItemKind::Function || item.name != "Main" {
                continue;
            }
            let counts = match item.parent {
                None => true,
                Some(parent) => item.is_static && tree.item(parent).kind != ItemKind::Interface,
            };
            if counts {
                mains.push(ItemId {
                    project,
                    file,
                    item: idx,
                });
            }
        }
        if tree.has_statements() {
            statement_files.push(file);
        }
    }
    let entry = project.entry(db);
    let entry_file = map_files
        .iter()
        .map(|&(file, _)| file)
        .find(|file| file.path(db) == entry);
    DefMap {
        namespaces: builder.namespaces,
        outermost,
        root,
        files: map_files,
        folder_problems: builder.folder_problems,
        mains,
        statement_files,
        kind: project.kind(db),
        entry_file,
    }
}

/// The parts of a [`DefMap`] that the folders of its files build.
struct Builder {
    /// The namespaces so far.
    namespaces: Arena<Namespace>,
    /// The namespace and first file of each folder seen, by its path.
    folders: BTreeMap<String, (NamespaceId, SourceFile)>,
    /// The problems found so far.
    folder_problems: Vec<FolderProblem>,
}

impl Builder {
    /// The namespace of the file at `path`, creating the namespaces of its
    /// folders on first sight. Files come by path, so the first file seen
    /// in a folder is its first file.
    fn namespace_of(&mut self, root: NamespaceId, path: &str, file: SourceFile) -> NamespaceId {
        let relative = path.strip_prefix("src/").unwrap_or(path);
        let mut folders: Vec<&str> = relative.split('/').collect();
        folders.pop();
        let mut namespace = root;
        let mut folder = String::from("src");
        for name in folders {
            folder.push('/');
            folder.push_str(name);
            if let Some(&(id, _)) = self.folders.get(&folder) {
                namespace = id;
                continue;
            }
            let pascal = pascal_case(name);
            if pascal.is_none() {
                self.folder_problems.push(FolderProblem::Invalid {
                    folder: folder.clone(),
                    file,
                });
            }
            // A folder that gives no name still gets a namespace, under a name
            // no code can write, so its files are checked like any other.
            let segment = pascal.unwrap_or_else(|| name.to_owned());
            let existing = self.namespaces[namespace].children.get(&segment).copied();
            let id = match existing {
                Some(id) => {
                    if let Some((other, &(_, other_file))) =
                        self.folders.iter().find(|(_, (it, _))| *it == id)
                    {
                        self.folder_problems.push(FolderProblem::Clash {
                            folder: folder.clone(),
                            other: other.clone(),
                            namespace: path_of(&self.namespaces, id),
                            file,
                            other_file,
                        });
                    }
                    id
                }
                None => {
                    let id = self.namespaces.alloc(Namespace {
                        name: segment.clone(),
                        parent: Some(namespace),
                        children: BTreeMap::new(),
                        items: BTreeMap::new(),
                    });
                    self.namespaces[namespace].children.insert(segment, id);
                    id
                }
            };
            self.folders.insert(folder.clone(), (id, file));
            namespace = id;
        }
        namespace
    }
}

/// The full name of a namespace, its segments joined with `.`.
fn path_of(namespaces: &Arena<Namespace>, id: NamespaceId) -> String {
    let mut segments = Vec::new();
    let mut at = Some(id);
    while let Some(id) = at {
        segments.push(namespaces[id].name.as_str());
        at = namespaces[id].parent;
    }
    segments.reverse();
    segments.join(".")
}
