//! Types (§5), and a lookahead that recognizes a type without parsing it.

use super::{Input, Parser, csharp, describe, exprs};
use crate::SyntaxKind;

use SyntaxKind as K;

/// The keywords that name predefined types (§5.2, §5.3).
pub(crate) const PREDEFINED: &[SyntaxKind] = &[
    K::BoolKw,
    K::ByteKw,
    K::SbyteKw,
    K::ShortKw,
    K::UshortKw,
    K::IntKw,
    K::UintKw,
    K::LongKw,
    K::UlongKw,
    K::NintKw,
    K::NuintKw,
    K::FloatKw,
    K::DoubleKw,
    K::CharKw,
    K::StringKw,
    K::VoidKw,
];

/// Parses a type, or reports that one is missing.
pub(crate) fn ty(p: &mut Parser<'_>) {
    ty_with(p, true);
}

/// The element type of `new int[10]` and `new int[] { 1 }`: the brackets
/// that follow belong to the allocation, not to the type.
pub(crate) fn element_ty(p: &mut Parser<'_>) {
    ty_with(p, false);
}

fn ty_with(p: &mut Parser<'_>, arrays: bool) {
    if !at_type_start(p) {
        p.error("expected a type");
        return;
    }
    let mut lhs = match p.current() {
        kind if PREDEFINED.contains(&kind) => {
            let m = p.start();
            p.bump_any();
            m.complete(p, K::PredefinedType)
        }
        K::LParen => tuple_type(p),
        K::ObjectKw | K::DynamicKw => csharp::reserved_type(p),
        K::WeakKw => {
            let m = p.start();
            p.bump(K::WeakKw);
            ty(p);
            m.complete(p, K::WeakType)
        }
        K::Dollar => {
            let m = p.start();
            exprs::splice(p);
            m.complete(p, K::SpliceType)
        }
        _ => {
            let m = p.start();
            path(p, true);
            m.complete(p, K::PathType)
        }
    };
    // `T?`, `T[]`, `T?[]`, `T[]?`.
    loop {
        if p.at(K::Question) {
            let m = lhs.precede(p);
            p.bump(K::Question);
            lhs = m.complete(p, K::OptionalType);
        } else if arrays && p.at(K::LBracket) && p.nth_at(1, K::RBracket) {
            let m = lhs.precede(p);
            p.bump(K::LBracket);
            p.bump(K::RBracket);
            lhs = m.complete(p, K::ArrayType);
        } else {
            break;
        }
    }
}

pub(crate) fn at_type_start(p: &Parser<'_>) -> bool {
    let kind = p.current();
    PREDEFINED.contains(&kind)
        || matches!(
            kind,
            K::Ident | K::LParen | K::WeakKw | K::Dollar | K::ObjectKw | K::DynamicKw
        )
}

fn tuple_type(p: &mut Parser<'_>) -> super::CompletedMarker {
    let m = p.start();
    p.bump(K::LParen);
    while !p.at(K::RParen) && !p.at_end() {
        let elem = p.start();
        ty(p);
        if p.at(K::Ident) {
            name(p);
        }
        elem.complete(p, K::TupleTypeElem);
        if !p.eat(K::Comma) {
            break;
        }
    }
    p.expect(K::RParen);
    m.complete(p, K::TupleType)
}

/// `Name`: a declared identifier.
pub(crate) fn name(p: &mut Parser<'_>) {
    let m = p.start();
    p.expect(K::Ident);
    m.complete(p, K::Name);
}

/// `NameRef`: a use of an identifier.
pub(crate) fn name_ref(p: &mut Parser<'_>) {
    let m = p.start();
    p.expect(K::Ident);
    m.complete(p, K::NameRef);
}

/// A path: `List<int>`, `Std.Json.JsonValue`, `Core::Parse`. In a type,
/// `<` always opens generic arguments; in an expression, only when the
/// lookahead of §14.4 says so.
pub(crate) fn path(p: &mut Parser<'_>, in_type: bool) {
    let m = p.start();
    if p.at(K::Ident) && p.nth_at(1, K::ColonColon) {
        let q = p.start();
        name_ref(p);
        p.bump(K::ColonColon);
        q.complete(p, K::AliasQualifier);
    }
    path_segment(p, in_type);
    // In an expression a path is one segment: later dots are member access.
    while in_type && p.at(K::Dot) && p.nth(1) == K::Ident {
        p.bump(K::Dot);
        path_segment(p, in_type);
    }
    m.complete(p, K::Path);
}

