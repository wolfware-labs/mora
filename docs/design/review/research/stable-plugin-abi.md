# Stabilize the plugin boundary, not the language

Mora should not give the whole language a stable ABI. It should let an author mark one **plugin-interface package** as the boundary, and give that package's types a published, compiler-independent binary contract. That contract is a versioned **Mora ABI edition** covering the object header protocol, the layouts of `string`, arrays, `Option` and `Result`, an append-only vtable format and a runtime function table. Everything outside the boundary stays whole-program optimized, exactly as today. The survey supports this firmly. Every system that made its ABI implicit and total paid heavily for it. C++ froze its standard library and now leaves an estimated 5-10% of performance on the table, while a single ABI break cost Google 5-10 engineer-years. Go's `plugin` package requires the host and every plugin to be "built together by a single person", so the Terraform ecosystem moved to subprocesses. Rust has no ABI beyond `extern "C"`, and Bevy deleted its dynamic plugins as unsound. The designs that aged well (COM/VST3, Node-API, Vulkan, Godot GDExtension, CPython's new abi3t, and Swift's library evolution, which is off by default) share four moves. They hide layout behind handles. They reach host functions through a table resolved at load time. They negotiate a version once, at load. They evolve only by appending. The proposal sketch from the conversation is mostly right, and this report sharpens it in six places. Code for non-frozen boundary types lives only in the host. Interfaces get append-only vtables with a slot count, so new members with defaults stay compatible. Retain and release keep an inline fast path guarded by a runtime-published mask, which leaves the refcount format free to change. The load check uses per-minor surface hashes plus a compiler-computed minimum version, not a single hash. Unloading is refused by default and allowed only for opt-in reloadable plugins, under a live-object count. And the same interface package also compiles to a Wasm component for untrusted code, so a host can choose the transport per plugin without changing the interface.

## Total ABIs either froze their language or were never offered

The languages surveyed fall into three camps. Some made everything ABI and paid for it, some made nothing ABI and pushed users to IPC, and one (Swift) made ABI an opt-in mode with per-type escape hatches.

