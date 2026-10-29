//! Escape sequences in character literals and regular strings (§3.5).

use mora_diag::Code;
use mora_diag::codes;

/// A problem with one escape sequence, relative to its backslash.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct EscapeError {
    pub code: Code,
    pub len: usize,
    pub message: String,
}

/// Decodes the escape sequence at the start of `text`, which begins with a
/// backslash. Returns the character and the length of the sequence in
/// bytes, or the error and the length to skip.
pub(crate) fn escape(text: &str) -> Result<(char, usize), EscapeError> {
    let Some(c) = text.strip_prefix('\\').and_then(|t| t.chars().next()) else {
        return Err(EscapeError {
            code: codes::MR0013,
            len: 1,
            message: "a backslash at the end of the file".into(),
        });
    };
    if c == '\n' || c == '\r' {
        // The line break is not part of the escape: the literal ends here.
        return Err(EscapeError {
            code: codes::MR0013,
            len: 1,
            message: "a backslash at the end of a line".into(),
        });
    }
    let simple = match c {
        '\'' => Some('\''),
        '"' => Some('"'),
        '\\' => Some('\\'),
        '0' => Some('\0'),
        'a' => Some('\u{7}'),
        'b' => Some('\u{8}'),
        'f' => Some('\u{c}'),
        'n' => Some('\n'),
        'r' => Some('\r'),
        't' => Some('\t'),
        'v' => Some('\u{b}'),
        _ => None,
    };
    if let Some(value) = simple {
        return Ok((value, 2));
    }
    match c {
        'x' => {
            let digits = hex_run(&text[2..], 4);
            Err(EscapeError {
                code: codes::MR0011,
                len: 2 + digits,
                message: "Mora has no `\\x` escape; write `\\u{...}` with the code point".into(),
            })
        }
        'u' if text[2..].starts_with('{') => {
            let digits = hex_run(&text[3..], usize::MAX);
            let close = 3 + digits;
            if digits == 0 || digits > 6 || !text[close..].starts_with('}') {
                let len = if text[close..].starts_with('}') {
                    close + 1
                } else {
                    close
                };
                return Err(EscapeError {
                    code: codes::MR0013,
                    len,
                    message: "`\\u{...}` takes one to six hexadecimal digits".into(),
                });
            }
            scalar(&text[3..close], close + 1)
        }
        'u' => {
            let digits = hex_run(&text[2..], 4);
            if digits < 4 {
                return Err(EscapeError {
                    code: codes::MR0013,
                    len: 2 + digits,
                    message: "`\\u` takes exactly four hexadecimal digits, or `\\u{...}`".into(),
                });
            }
            scalar(&text[2..6], 6)
        }
        other => Err(EscapeError {
            code: codes::MR0013,
            len: 1 + other.len_utf8(),
            message: format!("`\\{other}` is not an escape sequence"),
        }),
    }
}

/// The number of hexadecimal digits at the start of `text`, at most `max`.
fn hex_run(text: &str, max: usize) -> usize {
    text.bytes()
        .take(max)
        .take_while(u8::is_ascii_hexdigit)
        .count()
}

/// The character with the code point written in hexadecimal `digits`.
fn scalar(digits: &str, len: usize) -> Result<(char, usize), EscapeError> {
    u32::from_str_radix(digits, 16)
        .ok()
        .and_then(char::from_u32)
        .map(|c| (c, len))
        .ok_or_else(|| EscapeError {
            code: codes::MR0013,
            len,
            message: format!(
                "U+{digits} is not a Unicode scalar value: surrogates and code points above U+10FFFF are not characters"
            ),
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn simple_escapes() {
        for (text, value) in [
            (r"\'", '\''),
            (r#"\""#, '"'),
            (r"\\", '\\'),
            (r"\0", '\0'),
            (r"\a", '\u{7}'),
            (r"\b", '\u{8}'),
            (r"\f", '\u{c}'),
            (r"\n", '\n'),
            (r"\r", '\r'),
            (r"\t", '\t'),
            (r"\v", '\u{b}'),
        ] {
            assert_eq!(escape(text), Ok((value, 2)), "{text}");
        }
    }

    #[test]
    fn unicode_escapes() {
        assert_eq!(escape(r"\u00e9x"), Ok(('\u{e9}', 6)));
        assert_eq!(escape(r"\u{1D11E}x"), Ok(('𝄞', 9)));
        assert_eq!(escape(r"\u{0}"), Ok(('\0', 5)));
        assert_eq!(escape(r"\u{10FFFF}"), Ok(('\u{10FFFF}', 10)));
    }

    #[test]
    fn bad_escapes() {
        let code = |text: &str| escape(text).map_err(|e| (e.code, e.len));
        assert_eq!(code(r"\x41"), Err((codes::MR0011, 4)));
        assert_eq!(code(r"\x"), Err((codes::MR0011, 2)));
        assert_eq!(code(r"\q"), Err((codes::MR0013, 2)));
        assert_eq!(code(r"\uD800"), Err((codes::MR0013, 6)));
        assert_eq!(code(r"\u12"), Err((codes::MR0013, 4)));
        assert_eq!(code(r"\u1"), Err((codes::MR0013, 3)));
        assert_eq!(code(r"\u"), Err((codes::MR0013, 2)));
        assert_eq!(code(r"\u{110000}"), Err((codes::MR0013, 10)));
        assert_eq!(code(r"\u{1234567}"), Err((codes::MR0013, 11)));
        assert_eq!(code(r"\u{}"), Err((codes::MR0013, 4)));
        assert_eq!(code(r"\u{12"), Err((codes::MR0013, 5)));
        assert_eq!(code("\\é"), Err((codes::MR0013, 3)));
        assert_eq!(code("\\\n"), Err((codes::MR0013, 1)));
        assert_eq!(code("\\\r\n"), Err((codes::MR0013, 1)));
    }

    #[test]
    fn braced_escape_digit_count_message() {
        for text in [r"\u{}", r"\u{1234567}"] {
            let err = escape(text).unwrap_err();
            assert!(
                err.message.contains("one to six"),
                "{text}: {}",
                err.message
            );
        }
    }
}
