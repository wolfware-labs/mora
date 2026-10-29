use std::fmt;

use text_size::TextRange;

/// Identifies one source file for the lifetime of an analysis database.
///
/// Ids are assigned by whoever loads files (the driver or the LSP) and are
/// never reused while the database lives, so a [`Span`] stays meaningful
/// after the file's text changes.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct FileId(u32);

impl FileId {
    pub const fn new(raw: u32) -> Self {
        Self(raw)
    }

    pub const fn raw(self) -> u32 {
        self.0
    }
}

impl fmt::Display for FileId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "file#{}", self.0)
    }
}

/// A byte range in one source file.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Span {
    pub file: FileId,
    pub range: TextRange,
}

impl Span {
    pub const fn new(file: FileId, range: TextRange) -> Self {
        Self { file, range }
    }

    /// The smallest span covering both, or `None` when they are in different files.
    pub fn cover(self, other: Span) -> Option<Span> {
        (self.file == other.file).then(|| Span::new(self.file, self.range.cover(other.range)))
    }
}

#[cfg(test)]
mod tests {
    use text_size::TextSize;

    use super::*;

    fn span(file: u32, start: u32, end: u32) -> Span {
        Span::new(
            FileId::new(file),
            TextRange::new(TextSize::new(start), TextSize::new(end)),
        )
    }

    #[test]
    fn cover_joins_ranges_in_one_file() {
        assert_eq!(span(1, 4, 6).cover(span(1, 10, 12)), Some(span(1, 4, 12)));
    }

    #[test]
    fn cover_refuses_two_files() {
        assert_eq!(span(1, 4, 6).cover(span(2, 4, 6)), None);
    }

    #[test]
    fn file_id_keeps_its_raw_value_and_displays_it() {
        let id = FileId::new(42);
        assert_eq!(id.raw(), 42);
        assert_eq!(id.to_string(), "file#42");
    }
}
