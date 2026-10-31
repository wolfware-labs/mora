//! Compilation units, declarations and members (§4, §6, §8.2, §9.2).

use super::csharp::{self, OpenNamespace, PrimaryConstructor, TypeFirst};
use super::exprs;
use super::types::{self, name, name_ref, scan_type};
use super::{Input, Marker, Parser, stmts};
use crate::{SyntaxError, SyntaxFix, SyntaxKind};
use mora_diag::codes;
use mora_span::{TextRange, TextSize};

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
enum Context<'t> {
    /// At the top of a file, where statements are allowed too.
    TopLevel,
    /// In the member list of a type, named `owner` (none in `code { }`),
    /// or a local function.
    Member { owner: Option<&'t str> },
}

/// Parses a declaration with its attributes and modifiers. At the top
/// level, returns `false` without taking anything when the next tokens are
/// a statement instead.
fn declaration(p: &mut Parser<'_>, cx: Context<'_>) -> bool {
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
    // `event Action Changed;` is reported as a whole, not for its order too.
    let event = (start..p.pos()).any(|i| p.input().kind(i) == K::EventKw);
    let since = p.reported();
    let member = cx != Context::TopLevel;
    match p.current() {
        K::StructKw => struct_or_class(p, m, K::StructKw, K::StructDecl),
        K::ClassKw => struct_or_class(p, m, K::ClassKw, K::ClassDecl),
        K::InterfaceKw => interface_decl(p, m),
        K::UnionKw => union_decl(p, m),
        K::EnumKw => enum_decl(p, m),
        K::MacroKw => macro_decl(p, m),
        K::Ident if p.at_contextual("record") && record_follows(p) => record_decl(p, m),
        K::Tilde if member => destructor(p, m),
        K::ImplicitKw | K::ExplicitKw => conversion(p, m),
        K::Ident if member && p.at_contextual("type") && p.nth(1) == K::Ident => assoc_type(p, m),
        K::OperatorKw => operator(p, m, None),
        K::ThisKw if member && p.nth(1) == K::LBracket => indexer(p, m, 0, None),
        _ => {
            // `[Cfg(Debug = true)] Log(x);` at the top level (§6.11). There,
            // without modifiers, only a function has a shape.
            let statement = cx == Context::TopLevel && attributed && modifiers.is_empty();
            match shape(p, cx, !modifiers.is_empty()) {
                Some(shape) => member_with_shape(p, m, shape),
                _ if p.pos() == start => {
                    // Nothing taken: the caller reports and recovers.
                    m.abandon(p);
                    return false;
                }
                _ if statement => {
                    stmts::statement(p);
                    m.complete(p, K::AttributedStmt);
                }
                _ => {
                    p.error("expected a declaration after the attributes and modifiers");
                    m.complete(p, K::Error);
                }
            }
        }
    }
    if event {
        p.withdraw(since, codes::MR0135);
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
    // `Add(a: int, b: int): int => a + b;`, or `int Add(int a, int b)`
    // in the C# order.
    at_function(p.input(), p.pos()) || at_type_first_function(p.input(), p.pos())
}

/// Whether a function declared name first starts at `at`: a name, its type
/// parameters and its parameters, then what only a declaration has after
/// them: a body, a return type or a `where` clause. `Log(x);` is a call.
pub(crate) fn at_function(input: &Input<'_>, at: usize) -> bool {
    if input.kind(at) != K::Ident {
        return false;
    }
    let mut i = at + 1;
    if let Some(end) = types::scan_generic_args(input, i) {
        i = end;
    }
    if input.kind(i) != K::LParen {
        return false;
    }
    let mut depth = 0usize;
    loop {
        match input.kind(i) {
            K::LParen => depth += 1,
            K::RParen => {
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
    matches!(
        input.kind(i + 1),
        K::LBrace | K::FatArrow | K::Colon | K::WhereKw
    )
}

/// Whether a function declared in the C# order starts at `at`: a type, a
/// name and `(` or `<`.
pub(crate) fn at_type_first_function(input: &Input<'_>, at: usize) -> bool {
    scan_type(input, at).is_some_and(|end| {
        input.kind(end) == K::Ident && matches!(input.kind(end + 1), K::LParen | K::Lt)
    })
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
                csharp::report_required(p);
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
    let at = p.range(0).start();
    p.bump(keyword);
    struct_or_class_rest(p, m, kind, at);
}

/// The declaration after its keyword, which starts at `at`. A struct may
/// have a parameter list (§6.5); a class may not (§6.9).
fn struct_or_class_rest(p: &mut Parser<'_>, m: Marker, kind: SyntaxKind, at: TextSize) {
    let owner = p.nth_text(0);
    type_head(p);
    let mut primary = None;
    if p.at(K::LParen) {
        if kind == K::ClassDecl {
            primary = Some(PrimaryConstructor::parse(p));
        } else {
            param_list(p);
        }
    }
    type_tail(p);
    if let Some(primary) = primary {
        primary.report(p, owner, at);
    }
    if !p.eat(K::Semicolon) {
        member_list(p, Some(owner));
    }
    m.complete(p, kind);
}

/// `record`, `record class` and `record struct`, reported and taken as the
/// `struct` they stand for (§6.5).
fn record_decl(p: &mut Parser<'_>, m: Marker) {
    let at = p.range(0).start();
    p.bump_remap(K::StructKw);
    if p.at(K::ClassKw) || p.at(K::StructKw) {
        let e = p.start();
        p.bump_any();
        e.complete(p, K::Error);
    }
    let range = TextRange::new(at, p.prev_end());
    p.report(
        SyntaxError::new(
            codes::MR0133,
            range,
            format!(
                "`{}`: Mora writes it as `struct`",
                &p.input().source()[range]
            ),
        )
        .with_note("A record's data has no identity, which is what a struct is.")
        .with_fix(SyntaxFix {
            message: "write `struct`".to_owned(),
            edits: vec![(range, "struct".to_owned())],
        }),
    );
    struct_or_class_rest(p, m, K::StructDecl, at);
}

fn interface_decl(p: &mut Parser<'_>, m: Marker) {
    p.bump(K::InterfaceKw);
    let owner = p.nth_text(0);
    type_head(p);
    type_tail(p);
    member_list(p, Some(owner));
    m.complete(p, K::InterfaceDecl);
}

/// `union Json { Null, Bool(Value: bool), indirect Array(Items: List<Json>); members }`.
fn union_decl(p: &mut Parser<'_>, m: Marker) {
    p.bump(K::UnionKw);
    let owner = p.nth_text(0);
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
            members(p, Some(owner));
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

/// `{ members }` of the type named `owner`, or of a `code { }` quote.
pub(crate) fn member_list(p: &mut Parser<'_>, owner: Option<&str>) {
    let m = p.start();
    if p.expect(K::LBrace) {
        members(p, owner);
        p.expect(K::RBrace);
    }
    m.complete(p, K::MemberList);
}

/// Members up to the closing `}`, recovering at each member boundary.
fn members(p: &mut Parser<'_>, owner: Option<&str>) {
    let cx = Context::Member { owner };
    while !p.at(K::RBrace) && !p.at_end() {
        let before = p.pos();
        // `$..members;` is a splice of members; `$f: int;` declares a field.
        if p.at(K::Dollar) && shape(p, cx, false).is_none() {
            let m = p.start();
            exprs::splice(p);
            p.eat(K::Semicolon);
            m.complete(p, K::SpliceMember);
        } else if !(p.at(K::DelegateKw) && csharp::statement(p)) {
            declaration(p, cx);
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

/// What a member is, decided from the tokens before taking any.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Kind {
    Function,
    Constructor,
    Property,
    Field,
    Indexer,
    Operator,
}

/// A member's kind, the segments of its dotted name (more than one for an
/// explicit implementation, `IShape.Area`), and whether its type comes
/// before the name, as in C#.
#[derive(Clone, Copy)]
struct Shape {
    kind: Kind,
    segments: usize,
    type_first: bool,
}

/// The member that starts here, in either order, or `None`. At the top
/// level a field needs `modifiers` (`const`, `static`): without them a
/// declaration is a local, which is a statement.
fn shape(p: &Parser<'_>, cx: Context<'_>, modifiers: bool) -> Option<Shape> {
    let input = p.input();
    let at = p.pos();
    let member = cx != Context::TopLevel;
    let fields = member || modifiers;
    let shape = |kind, segments, type_first| {
        Some(Shape {
            kind,
            segments,
            type_first,
        })
    };
    // Name first: `Area(): float`, `Count: int`, `IList<int>.this[i: int]`.
    let (segments, name_at, end) = if p.at(K::Dollar) {
        let end = types::scan_splice(input, at)?;
        (1, at, end)
    } else {
        name_segments(p, at)
    };
    if segments > 0 {
        match input.kind(end) {
            K::Dot if input.kind(end + 1) == K::ThisKw && member => {
                return shape(Kind::Indexer, segments, false);
            }
            K::LParen | K::Lt => {
                if let Context::Member { owner: Some(owner) } = cx
                    && segments == 1
                    && input.text(name_at) == owner
                {
                    return shape(Kind::Constructor, 1, false);
                }
                if member || at_function(input, name_at) || p.at(K::Dollar) {
                    return shape(Kind::Function, segments, false);
                }
            }
            K::Colon => {
                let after = scan_type(input, end + 1).map(|i| input.kind(i));
                if member && matches!(after, Some(K::LBrace | K::FatArrow)) {
                    return shape(Kind::Property, segments, false);
                }
                if fields && segments == 1 {
                    return shape(Kind::Field, 1, false);
                }
            }
            K::Eq | K::Semicolon | K::Comma if fields && segments == 1 && !p.at(K::Dollar) => {
                // `Count;`: a field without its type, reported when parsed.
                return shape(Kind::Field, 1, false);
            }
            _ => {}
        }
    }
    // Type first: `int Count;`, `void Tick()`, `T this[int i]`.
    let after_type = scan_type(input, at)?;
    match input.kind(after_type) {
        K::OperatorKw if member => shape(Kind::Operator, 0, true),
        K::ThisKw if member => shape(Kind::Indexer, 0, true),
        K::Dollar => {
            let end = types::scan_splice(input, after_type)?;
            if input.kind(end) == K::LParen {
                shape(Kind::Function, 1, true)
            } else if fields {
                shape(Kind::Field, 1, true)
            } else {
                None
            }
        }
        K::Ident => {
            let (segments, _, end) = name_segments(p, after_type);
            match input.kind(end) {
                K::Dot if input.kind(end + 1) == K::ThisKw && member => {
                    shape(Kind::Indexer, segments, true)
                }
                K::LParen | K::Lt => shape(Kind::Function, segments, true),
                K::LBrace | K::FatArrow if member => shape(Kind::Property, segments, true),
                _ if fields => shape(Kind::Field, 1, true),
                _ => None,
            }
        }
        _ => None,
    }
}

fn member_with_shape(p: &mut Parser<'_>, m: Marker, shape: Shape) {
    if shape.kind == Kind::Field {
        // The type goes into the first declarator, where the name-first
        // form has it.
        variable_declarators(p, Declarators::Field);
        p.expect(K::Semicolon);
        m.complete(p, K::FieldDecl);
        return;
    }
    let first = shape.type_first.then(|| TypeFirst::parse(p));
    match shape.kind {
        Kind::Function => function(p, m, shape.segments, first),
        Kind::Constructor => constructor(p, m),
        Kind::Property => property(p, m, shape.segments, first),
        Kind::Indexer => indexer(p, m, shape.segments, first),
        _ => operator(p, m, first),
    }
}

/// The name of a function or a property: a dotted explicit implementation
/// `IShape.Area` when there are several `segments`, or a splice.
fn member_name(p: &mut Parser<'_>, segments: usize) {
    if segments > 1 {
        explicit_impl(p, segments - 1);
    }
    if p.at(K::Dollar) {
        exprs::splice(p);
    } else {
        name(p);
    }
}

/// `Deposit(amount: decimal): decimal { ... }`, `Max<T>(a: T, b: T): T
/// where T : IComparable<T>`, `Tick() { }` (§6.1).
fn function(p: &mut Parser<'_>, m: Marker, segments: usize, first: Option<TypeFirst>) {
    member_name(p, segments);
    let name_end = p.prev_end();
    if p.at(K::Lt) {
        type_param_list(p);
    }
    param_list(p);
    match first {
        Some(first) => {
            let end = p.prev_end();
            first.report_at(p, name_end, &[end], "", true);
        }
        None if p.at(K::Colon) => types::annotation(p, true),
        None => {}
    }
    while p.at(K::WhereKw) {
        where_clause(p);
    }
    body_or_semicolon(p);
    m.complete(p, K::FunctionDecl);
}

/// `Count: int { get; }`, `IsEmpty: bool => Count == 0;` (§6.9).
fn property(p: &mut Parser<'_>, m: Marker, segments: usize, first: Option<TypeFirst>) {
    member_name(p, segments);
    match first {
        Some(first) => first.report(p, ""),
        None => required_annotation(p),
    }
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
}

/// `: Type` where a declaration must have one.
fn required_annotation(p: &mut Parser<'_>) {
    if p.at(K::Colon) {
        types::annotation(p, false);
    } else {
        p.error("expected `:` and a type");
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

/// Where declarators are, which decides whether each needs a type.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Declarators {
    /// Fields, constants and statics: every declarator has a type.
    Field,
    /// Locals, after `let` or `var` (`keyword`) or, in the C# order,
    /// after neither.
    Local { keyword: bool },
}

/// `Count: int = 0, Peak: int` or, for a local, `x = 1, y: int`. A type
/// written first, `int x = 1, y`, goes into the first declarator and is
/// reported once, with a fix that gives each declarator the type.
pub(crate) fn variable_declarators(p: &mut Parser<'_>, at: Declarators) {
    let mut d = p.start();
    let first = types::at_type_first(p).then(|| TypeFirst::parse(p));
    let mut names = Vec::new();
    let mut initialized = None;
    loop {
        if p.at(K::Dollar) {
            exprs::splice(p);
        } else {
            name(p);
        }
        names.push(p.prev_end());
        if first.is_none() {
            if at == Declarators::Field {
                required_annotation(p);
            } else if p.at(K::Colon) {
                types::annotation(p, false);
            }
        }
        let init = p.eat(K::Eq);
        if init {
            exprs::expr(p);
        }
        initialized.get_or_insert(init);
        d.complete(p, K::VariableDeclarator);
        if !p.eat(K::Comma) {
            break;
        }
        d = p.start();
    }
    if let Some(first) = first {
        // `T x = e;` was a variable, `var x: T = e;`; `T x;` is assigned
        // once later, `let x: T;`.
        let keyword = match (at, initialized) {
            (Declarators::Local { keyword: false }, Some(true)) => "var ",
            (Declarators::Local { keyword: false }, _) => "let ",
            _ => "",
        };
        first.report_at(p, names[0], &names, keyword, false);
    }
}

fn accessor_list(p: &mut Parser<'_>) {
    let m = p.start();
    p.bump(K::LBrace);
    let mut inits = Vec::new();
    let mut get = false;
    while !p.at(K::RBrace) && !p.at_end() {
        let after = p.prev_end();
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
        get |= kind == K::GetKw;
        let keyword = p.range(0);
        p.bump_remap(kind);
        body_or_semicolon(p);
        accessor.complete(p, K::Accessor);
        if kind == K::InitKw {
            inits.push((TextRange::new(after, p.prev_end()), keyword));
        }
    }
    p.expect(K::RBrace);
    m.complete(p, K::AccessorList);
    // Reported once the list is read: with a `get`, the fix removes `init`;
    // alone, `init` becomes `get`.
    for (accessor, keyword) in inits {
        csharp::report_init(p, accessor, keyword, get);
    }
}

/// `public this[i: int]: T { ... }`; with `segments`, an explicit
/// implementation such as `IList<int>.this[i: int]: int`.
fn indexer(p: &mut Parser<'_>, m: Marker, segments: usize, first: Option<TypeFirst>) {
    if segments > 0 {
        explicit_impl(p, segments);
    }
    p.expect(K::ThisKw);
    let name_end = p.prev_end();
    p.expect(K::LBracket);
    while !p.at(K::RBracket) && !p.at_end() {
        param(p);
        if !p.eat(K::Comma) {
            break;
        }
    }
    p.expect(K::RBracket);
    match first {
        Some(first) => {
            let end = p.prev_end();
            first.report_at(p, name_end, &[end], "", false);
        }
        None => required_annotation(p),
    }
    if p.at(K::LBrace) {
        accessor_list(p);
    } else {
        expr_body(p);
        p.expect(K::Semicolon);
    }
    m.complete(p, K::IndexerDecl);
}

/// `public Counter(name: string) : this(name, 1) { ... }` (§6.9).
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

/// `public static operator +(a: Vec, b: Vec): Vec => ...` (§6.9).
fn operator(p: &mut Parser<'_>, m: Marker, first: Option<TypeFirst>) {
    p.bump(K::OperatorKw);
    match OVERLOADABLE.iter().find(|&&kind| p.at(kind)) {
        Some(&kind) => p.bump(kind),
        None => p.error("expected an operator"),
    }
    let name_end = p.prev_end();
    param_list(p);
    match first {
        Some(first) => {
            let end = p.prev_end();
            first.report_at(p, name_end, &[end], "", true);
        }
        None if p.at(K::Colon) => types::annotation(p, true),
        None => {}
    }
    body_or_semicolon(p);
    m.complete(p, K::OperatorDecl);
}

/// `public static implicit operator AppError(e: IoError) => Io(e);`
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

/// `amount: decimal`, `ref p: Point`, `port: int = 80` (§10.3).
fn param(p: &mut Parser<'_>) {
    let m = p.start();
    while p.at(K::LBracket) {
        attribute(p);
    }
    modifiers(p, true);
    if types::at_type_first(p) {
        let first = TypeFirst::parse(p);
        name(p);
        first.report(p, "");
    } else {
        name(p);
        required_annotation(p);
    }
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
/// name and the rest of a function (§7.11).
pub(crate) fn local_function(p: &mut Parser<'_>) {
    let m = p.start();
    modifiers(p, false);
    let type_first = !at_function(p.input(), p.pos());
    let first = type_first.then(|| TypeFirst::parse(p));
    function(p, m, 1, first);
}
