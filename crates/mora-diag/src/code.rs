use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Serialize};

/// A diagnostic code: `MR` and four digits (§17.6). It serializes as its text.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(into = "String", try_from = "String")]
pub struct Code(u16);

impl Code {
    /// Largest number a code can carry.
    pub const MAX: u16 = 9999;

    /// # Panics
    ///
    /// Panics if `number` has more than four digits.
    pub const fn new(number: u16) -> Self {
        assert!(number <= Self::MAX, "diagnostic codes have four digits");
        Self(number)
    }

    pub const fn number(self) -> u16 {
        self.0
    }
}

impl fmt::Display for Code {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "MR{:04}", self.0)
    }
}

/// The text is not `MR` followed by exactly four ASCII digits.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ParseCodeError(pub String);

impl fmt::Display for ParseCodeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "`{}` is not a diagnostic code like `MR0310`", self.0)
    }
}

impl std::error::Error for ParseCodeError {}

impl FromStr for Code {
    type Err = ParseCodeError;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        let error = || ParseCodeError(text.to_owned());
        let digits = text.strip_prefix("MR").ok_or_else(error)?;
        if digits.len() != 4 || !digits.bytes().all(|b| b.is_ascii_digit()) {
            return Err(error());
        }
        digits.parse().map(Self).map_err(|_| error())
    }
}

impl From<Code> for String {
    fn from(code: Code) -> Self {
        code.to_string()
    }
}

impl TryFrom<String> for Code {
    type Error = ParseCodeError;

    fn try_from(text: String) -> Result<Self, Self::Error> {
        text.parse()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn displays_with_four_digits() {
        assert_eq!(Code::new(3).to_string(), "MR0003");
        assert_eq!(Code::new(310).to_string(), "MR0310");
        assert_eq!(Code::new(9999).to_string(), "MR9999");
    }

    #[test]
    fn parses_only_the_canonical_form() {
        assert_eq!("MR0310".parse(), Ok(Code::new(310)));
        for bad in ["MR310", "MR00310", "mr0310", "MR+310", "MR031a", "0310", ""] {
            assert!(bad.parse::<Code>().is_err(), "{bad}");
        }
    }

    #[test]
    #[should_panic(expected = "four digits")]
    fn five_digits_panic() {
        let _ = Code::new(10_000);
    }
}
