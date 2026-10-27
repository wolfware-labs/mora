//! The values of character and string literal tokens (§3.5).
//!
//! These functions take the text of a token that [`lex`](super::lex)
//! produced without errors. They return `None` for text that is not such a
//! token.

use super::escape::escape;

/// The value of a [`Char`](super::TokenKind::Char) token, such as `'a'` or `'\n'`.
pub fn char_value(text: &str) -> Option<char> {
    let inner = text.strip_prefix('\'')?.strip_suffix('\'')?;
    let mut chars = inner.chars();
    match chars.next()? {
        '\\' => match escape(inner) {
            Ok((c, len)) if len == inner.len() => Some(c),
            _ => None,
        },
        c if chars.next().is_none() => Some(c),
        _ => None,
    }
}

/// The value of a [`String`](super::TokenKind::String),
/// [`VerbatimString`](super::TokenKind::VerbatimString) or
/// [`RawString`](super::TokenKind::RawString) token, and whether it has the
/// `u8` suffix.
pub fn string_value(text: &str) -> Option<(String, bool)> {
    let (text, utf8) = match text.strip_suffix("u8") {
        Some(text) => (text, true),
        None => (text, false),
    };
    let value = if let Some(body) = text.strip_prefix("@\"") {
        body.strip_suffix('"')?.replace("\"\"", "\"")
    } else if text.starts_with("\"\"\"") {
        raw_value(text)?
    } else {
        regular_value(text.strip_prefix('"')?.strip_suffix('"')?)?
    };
    Some((value, utf8))
}

/// Decodes the escapes of a regular string's contents.
fn regular_value(body: &str) -> Option<String> {
    let mut value = String::with_capacity(body.len());
    let mut rest = body;
    while let Some(at) = rest.find('\\') {
        value.push_str(&rest[..at]);
        let (c, len) = escape(&rest[at..]).ok()?;
        value.push(c);
        rest = &rest[at + len..];
    }
    value.push_str(rest);
    Some(value)
}

/// The contents of a raw string: as written on one line; on several lines,
/// the lines between the delimiters' lines with the closing quotes'
/// indentation removed. Line breaks are kept as written.
fn raw_value(text: &str) -> Option<String> {
    let quotes = text.bytes().take_while(|&b| b == b'"').count();
    let content = text.get(quotes..text.len().checked_sub(quotes)?)?;
    if !content.contains('\n') {
        return Some(content.to_owned());
    }
    let first_break = content.find('\n')? + 1;
    let last_break = content.rfind('\n')?;
    let indent = &content[last_break + 1..];
    // With no content lines, the opening and closing lines share one break.
    let body = content.get(first_break..last_break).unwrap_or_default();
    let mut value = String::with_capacity(body.len());
    for line in body.split_inclusive('\n') {
        value.push_str(
            line.strip_prefix(indent)
                .unwrap_or(line.trim_start_matches([' ', '\t'])),
        );
    }
    // The break before the closing line is not part of the value.
    if value.ends_with('\r') {
        value.pop();
    }
    Some(value)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chars() {
        for (text, value) in [
            ("'a'", 'a'),
            ("'𝄞'", '𝄞'),
            (r"'\''", '\''),
            (r"'\n'", '\n'),
            (r"'\u0041'", 'A'),
            (r"'\u{1D11E}'", '𝄞'),
            ("'\"'", '"'),
        ] {
            assert_eq!(char_value(text), Some(value), "{text}");
        }
        assert_eq!(char_value("'ab'"), None);
        assert_eq!(char_value("''"), None);
    }

    #[test]
    fn strings() {
        for (text, value, utf8) in [
            (r#""a\tb""#, "a\tb", false),
            (r#""""#, "", false),
            (r#""\u{48}i \"there\"""#, "Hi \"there\"", false),
            (r#""abc"u8"#, "abc", true),
            (r#"@"C:\temp\""x""""#, "C:\\temp\\\"x\"", false),
            (r#"@"a"u8"#, "a", true),
            (r#""""one "quoted" line""""#, "one \"quoted\" line", false),
            ("\"\"\"\n    a\n      b\n    \"\"\"", "a\n  b", false),
            ("\"\"\"\r\n  x\r\n  \"\"\"", "x", false),
            (
                "\"\"\"\"\n  \"\"\" inside\n  \"\"\"\"",
                "\"\"\" inside",
                false,
            ),
            ("\"\"\"\n  a\n\n  b\n  \"\"\"u8", "a\n\nb", true),
            ("\"\"\"\n  \"\"\"", "", false),
            ("\"\"\"\r\n\"\"\"", "", false),
        ] {
            assert_eq!(string_value(text), Some((value.to_owned(), utf8)), "{text}");
        }
    }
}
