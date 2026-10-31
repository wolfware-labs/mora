//! The fix for `"""` raw strings and `@"..."` verbatim strings (`MR0018`,
//! §3.5): the same literal written with backticks.

use std::ops::Range;

use mora_span::TextRange;

use super::{SyntaxFix, size};

/// A `"""` raw string or a verbatim string, interpolated or not, that has
/// closed.
pub(super) struct Legacy {
    /// Its prefix: `"""`, `@"`, `$@"`, `$$"""` and so on.
    pub open: Range<usize>,
    /// Its closing quotes.
    pub close: Range<usize>,
    /// Whether it is delimited by three or more quotes.
    pub raw: bool,
    /// Whether it is a verbatim string, where `""` is a quote.
    pub verbatim: bool,
    /// How many braces open or close one of its holes; zero when it is not
    /// interpolated.
    pub dollars: usize,
}

impl Legacy {
    /// The edits that write it with backticks and the same value, or `None`
    /// when no backtick string has that value. `holes` are where each hole
    /// starts and ends, in order, and may include the holes of strings
    /// nested in its holes.
    pub fn fix(&self, text: &str, holes: &[(usize, usize)]) -> Option<SyntaxFix> {
        let content = self.open.end..self.close.start;
        // Its own holes, without the holes of strings nested in them.
        let mut own: Vec<(usize, usize)> = Vec::new();
        for &(start, end) in holes {
            if own.last().is_none_or(|&(_, own_end)| start >= own_end) {
                own.push((start, end));
            }
        }
        // The text between the holes, as written and with the escapes of a
        // verbatim string decoded.
        let mut pieces = Vec::new();
        let mut at = content.start;
        for (hole_start, hole_end) in own.iter().copied().chain([(content.end, content.end)]) {
            let old = &text[at..hole_start];
            let mut new = old.to_owned();
            if self.verbatim {
                new = new.replace("\"\"", "\"");
                if self.dollars > 0 {
                    new = new.replace("{{", "{").replace("}}", "}");
                }
            }
            pieces.push((at..hole_start, new));
            at = hole_end;
        }
        // An interpolated verbatim string needs more `$` than the longest
        // run of braces in its text, so that the braces stay text.
        let dollars = if self.verbatim && self.dollars > 0 {
            1 + pieces
                .iter()
                .map(|(_, new)| longest_run(new, b'{').max(longest_run(new, b'}')))
                .max()
                .unwrap_or(0)
        } else {
            self.dollars
        };
        let ticks = "`".repeat(longest_run(&text[content.clone()], b'`') + 1);
        // The value starts on the line after the opening backticks when it
        // spans lines, when it is empty, or when a backtick at either end
        // would join the delimiters.
        let first = pieces.first().map_or("", |(_, new)| new.as_str());
        let last = pieces.last().map_or("", |(_, new)| new.as_str());
        let lines = !self.raw && text[content.clone()].contains('\n');
        let wrap = lines || content.is_empty() || first.starts_with('`') || last.ends_with('`');
        // The line break before the closing backticks would take the value's
        // trailing carriage return with it.
        if wrap && last.ends_with('\r') {
            return None;
        }
        let newline = if wrap { "\n" } else { "" };
        // An empty value needs only the one line break.
        let close_newline = if content.is_empty() { "" } else { newline };
        let range = |r: Range<usize>| TextRange::new(size(r.start), size(r.end));
        let mut edits = vec![(
            range(self.open.clone()),
            format!("{}{ticks}{newline}", "$".repeat(dollars)),
        )];
        for (old, new) in pieces {
            if text[old.clone()] != new {
                edits.push((range(old), new));
            }
        }
        if dollars != self.dollars {
            for &(start, end) in &own {
                edits.push((range(start..start + self.dollars), "{".repeat(dollars)));
                edits.push((range(end - self.dollars..end), "}".repeat(dollars)));
            }
        }
        edits.push((range(self.close.clone()), format!("{close_newline}{ticks}")));
        Some(SyntaxFix {
            message: "write it with backticks".to_owned(),
            edits,
        })
    }
}

/// The length of the longest run of `c` in `text`.
fn longest_run(text: &str, c: u8) -> usize {
    text.as_bytes()
        .split(|&b| b != c)
        .map(<[u8]>::len)
        .max()
        .unwrap_or(0)
}
