# Fast by default: a performance and memory design study for Mora

> **Status.** This study was written before most of the decisions listed in the [review README](README.md), and its proposals were decided item by item on 2026-10-31. Read it through those decisions: where it disagrees with [the specification](../mora.md) or its decisions register, the specification wins.

Reviewed: `docs/design/mora.md` (v0.1 draft, 2304 lines), closely §1.2 to §1.4, §2.2, §5, §6, §8, §10, §12 to §15, §16, §20.6, §22 to §24. It builds on the two companion studies and does not repeat them: [the inherited-debt study](debt.md) (its #3 "a bare interface is never a box, `any I` boxes" and its #12 "records as values" are taken as given or extended here) and [the future study](future.md) (effect inference #1 and the swappable World #4 are reused as building blocks). Code follows those studies' conventions: arm-style `switch (x) { p => ... }`, fields instead of auto-properties, `any I` for boxes, `none` for the empty option.

State of the implementation, checked in the repository: M1.1 to M1.4 (lexer and parser) are merged; MIR (M1.10), codegen (M1.12), the runtime's reference counting (M2.3) and the ARC passes (M2.12, M2.13) are not started. Nothing proposed here has to undo code. It only changes spec text that has not been built yet.

**The yardstick.** The owner's brief asks for "good performance without much effort": a smart compiler and smart memory management, with nothing to tweak. The spec's anti-goal "Hidden costs" (§1.4) adds a constraint: an optimization may remove a cost silently, but it must never add a cost the reader could not predict, and its worst case must be explainable. So every proposal below says whether it is a **guarantee** (a rule a programmer can rely on and the test suite checks, like Swift's guaranteed copy elision or Rust's moves) or **best-effort** (an optimization that may or may not fire), and what its worst case is.

Claims marked **[mem]** come from memory and were not re-checked during this study. Claims marked **[verify]** need a measurement before the spec relies on them.

---

## Executive summary: the twelve changes that would make "fast by default" true

Ranked by impact divided by cost. **G** marks a guarantee, **B** best-effort.

1. **Ship a size-class allocator (mimalloc v3) in `mora-rt` and free with the static size** (#1, S, M2). **B, predictable.** Linux builds link musl statically (§14.14), and musl's allocator is far slower than modern allocators on allocation-heavy code [verify], so without this change every other optimization starts from a slow baseline.
2. **Guaranteed stack promotion of objects that do not escape** (#2, M, M2). **G.** "A class, record or unique object of at most 1 KiB whose reference never leaves the function except as an argument to a non-escaping parameter never touches the heap." The rule is published, the compiler infers which parameters are non-escaping (C#'s `scoped`), and hover explains every heap allocation.
3. **Precise reference counting from function summaries** (#3, M, M2). **G for the new §10.13 items.** Per-function ownership and mod-ref summaries let retain/release pairs cancel across calls, drop most §10.4 snapshots, and release memory-only values at their last use, as Perceus and Mojo do.
4. **Reuse in place when the count is one** (#4, M, M2/M3). **G at compile time, B at run time.** `user with { Name = n }` on a uniquely held record and `s += t` on an unaliased local string update in place. Repeated `+=` on a local string becomes amortized linear, which removes the reason C# needs `StringBuilder` for simple loops.
5. **Structs are values without identity, and a published size rule picks the representation** (#5, M, M2; settled by D-56). **G.** Structs of at most 64 bytes are stored inline, and larger ones go into an immutable shared box. The §2.2 `User` stops being a heap object. Extends companion #12.
6. **An 8-byte object header** (#6, S-M, M2.3). **G.** After monomorphization every release site knows the concrete type, so the `meta` pointer in the 16-byte header (§13.3) has no job in release builds. Small objects shrink by 20 to 33 percent.
7. **Futures are inline state values, and a spawned task allocates once** (#7, M, M6). **G.** `await F()` allocates nothing. Only a `Spawn`, a conversion to the named `Future<T>` box, or async recursion (boxed automatically and shown in hover) allocates. As written today, D-18 makes every async call a heap allocation.
8. **A deterministic MIR optimizer: inlining, SROA, then ARC** (#8, M-L, M2 to M5). **G for LINQ fusion and bounds checks on canonical loops.** Cranelift does not inline across functions or remove bounds checks, so D-11's "one loop" promise depends on Mora's own MIR passes. They must use fixed budgets so that builds stay reproducible.
9. **16-byte strings with inline storage for up to 15 bytes and one-allocation concatenation** (#9, M, M1/M5). **G.** Most JSON keys, ids, numbers and header values stop allocating. `a + b + c` and `$"..."` allocate exactly once, or not at all when the result is short.
10. **Make cost visible: `mora explain-cost`, a cost lens in the LSP, `[Test(MaxAllocations = n)]`, allocation counts in CI** (#10, S-M, M2 then M7). **G (allocation counts are deterministic).** Nobody has to tune anything, but anyone can ask why a value was allocated on the heap and get the escape chain as the answer.
11. **An insertion-ordered Swiss-table `Dictionary` with keyed fast hashing** (#11, M, M5). **B.** It is a dense entry array plus a SIMD-probed index, which keeps D-29's insertion order. Go 1.24 made the same switch to Swiss tables and Datadog reported up to 70% less map memory.
12. **Niche-filling layout guarantees** (#12, S, M3). **G.** `bool?`, `char?`, enum and union options, and `string?` are the same size as the underlying type, and the spec lists the cases.

Next tier: guaranteed copy elision (#13), a zero-copy HTTP and JSON pipeline with a per-request allocation budget (#14), exact presizing of collections (#15), identical code folding (#16), and SIMD kernels written in the Rust runtime (#17).

**What Mora should not do:** add a tracing or background cycle collector, defer or coalesce decrements (both break G2's "freed at a point the reader can see"), apply struct-of-arrays automatically, compress pointers, intern strings automatically, adopt copy-on-write value collections, or make any guarantee depend on the post-MVP LLVM backend. Optimizations that change allocation behaviour must also run in debug builds, or allocation budgets in tests would mean nothing. Details are in "Coherence".

---

## All proposals

| # | Proposal | Verdict | Confidence | Cost |
|---|---|---|---|---|
| 1 | Bundled size-class allocator (mimalloc v3) with sized free | Adopt for MVP | High | S |
| 2 | Guaranteed stack promotion of non-escaping objects, with inferred `scoped` parameters | Adopt for MVP | High | M |
| 3 | Precise RC: ownership and mod-ref summaries, release at last use, snapshot elision | Adopt for MVP | High | M |
| 4 | Reuse when unique: in-place `with`, union rebuild, string append (Perceus/FBIP subset) | Adopt for MVP (`with`, `+=`) / post-MVP (general reuse) | Medium-high | M |
| 5 | Structs are identity-free values; a published size rule picks inline or boxed | Adopt (settled by D-56) | Medium | M |
| 6 | 8-byte object header: drop the `meta` pointer in release builds | Adopt for MVP | Medium-high | S-M |
| 7 | Futures as inline state values; one pooled allocation per spawned task; auto-box only async recursion | Adopt for MVP | High | M |
| 8 | Deterministic MIR optimizer: inliner, SROA, ARC after inlining; LINQ fusion and bounds-check guarantees | Adopt for MVP | High | M-L |
| 9 | 16-byte strings with 15-byte inline storage and one-allocation concatenation | Adopt for MVP | Medium-high | M |
| 10 | Cost visibility: `explain-cost`, LSP cost lens, allocation budgets in tests, deterministic CI counters | Adopt for MVP | High | S-M |
| 11 | Insertion-ordered Swiss-table `Dictionary` with keyed fast hashing | Adopt for MVP | Medium-high | M |
| 12 | Niche-filling layout guarantees for options and unions | Adopt for MVP | High | S |
| 13 | Guaranteed copy elision and in-place construction | Adopt for MVP | High | S |
| 14 | Zero-copy HTTP and JSON pipeline with pooled buffers and a per-request allocation budget | Adopt for MVP | Medium-high | M |
| 15 | Collection sizing: size hints, exact `ToList`, size-class-aware growth, lazy buffers | Adopt for MVP | High | S |
| 16 | Identical code folding and instance deduplication for monomorphized code | Adopt for MVP (linker ICF) / post-MVP (MIR dedup) | Medium-high | S |
| 17 | SIMD: hot std kernels in the Rust runtime now, `Vector128<T>` later, no auto-vectorization in Cranelift | Adopt for MVP (kernels) / post-MVP (types) | Medium-high | S-M |
| 18 | Closed-world devirtualization of `any I` values and function values | Adopt post-MVP | Medium-high | S |
| 19 | Small plain values stored inline in `any I` values, with no box | Explore | Medium | S-M |
| 20 | Substring sharing with a bounded-waste rule | Explore | Medium-low | M |
| 21 | Inferred task-local allocation regions, backed by §10.12 counted arenas | Explore | Medium-low | L |
| 22 | Mora-internal calling convention that returns `Result` and options in registers | Explore | Medium | M |
| 23 | Reference-count atomicity when threads arrive (OQ-7): mark on send, never atomic before | Explore (M10) | Medium | M |
| 24 | Profile-guided optimization from tests and benchmarks | Explore | Medium-low | M |
| 25 | LLVM release backend as an extra, never a dependency of a guarantee | Adopt post-MVP (M13, as planned) | High | L |
| 26 | Static `[NoAlloc]` and checked `scoped` contracts through effect inference | Adopt post-MVP | Medium | S-M |
| 27 | Deferred or coalesced decrements, and background cycle collection | Reject | High | n/a |
| 28 | Automatic struct-of-arrays transformation | Reject (automatic) / Explore (`[Soa]` macro) | High | n/a |
| 29 | Pointer compression and bit-packing of `bool` fields | Reject | High | n/a |
| 30 | Automatic string interning | Reject | Medium-high | n/a |
| 31 | Copy-on-write value-semantic collections | Reject | Medium-high | n/a |

---

## Detailed proposals

### 1. Bundled size-class allocator (mimalloc v3) with sized free

**Problem.** §13.1 says the runtime allocates "through the platform allocator", and §14.14 links Linux executables statically against musl. musl's allocator (mallocng) is designed for small size and hardening, not throughput. Rust and Python projects that build on Alpine regularly report allocation-heavy code running several times slower than with glibc, and the usual fix is to swap in mimalloc or jemalloc [mem, verify]. Mora allocates more than Rust does (every class object, string, array and escaping closure), so it is more exposed to this. Allocator behaviour also differs between platforms (musl, libSystem, the Windows heap), which works against G7's "same program, same memory use".

**Design.**
- `mora-rt` links mimalloc v3 statically on every hosted target. It is the only allocator Mora code uses, and the C runtime's `malloc` is left to C code. mimalloc keeps a heap per thread, shards free lists per 64 KiB page, and keeps separate lists for frees from the local thread and from other threads (MSR-TR-2019-18). That fits the single-threaded MVP and the threads of M10.
- **Sized free.** After monomorphization the compiler knows the size of every object it frees, so it calls `mora_rt_free_sized(ptr, size)`. Strings and arrays compute their size from the length.
- **Size-class-aware capacities.** `mora_rt_good_size(n)` (mimalloc's `mi_good_size`) gives growable collections and strings their real usable capacity for free (#4, #15).
- `mora-rt-core` stays `no_std` (D-50). The allocator is part of the platform interface of §19.4, so a bare-metal target can plug in its own (a TLSF or bump allocator). mimalloc belongs to the hosted platform layer.
- The debug runtime wraps the same allocator with poisoning and the live list (§10.11).

**Guarantee or best-effort.** Best-effort for speed. The behaviour is predictable, and it is the same on Linux, macOS and Windows, which is something the platform allocators cannot promise.

**Prior art.** Rust projects that set `#[global_allocator] = mimalloc`; Bun and Zig ship custom allocators; .NET and Go ship their own allocators inside the runtime. Mora's version is the default, with nothing to opt into.

**Impact.** Typically 10 to 30 percent on allocation-heavy servers compared with glibc, and much more compared with musl [mem, verify with the §20.6 acceptance benchmark]. About 100 to 200 KiB of binary size [verify against the 1.5 MiB hello-world budget]. No change for developers. **Compile time:** none. **Implementation:** S, since it is a C library built by `cc` in the runtime's build script (CI already has a C compiler, users do not need one). **Milestone:** M2.3, alongside the reference counts. **Risks:** a C dependency inside a Rust runtime (`cargo deny` and licenses: mimalloc is MIT); Windows arm64 support [verify]; the embedded story stays separate.

**Verdict:** Adopt for MVP. **Confidence:** High.

---

### 2. Guaranteed stack promotion of non-escaping objects, with inferred `scoped` parameters

**Problem.** Every `new` of a class in C#, Java and Swift starts as a heap allocation. Escape analysis exists, but as a best-effort optimization the programmer cannot see or rely on. HotSpot's scalar replacement fails as soon as an object crosses a call that is not inlined. Go's `-gcflags=-m` shows decisions that change from release to release. .NET 10 only recently started stack-allocating small arrays and delegates, and only after inlining (Microsoft, "What's new in .NET 10 runtime"). Swift's StackPromotion pass works only within the function after inlining [mem]. The result in all four languages is the same folklore: "avoid `new` in hot paths", or "make it a struct", which is exactly the tweaking the brief wants gone.

Mora is in an unusually good position. Generics are monomorphized, types are sealed (no inheritance, D-30), there is no reflection, the standard library is compiled with the program (§14.15), and §14.8 already runs an escape check. The compiler sees the whole program, so escape facts can be computed across functions without the inlining crutch.

**Design.**

*Escape summaries.* For every monomorphized function, a salsa query computes, for each parameter of a counted type (shared class, record box, array, string) and for `this`, whether the reference it holds can **escape**. A parameter escapes if, in any path, its reference value is:
- stored into a field, array element, collection, static, closure environment of an escaping closure, or the state of a spawned future;
- returned (directly, or inside a returned aggregate);
- converted to `any I`, or captured by `[weak x]`;
- passed to a parameter of another function that escapes, or to a call through `any I` or a function value (treated as escaping, unless #18 resolves the target).

Recursion is handled by a fixed point over strongly connected components of the call graph, starting from "does not escape" (the optimistic, standard approach). Hover shows the inferred fact as `scoped` on the parameter, with the C# 11 spelling that C# developers already know for "does not escape the call".

*Promotion rule (the guarantee).* An allocation `new C(...)` of a shared class, record (when boxed, #5), unique class, or an array with a constant length, is placed in the frame of the creating function (or in the state of an async function, see #7) when:
1. its size is at most 1 KiB, and the function's promoted objects total at most 16 KiB;
2. its reference is used only in locals of the function, as an `in` argument to a `scoped` parameter, as the receiver of a method whose `this` is `scoped`, or captured by a non-escaping closure (§10.9);
3. if it is created inside a loop body, it does not outlive the iteration.

A **unique** object that is never moved out of its local qualifies under the same rule. Since it has no header (§13.3), promotion is a plain scalar replacement.

```mora
public class Matrix3 { public mut float[] Cells = new float[9]; /* ... */ }

static float Determinant(Point a, Point b, Point c)
{
    let m = new Matrix3();            // hover: "stack (frame): does not escape"
    m.Fill(a, b, c);                  // Fill's `this` is inferred scoped
    return m.Det();                   // no allocation, no retain, no release
}

public User Add(NewUser input)
{
    let user = new User(_nextId, input.Name, input.Email);
    _users[user.Id] = user;           // hover on `new`: "heap: stored into `_users` (line 14)"
    return user;
}
```

*Representation.* A promoted shared object keeps the normal header (#6), so code shared with heap objects (any callee) works unchanged: it may retain and release it, but by the rule its count returns to one. At the end of the object's scope the compiler emits the drop (destructor, then fields) and no free. The destructor runs at the same point it would have run on the heap, since the last reference dies there, so G2's determinism holds. The debug runtime asserts `strong == 1 && weak == 0` before that drop, and aborts with `internal error: stack object escaped (type T)` otherwise, so a bug in the analysis cannot become a silent use-after-free in tests.

*Lists get a stack buffer (best-effort extension).* A `List<T>` object that is promoted also gets its first buffer, up to 256 bytes, in the frame. It moves to the heap when it grows past that, as Go 1.25 and 1.26 do for slice backing stores (go.dev/blog/allocation-optimizations). This part is best-effort because it depends on run-time sizes.

*Stability.* Because the analysis looks into callees, editing a callee can change a caller's allocation. That is fine as long as it is visible: `mora explain-cost --diff` (#10) and, for post-MVP libraries, the semantic diff of companion #9 report "parameter `p` of `F` now escapes". A programmer who wants the fact checked can write `scoped` on the parameter: it then becomes a contract, and storing it is an error (#26).

**Guarantee or best-effort.** The promotion rule is a **guarantee** for the cases it lists, checked by UI tests and allocation-count tests in both debug and release builds. The list buffer is best-effort. **Worst case:** an object that does not meet the rule costs exactly what it costs today, one allocation, and hover names the reason and the line.

**Prior art and how Mora differs.** Go escape analysis (best-effort, compiler flag to inspect); HotSpot C2 scalar replacement (best-effort, depends on inlining); .NET 10 object stack allocation (best-effort, depends on inlining); Swift StackPromotion [mem]; OCaml's locality modes (Lorenzen et al., ICFP 2024), where `local` values are inferred and live on a stack. Mora takes OCaml's idea that locality is an inferred property of parameters that can be written down, and adds Go's practice of showing the decision. Unlike every one of them, Mora publishes a rule and does not depend on inlining.

**Impact.** In typical application code most short-lived objects are temporaries: builders, iterators, matrices, small DTOs passed down a call chain. Removing them saves the allocation (roughly 10 to 20 ns with mimalloc), the free, all reference-count traffic, and a cache miss [mem]. The DevX gain is larger than the speed gain: "should this be a struct?" stops being a performance question. **Compile time:** one more summary query per function, linear in MIR size, with an SCC fixed point; small. **Implementation:** M. **Milestone:** M2 (after M2.12, as part of M2.13), with `scoped` hover in M7. **Risks:** soundness depends on the analysis being conservative on every escape route: `any I`, closures, async state, `this` in constructors, destructors that resurrect (already a run-time abort per D-46), and the std's `unsafe` dialect, whose functions need hand-written summaries. Mitigations are the debug assert, a fuzz target that generates escape patterns, and the §20.3 suite.

**Verdict:** Adopt for MVP. **Confidence:** High.

---

### 3. Precise RC: ownership and mod-ref summaries, release at last use, snapshot elision

**Problem.** Reference counting costs come from operations that do nothing useful: a retain followed by a release that the optimizer could not pair, a snapshot copy made just in case, an object kept alive to the end of its scope. In Swift, retain/release traffic is a well-known source of slowdowns, and OSSA is still being rolled out across module boundaries in 2025 (Swift forums, "Swift Language focus areas heading into 2025"). §14.12 already plans pair cancellation, but only when "the callee is known not to release (its MIR is visible and summarized)", and it does not specify the summaries. §10.4 adds snapshots (a retain plus a release) for every `in` borrow of a `mut` field of non-plain type. §10.8 says memory-only values "MAY" be released at their last use.

**Design.** Three pieces, all built on salsa queries over monomorphized MIR.

1. **Ownership summary** per function and parameter: `Borrowed` (never retained beyond the call), `Retains` (may store a copy) or `Consumes` (`move`). Pair cancellation (§14.12 item 2) uses it across calls. This is Lean 4's borrow inference (Ullrich and de Moura, "Counting Immutable Beans", IFL 2019) [mem] applied to Mora's existing `in`/`move` modes. Mora needs less inference than Lean because modes are explicit; the summary covers the cases in between.
2. **Mod-ref summary by type and field**: "may write `C.f`", "may release an object of type `T`" (transitively, through drops). With it:
   - a §10.4 **snapshot is dropped** when the callee cannot write the borrowed field, which covers the common case `Log(this._name)`;
   - a **root keep-alive retain** is dropped when the callee cannot release an object of the root's type;
   - a retain/release pair around a call cancels when the call cannot release that type.
   Mod-ref by type is coarse but sound, cheap, and easy to explain ("`Save` might write `Box.Value`").
3. **Release at last use, guaranteed.** Change §10.8's "MAY" to "MUST" for values whose drop runs no user code, in both build profiles: the release happens right after the last use, including the last use of any view or hidden owner that depends on it (§10.6). Destructor-bearing values keep scope-based drops, which are observable. This is what Perceus calls garbage-free (Reinking et al., PLDI 2021), and Mojo's "ASAP destruction" (Mojo manual, "Death of a value"). It also makes #4 possible: a count can only be one at the point of a `with` if the old reference has already been released.

**New guarantees for §10.13** (each with a MIR snapshot test):
- 8. Passing a `mut` field of a shared object as an `in` argument to a function that cannot write that field makes no snapshot.
- 9. A retain/release pair around a call to a function that cannot release objects of that type is removed.
- 10. A memory-only value is released immediately after its last use, also in debug builds.

**Guarantee or best-effort.** Guaranteed for the listed shapes. Elsewhere it is best-effort, and #10 shows what remains. **Worst case:** the current spec's cost. Nothing gets worse.

**Prior art.** Swift OSSA; Lean 4 borrow inference; Perceus precise RC; Lobster's compile-time RC, which its author reports removes about 95 percent of count operations (strlen.com/lobster, the project's own figure). Mora differs by being able to use whole-program summaries without the separate-compilation limits that Swift has.

**Impact.** Fewer RC operations on every path, plus lower peak memory from releasing at last use. Expect most remaining traffic to be in code that stores references, which is where it belongs. **Compile time:** two more summary queries; they are incremental. **Implementation:** M. **Milestone:** M2.12 and M2.13. **Risks:** a summary that is wrong is unsound (a missed release of type `T` leaves a dangling borrow). The std's `unsafe` functions need hand-written summaries, checked by debug runtime tests. Release at last use changes the order in which *memory* is freed, never the order of destructors, so §10.8 stays observable-equivalent.

**Verdict:** Adopt for MVP. **Confidence:** High.

---

### 4. Reuse when unique: in-place `with`, union rebuild, string append (Perceus/FBIP subset)

**Problem.** Immutable updates are idiomatic and clear, and they allocate. `user with { Name = "Ada" }` copies the object in C#. `s += x` in a loop is quadratic in C#, Java and Go, which is why every C# developer learns `StringBuilder`. Persistent trees (`indirect union`) rebuild their path on every insert. Koka, Lean and Roc showed that when the count is one at run time, the old cell can be reused in place, and that a compiler can pair the dropped cell with the new allocation statically ("functional but in-place").

**Design.**
- **`with` on records.** Lowering `x with { F = v }` when `x` is at its last use (#3) produces `if (strong(x) == 1 && weak(x) == 0) { release old F; x.F = v; result = x } else { copy, then set }`. For inline records (#5) there is nothing to reuse, because a `with` is already a struct copy that SROA removes. Reuse matters for boxed (large) records.
- **Union rebuild.** In `switch`, an `indirect` cell matched at its last use and a new cell of the same size built in the same arm are paired at compile time (drop-guided reuse, Lorenzen and Leijen, ICFP 2022). At run time, if the cell is unique it is reused. This is what makes a balanced-tree insert run in place:

  ```mora
  public indirect union Tree { Leaf, Node(Tree Left, int Key, Tree Right) }

  static Tree Insert(Tree t, int k) => switch (t)
  {
      Leaf => Node(Leaf, k, Leaf),
      Node(let l, let key, let r) when k < key => Node(Insert(l, k), key, r),   // reuses t's cell when unique
      Node(let l, let key, let r) => Node(l, key, Insert(r, k)),
  };
  ```
- **String append.** `s += t` and `s = s + t` where `s` is a local at its last use lower to `String.AppendInPlace(move s, t)`. If the string is heap-allocated and unique and its allocation (size class, #1) has room, the bytes are written in place and the length updated. Otherwise it reallocates with 1.5x growth, so the slack is invisible to the program. Strings stay immutable for every observer, because no one else can see a unique string. **Guarantee:** a loop of `+=` on an unaliased local string runs in amortized linear time.
- **Reuse requires `weak == 0`.** A weak reference can observe identity: reusing the cell would make a weak load return the new value instead of `none`. Records (#5) and union cells have no weak references, so this only matters for classes.

General reuse for any class allocation of the same size class (Perceus "reuse tokens" across arbitrary code) is post-MVP, after measurements. A Koka-style `fip` check (`[InPlace]`, an error when a function would allocate on the unique path; Lorenzen, Leijen and Swierstra, "FP²", ICFP 2023) goes with #26.

**Guarantee or best-effort.** The compile-time pairing is deterministic and visible in `explain-cost` ("reuses `t` if unique"). Whether reuse happens depends on the count at run time, so it is best-effort, with the amortized linear time of string appends as the one guaranteed bound. **Worst case:** one compare and branch more than the copy that happens today.

**Prior art.** Perceus (PLDI 2021), frame-limited reuse (ICFP 2022), FP² (ICFP 2023), Lean 4's destructive updates on unique arrays [mem], Roc's in-place updates [mem], Swift's `isKnownUniquelyReferenced` used by hand in CoW types. Mora differs in limiting MVP reuse to three idioms that C# developers write anyway, rather than making it a whole programming style.

**Impact.** Large on update-heavy code. Perceus reports Koka competitive with OCaml and Haskell, and often ahead in time and peak working set, on allocation-heavy benchmarks (MSR-TR-2020-42). String `+=` goes from quadratic to linear. **Compile time:** small. **Implementation:** M. **Milestone:** M2.13 (`with`, strings), M3 (unions). This moves part of §14.12's "Perceus post-MVP" into the MVP. **Risks:** the in-place path must release the old field values exactly once; this is covered by the drop-order suite (§20.3) and the debug runtime's poisoning.

**Verdict:** Adopt for MVP (`with`, `+=`, unions) / post-MVP (general reuse). **Confidence:** Medium-high.

---

### 5. Structs are identity-free values; a published size rule picks inline or boxed

**Settled by D-56.** This was written when Mora still had `record`; read “record” as `struct` below. D-56 made `struct` the identity-free value type and left the layout to the compiler, which is this proposal. OQ-21 holds the storage rule.

**Problem.** In the acceptance program (§2.2), `record User(int Id, string Name, string? Email)` is a shared class (§6.5). Each user is a heap allocation with a header and a count, `Dictionary<int, User>` stores pointers to them, and every read retains. C# programmers use records for DTOs because they are convenient, not because they want identity. Companion study #12 suggested making records value types by default. On performance grounds this study agrees, and proposes a version that keeps the benefits of both representations.

**Design.**
- **Semantics:** a record has **no identity**. Its fields are read-only (positional parameters, and any extra fields declared in the body, which may not be `mut`). `weak` of a record, `ReferenceEquals` on records, and `mut` fields in a record are errors with fixes ("make it a `class`"). Equality, hashing, `with` and deconstruction stay as they are.
- **Representation, by a published rule:** a record that is not self-recursive and whose inline size is at most **64 bytes** (one cache line) is stored inline, as a `record struct` would be. A larger or recursive record lives in an immutable shared box, as Swift's `indirect` does, and copying it is a retain. The programmer can override the rule with `inline record` or `boxed record`. Hover states "record `User`: inline value, 40 bytes" (with the 16-byte strings of #9: 4 + 4 padding + 16 + 16).
- Because the record has no identity, the choice cannot change what a program does. It only changes cost, and the cost is the one the rule announces.
- Superseded by D-56: there is no `record struct` or `record class`; `struct` is the only spelling, and `record` reports MR0133 with a fix.

**Guarantee or best-effort.** **Guaranteed** representation per the rule. **Worst case:** copying an inline record copies up to 64 bytes and retains each counted field (at most a handful). A boxed one costs a retain. Both are listed by #10.

**Prior art.** Swift structs and `indirect` enums; Hylo's mutable value semantics [mem]; C# `record struct`; companion #12. The difference is that Mora decides the representation by a fixed, documented rule instead of making the programmer choose between `record` and `record struct` for performance.

**Impact.** In §2.2, `Dictionary<int, User>` stores users inline in its entry array (#11), `Find` returns an inline `User?` (niche, #12), and JSON (de)serialization of a user allocates nothing for the record itself. For DTO-heavy web code this removes most object allocations. **Compile time:** none. **Implementation:** M (type checker rules and layout). **Milestone:** M2.2 (records). This must be decided before v0.1 because it changes semantics. **Risks:** C# developers who compare records by reference (rare, and now an error with a fix); a record growing past 64 bytes changes its cost, which `explain-cost --diff` reports.

**Verdict:** Adopt for MVP. **Confidence:** Medium. This is the largest semantic change in this study, and it should be decided together with companion #12.

---

### 6. 8-byte object header: drop the `meta` pointer in release builds

**Problem.** §13.3 gives every shared object a 16-byte header: `strong: u32`, `weak: u32`, `meta: *const TypeMeta`. Small objects dominate in application code. A boxed record of 24 bytes takes 40 bytes, which mimalloc rounds up to 48. Swift's header is also 16 bytes, and Java's object header is 12 to 16 bytes.

**Why `meta` is not needed.** After monomorphization, every retain, release and drop site in Mora knows the concrete type, except for dynamic interface values, whose vtable already carries the drop function (§13.6). `is C c` compares the type id in the vtable, not in the object. Strings and arrays are known statically. The remaining use is the release work list, which can push `(object, drop function)` pairs instead of bare objects.

**Design.** The release header is `{ strong: u32, weak: u32 }`, 8 bytes, which keeps 8-byte alignment of the payload. Each concrete type gets a compiler-generated `T_drop` function, called directly (so it can be inlined, which a call through `meta` could not be). The debug runtime keeps `meta` for the leak report and the cycle finder (§10.11) in a prefix word *before* the header, so field offsets stay the same in both profiles and layout is still computed once (§14.13).

**Guarantee or best-effort.** Guaranteed layout. **Worst case:** none. **Impact:** 8 bytes less per object: 20 to 33 percent for objects of 16 to 32 bytes, with fewer cache misses as a result. **Compile time:** none. **Implementation:** S-M (runtime and codegen; the work list format). **Milestone:** M2.3, which has not started. **Risks:** any future feature that needs the type of an unknown object (post-MVP inheritance, OQ-4, with virtual destructors) must carry it in a vtable, which is how C++ does it anyway.

**Prior art.** Rust's `Arc` (two counters, no type pointer, drop known statically); Swift keeps an isa pointer because of the Objective-C runtime and dynamic casts, which Mora does not have.

**Verdict:** Adopt for MVP. **Confidence:** Medium-high.

---

### 7. Futures as inline state values; one pooled allocation per spawned task; auto-box only async recursion

**Problem.** D-18 and §6.4 make `Future<T>` a **unique class**, which §13.3 defines as a heap object. Read literally, every call of an async function allocates, and an HTTP handler with five awaits allocates five times per request. C# has the same problem, solved by `ValueTask`, pooling and `AsyncMethodBuilder` overrides that have to be added by hand. Rust avoids it with inline futures, but users meet `Box::pin` for recursion and `Pin` in general, and futures can grow to tens of kilobytes and get copied around [mem]. §12.3 already says "await on a child future stores it in the parent's state and polls it in place", which contradicts a heap-allocated unique class.

**Design.**
- Each async function returns an **anonymous move-only state type** that implements `IFuture<T>`, as each lambda already gets an anonymous closure struct (§10.9). `let f = GetA();` gives `f` that type, since locals take the type of their initializer (§14.5).
- `await e` on such a value embeds the child state **inline** in the parent state and polls it in place. Calling and awaiting allocates nothing (**guarantee**).
- `Future<T>` written in source is the **boxed** form: a unique heap object holding any `IFuture<T>`, one allocation, shown by #10. It is used for heterogeneous storage (`List<Future<T>>`, the route table of `WebApp`). This is the "bare interface is never a box" rule of companion #3 applied to futures: the cost is in the type the programmer wrote.
- `Task.WhenAll` and `WhenAny` take generic `TF : IFuture<T>` parameters, so they stay inline.
- **Async recursion.** An async function that awaits a call in its own SCC of the async call graph would have infinite size. Rust reports an error; Mora boxes automatically at that call edge, one allocation per recursive call, with an inlay hint ("recursive await: one allocation per call"). The behaviour is explainable and the code stays as written.
- **Layout:** fields of the state that are live at disjoint suspension points share storage (Rust's generator layout optimization [mem]). This is best-effort; #10 reports state sizes.
- **Spawn:** `Task.Spawn(f)` allocates one block, sized exactly for the state, and constructs `f` directly in it (#13), so large futures are not copied. The executor keeps per-size-class free lists of task blocks; wakers are task indices (non-atomic in the MVP), not counted objects.
- **Stack promotion inside futures:** under #2, "the frame" of an async function is its state. An object that does not escape the async function lives inside the state, which lives inside the task's single block.

**Guarantee or best-effort.** Guaranteed: zero allocations for `await` on a direct call; exactly one per `Spawn` and per conversion to `Future<T>`; one per recursive await. Layout overlap is best-effort.

**Prior art.** Rust's `async fn` (inline, opaque types); C# `ValueTask` and pooling (manual); Swift's async frames, allocated with a task-local stack allocator [mem]. Mora differs by having no `Pin` (D-28 already guarantees no self-references) and by boxing recursion automatically.

**Impact.** For an HTTP server, one allocation per connection task plus one per request for the boxed handler future, instead of one per await. **Compile time:** small (async lowering already computes liveness). **Implementation:** M. **Milestone:** M6. D-18 must be amended before then. **Risks:** large states for deep await chains (mitigated by overlap and reported by #10); cancellation (§12.5) still drops the state, which drops inline children in reverse order, unchanged.

**Verdict:** Adopt for MVP. **Confidence:** High.

---

### 8. Deterministic MIR optimizer: inliner, SROA, ARC after inlining; LINQ fusion and bounds-check guarantees

**Problem.** D-11 and §15.5 promise that `orders.Where(o => o.Paid).Select(o => o.Total).Sum()` "compiles to one loop with no allocation and no virtual calls". The iterator structs make the allocation part true by construction. The "one loop" part requires inlining `Next()` of three nested iterator structs and the closure-struct calls, then scalar replacement of the structs. Cranelift has a strong mid-end (e-graph GVN, LICM, alias-based redundant load elimination) but it works on one function at a time, does not do the kind of interprocedural inlining a front end needs [verify recent Cranelift inlining work for Wasmtime], and does not eliminate bounds checks through range analysis. §14.1 also runs ARC optimization *before* "MIR optimizations", so retains made redundant by inlining would never be removed.

**Design.**
- **Pipeline after monomorphization:** inline → SROA (scalar replacement of aggregates, which turns iterator structs and small values into SSA locals that Cranelift can keep in registers) → copy propagation and constant folding → ARC insertion → ARC optimization (#3) → stack promotion (#2) → reuse (#4) → simplify CFG and dead code elimination → Cranelift.
- **Deterministic inlining policy** (counted in MIR statements, never in time, so builds are reproducible): always inline `[Inline]` functions, closure-struct calls whose closure has one call site, std iterator adapters' `Next()`, trivial accessors, and bodies under a small statement budget. Never inline `[Inline(Never)]`, recursive SCC members, or cold paths (panic branches). Inline in debug builds too, but only the cases that affect allocation or ARC guarantees (closure structs, accessors), so debug and release keep the same allocation counts.
- **Bounds checks.**
  - `foreach` over arrays, spans and `List<T>` has **no per-element bounds check** by construction: the enumerator knows the length (**guarantee**).
  - The canonical `for (var i = 0; i < a.Length; i++) { ... a[i] ... }` over an array or span local or parameter that the loop does not reassign has no check (**guarantee**). Arrays never change length (§5.4), so this needs no alias analysis.
  - The same loop over a `List<T>` keeps its check unless the loop body's mod-ref summary (#3) shows no structural change of any `List<T>` (best-effort).
  - The `List` enumerator's version check per `Next()` is hoisted out of the loop under the same summary (best-effort).
- Integer overflow stays wrapping in release (D-12); debug builds use Cranelift's flag instructions, as §14.13 says.

**Guarantee or best-effort.** The LINQ "one loop, no calls to the std adapters, no allocation" promise becomes a guarantee checked by MIR snapshots for the listed operators. The two loop shapes above are guaranteed. The rest is best-effort and shown by #10 ("bounds check kept: loop calls `Add` on `List<Order>`").

**Prior art.** Rust relies on LLVM for all of this; .NET's RyuJIT has range-check elimination and recently "array de-abstraction" of enumerators in .NET 10 (startdebugging.net summary of .NET 10 JIT work); Swift has a dedicated bounds-check optimization in SIL [mem]. Mora puts it in MIR so it does not depend on the backend.

**Impact.** This is where Cranelift-built Mora closes most of the gap with LLVM on everyday code. Cranelift's own comparisons put its output about 14 percent behind an LLVM-based engine on WebAssembly workloads (cited on cranelift.dev); most of what remains is inlining and loop optimizations that this pass supplies. **Compile time:** inlining raises code size; the budget keeps it bounded (watch the §20.6 build budgets). **Implementation:** M-L, the largest item here. **Milestone:** inliner and SROA in M2 or M3 (needed by #2 and #3), LINQ snapshots in M5, bounds checks in M5. **Risks:** compile-time growth; inlining must preserve spans for diagnostics and debug line tables (§14.7).

**Verdict:** Adopt for MVP. **Confidence:** High.

---

### 9. 16-byte strings with 15-byte inline storage and one-allocation concatenation

**Problem.** §13.4 makes `string` one pointer to a heap object. Every short string allocates: `id.ToString()`, each piece of `csv.Split(',')`, each JSON property value, each header value, each `Trim()`. In web code most strings are short. Swift's 16-byte `String` stores up to 15 UTF-8 bytes inline, and C++ libraries use a small-string optimization; C# and Java allocate every one.

**Design.**
- `string` becomes a 16-byte value: either **inline** (up to 15 bytes plus a tag and length byte), or **heap** (a pointer to `{ header, length, bytes }` plus the length in the second word). The tag is in the last byte. `string?` uses a spare tag value for `none` (#12), so it is still 16 bytes.
- Copying a heap string retains, and copying an inline one does not: one branch on the tag. Equality checks the length first. Literals stay immortal (§5.3).
- **One allocation per concatenation expression:** `a + b + c + d` lowers to one n-ary `Concat` that computes the total length first; `$"..."` measures its holes with `IDisplay` into a stack buffer and allocates once at the end (§7.13 already promises one string). Results of up to 15 bytes allocate nothing.
- `Split` returns a lazy iterator struct (as LINQ does), not an array, so `foreach (let f in csv.Split(','))` allocates only the pieces longer than 15 bytes. `ToList()` on it remains available.
- **Spans of inline strings.** §10.6 allows returning `text.AsSpan()` of a `string` parameter because "strings never move their storage". Inline strings do move with their owner. The rule still holds because a string parameter is passed by pointer to the caller's place (§10.3, not plain) and that place stays put while the view is live (§10.6). `AsMemory()` on an inline string, which must outlive any place, moves the bytes to a heap string first (one documented allocation).

**Guarantee or best-effort.** Guaranteed: strings of at most 15 bytes never allocate, and a concatenation or interpolation allocates at most once. **Worst case:** strings are 16 bytes instead of 8, so records and dictionary entries holding strings grow, and each copy has a branch.

**Prior art.** Swift `String` (16 bytes, 15 inline) [mem]; Go strings are also 16 bytes (pointer and length) without inline storage; .NET strings are 8-byte references. **Impact:** for request handling and JSON, a large fraction of string allocations disappear [estimate; measure on the acceptance test]. **Compile time:** none. **Implementation:** M (runtime string functions, codegen of copies, FFI passes pointer and length). **Milestone:** M1.15 and M2, before std text work in M5; it changes D-14's representation, not its semantics. **Risks:** interaction with views (above); memory per record grows by 8 bytes per string field, which #5's 64-byte rule absorbs.

**Verdict:** Adopt for MVP. **Confidence:** Medium-high.

---

### 10. Cost visibility: `explain-cost`, LSP cost lens, allocation budgets in tests, deterministic CI counters

**Problem.** "No tweaking needed" holds only if the remaining cost is easy to find. Today's languages make this hard: Go prints `-m` escape output nobody reads; .NET needs a profiler to find allocations; Swift's retain/release show up only in Instruments. G2 says "allocation and reference-count costs are visible", but the spec only shows boxing hints (§8.4) and shared-capture hints (§10.9).

**Design.**
- **`mora explain-cost [path::Function] [--diff base]`** (also `--message-format json` for agents, see companion #5). Per function, after optimization: every heap allocation with its type, size and **why it was not promoted**, as an escape chain ("`user` escapes: stored into `_users` by `Dictionary.Set` (line 14)"); every `any` box; every remaining retain/release inside a loop with the reason; every snapshot and copy-in/copy-out (§10.4); every struct copy over 64 bytes; non-inlined calls inside loops; remaining bounds checks in loops; future state sizes and recursive-await boxes (#7); reuse sites (#4). `--diff` compares two revisions.
- **LSP cost lens**, off by default to keep the editor quiet, toggled per file: inlay hints `heap`, `stack`, `box`, `+1 retain`, `snapshot`, `reuse?`. Hover on `new` and on parameters (`scoped`) works always.
- **Allocation budgets in tests:** `[Test(MaxAllocations = 0)]`, and `Assert.AllocatesAtMost(3, () => Handle(request))` in `Std.Testing`. Both runtimes keep a per-thread allocation counter, one increment per allocation, which costs nothing measurable next to the allocation itself.
- **Benchmarks in the toolchain:** `[Benchmark]` functions and `mora bench`. CI tracks **deterministic** numbers (allocations, bytes allocated, retains executed under the debug runtime, instructions retired where the runner allows), not just wall-clock time, so the §20.6 "worsens by more than 10 percent" rule is free of noise. Add "allocations per `GET /users/1`" to §20.6 (see #14).

**Guarantee or best-effort.** The counts are exact and identical in debug and release builds (a requirement on #2, #7 and #8). **Prior art:** Go's `testing.AllocsPerRun`; BenchmarkDotNet's `MemoryDiagnoser`; Swift's `-Xllvm -sil-print` [mem]. Mora's difference is the escape chain explanation and the fact that the optimizer's decisions are deterministic, so a test on allocation counts does not flake. **Impact:** no run-time cost; large DevX gain; it makes the guarantees in this study testable. **Implementation:** S-M; the compiler already has the facts. **Milestone:** counters and `MaxAllocations` in M2 (they are needed for the §20.3 tests of #2), `explain-cost` in M5, cost lens and `mora bench` in M7. **Risks:** hint noise (hence off by default).

**Verdict:** Adopt for MVP. **Confidence:** High.

---

### 11. Insertion-ordered Swiss-table `Dictionary` with keyed fast hashing

**Problem.** D-29 requires insertion order, which a plain Swiss table does not keep. §13.1 uses SipHash-1-3 for every key, which costs tens of nanoseconds for an `int` key [mem]. `Dictionary<int, User>` is the core data structure of §2.2.

**Design.**
- A **dense entry array** `(hash, key, value)` in insertion order, plus an **index table** of `u32` entry indices with one control byte per slot (7 bits of hash, Swiss-table style), probed 16 slots at a time with SSE2 or NEON (#17), at a load factor of 7/8. This is the layout of Python 3.6's compact dict [mem] and Rust's `indexmap` with a Swiss-table index [mem]. Removal leaves a tombstone in the entry array; the array compacts when more than half is tombstones, so iteration order is kept and the cost is amortized.
- Iteration walks the dense array (fast, cache-friendly). With #5, inline records live in the entries, with no pointer chasing.
- **Hashing:** `IHashable` with a per-process random key, as today, but with a fast keyed function: a folded-multiply hash for integers and short keys, and an AES-round hash for longer byte strings where the CPU has AES instructions (Go's runtime approach) [verify the HashDoS resistance of the chosen functions before committing]. SipHash-1-3 remains available as `Hasher.Strong` for code that hashes attacker-controlled keys and wants the stronger guarantee.

**Guarantee or best-effort.** Order is guaranteed (D-29). Speed is best-effort. **Prior art:** Abseil's Swiss tables (Kulukundis, CppCon 2017) [mem]; Go 1.24's built-in map, rewritten as Swiss tables (go.dev blog, "Faster Go maps with Swiss Tables", 2025), and Datadog's report of up to 70 percent less map memory in their services (one company's workload); hashbrown in Rust. **Impact:** faster lookups and smaller maps. **Implementation:** M. **Milestone:** M5.5. **Risks:** the hash-function choice (open question OQ-22 below).

**Verdict:** Adopt for MVP. **Confidence:** Medium-high (Medium for the hash choice).

---

### 12. Niche-filling layout guarantees for options and unions

**Problem.** §13.5 gives pointer types a free `T?` and leaves "further niche optimizations (for `bool?`, enums)" for later. Options are everywhere in Mora (lookups, JSON, `?.`), so their size matters for every record and collection.

**Design.** Layout computation (once, in `mora-codegen`, §14.13) uses invalid bit patterns as niches: `bool` (254 spare values), `char` (surrogates and values above U+10FFFF), enums (undeclared values, except `[Flags]`), union tags (spare tag values), the inline-string tag (#9), and nested options. A union whose cases other than the largest carry no data puts its tag in a niche of the largest case. **Published rule:** for every type with a niche, `sizeof(T?) == sizeof(T)`, and the spec lists which types have niches. Bit-packing several `bool` fields stays out (#29), because `ref` borrows need addressable fields.

**Guarantee.** Yes. **Impact:** smaller records and collections. **Implementation:** S. **Milestone:** M3 (unions). **Verdict:** Adopt for MVP. **Confidence:** High.

---

### 13. Guaranteed copy elision and in-place construction

**Problem.** Without a guarantee, returning a large struct, building a future (#7) or initializing a field may copy through temporaries. C++17 made this a guarantee; Swift guarantees it for returns [mem].

**Design.** Spec rule: a value returned by `return new S(...)`, `return localVar` (on every return path of the same local), a returned aggregate literal, or the result of a call assigned to a fresh local or field, is constructed directly in its final place. Values of more than two words are already returned through a caller-provided pointer (§13.7). Large struct `in` arguments are already passed by pointer (§10.3). Add a lint `large-copy` (allow by default, listed by #10) for copies over 256 bytes.

**Guarantee.** Yes, checked with MIR snapshots. **Implementation:** S. **Milestone:** M1.12 and M2. **Verdict:** Adopt for MVP. **Confidence:** High.

---

### 14. Zero-copy HTTP and JSON pipeline with pooled buffers and a per-request allocation budget

**Problem.** A naive implementation of §16 allocates per request a `Request` object, a header dictionary and one string per header, a path string, a route-value dictionary, a JSON output `string`, a `Response` object and its header dictionary: roughly 20 to 30 allocations for `GET /users/1` [estimate]. ASP.NET Core spent years removing these with pooling and `Span<T>` APIs, by hand.

**Design.**
- Each connection task owns a read buffer and a write buffer from a pool (16 KiB heads, §16.1). `httparse` already returns slices: the `Request` keeps offsets into the head buffer, and `Headers.Get(name)` builds the `string` lazily, usually inline (#9).
- `Request` and `Response` live in the connection task's state (#2 and #7): the server is library code that the same promotion rules cover, so no hand-written pooling is needed.
- `Results.Json(value)` stores the value and **serializes directly into the write buffer** when the response is written. No intermediate `string`.
- The `[Json]` macro generates serializers that write precomputed, pre-escaped property names (`"\"name\":"` as byte literals), and deserializers that dispatch on key length and then 8-byte word compares, generated at compile time. Number formatting writes straight into the buffer (§13.1 already uses shortest round-trip).
- **Budget:** add to §20.6 "`GET /users/1`, warm: at most 3 heap allocations per request" (expected: the boxed handler future of #7 plus at most two), checked with #10's counter in the acceptance test.

**Guarantee or best-effort.** The budget is a tested number. **Impact:** directly on the 20,000 requests per second budget of §20.6. **Implementation:** M, mostly in M5 and M6 std work that has to be written anyway. **Verdict:** Adopt for MVP. **Confidence:** Medium-high.

---

### 15. Collection sizing: size hints, exact `ToList`, size-class-aware growth, lazy buffers

**Design.**
- `IEnumerator<T>` gets a default member `SizeHint => (0, none)` (lower bound and optional exact count). Std adapters forward it (`Select` keeps it, `Where` gives an upper bound only). `ToList()`, `ToArray()` and `ToDictionary()` on a source with an exact count **allocate exactly once** (guarantee).
- `OrderBy(...).ToList()` sorts its buffer in place and **hands the buffer over** to the result list instead of copying it (one allocation total); this is §2.2's `All()`.
- `List<T>` grows by 2x and rounds the new capacity up to the allocator's size class (`mora_rt_good_size`, #1), so the slack the allocator would waste becomes usable capacity. `new List<T>()` and `new Dictionary<K, V>()` allocate no buffer until the first insertion.

**Guarantee.** Exact single allocation for sized sources; the rest is best-effort. **Implementation:** S. **Milestone:** M5. **Verdict:** Adopt for MVP. **Confidence:** High.

---

### 16. Identical code folding and instance deduplication for monomorphized code

**Problem.** Monomorphization (D-11) multiplies code: `List<User>`, `List<Order>` and `List<Product>` each get their own copy. Rust's attempt at polymorphization in rustc was removed again because it rarely paid off [mem, verify]. The §20.6 budget for hello world is 1.5 MiB.

**Design.** Mora's function values have **no equality** (§5.8) and Mora code cannot take a function's address, so merging identical functions is safe. Pass `--icf=all` to `ld.lld` and `lld-link` (macOS `ld` deduplicates by default [verify]). Post-MVP: hash monomorphized MIR bodies after ARC optimization and generate code once per distinct body, which saves compile time as well as size. Many `List<T>` instances for different shared classes differ only in which `T_drop` they call (#6), so a later step could pass the drop function as data for cold paths; measure first.

**Guarantee.** No (best-effort size reduction). **Impact:** typically 5 to 15 percent smaller binaries for generic-heavy code [mem]. **Implementation:** S for the linker flag. **Milestone:** M1.14 for the flag; MIR deduplication post-MVP. **Risks:** symbol names in backtraces can point at the folded twin (`List<Order>.Add` shown as `List<User>.Add`); document it. **Verdict:** Adopt for MVP (linker ICF) / post-MVP (MIR dedup). **Confidence:** Medium-high.

---

### 17. SIMD: hot std kernels in the Rust runtime now, `Vector128<T>` later, no auto-vectorization in Cranelift

**Problem.** Cranelift does not auto-vectorize. The text-heavy work of a web service (UTF-8 validation, `IndexOf`, JSON string scanning, comparisons, Swiss-table probes) is exactly where SIMD gives 4 to 16x.

**Design.**
- **MVP:** write these kernels in the Rust runtime, which is compiled by LLVM with SIMD (`memchr`, `simdutf8`-style validation, JSON whitespace and quote scanning, equality, the Swiss-table group probe). Std calls them for inputs of at least about 32 bytes and uses inline Mora code for shorter ones, so the FFI call does not dominate. The runtime is already Rust (D-50), so this costs nothing in architecture.
- **Post-MVP:** `Vector128<T>`, `Vector256<T>` and `Vector<T>` with .NET's names and operations (familiar to C# developers), lowered to Cranelift's 128-bit SIMD types (Cranelift supports them for WebAssembly SIMD) and to LLVM with #25.
- **Reject** auto-vectorization in Cranelift: it is a large research effort, and LLVM (#25) will bring it.

**Implementation:** S for the kernels, M for the types. **Verdict:** Adopt for MVP (kernels) / post-MVP (types). **Confidence:** Medium-high.

---

### 18. Closed-world devirtualization of `any I` values and function values

With companion #3, `any I` is rare and explicit. When the whole program converts only one concrete type to `any I` (the compiler sees every `ToInterface`, §14.7), calls through it become direct calls and can be inlined. The same applies to fields of function type that only one lambda is ever assigned to. Libraries (M8) break the closed world, so the analysis runs on the final program only. **Best-effort.** **Implementation:** S. **Verdict:** Adopt post-MVP. **Confidence:** Medium-high.

### 19. Small plain values stored inline in `any I` values, with no box

§8.4 boxes every struct converted to an interface value. A plain value of at most 8 bytes (an enum, an `int` wrapper, a small union) could be stored in the data word itself, with a vtable flag saying so, and the vtable methods receive a pointer to that word. This removes the box for small values at the cost of one flag check per copy of an `any I`. Swift's existential containers have a three-word inline buffer [mem], which makes them five words; Mora would stay at two. **Verdict:** Explore. **Confidence:** Medium. Measure how often small values are boxed once #3 of the companion is in.

### 20. Substring sharing with a bounded-waste rule

A slice of a heap string could share its owner's bytes (owner pointer, offset, length) instead of copying. Java removed this in 7u6 because a small substring kept a large string alive, a hidden memory cost. A deterministic rule would bound it: share only when the slice is at least half of the owner, otherwise copy. Retained waste is then at most 2x the slice. The 16-byte layout of #9 has room for a 32-bit offset. **Verdict:** Explore. **Confidence:** Medium-low. Inline strings (#9) already cover the short slices that dominate.

### 21. Inferred task-local allocation regions, backed by §10.12 counted arenas

**Idea.** The HTTP server opens an allocation region for each handler invocation. Allocation sites the compiler proves *task-local* (their objects never flow into captures of escaping closures, statics, or objects that outlive the request; a whole-program points-to analysis over monomorphized MIR, Steensgaard-style) allocate from a bump region that is freed in one step at the end of the request.

**Soundness** comes from §10.12 as specified: arena objects stay reference counted, and the arena's memory is freed only when its live count reaches zero. A misclassified object therefore costs memory, never safety. **The risk is the hidden cost:** one escaped object can pin a whole region chunk, and long-lived tasks (a keep-alive connection) must reset per request, not per task. With #1, #2, #5 and #7, most per-request garbage is already gone or freed promptly, so the gain is smaller than in GC languages. **Verdict:** Explore, after measuring #14. **Confidence:** Medium-low. **Cost:** L.

### 22. Mora-internal calling convention that returns `Result` and options in registers

Errors are values in Mora, so almost every fallible function returns a `Result<T, E>`, often three or four words. Under the C conventions of §13.7 anything over two words returns through memory. An internal convention (Cranelift supports multiple return values [verify the register limits per target]) could return up to four words in registers and keep the C convention for `[Export]`, FFI and function values. Debuggers unwind through DWARF CFI either way. **Verdict:** Explore (measure the cost on the acceptance test first). **Confidence:** Medium.

### 23. Reference-count atomicity when threads arrive (OQ-7): mark on send, never atomic before

D-5 keeps non-`shared` classes non-atomic forever. For strings and arrays (OQ-7), the options are: always atomic (Swift; several times the cost of a plain increment even uncontended [mem]); biased reference counting, with an owner-thread field (Choi, Shull and Torrellas, PACT 2018) [mem], which needs a larger header; or **mark on send**: a "shared" bit set when a value crosses a thread boundary (sending is explicit: `Channel.Send`, `Task.Run` captures, `MR0411`), with RC operations testing the bit. Marking a string or an array of plain values is O(1); an array of counted references must mark its elements, which is O(n) and reported by #10. Recommendation for M10: mark on send, never atomic for values that were never sent. **Verdict:** Explore (M10). **Confidence:** Medium.

### 24. Profile-guided optimization from tests and benchmarks

`mora build --release --pgo` builds an instrumented binary, runs `mora bench` (or the integration tests), and uses the counts for inlining decisions, block layout and #18's speculative devirtualization. Results must stay reproducible: the profile is an input file committed to the repository, not something gathered at build time. **Verdict:** Explore. **Confidence:** Medium-low.

### 25. LLVM release backend as an extra, never a dependency of a guarantee

M13 already plans it. This study adds one rule: **every guarantee in §10.13 and in this study is delivered by MIR passes**, so the LLVM backend adds auto-vectorization, better register allocation and better scheduling (typically 10 to 30 percent on compute-heavy code [mem]), but never changes allocation or RC behaviour. That keeps debug, Cranelift release and LLVM release builds consistent for #10's tests. **Verdict:** Adopt post-MVP (M13). **Confidence:** High.

### 26. Static `[NoAlloc]` and checked `scoped` contracts through effect inference

Companion #1 infers capability effects. Allocation can be one more effect: `[NoAlloc]` on a function becomes a compile-time check that no path allocates (calls through `any I` and function values count as allocating unless #18 resolves them). `scoped` written on a parameter turns #2's inferred fact into a checked contract. Koka's `fip` check plays the same role for in-place updates. These are tools for the few who want them; the default needs neither. **Verdict:** Adopt post-MVP. **Confidence:** Medium.

### 27. Deferred or coalesced decrements, and background cycle collection

Deferred RC (as in high-performance RC collectors such as LXR [mem]) and background cycle collectors make freeing faster on average, but free memory at points the reader cannot see, which breaks G2 and the deterministic destructor timing of §10.8. Debug-time cycle detection (§10.11) and the `strong-cycle` lint stay. **Verdict:** Reject. **Confidence:** High.

### 28. Automatic struct-of-arrays transformation

An automatic AoS-to-SoA transform changes the cost of element access, `ref` borrows and spans of elements in ways the reader cannot predict, and it interacts with every other layout rule. A `[Soa]` macro that generates a `SoaList<T>` with the same API would make it opt-in and visible. **Verdict:** Reject (automatic) / Explore (`[Soa]` macro). **Confidence:** High.

### 29. Pointer compression and bit-packing of `bool` fields

Pointer compression (as in V8 or the JVM's compressed oops) needs a contiguous heap base that Mora does not have, and adds a decode to every load. `bool` bit-packing conflicts with `ref` borrows of fields (§10.3) and makes every write a read-modify-write. The 8-byte header (#6) and niches (#12) give most of the memory win. **Verdict:** Reject. **Confidence:** High.

### 30. Automatic string interning

Interning hashes every string at creation time, a hidden cost, and keeps the strings alive in a global table. Literals are already deduplicated immortal objects (§5.3), and #9 removes short-string allocations. An explicit `Atom` type in std could come later. **Verdict:** Reject. **Confidence:** Medium-high.

### 31. Copy-on-write value-semantic collections

Swift's collections are values with copy-on-write, which keeps them safe but makes the cost of `a = b; a.Add(x)` depend on aliasing. Mora's collections are reference types, as in C# (§15.3), mutation is in place, and the borrow count and version number (§10.10) handle the aliasing problems. Switching would trade a familiar model for a hidden-copy model. Reuse (#4) already gives in-place updates where values are immutable. **Verdict:** Reject. **Confidence:** Medium-high.

---

## Coherence: one memory and CPU story

### How the pieces combine

The proposals form one pipeline, each stage feeding the next:

1. **Ownership facts from the language** (already in the spec): values by default, `in`/`move`/`ref` modes, unique classes, views that cannot escape, no inheritance, no reflection, whole-program monomorphization. These give the compiler more facts than Swift, Go, Java or C# have.
2. **Summaries** (#2, #3): for every function, which parameters escape, which are retained, which fields may be written, which types may be released. One set of salsa queries, used by every later stage and shown by hover and `explain-cost`.
3. **Inlining and SROA** (#8): turns iterator chains, accessors and closure structs into straight-line code, so the stages below see the real data flow.
4. **RC elision** (#3): retains and releases cancel across calls; snapshots disappear; values die at their last use.
5. **Reuse** (#4): a value that dies where a new one of the same shape is built gives its cell to the new one when it is unique.
6. **Placement** (#2, #5, #7, #9, #12): what does not escape lives in the frame or the future state; records of 64 bytes or less, short strings and options live inline; a spawned task is one block.
7. **What remains goes to a fast allocator** (#1) with an 8-byte header (#6), and is listed by #10.

Regions (#21) would be a stage 8 for request-shaped workloads, if measurements show they are needed after stages 1 to 7.

The CPU side follows the same structure: summaries let the inliner and devirtualizer (#8, #18) work without guessing, the runtime's Rust kernels (#17) handle bulk byte work, and the LLVM backend (#25) improves code generation without changing any of the guarantees.

### Guarantees Mora can publish in §10.13

Each is checked by MIR snapshot or allocation-count tests in **both** build profiles:

1. A class, record or unique object of at most 1 KiB whose reference is used only in locals, as `in` arguments to `scoped` parameters, as a `scoped` receiver, or by non-escaping closures, and which does not outlive its loop iteration, never touches the heap. (#2)
2. A value whose drop runs no user code is released immediately after its last use. (#3)
3. Passing a `mut` field as `in` to a function that cannot write it makes no copy; a retain/release pair around a call that cannot release that type is removed. (#3)
4. `with` on a uniquely referenced record and `+=` on an unaliased local string update in place when there is room; repeated `+=` is amortized linear. (#4)
5. A struct of at most 64 bytes that does not contain itself is stored inline. (#5)
6. `await` on a call allocates nothing; `Task.Spawn` allocates once; a recursive `await` allocates once per call. (#7)
7. A LINQ chain of the std operators over an array, span or `List<T>` compiles to one loop with no allocation and no calls to the adapters; `foreach` over an array, span or list, and the canonical `for` over an array or span, have no per-element bounds checks. (#8)
8. A string of at most 15 bytes never allocates; `a + b + ...` and `$"..."` allocate at most once. (#9)
9. `T?` is the same size as `T` for every type listed as having a niche. (#12)
10. A returned value is constructed in the caller's destination. (#13)
11. `ToList`, `ToArray` and `ToDictionary` on a source with an exact count allocate exactly once. (#15)

Together with today's seven guarantees, this gives a developer a short, testable answer to "what does this line cost?".

### What Mora should NOT do

- **No collector of any kind, and no deferred decrements** (#27). G2 is Mora's identity.
- **No guarantee that depends on LLVM, on inlining heuristics tuned by time, or on the build profile.** Allocation behaviour must be the same in debug and release builds, or `MaxAllocations` tests would mean different things in each. Optimization budgets are counted in MIR statements, so builds stay reproducible.
- **No silent extra cost.** Where an optimization trades one cost for another (inline records copy more bytes, 16-byte strings branch on copy, recursive awaits box), the trade is fixed by a published rule and shown by #10.
- **No automatic layout transforms that change access cost unpredictably** (#28, #29), and no automatic interning (#30).
- **No tuning knobs as the primary interface.** `scoped`, `[NoAlloc]` and `[InPlace]` exist to check or override, never because the default needs help.
- **No best-effort escape analysis as the only model** (the Go and HotSpot way): it is used here only for the extensions (list buffers), while the core rule is published.

---

## Spec sections and decisions each adopted idea touches

| Proposal | Spec sections | Decisions |
|---|---|---|
| P1 Allocator | §13.1, §13.2, §14.14, §19.4 (platform interface) | D-37, D-50 |
| P2 Stack promotion | §10.13, §14.8 (escape check), §14.1 pipeline order, §17.8 hover | D-3, D-49 (new: inferred `scoped`) |
| P3 Precise RC | §10.4, §10.8 (MAY to MUST), §10.13, §14.11, §14.12 | D-38 |
| P4 Reuse | §6.5 `with`, §6.6 `indirect`, §13.4, §14.12 (Perceus no longer all post-MVP) | new decision |
| P5 Structs as values (D-56) | §1.7, §5.1, §6.5, §10.2, §10.7, §13.5 | new decision; companion #12 |
| P6 8-byte header | §13.3, §13.6, §10.11 (debug prefix) | new decision |
| P7 Futures | §6.4, §12.2, §12.3, §12.4, §14.9 | D-18 (amend), D-27, D-28 |
| P8 MIR optimizer | §14.1, §14.7, §14.13, §15.5, §5.4 | D-11 |
| P9 Strings | §5.3, §10.6, §13.4, §13.9, §7.13, §15.2 | D-14 (representation only) |
| P10 Cost visibility | §8.4, §15.9, §17.1, §17.8, §20.5, §20.6 | new decision |
| P11 Dictionary | §13.1 hashing, §15.3 | D-29 (kept) |
| P12 Niches | §5.6, §13.5 | none |
| P13 Copy elision | §10.13, §13.7 | none |
| P14 HTTP/JSON pipeline | §16.1, §16.3, §20.6 | D-34 |
| P15 Collection sizing | §8.5 (`IEnumerator<T>`), §15.3, §15.5 | none |
| P16 ICF | §14.14, §20.6 | none |
| P17 SIMD kernels | §13.1, §15.2 | D-50 |
| P18 Devirtualization (post-MVP) | §8.4, §14.10 | none |
| P25 LLVM rule (post-MVP) | §14.13, §22 (M13) | D-13 |
| P26 Contracts (post-MVP) | §6.11, companion #1 | none |

### New open questions

| ID | Question | Default until decided | Decide by |
|---|---|---|---|
| OQ-20 | Inline limit for structs (#5): 64 bytes, or a count of counted fields as well? | 64 bytes | M2.2 |
| OQ-21 | Stack promotion limits (#2): 1 KiB per object and 16 KiB per frame, and the limit inside async states | As stated | M2.13 |
| OQ-22 | Default hash function for `IHashable`: speed versus HashDoS resistance for string keys | Keyed fast hash, `Hasher.Strong` available | M5.5 |
| OQ-23 | Is `scoped` a keyword in the MVP (checked contract) or only a hover annotation? | Hover only; keyword post-MVP | M7 |
| OQ-24 | How are escape and ownership summaries recorded in `mora.ir` for libraries, and is a change a semver break? | Recorded; reported by `mora diff` | M8 |
| OQ-25 | Name and spelling of the boxed future type: `Future<T>` or `any IFuture<T>` | `Future<T>` | M6 |
| OQ-26 | Does the release runtime keep the allocation counter, or only test builds? | Both (one increment per allocation) | M2.3 |
| OQ-27 | Substring sharing (#20): adopt with the 2x rule or never share | Never share | M5.2 |

---

## Sources

Prior art and recent developments (checked with web search in October 2026 unless marked):

- Alex Reinking, Ningning Xie, Leonardo de Moura, Daan Leijen. *Perceus: Garbage Free Reference Counting with Reuse*. PLDI 2021; extended version MSR-TR-2020-42. https://www.microsoft.com/en-us/research/publication/perceus-garbage-free-reference-counting-with-reuse-2/
- Anton Lorenzen, Daan Leijen. *Reference Counting with Frame Limited Reuse*. ICFP 2022. https://www.microsoft.com/en-us/research/publication/reference-counting-with-frame-limited-reuse/
- Anton Lorenzen, Daan Leijen, Wouter Swierstra. *FP²: Fully in-Place Functional Programming*. ICFP 2023, PACMPL 7, DOI 10.1145/3607840.
- Sebastian Ullrich, Leonardo de Moura. *Counting Immutable Beans: Reference Counting Optimized for Purely Functional Programming*. IFL 2019 (Lean 4 borrow inference). https://www.microsoft.com/en-us/research/?p=915972 [content from memory]
- Anton Lorenzen, Leo White, Stephen Dolan, Richard A. Eisenberg, Sam Lindley. *Oxidizing OCaml with Modal Memory Management*. ICFP 2024; Jane Street, "Oxidizing OCaml: Locality". https://blog.janestreet.com/oxidizing-ocaml-locality/
- Lobster memory management: compile-time reference counting, "95% of reference count ops removed at compile time" (project's own claim). https://strlen.com/lobster/
- Swift: OSSA and `-enable-ossa-modules` status, "Swift Language focus areas heading into 2025". https://forums.swift.org/t/swift-language-focus-areas-heading-into-2025/76611 ; Swift stack promotion and String layout [mem].
- Mojo manual, "Death of a value" (ASAP destruction). https://docs.modular.com/mojo/manual/lifecycle/death
- .NET 10 runtime: stack allocation of small arrays and delegates, escape analysis. https://learn.microsoft.com/dotnet/core/whats-new/dotnet-10/runtime ; design doc https://github.com/dotnet/runtime/blob/main/docs/design/coreclr/jit/object-stack-allocation.md ; .NET 10 array enumeration de-abstraction https://startdebugging.net/2025/04/net-10-array-ennumeration-performance-improvements-jit-array-de-abstraction/
- Go: stack allocation of slice backing stores in Go 1.25 and 1.26. https://go.dev/blog/allocation-optimizations
- Go 1.24 Swiss-table maps: https://go.googlesource.com/website/+/33b532a2f57ccceaeaf826d86ca10b069ce5cd9e/_content/blog/swisstable.md ; Datadog case study https://www.datadoghq.com/blog/engineering/go-swiss-tables/
- Matt Kulukundis, "Designing a Fast, Efficient, Cache-friendly Hash Table, Step by Step" (Abseil Swiss tables), CppCon 2017 [mem].
- Daan Leijen, Benjamin Zorn, Leonardo de Moura. *Mimalloc: Free List Sharding in Action*. MSR-TR-2019-18. https://www.microsoft.com/en-us/research/publication/mimalloc-free-list-sharding-in-action ; mimalloc v3 readme https://raw.githubusercontent.com/microsoft/mimalloc/main/readme.md
- Cranelift: performance relative to LLVM-based engines on WebAssembly (about 14 percent slower than WAVM, cited by the project). https://cranelift.dev/ ; Rust project goal on cg_clif compile-time performance (2026). https://rust-lang.github.io/goals/2026/improve-cg_clif-performance.html
- Swift lifetime dependencies, `Span` and `InlineArray` (Swift 6.2). https://forums.swift.org/t/experimental-support-for-lifetime-dependencies-in-swift-6-2-and-beyond/78638
- Jiho Choi, Thomas Shull, Josep Torrellas. *Biased Reference Counting: Minimizing Atomic Operations in Garbage Collection*. PACT 2018 [mem].
- David F. Bacon, V. T. Rajan. *Concurrent Cycle Collection in Reference Counted Systems*. ECOOP 2001 (already in §25).
- Raymond Hettinger's compact dict design, adopted in CPython 3.6; Rust `indexmap` [mem].
- Dimi Racordon et al., *Implementation Strategies for Mutable Value Semantics*, JOT 2022 (Hylo) [mem].
- musl mallocng throughput compared with glibc, jemalloc and mimalloc: widely reported in Alpine and Rust community issues [mem, verify with a benchmark before relying on it].
