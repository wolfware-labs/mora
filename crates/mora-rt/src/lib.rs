//! The hosted runtime layer for Linux, macOS and Windows: process startup, I/O, the reactor and backtraces (§13.1).
// One of the crates allowed to contain `unsafe` code (CONTRIBUTING.md).
#![allow(unsafe_code)]
