//! The portable runtime core. `no_std`; reaches the platform only through the `mora_pal_*` interface (§13.1, §19.4).

#![no_std]
// One of the crates allowed to contain `unsafe` code (CONTRIBUTING.md).
#![allow(unsafe_code)]
