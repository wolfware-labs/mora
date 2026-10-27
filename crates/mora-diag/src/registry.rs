//! Every diagnostic code, its range and a one-line summary (§17.6).
//!
//! A code is registered when the spec assigns it. Once a crate reports it,
//! a UI test expects it or an example names it, it also needs a page in
//! `docs/errors/`. `cargo xtask error-codes` checks both, and checks this
//! file against the range table and the codes in the spec.

use crate::Code;

/// A block of codes for one kind of problem, as the §17.6 table lists it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CodeRange {
    pub first: Code,
    pub last: Code,
    pub kind: &'static str,
}

impl CodeRange {
    /// # Panics
    ///
    /// Panics if `first` is greater than `last` or either has more than four digits.
    pub const fn new(first: u16, last: u16, kind: &'static str) -> Self {
        assert!(first <= last, "a code range must not be empty");
        Self {
            first: Code::new(first),
            last: Code::new(last),
            kind,
        }
    }

    pub fn contains(&self, code: Code) -> bool {
        self.first <= code && code <= self.last
    }
}

/// The ranges of §17.6, in order.
pub const RANGES: &[CodeRange] = &[
    CodeRange::new(1, 99, "Lexical"),
    CodeRange::new(
        100,
        199,
        "Syntax, declarations, C# constructs that Mora replaces",
    ),
    CodeRange::new(200, 299, "Types, generics, interfaces, propagation"),
    CodeRange::new(
        300,
        399,
        "Memory model: moves, borrows, views, escapes, closures",
    ),
    CodeRange::new(400, 499, "Async, threads, FFI, constant evaluation"),
    CodeRange::new(500, 599, "Manifest and build"),
    CodeRange::new(600, 699, "Macros"),
    CodeRange::new(2000, 2999, "Warnings and lints"),
    CodeRange::new(9000, 9999, "Internal compiler errors"),
];

/// The range a code belongs to, if any.
pub fn range_of(code: Code) -> Option<&'static CodeRange> {
    RANGES.iter().find(|range| range.contains(code))
}

/// A registered code.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Entry {
    pub code: Code,
    pub summary: &'static str,
}

/// Declares the codes once, as constants in [`codes`] and as [`CODES`].
macro_rules! registry {
    ($($name:ident = $number:literal: $summary:literal,)*) => {
        /// One constant per registered code, named after it.
        pub mod codes {
            use crate::Code;
            $(
                #[doc = $summary]
                pub const $name: Code = Code::new($number);
            )*
        }

        /// Every registered code, in increasing order.
        pub const CODES: &[Entry] = &[$(Entry { code: codes::$name, summary: $summary }),*];

        #[cfg(test)]
        const NAMES: &[&str] = &[$(stringify!($name)),*];
    };
}

