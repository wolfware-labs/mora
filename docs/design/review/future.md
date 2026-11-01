# Mora for the next decade: a forward-looking design study

> **Status.** This study was written before most of the decisions listed in the [review README](README.md), and its proposals were decided item by item on 2026-10-31. Read it through those decisions: where it disagrees with [the specification](../mora.md) or its decisions register, the specification wins.

Reviewed: `docs/design/mora.md` (v0.1 draft, 2302 lines), in particular §1, §2, §5 to §12, §14 to §18, §20, §22 to §24. It builds on the companion review [the inherited-debt study](debt.md) and does not repeat it. Every code block in this study assumes all of that review's **Change** verdicts at once: arm-style `switch (x) { p => ... }`, `var T x` / `let T x` locals, fields instead of auto-properties, arrow function types `(A) -> R`, `any I` for boxes, `[Error]` with `[From]`, and option-returning lookups.

The brief: fix patterns that every mainstream language still handles badly, and make Mora a good language for a world where much code is written, read, reviewed and run together with AI agents. Constraints: a small team must be able to build it on Mora's architecture (salsa-style incremental compiler, MIR, ARC with unique ownership, sandboxed JIT macros, Cranelift), it must not cost run-time speed or day-one learnability, and builds stay deterministic.

---

## Executive summary: the ideas that would most make Mora "the language for the future"

