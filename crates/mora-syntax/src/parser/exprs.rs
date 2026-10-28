//! Expressions (§7), by Pratt parsing.
//!
//! This is the core that declarations need: literals, interpolated strings,
//! names, member access, calls, indexing, prefix and binary operators,
//! parentheses and tuples, collection expressions, `new`, `default`,
//! `nameof`, `sizeof`, macro calls and splices. Lambdas, `switch`, `with`,
//! patterns, casts, assignments, ranges and the conditional operator arrive
//! with statements in M1.4.

use super::types::{self, PREDEFINED};
use super::{CompletedMarker, Parser};
use crate::SyntaxKind;

use SyntaxKind as K;

/// Tokens that are literals on their own (§3.5).
const LITERALS: &[SyntaxKind] = &[
    K::IntNumber,
    K::RealNumber,
    K::Char,
    K::String,
    K::VerbatimString,
    K::RawString,
    K::TrueKw,
    K::FalseKw,
    K::NullKw,
];

/// Binary operators and their binding power, tightest last (§7.1).
/// `??` associates to the right; the others to the left.
const BINARY: &[(SyntaxKind, u8)] = &[
    (K::QuestionQuestion, 1),
    (K::PipePipe, 2),
    (K::AmpAmp, 3),
    (K::Pipe, 4),
    (K::Caret, 5),
    (K::Amp, 6),
    (K::EqEq, 7),
    (K::BangEq, 7),
    (K::Lt, 8),
    (K::Gt, 8),
    (K::LtEq, 8),
    (K::GtEq, 8),
    (K::Shl, 9),
    (K::Shr, 9),
    (K::UShr, 9),
    (K::Plus, 10),
    (K::Minus, 10),
    (K::PlusPercent, 10),
    (K::MinusPercent, 10),
    (K::Star, 11),
    (K::Slash, 11),
    (K::Percent, 11),
    (K::StarPercent, 11),
];

/// The binding power of prefix operators: tighter than any binary one.
const PREFIX_POWER: u8 = 12;

/// Prefix operators (§7.1); `^` is an index from the end (§7.13).
const PREFIX: &[SyntaxKind] = &[
    K::Plus,
    K::Minus,
    K::Bang,
    K::Tilde,
    K::Caret,
    K::PlusPlus,
    K::MinusMinus,
];

pub(crate) fn expr(p: &mut Parser<'_>) {
    if expr_bp(p, 0).is_none() {
        p.error("expected an expression");
    }
}

/// Whether an expression can start at the next token.
pub(crate) fn at_expr_start(p: &Parser<'_>) -> bool {
    let kind = p.current();
    LITERALS.contains(&kind)
        || PREFIX.contains(&kind)
        || PREDEFINED.contains(&kind)
        || matches!(
            kind,
            K::Ident
                | K::InterpStart
                | K::ThisKw
                | K::LParen
                | K::LBracket
                | K::NewKw
                | K::DefaultKw
                | K::SizeofKw
                | K::Hash
                | K::Dollar
        )
}

/// The binary operator at the next token, longest first, so `>>` wins
/// over `>`.
fn binary_op(p: &Parser<'_>) -> Option<(SyntaxKind, u8)> {
    const LONGEST_FIRST: &[SyntaxKind] = &[K::UShr, K::Shr, K::GtEq];
    if let Some(&kind) = LONGEST_FIRST.iter().find(|&&kind| p.at(kind)) {
        // `>>=`, `>>>=` are assignments, which M1.4 parses.
        if p.at(K::ShrEq) || p.at(K::UShrEq) {
            return None;
        }
        return BINARY.iter().copied().find(|(k, _)| *k == kind);
    }
    BINARY.iter().copied().find(|&(kind, _)| p.at(kind))
}

fn expr_bp(p: &mut Parser<'_>, min: u8) -> Option<CompletedMarker> {
    let mut lhs = unary(p)?;
    while let Some((op, power)) = binary_op(p) {
        if power < min {
            break;
        }
        let m = lhs.precede(p);
        p.bump(op);
        // Left operators bind the right operand tighter; `??` does not.
        let next = if op == K::QuestionQuestion {
            power
        } else {
            power + 1
        };
        if expr_bp(p, next).is_none() {
            p.error("expected an expression");
        }
        lhs = m.complete(p, K::BinExpr);
    }
    Some(lhs)
}

