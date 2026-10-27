use rustc_hash::FxHashMap;

/// An interned string: comparing two symbols compares their text.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Symbol(u32);

/// Stores each distinct string once and hands out [`Symbol`]s for them.
///
/// Symbols from one interner mean nothing to another; the database owns one
/// interner per session.
#[derive(Default, Debug)]
pub struct Interner {
    ids: FxHashMap<Box<str>, Symbol>,
    strings: Vec<Box<str>>,
}

impl Interner {
    pub fn new() -> Self {
        Self::default()
    }

    /// Returns the symbol for `text`, adding it on first use.
    ///
    /// # Panics
    ///
    /// Panics after `u32::MAX` distinct strings.
    pub fn intern(&mut self, text: &str) -> Symbol {
        if let Some(&symbol) = self.ids.get(text) {
            return symbol;
        }
        let Ok(raw) = u32::try_from(self.strings.len()) else {
            panic!("interner is full: more than {} distinct strings", u32::MAX);
        };
        let symbol = Symbol(raw);
        self.strings.push(text.into());
        self.ids.insert(text.into(), symbol);
        symbol
    }

    /// Returns the symbol for `text` if it was interned before.
    pub fn get(&self, text: &str) -> Option<Symbol> {
        self.ids.get(text).copied()
    }

    /// The text of a symbol from this interner.
    ///
    /// # Panics
    ///
    /// Panics if the symbol came from another interner and is out of range.
    pub fn resolve(&self, symbol: Symbol) -> &str {
        &self.strings[symbol.0 as usize]
    }

    pub fn len(&self) -> usize {
        self.strings.len()
    }

    pub fn is_empty(&self) -> bool {
        self.strings.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn same_text_gives_same_symbol() {
        let mut interner = Interner::new();
        let a = interner.intern("Console");
        let b = interner.intern("WriteLine");
        assert_eq!(interner.intern("Console"), a);
        assert_ne!(a, b);
        assert_eq!(interner.len(), 2);
    }

    #[test]
    fn resolve_returns_the_text() {
        let mut interner = Interner::new();
        let s = interner.intern("𝄞 clef");
        assert_eq!(interner.resolve(s), "𝄞 clef");
        assert_eq!(interner.get("𝄞 clef"), Some(s));
        assert_eq!(interner.get("missing"), None);
    }
}
