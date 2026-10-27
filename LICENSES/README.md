# Third-party licenses

Mora's toolchain bundles the linker and C runtime pieces below (§14.14, §19.2). `cargo xtask sysroot` installs the same pieces for development and copies this directory beside them. Each text is copied unchanged from the pinned archive it covers.

| File | Component | License | Taken from |
|---|---|---|---|
| `LLVM.txt` | `lld`, its `libLLVM` and `libc++`/`libunwind` DLLs on Windows, `libunwind.a`, the compiler-rt builtins | Apache-2.0 WITH LLVM-exception | `LICENSE.TXT` of llvm-mingw 20261006 |
| `mingw-w64-runtime.txt` | MinGW-w64 start files, import libraries and runtime libraries | ZPL-2.1, with BSD, MIT and public-domain parts | `COPYING.MinGW-w64-runtime.txt` of llvm-mingw 20261006 |
| `winpthreads.txt` | `libwinpthread` | MIT | `COPYING.winpthreads.txt` of llvm-mingw 20261006 |
| `winstorecompat.txt` | `libwinstorecompat` | MIT | `COPYING.winstorecompat.txt` of llvm-mingw 20261006 |
| `musl.txt` | musl start files and `libc.a` | MIT | `COPYRIGHT` of musl 1.2.5, the version in Rust 1.99.1's `rust-std` |

MinGW-w64's profiling code (`gcrt*.o`, `libgmon.a`) is under the GPL and is not bundled. The LGPL parts of MinGW-w64 are headers and IDL files imported from Wine, which are not bundled either.
