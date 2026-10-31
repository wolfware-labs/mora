//! The parser (§14.4).
//!
//! Recursive descent over the tokens that are not trivia, with Pratt
//! parsing for expressions. The parser does not build the tree itself: it
//! emits events (open a node, take a token, close a node, report an error),
//! and [`build`] turns them into a tree, putting whitespace and comments back
//! in place. A marker can be preceded by a new node after it is closed, which
//! is how `a + b` wraps the already parsed `a` in a binary expression.
//!
//! The parser never stops at an error. It reports `MR0110`, wraps what it
//! cannot use in an `Error` node, and goes on at the next declaration or
//! member, so every later declaration still gets its tree.

mod build;
mod csharp;
mod exprs;
mod items;
mod patterns;
mod stmts;
mod types;

use mora_diag::codes;
use mora_span::{TextRange, TextSize};

use crate::ast::{self, AstNode};
use crate::lexer::{Lexed, lex};
use crate::{GreenNode, SyntaxError, SyntaxKind, SyntaxNode};

use SyntaxKind as K;

/// A parsed file: its tree and every lexical and syntax error in it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Parse {
    green: GreenNode,
    errors: Vec<SyntaxError>,
}

impl Parse {
    /// The root of the tree.
    pub fn syntax(&self) -> SyntaxNode {
        self.green.root()
    }

    /// The typed view of the root.
    pub fn tree(&self) -> ast::SourceFile {
        // The root is always a `SourceFile`: `parse` opens it first.
        ast::SourceFile::cast(self.syntax()).unwrap_or_else(|| unreachable!())
    }

    /// The lexical and syntax errors, in the order of the text.
    pub fn errors(&self) -> &[SyntaxError] {
        &self.errors
    }

    pub fn green(&self) -> &GreenNode {
        &self.green
    }
}

/// Parses a source file. Every byte of `text` ends up in the tree.
pub fn parse(text: &str) -> Parse {
    let lexed = lex(text);
    let input = Input::new(text, &lexed);
    let mut p = Parser::new(&input);
    items::source_file(&mut p);
    let (events, reports) = p.finish();
    let (green, mut errors) = build::build(text, &lexed, events);
    errors.extend(reports);
    errors.extend(lexed.errors.iter().cloned());
    errors.sort_by_key(|error| error.range.start());
    Parse { green, errors }
}

/// The tokens the parser sees: everything but trivia, with each token's
/// place among all tokens and whether it touches the next one.
pub(crate) struct Input<'t> {
    /// The whole text.
    text: &'t str,
    /// The kind of each token.
    kinds: Vec<SyntaxKind>,
    /// Where each token is.
    ranges: Vec<TextRange>,
    /// The index of each token in [`Lexed::tokens`].
    raw: Vec<usize>,
    /// Whether the token is directly followed by the next one, with no trivia.
    joined: Vec<bool>,
}

impl<'t> Input<'t> {
    fn new(text: &'t str, lexed: &Lexed) -> Self {
        let mut input = Input {
            text,
            kinds: Vec::new(),
            ranges: Vec::new(),
            raw: Vec::new(),
            joined: Vec::new(),
        };
        for (i, token) in lexed.tokens.iter().enumerate() {
            if token.kind.is_trivia() {
                continue;
            }
            input.kinds.push(token.kind);
            input.ranges.push(token.range);
            input.raw.push(i);
            input.joined.push(lexed.is_joined(i));
        }
        input
    }

    /// The kind of token `i`, or `Error` past the end (an end marker that
    /// matches nothing the grammar expects).
    fn kind(&self, i: usize) -> SyntaxKind {
        self.kinds.get(i).copied().unwrap_or(K::Error)
    }

    fn text(&self, i: usize) -> &'t str {
        self.ranges.get(i).map_or("", |range| &self.text[*range])
    }

    /// The whole text.
    pub(crate) fn source(&self) -> &'t str {
        self.text
    }

    /// Where token `i` is, or an empty range at the end of the text past
    /// the last token.
    pub(crate) fn range(&self, i: usize) -> TextRange {
        self.ranges
            .get(i)
            .copied()
            .unwrap_or_else(|| TextRange::empty(TextSize::of(self.text)))
    }

    fn is_joined(&self, i: usize) -> bool {
        self.joined.get(i).copied().unwrap_or(false)
    }

    fn len(&self) -> usize {
        self.kinds.len()
    }
}

/// What the parser did, in order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Event {
    /// Open a node. `forward_parent` points at a later `Start` that wraps
    /// this one, set by [`CompletedMarker::precede`].
    Start {
        kind: SyntaxKind,
        forward_parent: Option<usize>,
    },
    /// Close the innermost open node.
    Finish,
    /// Take `n_raw` adjacent tokens as one token of `kind`: `>>` from two `>`.
    Token { kind: SyntaxKind, n_raw: usize },
    /// Report `MR0110` at the next token.
    Error { message: String },
    /// A `Start` that was abandoned or moved.
    Tombstone,
}

