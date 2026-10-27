use std::ops::Range;
use std::sync::OnceLock;

use mora_span::{FileId, LineIndex, Span};

/// The file paths and texts that diagnostics point into. The driver and the
/// LSP implement it over the analysis database; [`Files`] is a standalone
/// implementation for tools and tests.
pub trait Sources {
    /// The path shown after `-->`. An unknown file should give a placeholder
    /// such as `<unknown>`, not panic: rendering must never fail.
    fn path(&self, file: FileId) -> &str;
    /// The text of the file, or `""` for an unknown file.
    fn text(&self, file: FileId) -> &str;
    fn line_index(&self, file: FileId) -> &LineIndex;
}

/// Files kept in memory, with ids assigned in the order they are added.
#[derive(Default, Debug)]
pub struct Files {
    files: Vec<(String, String, LineIndex)>,
}

impl Files {
    pub fn new() -> Self {
        Self::default()
    }

    /// # Panics
    ///
    /// Panics after `u32::MAX` files.
    pub fn add(&mut self, path: impl Into<String>, text: impl Into<String>) -> FileId {
        let Ok(raw) = u32::try_from(self.files.len()) else {
            panic!("too many files");
        };
        let text = text.into();
        let lines = LineIndex::new(&text);
        self.files.push((path.into(), text, lines));
        FileId::new(raw)
    }

    fn get(&self, file: FileId) -> Option<&(String, String, LineIndex)> {
        self.files.get(file.raw() as usize)
    }
}

impl Sources for Files {
    fn path(&self, file: FileId) -> &str {
        self.get(file).map_or("<unknown>", |(path, _, _)| path)
    }

    fn text(&self, file: FileId) -> &str {
        self.get(file).map_or("", |(_, text, _)| text)
    }

    fn line_index(&self, file: FileId) -> &LineIndex {
        static EMPTY: OnceLock<LineIndex> = OnceLock::new();
        self.get(file).map_or_else(
            || EMPTY.get_or_init(|| LineIndex::new("")),
            |(_, _, lines)| lines,
        )
    }
}

/// The byte range of `span` moved onto character boundaries inside `text`.
///
/// A span from a buggy pass may run past the end of the text or split a
/// character; the renderer would panic on it. Clamping keeps the report
/// readable and points as close as possible to where the span meant.
pub(crate) fn clamp(text: &str, span: Span) -> Range<usize> {
    let floor = |mut offset: usize| {
        offset = offset.min(text.len());
        while !text.is_char_boundary(offset) {
            offset -= 1;
        }
        offset
    };
    let end = floor(u32::from(span.range.end()) as usize);
    let start = floor(u32::from(span.range.start()) as usize).min(end);
    start..end
}
