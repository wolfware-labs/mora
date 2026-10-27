//! Typed views of the syntax tree.
//!
//! Each node of `mora.ungram` is a type here that wraps a [`SyntaxNode`] of
//! its kind and has a method per child. The types and methods are generated
//! by `cargo xtask codegen`; this module holds what they share. A view never
//! fails on a broken tree: a missing child is `None`, so the LSP can work on
//! code that is being typed.

use std::marker::PhantomData;

use crate::{SyntaxElement, SyntaxKind, SyntaxNode, SyntaxNodeChildren, SyntaxToken};

pub use crate::generated::nodes::*;

/// A typed view of a node.
pub trait AstNode: Sized {
    /// Whether a node of this kind can be viewed as `Self`.
    fn can_cast(kind: SyntaxKind) -> bool;

    /// The view of `node`, if its kind fits.
    fn cast(node: SyntaxNode) -> Option<Self>;

    fn syntax(&self) -> &SyntaxNode;
}

/// The children of a node that can be viewed as `N`.
#[derive(Debug, Clone)]
pub struct AstChildren<N> {
    inner: SyntaxNodeChildren,
    _node: PhantomData<N>,
}

impl<N: AstNode> Iterator for AstChildren<N> {
    type Item = N;

    fn next(&mut self) -> Option<N> {
        self.inner.find_map(N::cast)
    }
}

/// Helpers for the generated accessors.
pub(crate) mod support {
    use super::*;

    pub(crate) fn child<N: AstNode>(parent: &SyntaxNode) -> Option<N> {
        parent.children().find_map(N::cast)
    }

    /// The `n`th child that can be viewed as `N`, counting from zero: the
    /// right operand of a binary expression is the second `Expr`.
    #[allow(
        dead_code,
        reason = "generated only when no token separates two children of one type"
    )]
    pub(crate) fn nth_child<N: AstNode>(parent: &SyntaxNode, n: usize) -> Option<N> {
        parent.children().filter_map(N::cast).nth(n)
    }

    /// The first child viewable as `N` that comes after a token of one of
    /// `after` (anywhere, when `after` is empty) and before any token of
    /// `before`: the `end` of `start..end` is the `Expr` after `..`.
    pub(crate) fn child_between<N: AstNode>(
        parent: &SyntaxNode,
        after: &[SyntaxKind],
        before: &[SyntaxKind],
    ) -> Option<N> {
        let mut started = after.is_empty();
        for element in parent.children_with_tokens() {
            match element {
                SyntaxElement::Token(token) if !started => started = after.contains(&token.kind()),
                SyntaxElement::Token(token) if before.contains(&token.kind()) => return None,
                SyntaxElement::Node(node) if started => {
                    if let Some(found) = N::cast(node) {
                        return Some(found);
                    }
                }
                _ => {}
            }
        }
        None
    }

    pub(crate) fn children<N: AstNode>(parent: &SyntaxNode) -> AstChildren<N> {
        AstChildren {
            inner: parent.children(),
            _node: PhantomData,
        }
    }

    /// The first direct child token whose kind is one of `kinds`.
    pub(crate) fn token(parent: &SyntaxNode, kinds: &[SyntaxKind]) -> Option<SyntaxToken> {
        parent
            .children_with_tokens()
            .filter_map(|element| element.into_token())
            .find(|token| kinds.contains(&token.kind()))
    }
}