pub(crate) struct Parser<'t> {
    input: &'t Input<'t>,
    pos: usize,
    events: Vec<Event>,
    /// Errors with their own code and range, such as `MR0101` on `try`.
    reports: Vec<SyntaxError>,
    no_lambda: bool,
}

/// A node that is open: it must be completed or abandoned.
pub(crate) struct Marker {
    pos: usize,
}

/// A node that is closed and can still be wrapped by a new one.
#[derive(Clone, Copy)]
pub(crate) struct CompletedMarker {
    pos: usize,
}

impl Marker {
    /// Whether an error without its own code was taken since this node
    /// opened.
    pub(crate) fn has_errors(&self, p: &Parser<'_>) -> bool {
        p.events[self.pos..]
            .iter()
            .any(|event| matches!(event, Event::Error { .. }))
    }

    pub(crate) fn complete(self, p: &mut Parser<'_>, kind: SyntaxKind) -> CompletedMarker {
        if let Some(Event::Start { kind: slot, .. }) = p.events.get_mut(self.pos) {
            *slot = kind;
        }
        p.events.push(Event::Finish);
        CompletedMarker { pos: self.pos }
    }

    /// Drops the node; its children go to the parent.
    pub(crate) fn abandon(self, p: &mut Parser<'_>) {
        if self.pos + 1 == p.events.len() {
            p.events.pop();
        } else if let Some(event) = p.events.get_mut(self.pos) {
            *event = Event::Tombstone;
        }
    }
}

impl CompletedMarker {
    /// Opens a node that starts where this one starts and contains it.
    pub(crate) fn precede(self, p: &mut Parser<'_>) -> Marker {
        let m = p.start();
        if let Some(Event::Start { forward_parent, .. }) = p.events.get_mut(self.pos) {
            *forward_parent = Some(m.pos);
        }
        m
    }
}

/// The joined kinds and the pieces they are made of (§14.4).
const JOINED: &[(SyntaxKind, &[SyntaxKind])] = &[
    (K::UShrEq, &[K::Gt, K::Gt, K::Gt, K::Eq]),
    (K::UShr, &[K::Gt, K::Gt, K::Gt]),
    (K::ShrEq, &[K::Gt, K::Gt, K::Eq]),
    (K::Shr, &[K::Gt, K::Gt]),
    (K::GtEq, &[K::Gt, K::Eq]),
    (K::QuestionLBracket, &[K::Question, K::LBracket]),
];

impl<'t> Parser<'t> {
    fn new(input: &'t Input<'t>) -> Self {
        Parser {
            input,
            pos: 0,
            events: Vec::new(),
            reports: Vec::new(),
            no_lambda: false,
        }
    }

    fn finish(self) -> (Vec<Event>, Vec<SyntaxError>) {
        (self.events, self.reports)
    }

    /// Reports an error other than `MR0110`, with its own range.
    pub(crate) fn report(&mut self, error: SyntaxError) {
        self.reports.push(error);
    }

    /// How many errors other than `MR0110` have been reported, for
    /// [`Parser::withdraw`].
    pub(crate) fn reported(&self) -> usize {
        self.reports.len()
    }

    /// Withdraws the errors with `code` reported after the first `since`:
    /// inside a C# construct that is reported whole, what it contains is
    /// not reported again.
    pub(crate) fn withdraw(&mut self, since: usize, code: mora_diag::Code) {
        let mut i = 0;
        self.reports.retain(|error| {
            i += 1;
            i <= since || error.code != code
        });
    }

    /// Takes the fixes of the errors reported after the first `since` whose
    /// edits all fall within `range`, leaving the errors without them: a
    /// fix that moves the text of `range` carries them instead.
    pub(crate) fn take_fixes(
        &mut self,
        since: usize,
        range: TextRange,
    ) -> Vec<(TextRange, String)> {
        let mut edits = Vec::new();
        for error in self.reports.iter_mut().skip(since) {
            let inside = error
                .fix
                .as_ref()
                .is_some_and(|fix| fix.edits.iter().all(|(at, _)| range.contains_range(*at)));
            if inside && let Some(fix) = error.fix.take() {
                edits.extend(fix.edits);
            }
        }
        edits
    }

    /// The end of the last token taken, or the start of the text.
    pub(crate) fn prev_end(&self) -> TextSize {
        self.pos
            .checked_sub(1)
            .map_or(TextSize::new(0), |i| self.input.range(i).end())
    }

    /// Where the `n`th token from here is.
    pub(crate) fn range(&self, n: usize) -> TextRange {
        self.input.range(self.pos + n)
    }

