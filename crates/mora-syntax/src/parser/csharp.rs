//! C# constructs that Mora replaces (§3.4, §4.2, §4.3, §7.4, §7.12). Rather
//! than `MR0110`, each gets its own error code and a note on what to write
//! instead, and the parser takes the whole construct, so the errors after it
//! are real ones.

use mora_diag::{Code, codes};
use mora_span::{TextRange, TextSize};

use super::{CompletedMarker, Parser, exprs, stmts, types};
use crate::{SyntaxError, SyntaxFix, SyntaxKind};

use SyntaxKind as K;

const EXCEPTIONS: (Code, &str, &str) = (
    codes::MR0101,
    "Mora has no exceptions",
    "Return `Result<T, E>` and propagate errors with `?`.",
);

const INHERITANCE: (Code, &str, &str) = (
    codes::MR0108,
    "Mora has no class inheritance",
    "Use an interface with default methods.",
);

/// The code, the reason and the note for a reserved C# keyword (§3.4).
fn reserved(kind: SyntaxKind) -> Option<(Code, &'static str, &'static str)> {
    Some(match kind {
        K::TryKw | K::CatchKw | K::FinallyKw | K::ThrowKw => EXCEPTIONS,
        K::ObjectKw | K::DynamicKw => (
            codes::MR0102,
            "there is no universal base type",
            "Use a generic parameter or an interface.",
        ),
        K::TypeofKw => (
            codes::MR0103,
            "there is no runtime reflection",
            "Use `nameof`, a generic constraint or a macro.",
        ),
        K::LockKw | K::VolatileKw => (
            codes::MR0104,
            "Mora has no `lock` or `volatile`",
            "Use `Mutex<T>` or `Atomic<T>`.",
        ),
        K::DelegateKw | K::EventKw => (
            codes::MR0105,
            "Mora has no delegates or events",
            "Use a function type such as `Func<A, R>` or `Action<A>`, and `Signal<T>` for events.",
        ),
        K::GotoKw => (
            codes::MR0106,
            "Mora has no `goto`",
            "Use a loop with `break` or `continue`, or split the code into functions.",
        ),
        K::FixedKw | K::UnsafeKw => (
            codes::MR0107,
            "Mora has no unsafe code yet",
            "Unsafe code is planned after the first release.",
        ),
        K::AbstractKw | K::VirtualKw | K::OverrideKw | K::SealedKw | K::ProtectedKw | K::BaseKw => {
            INHERITANCE
        }
        _ => return None,
    })
}

/// Reports the reserved keyword at the next token.
fn report_keyword(p: &mut Parser<'_>) {
    if let Some((code, reason, note)) = reserved(p.current()) {
        let message = format!("`{}`: {reason}", p.nth_text(0));
        let range = p.range(0);
        p.report(SyntaxError::new(code, range, message).with_note(note));
    }
}

/// Takes `( ... )` with whatever is inside, if it is there.
fn skip_parens(p: &mut Parser<'_>) {
    if !p.at(K::LParen) {
        return;
    }
    let mut depth = 0usize;
    while !p.at_end() {
        match p.current() {
            K::LParen => depth += 1,
            K::RParen => depth -= 1,
            _ => {}
        }
        p.bump_any();
        if depth == 0 {
            return;
        }
    }
}

/// Takes the tokens up to and including the next `;`, stopping before a
/// `}` that would close the enclosing block.
fn skip_to_semicolon(p: &mut Parser<'_>) {
    while !p.at_end() && !p.at(K::RBrace) {
        if p.eat(K::Semicolon) {
            return;
        }
        p.bump_any();
    }
}