**C++ is the cautionary tale of an accidental ABI.** No language standard defines one. Platform ABIs (Itanium on Unix-like systems, MSVC on Windows) fix layout, vtables and mangling, so every field, virtual function and inline body visible in a header silently becomes contract. When C++11 required an O(1) `std::list::size()` and banned copy-on-write strings, GCC 5.1 had to ship a dual ABI. New types went into an inline namespace (`std::__cxx11`), and a per-translation-unit macro selected the ABI. The failure modes included link errors and, worse, silent layout bugs when a class *containing* a `std::string` mangled identically under both ABIs ([libstdc++ manual](https://gcc.gnu.org/onlinedocs/libstdc++/manual/using_dual_abi.html)). MSVC took the opposite road. It broke ABI on every release until VS2013, then kept v140 through v145 (VS2026) binary-compatible, except that `/GL` and `/LTCG` objects are not compatible even across minor versions ([Microsoft Learn](https://learn.microsoft.com/en-us/cpp/porting/binary-compat-2015-2017)). Titus Winters' "ABI: now or never" put numbers on the trap. **Perhaps half of all C++ developers depend on ABI stability. A replacement hash table could be "200-300%" faster. The aggregate loss is "perhaps 5-10%", and one ABI-breaking change cost Google "5-10 engineer-years"** ([P1863R1](https://www.open-std.org/jtc1/sc22/wg21/docs/papers/2020/p1863r1.pdf)). The 2020 Prague poll on a C++23 break was "insufficient to call consensus" ([SG15](https://lists.isocpp.org/sg15/2020/03/0976.php)), so the de facto policy remains "we say performance but vote ABI". The lesson for Mora is direct. A whole-language stable ABI would freeze the layouts and inlined bodies that whole-program optimization exists to improve.

**Go and Rust show what happens with no usable ABI: the ecosystem routes around the language.** Go's `plugin` package works only on Linux, FreeBSD and macOS. Crashes are "likely" unless every part of the program uses "exactly the same version of the toolchain" and the same common dependencies, plugins "cannot be closed", and the docs conclude that host and plugins "must all be built together by a single person or component of a system" ([pkg.go.dev/plugin](https://pkg.go.dev/plugin)). HashiCorp's go-plugin, used by Terraform, Vault, Nomad and Packer, runs plugins as subprocesses over gRPC or net/rpc instead. It accepts lower throughput in exchange for crash isolation, version negotiation and independent builds ([go-plugin](https://github.com/hashicorp/go-plugin)). Rust has nothing beyond `extern "C"`/`repr(C)`. crABI, the proposed opt-in `extern "crabi"` that would pass `Option`, `Result` and counted strings by lowering them to C, was approved for experimentation in December 2022. Its PR has been `S-waiting-on-author` since November 2024, with niche encoding, tuples and extensible enums still unresolved ([rust-lang/rust#105586](https://github.com/rust-lang/rust/pull/105586)). The ecosystem compensates with crates. abi_stable rebuilds `RString`, `RVec` and FFI-safe trait objects, checks type layouts recursively at load time, offers prefix types and `NonExhaustive` enums for versioning, and lists unloading as a non-feature ([abi_stable](https://github.com/rodrimati1992/abi_stable_crates)). stabby keeps niche packing and embeds compiler-version canaries. In exchange it gives up pattern matching on its enums, slows compilation, and since rustc 1.78 has a vtable performance and memory-growth problem ([stabby](https://github.com/ZettaScaleLabs/stabby)). Bevy deprecated its dynamic plugins because "the current implementation for dynamic plugins is unsound", calling them "too much of a footgun for the meager benefit", and removed them for 0.15 ([bevy#13080](https://github.com/bevyengine/bevy/pull/13080)). Zed builds extensions as Wasm components with versioned WIT directories ([Zed](https://zed.dev/blog/zed-decoded-extensions)). Nushell uses separate processes speaking MessagePack, with a version handshake ([Nushell](https://www.nushell.sh/contributor-book/plugin_protocol_reference.html)). This is the "everything must go through the C ABI" complaint in the question. The pain is not the C calling convention. It is that C has no owned string, no shared allocator, no sum types, no interfaces and no version negotiation, so every crate rebuilds them.

**Swift is the one language that made a rich ABI work, and it did so by making it opt-in and resilient by default.** Swift splits the problem in three: ABI stability (Swift 5.0, 2019, Apple platforms only), module stability (a textual `.swiftinterface`), and library evolution ([swift.org](https://www.swift.org/blog/abi-stability-and-more/)). With `-enable-library-evolution`, public structs and enums have opaque layout across the module boundary. Clients handle them through type metadata and value witness tables, access properties through accessor functions, and call protocol and class methods through dispatch thunks. Values are not boxed. A resilient struct has the same in-memory layout as a fragile one, and the cost lies in client code that asks the runtime for sizes and offsets ([Library Evolution blog](https://www.swift.org/blog/library-evolution/)). Authors buy back speed type by type with `@frozen` and `@inlinable`, and adding or removing `@frozen` is itself ABI-breaking ([SE-0260](https://github.com/swiftlang/swift-evolution/blob/main/proposals/0260-library-evolution.md)). Two facts matter most for Mora. First, **the mode is off by default, and Apple says to use it only for frameworks "built and updated separately from its clients"**. Second, Swift's resilience reuses the machinery of unspecialized generics: metadata, witness tables and indirect passing ([ABIStabilityManifesto](https://github.com/swiftlang/swift/blob/main/docs/ABIStabilityManifesto.md)). Mora monomorphizes everything. It has no such machinery, so it would have to build Swift's whole apparatus just for plugins. Swift also publishes no benchmark of the resilience overhead. Even Apple was still tuning cross-dylib refcounting in 2025: PR #85044 inlines the fast path of `swift_retain`/`swift_release` into clients and checks a runtime-exported `_swift_retainRelease_slowpath_mask_v1`, so the refcount representation stays versionable ([PR #85044](https://github.com/swiftlang/swift/pull/85044)).

Objective-C and .NET complete the picture. Objective-C methods were always non-fragile because `objc_msgSend` dispatches by selector. Instance variables became non-fragile only when the 64-bit and iOS runtime replaced hardcoded offsets with a per-ivar offset *variable* that the runtime slides at load time ([Sealie Software](https://sealiesoftware.com/blog/archive/2009/01/27/objc_explain_Non-fragile_ivars.html)). That fix needed a platform transition, because it could not be retrofitted onto the 32-bit ABI. .NET gets binary compatibility almost free because the JIT resolves offsets, slots and generic instantiations on the target machine. Once code is precompiled, the fragility returns: ReadyToRun images are 2-3 times larger, still fall back to the JIT for "generic types defined in separate assemblies", and only Composite R2R, a set "that must be distributed together", gets cross-assembly optimization ([Microsoft Learn: ReadyToRun](https://learn.microsoft.com/en-us/dotnet/core/deploying/ready-to-run)). The general rule holds everywhere: **binary compatibility is cheap when layout is decided at load time or never exposed, and expensive when it is baked into clients.**

| System | Boundary mechanism | Add field | Add method | Cross-boundary generics | Unload | Verdict for Mora |
|---|---|---|---|---|---|---|
| C++ (Itanium/MSVC) | Implicit: headers are ABI | Breaks | Breaks if virtual | Instantiated in client, body frozen | Platform-dependent | Avoid: implicit, total ABI |
| Objective-C 2.0 | Selector dispatch, ivar offset variables | OK (slid at load) | OK | n/a | Not covered by these sources | Borrow: load-time offsets |
| Swift library evolution | Opt-in mode, metadata, thunks, `@frozen` | OK unless frozen | OK (thunks) | Metadata plus witness tables | Not covered by these sources | Borrow: opt-in, frozen escape hatch |
| COM/WinRT, VST3 | Immutable vtable interfaces, IID, refcount in vtable | No fields cross | New interface (`IFoo2`) | Fixed parameterized interfaces only | Host-defined | Borrow: handles, append, capability query |
| .NET (JIT) | IL plus metadata, layout at load | OK | OK | JIT instantiates | Not covered by these sources | Not applicable: no JIT in Mora |
| Rust (abi_stable, stabby) | Library-level `repr(C)` rebuilds | Prefix types | Prefix vtables | No | No (abi_stable) | Avoid: no language support |
| Go `plugin` | Same toolchain and dependencies required | n/a | n/a | n/a | No | Avoid: lockstep builds |
| Wasm components | WIT, canonical ABI, copying | Versioned WIT | Versioned WIT | Fixed WIT types | Reinstantiate | Keep as Mora's sandboxed path |

## Plugin hosts converge on handles, tables, negotiation and appending

Real hosts confirm the pattern from the other side. They do not care about language elegance. They care about plugins built years apart still loading.

**The hosts that aged well never let plugins see host layout.** Node-API is "ABI stable across versions of Node.js" only for addons that use `node_api.h` exclusively. Every value is an opaque `napi_value`, every call returns a `napi_status`, versions are additive through `NAPI_VERSION`, and enums are "fixed size `int32_t`" that callers must switch on with a default branch ([Node-API](https://nodejs.org/api/n-api.html)). Vulkan layers must export `vkNegotiateLoaderLayerInterfaceVersion`. Loader and layer agree on the highest common version, and the loader remains "backwards compatible with all layer interface versions" ([Vulkan LoaderLayerInterface](https://github.com/KhronosGroup/Vulkan-Loader/blob/main/docs/LoaderLayerInterface.md)). VST3's `FUnknown` is binary-compatible with COM's `IUnknown`, and "once an interface has been released, it must never change again", so growth means `IPluginFactory3` extending `IPluginFactory2` ([Steinberg](https://steinbergmedia.github.io/vst3_dev_portal/pages/Technical+Documentation/VST+Module+Architecture/Index.html)). COM's own rule is identical: adding or removing methods "creates a new interface" with a new IID ([Microsoft Learn: COM interfaces](https://learn.microsoft.com/en-us/windows/win32/com/interface-pointers-and-interfaces)). WinRT formalizes it further. Structs and interfaces are immutable, while enums and runtime classes are "additively versionable" with a `Version` attribute that projections use for light-up ([WinRT type system](https://learn.microsoft.com/en-us/uwp/winrt-cref/winrt-type-system)). Godot's GDExtension entry point receives `get_proc_address`, a by-name lookup that returns null for unknown functions. New behaviour arrives as suffixed functions such as `get_godot_version2`, and the `.gdextension` manifest declares a `compatibility_minimum` ([godot-docs PR 7378](https://github.com/godotengine/godot-docs/pull/7378/files); [Godot commit](https://remotebranch.eu/Stowage/godot/commit/05ffa218f3dce405b5d359c353fd998eb766a89d); [godot-rust](https://godot-rust.github.io/book/toolchain/compatibility.html)). JNI does the same through `JNI_OnLoad`'s returned version and the per-thread `JNIEnv` function table ([JNI spec](https://docs.oracle.com/en/java/javase/21/docs/specs/jni/invocation.html)).

**CPython is the most relevant failure, because it is an ARC runtime that exposed its refcount.** The abi3 stable ABI (since 3.2) left the `PyObject` struct visible, refcount field included. PEP 803 calls that exposure "one of the main shortcomings of the existing Stable ABI", because it blocked immortalization and free-threading. Python 3.15's abi3t makes `PyObject` an incomplete type, removes `PyObject_HEAD`, turns `Py_SIZE` and `Py_IS_TYPE` into exported functions, and moves instance data behind `PyObject_GetTypeData()`. PEP 803 calls even that a "transitional state" ahead of an `abi2026` ([PEP 803](https://peps.python.org/pep-0803/)). The stable ABI also costs speed on purpose. `PyList_GET_ITEM` is unavailable because "compiling for Stable ABI disables this inlining", and the load-time check is opt-in and, by the docs' own account, incomplete ([Python docs](https://docs.python.org/3/c-api/stable.html)). HPy went further, with opaque handles, a context passed to every call and refcounting outside the API, so one binary loads on CPython, PyPy and GraalPy ([HPy](https://docs.hpyproject.org/en/latest/overview.html)). For Mora the lesson is specific. §13.3 already publishes a 16-byte header with an immortal bit, and the compiler inlines retain and release. **If plugins inline that code against a frozen header, Mora can never move to biased or atomic-by-default counts without a new ABI.** The fix is Swift's 2025 answer: inline the fast path, but route any header bit the plugin was not built to understand through a runtime-published mask to the slow path.

**Some hosts deliberately promise nothing, and they are coherent too.** Linux has no stable in-kernel ABI, because compiler and config options change structure alignment and fields, and internal APIs must stay free to change for security fixes ([kernel.org](https://www.kernel.org/doc/html/latest/process/stable-api-nonsense.html)). PostgreSQL rejects a module built for another major version through `PG_MODULE_MAGIC` ([PostgreSQL](https://www.postgresql.org/docs/current/xfunc-c.html)). Unreal rejects a plugin whose BuildId differs, and forum advice is to recompile because faked BuildIds cause "random crashes" ([Epic](https://dev.epicgames.com/documentation/unreal-engine/how-to-version-binaries-in-unreal-engine)). Mora must scale to an OS kernel, so it needs the mechanism without imposing the policy. A kernel written in Mora can mark nothing as a boundary and stay fully whole-program, or it can define a narrow driver-interface package with a published edition.

**Wasm components are the strongest alternative, and Mora already has them.** WASI 0.3.0 shipped on 2026-06-11 with native `async func`, `stream<T>` and `future<T>`, on by default in Wasmtime 46 ([wasi.dev](https://wasi.dev/releases/wasi-p3)). The Bytecode Alliance's own roadmap says async task handling across component boundaries "adds roughly 3.5x overhead even on purely synchronous call paths". Returned lists are copied element by element through `cabi_realloc`, and the lazy zero-copy ABI that fixes this is planned for 0.3.x as opt-in and becomes the default only at 1.0, which waits on two browser implementations ([bytecodealliance.org PR #147](https://github.com/bytecodealliance/bytecodealliance.org/pull/147/files)). The sandbox buys things native code never can. Shopify Functions enforce a deterministic 11-million-instruction budget ([Shopify](https://shopify.dev/docs/apps/build/functions/optimize-instruction-counts)), and hosts get sound unloading by reinstantiation. The cost is copying and call overhead that an audio plugin, a game-engine system or a kernel driver cannot pay. Mora should treat the two as **one interface with two transports**, not as competitors.

## Five run-time problems decide whether native plugins are pleasant

Layout is only half of a plugin ABI. The hosts above solve the same five run-time problems, and Mora's existing choices already settle three of them.

**Memory: one allocator, reached through the runtime.** PostgreSQL forces `palloc`/`pfree` ([PostgreSQL](https://www.postgresql.org/docs/current/xfunc-c.html)). abi3t forces the host to own object layout and allocation ([PEP 803](https://peps.python.org/pep-0803/)). COM puts `AddRef`/`Release` in the object's own vtable so the allocator stays private to the creating binary ([COM](https://learn.microsoft.com/en-us/windows/win32/com/the-component-object-model)). Mora's §13.3 design already routes deallocation through `mora_rt_drop` and the type's `meta` pointer. The plugin boundary therefore needs one runtime instance per process, shared by host and plugins, rather than a static `libmora_rt.a` copy in each binary. A side effect matters more than it first appears. Because native builds are whole-program, each plugin carries its own compiled copy of the parts of `Std` it uses. Duplicated *code* is harmless. Duplicated *process-global state* is not: two stdout buffers interleave, and two executors deadlock. **Every piece of `Std` state that must be process-unique has to live behind the runtime table.**

**Errors: never unwind across the boundary.** Every host forbids it. PostgreSQL tells C++ authors to catch everything at each `extern "C"` function, because its `longjmp` error path does not unwind C++ frames ([PostgreSQL](https://www.postgresql.org/docs/current/xfunc-c.html)). Rust needed `C-unwind` (1.71) to define panics crossing FFI ([Rust 1.71](https://blog.rust-lang.org/2023/07/13/Rust-1.71.0/)). Mora already has the cleanest possible answer. Errors are `Result` values, and a panic aborts the process without unwinding (§11.4). "Panics don't cross the boundary" is therefore free. What remains is attribution: a plugin's panic message must name the plugin and its version, because it takes the host down with it.

**Lifetimes: refcounts or handles, never borrowed pointers that outlive a call.** Node-API needs handle scopes and `napi_ref` ([Node-API](https://nodejs.org/api/n-api.html)). Mora's ARC already gives every shared object an owner-independent lifetime, and its view rules (no borrow across `await`, §12.3) already confine spans to a call. Shared objects cross as retained pointers. Views cross only as `in` parameters for the duration of a call.

**Threads: explicit context, not ambient globals.** JNI's `JNIEnv` is "valid only in the current thread", and foreign threads must attach ([JNI](https://docs.oracle.com/en/java/javase/21/docs/specs/jni/invocation.html)). CPython needed a whole new ABI tag before extensions were safe on free-threaded builds ([PEP 803](https://peps.python.org/pep-0803/)). Mora's MVP is single-threaded with non-atomic counts, and `shared class` with atomic counts is post-MVP (§12.6). The boundary must encode sendability now, so that threads do not force a new edition later.

**Unloading: the problem nobody fully solves.** PostgreSQL has "no way to unload". On musl, `dlclose` is a no-op. glibc pins any DSO that registered a `thread_local` destructor. Go and abi_stable cannot unload. Bevy's reload issue was closed along with its dynamic plugins ([PostgreSQL](https://www.postgresql.org/docs/current/xfunc-c.html); [musl](https://wiki.musl-libc.org/functional-differences-from-glibc.html); [libc-alpha](https://sourceware.org/ml/libc-alpha/2015-07/msg00315.html); [pkg.go.dev/plugin](https://pkg.go.dev/plugin); [bevy#13080](https://github.com/bevyengine/bevy/pull/13080)). Godot supports GDExtension reload only from 4.2, behind a `reloadable` flag ([godot-rust](https://godot-rust.github.io/book/toolchain/compatibility.html)). Mora has a specific hazard of its own. String literals and `static readonly` objects are immortal objects in the binary's read-only data (§13.3), and every object's `meta` and vtable pointers point into the binary that defined its type. A host that keeps a plugin's literal string or object after `dlclose` holds a dangling pointer, with no refcount bug anywhere.

## The recommended design: one interface package, one runtime table, one ABI edition

### The boundary is a package, compiled once and owned by the host

An author opts in by marking a package as a stable ABI boundary in its manifest (`[abi] stable = true`, `edition = 1`). The compiler then checks that every public declaration in the package is *boundary-eligible*. Non-eligible public declarations are compile errors with a fix, so the boundary cannot grow by accident the way C++ headers did. **The interface package's non-frozen code is compiled into exactly one place, the host, and exported through a load-time table.** Plugins compile against its `mora.meta`, but they never inline the bodies of its non-frozen types. This is Swift's resilience domain made concrete: one version-locked side owns the implementation. Inside the host and inside each plugin, everything else stays whole-program optimized, including private types that implement boundary interfaces.

```mora
// package imagefx-api, mora.toml: [abi] stable = true, edition = 1
namespace ImageFx.Api;

newtype Percent : float is >= 0.0 and <= 100.0;

[Frozen]
public struct Rgba {
    public R: byte;
    public G: byte;
    public B: byte;
    public A: byte;
}

[NonExhaustive]
public enum Severity { Info, Warning, Error }

[NonExhaustive]
public union PluginError {
    InvalidInput(Message: string),
    Unsupported(Feature: string),
    Io(Path: string, Code: int);
}

// Not frozen: crosses as a handle; properties are accessor calls into the host.
public struct FilterOptions(Strength: Percent, Label: string);

public class Image {
    public Width: int;
    public Height: int;
    public Pixels(): Span<Rgba>;
}

[Sealed] // implemented only by the host: adding members never breaks plugins
public interface IHost {
    Log(severity: Severity, message: string);
    CreateImage(width: int, height: int): Image;
}

public interface IFilter {
    Name(): string;
    Apply(image: Image, options: FilterOptions): Result<Image, PluginError>;
    async Preview(image: Image): Result<Image, PluginError>;
    Describe(): string => "no description";   // added in 1.1 with a default: compatible

    contract {
        Apply {
            fails with PluginError.InvalidInput when image.Width == 0;
        }
    }
}

[PluginEntry]
public interface IFilterPlugin {
    Filters(host: IHost): List<IFilter>;
}
```

The plugin is an ordinary Mora package that implements the entry interface and exports one factory. Its classes are private, whole-program optimized, and visible to the host only through vtables.

```mora
// package blur-plugin, mora.toml: [plugin] implements = "imagefx-api@1"
using ImageFx.Api;

class Blur : IFilter {
    private _host: IHost;

    public Blur(host: IHost) { _host = host; }

    public Name(): string => "blur";

    public Apply(image: Image, options: FilterOptions): Result<Image, PluginError> {
        if (image.Width == 0) {
            return Err(PluginError.InvalidInput("empty image"));
        }
        let output = _host.CreateImage(image.Width, image.Height);
        BoxBlur(image.Pixels(), output.Pixels(), options.Strength);
        return Ok(output);
    }

    public async Preview(image: Image): Result<Image, PluginError> {
        return Apply(image, new FilterOptions(Strength: 25.0, Label: "preview"));
    }
}

class BlurPlugin : IFilterPlugin {
    public Filters(host: IHost): List<IFilter> {
        return [new Blur(host)];
    }
}

[Export(Plugin)]
public CreatePlugin(): IFilterPlugin {
    return new BlurPlugin();
}
```

The host loads it through a typed API that performs every check at load time and returns a `Result`, never a crash:

```mora
public async Main(): Result<(), AppError> {
    let host = new AppHost();   // the host's own class implementing IHost
    let grants = Grants.Of("fs.read");
    let plugin = Plugin.Load<IFilterPlugin>("plugins/libblur.so", grants)?;
    foreach (let filter in plugin.Instance.Filters(host)) {
        host.Log(Severity.Info, $"loaded {filter.Name()} from {plugin.Name} {plugin.Version}");
    }
    return Ok(());
}
```

`Plugin.Load` returns `Err(LoadError.…)` with a readable reason when any check fails, which is what Vulkan's negotiation, `JNI_OnLoad` and Node-API's version gating achieve. It never maps the library and hopes, as Go's `plugin` and Unreal's BuildId do.

### The Mora ABI edition is a short, published document

The edition is the language-level half of the contract. It is versioned separately from any interface package and changes rarely, in the way CPython moved from abi3 to abi3t. Edition 1 should fix the following and nothing more. The calling convention is the target's C convention, which §13.7 already uses, so every boundary entry point is also callable from C. A plain object header protocol: strong count at offset 0, weak count at offset 4, `meta` at offset 8, with retain and release emitted inline as `if (strong & rt.SlowMask) == 0 { strong += 1 } else { rt.Retain(obj) }`. `SlowMask` is published by the runtime and today contains only the immortal bit, so the runtime can later reserve bits for atomic, biased or other counting schemes without breaking any plugin. This is Swift PR #85044's design, adopted from day one instead of retrofitted. The layouts of `string` and `T[]` (§13.4), `List<T>`, tuples, `Option` (including the null-pointer niche) and the union tag rules for `[Frozen]` unions. The interface value is two words, and the boundary vtable is `[meta, drop, slotCount, slots…]`. A `TypeMeta` prefix holds size, alignment, drop and the 128-bit BLAKE3 type id of §13.6, so `is C c` works across binaries, unlike abi_stable's downcasting ([abi_stable](https://github.com/rodrimati1992/abi_stable_crates)). Finally, a **runtime function table** (`MoraRuntimeV1`): allocation, the drop slow path, weak references, panic, the executor and wakers, and the process-unique `Std` state. Plugins reach the runtime through that table rather than through dynamic-linker symbols. The plugin's loader stub stores the table pointer once at load, so each runtime call costs one load plus an indirect call. The design works identically on Windows (no host-executable exports needed), on musl, and in a kernel module loader with no dynamic linker at all. It mirrors HPy's context and GDExtension's `get_proc_address`. Because the edition is a written specification, not "whatever this compiler emits", **a plugin built with a newer Mora compiler loads into a host built with an older one whenever both support the edition.** That is the property Rust and Go lack, and it is what makes plugins easy to build: install any recent toolchain, target the edition, ship one binary per platform.

A side benefit follows from the same choice: `mora abi header imagefx-api` can emit a C header for the boundary, because every edition type lowers to C. C, Zig or Rust authors can then implement Mora plugins without a Mora compiler. That is crABI's goal, reached for Mora's boundary without waiting on Rust ([rust-lang/rust#105586](https://github.com/rust-lang/rust/pull/105586)).

### How each kind of value crosses

| Kind | How it crosses | Evolution rule | Cost per use |
|---|---|---|---|
| Primitives, `newtype` over them | By value, as the underlying representation | Refinement changes judged by `mora diff` | None |
| `[NonExhaustive] enum` | Fixed-width integer; consumers need a `_` arm | Add values | None |
| `[Frozen] struct` / `[Frozen] union` | By value, edition layout rules | Fields and cases fixed for the major version | None |
| `string`, `T[]` of eligible `T` | One retained pointer, zero copy | Edition-fixed | Inline retain/release |
| `ReadOnlySpan<T>`, `Span<T>` | Pointer plus length, `in` parameters only | Edition-fixed | None |
| `List<T>`, `Option<T>`, `Result<T, E>`, tuples | Edition-fixed `[Frozen]` std types | Edition-fixed | As their parts |
| Class, non-frozen struct, non-frozen union declared in the interface package | Retained handle; members are accessor calls into the host table | Add members, add cases to `[NonExhaustive]` unions | Indirect call per member; one allocation per non-frozen value type |
| Interface implemented by plugins | Two-word interface value, append-only vtable with `slotCount` | Add members with defaults | Indirect call |
| `[Sealed]` interface (host-implemented) | Same | Add any member | Indirect call |
| Function values | Edition layout (§13.5) | Signature fixed | Indirect call |
| `async` methods | Boxed `Future<T>` as a unique interface value `{poll, drop}` on the host executor | As the method | One allocation per call |
| Open generics, generic methods | Not allowed | n/a | n/a |

The non-frozen value rule is where this design deliberately departs from Swift. Swift passes resilient structs by value, with their size read from metadata at run time. That requires dynamically sized stack slots and metadata-driven copies, which Mora's monomorphizing backend does not have. Mora edition 1 instead offers two honest choices. Freeze the type and pay nothing, or leave it evolvable and pay a handle (one allocation for value types). The author picks per type, and the boundary checker shows the cost in the hover, in line with Mora's "no hidden costs". Objective-C's load-time offset variables and Swift's metadata-sized values remain an option for a later edition if profiles show that handle allocation hurts ([Sealie Software](https://sealiesoftware.com/blog/archive/2009/01/27/objc_explain_Non-fragile_ivars.html); [Library Evolution blog](https://www.swift.org/blog/library-evolution/)).

**Generics** cross only as closed instantiations. Frozen generic types (`Option`, `Result`, `List`, a user's `[Frozen] Pair<A, B>`) can be instantiated on either side, because their layout and bodies are fixed by the edition or the frozen declaration. A non-frozen generic class in the interface package is instantiated **in the host** for every closed instantiation that appears in a boundary signature, and exported through the table. Plugins cannot create new instantiations of it. Generic methods on boundary interfaces are already illegal on dynamic interface values (MR0231), and the boundary keeps that rule. Swift's witness-table approach, which would lift the restriction, is the most expensive piece of Swift's design, and Mora does not need it for plugins.

**Strings and lists** cost nothing extra to cross, because one runtime and one allocator mean that a retained pointer is all the receiver needs. This is the decisive advantage over Wasm components, which copy each returned list element through `cabi_realloc` ([bytecodealliance.org PR #147](https://github.com/bytecodealliance/bytecodealliance.org/pull/147/files)). It also places a real obligation on the edition. `List<T>`'s `{items, count, borrows, version}` layout and its borrow-checking protocol become frozen, and the plugin's inlined copy of `List.Add` must keep honouring them. This is the `@inlinable` bargain the Swift standard library makes for `Array` ([LibraryEvolution.rst](https://github.com/swiftlang/swift/blob/main/docs/LibraryEvolution.rst)). `Dictionary` and `HashSet` should cross only as interface values (`IReadOnlyDictionary<K, V>`) in edition 1 until their layout is worth freezing.

**Async** works because there is one executor. A boundary `async` method returns its cold `Future<T>` boxed behind a two-slot vtable, and the host awaits it like any other future. Cancellation by drop (§12.5) crosses unchanged because `drop` is a vtable slot. This is the same shape as stabby's `DynFuture` workaround and WASI 0.3's `future<T>` ([stabby](https://github.com/ZettaScaleLabs/stabby); [wasi.dev](https://wasi.dev/releases/wasi-p3)). The cost is one allocation per boundary call, stated in the cost model rather than hidden. Today MR0230 forbids converting a move-only value to an interface value, so the boundary needs a compiler-generated exception for futures.

**Errors** cross as `Result<T, E>` values. A `[NonExhaustive] union` such as `PluginError` is boxed as a handle, so new cases do not change any size. The allocation happens only on the error path. A hot-path error can be a `[Frozen]` union, or a `[NonExhaustive] enum` code next to a `string`. Contract `fails` clauses on boundary interfaces become part of the surface, so adding a `fails` case is reported by `mora diff` exactly as the contracts design already specifies. Panics abort, as everywhere in Mora. The runtime's panic handler prints the plugin's package name and version before aborting.

### Loading checks versions by surface history, not by one hash

A single surface hash is too strict for a minor release that only appends, and too weak to say *what* broke. `mora.abi` in a boundary package should therefore carry the edition, the major version, and **a hash for each minor version's surface**. Every minor's surface is a strict superset of the previous one, as `mora publish` already enforces through §18.4. The compiler also records in the plugin the **lowest minor whose surface covers every boundary symbol the plugin actually uses**. That is the compiler-computed counterpart of `NAPI_VERSION` and Godot's `compatibility_minimum`, so a plugin built against 1.5 that uses only 1.2 features loads on a 1.2 host ([Node-API](https://nodejs.org/api/n-api.html); [godot-rust](https://godot-rust.github.io/book/toolchain/compatibility.html)). At load, the host checks five things. The edition matches. The package name and major match. The plugin's needed minor is at most the host's minor, and the two sides agree on that minor's hash. Every imported table entry exists, with a matching signature hash, which is how stabby's per-symbol reports produce precise diagnostics ([stabby](https://github.com/ZettaScaleLabs/stabby)). The plugin's declared capabilities are a subset of the grants. When the hashes match, the per-symbol pass is skipped. When it fails, the error names the symbol.

### What can evolve without breaking plugins

| Change in the interface package (same major) | Compatible? | Mechanism |
|---|---|---|
| Change any function or accessor body of a non-frozen type | Yes | Code lives only in the host |
| Add a type, a free function or a member to a class or struct | Yes | New table entries; old plugins never import them |
| Add a property with a default to a non-frozen struct | Yes | Handle layout private to the host |
| Reorder members in source | Yes | Slot order is publication order, recorded in `mora.abi` |
| Add a member to a plugin-implemented interface **with a default** | Yes | Calls to `slot ≥ slotCount` run the host's default |
| Add any member to a `[Sealed]` interface | Yes | Plugins never implement it |
| Add a case to a `[NonExhaustive]` union or a value to a `[NonExhaustive]` enum | Yes | Boxed union, fixed-width enum, `_` arm required |
| Add an optional trailing parameter, keeping the old overload | Yes | Old signature stays a table entry (§18.4) |
| Weaken a `requires`, strengthen an `ensures`, remove a `fails` case | Yes | `mora diff` contract rules |
| Change the edition of the runtime (for example from 1 to 2) | Host-dependent | A host may support several editions at once |
| Add, remove or reorder fields of a `[Frozen]` type; add or remove `[Frozen]` | No | Layout is compiled into plugins |
| Add a member without a default to a plugin-implemented interface | No | Old plugins have no slot for it |
| Change a signature, remove a member, narrow a `newtype` refinement on an input | No | Signature hash or contract change |
| Change the body of a `[Frozen]` type's method | Yes, but old plugins keep the old copy | Same as Swift `@inlinable` |

The last row deserves emphasis because §18.4 currently says that recompiled generic and inlined bodies "never break the ABI". For plugins, that holds only for frozen types, and a fix to a frozen method reaches a plugin only when the plugin is rebuilt. `mora diff` should warn when a frozen method's body changes.

### Unloading is refused by default and earned by reloadable plugins

The default is the PostgreSQL and musl stance: a loaded plugin stays mapped for the life of the process. Hot reload is opt-in. A plugin declares `[plugin] reloadable = true`, and the toolchain and runtime then make unloading a checkable guarantee rather than a hope. The runtime keeps a per-plugin live count, incremented when an object whose `meta` or vtable lives in the plugin image is allocated or converted to an interface value, and when the plugin spawns a task or hands out a closure. `Plugin.Unload()` returns `Err(UnloadError.Live(count, firstTypeName))` unless the count is zero, so leaks are reported, not crashed into. For reloadable plugins, the loader copies the plugin's immortal literals into a host-owned immortal arena at load. A string literal the host keeps therefore survives the unload, and each reload leaks a few kilobytes at most. The host also records every callback the plugin registers (for example a `Subscription`) and revokes them before `dlclose`, as Node-API's cleanup hooks do ([Node-API](https://nodejs.org/api/n-api.html)). A reloadable plugin may not declare `static mut` state that the host can reach. On musl, `dlclose` is a no-op, so a reload maps a fresh image and the old one stays resident ([musl](https://wiki.musl-libc.org/functional-differences-from-glibc.html)). State migration across a reload is left to the application, with an optional `ISnapshot` interface in the boundary package.

### Security: native plugins are trusted code, and the toolchain says so

None of the native hosts surveyed sandbox anything. PostgreSQL's magic block, Python's ABI check and Unreal's BuildId detect incompatibility, not malice ([PostgreSQL](https://www.postgresql.org/docs/current/xfunc-c.html); [Python docs](https://docs.python.org/3/c-api/stable.html); [Epic](https://dev.epicgames.com/documentation/unreal-engine/how-to-version-binaries-in-unreal-engine)). A native Mora plugin runs with the host's full privileges, and `Plugin.Load` should require the host to hold the `ffi` capability that §18.6 defines as "trust fully". Capabilities and inferred effects still earn their place as **policy**. The plugin's `mora.caps` and its inferred effect set are recorded and signed with the package (§18.7), and the loader refuses a plugin whose declared capabilities exceed the grant. That catches the JSON parser that suddenly wants the network, and gives operators an auditable manifest. It does not stop a hostile binary that lies about its capabilities, and the documentation should say exactly that. **Untrusted code takes the other transport.** The same interface package compiles with `--emit component` to WIT, and `Plugin.LoadComponent<IFilterPlugin>("blur.wasm", grants)` returns the same interface type, backed by Wasmtime. Grants become linked imports, and instruction or memory budgets in the style of Shopify become possible ([Shopify](https://shopify.dev/docs/apps/build/functions/optimize-instruction-counts)). Zed and HashiCorp each had to pick one transport for their whole ecosystem ([Zed](https://zed.dev/blog/zed-decoded-extensions); [go-plugin](https://github.com/hashicorp/go-plugin)). Mora lets the host choose per plugin, from one interface definition. In debug builds, the host-side shim of each boundary call also checks the interface's `ensures` clauses and the `newtype` refinements of values a plugin returns. That turns contracts into plugin-conformance tests at no release-build cost.

### Cost model

| Operation | Whole-program Mora today | Across the native boundary | Across a Wasm component |
|---|---|---|---|
| Method call on boundary interface | Direct or inlined | One indirect call, no inlining | Canonical ABI lift/lower; ~3.5x sync overhead reported ([BA PR #147](https://github.com/bytecodealliance/bytecodealliance.org/pull/147/files)) |
| Retain/release | Inline, immortal-bit check | Inline, mask load plus check | Not shared; values are copied |
| Allocation | Direct runtime call | Load-time-bound indirect call | Callee's `cabi_realloc` |
| Pass `string` or `List<T>` | Pointer | Pointer (zero copy) | Copy per element |
| Field of `[Frozen]` struct | Load | Load | Copied with the value |
| Property of non-frozen struct or class | Load, usually | Indirect accessor call | Copied or resource call |
| Non-frozen value type, `[NonExhaustive]` union | Inline value | One allocation | Copied |
| `async` boundary call | State machine inline in caller | One boxed future allocation, indirect `poll` | Component-model async |
| Load | n/a | `dlopen` plus table fill plus hash compare | Compile or instantiate module |
| Build | Whole program | Plugin whole-program internally; interface compiled once in host | Separate component |
| Crash isolation | n/a | None: a plugin panic aborts the host | Trap contained |

Two numbers are missing from the evidence and should be measured during implementation. Swift publishes no measurement of resilience overhead ([SE-0260](https://github.com/swiftlang/swift-evolution/blob/main/proposals/0260-library-evolution.md)), and no source found compares COM-style vtable calls with direct calls. The design's claim rests on structure: a fragile build pays nothing, frozen types pay nothing, and every other cost is one indirect call or one allocation, visible at the declaration.

## Seven alternatives, ranked against Mora's principles

| Rank | Alternative | Strength | Fatal or limiting cost |
|---|---|---|---|
| 1 | **Opt-in boundary package with a published ABI edition (this report)** | Zero cost outside the boundary; rich types; compiler-independent plugins; kernel-capable; pairs with Wasm components | Edition document and runtime table must be designed carefully before edition 1 ships |
| 2 | Rank 1 plus Swift-style resilient value types (metadata-sized values, load-time offsets) | No allocation for evolvable structs | Needs dynamic stack slots and value witness machinery that monomorphization avoided; defer to a later edition if profiling demands it |
| 3 | Wasm components only for plugins | Sandbox, unloading, budgets, language-neutral; already in §18.3 | Copying and ~3.5x sync overhead; unsuitable for audio, engines, kernels |
| 4 | COM/VST3-style immutable interfaces (`IFoo2`), no frozen values | Proven for decades; trivial checking | Poor ergonomics; every new member means a new interface; strings and lists need custom types |
| 5 | Process plugins as a `Std.Plugins.Remote` library (go-plugin model) | Crash isolation, any language | Serialization per call; better as a library than a language feature |
| 6 | Richer C ABI only (crABI-style `[Export]` with `string`, `Option`, `Result`) | Small, useful for C interop | No interfaces, no versioning, no load checks; still "everything through C" |
| 7 | Swift-style library evolution for every public API, or toolchain-locked dylibs (Go `plugin`, Unreal BuildId) | Maximum flexibility, or minimum design work | The first makes the whole language pay for plugins; the second recreates Go's "built together by a single person" rule |

**The proposal sketch survives with six changes.** "Opt-in boundary only" is correct and is the single most important decision. It matches Swift's off-by-default mode and avoids C++'s implicit ABI. "One shared runtime for ARC and allocation" is correct, but it should be a versioned function table carrying `Std`'s process-unique state, with inline retain and release kept fast through a published slow-path mask instead of a frozen header. "Classes and interfaces as opaque handles with vtables" is correct, with three additions: a slot count, append-only slot order recorded in `mora.abi`, and host-supplied defaults, so that adding an interface member with a default is compatible. `[Sealed]` interfaces make host-side growth free. "`[Frozen]` structs with fixed layout" is correct. The missing half is the rule that non-frozen boundary types are handles whose code lives only in the host. "ABI hash checked at load" should become per-minor surface hashes, a compiler-computed minimum minor and per-symbol signature hashes for diagnostics. "Panics do not cross" is already guaranteed by §11.4's abort, and the remaining work is attribution in the panic message. The sketch also needs three things it did not mention: the edition as a document independent of the compiler, an unloading stance, and one interface package serving two transports.

**Milestones.** The Objective-C and C++ histories argue for writing the specification before the code, because the fragile-versus-resilient decision cannot be retrofitted ([Sealie Software](https://sealiesoftware.com/blog/archive/2009/01/27/objc_explain_Non-fragile_ivars.html); [P1863R1](https://www.open-std.org/jtc1/sc22/wg21/docs/papers/2020/p1863r1.pdf)). The sequence fits after the library-format milestones (M8 and M9), because it depends on `mora.meta` and `mora.abi`.

| Step | Deliverable | Exit criterion |
|---|---|---|
| P1 | Spec: ABI edition 1 document, boundary-eligibility rules, `mora diff` boundary rules, §18.4 amendment on frozen bodies | Reviewed spec section; `mora diff` reports every row of the evolution table |
| P2 | Runtime: `libmora_rt` usable as one shared instance; `MoraRuntimeV1` table; `Std` global state moved behind it; `SlowMask` retain/release; Cranelift PIC shared-library output | A host and a plugin built by different compiler versions exchange strings and lists |
| P3 | Boundary codegen: `[Frozen]`, handles with host-side accessors, append-only vtables with `slotCount`, `[Sealed]`, closed generic instantiations, `Plugin.Load` with all five checks, `mora abi header` | Example plugin above loads; adding `Describe()` with a default keeps an old plugin working |
| P4 | Async futures across the boundary, boxed `[NonExhaustive]` unions, debug-mode contract and refinement checks, panic attribution | Async filter round trip; contract violation reported in debug |
| P5 | Reloadable plugins: live counts, literal arena, callback revocation | Reload loop of 10,000 iterations with bounded memory; `Unload` refuses with a named leak |
| P6 | Transport choice: `Plugin.LoadComponent` over the same interface package; send/sync rules once threads land (§12.6) | Same host code runs a native and a Wasm build of one plugin |

**Open questions.** Several decisions remain genuinely open. How much of the 16-byte header to freeze: the proposal freezes offsets and the mask protocol, but whether `weak` must be inline-accessible is undecided. Whether freezing `List<T>`'s borrow and version fields is acceptable, or whether lists should cross as an interface in edition 1 and be frozen in edition 2. How non-atomic counts in the MVP interact with a plugin that later runs on a worker thread: the boundary should probably require `shared` (atomic) types for anything a `[Sendable]` interface passes, before threads ship. Whether a panic in a plugin should be able to end only the calling task, which ties into OQ-1 and would need the runtime to support plugin-scoped abort. Whether type ids should include the interface package's major version, so that the same name under a different layout never compares equal. Whether plugins may depend on other plugins' interface packages, or only on the host's. And whether native plugin binaries from the registry should be reproducibly built from signed source, so that the capability manifest can be trusted, not merely declared.

## Conclusion

The question framed Rust's problem as "everything must go through the C ABI". The survey shows that the C calling convention was never the obstacle. Mora's §13.7 already uses it, and Node-API, Vulkan and COM all sit on top of it. The obstacle is missing *semantics* at the boundary: no shared allocator, no owned strings, no interfaces with lifetimes, no versioning and no load-time negotiation. Mora is unusually well placed to supply them. ARC already gives cross-binary lifetimes, panics already abort instead of unwinding, errors are already values, `mora.abi` and `mora diff` already enforce semver, and the Wasm component path already exists for untrusted code. What remains is to publish one small, versioned document (the edition) and to confine its costs to the packages that ask for them.

The deeper insight is that "stable ABI" and "whole-program optimization" are not in tension once the boundary is a declared package rather than a property of the language. C++ and CPython lost performance because their ABI was everywhere and implicit. Go lost its plugin ecosystem because its ABI was nowhere. Swift got it right but had to build heavy runtime machinery, because it made resilience the default *inside* the boundary. Mora can make freezing explicit and handles visible instead, and let one interface definition run natively in-process or sandboxed in Wasm. That would make it the first language where writing a fast, independently versioned native plugin is as ordinary as writing a library, while untrusted plugins remain a one-flag change away.
