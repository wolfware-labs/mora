//! Statements (§7.10, §7.11).

use super::exprs::{self, at_expr_start};
use super::types::{self, scan_type};
use super::{Parser, csharp, items, patterns};
use crate::SyntaxKind;

use SyntaxKind as K;

/// Access modifiers never start a statement, so meeting one where a
/// statement should start means the block's `}` is missing.
const ACCESS: &[SyntaxKind] = &[K::PublicKw, K::PrivateKw, K::InternalKw];

/// `{ statements }`.
pub(crate) fn block(p: &mut Parser<'_>) {
    let m = p.start();
    if !p.expect(K::LBrace) {
        m.complete(p, K::Block);
        return;
    }
    while !p.at(K::RBrace) {
        if p.at_end() || p.at_any(ACCESS) {
            p.error(format!("expected {}", super::describe(K::RBrace)));
            m.complete(p, K::Block);
            return;
        }
        statement(p);
    }
    p.bump(K::RBrace);
    m.complete(p, K::Block);
}

/// One statement. Always takes at least one token.
pub(crate) fn statement(p: &mut Parser<'_>) {
    let before = p.pos();
    statement_inner(p);
    if p.pos() == before {
        let e = p.start();
        p.error("expected a statement");
        p.bump_any();
        e.complete(p, K::Error);
    }
}

fn statement_inner(p: &mut Parser<'_>) {
    if csharp::statement(p) {
        return;
    }
    match p.current() {
        K::LBrace => block(p),
        K::Semicolon => {
            let m = p.start();
            p.bump(K::Semicolon);
            m.complete(p, K::EmptyStmt);
        }
        K::IfKw => if_stmt(p),
        K::WhileKw => while_stmt(p),
        K::DoKw => do_while_stmt(p),
        K::ForKw => for_stmt(p),
        K::ForeachKw => foreach_stmt(p),
        K::BreakKw => keyword_stmt(p, K::BreakKw, K::BreakStmt),
        K::ContinueKw => keyword_stmt(p, K::ContinueKw, K::ContinueStmt),
        K::ReturnKw => return_stmt(p),
        K::SwitchKw if p.nth(1) == K::LParen => switch_stmt(p),
        K::LBracket if items::at_attribute(p) => {
            let m = p.start();
            while p.at(K::LBracket) {
                items::attribute(p);
            }
            statement(p);
            m.complete(p, K::AttributedStmt);
        }
        K::LetKw | K::VarKw if p.nth(1) == K::LParen => deconstruct_stmt(p),
        K::LetKw | K::VarKw => local_decl_stmt(p),
        _ if at_local_function(p) => {
            let m = p.start();
            items::local_function(p);
            m.complete(p, K::LocalFunctionStmt);
        }
        _ if at_typed_local(p) => local_decl_stmt(p),
        _ if at_expr_start(p) => {
            let m = p.start();
            exprs::expr(p);
            p.expect(K::Semicolon);
            m.complete(p, K::ExprStmt);
        }
        _ => {}
    }
}

/// `int Add(int a, int b) => a + b;` or `static T Max<T>(...)` in a body.
fn at_local_function(p: &Parser<'_>) -> bool {
    let input = p.input();
    let mut at = p.pos();
    while matches!(input.kind(at), K::StaticKw | K::AsyncKw | K::ExternKw) {
        at += 1;
    }
    let Some(end) = scan_type(input, at) else {
        return false;
    };
    input.kind(end) == K::Ident && matches!(input.kind(end + 1), K::LParen | K::Lt)
}

/// `int x = 1;`, `List<int> xs;`, `User? u = none, v;`.
fn at_typed_local(p: &Parser<'_>) -> bool {
    let input = p.input();
    scan_type(input, p.pos()).is_some_and(|end| {
        input.kind(end) == K::Ident
            && matches!(input.kind(end + 1), K::Eq | K::Semicolon | K::Comma)
    })
}

/// `let x = 1;`, `var x = 1;`, `let int x = 1;`, `int x = 1;`, `let x;` (§7.10).
pub(crate) fn local_decl_stmt(p: &mut Parser<'_>) {
    let m = p.start();
    local_decl(p);
    p.expect(K::Semicolon);
    m.complete(p, K::LocalDeclStmt);
}

fn local_decl(p: &mut Parser<'_>) {
    let m = p.start();
    if p.at(K::LetKw) || p.at(K::VarKw) {
        p.bump_any();
        if scan_type(p.input(), p.pos()).is_some_and(|end| p.input().kind(end) == K::Ident) {
            types::ty(p);
        }
    } else {
        types::ty(p);
    }
    items::variable_declarators(p);
    m.complete(p, K::LocalDecl);
}

/// `let (id, name) = pair;`.
fn deconstruct_stmt(p: &mut Parser<'_>) {
    let m = p.start();
    p.bump_any();
    patterns::tuple_designation(p);
    p.expect(K::Eq);
    exprs::expr(p);
    p.expect(K::Semicolon);
    m.complete(p, K::DeconstructStmt);
}

