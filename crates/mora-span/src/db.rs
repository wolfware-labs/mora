use crate::{FileId, LineIndex};

/// The analysis database (§14.3). Later crates add their queries as tracked
/// functions over this trait; the driver and the LSP own the concrete
/// database type.
#[salsa::db]
pub trait Db: salsa::Database {}

/// One `.mr` file: its id, its path relative to the project root, and its text.
#[salsa::input(debug)]
pub struct SourceFile {
    #[returns(copy)]
    pub id: FileId,
    #[returns(ref)]
    pub path: String,
    #[returns(ref)]
    pub text: String,
}

/// A project and the source files it compiles as one unit (§4.1).
#[salsa::input(debug)]
pub struct Project {
    #[returns(ref)]
    pub name: String,
    #[returns(ref)]
    pub files: Vec<SourceFile>,
}

/// The line index of a file, recomputed only when the file's text changes.
#[salsa::tracked(returns(ref))]
pub fn line_index(db: &dyn Db, file: SourceFile) -> LineIndex {
    LineIndex::new(file.text(db))
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use salsa::Setter;

    use super::*;

    /// A database that records every query execution.
    #[salsa::db]
    #[derive(Clone)]
    struct TestDb {
        storage: salsa::Storage<Self>,
        executed: Arc<Mutex<Vec<String>>>,
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
            Self { storage, executed }
        }

        fn take_executed(&self) -> Vec<String> {
            std::mem::take(&mut *self.executed.lock().unwrap())
        }
    }

    #[salsa::db]
    impl salsa::Database for TestDb {}

    #[salsa::db]
    impl Db for TestDb {}

    #[test]
    fn changing_one_file_recomputes_only_its_line_index() {
        let mut db = TestDb::new();
        let a = SourceFile::new(&db, FileId::new(0), "src/A.mr".into(), "one\ntwo\n".into());
        let b = SourceFile::new(&db, FileId::new(1), "src/B.mr".into(), "one\n".into());
        let project = Project::new(&db, "demo".into(), vec![a, b]);

        assert_eq!(line_index(&db, a).line_count(), 3);
        assert_eq!(line_index(&db, b).line_count(), 2);
        assert_eq!(db.take_executed().len(), 2);

        // Nothing changed: both answers come from the cache.
        line_index(&db, a);
        line_index(&db, b);
        assert_eq!(db.take_executed(), Vec::<String>::new());

        a.set_text(&mut db).to("one\ntwo\nthree\n".into());
        assert_eq!(line_index(&db, a).line_count(), 4);
        assert_eq!(line_index(&db, b).line_count(), 2);
        let executed = db.take_executed();
        assert_eq!(executed.len(), 1, "{executed:?}");
        assert!(executed[0].contains("line_index"), "{executed:?}");

        assert_eq!(project.files(&db), &vec![a, b]);
        assert_eq!(a.path(&db), "src/A.mr");
        assert_eq!(b.id(&db), FileId::new(1));
    }
}