fn unary(p: &mut Parser<'_>) -> Option<CompletedMarker> {
    if PREFIX.iter().any(|&kind| p.at(kind)) {
        let m = p.start();
        p.bump_any();
        if expr_bp(p, PREFIX_POWER).is_none() {
            p.error("expected an expression");
        }
        return Some(m.complete(p, K::PrefixExpr));
    }
    let lhs = primary(p)?;
    Some(postfix(p, lhs))
}

fn primary(p: &mut Parser<'_>) -> Option<CompletedMarker> {
    let kind = p.current();
    let m = p.start();
    let done = if LITERALS.contains(&kind) {
        p.bump_any();
        m.complete(p, K::LiteralExpr)
    } else if PREDEFINED.contains(&kind) {
        // `int.Parse(...)`.
        let t = p.start();
        p.bump_any();
        t.complete(p, K::PredefinedType);
        m.complete(p, K::PredefinedTypeExpr)
    } else {
        match kind {
            K::InterpStart => {
                m.abandon(p);
                return Some(interp_string(p));
            }
            K::Ident if p.at_contextual("nameof") && p.nth(1) == K::LParen => {
                p.bump_remap(K::NameofKw);
                p.bump(K::LParen);
                expr(p);
                p.expect(K::RParen);
                m.complete(p, K::NameofExpr)
            }
            K::Ident => {
                types::path(p, false);
                m.complete(p, K::PathExpr)
            }
            K::ThisKw => {
                p.bump(K::ThisKw);
                m.complete(p, K::ThisExpr)
            }
            K::LParen => {
                m.abandon(p);
                return Some(paren_or_tuple(p));
            }
            K::LBracket => {
                m.abandon(p);
                return Some(collection(p));
            }
            K::NewKw => {
                m.abandon(p);
                return Some(new_expr(p));
            }
            K::DefaultKw => {
                p.bump(K::DefaultKw);
                if p.eat(K::LParen) {
                    types::ty(p);
                    p.expect(K::RParen);
                }
                m.complete(p, K::DefaultExpr)
            }
            K::SizeofKw => {
                p.bump(K::SizeofKw);
                p.expect(K::LParen);
                types::ty(p);
                p.expect(K::RParen);
                m.complete(p, K::SizeofExpr)
            }
            K::Hash => {
                p.bump(K::Hash);
                types::name_ref(p);
                arg_list(p);
                m.complete(p, K::MacroCallExpr)
            }
            K::Dollar => {
                splice(p);
                m.complete(p, K::SpliceExpr)
            }
            _ => {
                m.abandon(p);
                return None;
            }
        }
    };
    Some(done)
}

fn postfix(p: &mut Parser<'_>, mut lhs: CompletedMarker) -> CompletedMarker {
    loop {
        lhs = if p.at(K::Dot) || p.at(K::QuestionDot) {
            let m = lhs.precede(p);
            p.bump_any();
            if p.at(K::Dollar) {
                splice(p);
            } else {
                types::name_ref(p);
            }
            if p.at(K::Lt) && types::generic_args_follow(p.input(), p.pos()) {
                types::generic_arg_list(p);
            }
            m.complete(p, K::MemberExpr)
        } else if p.at(K::LParen) {
            let m = lhs.precede(p);
            arg_list(p);
            m.complete(p, K::CallExpr)
        } else if p.at(K::QuestionLBracket) || p.at(K::LBracket) {
            let m = lhs.precede(p);
            if !p.eat(K::QuestionLBracket) {
                p.bump(K::LBracket);
            }
            while !p.at(K::RBracket) && !p.at_end() {
                arg(p);
                if !p.eat(K::Comma) {
                    break;
                }
            }
            p.expect(K::RBracket);
            m.complete(p, K::IndexExpr)
        } else {
            return lhs;
        };
    }
}

/// `(args)`: `F(x)`, `F(ref x)`, `F(name: v)`.
pub(crate) fn arg_list(p: &mut Parser<'_>) {
    let m = p.start();
    if !p.expect(K::LParen) {
        m.complete(p, K::ArgList);
        return;
    }
    while !p.at(K::RParen) && !p.at_end() && !p.at(K::Semicolon) && !p.at(K::RBrace) {
        arg(p);
        if !p.eat(K::Comma) {
            break;
        }
    }
    p.expect(K::RParen);
    m.complete(p, K::ArgList);
}

fn arg(p: &mut Parser<'_>) {
    let m = p.start();
    if p.at(K::Ident) && p.nth(1) == K::Colon {
        types::name_ref(p);
        p.bump(K::Colon);
    }
    if p.at_any(&[K::RefKw, K::OutKw, K::InKw, K::MoveKw]) {
        p.bump_any();
    }
    expr(p);
    m.complete(p, K::Arg);
}

