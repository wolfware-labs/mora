//! Compilation units, declarations and members (§4, §6, §8.2, §9.2).

use super::csharp::{self, OpenNamespace};
use super::exprs;
use super::types::{self, name, name_ref, scan_type};
use super::{Marker, Parser, stmts};
use crate::{SyntaxError, SyntaxKind};
use mora_diag::codes;
use mora_span::TextRange;

use SyntaxKind as K;

/// Keywords that are always modifiers (§6.1).
const MODIFIERS: &[SyntaxKind] = &[
    K::PublicKw,
    K::InternalKw,
    K::PrivateKw,
    K::StaticKw,
    K::MutKw,
    K::ReadonlyKw,
    K::UniqueKw,
    K::SharedKw,
    K::RefKw,
    K::AsyncKw,
    K::LazyKw,
    K::ConstKw,
    K::IndirectKw,
    K::ExternKw,
    K::MoveKw,
    K::InKw,
    K::OutKw,
    K::ParamsKw,
    K::ThisKw,
    K::AbstractKw,
];

/// Tokens a member list or the file can recover at: the start of the next
/// declaration.
const ITEM_RECOVERY: &[SyntaxKind] = &[
    K::LBracket,
    K::PublicKw,
    K::InternalKw,
    K::PrivateKw,
    K::StaticKw,
    K::StructKw,
    K::ClassKw,
    K::UnionKw,
    K::EnumKw,
    K::InterfaceKw,
    K::MacroKw,
    K::ImportKw,
    K::UsingKw,
    K::NamespaceKw,
];

/// How far into a file the parser is: imports, then usings, then the file
/// namespace, then items and statements (§4.2).
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Phase {
    Imports,
    Usings,
    Namespace,
    Items,
}

pub(crate) fn source_file(p: &mut Parser<'_>) {
    let m = p.start();
    let mut phase = Phase::Imports;
    let mut namespaces = Namespaces::default();
    while !p.at_end() {
        let before = p.pos();
        if p.at(K::RBrace)
            && let Some(open) = namespaces.open.pop()
        {
            // The end of a block namespace, reported with its start.
            let close = p.range(0);
            let e = p.start();
            p.bump(K::RBrace);
            e.complete(p, K::Error);
            open.report(p, Some(close));
            continue;
        }
        phase = top_level(p, phase, &mut namespaces);
        if p.pos() == before {
            // Nothing could use the token: take it, so the loop moves on.
            let e = p.start();
            p.error("expected a declaration or a statement");
            p.bump_any();
            e.complete(p, K::Error);
        }
    }
    for open in namespaces.open.drain(..).rev() {
        open.report(p, None);
    }
    m.complete(p, K::SourceFile);
}

/// The namespace declarations seen so far.
#[derive(Default)]
struct Namespaces {
    /// Whether the file had one before.
    seen: bool,
    /// Block namespaces whose `}` is still to come, innermost last.
    open: Vec<OpenNamespace>,
}

/// Parses one top-level element and returns the phase after it. A header
/// element out of order is still parsed, with an error.
fn top_level(p: &mut Parser<'_>, phase: Phase, namespaces: &mut Namespaces) -> Phase {
    let is_using = (p.at(K::UsingKw) && !csharp::at_using_statement(p))
        || (p.at_contextual("global") && p.nth(1) == K::UsingKw);
    let (element, latest, message) = if p.at(K::ImportKw) {
        (
            Phase::Imports,
            Phase::Imports,
            "`import` declarations come first in a file",
        )
    } else if is_using {
        (
            Phase::Usings,
            Phase::Usings,
            "`using` directives come after the imports and before the namespace and the declarations",
        )
    } else if p.at(K::NamespaceKw) {
        (
            Phase::Namespace,
            Phase::Usings,
            "a file has one `namespace`, after its imports and usings and before its declarations",
        )
    } else {
        if !declaration(p, Context::TopLevel) {
            stmts::statement(p);
        }
        return Phase::Items;
    };
    if phase > latest {
        p.error(message);
    }
    match element {
        Phase::Imports => import_decl(p),
        Phase::Usings => using_directive(p),
        _ => file_namespace(p, namespaces),
    }
    phase.max(element)
}

