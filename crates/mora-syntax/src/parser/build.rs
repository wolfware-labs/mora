//! Turns parser events into a tree, putting trivia back in place.
//!
//! Trivia goes into the innermost open node, with one exception: comments
//! directly above a declaration, with no blank line between them, belong to
//! that declaration, so a doc comment is part of the item it documents.

use std::mem;

use mora_span::{TextRange, TextSize};

use super::{Event, SYNTAX_ERROR};
use crate::lexer::Lexed;
use crate::{GreenNode, SyntaxError, SyntaxFix, SyntaxKind, TreeBuilder};

use SyntaxKind as K;

pub(super) fn build(
    text: &str,
    lexed: &Lexed,
    mut events: Vec<Event>,
) -> (GreenNode, Vec<SyntaxError>) {
    let mut sink = Sink {
        text,
        lexed,
        raw: 0,
        depth: 0,
        builder: TreeBuilder::new(),
        errors: Vec::new(),
    };
    let mut chain = Vec::new();
    for i in 0..events.len() {
        match mem::replace(&mut events[i], Event::Tombstone) {
            Event::Start {
                kind,
                forward_parent,
            } => {
                // Open the outermost of the nodes that precede this one first.
                chain.push(kind);
                let mut next = forward_parent;
                while let Some(j) = next {
                    match mem::replace(&mut events[j], Event::Tombstone) {
                        Event::Start {
                            kind,
                            forward_parent,
                        } => {
                            chain.push(kind);
                            next = forward_parent;
                        }
                        _ => break,
                    }
                }
                for kind in chain.drain(..).rev() {
                    sink.start(kind);
                }
            }
            Event::Finish => sink.finish(),
            Event::Token { kind, n_raw } => sink.token(kind, n_raw),
            Event::Error { message, fix } => {
                let wraps_token = matches!(events.get(i + 1), Some(Event::Token { .. }));
                sink.error(message, fix, wraps_token);
            }
            Event::Tombstone => {}
        }
    }
    (sink.builder.finish(), sink.errors)
}

struct Sink<'a> {
    /// The whole text.
    text: &'a str,
    /// Every token of the text, trivia included.
    lexed: &'a Lexed,
    /// The next token of `lexed` to add.
    raw: usize,
    /// How many nodes are open.
    depth: usize,
    /// The tree so far.
    builder: TreeBuilder,
    /// The syntax errors so far.
    errors: Vec<SyntaxError>,
}

impl Sink<'_> {
    fn kind(&self, i: usize) -> Option<SyntaxKind> {
        self.lexed.tokens.get(i).map(|token| token.kind)
    }

    /// The index of the next token that is not trivia, or the end.
    fn next_significant(&self) -> usize {
        let mut i = self.raw;
        while self.kind(i).is_some_and(SyntaxKind::is_trivia) {
            i += 1;
        }
        i
    }

    fn add_raw(&mut self, until: usize) {
        while self.raw < until {
            let token = self.lexed.tokens[self.raw];
            self.builder.token(token.kind, &self.text[token.range]);
            self.raw += 1;
        }
    }

    fn start(&mut self, kind: SyntaxKind) {
        if self.depth == 0 {
            self.builder.start_node(kind);
            self.depth += 1;
            return;
        }
        let end = self.next_significant();
        let attach_from = if attaches_comments(kind) {
            self.attached_comments(end)
        } else {
            end
        };
        self.add_raw(attach_from);
        self.builder.start_node(kind);
        self.depth += 1;
        self.add_raw(end);
    }

    /// Where the comments directly above the token at `end` start: comments
    /// and the whitespace between them, up to a blank line or a directive.
    fn attached_comments(&self, end: usize) -> usize {
        let mut from = end;
        let mut i = end;
        let mut newline_seen = false;
        while i > self.raw {
            i -= 1;
            match self.kind(i) {
                Some(K::LineComment | K::DocComment | K::BlockComment) => {
                    from = i;
                    newline_seen = false;
                }
                Some(K::Whitespace) => {}
                Some(K::Newline) if newline_seen => break,
                Some(K::Newline) => newline_seen = true,
                _ => break,
            }
        }
        from
    }

    fn finish(&mut self) {
        if self.depth == 1 {
            // The trivia at the end of the file belongs to the root.
            self.add_raw(self.lexed.tokens.len());
        }
        self.builder.finish_node();
        self.depth -= 1;
    }

    fn token(&mut self, kind: SyntaxKind, n_raw: usize) {
        let first = self.next_significant();
        self.add_raw(first);
        let last = (first + n_raw).min(self.lexed.tokens.len());
        if first >= last {
            return;
        }
        let range = TextRange::new(
            self.lexed.tokens[first].range.start(),
            self.lexed.tokens[last - 1].range.end(),
        );
        self.builder.token(kind, &self.text[range]);
        self.raw = last;
    }

    /// Reports `MR0110`: on the token that follows, when the parser wraps it
    /// in an `Error` node; otherwise just after the last token taken, where
    /// something is missing.
    /// A `fix` replaces the token the error wraps; without one, it is
    /// dropped.
    fn error(&mut self, message: String, fix: Option<(String, String)>, wraps_token: bool) {
        let next = self.next_significant();
        let wrapped = if wraps_token {
            self.lexed.tokens.get(next).map(|token| token.range)
        } else {
            None
        };
        let range = if let Some(range) = wrapped {
            range
        } else {
            let end = self.lexed.tokens[..self.raw]
                .iter()
                .rev()
                .find(|token| !token.kind.is_trivia())
                .map_or(TextSize::new(0), |token| token.range.end());
            TextRange::empty(end)
        };
        let mut error = SyntaxError::new(SYNTAX_ERROR, range, message);
        if let (Some(range), Some((message, replacement))) = (wrapped, fix) {
            error = error.with_fix(SyntaxFix {
                message,
                edits: vec![(range, replacement)],
            });
        }
        self.errors.push(error);
    }
}

/// Whether comments directly above a node of this kind belong to it.
fn attaches_comments(kind: SyntaxKind) -> bool {
    matches!(
        kind,
        K::UsingDirective
            | K::StructDecl
            | K::ClassDecl
            | K::UnionDecl
            | K::EnumDecl
            | K::InterfaceDecl
            | K::MacroDecl
            | K::ExtensionDecl
            | K::FunctionDecl
            | K::FieldDecl
            | K::GetterDecl
            | K::IndexerDecl
            | K::ConstructorDecl
            | K::DestructorDecl
            | K::OperatorDecl
            | K::AssocTypeDecl
            | K::EnumMember
            | K::UnionCase
            | K::Accessor
    )
}