fn keyword_stmt(p: &mut Parser<'_>, keyword: SyntaxKind, kind: SyntaxKind) {
    let m = p.start();
    p.bump(keyword);
    p.expect(K::Semicolon);
    m.complete(p, kind);
}

fn return_stmt(p: &mut Parser<'_>) {
    let m = p.start();
    p.bump(K::ReturnKw);
    if !p.at(K::Semicolon) && at_expr_start(p) {
        exprs::expr(p);
    }
    p.expect(K::Semicolon);
    m.complete(p, K::ReturnStmt);
}

/// `( condition )` after `if`, `while` and `switch`.
fn parenthesized(p: &mut Parser<'_>) {
    p.expect(K::LParen);
    p.lambdas(true, exprs::expr);
    p.expect(K::RParen);
}

fn if_stmt(p: &mut Parser<'_>) {
    let m = p.start();
    p.bump(K::IfKw);
    parenthesized(p);
    statement(p);
    if p.at(K::ElseKw) {
        let e = p.start();
        p.bump(K::ElseKw);
        statement(p);
        e.complete(p, K::ElseClause);
    }
    m.complete(p, K::IfStmt);
}

fn while_stmt(p: &mut Parser<'_>) {
    let m = p.start();
    p.bump(K::WhileKw);
    parenthesized(p);
    statement(p);
    m.complete(p, K::WhileStmt);
}

fn do_while_stmt(p: &mut Parser<'_>) {
    let m = p.start();
    p.bump(K::DoKw);
    statement(p);
    p.expect(K::WhileKw);
    parenthesized(p);
    p.expect(K::Semicolon);
    m.complete(p, K::DoWhileStmt);
}

/// `for (init; condition; update) body`.
fn for_stmt(p: &mut Parser<'_>) {
    let m = p.start();
    p.bump(K::ForKw);
    p.expect(K::LParen);
    if !p.at(K::Semicolon) {
        if p.at(K::LetKw) || p.at(K::VarKw) || at_typed_local(p) {
            local_decl(p);
        } else {
            let init = p.start();
            expr_list(p);
            init.complete(p, K::ForInitExprs);
        }
    }
    p.expect(K::Semicolon);
    if !p.at(K::Semicolon) {
        exprs::expr(p);
    }
    p.expect(K::Semicolon);
    if !p.at(K::RParen) {
        let update = p.start();
        expr_list(p);
        update.complete(p, K::ForUpdate);
    }
    p.expect(K::RParen);
    statement(p);
    m.complete(p, K::ForStmt);
}

fn expr_list(p: &mut Parser<'_>) {
    exprs::expr(p);
    while p.eat(K::Comma) {
        exprs::expr(p);
    }
}

/// `foreach (let x in xs)`, `foreach (var (k, v) in d)`,
/// `foreach (ref var x in span)`, `foreach (int x in xs)` (§7.11).
fn foreach_stmt(p: &mut Parser<'_>) {
    let m = p.start();
    p.bump(K::ForeachKw);
    p.expect(K::LParen);
    p.eat(K::RefKw);
    if p.at(K::LetKw) || p.at(K::VarKw) {
        p.bump_any();
    }
    patterns::designation(p, false);
    p.expect(K::InKw);
    exprs::expr(p);
    p.expect(K::RParen);
    statement(p);
    m.complete(p, K::ForeachStmt);
}

/// `switch (x) { case p when g: ... default: ... }` (§7.7).
fn switch_stmt(p: &mut Parser<'_>) {
    let m = p.start();
    p.bump(K::SwitchKw);
    parenthesized(p);
    if p.expect(K::LBrace) {
        while !p.at(K::RBrace) && !p.at_end() {
            if !p.at(K::CaseKw) && !p.at(K::DefaultKw) {
                // Statements before the first label: report them once.
                let e = p.start();
                p.error("expected `case` or `default`");
                while !p.at_end() && !p.at_any(&[K::CaseKw, K::DefaultKw, K::RBrace]) {
                    statement(p);
                }
                e.complete(p, K::Error);
                continue;
            }
            switch_section(p);
        }
        p.expect(K::RBrace);
    }
    m.complete(p, K::SwitchStmt);
}

fn switch_section(p: &mut Parser<'_>) {
    let m = p.start();
    while p.at(K::CaseKw) || (p.at(K::DefaultKw) && p.nth(1) == K::Colon) {
        let label = p.start();
        if p.eat(K::DefaultKw) {
            p.expect(K::Colon);
            label.complete(p, K::DefaultLabel);
        } else {
            p.bump(K::CaseKw);
            p.lambdas(false, patterns::pattern);
            if p.at_contextual("when") {
                patterns::when_clause(p);
            }
            p.expect(K::Colon);
            label.complete(p, K::CaseLabel);
        }
    }
    while !p.at_end()
        && !p.at_any(&[K::CaseKw, K::RBrace])
        && !(p.at(K::DefaultKw) && p.nth(1) == K::Colon)
    {
        statement(p);
    }
    m.complete(p, K::SwitchSection);
}