/// `import Core;`, `import Core from "../core";` (§4.4).
fn import_decl(p: &mut Parser<'_>) {
    let m = p.start();
    p.bump(K::ImportKw);
    name(p);
    if p.at_contextual("from") {
        p.bump_remap(K::FromKw);
        p.expect(K::String);
    }
    p.expect(K::Semicolon);
    m.complete(p, K::ImportDecl);
}

/// `using X;`, `using static X;`, `using A = X;`, `global using X;` (§4.3).
fn using_directive(p: &mut Parser<'_>) {
    let m = p.start();
    if p.at_contextual("global") {
        p.bump_remap(K::GlobalKw);
    }
    p.bump(K::UsingKw);
    p.eat(K::StaticKw);
    if p.at(K::Ident) && p.nth(1) == K::Eq {
        let alias = p.start();
        name(p);
        p.bump(K::Eq);
        alias.complete(p, K::UsingAlias);
    }
    types::path(p, true);
    p.expect(K::Semicolon);
    m.complete(p, K::UsingDirective);
}

/// `namespace Users.Api;` (§4.2), or the C# block form `namespace X {`,
/// whose `}` [`source_file`] takes.
fn file_namespace(p: &mut Parser<'_>, namespaces: &mut Namespaces) {
    let m = p.start();
    let start = p.range(0).start();
    p.bump(K::NamespaceKw);
    types::path(p, true);
    if p.at(K::LBrace) {
        let name_end = p.prev_end();
        let open = p.range(0);
        let e = p.start();
        p.bump(K::LBrace);
        e.complete(p, K::Error);
        // Only the first namespace of a file, with nothing nested in it,
        // can become the file-scoped one.
        let fixable = !namespaces.seen;
        for outer in &mut namespaces.open {
            outer.forbid_fix();
        }
        namespaces.open.push(OpenNamespace::new(
            TextRange::new(start, name_end),
            name_end,
            open,
            fixable && namespaces.open.is_empty(),
        ));
    } else {
        p.expect(K::Semicolon);
    }
    namespaces.seen = true;
    m.complete(p, K::FileNamespace);
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Context {
    /// At the top of a file, where statements are allowed too.
    TopLevel,
    /// In the member list of a type.
    Member,
}

/// Parses a declaration with its attributes and modifiers. At the top
/// level, returns `false` without taking anything when the next tokens are
/// a statement instead.
fn declaration(p: &mut Parser<'_>, cx: Context) -> bool {
    if cx == Context::TopLevel && !at_declaration_start(p) {
        return false;
    }
    let start = p.pos();
    let m = p.start();
    while p.at(K::LBracket) {
        attribute(p);
    }
    let attributed = p.pos() > start;
    let modifiers = modifiers(p, false);
    let has_const_or_static = modifiers
        .iter()
        .any(|k| matches!(k, K::ConstKw | K::StaticKw));
    match p.current() {
        K::StructKw => struct_or_class(p, m, K::StructKw, K::StructDecl),
        K::ClassKw => struct_or_class(p, m, K::ClassKw, K::ClassDecl),
        K::InterfaceKw => interface_decl(p, m),
        K::UnionKw => union_decl(p, m),
        K::EnumKw => enum_decl(p, m),
        K::MacroKw => macro_decl(p, m),
        K::Ident if p.at_contextual("record") && record_follows(p) => record_decl(p, m),
        K::Tilde if cx == Context::Member => destructor(p, m),
        K::ImplicitKw | K::ExplicitKw => conversion(p, m),
        K::Ident if cx == Context::Member && p.at_contextual("type") && p.nth(1) == K::Ident => {
            assoc_type(p, m)
        }
        K::Ident if cx == Context::Member && p.nth(1) == K::LParen => constructor(p, m),
        _ => match scan_type(p.input(), p.pos()).map(|end| p.input().kind(end)) {
            Some(K::OperatorKw) => operator(p, m),
            Some(K::ThisKw) => indexer(p, m, 0),
            Some(K::Ident | K::Dollar)
                if cx == Context::TopLevel
                    && attributed
                    && modifiers.is_empty()
                    && !at_function_after_type(p) =>
            {
                // `[Cfg(Debug = true)] int x = 1;`: an attributed statement.
                stmts::statement(p);
                m.complete(p, K::AttributedStmt);
            }
            Some(K::Ident | K::Dollar) => {
                // `int IList<int>.this[int i]`: an explicit indexer.
                let after_type = scan_type(p.input(), p.pos()).unwrap_or(p.pos());
                let (segments, _, end) = name_segments(p, after_type);
                if p.input().kind(end) == K::Dot && p.input().kind(end + 1) == K::ThisKw {
                    indexer(p, m, segments);
                } else {
                    member_after_type(p, m, cx, has_const_or_static);
                }
            }
            _ if p.pos() == start => {
                // Nothing taken: the caller reports and recovers.
                m.abandon(p);
                return false;
            }
            // `[Cfg(Debug = true)] Log(x);` at the top level (§6.11).
            _ if cx == Context::TopLevel && attributed && modifiers.is_empty() => {
                stmts::statement(p);
                m.complete(p, K::AttributedStmt);
            }
            _ => {
                p.error("expected a declaration after the attributes and modifiers");
                m.complete(p, K::Error);
            }
        },
    }
    true
}

/// Whether a top-level declaration starts here rather than a statement:
/// attributes, a modifier, a declaration keyword, or a type followed by a
/// name and `(` (a function).
fn at_declaration_start(p: &Parser<'_>) -> bool {
    if at_attribute(p)
        || (p.at_any(MODIFIERS) && !p.at(K::ThisKw))
        || at_contextual_modifier(p)
        || csharp::at_reserved_modifier(p)
    {
        return true;
    }
    if p.at_any(&[
        K::StructKw,
        K::ClassKw,
        K::InterfaceKw,
        K::UnionKw,
        K::EnumKw,
        K::MacroKw,
    ]) || (p.at_contextual("record") && record_follows(p))
    {
        return true;
    }
    // `int Add(int a, int b) => a + b;` and `T Max<T>(...)`.
    let Some(end) = scan_type(p.input(), p.pos()) else {
        return false;
    };
    p.input().kind(end) == K::Ident && matches!(p.input().kind(end + 1), K::LParen | K::Lt)
}

/// Whether a `[` at the top level opens an attribute rather than a
/// collection expression: an attribute names something and is followed by
/// what it applies to, while `[1, 2].Count` and `[x];` go on as expressions.
pub(crate) fn at_attribute(p: &Parser<'_>) -> bool {
    if !p.at(K::LBracket) || p.nth(1) != K::Ident {
        return false;
    }
    let input = p.input();
    // Skip every bracket group in a row: `[A]\n[B]` stacks attributes,
    // `[x][0]` indexes a collection. What follows the last one decides.
    let mut i = p.pos();
    while input.kind(i) == K::LBracket {
        let mut depth = 0usize;
        loop {
            match input.kind(i) {
                K::LBracket | K::LParen => depth += 1,
                K::RBracket | K::RParen => {
                    depth -= 1;
                    if depth == 0 {
                        break;
                    }
                }
                _ if i >= input.len() => return false,
                _ => {}
            }
            i += 1;
        }
        i += 1;
    }
    // Attributes apply to a declaration or a statement. Any operator, or
    // `is`, `as`, `switch` or `with`, continues an expression instead.
    let next = input.kind(i);
    let operator = SyntaxKind::PUNCTUATION.contains(&next)
        && !matches!(next, K::LBrace | K::At | K::Dollar | K::Hash);
    let continues = matches!(next, K::IsKw | K::AsKw | K::SwitchKw)
        || (input.kind(i) == K::Ident && input.text(i) == "with" && input.kind(i + 1) == K::LBrace);
    !operator && !continues && i < input.len()
}

/// Whether a type, a name and `(` or `<` follow: a function.
fn at_function_after_type(p: &Parser<'_>) -> bool {
    let input = p.input();
    scan_type(input, p.pos()).is_some_and(|end| {
        input.kind(end) == K::Ident && matches!(input.kind(end + 1), K::LParen | K::Lt)
    })
}

/// `record Point(...)`, `record class Point`, `record struct Point`.
fn record_follows(p: &Parser<'_>) -> bool {
    matches!(p.nth(1), K::Ident | K::ClassKw | K::StructKw)
}

/// The contextual modifiers `file` and `required`, when another word of the
/// declaration follows (§6.1).
fn at_contextual_modifier(p: &Parser<'_>) -> bool {
    (p.at_contextual("file") || p.at_contextual("required"))
        && (p.nth(1) == K::Ident || p.nth(1).is_keyword())
}

/// Takes modifiers and returns their kinds. `this` is a modifier only on
/// a parameter (an extension method's receiver, §6.9); elsewhere it starts
/// an expression or an indexer.
fn modifiers(p: &mut Parser<'_>, params: bool) -> Vec<SyntaxKind> {
    let mut kinds = Vec::new();
    let mut ranges = Vec::new();
    loop {
        let m = p.start();
        if at_contextual_modifier(p) {
            let kind = if p.at_contextual("file") {
                K::FileKw
            } else {
                K::RequiredKw
            };
            p.bump_remap(kind);
            kinds.push(kind);
        } else if p.at_any(MODIFIERS) && (params || !p.at(K::ThisKw)) {
            kinds.push(p.current());
            ranges.push((p.current(), p.range(0)));
            p.bump_any();
        } else if csharp::modifier(p) {
            // `virtual`, `event` and the like: reported, then skipped.
        } else {
            m.abandon(p);
            csharp::check_abstract(p, &ranges);
            return kinds;
        }
        m.complete(p, K::Modifier);
    }
}

/// `[Json]`, `[Cfg(Os = "linux")]`, `[Allow("MR2001"), MustUse]` (§6.11).
pub(crate) fn attribute(p: &mut Parser<'_>) {
    let m = p.start();
    p.bump(K::LBracket);
    while !p.at(K::RBracket) && !p.at_end() {
        let item = p.start();
        types::path(p, true);
        if p.at(K::LParen) {
            let args = p.start();
            p.bump(K::LParen);
            while !p.at(K::RParen) && !p.at_end() {
                let arg = p.start();
                if p.at(K::Ident) && p.nth(1) == K::Eq {
                    name_ref(p);
                    p.bump(K::Eq);
                }
                exprs::expr(p);
                arg.complete(p, K::AttributeArg);
                if !p.eat(K::Comma) {
                    break;
                }
            }
            p.expect(K::RParen);
            args.complete(p, K::AttributeArgList);
        }
        item.complete(p, K::AttributeItem);
        if !p.eat(K::Comma) {
            break;
        }
    }
    p.expect(K::RBracket);
    m.complete(p, K::Attribute);
}

/// The type declaration's name, type parameters, base list and `where`
/// clauses.
fn type_head(p: &mut Parser<'_>) {
    name(p);
    if p.at(K::Lt) {
        type_param_list(p);
    }
}

fn type_tail(p: &mut Parser<'_>) {
    if p.at(K::Colon) {
        base_list(p);
    }
    while p.at(K::WhereKw) {
        where_clause(p);
    }
}

fn struct_or_class(p: &mut Parser<'_>, m: Marker, keyword: SyntaxKind, kind: SyntaxKind) {
    p.bump(keyword);
    type_head(p);
    if p.at(K::LParen) {
        // C# 12 primary constructors are not Mora (§6.9).
        let start = p.range(0).start();
        let e = p.start();
        param_list(p);
        e.complete(p, K::Error);
        let range = TextRange::new(start, p.prev_end().max(start));
        p.report(
            SyntaxError::new(
                codes::MR0128,
                range,
                "a class or struct has no parameter list",
            )
            .with_note("Declare fields and a constructor."),
        );
    }
    type_tail(p);
    if !p.eat(K::Semicolon) {
        member_list(p);
    }
    m.complete(p, kind);
}

fn record_decl(p: &mut Parser<'_>, m: Marker) {
    p.bump_remap(K::RecordKw);
    if p.at(K::ClassKw) || p.at(K::StructKw) {
        p.bump_any();
    }
    type_head(p);
    if p.at(K::LParen) {
        param_list(p);
    }
    type_tail(p);
    if !p.eat(K::Semicolon) {
        member_list(p);
    }
    m.complete(p, K::RecordDecl);
}

fn interface_decl(p: &mut Parser<'_>, m: Marker) {
    p.bump(K::InterfaceKw);
    type_head(p);
    type_tail(p);
    member_list(p);
    m.complete(p, K::InterfaceDecl);
}

/// `union Json { Null, Bool(bool Value), indirect Array(List<Json> Items); members }`.
fn union_decl(p: &mut Parser<'_>, m: Marker) {
    p.bump(K::UnionKw);
    type_head(p);
    type_tail(p);
    let body = p.start();
    if p.expect(K::LBrace) {
        while !p.at_any(&[K::RBrace, K::Semicolon]) && !p.at_end() {
            let case = p.start();
            while p.at(K::LBracket) {
                attribute(p);
            }
            p.eat(K::IndirectKw);
            name(p);
            if p.at(K::LParen) {
                param_list(p);
            }
            case.complete(p, K::UnionCase);
            if !p.eat(K::Comma) {
                break;
            }
        }
        if p.eat(K::Semicolon) {
            members(p);
        }
        p.expect(K::RBrace);
    }
    body.complete(p, K::UnionBody);
    m.complete(p, K::UnionDecl);
}

/// `enum Color : byte { Red, Green, Blue = 10 }`.
fn enum_decl(p: &mut Parser<'_>, m: Marker) {
    p.bump(K::EnumKw);
    name(p);
    if p.at(K::Colon) {
        let base = p.start();
        p.bump(K::Colon);
        types::ty(p);
        base.complete(p, K::EnumBase);
    }
    let body = p.start();
    if p.expect(K::LBrace) {
        while !p.at(K::RBrace) && !p.at_end() {
            let member = p.start();
            while p.at(K::LBracket) {
                attribute(p);
            }
            name(p);
            if p.eat(K::Eq) {
                exprs::expr(p);
            }
            member.complete(p, K::EnumMember);
            if !p.eat(K::Comma) {
                break;
            }
        }
        p.expect(K::RBrace);
    }
    body.complete(p, K::EnumBody);
    m.complete(p, K::EnumDecl);
}

/// `macro Name(params) adds T { ... }` (§9.2).
fn macro_decl(p: &mut Parser<'_>, m: Marker) {
    p.bump(K::MacroKw);
    name(p);
    param_list(p);
    if p.at_contextual("adds") {
        let adds = p.start();
        p.bump_remap(K::AddsKw);
        types::ty(p);
        while p.eat(K::Comma) {
            types::ty(p);
        }
        adds.complete(p, K::AddsClause);
    }
    stmts::block(p);
    m.complete(p, K::MacroDecl);
}

pub(crate) fn member_list(p: &mut Parser<'_>) {
    let m = p.start();
    if p.expect(K::LBrace) {
        members(p);
        p.expect(K::RBrace);
    }
    m.complete(p, K::MemberList);
}

/// Members up to the closing `}`, recovering at each member boundary.
fn members(p: &mut Parser<'_>) {
    while !p.at(K::RBrace) && !p.at_end() {
        let before = p.pos();
        // `$..members;` is a splice of members; `$T value;` declares a field.
        let declares = scan_type(p.input(), p.pos())
            .is_some_and(|end| matches!(p.input().kind(end), K::Ident | K::Dollar));
        if p.at(K::Dollar) && !declares {
            let m = p.start();
            exprs::splice(p);
            p.eat(K::Semicolon);
            m.complete(p, K::SpliceMember);
        } else if !(p.at(K::DelegateKw) && csharp::statement(p)) {
            declaration(p, Context::Member);
        }
        if p.pos() == before {
            // Skip to something that can start a member, as one error.
            let e = p.start();
            p.error("expected a member");
            p.bump_any();
            while !p.at_end()
                && !p.at(K::RBrace)
                && !p.at_any(ITEM_RECOVERY)
                && !types::at_type_start(p)
            {
                p.bump_any();
            }
            e.complete(p, K::Error);
        }
    }
}

/// After a type: a function, a property or a field, possibly with an
/// explicit interface implementation `IShape.Area`.
fn member_after_type(p: &mut Parser<'_>, m: Marker, cx: Context, has_const_or_static: bool) {
    // Decide from the tokens after the name before taking anything.
    let after_type = scan_type(p.input(), p.pos()).unwrap_or(p.pos());
    let (segments, name_at, _) = name_segments(p, after_type);
    let next = p.input().kind(name_at + 1);
    let is_function = matches!(next, K::LParen | K::Lt);
    let is_property = cx == Context::Member && matches!(next, K::LBrace | K::FatArrow);
    if cx == Context::TopLevel && !is_function && !has_const_or_static {
        // `int x = 1;` at the top level is a local variable (§4.2).
        m.abandon(p);
        stmts::local_decl_stmt(p);
        return;
    }
    types::ty(p);
    if segments > 1 {
        explicit_impl(p, segments - 1);
    }
    if p.at(K::Dollar) {
        exprs::splice(p);
    } else if is_function || is_property {
        name(p);
    }
    if is_function {
        if p.at(K::Lt) {
            type_param_list(p);
        }
        param_list(p);
        while p.at(K::WhereKw) {
            where_clause(p);
        }
        body_or_semicolon(p);
        m.complete(p, K::FunctionDecl);
    } else if is_property {
        if p.at(K::LBrace) {
            accessor_list(p);
            if p.eat(K::Eq) {
                exprs::expr(p);
                p.expect(K::Semicolon);
            }
        } else {
            expr_body(p);
            p.expect(K::Semicolon);
        }
        m.complete(p, K::PropertyDecl);
    } else {
        variable_declarators(p);
        p.expect(K::Semicolon);
        m.complete(p, K::FieldDecl);
    }
}

/// The dotted name after the type at `at`: how many segments it has
/// (more than one means an explicit implementation, `IShape.Area`), the
/// index of the last one, the member's own name, and the index after it
/// and its generic arguments.
fn name_segments(p: &Parser<'_>, at: usize) -> (usize, usize, usize) {
    let input = p.input();
    let mut i = at;
    let mut segments = 0;
    while input.kind(i) == K::Ident {
        segments += 1;
        let mut j = i + 1;
        if let Some(end) = types::scan_generic_args(input, j) {
            j = end;
        }
        if input.kind(j) == K::Dot && input.kind(j + 1) == K::Ident {
            i = j + 1;
        } else {
            return (segments, i, j);
        }
    }
    (segments, i, i)
}

/// `IShape.` before the member name: the first `segments` segments of the
/// dotted name and the dot after them.
fn explicit_impl(p: &mut Parser<'_>, segments: usize) {
    let m = p.start();
    let path = p.start();
    for i in 0..segments {
        let segment = p.start();
        name_ref(p);
        if p.at(K::Lt) {
            types::generic_arg_list(p);
        }
        segment.complete(p, K::PathSegment);
        if i + 1 < segments {
            p.expect(K::Dot);
        }
    }
    path.complete(p, K::Path);
    p.expect(K::Dot);
    m.complete(p, K::ExplicitImpl);
}

pub(crate) fn variable_declarators(p: &mut Parser<'_>) {
    loop {
        let d = p.start();
        if p.at(K::Dollar) {
            exprs::splice(p);
        } else {
            name(p);
        }
        if p.eat(K::Eq) {
            exprs::expr(p);
        }
        d.complete(p, K::VariableDeclarator);
        if !p.eat(K::Comma) {
            break;
        }
    }
}

fn accessor_list(p: &mut Parser<'_>) {
    let m = p.start();
    p.bump(K::LBrace);
    while !p.at(K::RBrace) && !p.at_end() {
        let accessor = p.start();
        while p.at(K::LBracket) {
            attribute(p);
        }
        modifiers(p, false);
        let kind = ["get", "set", "init"]
            .iter()
            .zip([K::GetKw, K::SetKw, K::InitKw])
            .find(|(word, _)| p.at_contextual(word))
            .map(|(_, kind)| kind);
        let Some(kind) = kind else {
            p.error("expected `get`, `set` or `init`");
            accessor.complete(p, K::Error);
            break;
        };
        p.bump_remap(kind);
        body_or_semicolon(p);
        accessor.complete(p, K::Accessor);
    }
    p.expect(K::RBrace);
    m.complete(p, K::AccessorList);
}

/// `public T this[int i] { ... }`; with `explicit` segments, an explicit
/// implementation such as `int IList<int>.this[int i]`.
fn indexer(p: &mut Parser<'_>, m: Marker, explicit: usize) {
    types::ty(p);
    if explicit > 0 {
        explicit_impl(p, explicit);
    }
    p.expect(K::ThisKw);
    p.expect(K::LBracket);
    while !p.at(K::RBracket) && !p.at_end() {
        param(p);
        if !p.eat(K::Comma) {
            break;
        }
    }
    p.expect(K::RBracket);
    if p.at(K::LBrace) {
        accessor_list(p);
    } else {
        expr_body(p);
        p.expect(K::Semicolon);
    }
    m.complete(p, K::IndexerDecl);
}

/// `public Counter(string name) : this(name, 1) { ... }` (§6.9).
fn constructor(p: &mut Parser<'_>, m: Marker) {
    name(p);
    param_list(p);
    if p.at(K::Colon) {
        let init = p.start();
        p.bump(K::Colon);
        p.expect(K::ThisKw);
        exprs::arg_list(p);
        init.complete(p, K::ConstructorInitializer);
    }
    body_or_semicolon(p);
    m.complete(p, K::ConstructorDecl);
}

fn destructor(p: &mut Parser<'_>, m: Marker) {
    p.bump(K::Tilde);
    name(p);
    p.expect(K::LParen);
    p.expect(K::RParen);
    body_or_semicolon(p);
    m.complete(p, K::DestructorDecl);
}

/// The operators a type may declare (§6.9), longest first.
const OVERLOADABLE: &[SyntaxKind] = &[
    K::UShr,
    K::Shr,
    K::GtEq,
    K::PlusPercent,
    K::MinusPercent,
    K::StarPercent,
    K::PlusPlus,
    K::MinusMinus,
    K::EqEq,
    K::BangEq,
    K::LtEq,
    K::Shl,
    K::Plus,
    K::Minus,
    K::Star,
    K::Slash,
    K::Percent,
    K::Amp,
    K::Pipe,
    K::Caret,
    K::Bang,
    K::Tilde,
    K::Lt,
    K::Gt,
];

fn operator(p: &mut Parser<'_>, m: Marker) {
    types::ty(p);
    p.bump(K::OperatorKw);
    match OVERLOADABLE.iter().find(|&&kind| p.at(kind)) {
        Some(&kind) => p.bump(kind),
        None => p.error("expected an operator"),
    }
    param_list(p);
    body_or_semicolon(p);
    m.complete(p, K::OperatorDecl);
}

/// `public static implicit operator AppError(IoError e) => Io(e);`
fn conversion(p: &mut Parser<'_>, m: Marker) {
    p.bump_any();
    p.expect(K::OperatorKw);
    types::ty(p);
    param_list(p);
    body_or_semicolon(p);
    m.complete(p, K::ConversionOperatorDecl);
}

/// `type Enumerator : IEnumerator<T>;`, `type Enumerator = ListEnumerator<T>;` (§8.7).
fn assoc_type(p: &mut Parser<'_>, m: Marker) {
    p.bump_remap(K::TypeKw);
    name(p);
    if p.at(K::Colon) {
        base_list(p);
    }
    if p.eat(K::Eq) {
        types::ty(p);
    }
    p.expect(K::Semicolon);
    m.complete(p, K::AssocTypeDecl);
}

pub(crate) fn param_list(p: &mut Parser<'_>) {
    let m = p.start();
    if p.expect(K::LParen) {
        while !p.at(K::RParen) && !p.at_end() && !p.at_any(&[K::LBrace, K::Semicolon, K::FatArrow])
        {
            param(p);
            if !p.eat(K::Comma) {
                break;
            }
        }
        p.expect(K::RParen);
    }
    m.complete(p, K::ParamList);
}

fn param(p: &mut Parser<'_>) {
    let m = p.start();
    while p.at(K::LBracket) {
        attribute(p);
    }
    modifiers(p, true);
    types::ty(p);
    name(p);
    if p.eat(K::Eq) {
        exprs::expr(p);
    }
    m.complete(p, K::Param);
}

fn type_param_list(p: &mut Parser<'_>) {
    let m = p.start();
    p.bump(K::Lt);
    while !p.at(K::Gt) && !p.at_end() {
        let param = p.start();
        while p.at(K::LBracket) {
            attribute(p);
        }
        if p.at(K::InKw) || p.at(K::OutKw) {
            p.bump_any();
        }
        name(p);
        param.complete(p, K::TypeParam);
        if !p.eat(K::Comma) {
            break;
        }
    }
    p.expect(K::Gt);
    m.complete(p, K::TypeParamList);
}

fn base_list(p: &mut Parser<'_>) {
    let m = p.start();
    p.bump(K::Colon);
    types::ty(p);
    while p.eat(K::Comma) {
        types::ty(p);
    }
    m.complete(p, K::BaseList);
}

/// `where T : IComparable<T>, new()` (§8.2).
fn where_clause(p: &mut Parser<'_>) {
    let m = p.start();
    p.bump(K::WhereKw);
    name_ref(p);
    p.expect(K::Colon);
    loop {
        let c = p.start();
        if p.at_any(&[K::StructKw, K::ClassKw, K::UniqueKw, K::MoveKw]) && !p.nth_at(1, K::Lt) {
            p.bump_any();
            c.complete(p, K::KindConstraint);
        } else if p.at(K::NewKw) {
            p.bump(K::NewKw);
            p.expect(K::LParen);
            p.expect(K::RParen);
            c.complete(p, K::NewConstraint);
        } else if p.at(K::RefKw) || (p.at_contextual("allows") && p.nth(1) == K::RefKw) {
            if p.at_contextual("allows") {
                p.bump_remap(K::AllowsKw);
            }
            p.bump(K::RefKw);
            p.expect(K::StructKw);
            c.complete(p, K::RefStructConstraint);
        } else {
            types::ty(p);
            c.complete(p, K::TypeConstraint);
        }
        if !p.eat(K::Comma) {
            break;
        }
    }
    m.complete(p, K::WhereClause);
}

/// A body: a block, `=> expr;`, or `;` for a declaration without one.
fn body_or_semicolon(p: &mut Parser<'_>) {
    if p.at(K::LBrace) {
        stmts::block(p);
    } else if p.at(K::FatArrow) {
        expr_body(p);
        p.expect(K::Semicolon);
    } else {
        p.expect(K::Semicolon);
    }
}

fn expr_body(p: &mut Parser<'_>) {
    let m = p.start();
    p.expect(K::FatArrow);
    exprs::expr(p);
    m.complete(p, K::ExprBody);
}

/// A local function in a body: modifiers such as `static` or `async`, a
/// return type, a name and the rest of a function (§7.11).
pub(crate) fn local_function(p: &mut Parser<'_>) {
    let m = p.start();
    modifiers(p, false);
    member_after_type(p, m, Context::Member, false);
}