They are ranked by impact divided by cost. Most of them reuse one mechanism: **effect inference**, meaning that the compiler knows which capabilities each function can use (#1). Fourteen of the proposals depend on it.

1. **Effect inference with optional `uses` bounds** (#1, M, MVP). The compiler infers which capabilities each function can reach (`fs.read`, `net.outbound`, `clock`, `random`, ...) from the `[RequiresCapability]` tags std already has (§6.11, §18.6). Nobody has to write annotations, and an optional `uses none` bound is checked. The signature then answers "can this touch the network?" with no syntax tax on day one.
2. **Target-typed interpolation handlers** (#2, S-M, MVP). `$"..."` assigned to `Sql`, `Html`, `Command`, `Url` or a log call builds that type: literal parts are trusted, holes are typed parameters, and `string` never converts to `Sql`. SQL, HTML and shell injection stop compiling, at zero run-time cost.
3. **Built for models that have never seen Mora** (#3, S, M7). A new language has almost no training data, so agents write C#. Mora should ship machine-applicable fixes for C# habits, a compact `mora docs --llm` reference, and a measured "first-try compile rate" budget beside §20.6. This is the cheapest high-impact item in the study.
4. **A swappable World and deterministic simulation testing** (#4, M, runtime hook in M6, full simulator in v0.2). Clock, random, network, files and scheduling all go through the runtime's platform layer (D-50) and one executor (D-36). Swapping that layer in test builds gives FoundationDB-style simulation, with seeds and fault injection, to every Mora app. Release binaries are unchanged.
5. **Agent-grade toolchain** (#5, M, M7 then v0.2). `mora query`, `mora fix`, `mora context` and `mora mcp` serve the compiler's own semantic database as stable JSON. Agents get types, effects, contracts and fixes from the compiler instead of guessing from text.
6. **Newtypes and pattern refinements** (#6, M, MVP for `newtype`). `public newtype Port : int is >= 1 and <= 65535;` reuses §7.8 patterns as refinements. Literals are checked at compile time, `Port.From(x)` returns `Port?`, JSON validates on input, and JSON Schema and test generators get the bounds for free. It ends "is this `long` a user id or an order id?".
7. **Contracts in a `contract` section** (#7, M; `contract` reserved for MVP, feature in v0.2). Each type lists `invariant`, `requires`, `fails with E when` and `ensures` clauses per member, linked by name, so signatures stay clean. Docs, hover, tool schemas, `mora diff` and the property tester read the same metadata.
8. **Property tests derived from types and contracts** (#8, M, v0.2). A `[Test]` with parameters gets generated, shrunk and seed-reproducible inputs. `mora test --contracts` fuzzes every member with a `contract` entry. Unit tests live in `test { }` sections next to the code, compiled only under `mora test`. This is a cheap and strong check on generated code.
9. **Semantic diff and compiler-enforced semver** (#9, M, M9). `mora diff main..HEAD` reports API, effect, capability, contract and JSON-schema changes. `mora publish` refuses a version number that understates the change, as Elm does. Reviewing an agent's change starts with "this adds `net.outbound` to `Pricing`".
10. **Static capability enforcement and sandboxed runs** (#10, M, M9). §18.6 grants become a static check built on #1. Authority flows through the objects a dependency is handed (object capabilities), and `mora run --sandbox` refuses to build generated scripts that exceed their grants.
11. **Time done right** (#11, M, M5 types, tz data in v0.2). Replace `DateTime` with `Instant`, `LocalDate`, `LocalDateTime`, `ZonedDateTime` and `TimeZone`, make the handling of DST gaps and overlaps explicit, and separate monotonic from wall time. This is JS Temporal, java.time, NodaTime and jiff, made the default from v0.1.
12. **Structured concurrency by default, with inherited deadlines** (#12, S-M, M6). Detached spawning becomes explicit and linted. Scopes cancel siblings when one child fails. Deadlines flow through the task tree. `blocking-in-async` comes for free from effect inference.

Next tier: `Secret<T>`, which uses deterministic destructors to zero memory reliably, something no GC language can promise (#13). Structured logging and tracing built on #2 and destructor-scoped spans (#14). Errors with separate user and developer messages (#15). Sound test caching (#18).

What Mora should **not** do: algebraic effect handlers, AI or any non-determinism inside the compiler, LLM client types in std, content-addressed source storage, stackful green threads to cure function colouring, and any annotation burden on day one (see "Coherence").

---

## All proposals

| # | Proposal | Verdict | Confidence | Cost |
|---|---|---|---|---|
| 1 | Effect inference with optional `uses` bounds | Adopt for MVP | High | M |
| 2 | Target-typed interpolation handlers (`Sql`, `Html`, `Command`, `Url`, log templates) | Adopt for MVP | High | S-M |
| 3 | Built for models that have never seen Mora (C#-habit fixes, `mora docs --llm`, first-try budget) | Adopt for MVP | High | S |
| 4 | Swappable World and deterministic simulation testing | Adopt for MVP (hook) / post-MVP (simulator) | High | M |
| 5 | Agent-grade toolchain: `mora query`, `fix`, `context`, `mcp` | Adopt for MVP (JSON, fix) / post-MVP (mcp) | High | M |
| 6 | Newtypes and pattern refinements | Adopt for MVP (`newtype`) / post-MVP (`where` predicates) | Medium-high | M |
| 7 | Contracts: a `contract` section per type (`requires`, `fails`, `ensures`, `invariant`) | Adopt post-MVP (reserve `contract` in MVP) | Medium-high | M |
| 8 | Property tests derived from types and contracts | Adopt post-MVP | High | M |
| 9 | Semantic diff and compiler-enforced semver | Adopt post-MVP | High | M |
| 10 | Static capability enforcement, narrowed grants, sandboxed runs | Adopt post-MVP | High | M |
| 11 | Time done right (`Instant`, `ZonedDateTime`, `LocalDate`, tzdb) | Adopt for MVP (types) / post-MVP (zones) | High | M |
| 12 | Structured concurrency by default, inherited deadlines, `blocking-in-async` | Adopt for MVP | High | S-M |
| 13 | `Secret<T>`: redacted, non-serializable, zeroed on drop | Adopt for MVP | High | S |
| 14 | Structured logging and tracing via interpolation and destructor spans | Adopt for MVP (core) | Medium-high | M |
| 15 | Errors for humans and machines: codes, public vs internal messages, `Report` and `Context` | Adopt for MVP (partial) | Medium-high | S-M |
| 16 | Typed configuration with `[Config]` | Adopt post-MVP | Medium | S-M |
| 17 | Retry policies, transient errors and cancel-safety (`Task.Shield`) | Adopt post-MVP | Medium | M |
| 18 | Sound test caching and flaky-test triage from effects | Adopt post-MVP | Medium | S-M |
| 19 | Checked doc examples in user code and snapshot assertions | Adopt for MVP | High | S |
| 20 | JSON Schema from types; `[Tool]` signatures as MCP tools (package) | Adopt post-MVP | Medium-high | S-M |
| 21 | Compile-time evaluation of effect-free functions | Adopt post-MVP | Medium | M |
| 22 | Editions and `mora migrate` | Adopt post-MVP (policy before v0.1) | High | S-M |
| 23 | Grammar and locality audit (one parse, visible implicits) | Adopt for MVP | Medium | S |
| 24 | Units of measure (`newtype` arithmetic, then full unit algebra) | Adopt post-MVP | Medium | M-L |
| 25 | `decimal` and currency-safe money | Adopt post-MVP | Medium | M |
| 26 | Conditionally async higher-order functions (`async?`) | Adopt post-MVP | Medium | M |
| 27 | Panic containment through supervised worker processes (framework option; thread-ready runtime) | Adopt post-MVP | Medium | M |
| 28 | Hot reload of function bodies in dev builds | Adopt post-MVP | Medium | M |
| 29 | Record and replay (`mora run --record` / `mora replay`; production flight recorder later) | Adopt post-MVP (stage 1) / Explore (stage 2) | Medium | M / L |
| 30 | Algebraic effect handlers | Reject | High | L |
| 31 | Content-addressed code storage (Unison model) | Reject | High | L |
| 32 | LLM client, prompt and cost-budget types in the language or std | Reject | High | M |
| 33 | AI or other non-determinism inside the compiler or `mora fix` | Reject | High | n/a |
| 34 | Database migrations and feature flags as language features | Reject | Medium-high | n/a |
| 35 | Stackful green threads to remove async colouring | Reject | Medium-high | L |
| 36 | Stable plugin ABI at an opt-in boundary (`[abi] stable`, `[Frozen]`, native + Wasm) | Adopt post-MVP | Medium-high | L |

---

## Detailed proposals

### 1. Effect inference with optional `uses` bounds

**The problem.** No mainstream language can answer "can this function touch the network, the disk or the clock?" from its signature. The answer decides whether a function is safe to call from a test, to cache, to run at compile time, to give to an AI agent as a read-only tool, or to accept from a dependency update.

```csharp
// C#: nothing in the signature says this reads the clock and calls a remote service
public decimal Price(Order o) => _rates.Convert(o.Total, DateTime.UtcNow);
```

```rust
// Rust: same. `fn price(o: &Order) -> Decimal` may open sockets; only reading the body tells you.
```

```go
// Go: context.Context in a signature hints at I/O, but nothing enforces it.
```

The academic answer is effect systems (Koka, Eff, Unison abilities, OCaml 5 handlers) and capability passing (Austral, Pony, Scala 3 capture checking, which is still experimental in 2025, and Zig 0.16, where `std.Io` became an explicit parameter in 2025-2026). Each of these either makes everyone write effect annotations and effect polymorphism, or threads a capability parameter through every call. Both cost day-one learnability, which Mora's anti-goals forbid (§1.4: "Rust with different syntax").

**The Mora design.** Effects are an **inferred analysis with optional upper bounds**. They are not part of the type system that users must spell.

- **The effect vocabulary** is §18.6's capability list plus two analysis facts: `fs.read`, `fs.write`, `net.outbound`, `net.listen`, `env`, `clock`, `random`, `process`, `console`, `ffi`, and the facts `blocking` (the call blocks the executor thread, §12.1) and `dyn` (the call reaches code the compiler cannot see statically). The groups `fs` and `net` name both halves. An effect set is a 16-bit bitset.
- **Sources.** Std functions already carry `[RequiresCapability("...")]` (§6.11). That is the only place effects originate, apart from `extern`/FFI, which is `ffi`.
- **Inference.** The effect set of a function is the union of three things: the effects of the code written lexically inside it, including the lambdas it creates; the effects of its statically resolved callees, computed per strongly connected component of the call graph; and the effects of the closures it receives through `in` parameters, which are known at the call site because non-escaping closures are views (§5.8, §10.9). Generic functions are inferred per instantiation after monomorphization (§14.10), so `list.Sort(cmp)` costs exactly what `cmp` costs.
- **Dynamic calls.** A call through an escaping function value or an `any I` box adds `dyn`, unless the function type or interface method declares a bound. An interface method may write `uses none`, and every implementation is checked against it. A function type may write `(in T, in T) -> int uses none`.
- **Lexical attribution.** An effect is charged to the function, and so to the package, whose code performs it. A closure created in `App` and passed into `Lib` carries `App`'s effects, not `Lib`'s. This is what makes the package-level capability check (#10) sound without effect polymorphism: authority travels with objects and closures (object capabilities), never with the call stack.
- **Bounds.** An optional `uses` clause after the parameter list is an upper bound, checked against the inferred set. `uses none` means no capability. It does not mean "no mutation", "no allocation" or "no panic": those are separate questions, and conflating them is what makes `pure` keywords disappointing elsewhere.

```mora
public class PriceService
{
    private RateClient _rates;

    // No annotation needed. Hover shows: uses net.outbound, clock
    public async Result<int, PricingError> PriceCentsAsync(Order o)
    {
        let rate = await _rates.GetAsync(o.Currency, Clock.Now())?;
        return Pricing.Apply(o.TotalCents, rate);
    }
}

public static class Pricing
{
    // A checked bound: adding Clock.Now() here is error MR0250
    // "Apply is declared `uses none` but calls Clock.Now(), which uses clock".
    public static int Apply(int cents, Rate rate) uses none => cents * rate.Numerator / rate.Denominator;
}

public interface IComparer<T>
{
    int Compare(in T a, in T b) uses none;      // every implementation is checked
}

// A function type with a bound: a sort comparator can never do I/O
public void Sort((in T, in T) -> int uses none compare) { ... }
```

Where effects show up, with no syntax required from the user:

- LSP hover and an optional inlay hint after the signature: `uses net.outbound, clock`.
- `mora query effects UsersApi.PriceService.PriceCentsAsync --why`, which prints the call path that introduces each effect (`clock ← Clock.Now() at src/Price.mr:12`).
- Lint `blocking-in-async` (warn by default): an `async` function whose inferred set contains `blocking`. This fixes a real §12.1 footgun ("a blocking call blocks every task"), and it is transitive: it finds `File.ReadAllText` three helpers down.
- The macro sandbox of §9.7 becomes "the macro's effect set is `none`". `MR0613` already says exactly that, so the analysis reuses a check the spec needs anyway.
- Contracts (#7), compile-time evaluation (#21), test caching (#18), tool hints (#20), semantic diff (#9) and capability grants (#10) all consume the same sets.

**Prior art and how Mora differs.** Koka and Unison track effects in types, with row polymorphism and handlers. Mora infers them and has no handlers (see #30). Roc separates pure code from platform effects by architecture. Mora gets the same separation from inference plus the World swap (#4). Austral and Zig 0.16 pass capabilities as parameters, so they are explicit everywhere. Mora is implicit by default and checked only where a bound is written. Scala capture checking tracks capabilities captured in closures, which is close to Mora's lexical attribution, but it is a type-system feature with annotations. Java's checked exceptions are the cautionary tale: a mandatory, non-polymorphic effect annotation that people learned to hate. Mora never requires one. Swift's `@Sendable` and actor isolation show that inferred-then-diagnosed works for application developers.

**Impact.**
- Speed: none at run time. At compile time, a bitset union per call edge and a fixpoint per SCC, cached per function in salsa (§14.3). This is negligible next to type checking.
- DevX: invisible until wanted, then a one-hover answer. `blocking-in-async` alone prevents a class of production stalls.
- AI era: agents and reviewers get a truthful, machine-readable summary of what code can do, which is the basis for #9, #10 and #20.
- Cost: M. A `mora-hir` query over MIR call edges, the `uses` clause in the grammar, two diagnostics, and an LSP hover.
- Milestone: reserve `uses` in M1.5. Inference lands with the std tags in M5 (it needs calls resolved, M3, and tagged std, M5). The `uses` bound check and the lint land in M5/M6, hover in M7.

**Risks.** `dyn` noise around boxed interfaces and stored closures: interface bounds and the debt report's "bare interface is not a box" change reduce it. Users may read `uses none` as "pure", so the docs and the hover text must say "uses no capabilities". Static mutable function pointers set at run time are the same as `dyn`. FFI is `ffi` and means "trust fully", as §18.6 already says.

**Verdict: Adopt for MVP (inference, `uses` bounds, `blocking-in-async`). Confidence high.**

---

### 2. Target-typed interpolation handlers

**The problem.** Injection is still in the OWASP top three, because the easiest way to build a query is string concatenation.

```python
cur.execute(f"select * from users where name = '{name}'")    # injection
# Python 3.14 t-strings (PEP 750) return a Template, but str + Template and string-built
# templates remain footguns, and safety depends on each library.
```

```csharp
// C#: FormattableString / interpolated string handlers exist, but `string` converts silently,
// and EF Core needed a separate FromSqlRaw vs FromSqlInterpolated API to cope.
db.Users.FromSqlRaw($"select * from users where name = '{name}'");   // compiles, injectable
```

```go
exec.Command("sh", "-c", "git log "+branch)   // shell injection
```

Java's string templates (JEP 430/459) were withdrawn in 2024, and JavaScript tagged templates are untyped.

**The Mora design.** Interpolation is already target-sensitive: §7.13 lowers `$"..."` straight into a `ref Formatter`. Generalize that rule into one interface.

```mora
public interface IInterpolate<TSelf>
{
    // Called first. Returning none skips every hole: none of them is evaluated (used by logging).
    static abstract TSelf? Begin(int literalBytes, int holeCount);
    mut void AppendLiteral(Literal text);                       // only from source text
    // Each hole: its value, its format spec, and its source text (for log field names)
    mut void AppendHole<T>(in T value, Literal format, Literal source) where T : IHole<TSelf>;
    // [post-MVP] Run at compile time on the literal parts; an Err is a compile error at the string
    static virtual Result<(), string> Check(ReadOnlySpan<Literal> parts) uses none => Ok(());
}
```

- `Literal` is a std type that only the compiler can create, from source literals and `const` strings. No API turns a run-time `string` into a `Literal`. This single rule is what makes injection unrepresentable.
- When the target type of `$"..."` implements `IInterpolate<T>`, the compiler emits the `Begin` / `AppendLiteral` / `AppendHole` calls. The calls are statically dispatched and inlinable, as C#'s interpolated string handlers are. With no target type, the result is `string`, as today.
- Each hole must satisfy the handler's constraint: `IHole<Sql>` is implemented for the SQL parameter types and for `Sql` itself, so sub-queries compose. A hole of type `User` is a compile error that names the missing implementation.
- There is no implicit conversion from `string` to `Sql`, `Html` or `Command`. `Sql.Unchecked(string)` exists, is linted (`unchecked-injection`, warn by default), and shows up in the semantic diff (#9).

```mora
public async Result<User?, DbError> FindByNameAsync(string name)
{
    // Becomes "select id, name from users where name = $1" with parameter [name]
    return await _db.QuerySingleAsync<User>($"select id, name from users where name = {name}");
}

let page = Html.Render($"<p class=\"greeting\">Hello, {user.Name}</p>");   // escaped for the context
var Command cmd = $"git log --format=%H {branch}";                       // argv: ["git","log","--format=%H", branch]
let url = Url.Of($"https://api.example.com/users/{id}/orders?status={status}"); // per-component percent-encoding
Log.Info($"created user {user.Id} from {req.Path}");                      // structured fields user.Id, req.Path (#14)

_db.Query(sqlText);   // error MR0254: `string` does not convert to `Sql`
                      // help: write the query as $"..." so values become parameters
```

**Context awareness.** `Html` tracks a small tokenizer state across literal parts, so it knows whether a hole sits in text, in a quoted attribute, or in a URL attribute. It escapes accordingly and refuses holes inside `<script>` or unquoted attributes. Go's `html/template` does this at run time with reflection. Mora does it with plain code, and post-MVP at compile time through `Check`, which is effect-free and runs on the macro JIT (#21). `Command` splits literal text on whitespace and turns each hole into exactly one argv element, never parsed by a shell.

**Prior art and how Mora differs.** C# 10's interpolated string handlers supply the mechanism, but C# lets `string` flow anywhere and has no `Literal`. Python t-strings give a runtime `Template` with no typed holes. Scala's string interpolators and JS tagged templates are untyped. Go's `html/template` has the context awareness but no typing. Rust's `sqlx::query!` checks queries at compile time with a macro and a live database. Mora's `Check` can grow toward that with file-reading macros (§9.10). Mora is the first to combine target typing, typed holes, an unforgeable literal type and compile-time template checks.

**Impact.**
- Speed: zero cost. It is often faster than building a string, because SQL parameters and argv never get escaped and re-parsed.
- DevX: the safe way is the shortest way.
- AI era: models write `$"select ... {x}"` by reflex. In Mora that reflex is safe, and the unsafe variant does not compile.
- Cost: S for the protocol (it generalizes §7.13 in M3) and M for the `Html`/`Url` types in std (M5). `Sql` and `Command` arrive with a DB package and with `Process`, both post-MVP.
- Milestone: protocol and `Literal` in M3. `Url`, `Html` and log templates in M5. `Check` with #21.

**Risks.** The rule "the target type decides" must be explained once in the tour. Lazy holes, where `Begin` returns `none`, change evaluation, as in C#: holes with side effects are linted. Getting the `Html` context machine right is real work: start with text and quoted attributes only.

**Verdict: Adopt for MVP. Confidence high.**

---

### 3. Built for models that have never seen Mora

**The problem.** A language launched in 2026 or later starts with almost no code in any model's training data. Agents then write the closest language they know, which for Mora is C#. Gleam, Roc and Mojo all hit this: models confidently produce syntax from a neighbouring language. Most new languages respond with documentation alone.

**The Mora design.** Treat "an agent writes correct Mora on the first attempt" as a product requirement with a budget.

1. **The compiler as tutor.** The MR01xx family already turns C# constructs into targeted errors (§3.4, §6.9, §7.12). Make every one of them carry a machine-applicable fix (applicability `safe`, see #5), and grow the list from evidence: log the C#-isms that agents produce on the eval suite (item 4 of this list), and add a fix for every one that appears more than a threshold number of times. The debt report's "one way" rule (its #2) is what makes this work: each shim becomes an error with an automatic fix rather than a silent alternative.
2. **`mora docs --llm`.** A deterministic, versioned, compact reference: the grammar summary, the "differences from C#" table, the std signature index with one-line docs, effects, and the error-code catalogue with the before and after of each fix. It is published at `mora-lang.dev/llms.txt` and embedded in the toolchain, so offline agents can use it, and it is generated from the same sources as the API reference (§15), so it cannot drift.
3. **Error pages as paired examples.** `docs/errors/MRnnnn.md` already holds a failing and a fixed example compiled in CI (§17.6, §20.4). Publish them as a dataset, so they are the paired examples that teach a model what to change.
4. **An agent eval suite as a budget.** `mora-evals` holds 100 to 200 small tasks (an endpoint, a parser, a union refactor), each with hidden tests. Every release measures "compiles on the first try" and "passes tests within three compile-fix rounds" for two or three popular models, plus a small local one. These numbers sit beside §20.6. A regression of more than 10% needs an explanation, as the `bench` check does.
5. **Familiar where it costs nothing.** This argues for the debt report's approach (C# spelling unless the model differs) and against novelty in syntax. Every novel construct proposed in this study (`uses`, `requires`, `newtype`) reads as English and appears only where the user opts in.

**Prior art.** Rust's `rustfix` with machine-applicable suggestions; the llms.txt convention (2024); Elm's error messages. Nobody runs model success as a release budget.

**Impact.**
- Speed: none.
- DevX: humans coming from C# get the same fixes.
- AI era: decisive for adoption. Agent-written code is a large share of new code, and a language that agents fail at will not be chosen.
- Cost: S. Most of the parts exist (error pages, JSON fixes, API reference); the eval harness is about a week of work.
- Milestone: M7, which already contains `mora explain`, error pages and the documentation site.

**Risks.** Evals measure models that change, so freeze model versions per release line. Do not distort the language to suit today's models: the budget informs fixes and docs, not syntax.

**Verdict: Adopt for MVP. Confidence high.**

---

### 4. A swappable World and deterministic simulation testing

**The problem.** Tests that touch time, randomness, concurrency or the network are flaky, and the bugs that matter most (timeouts, retries, partitions, reordering, crash recovery) are the ones nobody tests. FoundationDB and TigerBeetle showed that deterministic simulation finds them, but doing it today means building your whole system around it. Rust has madsim and turmoil (with mad-turmoil adding libc overrides in 2025), whose authors still fight seeding for `HashMap` and dependencies that bypass the simulator. Antithesis sells it as a hypervisor.

```csharp
// C#: every team writes its own IClock / ISystemClock, then forgets one DateTime.UtcNow.
await Task.Delay(TimeSpan.FromMinutes(5));   // the test takes five minutes, or gets skipped
```

**Why Mora can do it cheaply.** Three facts in the spec make this nearly free for Mora:

- All OS access already goes through one runtime platform interface (D-50, §13.1, §19.4).
- MVP execution runs on one thread under one executor (D-36, §12.1), so scheduling is just a decision about which ready task runs next.
- Dictionaries iterate in insertion order (D-29), and the macro sandbox already keeps builds deterministic.

**The Mora design.**

- **World.** The platform layer is the *World*: clock, monotonic time, timers, random, sockets, files, environment, the console and the task scheduler's choice of the next ready task. Production binaries link the real World with direct calls, which are devirtualized at link time, so release code pays nothing. Dev and test builds route the same calls through a task-local `World` pointer. That costs one load and an indirect call next to a syscall.
- **Simulated World** (`Std.Testing.Sim`): a virtual clock that jumps to the next timer, so `Task.Delay(5 min)` takes microseconds; seeded random; an in-memory network with per-link latency, loss, reordering and partitions; an in-memory file system with crash semantics (unsynced writes lost); and a seeded scheduler that picks among ready tasks. Several *hosts* in one process each get their own addresses, files and clock skew.
- **Tests.** A `[Test(World = Sim)]` function may take a `Sim` parameter. `Seeds = 1000` runs it under 1000 seeds. A failure prints the seed and `mora test --seed 0x9E37...` reproduces it exactly.

```mora
[Test(World = Sim, Seeds = 1000)]
async Result<(), TestError> LeaseIsNeverHeldTwice(Sim sim)
{
    let a = sim.Host("10.0.0.1", async () => await LeaseNode.RunAsync("10.0.0.1:7000"));
    let b = sim.Host("10.0.0.2", async () => await LeaseNode.RunAsync("10.0.0.2:7000"));

    await sim.RunFor(TimeSpan.FromSeconds(30));
    sim.Network.Partition("10.0.0.1", "10.0.0.2");
    sim.Clock.Skew("10.0.0.2", TimeSpan.FromMilliseconds(400));
    await sim.RunFor(TimeSpan.FromSeconds(60));
    sim.Host("10.0.0.1").Crash();                    // drops unsynced writes, restarts later
    await sim.RunFor(TimeSpan.FromSeconds(60));

    Assert.True(sim.History.Count(e => e is LeaseGranted { Overlapping: true }) == 0);
    return Ok(());
}
```

- **Plain tests become deterministic too.** Under `mora test`, the real World still seeds `Random` (and prints the seed on failure). Every async test runs with a seeded scheduler, so interleavings are reproducible even without `Sim`.
- **The §2.2 acceptance program** can run its transcript under `Sim` with concurrent clients, slow readers and connection resets, at thousands of seeds per CI run, with no sockets.

**How it fits the rest.** Effect inference (#1) tells the runner which tests are fully simulated and therefore cacheable (#18). Contracts (#7) and invariants are checked during simulation. Property tests (#8) can draw fault schedules from the same seed.

**Prior art and how Mora differs.** FoundationDB's simulator and TigerBeetle's VOPR are bespoke, application-wide designs. madsim and turmoil are bolted onto Rust and fight dependencies that bypass them. Antithesis works at the hypervisor level and is commercial. Mora puts the seam in the runtime, which every library already uses. Because effects are inferred, the compiler can *prove* that no code in a test bypasses the World: the only escape is `ffi`, and that is flagged.

**Impact.**
- Speed: zero cost in release builds; one indirection per I/O call in dev and test.
- DevX: instant timer tests and reproducible concurrency bugs.
- AI era: agents iterate against fast, deterministic, seed-reproducible tests, which is the feedback loop that makes autonomous fixing work. Flaky tests are poison to agents.
- Cost: M. The MVP part is small but must be decided before M6: the platform interface must include scheduling decisions and time as World calls, and the executor must take a seedable "pick next" policy. The simulator itself is about one engineer-quarter post-MVP.
- Milestone: World seam and seeded scheduler in M6; `Std.Testing.Sim` in v0.2 (M8/M9 timeframe).

**Risks.** Static state is shared between simulated hosts in one process. Mitigate by documenting it, linting `static mut` reachable from a host closure, and post-MVP giving simulated hosts their own static tables (statics are lazily initialized through the runtime, §4.7). Threads (M10) need the simulator to own thread scheduling too, which is still possible with a deterministic thread scheduler, but must be designed in M10. A real database or other external process cannot be simulated: those tests stay on the real World.

**Verdict: Adopt. The World seam and seeded scheduler go into the MVP (M6); the simulator follows post-MVP. Confidence high.**

---

### 5. Agent-grade toolchain: `mora query`, `mora fix`, `mora context`, `mora mcp`

**The problem.** Agents read code as text, guess types, re-run whole builds to see errors, and parse human-oriented output. LSP is built for one editor session (cursor position, open documents), not for "tell me every caller of X and its effects". Roslyn and rust-analyzer have rich semantic APIs, but they are in-process libraries, unstable, and not exposed as a CLI contract.

**The Mora design.** The salsa database that serves the compiler and the LSP (§14.3) also serves a stable, versioned, JSON-in/JSON-out command surface:

| Command | Returns |
|---|---|
| `mora query symbol UsersApi.UserStore.Add` | Signature, doc, `uses` set, contracts, location, visibility, semantic hash |
| `mora query refs/callers/callees <symbol>` | Locations with enclosing symbols |
| `mora query type-at src/Program.mr:14:19` | The type, the expansion origin if macro-generated, implicit conversions applied |
| `mora query effects <symbol> --why` | The effect set and the call path that introduces each effect |
| `mora context <symbol> --budget 4000` | A compact "context pack" sized to a token budget: the signature, docs, contracts, effects, field layout, one hop of callers, the tests that reach the symbol |
| `mora fix [--code MR0162] [--applicability safe]` | Applies fixes, then rechecks |
| `mora check --watch --message-format json` | Streams diagnostics as they change |
| `mora mcp` | An MCP server over stdio that exposes the commands above plus `check`, `test` (with filter and seed), `expand`, `explain`, `diff` and `docs` |

- **Fix applicability.** Extend the §17.6 diagnostic JSON with `"applicability": "safe" | "review" | "placeholder"` (rustc's MachineApplicable, MaybeIncorrect and HasPlaceholders). `mora fix` applies `safe` fixes only, unless told otherwise.
- **Stability.** Each output carries `"schema": "mora.query/1"`. Schema changes follow semver and are documented like the std. Responses are deterministic: same inputs, same bytes. Agents can therefore cache them.
- **`mora mcp` is read-only by default.** Edits go through `fix` only when `--allow-edits` is passed. No tool spends tokens or calls a model: Mora is the tool, not an agent.

**Prior art.** rust-analyzer's SCIP and LSIF exports (offline indexes), the TypeScript `tsserver` protocol, Roslyn workspaces, Unison's codebase manager, and the 2025 wave of MCP wrappers around language servers. Mora differs in making these first-party, stable, part of the one `mora` binary (G5), and in including effects and contracts, which no other semantic API has.

**Impact.**
- Speed: none.
- DevX: humans get scripting and CI power: "fail if any handler can reach `process`".
- AI era: high. It cuts the guess-compile-read loop, and context packs cut token use.
- Cost: M. Mostly serialization of queries the LSP already answers; `mcp` is a thin adapter.
- Milestone: JSON applicability and `mora fix` in M7 (the fixes exist already); `query` and `context` in M7 if the budget allows, otherwise v0.2; `mcp` in v0.2.

**Risks.** The surface becomes an API that must be kept stable. Start small (symbol, refs, type-at, effects) and version it. MCP itself is still evolving, so keep the adapter thin.

**Verdict: Adopt (JSON applicability and `fix` for the MVP; `query`, `context` and `mcp` post-MVP if M7 is tight). Confidence high.**

---

### 6. Newtypes and pattern refinements

**The problem.** Primitive obsession: ids, ports, percentages, emails and cents are all `int`, `long` or `string`, so they get swapped, unvalidated, or validated in twelve places. Each language has a partial fix that people do not use, because it costs too much ceremony.

```csharp
void Transfer(long fromAccount, long toAccount, long amountCents);   // swap two args: compiles
public readonly record struct UserId(long Value);                    // works, but .Value everywhere,
                                                                     // custom JSON converters, no validation
```

```rust
struct Port(u16);   // plus TryFrom, Display, serde attributes, FromStr... by hand or with 3 crates
```

```typescript
type UserId = string & { __brand: "UserId" };   // branding: erased, unchecked at boundaries
```

**The Mora design.** A `newtype` declaration creates a new value type with the same layout as its underlying type. The keyword names what you get, a new type; `newtype` was rejected because it raises "distinct from what?", and `type X = Y` because every language, Mora's `using X = Y;` included, reads `=` as an alias. It may carry a refinement written with the §7.8 patterns that Mora already has.

```mora
public newtype UserId : long;
public newtype OrderId : long;
public newtype Port : int is >= 1 and <= 65535;
public newtype Percent : double is >= 0.0 and <= 100.0;
public newtype Slug : string is { Length: > 0 and <= 64 };
public newtype Email : string where EmailAddress.IsValid(value);   // [post-MVP] predicate, must be `uses none`
```

Rules:

- **Layout and cost.** Same layout as the underlying type, with no tag. `Port?` uses a tag byte, as every `T?` of a value type does.
- **Conversions.** Implicit widening to the underlying type (`Port` → `int`), which is safe because every `Port` is an `int`. No implicit conversion back: `Port.From(int)` returns `Port?`. `Port.Parse` and `IParse` come from the underlying type plus the check, with a `ParseError` that names the refinement.
- **Literals are checked at compile time.** `Port p = 8080;` compiles; `Port p = 70000;` is error `MR0260`: "70000 is not a valid Port (requires >= 1 and <= 65535)". This uses the constant evaluator of §14.6.
- **Derived for free.** `IEquatable`, `IHashable`, `IComparable`, `IDisplay` and `[Json]` come from the underlying type, transparently: a `UserId` is a JSON number. Deserialization runs the check and reports `JsonError` "port: 70000 is not in 1..=65535". JSON Schema (#20) emits `minimum`/`maximum`, `minLength`/`maxLength`. The generators of #8 produce only valid values.
- **Arithmetic is not derived** (`UserId + UserId` is meaningless). `[Arithmetic]` opts in, for same-type `+`/`-` and scalar `*`/`/`. That is the cheap 80% of units of measure (#24).
- **Members.** A `newtype` may have a body with members, as a struct does.

```mora
public void Transfer(AccountId from, AccountId to, Cents amount) { ... }
Transfer(order.Id, account.Id, total);   // error MR0261: expected AccountId, found OrderId
```

**Prior art and how Mora differs.** Haskell `newtype` (the keyword's source) and Rust tuple structs give new types without derivation or refinement. F# single-case unions are ceremonious. Ada subtypes with ranges are the closest ancestor, and Mora generalizes the range to any pattern. Liquid Haskell and F* do refinement types with SMT, which is too heavy. TypeScript branding is erased. Mora's version is the cheapest that works: refinements are patterns the compiler already checks for exhaustiveness, checked at construction only, so values carry a proof by construction and no solver is needed.

**Impact.**
- Speed: zero cost after construction. It can be faster, because validated values skip re-checks.
- DevX: removes a top source of logic bugs, plus the JSON and validation boilerplate.
- AI era: models mix up arguments of the same primitive type. Newtypes turn those mistakes into compile errors, and refinements are machine-readable specifications that flow into schemas and tool definitions (#20).
- Cost: M. A declaration kind, a conversion rule, derived impls through the macro path `struct` already uses for equality and `with` (§9.6), and the literal check through constant evaluation.
- Milestone: plain `newtype` and pattern refinements in M3/M5; `where` predicates with #21 post-MVP.

**Risks.** The keyword budget: `newtype` is contextual only at declaration start. Overlap with `struct UserId(Value: long)`: make `newtype` the documented way, and have a one-field positional struct carry a lint suggesting it.

**Verdict: Adopt for MVP (`newtype` and pattern refinements); `where` predicates post-MVP. Confidence medium-high.**

---

### 7. Contracts: a `contract` section per type

**The problem.** Function specifications live in comments, in asserts scattered through bodies, or nowhere. Nobody can check comments, asserts are invisible to callers, and an AI agent asked to "implement or modify X correctly" has no specification to read.

```csharp
/// <param name="amount">Must be positive.</param>   // unchecked prose
public void Withdraw(decimal amount) { Debug.Assert(amount > 0); ... }   // invisible to callers
// Code Contracts (2008-2017) died partly because they were a rewriter bolted onto IL.
```

**Decided (2026-10-31).** Contracts never touch the signature line. Clauses after the signature, inline parameter refinements and `requires` statements in the body were all rejected as clutter. A type holds its contracts in a `contract` section, linked to members by name. Research: [Contracts placement](research/contracts-placement.md).

**The Mora design.**

```mora
public class Account
{
    public mut(private) Balance: Cents;
    public mut(private) IsFrozen: bool;

    public mut Withdraw(amount: Cents): Result<(), AccountError>
    {
        Balance -= amount;
        return Ok(());
    }

    public Slice(start: int, end: int): Statement
    {
        return Statement(Entries[start..end]);
    }

    contract
    {
        invariant Balance >= 0;

        Withdraw
        {
            fails with AccountError.Frozen when IsFrozen;
            fails with AccountError.Insufficient when amount > Balance;
            ensures Balance == old(Balance) - amount;
        }

        Slice
        {
            requires 0 <= start && start <= end && end <= Entries.Count;
            ensures result.Count == end - start;
        }
    }
}
```

The four clauses, each with one fixed meaning:

| Clause | Constrains | Checked | On violation |
|---|---|---|---|
| `invariant cond;` | The type's state | After constructors and every public member that may assign fields | Panic, blames the member |
| `requires cond;` | The caller's input | Before the body, every build (elided when proven) | Panic, blames the caller |
| `fails with E when cond;` | Declared failure | Before the body, every build: it is program behaviour | Returns `Err(E)`; the body does not run |
| `ensures cond;` | The result and the new state | After the body, in `dev` and `mora test`; trusted in `release` | Panic, blames the member |

Rules:

- **Entries are linked by name only.** `Withdraw { ... }` repeats nothing: the member's own parameter names are in scope. Only when overloads share a name does the entry add parameter types, as `Withdraw(Cents) { ... }` (the same form C# `cref` uses). An ambiguous or unknown name is a compile error with a fix per candidate.
- **Order.** `requires` clauses run first (caller bugs), then `fails` clauses in written order, then the body.
- **Expressions are effect-free** (`uses none`, checked through #1). `result` names the return value in `ensures`. `old(e)` is valid only in `ensures`, evaluates `e` on entry and requires a copyable type.
- **Visibility.** `requires` and `fails` may reference only members at least as visible as the member, because a caller must be able to check them.
- **Messages.** A violation prints the clause's source text and operand values, reusing `#Assert` rendering (§9.9): `contract violated: requires start <= end (start = 5, end = 2) at Account.Slice`. Invariants need no label.
- **Placement.** A type may have more than one `contract` section, so an entry can sit next to its member. Free functions use a `contract` section at file level.
- **Interfaces carry contracts**, and implementations inherit them. An implementation may only add `requires else` (OR-ed, weaker) or `ensures also` (AND-ed, stronger), so substitution holds by construction.
- **Release profile.** `contracts = "all" | "public-requires" | "none"`; the defaults are `"all"` in `dev` and `mora test`, `"public-requires"` in `release`. `fails` clauses are not affected: they are behaviour, not checks.
- **Types come first.** Most real contract clauses are presence checks (73% in the .NET Code Contracts corpus). Mora has no `null`, and `newtype ... is` refinements (#6) take the single-value ranges, so a `contract` section holds only relational rules, failures and postconditions.

**Readers.** The compiler stores the sections in package metadata. Hover and code lens show them under the signature (`fails Frozen, Insufficient · ensures Balance == old(Balance) - amount`). Generated docs, `mora query symbol`, JSON Schema (a `requires` comparing one parameter to a constant maps to `minimum`/`maximum`), the property tester (#8) and `mora diff` (#9) read the same data. A stronger `requires`, a weaker `ensures` or `invariant`, or a new `fails` case on a public member is a breaking change.

**Tooling.** An "Add contract" code action creates the entry. Go to definition jumps between a member and its entry. Rename updates both. Hover on `old(...)` explains it, and completion offers `old` only inside `ensures`.

**Prior art and how Mora differs.** Eiffel (Design by Contract; its contract view is what Mora's hover renders), Ada 2012 aspects, D, Code Contracts, C++26 `pre`/`post` (whose standardization stalled on implementation-defined evaluation semantics, which Mora fixes per clause), Move's name-linked `spec` blocks, Gospel's `checks` for OCaml. `fails with E when cond`, a declared failure that generates the `Result` guard, is new: it moves validation out of the body and into the published API.

**Impact.**
- Speed: zero in release beyond `requires` and the `fails` guards the code would contain anyway.
- DevX: the signature and the body both get shorter; the rules live in one checked place.
- AI era: high. Contracts are the specification an agent reads, the property tester attacks (#8) and a reviewer diffs (#9).
- Cost: M (parsing, name linking, `old` lowering in MIR, guard and check insertion, the profile key, metadata).
- Milestone: reserve `contract` as a contextual keyword in M1.5 (cost S); the clause words are contextual inside the section only. Implement in v0.2.

**Risks.** The rules sit apart from the member, so readers look in two places; hover, code lens and go to definition carry that cost. Generated `fails` guards are control flow outside the body; they are declarative, ordered as written and visible on hover. `old()` on large values copies them: restricted to copyable values, with a lint on large copies.

**Verdict: Adopt post-MVP with the `contract` section design; reserve `contract` for the MVP. Decided 2026-10-31.**

---

### 8. Property tests derived from types and contracts

**The problem.** Example tests check the cases the author thought of, and AI-generated tests mostly restate the implementation. Property-based testing (QuickCheck, Hypothesis, FsCheck, proptest) finds the cases nobody thought of, but it needs generator boilerplate, so few teams use it.

**The Mora design.**

- **A `[Test]` may take parameters.** Its inputs are generated. Every struct, union, enum, tuple, collection, primitive and `newtype` gets a generator automatically through an `[Arbitrary]` macro, applied like the automatic struct macros of §9.6. Refinements (#6) constrain generation.
- **Shrinking follows Hypothesis.** Generators draw from one seeded choice sequence, and shrinking shortens and simplifies the sequence. No per-type shrinkers are needed, and this is far simpler to build than per-type shrinking.
- **Failing cases are saved** to `tests/.regressions/<TestName>.seeds`, checked into the repository, and replayed first on every run.
- **Contract fuzzing.** `mora test --contracts [filter]` generates inputs that satisfy `requires` for every member with an entry in a `contract` section (#7), and checks `ensures`, `invariant` and the `fails` mapping. Its oracle is "no contract violated and no panic". It needs no test code.
- **Test code never reaches a release build.** Tests, test helpers and the derived generators are compiled only under `mora test`. Production types carry no test attribute.

**Where tests live (decided 2026-10-31).** Unit tests sit next to the code, as in Rust, Zig and D, inside a `test` section, which has the same shape as the `contract` section of #7:

```mora
// src/Orders/Order.mr
public struct Order
{
    public Id: OrderId;
    public Lines: List<Line>;

    public Total(): Cents
    {
        return Lines.Sum(l => l.Price * l.Quantity);
    }
}

test
{
    [Test]
    EmptyOrderTotalsZero()
    {
        Assert.Equal(Cents.Zero, MakeOrder(lines: 0).Total());
    }

    [Test(Cases = 500)]
    JsonRoundTrips(order: Order)
    {
        let text = Json.Serialize(order);
        Assert.Equal(order, Json.Deserialize<Order>(text).Expect("round trip"));
    }

    MakeOrder(lines: int): Order          // helper: no [Test], still test-only
    {
        return Order(Id: OrderId.From(1)!, Lines: Line.Fake(lines));
    }
}
```

- **`test { }` is a section** at file level or inside a type, and a file may have several. `test` is contextual: reserved only at section position.
- **Inside it, `[Test]` marks a test**; anything else is a helper. Both exist only under `mora test`, so neither reaches a release binary nor raises dead-code warnings. `[Test]` outside a test section is an error with a fix to move it.
- **Access follows placement.** At file level a section sees what the folder sees (unmarked and `internal` members). Inside a type it also sees that type's `private` members, for the rare white-box test.
- **A sibling file is the escape hatch for long files.** `Order.tests.mr` next to `Order.mr` is a test section in its entirety, with file-level access and no wrapper.
- **Integration tests** stay in `tests/*.mr` and see only `public` items (§17.5).

| Where | Sees | Typical use |
|---|---|---|
| `test { }` at file level | Folder and project members | Most unit tests |
| `test { }` inside a type | Also `private` members | Rare white-box tests |
| `Order.tests.mr` | Same as file level | Tests that outgrew the file |
| `tests/*.mr` | `public` only | Integration tests |

**Prior art.** QuickCheck, Hypothesis (whose choice-sequence shrinking is the model here), proptest, Jqwik, and Go's built-in fuzzing (`func FuzzX(f *testing.F)`), which is the closest in being built into the toolchain. Mora differs in deriving generators from refinements and contracts without user code, and in fuzzing contracts directly.

**Impact.**
- Speed: none at run time.
- DevX: property tests cost one parameter.
- AI era: a cheap, strong oracle for generated code. "Write the contract, let the tester attack the implementation" is a workflow agents can run without a human writing tests.
- Cost: M. It needs the `[Arbitrary]` macro, the choice-sequence engine, the regression store and runner integration.
- Milestone: v0.2 for generation and shrinking. The `test` section, `.tests.mr` files and test-only compilation belong in the MVP test runner (§17.5), which should also accept parameterized tests in its design (§15.9 currently forbids parameters).

**Risks.** Generating values for shared classes with cycles: generate trees only. Run time: `Cases` defaults to 100, and `mora test --explore` raises it.

**Verdict: Adopt post-MVP; `test` sections in the MVP runner. Decided 2026-10-31.**

---

### 9. Semantic diff and compiler-enforced semver

**The problem.** Semver is an honour system. Breaking changes ship as minors, and nobody notices that a minor update of a parsing library started reading the environment. The next problem is already here: reviewing large agent-written diffs line by line does not scale, and textual diffs hide semantic changes.

**The Mora design.** `mora diff <base>..<head>` (git refs or two lockfile states) compares the two semantic databases and reports:

| Category | Example output |
|---|---|
| Public API | `- public User? Find(int id)` / `+ public Result<User?, DbError> Find(UserId id)`: **major** |
| Effects | `Pricing.Apply: + clock (via Clock.Now at src/Pricing.mr:31)`; `package json-lite: + net.outbound` |
| Capabilities | New capability required by a dependency, and whether `mora.toml` grants it |
| Contracts | `Withdraw: requires strengthened (amount > 0 → amount >= 100)`: **major** |
| Schemas | `[Json] Order: + required property "currency"`: breaking for readers of old data |
| Risk markers | New `Sql.Unchecked`, `ffi`, `[Allow]` of a deny lint, `Expect`/`.Value` sites in non-test code, new `static mut` |
| Semver verdict | `minimum required bump: major (2 reasons)` |

- **Output.** Human, Markdown (for a PR comment) and JSON. Semantic hashes per definition (a MIR hash, which §9.7 already computes for macro caching) let the diff say "body changed, behaviour-relevant" versus "formatting only".
- **`mora publish`** refuses a version that understates the bump, as Elm's package manager does. Combined with §18.4's ABI hash, this makes semver a property the compiler checks.
- **Review mode.** `mora diff --review` lists, per changed public function, its effects, contracts and the tests that reach it, which gives the reviewer the right questions to ask.

**Prior art and how Mora differs.** Elm (enforced semver for APIs and types), cargo-semver-checks (lint-based, Rust), Go's `apidiff`/`gorelease`, .NET package validation, TypeScript API Extractor. None of them diff effects, capabilities, contracts or serialization schemas, and none present the result as a review aid.

**Impact.**
- Speed: none.
- DevX: no surprise breaking updates.
- AI era: high. The reviewer of an agent's change starts from "what changed in what this code can do", and supply-chain capability creep becomes visible.
- Cost: M. It needs a query over two database snapshots; the rest is presentation.
- Milestone: M9 with packages. `mora diff` for a single project can come earlier, in v0.2.

**Risks.** False "major" verdicts erode trust: ship them as warnings for one release line before `publish` enforces them.

**Verdict: Adopt post-MVP. Confidence high.**

---

### 10. Static capability enforcement, narrowed grants and sandboxed runs

**The problem.** Every dependency runs with the full authority of the process: event-stream (2018), ua-parser-js (2021), xz (2024) and a steady stream of malicious npm and PyPI packages in 2025. AI-generated code adds a new source of code nobody fully reviewed. Deno's run-time permission flags are per process, not per dependency.

**The Mora design.** §18.6 already plans per-package grants. This proposal makes them a static guarantee built on #1:

- A package's capability set is the union of the effect sets of its functions, under lexical attribution. A dependency that has no grant for a capability cannot contain code that reaches it. The check runs at compile time, on native and wasm targets alike.
- **Object capabilities.** A library with no `net` grant can still send bytes through an `any IStream` the app hands it, because the effect belongs to the app's implementation. Authority is passed as objects, which is the object-capability discipline (E, Joe-E, Austral) obtained without new syntax.
- **The application's own manifest.** `[capabilities] app = ["net.listen", "fs.read"]` is checked too. An agent edit that makes the app spawn processes fails `mora check` until a human edits the manifest, and that edit is visible in review.
- **Narrowed grants** are run-time allowlists, applied process-wide by the std at the I/O boundary: `net.outbound = ["api.stripe.com:443"]`, `fs.read = ["./config"]`. They are checked when connecting or opening, which is cheap next to the syscall. Per-package narrowing is not attempted at run time.
- **Sandboxed runs for generated code.** `mora run script.mr --allow fs.read=./data` first checks statically that the script's effect set fits the grants, and refuses to build if it does not ("script uses net.outbound via HttpClient.GetAsync at line 9; not granted"). It then runs with the narrowed grants enforced. For code that must be treated as hostile, `--isolate` compiles to wasm32-wasi and runs it in embedded Wasmtime (post-M8, §18.1).
- **`ffi` means full trust**, and in the MVP only std uses `unsafe` (D-44), so the static check is sound as long as the compiler is. For hostile code, rely on the wasm path, not on compiler soundness.

**Prior art.** Deno permissions (run-time, per process), Java's SecurityManager (dynamic and stack-inspecting, removed in JDK 24), WebAssembly components with WASI capabilities, Austral linear capabilities, Capslock (Google's 2023 Go capability analysis tool, which is static analysis after the fact). Mora is the first to make capability analysis part of the ordinary compiler for every build, per package, with zero run-time cost.

**Impact.**
- Speed: zero cost (static), apart from narrowed allowlists, which add one check per connect or open.
- DevX: the manifest says what the app can do.
- AI era: high. Generated scripts get a pre-flight answer to "can this delete my files?", and capability creep in dependencies becomes a build error.
- Cost: M (manifest, grant check, diagnostics); the analysis is #1.
- Milestone: M9, as §22 plans; `mora run --allow` with it.

**Risks.** Std functions that are mis-tagged become holes: a test asserts that every `extern` std path is tagged. Effects of macro-generated code count, which is correct.

**Verdict: Adopt post-MVP (M9). Confidence high.**

---

### 11. Time done right

**The problem.** `DateTime` is the most-cited API mistake in .NET. One type means UTC, local or "unspecified" depending on `Kind`, and it is used both for instants and for calendar dates. Java's `Date`, JavaScript's `Date` (replaced by Temporal: Firefox 139 in May 2025, Chrome 144 in January 2026, Node 26) and Python's naive versus aware `datetime` repeat the mistake. Typical bugs: storing local time, adding 24 hours across a DST change, measuring durations with the wall clock.

```csharp
var start = DateTime.Now;                // local, wall clock
DoWork();
var elapsed = DateTime.Now - start;      // wrong across DST or NTP jumps
var due = DateTime.Today.AddDays(1);     // Kind = Local; serialized differently by each library
```

Mora's §15.6 currently copies `DateTime` (UTC only), with time zones post-MVP.

**The Mora design.** In `Std.Time`, one type per concept, following Temporal, java.time, NodaTime and jiff:

| Type | Meaning |
|---|---|
| `Instant` | A point on the UTC timeline, in nanoseconds. `Clock.Now()` returns one (`uses clock`) |
| `MonoTime` | A monotonic reading for measuring durations. `Stopwatch` and timers use it, and it never serializes |
| `TimeSpan` | An exact duration (the name is kept from §15.6) |
| `LocalDate`, `LocalTime`, `LocalDateTime` | Calendar values with no zone (birthdays, "09:00 every day") |
| `TimeZone` | From the IANA tzdb: `TimeZone.Find("Europe/Berlin")` returns `Result<TimeZone, TimeZoneError>` |
| `ZonedDateTime` | A `LocalDateTime` plus a `TimeZone` plus an offset, which is unambiguous |
| `Period` | Calendar arithmetic (months, days) as distinct from `TimeSpan` |

- **No `DateTime` type.** Using `DateTime` is error MR01xx with fixes to `Instant` or `LocalDateTime`, in line with the debt report's "one way" rule.
- **Explicit DST resolution.** Turning a local time into a zoned time in a DST gap or overlap needs a `Disambiguation` argument (`Earlier`, `Later`, `Reject`). There is no silent guess.
- **Arithmetic is precise.** `zoned + Period.Days(1)` means "same local time tomorrow", while `zoned + TimeSpan.FromHours(24)` means exactly 24 hours later.
- **JSON uses RFC 3339 with offsets.** `ZonedDateTime` adds the RFC 9557 bracketed zone suffix (`2026-03-29T09:00:00+02:00[Europe/Berlin]`).
- **Lint `wall-clock-duration`** flags `Instant - Instant` used as an elapsed time inside one function, with a fix to `Stopwatch`.
- **tzdb comes from the OS** when available. The bundled copy (about 100 KiB compressed) is linked only when `TimeZone.Find` is reachable, so size does not grow for programs that never use zones (§20.6 binary budget). The data version goes into the build info, for reproducibility.
- **Under simulation** (#4), `Clock.Now()` is virtual, and time-zone transitions can be tested at any date.

```mora
let berlin = TimeZone.Find("Europe/Berlin")?;
let local = new LocalDateTime(2026, 3, 29, 2, 30);        // inside the spring-forward gap
let meeting = local.InZone(berlin, Disambiguation.Later);   // 03:30+02:00, explicitly
let reminder = meeting - Period.Days(1);                    // 2026-03-28T02:30+01:00
let deadline = Clock.Now() + TimeSpan.FromMinutes(15);      // Instant
```

**Impact.**
- Speed: none.
- DevX: removes a classic bug class.
- AI era: models misuse `DateTime` constantly. Here the types refuse the misuse, and the MR01xx fix teaches the replacement.
- Cost: M (std work only).
- Milestone: `Instant`, `MonoTime`, `TimeSpan` and the `Local*` types in M5, replacing `DateTime` in §15.6. `TimeZone` and `ZonedDateTime` in v0.2. Moving them into the MVP is worth it if M5 has room, because the types are cheap and the tzdb loader is moderate work.

**Verdict: Adopt for MVP (types without zones); zones post-MVP. Confidence high.**

---

### 12. Structured concurrency by default, with inherited deadlines

**The problem.** Fire-and-forget tasks leak, swallow errors and outlive their requests. Go's `go f()`, C#'s `Task.Run` without await, and JavaScript promises nobody awaits all do this. Deadlines are threaded by hand (Go's `ctx context.Context` in every signature, `CancellationToken` in C#), and they get forgotten.

**Where Mora stands.** Cancellation by drop (§12.5) is already better than tokens. `Task.Scope` exists (§12.4). But `Task.Spawn` is detached by default, and the spec does not say what happens to a scope when a child fails.

**The Mora design.**

- **`Task.Scope` returns a result.** It returns `Result<T, E>`. The first child that returns `Err` cancels its siblings, by dropping them, and the scope returns that error. This is how Trio nurseries, Kotlin's `coroutineScope` and Swift's throwing task groups behave.
- **Detached spawning is explicit.** `Task.Spawn` is renamed `Task.SpawnDetached`, and lint `detached-task` warns outside `Main`. The idiomatic API is `scope.Spawn`. The HTTP server already runs connections in the app's scope (§16.1), so this is consistent.
- **Deadlines are inherited.** `Task.WithDeadline(d, async () => ...)` and `Task.WithTimeout` set a deadline on the current task subtree, stored as one field in the executor's task record. Children inherit it. `Task.Remaining` exposes it. Std I/O and timers cancel at the deadline. The HTTP server sets a per-request deadline from `WebAppOptions.RequestTimeout`. Post-MVP, an HTTP client forwards the remaining time downstream as a header, for cross-service deadline propagation as in gRPC.
- **`blocking-in-async`** comes from #1.

```mora
app.MapGet("/dashboard/{id}", async (Request req) =>
{
    let id = req.RouteValue<UserId>("id")?;
    return await Task.WithTimeout(TimeSpan.FromSeconds(2), async () =>
    {
        return await Task.Scope(async scope =>
        {
            let profile = scope.Spawn(_profiles.LoadAsync(id));
            let orders = scope.Spawn(_orders.RecentAsync(id));      // if this fails, profile is cancelled
            return Results.Json(new Dashboard { Profile = await profile?, Orders = await orders? });
        });
    });
});
```

**Impact.**
- Speed: one pointer-sized field per task.
- DevX: no `ctx` parameters, and no leaked tasks.
- AI era: models forget to pass contexts and tokens. Here the deadline is inherited and cannot be forgotten.
- Cost: S-M within M6.

**Verdict: Adopt for MVP (M6). Confidence high.**

---

### 13. `Secret<T>`

**The problem.** Secrets leak through logs, exception messages, serialized configuration and memory dumps. GC languages cannot erase them reliably: .NET's `SecureString` is deprecated because the GC copies memory. Go and Java have the same problem.

**The Mora design.** A std type, `Std.Security.Secret<T>`:

- `IDisplay` prints `Secret<string>(***)`, so interpolation and logging are safe by construction.
- `[Json]` refuses to serialize it: the macro reports `Fail` with "`DatabaseUrl` is a Secret; mark it [JsonSecret] to serialize". Deserializing is allowed (configuration).
- The plaintext is reached only through `secret.Use((ReadOnlySpan<byte> s) -> R)`. The span is a view (§10.6), so it cannot escape the closure. Copying it out into a `string` is possible but visible, and the lint `secret-copied` flags it.
- The destructor zeroes the buffer, deterministically (§10.8), something GC runtimes cannot promise. `Secret<T>` is move-only by default, so copies cannot multiply silently, with `Clone()` explicit.
- `Use` sites are reported as an analysis fact, `secret`, in `mora query effects` and `mora diff` ("this change reads `DatabaseUrl`").

**Prior art.** Rust's `secrecy` crate (a library, opt-in, with no zeroization guarantee across moves), .NET's `SecureString` (deprecated) and Go's `memguard`. Mora is the first to make it std, with views and deterministic destruction behind it.

**Impact.** No speed cost. Big safety win for little effort. Agents that log configuration objects cannot leak secrets. Cost S. Milestone M5.

**Verdict: Adopt for MVP. Confidence high.**

---

### 14. Structured logging and tracing

**The problem.** Logs are either unstructured strings (`$"user {id} created"`, which you cannot query) or ceremony (`logger.LogInformation("User {UserId} created", id)`, where the placeholder names and argument order drift apart). Tracing spans need `using` blocks or manual `end()` calls that get lost on early returns.

**The Mora design.**

- **`Log.Info($"created user {user.Id} for {req.Path}")`** uses #2. The handler records the template `created user {user.Id} for {req.Path}`, structured fields named after each hole's source text (`user.Id = 7`, `req.Path = "/users"`), the level, the time from the World, and the current task's span. `Begin` returns `none` when the level is disabled, so holes are not evaluated. This gives the C# `LoggerInterpolatedStringHandler` behaviour without a source generator.
- **Spans are values with destructors.** `let span = Trace.Span($"load user {id}");` ends at scope exit, on every path including `?` early returns, because drops are deterministic (§10.8). A span is unique and move-only, so it can be moved into a spawned task deliberately. The current span is task-local and inherited by children (as in #12).
- **Output.** A human format by default (the §16.1 request-log line becomes one of these events), JSON lines with `MORA_LOG_FORMAT=json`, and an OpenTelemetry (OTLP) exporter post-MVP.
- **Logging is not a capability.** It is ambient, so libraries may log without a grant, and `Secret<T>` holes are redacted (#13). Under simulation, logs carry virtual timestamps and are attached to a failing seed's report.

**Prior art.** Serilog message templates, Rust `tracing` (whose span guards are dropped at scope exit, the same idea), Go's `slog`. Mora derives field names from the interpolation itself, so there is no template/argument mismatch.

**Impact.**
- Speed: a disabled level costs one branch.
- DevX: one natural syntax that is also structured.
- AI era: agents operating services query structured fields, and spans give causality.
- Cost: M.
- Milestone: core `Log` and `Trace.Span` in M5/M6 (the HTTP request log needs them anyway); OTLP export post-MVP.

**Verdict: Adopt for MVP (core). Confidence medium-high.**

---

### 15. Errors for humans and machines

**The problem.** One error string serves three audiences badly: the end user ("something went wrong"), the developer (the cause chain) and the machine (a stable code to branch on and alert on). Internal details leak to HTTP clients, and stable codes are reinvented per project. Rust's `thiserror` and `anyhow` split solves half of this.

**The Mora design** (building on the debt report's `[Error]` and `[From]`):

- **`IError` gains members with defaults:** `string Code` (stable, dotted, for example `checkout.card_declined`), `string? PublicMessage` (`none` means internal), `int? HttpStatus` and `bool IsTransient` (used by #17).
- **`[Error]` reads `[Code]`, `[Status]` and `[Public("...")]`** on cases, plus doc comments.
- **`HttpError` maps through these members.** An error with no `PublicMessage` becomes a 500 `application/problem+json` (RFC 9457) body with the code and a trace id. The full chain goes to the log (#14), so no internals leak.
- **`Report` and `Context` for applications.** `Report` wraps `any IError` plus a context chain and is meant for application code that does not need typed handling. `r.Context($"loading {path}")` adds a frame. Libraries keep typed unions. The box allocates only on the error path.

```mora
[Error]
public union CheckoutError
{
    [Code("checkout.empty_cart"), Status(422), Public("Your cart is empty.")]
    EmptyCart,
    [Code("checkout.card_declined"), Status(402), Public("The card was declined.")]
    CardDeclined(string Reason),
    [Code("checkout.store")]
    Store([From] DbError Inner);     // internal: the client sees 500 + trace id; the log gets everything
}

let config = File.ReadAllText(path).Context($"reading config {path}")?;   // in a function returning Result<T, Report>
```

**Impact.** No speed cost on the success path. Safer APIs. Agents and alerting branch on codes, not on message text. Cost S-M. Milestone: the `IError` members and HTTP mapping in M5/M6, `Report` in M5.

**Verdict: Adopt for MVP (partial). Confidence medium-high.**

---

### 16. Typed configuration with `[Config]`

**The problem.** Configuration arrives as strings from environment variables, files and flags, is parsed ad hoc, fails at first use rather than at startup, reports one error at a time, and has no documentation. .NET's `IConfiguration` binding is reflection-based and stringly keyed. Go's `os.Getenv` returns `""` for missing values.

**The Mora design.** A std macro, `[Config(Prefix = "USERS_API")]`, on a struct with fields (fields, as the debt report recommends):

```mora
[Config(Prefix = "USERS_API")]
public struct Settings
{
    /// TCP port to listen on.
    public Port Port = 8080;
    /// Database connection string.
    public Secret<string> DatabaseUrl;
    public TimeSpan RequestTimeout = TimeSpan.FromSeconds(30);
}

let settings = Config.Load<Settings>(args)?;   // Result<Settings, ConfigError>, uses env
```

- Sources, in precedence order: command-line flags (`--port`), environment variables (`USERS_API_PORT`), then an optional `mora.config.toml` or `appsettings.json`.
- **Every problem is reported at once**, at startup ("USERS_API_DATABASE_URL is missing; USERS_API_PORT=70000 is not a valid Port"). Refinements (#6) and `Secret<T>` (#13) compose.
- `--help` is generated from doc comments. `mora config schema` emits JSON Schema and a `.env.example`.
- Under simulation (#4), `env` comes from the World.

**Impact.** No speed cost. One of the most common app chores becomes declarative. Agents read the schema rather than grepping for `GetEnvironmentVariable`. Cost S-M (the macro plus a loader). Milestone v0.2. It could slip into M5 as a stretch goal, because `WebApp.Create(args)` already reads `MORA_URLS`.

**Verdict: Adopt post-MVP. Confidence medium.**

---

### 17. Retry policies, transient errors and cancel-safety

**The problem.** Retries are hand-rolled loops with no jitter, no deadline awareness and no notion of which errors are retryable, so they cause retry storms. Separately, dropping a future to cancel it (Mora's §12.5 model, which Rust shares) cancels in the middle of an operation. A `WhenAny` or `Timeout` that drops a half-done read or write loses data. This "cancellation safety" problem is one of the most-cited pain points of async Rust.

**The Mora design.**

- **`Retry` in std.** `Retry.Exponential(attempts: 5, first: TimeSpan.FromMilliseconds(50), jitter: true).RunAsync(op)` retries only errors where `IsTransient` is true (#15), never sleeps past the inherited deadline (#12), and draws its jitter from the World (#4), so it is deterministic under simulation.
- **`[CancelSafe]` on std async methods.** The attribute documents which methods lose nothing if dropped at an `await`. Lint `cancel-unsafe-race` (warn by default) flags a non-`[CancelSafe]` future passed directly to `WhenAny` or `Timeout`.
- **`Task.Shield(async () => ...)`** (promoted from Explore, 2026-10-31). The body runs to completion even if its parent is cancelled, parented to the enclosing scope, so the scope still waits for it. It covers critical sections such as "commit, then update the cache".

**Prior art.** Polly (.NET), Tokio's cancel-safety documentation, Python's `asyncio.shield`, Trio's shielded cancel scopes.

**Impact.**
- Speed: none.
- DevX: removes retry storms and data-losing races.
- Cost: S for `Retry`; M for `Shield` and the lint.
- Milestone: `Retry` in v0.2; `[CancelSafe]` annotations on std from M6, because they are cheap to add while writing std.

**Verdict: Adopt post-MVP for `Retry`, `[CancelSafe]` and `Task.Shield` (Shield promoted from Explore, 2026-10-31).**

---

### 18. Sound test caching and flaky-test triage

**The problem.** Running only the tests affected by a change matters most in agent loops, which may run the suite hundreds of times. Bazel caches soundly but needs hermetic builds. Go's test cache tracks the environment variables and files a test opens, and is still sometimes wrong. Flaky tests are rerun until green, and the cause is never found.

**The Mora design.**

- **Cache key.** The hash of the test's MIR closure (every function it can reach, which is the same data as §9.7's macro cache), the toolchain version, the seeds, and any declared `[Test(Inputs = ["testdata/**"])]` file hashes.
- **What is cacheable.** A test is cacheable only if its effect set (#1), or its World (Sim), is deterministic: no real `net`, `fs` outside the declared inputs, `env`, `process`, or real `clock`. Seeded `random` is fine. The set comes from the compiler, so the decision is sound up to compiler correctness.
- **Flaky triage.** On a failure, `mora test` reruns the test once with the same seed. A deterministic test whose result changes is a toolchain bug and gets an MR9xxx internal-error report. A test that uses the real World and flips is marked flaky, and the runner names the effects that make it non-deterministic ("uses clock via Task.Delay at Retry.mr:40; consider World = Sim").

**Impact.** No run-time cost. Much faster feedback. Agent loops get trustworthy green and red signals. Cost S-M. Milestone v0.2.

**Verdict: Adopt post-MVP. Confidence medium.**

---

### 19. Checked doc examples and snapshot assertions

**The problem.** Examples in documentation go stale. Golden outputs get written by hand. Rust's doctests and Elixir's doctests prove the fix works. Mora checks the examples in its own spec (§20.4) but not in users' code.

**The Mora design.**

- **Doc comment examples are tests.** A ```` ```mora ```` block in a doc comment (D-21: doc comments are Markdown) whose first line is `// expect:` or `// error: MRnnnn` becomes a test that `mora test` runs. This reuses the §20.4 harness. Contracts appear in the same rendered docs.
- **`Assert.Snapshot(value)`** compares the value's `IDisplay` (or JSON) output with `tests/snapshots/<test>.snap`. `mora test --bless` rewrites it, as `cargo test -- --bless` does in §20.1.

**Impact.** No run-time cost. Documentation that cannot lie. Agents update snapshots deterministically, and examples in docs are reliable context for them. Cost S. Milestone M5, with `mora test`.

**Verdict: Adopt for MVP. Confidence high.**

---

### 20. JSON Schema from types, and signatures as tool definitions

**The problem.** Every AI tool integration (MCP tools, function calling, structured output) needs a JSON Schema that matches a function's parameters. Today these are written by hand or produced by reflection (Pydantic, Zod, `System.Text.Json` schema exporters), and they drift from the code. Validation of model output is a separate step.

**The Mora design.**

- **In std:** `[Json]` (§16.3) also generates `static JsonSchema Schema`, taking descriptions from doc comments (`Std.Meta` exposes `Doc`, §9.5) and constraints from refinements (#6) and contracts (#7). The generated schema is the deserializer's contract, so validation is `Json.Deserialize<T>` itself. This covers structured LLM output with no LLM types in std.
- **In a first-party package** (`mora-lang/mcp`, not std, because MCP evolves faster than std should): a `[Tool]` attribute macro turns a method into an MCP tool. It takes the name, the description from the doc comment, the parameter schema from the parameter types, and the **tool annotations from the inferred effects**. An effect set without `fs.write`, `net`, `process` or `ffi` gives `readOnlyHint: true`; `net.outbound` gives `openWorldHint: true`. The hints are true because the compiler computed them, where in other languages they are a promise the author made.

```mora
public class UserTools
{
    private UserStore _store;

    /// Finds a user by id. Returns none when there is no such user.
    [Tool]
    public User? FindUser(UserId id) uses none => _store.Find(id);
}
// Generated: name "find_user", inputSchema {"type":"object","properties":{"id":{"type":"integer"}},"required":["id"]},
//            annotations {"readOnlyHint": true, "openWorldHint": false}
```

**Impact.** No run-time cost. Tool definitions that cannot drift and whose safety hints are verified. Cost S-M. Milestone: the schema in std in v0.2 (or M5 as a stretch); the MCP package after it.

**Verdict: Adopt post-MVP. Confidence medium-high.**

---

### 21. Compile-time evaluation of effect-free functions

**The problem.** §14.6's constant evaluator handles arithmetic and literals only. Anything richer needs `static lazy`, which runs at run time and can fail at run time. Zig's `comptime`, C++ `constexpr` and Rust `const fn` each need a separate "const" colour of function.

**The Mora design.** Any function whose inferred effect set is `none` (#1), and whose arguments are constants, may be evaluated at compile time by the macro JIT (§9.7), with the same sandbox and the same resource limits. No `const fn` marking is needed: the effect analysis is the marking. Uses:

- `const` and `static` initializers that build tables;
- refinement `where` predicates on literals (#6);
- `IInterpolate.Check` on SQL and HTML templates (#2);
- validated literals such as `#Url("https://...")` or `#Regex("...")`, as expression macros that call the parser at compile time.

**Impact.** No run-time cost. It gives a capability close to Zig's `comptime` with no new keyword. Cost M: the JIT exists, but heap values produced at compile time have to be serialized into static data. Milestone M11.

**Risks.** Effect-free does not mean terminating: the 5-second limit of §9.7 applies. Floating-point results must match between the JIT and the target, so restrict compile-time evaluation to the build host's target or interpret floats in software.

**Verdict: Adopt post-MVP. Confidence medium.**

---

### 22. Editions and `mora migrate`

**The problem.** Languages either freeze their mistakes (C#, Java) or break users (Python 3). Rust's editions show a third way. In an AI era, migration can be fully mechanical and run by tools.

**The Mora design.** `edition = "2026"` in `mora.toml`. Editions may change syntax and lints, but not the ABI or semantics across crate boundaries. Every edition change ships with a machine-applicable fix, and `mora migrate --edition 2028` applies them. Decide the policy before v0.1, because the debt report's "one way" errors and this study's contextual keywords are exactly the kind of change editions exist for.

**Impact.** No run-time cost. Mistakes found after v0.1 stay fixable. Cost S-M.

**Verdict: Adopt post-MVP (decide the policy before v0.1). Confidence high.**

---

### 23. Grammar and locality audit

**The problem.** Code that needs type information to parse, or whose meaning depends on distant declarations, confuses humans and models alike. C# has several such inheritances: generic `<` versus less-than, the `?` propagation versus conditional rule (§7.4), extension methods resolved through `using`, implicit conversions, and default interface methods.

**The Mora design.**

1. **Parse without types.** Every construct must parse without type information. The ungrammar (§14.4) is published as the normative grammar, and the parser's fuzzing (§20.1) gains a round-trip check against it.
2. **Canonicalize ambiguous forms.** The formatter rewrites the ambiguous `?` forms to their parenthesized versions, so the canonical text is never ambiguous.
3. **Make implicit mechanisms visible.** Each implicit mechanism (conversions, extension resolution, default-method dispatch, boxing, lazy statics) shows up as an inlay hint and in `mora query type-at`. The boxing hint already exists (§17.8).
4. **Put format in `mora check`.** `mora check` reports formatting differences as warnings, so there is one canonical text form in the repository.

**Impact.** No run-time cost. Code reads the same in an editor, in a diff and in a model's context. Cost S. Milestone M1.5 and M7.

**Verdict: Adopt for MVP. Confidence medium.**

---

### 24. Units of measure

F# has shown since 2009 that dimension types (`float<m/s>`) are erasable and cheap at run time, but they need unit unification over abelian groups in the type checker. Mars Climate Orbiter-class bugs are rare in web apps and common in IoT, finance (currencies) and science. The pragmatic MVP path is #6's `newtype` with `[Arithmetic]`, which covers same-unit arithmetic. Full dimension algebra (`Meters / Seconds` giving `MetersPerSecond`) and unit-suffixed literals (`30.Seconds`, `5kg`) are an Explore item for M11. Cost M-L.

**Decided (2026-10-31): adopt both levels.** The cost of unit algebra falls only on code that declares units, so programs without units pay nothing in compile time, and authors who write units have chosen that precision. The remaining cost is implementing it.

- **Level 1, with #6:** `[Arithmetic] newtype Meters : double;` gives same-unit `+`/`-`, scalar `*`/`/`, and errors on `Meters + Seconds`.
- **Level 2, post-MVP:** unit-annotated numbers (`double<m>`, `double<m/s>`), with `*` and `/` combining units, `+`, `-` and comparison requiring equal units, and literals such as `9.81<m/s^2>`. Units are erased: the run-time value is the plain number. Generic functions may be polymorphic over units (`Square(x: double<u>): double<u^2>`), checked in the bidirectional checker of §14.5.

```mora
let distance = 100.0<m>;
let time = 9.58<s>;
let speed = distance / time;           // double<m/s>
let total = distance + time;           // error: cannot add m and s
```

**Verdict: Adopt post-MVP (level 1 with #6; level 2 after the MVP). Decided 2026-10-31.**

### 25. `decimal` and currency-safe money

OQ-12 defers `decimal` to M11. Money is among the most common application data and must not be `double`. Recommendation: move `decimal` (128-bit, with .NET-compatible semantics, so the migration guide stays simple) to v0.2, make JSON numbers deserialize into it exactly, and put `Money` (an amount plus an ISO 4217 currency, where `Add` returns `Result` on a currency mismatch and `Money<Usd>` is available when the currency is static) into a first-party package. Cost M. **Adopt post-MVP, medium.**

### 26. Conditionally async higher-order functions (`async?`)

Function colouring hurts most in higher-order code: `list.ForEach(async x => ...)` either does not compile or silently runs fire-and-forget, so libraries duplicate every helper as `ForEach`/`ForEachAsync`, `Select`/`SelectAsync`. Because closures passed to `in` parameters are known at the call site and generics are monomorphized (D-11), the compiler can produce an async state-machine variant of a function when it receives an async closure, and a plain variant otherwise. Swift's `rethrows` is the model, and Swift's pitched `reasync` is the precedent.

**Decided (2026-10-31).** The modifier is `async?`, not `reasync`, and the feature is adopted.

```mora
public static async? ForEach<T>(items: List<T>, action: (T) => ())
{
    foreach (let x in items)
    {
        await action(x);   // a plain call in the sync variant; a real await in the async variant
    }
}

users.ForEach(u => Console.WriteLine(u.Name));        // sync variant, no async cost
await users.ForEach(async u => await SendEmail(u));   // async variant, waits for each call

// Narrowed: async only if `map` or `filter` is async; `log` never decides it
public static async?(map, filter) Pipeline<T, U>(items: List<T>, map: (T) => U, filter: (U) => bool, log: (string) => ())
```

| Modifier | Meaning |
|---|---|
| `async` | Always async |
| `async?` | Async if any closure parameter is async |
| `async?(map, filter)` | Async if `map` or `filter` is async |

- **The `?` sits on the keyword**, and the parentheses narrow it, as `mut(private)` narrows `mut`. `async(map)?` would read as a call followed by the error operator, so it is rejected.
- **Inside the body, `await` marks every call to a deciding closure.** In the sync variant it compiles to a plain call. A missing `await` is an error ("the call to `action` can suspend in the async variant").
- **Awaiting a closure that is not listed** in `async?(...)` is an error with a fix that adds it. `async?(action)` when `action` is the only closure parameter is a warning with a fix to `async?` (redundant, not misleading).
- **The async variant returns a task**, so dropping it is the usual unawaited-task error, never fire-and-forget.
- Std uses it for LINQ, `ForEach`, `Retry` (#17) and similar helpers, so no `XAsync` duplicates are needed.

Cost M (async lowering of a second instantiation). **Adopt post-MVP. Decided 2026-10-31.**

### 27. Panic containment through supervised worker processes (an answer to OQ-1)

D-26 aborts the process on a panic, so one bad request kills a server. Unwinding (Rust's `catch_unwind`) is unsound with shared ARC state that may be half-updated, and expensive. Erlang's answer is process isolation. A cheap Mora version: `WebAppOptions.Workers = N` pre-forks N single-threaded worker processes that share the listening socket (`SO_REUSEPORT` on Linux and macOS; socket duplication on Windows [verify]). A supervisor restarts a worker that panics. Only the requests in flight on that worker fail. It also gives multi-core scaling before threads (M10). Cost M.

**Decided (2026-10-31): infrastructure first; workers are a framework option.**

- **Restart and scaling belong to the infrastructure.** Kubernetes, systemd and cloud platforms already restart crashed processes and run one replica per core. The MVP docs say so: run one single-process server per core and let the orchestrator restart it.
- **`WebAppOptions.Workers = N` is a post-MVP web-framework option**, not a language or compiler feature. It serves setups without an orchestrator (bare metal, a single VM, desktop and embedded) and shrinks the blast radius of a panic to one worker's requests. It changes no syntax and can ship at any time.
- **Threads would not contain panics.** A panic aborts the whole process (D-26), every thread included, so only separate processes isolate one.
- **Requirement for the MVP runtime: thread-ready, not threaded.** Reference counting goes through one runtime abstraction with atomic variants already built, the executor is designed as one per thread, and std keeps no global mutable state. This keeps M10 (threads, §12.6) an addition rather than a retrofit, at a cost of days now instead of weeks later.

**Verdict: Adopt post-MVP as a framework option; thread-ready runtime in the MVP. Decided 2026-10-31.**

### 28. Hot reload of function bodies in dev builds

The JIT (§14.16) can recompile changed function bodies and patch an indirection table in `mora run --watch`. .NET Hot Reload restricts edits in the same way (body changes only, no layout changes). It costs nothing in release builds because it is a dev-only indirection. Agents and humans iterate on servers without losing state. Edits that change a type's layout (adding a field, changing a struct) cannot be patched, so `--watch` restarts the process for those and says why. Plugins marked `reloadable` (#36) reuse the same reload path. Cost M. **Adopt post-MVP (promoted from Explore, 2026-10-31).**

### 29. Record and replay

Because all non-determinism goes through the World (#4) and the MVP is single-threaded, a production process could record World results (I/O results, clock readings, scheduling decisions) to a ring buffer and dump it on panic. `mora replay dump.mrr` then reproduces the failure exactly, the way `rr` does but at language level. The overhead while recording is real (I/O payloads), so it is opt-in. Threads (M10) make it much harder. Cost L.

**Decided (2026-10-31): two stages, plus one MVP rule.**

Replay is reliable only when every source of non-determinism is recorded. The World (#4) covers the clock, timers, random, network, files, environment, task scheduling and dictionary order (D-29). Three leaks remain:

| Leak | Fix |
|---|---|
| FFI calls (`[Import]`) and native plugins (#36) | Effect inference (#1) marks them with the `ffi` effect, so `mora replay` proves a program replayable or names the call that is not. Extern results can be recorded at the boundary like other World calls |
| Memory addresses observed by the program | **MVP rule:** object identity hashing and any identity-derived value use a per-process allocation counter, never an address. This is observable behaviour, so it must be fixed before v0.1 |
| Threads (M10) | Out of scope for stage 1; a multi-threaded recorder is future work |

- **Stage 1, Adopt post-MVP (cost M).** `mora run --record` records every World result from process start to `run.mrr`; `mora replay run.mrr` re-executes it exactly, with step-back debugging in the debugger. It targets dev, test and staging, reuses #4's World, and works on every platform, unlike `rr`, which needs CPU performance counters on Linux.
- **Stage 2, Explore (cost L).** A production flight recorder: an always-on ring buffer that dumps on panic. A window of recent events needs the program state at the window's start, so it needs periodic snapshots (`fork()` checkpoints, as `rr` uses), Linux first.

**Verdict: Adopt post-MVP for stage 1; Explore stage 2; address-independent identity in the MVP. Decided 2026-10-31.**

### 30. Algebraic effect handlers (Reject)

Koka, Eff, OCaml 5 and Unison use resumable handlers to swap effect implementations. They need stack capture or CPS conversion, which conflicts with cold state-machine futures (D-27, D-28), ARC drop elaboration (§10.8) and Cranelift lowering, and they bring a hard learning curve. The World swap (#4) plus effect inference (#1) gives the testing and injection benefits with none of those costs. **Reject, high.**

### 31. Content-addressed code storage (Reject; keep semantic hashes)

Unison stores code as hashed ASTs in a database. That gives perfect provenance and no build or dependency conflicts, but it breaks every text tool, git and code review as people know them, and adoption has stayed small. Mora keeps text files and gets most of the benefits from per-definition semantic hashes (used by #9 and #18) and hash-pinned dependencies (§18.5). **Reject, high.**

### 32. LLM client, prompt and cost-budget types in the language or std (Reject)

Provider APIs, prompt formats and pricing change monthly, and std must stay stable for years. "Cost and latency budgets as effects" would make capabilities about money rather than authority, and could not be checked statically. Mora's contribution to LLM-calling code is typed schemas (#20), validation by deserialization, deadlines (#12), retries (#17), capabilities (`net.outbound`) and simulation for tests (#4). Clients belong in packages. **Reject, high.**

### 33. AI or other non-determinism inside the compiler or `mora fix` (Reject)

Builds must be a pure function of inputs (§9.7's caching depends on it). `mora fix` applies only fixes the compiler itself generates. Model-backed suggestions belong in editors and agents that *call* `mora`, never inside it. **Reject, high.**

### 34. Database migrations and feature flags as language features (Reject)

Both are application-level concerns with many valid designs. The language contribution is already covered: typed config (#16) for flags, `[Cfg(Feature)]` for compile-time features (§17.2), typed SQL (#2), and post-MVP file-reading macros (§9.10) that can check queries against a schema file. **Reject, medium-high.**

### 35. Stackful green threads to remove async colouring (Reject)

Go and Java 21 virtual threads remove colouring by giving every task a stack. For Mora that means a stack per task, stack switching on wasm (the stack-switching proposal is not yet shipped everywhere) and on embedded targets (G7), and an FFI with C code that blocks. It would reverse D-27 and D-28. Zig chose another path in 2025-2026, an `Io` interface parameter, at the cost of a vtable call and an explicit parameter. Mora keeps stackless async and reduces the pain with `async?` (#26), `blocking-in-async` (#1) and structured concurrency (#12). **Reject, medium-high.**

---

### 36. Stable plugin ABI at an opt-in boundary

**The problem.** Native plugins (a `.so` or `.dll` loaded at run time) need two separately compiled binaries to agree on types. Rust has no stable ABI, so every `String`, `Vec` or trait object is flattened by hand into C types: crABI has been stalled since 2022, `abi_stable` and `stabby` add their own type systems, and Bevy removed its dynamic plugins in 2024 as unsound. Go's `plugin` package needs host and plugins built by the same toolchain with the same dependencies, so Terraform moved plugins into subprocesses. C++ took the opposite road and froze its whole ABI by accident: P1863 estimates 5-10% of performance blocked, and one ABI break cost Google 5-10 engineer-years. Today Mora offers only `[Export]` with the C ABI and plain types (§13), plus Wasm plugins (§18.3).

**Decided (2026-10-31).** Mora gets a stable plugin ABI, limited to a declared boundary. Research: [Stable plugin ABI](research/stable-plugin-abi.md).

**The Mora design.**

- **The boundary is one interface package.** `mora.toml` says `[abi] stable = true, edition = 1`. Only that package's types follow the stable ABI. Host and plugins both import it, and all other code keeps whole-program optimization.
- **A published ABI edition, separate from the compiler version.** One short document fixes the target's C calling convention, the layouts of `string`, `T[]`, `List<T>`, `Option<T>` and `Result<T, E>`, the vtable layout (append-only, with a slot count), and a versioned runtime function table (`MoraRuntimeV1`) for allocation, retain and release, and `Std`'s process-wide state. The object header stays private: retain and release keep an inline fast path guarded by a mask the runtime publishes, so the header is never frozen (CPython's abi3 froze its refcount field, which blocked free-threading for years). `mora abi header` generates a C header, so C, Zig or Rust code can be a Mora plugin.
- **Every boundary type is `[Frozen]` or a handle.** A `[Frozen]` struct or union has a fixed layout and crosses by value at no cost. Any other type crosses as a handle whose code lives only in the host, so its fields can change freely.
- **Interfaces grow without breaking plugins.** A `[Sealed]` interface is implemented only by the host, so adding members never breaks plugins. Adding a member with a default to a plugin-implemented interface is compatible, because the host supplies the default.
- **Loading checks versions precisely.** `mora.abi` keeps a surface hash per minor version, and the compiler records the lowest minor a plugin actually uses: a plugin built against 1.5 that uses only 1.2 features loads on a 1.2 host. A mismatch names the symbol. The loader also refuses a plugin whose declared capabilities (§18.6, #10) exceed the host's grants.
- **One interface, two transports.** `Plugin.Load` maps a trusted native plugin at full speed and requires the `ffi` capability. `Plugin.LoadComponent` loads the same interface package compiled with `--emit component` as a sandboxed Wasm component, for untrusted code. The host chooses per plugin.
- **Panics and errors.** A panic inside a plugin aborts the process as everywhere else (§11.4), and the message names the plugin. Errors cross as `Result` values.
- **Unloading is off by default.** A plugin that declares `[plugin] reloadable = true` gets a per-plugin live-object count; `Plugin.Unload()` returns `Err(UnloadError.Live(...))` while its objects are alive, instead of crashing later.
- **Contracts check plugins.** In debug builds, the host side of each boundary call checks the interface's `contract` entries (#7) and the `newtype` refinements (#6) of values a plugin returns.

```mora
// package imagefx-api: [abi] stable = true, edition = 1
[Frozen]
public struct Rgba
{
    public R: byte;
    public G: byte;
    public B: byte;
    public A: byte;
}

public interface IFilter
{
    Name(): string;
    Apply(image: Image, options: FilterOptions): Result<Image, PluginError>;
    Describe(): string => "no description";   // added in 1.1 with a default: old plugins still load

    contract
    {
        Apply
        {
            fails with PluginError.InvalidInput when image.Width == 0;
        }
    }
}

// package blur-plugin: [plugin] implements = "imagefx-api@1"
[Export(Plugin)]
public CreatePlugin(): IFilterPlugin
{
    return new BlurPlugin();
}

// host
let plugin = Plugin.Load<IFilterPlugin>("plugins/libblur.so", Grants.Of("fs.read"))?;
```

**Prior art and how Mora differs.** Swift library evolution (opt-in, resilient types, `@frozen`), COM and VST3 (reference-counted interfaces, append-only), Node-API, Vulkan and Godot GDExtension (function tables, version negotiated at load, add-only), CPython abi3/abi3t (the warning about exposing object headers). Mora's version uses a boundary declared by package rather than a compiler mode, a compiler-computed minimum version, capability checks at load, and one interface definition serving both native and Wasm plugins.

**Impact.**
- Speed: none outside the boundary. Across it, an indirect call per handle member; `[Frozen]` values are free.
- DevX: plugins written in normal Mora, with strings, lists, interfaces and `Result`, and load errors that name the symbol.
- AI era: the interface package is a typed, versioned, contract-carrying description of an extension point.
- Cost: L (the edition document, runtime table, handle shims, loader, version history in `mora.abi`, `Plugin` API).
- Milestone: after M8/M9 (the report's P1-P6). Reserve `[Frozen]`, `[Sealed]` and the `[abi]` manifest keys early.

**Spec changes.** §18.4 says bodies recompiled by consumers "never break the ABI"; for plugins this holds only for non-frozen types, since a plugin keeps its copy of a frozen type's inlined code until rebuilt, and `mora diff` (#9) warns on it. MR0230 needs an exception for boxed futures returned by async boundary calls.

**Verdict: Adopt post-MVP. Decided 2026-10-31.**

---

## Coherence: one story, not 35 features

The adopted ideas form one pipeline built on a single analysis.

```text
                [RequiresCapability] tags in std (exists, §6.11)
                                │
                     #1 effect inference (per function, per package)
       ┌──────────┬─────────────┼──────────────┬────────────┬─────────────┐
       ▼          ▼             ▼              ▼            ▼             ▼
  #10 grants  #21 comptime  #7 contracts   #18 test      #20 tool     #9 semantic
  & sandbox   (effect-free  (effect-free   caching      hints         diff
              = evaluable)   clauses)      (sound)      (truthful)    (effects added)
                                │
                     #6 refinements ─► #8 generators ─► property & contract fuzzing
                                                             │
     #4 World seam (runtime) ─► simulation, seeds, faults ───┘
                                │
     #2 typed interpolation ─► #14 logs, #13 secrets, Sql/Html/Command
                                │
     #5 query/fix/context/mcp ─► agents consume all of the above as stable JSON
```

1. **The compiler knows what code can do (#1).** The data comes from the std tags the spec already plans. Nobody writes annotations unless they want a guarantee.
2. **That knowledge is enforced** at package boundaries (#10), in contracts (#7), in compile-time evaluation (#21) and in the macro sandbox (§9.7, which becomes "effect set = none").
3. **What code can do is swappable at run time (#4),** through the World. Every test can be deterministic, and every distributed bug can be reproduced from a seed.
4. **What code should do is written next to it** (#6 refinements in types, #7 `contract` sections) and attacked automatically (#8), inside the simulated World.
5. **The safe way is the shortest way:** typed interpolation (#2), secrets (#13), time types (#11), structured concurrency (#12).
6. **Everything the compiler knows is exported** as stable, deterministic JSON (#5): to agents, CI, reviewers (#9) and tool schemas (#20). A new language is made usable to models through fixes and measured evals (#3).

The result is a workflow agents can run safely. An agent writes code, `mora check` returns typed diagnostics with fixes, `mora test` runs contract fuzzing under a simulated World with cached results, and `mora diff --review` tells the human reviewer which capabilities, contracts and schemas changed. The human reviews meaning, not syntax.

What makes this feasible for a small team:

- Every piece reuses existing machinery: the salsa database, MIR hashes, the macro JIT and its sandbox, the platform layer, the pattern checker, the interpolation lowering and the JSON diagnostics.
- None of it changes code generation or the memory model.
- None of it adds a mandatory annotation.
- The run-time cost in release builds is zero, except opt-in narrowed grants and the public `requires` checks.

### What Mora should explicitly NOT do

- **No mandatory effect annotations or effect polymorphism syntax.** Inference first, bounds only where wanted. Java's checked exceptions are the lesson.
- **No algebraic effect handlers** (#30), **no stackful green threads** (#35), **no unwinding** (keep D-26; contain panics with processes, #27).
- **No AI inside the compiler, formatter or fixer** (#33), and no non-deterministic tool output. Agents call Mora; Mora never calls a model.
- **No LLM provider, prompt or cost types in std** (#32). Packages only.
- **No in-language provenance markers for AI-written code.** Provenance belongs in VCS metadata and review tools. Mora gives those tools semantic hashes and diffs.
- **No content-addressed source storage** (#31), **no mandatory verification or SMT** in the build, **no runtime reflection** to support tooling (macros and compile-time schemas cover it, D-10).
- **No novel syntax where C# spelling fits.** Every new construct here is an opt-in clause (`uses`), a `contract` section, a declaration kind (`newtype`) or a std type.

---

## Spec sections and decisions touched by each adopted idea

| Proposal | Sections | Decisions | Milestone |
|---|---|---|---|
| #1 Effect inference, `uses` | §3.4 (contextual `uses`), §5.8 (bounds on function types), §6.1 grammar, §6.8 (interface bounds), §6.11 (`[RequiresCapability]` becomes the effect source), §9.7 (sandbox = effect `none`), §12.1 (`blocking`), §14.1 and §14.3 (new query), §17.6 (MR0250 range), §17.8 (hover), §18.6 | New D-54 "effects are inferred; `uses` is an optional upper bound; lexical attribution" | M1.5 keyword, M5, M7 |
| #2 Typed interpolation | §3.5, §7.13, §8.5 (new `IInterpolate`, `IHole`, `Literal`), §15.2, new §15.x `Html`/`Url`, §16 | New D-55 "`string` never converts to injection-sensitive types; `Literal` is compiler-only" | M3, M5 |
| #3 AI readiness | §17.1 (`docs --llm`), §17.6 (applicability), §20.6 (new budget row), §22 M7 | Amend D-33/D-53 with "agents are an audience" | M7 |
| #4 World and simulation | §12.1, §12.4, §13.1, §13.2, §15.9, §17.5, §19.4, §20.1 | Amend D-4 (seedable scheduling policy), D-50 (World is the platform interface); new D-56 | M6 hook, v0.2 |
| #5 Agent toolchain | §17.1, §17.6 (applicability, schema version), §17.8 | New D-57 "stable JSON query surface" | M7, v0.2 |
| #6 Newtypes and refinements | §3.4 (contextual `newtype`), §5.1, §5.10, §6.1, new §6.12, §7.8 (patterns as refinements), §9.6, §14.6, §16.3 | New D-58 | M3/M5 |
| #7 Contracts | §3.4 (contextual `contract`; clause words contextual inside it), §6.1 grammar, §6.9, §9.9 (`#Assert` rendering reuse), §11.4 (new panic cause), §17.2 (profile `contracts`) | New D-59 | M1.5 keywords, v0.2 |
| #8 Property tests | §15.9 (parameterized tests), §17.5, §9.9 (`[Arbitrary]`) | n/a | v0.2 |
| #9 Semantic diff, semver | §17.1, §18.4, §18.5 | Extend §18.4's ABI rule to effects, contracts, schemas | M9 |
| #10 Capability enforcement | §17.2 (`[capabilities]`), §18.6, §17.1 (`run --allow`) | Amend §18.6 text to "static, lexical attribution, process-wide narrowing" | M9 |
| #11 Time | §15.1 (prelude), §15.6, §16.3 (JSON mapping), §3.4/MR01xx for `DateTime` | Amend OQ-14 scope | M5, v0.2 |
| #12 Structured concurrency, deadlines | §12.4 (Scope result, `SpawnDetached`, `WithDeadline`), §12.5, §16.1 (request timeout) | Amend D-18 (`TaskHandle` usage) | M6 |
| #13 `Secret<T>` | new §15.x, §16.3 (`[JsonSecret]`), §10.6 (view use) | n/a | M5 |
| #14 Logging and tracing | §15.6, §16.1 (request log), new §15.x `Log`/`Trace` | n/a | M5/M6 |
| #15 Errors | §11.2 (`IError` members), §16.2 (problem+json mapping), §9.9 (`[Error]` per debt report) | n/a | M5/M6 |
| #16 `[Config]` | §9.9, §15.6 | n/a | v0.2 |
| #17 Retry, cancel-safety | §12.4, §12.5, §15 | n/a | v0.2 |
| #18 Test caching | §17.5, §20.1 | n/a | v0.2 |
| #19 Doc examples, snapshots | §15.9, §17.5, §20.4 (extended to user code) | n/a | M5 |
| #20 JSON Schema, `[Tool]` | §16.3, §9.5 (`Doc` already present) | n/a | v0.2 |
| #21 Comptime | §14.6, §9.7 | n/a | M11 |
| #22 Editions | §17.2 | New D-60 | policy before v0.1 |
| #23 Grammar and locality audit | §7.4, §14.4, §17.7, §17.8 | n/a | M1.5, M7 |
| #25 `decimal` | §5.2, OQ-12 (move to v0.2) | n/a | v0.2 |

## New open questions

| ID | Question | Proposed default | Decide by |
|---|---|---|---|
| OQ-20 | Exact effect vocabulary: is `console` a capability or ambient? Is `secret` an effect or only a fact? | `console` is a capability, `secret` is a fact | M5 |
| OQ-21 | Do public functions of `lib` projects require `uses` clauses, or only record inferred sets in `mora.meta`? | Record only; `mora diff` flags increases | M9 |
| OQ-22 | Release-profile contract checks: `public-requires` or `none` by default? | `public-requires` | v0.2 |
| OQ-23 | Per-host static state in the simulator | Shared, with a lint, in v0.2; per-host tables later | v0.2 |
| OQ-24 | Should `newtype` widen implicitly to its underlying type? | Yes for reads; never the other way | M3 |
| OQ-25 | `TimeZone` and tzdb in the MVP or v0.2? | v0.2, unless M5 finishes early | M5 |
| OQ-26 | `mora query` JSON schema governance and stability promise | Semver on `mora.query/N`; experimental until v0.2 | M7 |
| OQ-27 | Narrowed grants: process-wide only, or per package through compile-time call-site tagging? | Process-wide | M9 |
| OQ-28 | Floating-point determinism of compile-time evaluation across host and target | Host equals target only, otherwise soft-float | M11 |
| OQ-29 | Agent eval suite: which models, how frozen, who maintains the tasks | Two hosted models plus one local, frozen per release line | M7 |
| OQ-30 | Edition cadence and the first edition name | `2026`, at most one every two years | before v0.1 |

## Sources consulted for 2024-2026 prior art

- Zig's new `std.Io` interface (Andrew Kelley, October 2025): https://andrewkelley.me/post/zig-new-async-io-text-version.html, and Loris Cro: https://kristoff.it/blog/zig-new-async-io/
- PEP 750 template strings (Python 3.14): https://peps.python.org/pep-0750/ and the `str + Template` discussion: https://discuss.python.org/t/pep-750-disallow-str-template/90281
- Scala 3 capture checking (experimental; System Capless paper, 2025): https://docs.scala-lang.org/scala3/reference/experimental/capture-checking/ and https://www.arxiv.org/abs/2509.07609
- TC39 Temporal shipping status: https://github.com/tc39/proposal-temporal and https://www.infoq.com/news/2026/02/chrome-temporal-date-api
- Deterministic simulation testing in Rust (turmoil, madsim, mad-turmoil): https://s2.dev/blog/dst and https://risingwave.com/blog/deterministic-simulation-a-new-era-of-distributed-system-testing/
- Also relied on, from prior knowledge: Hypothesis's choice-sequence shrinking, Elm's enforced semver, cargo-semver-checks, Google Capslock, the MCP tool annotations (2025 spec revision), RFC 9457 problem details, C++26 contracts, and JEP 430/459 (withdrawn Java string templates).