registry! {
    MR0001 = 1: "a source file is not UTF-8",
    MR0002 = 2: "a `#` directive other than `#region` or `#endregion`",
    MR0003 = 3: "a source file is 4 GiB or larger",
    MR0010 = 10: "a numeric literal does not fit its type",
    MR0011 = 11: "the C# `\\x` escape in a string or character literal",
    MR0012 = 12: "a string, character literal, interpolated string or block comment is not closed",
    MR0013 = 13: "an unknown or malformed escape sequence",
    MR0014 = 14: "a malformed numeric literal",
    MR0015 = 15: "a character literal that is empty or holds more than one character",
    MR0016 = 16: "a raw or interpolated string breaks its quote, brace or indentation rules",
    MR0017 = 17: "a character that cannot start a token",
    MR0101 = 101: "`try`, `catch`, `finally` or `throw`: Mora has no exceptions",
    MR0102 = 102: "`object` or `dynamic`: there is no universal base type",
    MR0103 = 103: "`typeof`: there is no runtime reflection",
    MR0104 = 104: "`lock` or `volatile`",
    MR0105 = 105: "`delegate` or `event`",
    MR0106 = 106: "`goto`",
    MR0107 = 107: "`fixed` or `unsafe`",
    MR0108 = 108: "class inheritance keywords such as `virtual`, `override` or `base`",
    MR0109 = 109: "`checked(...)` or `unchecked(...)`",
    MR0120 = 120: "a block-scoped namespace declaration",
    MR0121 = 121: "`using` as a disposal statement",
    MR0122 = 122: "an unsupported source form",
    MR0123 = 123: "a public signature mentions a less visible type",
    MR0124 = 124: "both top-level statements and `Main`, or more than one `Main`",
    MR0125 = 125: "a library with top-level statements",
    MR0126 = 126: "a cycle between `static lazy` initializers",
    MR0127 = 127: "a static constructor",
    MR0130 = 130: "a string indexed by `int`",
    MR0131 = 131: "`null` without a target type",
    MR0132 = 132: "`return value;` is ambiguous when `T` and `E` are the same type",
    MR0140 = 140: "a constructor returns before assigning every field, or uses `this` too early",
    MR0141 = 141: "a `mut` struct method called on a value that is not a mutable place",
    MR0142 = 142: "`this` escapes from a destructor",
    MR0143 = 143: "`new S()` on a struct with a field that has no initializer or default",
    MR0144 = 144: "an object initializer does not set a `required` member",
    MR0150 = 150: "a union contains itself without `indirect`",
    MR0160 = 160: "the null-forgiving operator `x!`",
    MR0161 = 161: "`r?.Name` on a `Result`: member access after propagation needs parentheses",
    MR0162 = 162: "member access on an optional value",
    MR0170 = 170: "a switch that is not exhaustive",
    MR0171 = 171: "a collection expression without a target type",
    MR0180 = 180: "a local shadows a local or parameter of an enclosing scope",
    MR0181 = 181: "a local used before it is definitely assigned",
    MR0182 = 182: "a `let` local is reassigned",
    MR0190 = 190: "`default` of a type that has no default value",
    MR0201 = 201: "`?` cannot convert the error to the function's error type",
    MR0202 = 202: "`?` on an option in a function that does not return an option",
    MR0220 = 220: "polymorphic recursion without bound",
    MR0230 = 230: "a move-only value converted to an interface value",
    MR0231 = 231: "an interface with static abstract members or generic methods used as a type",
    MR0232 = 232: "an `IDisplay` type overrides neither `Format` nor `ToString`",
    MR0233 = 233: "`==` on a type without `IEquatable<T>`",
    MR0234 = 234: "variance annotation on a type parameter",
    MR0235 = 235: "two interfaces give different defaults for one inherited member and the type does not implement it",
    MR0300 = 300: "a moved-from variable is used",
    MR0301 = 301: "one field moved out of a local",
    MR0302 = 302: "a value moved out of an aliased place",
    MR0303 = 303: "a value declared outside a loop moved inside it",
    MR0310 = 310: "an `in` or `ref` parameter of a move-only type is stored",
    MR0311 = 311: "an `out` parameter is not assigned before a return",
    MR0312 = 312: "a `ref`, `out` or view parameter on an async function",
    MR0313 = 313: "an async instance method on a move-only type is not marked `move`",
    MR0320 = 320: "one call borrows the same place twice in conflicting ways",
    MR0321 = 321: "a read-only field passed by `ref`",
    MR0322 = 322: "a `mut` field, element or slot of a move-only type borrowed",
    MR0323 = 323: "a collection element passed by `ref`",
    MR0330 = 330: "a move-only variable captured without `[move x]`",
    MR0331 = 331: "a `ref`, `out` or view variable captured by an escaping closure",
    MR0332 = 332: "a `static` lambda captures a variable",
    MR0333 = 333: "an `in` parameter of function type is stored",
    MR0340 = 340: "a view stored where it could outlive its source",
    MR0341 = 341: "a local changed while a view of it is live",
    MR0342 = 342: "a view of a temporary used after the statement",
    MR0343 = 343: "a view of a local returned",
    MR0344 = 344: "a view that depends on a hidden owner returned",
    MR0350 = 350: "`weak` on a type that is not a shared class",
    MR0360 = 360: "a destructor called directly",
    MR0400 = 400: "`await` on a non-future or outside an async context",
    MR0401 = 401: "an async function declared as returning `Future<T>` or `Task<T>`",
    MR0402 = 402: "`async void`",
    MR0403 = 403: "`ConfigureAwait`",
    MR0410 = 410: "a `shared` class with a field that is not read-only and sendable",
    MR0411 = 411: "a value that is not sendable passed to another thread",
    MR0450 = 450: "an `extern` function with a parameter or return type that is not plain",
    MR0451 = 451: "an `extern` function without `[Import]`",
    MR0460 = 460: "a constant expression that cannot be evaluated at compile time",
    MR0500 = 500: "an unknown key in `mora.toml`",
    MR0510 = 510: "an API that needs a capability the target lacks",
    MR0610 = 610: "a macro's members do not match the interfaces in its `adds` clause",
    MR0611 = 611: "a splice whose value does not fit its position",
    MR0612 = 612: "a generated member clashes with an existing one",
    MR0613 = 613: "a macro uses a function that needs a capability",
    MR0614 = 614: "a macro depends on types that use it",
    MR2001 = 2001: "`T??` in source",
    MR2002 = 2002: "a discarded `Result`",
    MR2003 = 2003: "use of an `[Obsolete]` declaration",
    MR2010 = 2010: "a switch arm that can never match",
    MR2020 = 2020: "lint `strong-cycle`: shared classes that can form a strong cycle",
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_match_codes() {
        for (entry, name) in CODES.iter().zip(NAMES) {
            assert_eq!(entry.code.to_string(), *name);
        }
    }

    #[test]
    fn codes_are_sorted_unique_and_in_a_range() {
        for pair in CODES.windows(2) {
            assert!(pair[0].code < pair[1].code, "{:?}", pair[1].code);
        }
        for entry in CODES {
            assert!(range_of(entry.code).is_some(), "{}", entry.code);
        }
    }

    #[test]
    fn ranges_are_ordered_and_disjoint() {
        for pair in RANGES.windows(2) {
            assert!(pair[0].first <= pair[0].last);
            assert!(pair[0].last < pair[1].first);
        }
    }
}