/// A C# statement that Mora does not have, at the start of a statement.
/// Returns `false` without taking anything when there is none.
pub(crate) fn statement(p: &mut Parser<'_>) -> bool {
    let kind = p.current();
    let m = p.start();
    match kind {
        // `try { } catch (E e) when (c) { } finally { }`, or a `catch` or
        // `finally` left on its own.
        K::TryKw | K::CatchKw | K::FinallyKw => {
            report_keyword(p);
            p.eat(K::TryKw);
            if p.at(K::LBrace) {
                stmts::block(p);
            }
            loop {
                if p.eat(K::CatchKw) {
                    skip_parens(p);
                    if p.at_contextual("when") {
                        p.bump_any();
                        skip_parens(p);
                    }
                } else if !p.eat(K::FinallyKw) {
                    break;
                }
                if p.at(K::LBrace) {
                    stmts::block(p);
                }
            }
        }
        // `throw;` and `throw new E();`.
        K::ThrowKw => {
            report_keyword(p);
            p.bump(K::ThrowKw);
            if !p.at(K::Semicolon) {
                exprs::expr(p);
            }
            p.expect(K::Semicolon);
        }
        // `lock (gate) { }` and `fixed (int* q = p) { }`.
        K::LockKw | K::FixedKw => {
            report_keyword(p);
            p.bump_any();
            skip_parens(p);
            stmts::statement(p);
        }
        // `unsafe { }`, or `unsafe` before a local function.
        K::UnsafeKw => {
            report_keyword(p);
            p.bump(K::UnsafeKw);
            stmts::statement(p);
        }
        // `goto done;`, `goto case 1;`.
        K::GotoKw => {
            report_keyword(p);
            skip_to_semicolon(p);
        }
        // `done:`, the label a `goto` jumps to.
        K::Ident if p.nth(1) == K::Colon => {
            let range = TextRange::new(p.range(0).start(), p.range(1).end());
            p.report(
                SyntaxError::new(
                    codes::MR0106,
                    range,
                    "a statement label: Mora has no `goto`",
                )
                .with_note(
                    "Use a loop with `break` or `continue`, or split the code into functions.",
                ),
            );
            p.bump(K::Ident);
            p.bump(K::Colon);
        }
        // `delegate void Handler(int x);`
        K::DelegateKw if p.nth(1) != K::LParen && p.nth(1) != K::LBrace => {
            report_keyword(p);
            skip_to_semicolon(p);
        }
        K::UsingKw if at_using_statement(p) => using_statement(p),
        // `checked { ... }`
        K::Ident if at_checked(p) && p.nth(1) == K::LBrace => {
            report_checked(p);
            p.bump_any();
            stmts::block(p);
        }
        _ => {
            m.abandon(p);
            return false;
        }
    }
    m.complete(p, K::Error);
    true
}

/// `using (var f = Open()) { }`, `using var f = Open();` or
/// `using File f = Open();`, rather than a `using` directive.
pub(crate) fn at_using_statement(p: &Parser<'_>) -> bool {
    match p.nth(1) {
        K::LParen | K::VarKw | K::LetKw => true,
        _ => types::scan_type(p.input(), p.pos() + 1)
            .is_some_and(|end| p.input().kind(end) == K::Ident && p.input().kind(end + 1) == K::Eq),
    }
}

fn using_statement(p: &mut Parser<'_>) {
    let range = p.range(0);
    p.report(
        SyntaxError::new(
            codes::MR0121,
            range,
            "`using`: Mora has no disposal statement",
        )
        .with_note(
            "A value's destructor runs when its owner goes out of scope, so \
             declare it without `using`.",
        ),
    );
    p.bump(K::UsingKw);
    if p.at(K::LParen) {
        skip_parens(p);
        stmts::statement(p);
    } else {
        stmts::local_decl_stmt(p);
    }
}

/// C# modifiers that Mora reserves.
pub(crate) const RESERVED_MODIFIERS: &[SyntaxKind] = &[
    K::VirtualKw,
    K::OverrideKw,
    K::SealedKw,
    K::ProtectedKw,
    K::VolatileKw,
    K::EventKw,
    K::UnsafeKw,
    K::FixedKw,
];

/// Whether a reserved modifier starts a declaration here, rather than the
/// statements `unsafe { ... }` and `fixed (...) ...`.
pub(crate) fn at_reserved_modifier(p: &Parser<'_>) -> bool {
    p.at_any(RESERVED_MODIFIERS)
        && !(p.at(K::UnsafeKw) && p.nth(1) == K::LBrace)
        && !(p.at(K::FixedKw) && p.nth(1) == K::LParen)
}

/// A reserved C# keyword among the modifiers of a declaration, such as
/// `virtual` or `event`. Takes it and returns `true` if there is one.
/// `abstract` is a modifier of Mora's static abstract interface members and
/// is checked once all the modifiers are known.
pub(crate) fn modifier(p: &mut Parser<'_>) -> bool {
    let at_reserved = p.at_any(RESERVED_MODIFIERS);
    if at_reserved {
        report_keyword(p);
        p.bump_any();
    }
    at_reserved
}

/// Reports `abstract` without `static`: an abstract class or method.
pub(crate) fn check_abstract(p: &mut Parser<'_>, modifiers: &[(SyntaxKind, TextRange)]) {
    let is_static = modifiers.iter().any(|(kind, _)| *kind == K::StaticKw);
    if is_static {
        return;
    }
    for (kind, range) in modifiers {
        if *kind == K::AbstractKw {
            let (code, reason, note) = INHERITANCE;
            p.report(
                SyntaxError::new(code, *range, format!("`abstract`: {reason}")).with_note(note),
            );
        }
    }
}

/// A C# expression that Mora does not have, at the start of an operand:
/// `typeof(T)`, `base`, `throw e` or an anonymous `delegate`.
pub(crate) fn primary(p: &mut Parser<'_>) -> Option<CompletedMarker> {
    let kind = p.current();
    if !matches!(
        kind,
        K::TypeofKw | K::BaseKw | K::ThrowKw | K::DelegateKw | K::ObjectKw | K::DynamicKw
    ) {
        return None;
    }
    let m = p.start();
    report_keyword(p);
    p.bump_any();
    match kind {
        K::TypeofKw => {
            if p.eat(K::LParen) {
                types::ty(p);
                p.expect(K::RParen);
            }
        }
        K::ThrowKw => exprs::expr(p),
        K::DelegateKw => {
            skip_parens(p);
            if p.at(K::LBrace) {
                stmts::block(p);
            }
        }
        _ => {}
    }
    Some(m.complete(p, K::Error))
}

