//! The parser never panics, keeps every byte of its input, and reports
//! errors inside the text (spec §14.4, §20.1).

#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let Ok(text) = std::str::from_utf8(data) else {
        return;
    };
    let parse = mora_syntax::parse(text);
    assert_eq!(parse.syntax().text(), text, "the tree lost or changed text");
    let len = mora_syntax::SyntaxNode::text_range(&parse.syntax()).end();
    for error in parse.errors() {
        assert!(error.range.end() <= len, "error past the end: {error:?}");
    }
});