fn paren_or_tuple(p: &mut Parser<'_>) -> CompletedMarker {
    let m = p.start();
    p.bump(K::LParen);
    if p.eat(K::RParen) {
        return m.complete(p, K::TupleExpr);
    }
    let mut elements = 0;
    let mut named = false;
    loop {
        let e = p.start();
        if p.at(K::Ident) && p.nth(1) == K::Colon {
            types::name_ref(p);
            p.bump(K::Colon);
            named = true;
        }
        expr(p);
        e.complete(p, K::TupleElem);
        elements += 1;
        if !p.eat(K::Comma) || p.at(K::RParen) {
            break;
        }
    }
    p.expect(K::RParen);
    if elements == 1 && !named {
        // `(e)`: the element node was only a guess.
        m.complete(p, K::ParenExpr)
    } else {
        m.complete(p, K::TupleExpr)
    }
}

fn collection(p: &mut Parser<'_>) -> CompletedMarker {
    let m = p.start();
    p.bump(K::LBracket);
    while !p.at(K::RBracket) && !p.at_end() {
        let e = p.start();
        p.eat(K::DotDot);
        expr(p);
        e.complete(p, K::CollectionElem);
        if !p.eat(K::Comma) {
            break;
        }
    }
    p.expect(K::RBracket);
    m.complete(p, K::CollectionExpr)
}

/// `new T(args) { P = v }`, `new()`, `new T { P = v }`, `new int[10]`,
/// `new[] { 1, 2 }`.
fn new_expr(p: &mut Parser<'_>) -> CompletedMarker {
    let m = p.start();
    p.bump(K::NewKw);
    let has_type = types::at_type_start(p) && !p.at(K::LParen);
    if has_type {
        types::element_ty(p);
    }
    if p.at(K::LBracket) {
        p.bump(K::LBracket);
        if !p.at(K::RBracket) {
            expr(p);
        }
        p.expect(K::RBracket);
        if p.at(K::LBrace) {
            let init = p.start();
            p.bump(K::LBrace);
            while !p.at(K::RBrace) && !p.at_end() {
                expr(p);
                if !p.eat(K::Comma) {
                    break;
                }
            }
            p.expect(K::RBrace);
            init.complete(p, K::ArrayInitializer);
        }
        return m.complete(p, K::NewArrayExpr);
    }
    if p.at(K::LParen) {
        arg_list(p);
    }
    if p.at(K::LBrace) {
        object_initializer(p);
    }
    m.complete(p, K::NewExpr)
}

pub(crate) fn object_initializer(p: &mut Parser<'_>) {
    let m = p.start();
    p.bump(K::LBrace);
    while !p.at(K::RBrace) && !p.at_end() {
        let init = p.start();
        types::name_ref(p);
        p.expect(K::Eq);
        expr(p);
        init.complete(p, K::MemberInit);
        if !p.eat(K::Comma) {
            break;
        }
    }
    p.expect(K::RBrace);
    m.complete(p, K::ObjectInitializer);
}

/// `$x`, `$(expr)`, `$..xs` inside `code { }` (§9.3).
pub(crate) fn splice(p: &mut Parser<'_>) {
    let m = p.start();
    p.bump(K::Dollar);
    p.eat(K::DotDot);
    if p.eat(K::LParen) {
        expr(p);
        p.expect(K::RParen);
    } else {
        types::name_ref(p);
    }
    m.complete(p, K::Splice);
}

/// `$"x = {x,5:F2}"`: text pieces and holes between the start and the end.
fn interp_string(p: &mut Parser<'_>) -> CompletedMarker {
    let m = p.start();
    p.bump(K::InterpStart);
    loop {
        if p.at(K::InterpText) {
            let t = p.start();
            p.bump(K::InterpText);
            t.complete(p, K::InterpTextPart);
        } else if p.at(K::HoleOpen) {
            let h = p.start();
            p.bump(K::HoleOpen);
            expr(p);
            if p.eat(K::Comma) {
                expr(p);
            }
            p.eat(K::HoleFormat);
            if !p.eat(K::HoleClose) {
                p.error("expected the `}` that closes the hole");
                // Skip to the end of the hole or the string.
                while !p.at_end() && !p.at_any(&[K::HoleClose, K::InterpEnd]) {
                    p.bump_any();
                }
                p.eat(K::HoleClose);
            }
            h.complete(p, K::InterpHole);
        } else {
            break;
        }
    }
    // A string the lexer found unclosed has no end token; MR0012 says so.
    p.eat(K::InterpEnd);
    m.complete(p, K::InterpString)
}
