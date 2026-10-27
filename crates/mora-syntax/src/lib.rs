//! Lexer, parser, lossless syntax tree and typed AST views (§3, §14.4).

pub mod lexer;

pub use lexer::{LexError, Lexed, Token, TokenKind, lex};