fn path_segment(p: &mut Parser<'_>, in_type: bool) {
    let m = p.start();
    name_ref(p);
    if p.at(K::Lt) && (in_type || generic_args_follow(p.input(), p.pos())) {
        generic_arg_list(p);
    }
    m.complete(p, K::PathSegment);
}

pub(crate) fn generic_arg_list(p: &mut Parser<'_>) {
    let m = p.start();
    p.bump(K::Lt);
    while !p.at(K::Gt) && !p.at_end() {
        ty(p);
        if !p.eat(K::Comma) {
            break;
        }
    }
    if !p.eat(K::Gt) {
        p.error(format!("expected {}", describe(K::Gt)));
    }
    m.complete(p, K::GenericArgList);
}

/// Whether the `<` at `at` opens generic arguments in an expression: the
/// arguments must be types and the token after the closing `>` one of
/// `( ) ] } : ; , . ? == != | ^ && || & [` (§14.4).
pub(crate) fn generic_args_follow(input: &Input<'_>, at: usize) -> bool {
    let Some(end) = scan_generic_args(input, at) else {
        return false;
    };
    matches!(
        input.kind(end),
        K::LParen
            | K::RParen
            | K::RBracket
            | K::RBrace
            | K::Colon
            | K::Semicolon
            | K::Comma
            | K::Dot
            | K::Question
            | K::EqEq
            | K::BangEq
            | K::Pipe
            | K::Caret
            | K::AmpAmp
            | K::PipePipe
            | K::Amp
            | K::LBracket
            | K::QuestionDot
    ) || end >= input.len()
}

/// The index after `<...>` at `at`, if it holds a list of types. Like
/// `generic_arg_list`, it takes an empty list and a trailing comma.
pub(crate) fn scan_generic_args(input: &Input<'_>, at: usize) -> Option<usize> {
    if input.kind(at) != K::Lt {
        return None;
    }
    let mut i = at + 1;
    loop {
        if input.kind(i) == K::Gt {
            return Some(i + 1);
        }
        i = scan_type(input, i)?;
        match input.kind(i) {
            K::Comma => i += 1,
            K::Gt => return Some(i + 1),
            _ => return None,
        }
    }
}

/// The index after a type that starts at `at`, if one does. Used to decide
/// between a declaration and an expression before parsing either.
pub(crate) fn scan_type(input: &Input<'_>, at: usize) -> Option<usize> {
    let mut i = match input.kind(at) {
        kind if PREDEFINED.contains(&kind) => at + 1,
        // `object` and `dynamic` are C# types, reported where they are parsed.
        K::ObjectKw | K::DynamicKw => at + 1,
        K::WeakKw => return scan_type(input, at + 1),
        // `$t`, `$..ts`, `$(expr)` (§9.3).
        K::Dollar => {
            let mut i = at + 1;
            if input.kind(i) == K::DotDot {
                i += 1;
            }
            match input.kind(i) {
                K::Ident => i + 1,
                K::LParen => {
                    let mut depth = 0usize;
                    loop {
                        match input.kind(i) {
                            K::LParen => depth += 1,
                            K::RParen => {
                                depth -= 1;
                                if depth == 0 {
                                    break i + 1;
                                }
                            }
                            _ if i >= input.len() => return None,
                            _ => {}
                        }
                        i += 1;
                    }
                }
                _ => return None,
            }
        }
        // `()`, the unit type.
        K::LParen if input.kind(at + 1) == K::RParen => at + 2,
        K::LParen => {
            let mut i = at + 1;
            loop {
                i = scan_type(input, i)?;
                if input.kind(i) == K::Ident {
                    i += 1;
                }
                match input.kind(i) {
                    // A trailing comma, as `tuple_type` takes: `(int, string,)`.
                    K::Comma if input.kind(i + 1) == K::RParen => break i + 2,
                    K::Comma => i += 1,
                    K::RParen => break i + 1,
                    _ => return None,
                }
            }
        }
        K::Ident => {
            let mut i = at;
            if input.kind(i + 1) == K::ColonColon {
                i += 2;
            }
            loop {
                if input.kind(i) != K::Ident {
                    return None;
                }
                i += 1;
                if input.kind(i) == K::Lt {
                    i = scan_generic_args(input, i)?;
                }
                if input.kind(i) == K::Dot && input.kind(i + 1) == K::Ident {
                    i += 1;
                } else {
                    break i;
                }
            }
        }
        _ => return None,
    };
    loop {
        match input.kind(i) {
            K::Question => i += 1,
            K::LBracket if input.kind(i + 1) == K::RBracket => i += 2,
            _ => return Some(i),
        }
    }
}