    /// Runs `parse` with `x => ...` read as something else than a lambda,
    /// or, with `allowed`, as a lambda again. In a switch arm `None => 0`
    /// and in a guard `when ok => 1`, the `=>` ends the pattern or the
    /// guard; inside parentheses, brackets and arguments lambdas come back.
    pub(crate) fn lambdas<T>(&mut self, allowed: bool, parse: impl FnOnce(&mut Self) -> T) -> T {
        let saved = self.no_lambda;
        self.no_lambda = !allowed;
        let result = parse(self);
        self.no_lambda = saved;
        result
    }

    pub(crate) fn lambdas_allowed(&self) -> bool {
        !self.no_lambda
    }

    pub(crate) fn start(&mut self) -> Marker {
        let pos = self.events.len();
        self.events.push(Event::Start {
            kind: K::Error,
            forward_parent: None,
        });
        Marker { pos }
    }

    /// The kind of the `n`th token ahead, without joining.
    pub(crate) fn nth(&self, n: usize) -> SyntaxKind {
        self.input.kind(self.pos + n)
    }

    pub(crate) fn current(&self) -> SyntaxKind {
        self.nth(0)
    }

    pub(crate) fn nth_text(&self, n: usize) -> &'t str {
        self.input.text(self.pos + n)
    }

    pub(crate) fn at_end(&self) -> bool {
        self.pos >= self.input.len()
    }

    /// The position of the next token, for lookahead with [`types::scan_type`].
    pub(crate) fn pos(&self) -> usize {
        self.pos
    }

    pub(crate) fn input(&self) -> &'t Input<'t> {
        self.input
    }

    /// The number of tokens `kind` takes at `pos`, if it is there: joined
    /// kinds take several adjacent pieces.
    fn pieces(&self, at: usize, kind: SyntaxKind) -> Option<usize> {
        if let Some((_, pieces)) = JOINED.iter().find(|(joined, _)| *joined == kind) {
            let matches = pieces.iter().enumerate().all(|(i, piece)| {
                self.input.kind(at + i) == *piece
                    && (i + 1 == pieces.len() || self.input.is_joined(at + i))
            });
            return matches.then_some(pieces.len());
        }
        (self.input.kind(at) == kind).then_some(1)
    }

    pub(crate) fn at(&self, kind: SyntaxKind) -> bool {
        self.pieces(self.pos, kind).is_some()
    }

    pub(crate) fn nth_at(&self, n: usize, kind: SyntaxKind) -> bool {
        self.pieces(self.pos + n, kind).is_some()
    }

    pub(crate) fn at_any(&self, kinds: &[SyntaxKind]) -> bool {
        kinds.iter().any(|&kind| self.at(kind))
    }

    /// Whether the next token is the identifier `word`, which the grammar
    /// uses as a contextual keyword here.
    pub(crate) fn at_contextual(&self, word: &str) -> bool {
        self.current() == K::Ident && self.nth_text(0) == word
    }

    /// Takes the next token as `kind`, which must be there.
    pub(crate) fn bump(&mut self, kind: SyntaxKind) {
        let n_raw = self.pieces(self.pos, kind).unwrap_or(1);
        self.events.push(Event::Token { kind, n_raw });
        self.pos += n_raw;
    }

    /// Takes the next token as it was lexed.
    pub(crate) fn bump_any(&mut self) {
        if !self.at_end() {
            let kind = self.current();
            self.bump(kind);
        }
    }

    /// Takes an identifier as the contextual keyword `kind`.
    pub(crate) fn bump_remap(&mut self, kind: SyntaxKind) {
        self.events.push(Event::Token { kind, n_raw: 1 });
        self.pos += 1;
    }

    pub(crate) fn eat(&mut self, kind: SyntaxKind) -> bool {
        if self.at(kind) {
            self.bump(kind);
            true
        } else {
            false
        }
    }

    /// Takes `kind`, or reports that it is missing and takes nothing.
    pub(crate) fn expect(&mut self, kind: SyntaxKind) -> bool {
        if self.eat(kind) {
            return true;
        }
        self.error(format!("expected {}", describe(kind)));
        false
    }

    pub(crate) fn error(&mut self, message: impl Into<String>) {
        self.events.push(Event::Error {
            message: message.into(),
        });
    }
}

/// How an error message names a token kind: `` `;` ``, `an identifier`.
pub(crate) fn describe(kind: SyntaxKind) -> String {
    match kind.text() {
        Some(text) => format!("`{text}`"),
        None => match kind {
            K::Ident => "an identifier".into(),
            K::String => "a string".into(),
            _ => format!("{kind:?}"),
        },
    }
}

/// The code of every syntax error.
pub(crate) const SYNTAX_ERROR: mora_diag::Code = codes::MR0110;