/// `null`, reported and taken as the `none` it stands for, so the tree
/// and everything after it read as if `none` had been written.
pub(crate) fn null_literal(p: &mut Parser<'_>) -> Option<CompletedMarker> {
    if !p.at(K::NullKw) {
        return None;
    }
    let range = p.range(0);
    p.report(
        SyntaxError::new(
            codes::MR0129,
            range,
            "`null`: Mora writes the empty option as `none`",
        )
        .with_note("A `T?` holds either a value or `none`; no other type can be empty.")
        .with_fix(SyntaxFix {
            message: "write `none`".to_owned(),
            edits: vec![(range, "none".to_owned())],
        }),
    );
    let m = p.start();
    p.bump_remap(K::NoneKw);
    Some(m.complete(p, K::LiteralExpr))
}

/// `object` or `dynamic` where a type goes.
pub(crate) fn reserved_type(p: &mut Parser<'_>) -> CompletedMarker {
    let m = p.start();
    report_keyword(p);
    p.bump_any();
    m.complete(p, K::Error)
}

/// Whether `checked` or `unchecked` starts here.
pub(crate) fn at_checked(p: &Parser<'_>) -> bool {
    p.at_contextual("checked") || p.at_contextual("unchecked")
}

/// Reports `checked` or `unchecked`; the caller parses what follows.
pub(crate) fn report_checked(p: &mut Parser<'_>) {
    let message = format!(
        "`{}`: Mora sets overflow checking per build profile",
        p.nth_text(0)
    );
    let range = p.range(0);
    p.report(SyntaxError::new(codes::MR0109, range, message).with_note(
        "Set `overflow-checks` in the profile, or use the wrapping operators \
             `+%`, `-%` and `*%`.",
    ));
}

/// Reports the null-forgiving `!` at the next token; the caller takes it.
pub(crate) fn report_null_forgiving(p: &mut Parser<'_>) {
    let range = p.range(0);
    p.report(
        SyntaxError::new(
            codes::MR0160,
            range,
            "`x!`: Mora has no null-forgiving operator",
        )
        .with_note("Use `.Value`, or `.Expect(\"why it is set\")` to say why it cannot be `null`."),
    );
}

/// A block namespace waiting for its `}`.
pub(crate) struct OpenNamespace {
    /// From `namespace` to the end of the name.
    range: TextRange,
    /// The end of the name, where the `;` goes.
    name_end: TextSize,
    /// The `{`.
    open: TextRange,
    /// Whether the fix can rewrite it: the first namespace of the file and
    /// not nested in another.
    fixable: bool,
}

impl OpenNamespace {
    pub(crate) fn new(
        range: TextRange,
        name_end: TextSize,
        open: TextRange,
        fixable: bool,
    ) -> Self {
        Self {
            range,
            name_end,
            open,
            fixable,
        }
    }

    /// Reports `MR0120`, with the fix when `close`, the range of the `}`, is
    /// known and nothing stops the rewrite.
    /// Keeps the fix from being offered: another namespace is nested in it.
    pub(crate) fn forbid_fix(&mut self) {
        self.fixable = false;
    }

    pub(crate) fn report(self, p: &mut Parser<'_>, close: Option<TextRange>) {
        let mut error = SyntaxError::new(
            codes::MR0120,
            self.range,
            "a namespace declaration is file-scoped, without braces",
        )
        .with_note("Write `namespace Name;` before the declarations of the file.");
        if let Some(close) = close
            && self.fixable
        {
            error = error.with_fix(SyntaxFix {
                message: "use a file-scoped namespace".to_owned(),
                edits: vec![
                    (TextRange::empty(self.name_end), ";".to_owned()),
                    (whole_line(p.input().source(), self.open), String::new()),
                    (whole_line(p.input().source(), close), String::new()),
                ],
            });
        }
        p.report(error);
    }
}

/// `range`, widened to its whole line, line break included, when nothing
/// else is on that line.
fn whole_line(text: &str, range: TextRange) -> TextRange {
    let start = usize::from(range.start());
    let end = usize::from(range.end());
    let line_start = text[..start].rfind('\n').map_or(0, |i| i + 1);
    let line_end = text[end..].find('\n').map_or(text.len(), |i| end + i + 1);
    if text[line_start..start].trim().is_empty() && text[end..line_end].trim().is_empty() {
        TextRange::new(size(line_start), size(line_end))
    } else {
        range
    }
}

fn size(offset: usize) -> TextSize {
    TextSize::new(u32::try_from(offset).unwrap_or(u32::MAX))
}
