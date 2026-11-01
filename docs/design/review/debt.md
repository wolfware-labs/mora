# Inherited design debt in Mora: a language-design review

> **Status.** This study was written before most of the decisions listed in the [review README](README.md), and its proposals were decided item by item on 2026-10-31. Read it through those decisions: where it disagrees with [the specification](../mora.md) or its decisions register, the specification wins.

Reviewed: `docs/design/mora.md` (v0.1 draft, 2302 lines), with spot checks of the implemented parser in `crates/mora-syntax/src/parser/` (M1.1 to M1.4). Line numbers refer to `mora.md` unless a crate path is given.

Positioning used as the yardstick: Mora is *highly inspired* by C#, aims at Rust-class speed and an excellent developer experience, and takes a cleaner path whenever C#'s answer exists only because of C#'s own history (CLR, binary compatibility, reflection, exceptions, `void` not being a type, Java ancestry). The spec already says this (§1.4 line 65: "Adding a feature because C# has it, or keeping a C# quirk for the sake of familiarity" is an anti-goal; D-53). This review measures the spec against its own anti-goal.

Convention for code: **before** blocks are current Mora. **after** blocks are valid under *every* Change verdict in this report at once (no `T x = e;` locals, no auto-properties, arm-style `switch`, arrow function types, no `out`, and so on), so they can be read together. Exception: they were written before item 37 (name-first declarations), so most still put the type first; read `T x` as `x: T`.

---

## Executive summary (top changes, ranked by (DevX gain + speed gain) / cost)

1. **Make std lookups return options, not panics** (`dict[key]`, `First`, `Single`, `Add` on a duplicate). Panics abort the whole server process (§11.4, D-26), so C#'s "throw and let middleware catch it" shape turns into a crash in Mora. Std is unimplemented (M5), so the cost is S. **Change.**
2. **Adopt a "one way" rule and turn every familiarity shim into an error with a fix**: `T x = e;` locals, `readonly`, `static readonly`, `@"..."`, `#region`, `TryParse(out)`, `TryGetValue`, `Runes()`, `new()` constraint, the two `ref struct` constraint spellings, `record class`, `new[] { }`. The parser already accepts them, so only M1.5 diagnostics are needed. Cost S. **Change.**
3. **A bare interface type is never a box**: an interface used as a parameter or return type is generic or opaque (Swift `some`, Rust `impl`), and boxing must be written `any IShape`. This makes the C# habit `void F(IShape s)` fast instead of a hidden allocation. Cost M (type checker, M3 not started). **Change.**
4. **Memberwise object initializers replace constructor boilerplate**: a type with no constructor can be built with `new T { Field = ... }`, which must set every field that has no initializer. This removes `public Point(int x, int y) { X = x; Y = y; }`, `required` and `init`, and keeps D-51's "every field is written down". No parser change. **Change.**
5. **One `switch`, with arms**: drop the `case p:` / `default:` / `break` statement form and the postfix `x switch { }`, and use `switch (x) { p => ... }` as both statement and expression. Cost S to M. **Change.**
6. **Fields instead of auto-properties**: with no reflection, no data binding, no inheritance and fields read-only by default, `{ get; }`, `{ get; init; }` and `{ get; set; }` are just fields with extra syntax, and they cost a retain/release on read that a field borrow avoids (§10.13 item 5). Keep `=>` getters but check that they are cheap, add `mut(private)` write scopes and a `stored` modifier, and drop setters. Cost M. **Change (decided).**
7. **Derive `==`/`!=`/`<`/`<=`/`>`/`>=` from `IEquatable`/`IComparable`** instead of `public static bool operator ==` with C#'s pairing rules. Cost S. **Change.**
8. **`[From]` on error-union case fields** instead of a hand-written `public static implicit operator` per wrapped error (§11.2, §16.2). It uses the macro system Mora already has. Cost S. **Change.**
9. **Arrow function types `(A, B) => R`** instead of `Func<A, B, R>` / `Action<A>` / `Comparison<T>`. `Action` exists only because `void` is not a type in C#, and in Mora it is (`()`). One arrow for lambdas, function types, expression bodies and switch arms. Cost S. **Change (decided).**
10. **Modules from folders, dependencies from the manifest**: drop `namespace X;` (the namespace follows the folder) and the `import ... from` statement. That leaves three mechanisms (folder, `using`, `mora.toml`) where there are now four. Cost M. **Change** (medium confidence).

Strongest **Consider** items: C# 14 extension blocks instead of `this`-parameter extension methods, removing the `out` parameter mode, `@Name` attributes instead of `[Name]`, and an `internal` default for members.

---

## Candidate table

| # | Item | Verdict | Confidence | Cost |
|---|---|---|---|---|
| 1 | Panicking, exception-shaped std lookups (`this[key]`, `First`, `Single`, `Add`) | Change | High | S |
| 2 | Familiarity shims, "two ways to do it" (bundle, see §2) | Change | High | S |
| 3 | Interface-as-type means dynamic box (`IShape s`) | Change | Medium-high | M |
| 4 | Constructor boilerplate, `required`/`init`, D-51 | Change | High | S-M |
| 5 | `switch` statement (`case:`) vs postfix `switch` expression | Change | High (arms) / Medium (prefix-only) | S-M |
| 6 | Auto-properties `{ get; set; init; }` vs fields | Change | Medium-high | M |
| 7 | Operator overloading with `public static operator ==` and pairing rules | Change | High | S |
| 8 | Implicit conversion operators as the error-conversion mechanism | Change (`[From]`) / Consider (restrict implicit ops) | High / Medium | S |
| 9 | `Func<>`/`Action<>`/`Comparison<T>` function types | Change | Medium-high | S |
| 10 | `namespace` + `using` + `import` + manifest | Change | Medium | M |
| 11 | `out` parameter mode | Consider (lean Change) | Medium | S-M |
| 12 | `record` = heap class by default | Settled (D-56) | High | none |
| 13 | Extension methods in static classes (`this T x`) | Change (decided) | Medium | M |
| 14 | `[Attr]` attribute syntax (collides with `[...]` collections and `[weak x]` captures) | Consider | Medium-low | M |
| 15 | Default member visibility `private` (`public` everywhere) | Change (decided) | Medium | S |
| 16 | `new` keyword | Keep (plus `[]` for empty dictionaries and sets) | Medium | S |
| 17 | Generic constraints: `where` only; `where T : move` reads as a restriction but widens | Change (decided: `where` only; spell widenings `allows`) | Medium | S |
| 18 | Allman braces (D-41) | Consider | Low | S |
| 19 | `Async` method suffix | Keep | Medium | none |
| 20 | `;` terminators, braces, parenthesized conditions | Keep | High | none |
| 21 | PascalCase / camelCase / `_field` conventions | Keep | High | none |
| 22 | `I` prefix on interfaces | Keep | Medium | none |
| 23 | `T?` + `none` + `??` / `?.` as `Option<T>` (spelling settled by D-54) | Keep | High | none |
| 24 | LINQ names (`Where`/`Select`/`Aggregate`) | Keep; drop query syntax from the plan | High | none |
| 25 | `is` patterns, property and list patterns | Keep | High | none |
| 26 | `~T()` destructor syntax | Keep (migration-guide note) | Medium | none |
| 27 | `enum` separate from `union` | Keep | Medium | none |
| 28 | Top-level statements plus `Main` | Keep | Medium | none |
| 29 | `$"..."` interpolation and raw strings | Keep (drop verbatim, see #2) | High | none |
| 30 | `static class` | Keep | Medium | none |
| 31 | `params`, `default`, `sizeof`, `stackalloc`, `nameof` | Keep | High | none |
| 32 | `(T)x` casts | Keep | Medium | none |
| 33 | `int` lengths and indexes | Keep | Medium | none |
| 34 | Overloading, optional and named arguments | Keep | Medium | none |
| 35 | Events/delegates → `Signal<T>`, `IDisposable` → destructors, `Task<T>` → `async T` | Keep (already de-debted) | High | none |
| 36 | Multi-declarators `int a = 1, b;` | Change (part of #2) | High | S |
| 37 | Type-first declarations `T x` vs name-first `x: T` | Change (decided) | Medium-high | M |

---

## Detailed analysis: top 10

### 1. Panicking, exception-shaped std lookups

**What Mora does now.** §15.4 (lines 1720-1722): "The throwing names (`First`, `Single`, `this[key]`) are kept and panic, because they signal 'this cannot be missing' in C# code." §15.3 line 1711: `this[key]` "get panics when missing", `Add` "panics on duplicate, as in C#", and next to them `Get(key)` returning `TValue?`, `TryGetValue(key, out let v)` and `TryAdd`. §15.5 lists `First`/`FirstOrNull`, `Last`/`LastOrNull`, `Single`/`SingleOrNull`, `ElementAt`/`ElementAtOrNull`.

```mora
let user = _users[id];              // panics, which aborts the process (§11.4)
let first = orders.First();         // panics on empty
```

**Why it is debt.** In .NET these APIs throw `KeyNotFoundException` or `InvalidOperationException`, and ASP.NET's exception middleware turns that into a 500 for one request. Mora has no exceptions, and a panic aborts the process with exit code 101 (§11.4, line 1363, D-26; OQ-1 is still open). Copying the C# name copies the C# *expectation*, and in Mora the consequence is that one bad key in one request kills every live connection of the §2.2 server. The std is also inconsistent with itself: `Queue.Dequeue` and `Stack.Pop` already return `T?` (line 1713). And every lookup ends up with three or four spellings (`[k]`, `Get`, `TryGetValue`, `ContainsKey` + `[k]`), so the API surface doubles.

**Alternative.** Follow Swift: the dictionary subscript returns `V?`, and assigning `none` removes the key. Sequence terminals that can come up empty return `T?`. Mutations that can conflict return `bool`, as `HashSet.Add` already does. "This cannot be missing" is then written `.Expect("...")` or `.Value`, which is explicit and greppable (Rust's `.unwrap()` / `.expect()`).

```mora
// after
public User? Find(int id) => _users[id];                 // V?, no separate Get
let first = orders.First() ?? panic("no orders");         // T?
if (!_users.Add(user.Id, user)) { return Results.Conflict("duplicate id"); }
_users[user.Id] = none;                                   // removes, as in Swift
if (_users[id] is User u) { Console.WriteLine(u.Name); }  // replaces TryGetValue
```

Prior art: Swift `Dictionary.subscript -> Value?`, `Sequence.first -> Element?`; Kotlin `map[key]: V?`, `firstOrNull` as the main form; Rust keeps a panicking `Index` but servers catch panics per task with unwinding, which Mora deliberately does not do.

Note on indexer typing: a getter of `V?` and a setter of `V?` keep one indexer type, so §6.9 needs no asymmetric-indexer rule. `_users[k] = user` works through the implicit `T → T?` conversion (§5.6).

**Impact.** Speed: neutral or better. `V?` of a class, string or array is one nullable pointer with no tag (§5.6 layout, line 531), so no wrapping cost. DevX: a large gain in robustness. The most common crash in ported C# code goes away, and the API is halved (`FirstOrNull`/`SingleOrNull`/`LastOrNull`/`ElementAtOrNull`/`Get`/`TryGetValue`/`TryAdd` all go). Familiarity cost: `dict[k].Name` no longer compiles. MR0162 already reports `x.y` on `T?` with fixes `x?.y` / `x.Value.y` (§7.2), so the C# habit gets a guided fix. Cost: §15.3, §15.4, §15.5, §2.2, the migration guide. No parser work. M5 is not started.

**Verdict: Change. Confidence high.**

---

### 2. Familiarity shims: a "one way" rule

**What Mora does now.** Many C# spellings are kept next to the Mora spelling "for familiarity", which is exactly the §1.4 anti-goal:

| Shim | Where | The Mora way |
|---|---|---|
| `T x = e;` mutable typed local, plus multi-declarators | §7.10 line 876; parser `stmts.rs:101-108` | `var T x = e;` / `let T x = e;` |
| `readonly` field modifier, "accepted as redundant" | §6.3 line 636 | fields are read-only by default (D-15) |
| `static readonly T X = expr;` (must be compile-time) | §4.7 line 459 | `const` (generalized) |
| Verbatim `@"..."`, `$@"..."`, and raw `"""..."""` | §3.5 line 355 | backtick raw strings (D-55) |
| `#region` / `#endregion` "accepted and ignored so code pasted from C# keeps its structure" | §3.2 line 305 | delete them; editors fold on braces |
| `int.TryParse(s, out let n)` "kept for C# familiarity" | §15.1 line 1693 | `int.Parse(s) is Ok(let n)` |
| `TryGetValue(key, out let v)` | §15.3 line 1711 | `dict[key] is V v` (item 1) |
| `s.Runes()` "alias kept for readers coming from .NET" | §5.3 line 499 | `s.Chars()` |
| `where T : new()` = `where T : IDefault` | §8.2 line 920 | `where T : IDefault` |
| `where T : ref struct` and `allows ref struct`, "both spellings are accepted" | §8.2 line 921 | one spelling (see item 17) |
| `record class` "accepted and means `record`" | §6.5 line 670 | `record` |
| `new[] { 1, 2, 3 }` next to `[1, 2, 3]` | §5.4 line 506, §7.3 line 806 | collection expression |

**Why it is debt.** Each pair doubles what readers must recognize, what the formatter and LSP must handle, and what the style guide must arbitrate. The `T x = e;` local is the worst. It is the *shortest* typed form and it is *mutable*, which contradicts the immutable-first design (`prefer-let` warns on it, line 879). It also forces the parser into the classic C# declaration-or-expression guess: `at_typed_local` in `crates/mora-syntax/src/parser/stmts.rs:102` scans a whole type and peeks at the token after the identifier, to tell `a < b > c;` and `List<int> xs;` apart. `const` versus `static readonly` is a CLR distinction (constant inlined into callers versus field that can be re-versioned), and Mora's `static readonly` *must* already be compile-time evaluable (§4.7, §14.6), so the two are the same thing under two names.

**Alternative.** Keep a single rule: **C# spellings that Mora replaces are parsed and reported with an `MR01xx` error carrying a machine-applicable fix.** The spec already does this for `namespace X { }` (MR0120), primary constructors (MR0128), `using var` (MR0121) and `x!` (MR0160). Extend it to every row above.

```mora
// before
int count = 0;
static readonly string Banner = "users-api";
let path = @"C:\data\users.json";
if (int.TryParse(text, out let n)) { Use(n); }

// after
var int count = 0;            // or: var count = 0;
const string Banner = "users-api";
let path = `C:\data\users.json`;
if (int.Parse(text) is Ok(let n)) { Use(n); }
```

Generalized `const`: a `const` of any type that §14.6's evaluator supports (plain structs, tuples, unions, arrays of these, strings), placed in read-only data as an immortal object. Zig's and Rust's `const` work this way.

**Impact.** Speed: none. DevX: one canonical form, a smaller tour and migration guide, and a clean "the compiler teaches you Mora" story (G1: "Where a C# habit would mislead, the compiler says what to write in Mora"). Parser: none. The lossless parser keeps accepting these forms for recovery; M1.5 "Diagnostics for C# constructs" (ROADMAP line 202, not done yet) adds the errors. Note that `T x;` without an initializer goes too; `let x;` / `var T x;` remain.

**Verdict: Change. Confidence high.** The one judgment call is `T x = e`, which C# developers type by reflex. The fix-it makes it a one-keystroke correction, and the gain in consistency outweighs the cost.

---

### 3. A bare interface type is never a box (`some` by default, `any` explicit)

**What Mora does now.** §8.4 lines 931-936: "An interface used as a type (`IShape s`, `List<IShape>`) is a **dynamic interface value**", and converting a struct to it "**boxes** it ... Boxing is the only implicit allocation in the language." Returning `IEnumerable<User>` keeps working "at the cost of one box per enumeration" (line 936). Static dispatch requires spelling out a generic with `where T : IShape` (§8.3).

```mora
// before: idiomatic C#, and in Mora every call boxes a struct and goes through a vtable
public static void Print(IDisplay value) => Console.WriteLine(value);
public IEnumerable<User> Active() => _users.Values.Where(u => u.Active);   // boxes the iterator
```

**Why it is debt.** This is the most expensive C# habit in the spec. In C#, "program to the interface" (`IEnumerable<T>` parameters and returns, `ILogger` parameters) costs little because everything is already a heap object behind a GC. In Mora it means a heap box for structs, virtual calls that block inlining (LINQ's no-allocation chains, D-11, are lost the moment one is returned as `IEnumerable<T>`), and an inlay hint the reader has to notice. That breaks the "Hidden costs" anti-goal (§1.4 line 66) for the code a C# developer writes *by default*. Swift had exactly this problem and fixed it: SE-0335 made existentials explicit (`any P`), and SE-0341 / SE-0328 made `some P` the cheap default for parameters and returns.

**Alternative.**

- In a **parameter** type, a bare interface `I` means an implicit generic parameter `T : I` (Swift `some P`, Rust `impl Trait` in argument position). It is monomorphized and statically dispatched.
- In a **return** type, a bare interface means an opaque type: the function returns one concrete type that callers see only through `I` (Swift `some`, Rust `-> impl Iterator`). Returning two different concrete types is an error whose fix is `any I`.
- Everywhere a value must be stored heterogeneously (fields, type arguments, locals with a written type), write `any I`. A bare `I` there is an error with the fix `any I`. `any` is a contextual keyword in type position.

```mora
// after
public static void Print(IDisplay value) => Console.WriteLine(value);          // generic: no box
public IEnumerable<User> Active() => _users.Values.Where(u => u.Active);       // opaque: the concrete WhereIterator
public static float TotalArea(List<any IShape> shapes) => shapes.Sum(s => s.Area());  // heterogeneous: boxes, visibly
private any ILogger _log;                                                      // stored existential, visibly
```

Most C#-shaped code compiles unchanged and gets faster. Only *stored* polymorphism needs `any`, and there the cost is real and now visible.

**Impact.** Speed: large where it applies. Struct arguments are no longer boxed, calls are direct and can be inlined, and LINQ pipelines can cross function boundaries without allocating. The trade-off is code size from extra monomorphized instances, the usual one for this design. DevX: the cost model fits in one sentence ("`any` is a box; nothing else is"). The I-prefixed name still tells the reader it is an interface. Library ABI (§18.4): public generic and opaque functions need their IR, which Mora `.wasm` libraries already carry (G6). Opaque return types become part of a library's API contract, as they are in Swift; settle this in M8. Cost: §5.1, §5.10, §8.3, §8.4, §15.5, §16.1 (handler types), §10.9 (closure struct rows), plus a new D-nn. Parser: a contextual `any` in `types.rs`, small. Type checker: implicit generic parameters and opaque returns, in M3, which is not started.

**Verdict: Change. Confidence medium-high.** The main risk is a "why does this need `any`?" moment for a heterogeneous `List<IShape>`. The fix-it and the inlay hint make it a teaching moment, and it is the one place the cost is real.

---

### 4. Constructor boilerplate → memberwise object initializers

**What Mora does now.** §6.9 line 737: constructors are `public T(...) { ... }`; primary constructors are error MR0128 (D-51) because "every field's mutability and ownership must be written down". Object initializers assign `set`/`init` members after the constructor runs, and `required` members must be set (MR0144). The spec's own examples carry the boilerplate:

```mora
// before (§9.1, lines 988-993)
[Equatable]
public class Point
{
    public int X;
    public int Y;
    public Point(int x, int y) { X = x; Y = y; }
}
Console.WriteLine(new Point(1, 2) == new Point(1, 2));
```

`this.X = x` assignments, `required`, and `{ get; init; }` exist in C# to work around constructors that cannot see the fields being set, and records were added in part to escape that.

**Why it is debt.** D-51's concern is valid: fields should be declared with their mutability and ownership. But that argues against *parameter-as-field* (C# 12 primary constructors), not against a *synthesized* constructor over fields that are already declared. Mora already knows every field, its mutability (D-15) and whether it has an initializer, so the per-field constructor adds nothing.

**Alternative (Swift memberwise init, spelled with C#'s object-initializer syntax).** A struct, class or unique class that declares **no** constructor can be created with `new T { F = v, ... }`, which must assign every field that has no initializer (as if each were `required`). Fields with initializers may be overridden. Fields not visible at the construction site must have initializers, or memberwise creation is unavailable there (Swift's rule). Read-only fields may be set in the initializer: that is the object's construction, not a later mutation. Declaring any constructor turns memberwise creation off, so a type can enforce invariants with a constructor or a `Result`-returning factory.

```mora
// after
[Equatable]
public class Point
{
    public int X;
    public int Y;
}
Console.WriteLine(new Point { X = 1, Y = 2 } == new Point { X = 1, Y = 2 });

public unique class TempFile
{
    public string Path;
    private TempFile(string path) { Path = path; }   // a constructor turns memberwise creation off
    public static Result<TempFile, IoError> Create(string path)
    {
        File.WriteAllText(path, "")?;
        return new TempFile(path);
    }
    ~TempFile() { _ = File.Delete(Path); }
}
```

This also lets `required` (MR0144) and the `init` accessor go: a field with no initializer *is* required, and a read-only field *is* init-only. Positional `struct` declarations (D-56) keep their positional form.

Prior art: Swift memberwise initializers; Kotlin data classes; Rust struct literals `Point { x: 1, y: 2 }` (the same idea, without `new`); C# 11 `required` + `init`, which is the same intent spread over three features.

**Impact.** Speed: none. DevX: the most visible boilerplate in class-heavy code disappears without giving up explicit field declarations. Cost: semantic only (§6.2, §6.3, §6.5, §6.9, §9.1, the §2.3 example, D-51 amended, a new D-nn). Object-initializer parsing already exists (M1.4); `required`/`init` become MR01xx with fixes. Fits M2/M3.

**Verdict: Change. Confidence high.**

---

### 5. One `switch`, with arms

**What Mora does now.** §7.7 lines 839-843: the expression `x switch { p => e, ... }` and the statement `switch (x) { case p: ... }`, where "`break` is optional, there is no fall-through, and `goto case` does not exist. `default:` is `case _:`."

```mora
// before
switch (command)
{
    case "start":
        Start();
        break;
    case "stop" when force:
        Kill();
        break;
    default:
        Console.WriteLine($"unknown command {command}");
        break;
}

return store.Find(id) switch
{
    User u => Results.Json(u),
    none => Results.NotFound(),
};
```

**Why it is debt.** The `case:` label form exists in C# because of C's fall-through `switch`. Mora removed fall-through and `goto case` and made `break` optional, so the labels are syntax with no semantics left. That leaves two pattern constructs with different grammar (`case p:` vs `p =>`), different exhaustiveness wording (MR0170 has two descriptions), and a scope rule ("each section ends at the next label") that differs from blocks. The postfix `x switch { }` form also has its own precedence level between unary and multiplicative (§7.1 line 784), which surprises readers: `a + b switch { ... }` switches on `b`.

**Alternative.** A single construct `switch (e) { arm, ... }` in expression *and* statement position. Each arm is `pattern [when guard] => expr` or `pattern [when guard] => { block }`. In statement position the arms' values are discarded; in expression position they unify to the result type. Exhaustiveness rules stay as they are.

```mora
// after
switch (command)
{
    "start" => Start(),
    "stop" when force => Kill(),
    _ => Console.WriteLine($"unknown command {command}"),
}

return switch (store.Find(id))
{
    User u => Results.Json(u),
    none => Results.NotFound(),
};
```

Prior art: Rust `match`, Kotlin `when`, Swift `switch` (prefix, statement and expression since Swift 5.9), Scala `match`. C#'s own direction is the arm syntax.

**Impact.** Speed: none. DevX: one construct to learn, read and format; no `break` noise; no precedence surprise. Familiarity cost: C# 8+ developers know both forms, and the arm form is the one they now prefer. Cost: §7.1 (remove the `switch` level), §7.7, §7.8, examples in §2.2, §2.4, §6.6, §11.2, the formatter. Parser: `stmts.rs` switch-section parsing becomes arm parsing (reused from the expression form), and the postfix operator is dropped from the Pratt table in `exprs.rs`. Both old forms get MR01xx fixes. M1.4 rework, roughly a day.

**Verdict: Change.** High confidence on dropping `case:` labels; medium on dropping the postfix form. If the owner wants to keep postfix, keep it *only* in expressions and still remove `case:`.

---

### 6. Fields instead of auto-properties

**What Mora does now.** §6.3 lines 621-636 and §6.5 line 662: `public string Name { get; }`, `public int Step { get; set; } = 1;`, `{ get; private set; }`, `{ get; init; }`, next to `public mut int X;` fields. Records' positional parameters "become public `{ get; init; }` properties". §2.3 line 247: `public List<Node> Children { get; } = new();`.

**Why it is debt.** C# auto-properties exist because of CLR concerns Mora does not have:

- **Binary compatibility**: changing a field to a property breaks compiled callers in .NET. Mora has no binary distribution in the MVP, and §18 libraries ship typed IR.
- **Reflection and data binding** (WPF, serializers, ORMs) see only properties. Mora has no reflection; `[Json]` reads `Std.Meta` fields.
- **Virtual/override** of properties requires inheritance, which Mora does not have.
- **Read-only-ness**: C# fields are mutable by default, so `{ get; }` was the concise read-only form. Mora fields are read-only by default (D-15).

What is left is a second way to declare state. It also costs something in Mora specifically. §10.3 (line 1153): "Return values are always owned by the caller", so a getter of `string` returns +1 (a retain) unless inlining and ARC optimization remove it. A read-only field borrow is guaranteed free (§10.13 item 5) and can be borrowed in place (§10.4 table), and `ref obj.Prop` is impossible (CS0206 / MR0323). `Std.Meta` also has to model both `Fields` and `Properties` (§9.5 line 1057), and every macro handles both.

**Decided.** Two rules carry it: a member read without parentheses never does heavy work, and `x.F = v` from outside the type is always a plain store.

- **Fields hold state.** Visibility says who reads; `mut` says who writes. Bare `mut` means writable wherever the field is readable, which a field's type caps (effective accessibility): `public mut Step: int` is writable in the folder inside a folder-visible type, in the project inside an `internal` type, and everywhere inside a `public` type. `mut(scope)` narrows writes: `public mut(private) Count: int` reads anywhere and is written only inside the type (it replaces `{ get; private set; }`); `mut(ns)` limits writes to the folder (namespace) and `mut(internal)` to the project. `mut(...)` takes only `private`, `ns` or `internal`, and the scope must be narrower than the field's effective visibility; an equal or wider one (including any `mut(public)`) has no effect, and the lint `redundant-mut-scope` (warn by default, like `prefer-let`) offers a fix to bare `mut`. It is a warning, not an error, because it repeats rather than misleads, and because narrowing a type's visibility can make a previously meaningful scope redundant without the line changing. `ns` appears only inside `mut(...)`, because an unmarked declaration already means the folder (item 15).
- **`=>` getters stay, and the compiler checks they are cheap.** A getter reads the object's own fields and may call functions the compiler infers do not allocate, loop, mutate or fail (future item 1, effect inference). A violation names the call: `getter 'Total' iterates (via Sum, line 12); make it a method or 'stored'`. This enforces what .NET, Kotlin, Swift and PEP 8 already ask of getters; none of them checks it.
- **`stored` computes on write.** `public stored string FullName => $"{First} {Last}";` keeps a hidden slot that the compiler recalculates whenever an input changes, so a read is one load and the body may allocate. Every input must be writable only inside the type (an immutable field, `private mut` or `mut(private)`), and only this object's own fields count, so the recalculation always runs inside the type's own methods. Readers cannot tell `stored` from a plain getter, so switching is never a breaking change.
- **No setters.** `get`/`set` blocks go away; a write that does work (validation, hashing, notification) is a method, and a set-only property is an error with a fix to `SetX(T value)`.
- **Change notification** (`INotifyPropertyChanged`) is not a language feature: a `Std` macro, `[Observable]`, generates `SetX` methods and a typed `Signal<T.Field>` (§15.8), and reads `stored` dependencies from `Std.Meta` so dependent values notify too.
- **Deferred:** lazy `memo` methods (cache on first call), blocked on invalidation through mutable collections.

Research behind this choice: 20+ languages surveyed; no mainstream compiler checks getter cost, systems languages ban getters and lose field-to-computed evolution, and SQL computed columns show that a rule naming a property ("deterministic, this row only") reads as fair where a syntax list does not. Record positional parameters become public read-only fields.

```mora
// before (§6.3)
public class Counter
{
    private mut int _count;
    public string Name { get; }
    public int Step { get; set; } = 1;
    public Counter(string name) { Name = name; }
    public void Tick() { _count += Step; }
    public int Count => _count;
}

// after (with item 4)
public class Counter
{
    public string Name;
    public mut int Step = 1;
    public mut(private) int Count = 0;

    public bool IsIdle => Count == 0;                    // checked: field read + compare
    public stored string Label => $"{Name}: {Count}";    // allocates, so computed on write

    public void Tick() { Count += Step; }                // Label is recalculated here
}

let counter = new Counter { Name = "requests" };
counter.Step = 2;              // plain store
counter.Tick();                // the work is in a method call
```

Prior art: Swift `private(set)` and Kotlin `private set` for write scopes; Rust and Go plain fields; Carbon's unfinished properties proposal ("a pure function of the object's state, about the cost of member access"); PostgreSQL 18 virtual/`STORED` generated columns and SQL Server `PERSISTED` for `stored`. No compiled language without a GC keeps stored derived values current on write; this part is new.

**Impact.** Speed: small but real. Field reads are borrowable with no retain, guaranteed rather than left to the optimizer. DevX: no "field or property?" decision; less syntax; `Std.Meta` and every macro get simpler. Risk: a library that later turns a public field into a computed property changes the *source* only for writers and `ref` borrowers (reads look the same), and §18.4 must make field access part of the ABI or route it through IR. Swift's library-evolution model is the reference. Cost: §2.3, §6.3, §6.5, §6.9, §9.5, §9.9 (macros iterate fields only), §16.3, §3.4 (`init` / `required` contextual keywords become errors), D-15 note, a new D-nn. Parser: auto-property bodies become an MR01xx fix (rewrite to a field); `get`/`set` blocks and set-only properties get MR01xx fixes; `mut(scope)` and `stored` are small modifier additions in `items.rs`. The getter check needs effect inference (future item 1), and `stored` needs write-site recalculation in lowering. M3 "properties" deliverable changes shape rather than shrinking.

**Verdict: Change. Confidence medium-high.** Of the top 10 this is the one most likely to feel un-C#-like to C# developers, but the fix-it is mechanical and the model is simpler.

---

### 7. Comparison and equality operators derived from interfaces

**What Mora does now.** §6.9 line 741: "Operators are `public static` methods named `operator +` and so on. The C# pairing rules apply (`==` with `!=`, `<` with `>`, `<=` with `>=`). User-defined `==` makes the type implement `IEquatable<T>`." §8.5 line 944: `IComparable<T>` drives `<` ... "(when the type declares them)".

```mora
// before
public struct Money : IEquatable<Money>, IComparable<Money>
{
    public long Cents;
    public bool Equals(in Money other) => Cents == other.Cents;
    public int CompareTo(in Money other) => Cents.CompareTo(other.Cents);
    public static bool operator ==(Money a, Money b) => a.Equals(b);
    public static bool operator !=(Money a, Money b) => !a.Equals(b);
    public static bool operator <(Money a, Money b) => a.CompareTo(b) < 0;
    public static bool operator >(Money a, Money b) => a.CompareTo(b) > 0;
    public static bool operator <=(Money a, Money b) => a.CompareTo(b) <= 0;
    public static bool operator >=(Money a, Money b) => a.CompareTo(b) >= 0;
}
```

**Why it is debt.** C# needs pairing rules because each operator is an independent static method that can disagree with `Equals`. That is a known source of bugs (`==` and `Equals` inconsistent, `!=` not the negation of `==`). Mora already routes `==` through `IEquatable<T>` (§8.5 line 943) and `Dictionary` through `Equals`/`Hash`, so letting a user declare `==` separately reintroduces the inconsistency the interface removes.

**Alternative.** `==`/`!=` are *always* `Equals`/its negation; the four relational operators are *always* derived from `CompareTo`. Declaring `operator ==`, `!=`, `<`, `>`, `<=`, `>=` is an error with a fix that turns the body into `Equals`/`CompareTo`. Arithmetic, bitwise and unary operators keep the C# `public static T operator +(T a, T b)` form (familiar, and consistent with `INumber<T>` static abstracts).

```mora
// after
[Equatable, Comparable]
public struct Money
{
    public long Cents;
    public static Money operator +(Money a, Money b) => new Money { Cents = a.Cents + b.Cents };
}
```

Prior art: Swift `Comparable` (implement `<`, get the rest), Kotlin `compareTo` drives all four, Rust `PartialOrd` and `PartialEq`.

**Impact.** Speed: none; it is all monomorphized direct calls. DevX: eliminates a class of bugs and ~6 lines per comparable type. Cost: §6.9, §8.5, §9.9 (`[Comparable]` stops emitting the four operators), MR codes. No parser change. M3.

**Verdict: Change. Confidence high.**

---

### 8. Error conversion through `[From]`, not hand-written implicit operators

**What Mora does now.** §11.2 lines 1320-1334 and §16.2 lines 1800-1810. `?` converts errors through user-defined `public static implicit operator` (§11.3), one per wrapped error:

```mora
// before
public union AppError : IError
{
    NotFound(string What),
    Io(IoError Inner),
    Json(JsonError Inner);

    public static implicit operator AppError(IoError e) => Io(e);
    public static implicit operator AppError(JsonError e) => Json(e);
    public string ToString() => this switch { ... };
}
```

**Why it is debt.** General user-defined implicit conversions are one of C#'s more criticized features: they are hidden calls (against "Hidden costs", §1.4), they complicate overload resolution (§6.9 refers to the C# rules), and the spec needs a lint (`implicit-panics`) because they can fail. Mora uses them mostly for one job, wrapping an error into a layer's error union, and that job is fully mechanical.

**Alternative.** A standard-library `[Error]` macro on a union (or struct) that adds `IError` and, for every case with a single field marked `[From]`, the conversion `?` uses. The marker attribute follows the existing precedent of `[JsonName]`/`[JsonIgnore]` and of `[Default]` on union cases (§9.9). Separately, **Consider** narrowing user-defined implicit conversions: make `?` use a dedicated interface (`IFrom<E>` with `static abstract Self From(E e)`, Rust's `From`), keep the compiler-known `T → Result<T, E>` and `T → T?` conversions, and require `explicit` for everything else.

```mora
// after
[Error]
public union AppError
{
    NotFound(string What),
    Io([From] IoError Inner),
    Json([From] JsonError Inner);

    public string ToString() => switch (this)
    {
        NotFound(let w) => $"{w} not found",
        Io(let e) => $"I/O error: {e}",
        Json(let e) => $"bad JSON: {e}",
    };
}
```

Prior art: Rust `thiserror`'s `#[from]` with the `From` trait; Swift's typed throws keep conversions explicit.

**Impact.** Speed: none. DevX: less boilerplate in the most common error pattern, and a clear use of D-48 macros. Cost: §9.9 (new macro), §11.2, §11.3, §16.2, Std.Meta must expose attributes on case fields (§9.5 `Case.Fields` → `Field.Attributes`, already modeled). M4/M5. Restricting implicit conversions touches §5.10, §6.9 and §11.3 and is M3 semantics only.

**Verdict: Change for `[From]`/`[Error]` (high). Consider restricting general implicit conversions (medium).**

---

### 9. Arrow function types

**What Mora does now.** §5.8 line 552: "`Func<T1, ..., TResult>` and `Action<T1, ...>` are built-in function types, with the familiar names and up to 16 parameters." Std uses `Comparison<T>` (§15.3 line 1710). The HTTP handler type is `Func<Request, Future<Result<Response, HttpError>>>` (§16.1 line 1779). Closure-struct constraints read `where TPredicate : Func<T, bool>` (§10.9 line 1236).

**Why it is debt.** `Func` vs `Action` exists because `void` is not a type in the CLR. In Mora `void` *is* `()` (§5.2 line 491), so `Action<T>` is just `Func<T, ()>`, and having both is pure legacy. The return type last among the type arguments is hard to read in nested signatures, the 16-parameter limit is a CLR artifact, and `Comparison<T>` is a named delegate type, which Mora removed (§3.4 MR0105).

**Decided.** `(A, B) => R` is the one function-type syntax, using the same arrow as lambdas, so a parameter's type has the shape of the lambda passed to it: `Sort(compare: (T, T) => int)` takes `(a, b) => a.Age - b.Age`. `=>` then means "this produces that" everywhere: lambdas, function types, expression bodies and switch arms. `() => ()` is no argument and no result. A one-parameter type is `(A) => R`, with parentheses always required so that it never collides with the tuple type `(A, B)`: a tuple type followed by `=>` in type position is a parameter list. Function types appear only where the parser expects a type, and lambdas only where it expects an expression, so the two never compete (TypeScript has used this split since 2012). `->` stays reserved; writing it in a type gets a fix to `=>`. `Func`/`Action` names in source get an MR01xx fix.

```mora
// before
public void Sort(Comparison<T> compare) { ... }
public void Subscribe(move Action<T> handler) { ... }
public void MapGet(string pattern, move Func<Request, Future<Result<Response, HttpError>>> handler) { ... }

// after
public Sort(compare: (in T, in T) => int) { ... }
public Subscribe(handler: move (T) => ()) { ... }
public MapGet(pattern: string, handler: move (Request) => Future<Result<Response, HttpError>>) { ... }

list.Sort((a, b) => a.Age - b.Age);   // the argument has the shape of the type
```

Whether parameter modes (`in T`) appear in function types needs a rule. The simplest is that function types carry modes as parameters do, with `in` as the default and therefore usually omitted.

Prior art: TypeScript `(a: A) => R` (one arrow, as decided); Swift `(A) -> R`, Kotlin `(A) -> R`, Rust `Fn(A) -> R` and F# `A -> R` use a second arrow because their lambda syntax differs (`{ x in }`, `{ x -> }`, `|x|`).

**Impact.** Speed: none. DevX: readable signatures, and one concept instead of three (`Func`, `Action`, named delegates). Familiarity cost: low, since every other mainstream language uses arrows. Cost: §5.8, §10.9, §15.3, §15.8, §16.1, §3.4 (MR0105 hint text), D-43 text. Parser: a function-type production in `types.rs` (lookahead for `->` after a parenthesized type list), small. M1.4 touch-up.

**Verdict: Change. Confidence medium-high.**

---

### 10. Modules: folders and manifest, not `namespace` + `import`

**What Mora does now.** Four overlapping mechanisms:

- `namespace UsersApi;` per file, independent of paths ("namespaces are independent of file paths", §4.1 line 383; D-22);
- `using X;`, `using static`, `using A = B;`, `global using` (§4.3);
- `import Core from "../core";` binding a root namespace, plus `Alias::Name` (C#'s `extern alias`) (§4.4, D-35);
- `[dependencies] core = { path = "../core" }` in `mora.toml` (§17.2), and `import Json;` refers back to it.

Every file of §2.2 starts with `namespace UsersApi;`, and `Program.mr` adds `using UsersApi;` to see its own project's types.

**Why it is debt.** Path-independent namespaces are a .NET assembly-era design. In practice every .NET style guide enforces "namespace matches folder" (IDE0130), so the freedom is mostly a source of inconsistency, and it costs tooling: the LSP and incremental compilation cannot map a name to files without indexing everything. The `import` statement duplicates the manifest: a dependency has to be declared in two places, which is exactly what Cargo and Go modules avoid.

**Alternative.**

- A file's namespace is the project root namespace (`[project] namespace`, default PascalCase name) plus its folder path under `src/`, PascalCased: `src/Store/UserStore.mr` → `UsersApi.Store`. `namespace` declarations become an error with a fix (delete, or move the file).
- Dependencies are declared only in `mora.toml`. Each dependency's root namespace is in scope by its alias (Cargo's model: `core = { path = "../core" }` binds `Core`). The `import` statement is not in the MVP (keep it reserved for the post-MVP URL-import story, §18.5, if that story survives).
- `using` stays exactly as is: it is the familiar, cheap part.

```mora
// before: src/Model.mr
namespace UsersApi;

[Json]
public record User(int Id, string Name, string? Email);

// after: src/Model.mr (namespace UsersApi, from the folder)
[Json]
public struct User(Id: int, Name: string, Email: string?);
```

Prior art: Go (package = directory), Rust 2018 (module = file/folder; crates from `Cargo.toml`), Kotlin and Java (package matches path by convention, enforced by tools), Swift (module = target from the manifest).

**Impact.** Speed: build and tooling only: name-to-file lookup by path helps the LSP and the OQ-17 incremental cache. DevX: one less line per file, no "which namespace is this in?", no double dependency declaration. Familiarity cost: C# developers lose a freedom they mostly do not use. Cost: §4.1, §4.2, §4.4, §17.2, §17.3, §18.5, D-22, D-35, examples. Parser: the `namespace` declaration and `import` stay parseable and become MR01xx errors (M1.5). Name resolution (M1.6) is not started.

**Verdict: Change. Confidence medium.** If the owner values the C# feel of `namespace X;`, the cheaper middle ground is to keep it *optional* and error when it disagrees with the folder; dropping `import` from the MVP stands either way.

---

## Other candidates (Consider and smaller Changes)

### 11. The `out` parameter mode: Consider (lean Change), medium, S-M

**Now.** §10.3 line 1145: `out` passes "a pointer to uninitialized memory", with MR0311 definite-assignment rules; call sites use `F(out var a)` / `F(out let a)` (§7.2 line 801). After items 1 and 2, std has no remaining `out` users.

**Debt.** In C# `out` exists because the language had no tuples and no options until C# 7. Mora has both, plus `Result`. `out` adds a parameter mode to the memory model (§10.3 table row, §10.14, MIR uninit pointers, async restriction MR0312) for no expressiveness gain.

**Alternative.** Return a tuple or an option; `ref` stays for in-place mutation (which is useful and Swift-like).

```mora
// before
static bool TryDivide(int a, int b, out int quotient) { ... }
// after
static (int Quotient, int Remainder)? Divide(int a, int b) => b == 0 ? none : (a / b, a % b);
```

**Speed.** Neutral or better: small tuples come back in registers (§13.7 "values larger than two words ... returned through a caller-provided pointer", which is the same as `out` for large ones). FFI keeps `Span<T>` and post-MVP raw pointers for C out-parameters. **Cost.** §7.2, §10.3, §10.14, §12.3, §13.9, `out` becomes an MR01xx fix; MIR (M2) simplifies. **Verdict: Consider, lean Change.**

### 12. `record` = shared heap class by default: Settled (D-56)

`record` no longer exists, so there is nothing to decide. `struct` is the identity-free value type, with generated equality and `with`; `struct Point(X: int, Y: int);` is the positional form; `class` is the identity type; the compiler picks the memory layout (perf item 5, OQ-21). `record` reports MR0133 with a fix to `struct`.

### 13. Extension methods: Consider (lean Change), medium, M

**Now.** §6.9 line 745: C# 3 extension methods (`this` on the first parameter) in a static class or namespace; "C# 14 extension blocks are post-MVP". Retroactive interface implementations are OQ-5 (M11).

**Debt.** C# itself moved to extension blocks in C# 14 (shipped November 2025), which add extension properties and static members and group extensions by receiver. Starting Mora on the deprecated-in-spirit form means LINQ and std are written twice.

**Decided.** Only extension blocks, in v0.1; the C# 3 `this`-parameter form is an error with a fix. Blocks also allow extension getters, which the cheapness check of item 6 covers like any other getter.

```mora
public extension<T>(list: List<T>)
{
    public IsEmpty: bool => list.Count == 0;
    public Pop(): T? { ... }
}
```

**Retroactive conformance (post-MVP, OQ-5), rule decided now.** The same block form later declares that an existing type implements an interface: `public extension Point : IDisplay { ... }`. The `:` means "implements", as on a class; it adds no fields, creates no type and overrides nothing. To keep one implementation per type and interface in a program (coherence), which hashing, equality and ordering depend on:

- **Libraries follow the orphan rule.** A library may write `extension T : I` only if it defines `T` or `I`, so two libraries can never conflict.
- **Applications are exempt.** A `bin` project may write any conformance, because nothing depends on it. If a library later adds the same one, the error is reported in the application with a fix: delete the application's copy.
- **Libraries that need a foreign-foreign conformance** wrap the type in a `newtype` (future item 6), which costs nothing at run time.

Prior art: Rust's orphan rule (and its long-discussed, never-adopted exemption for binary crates), Swift's `@retroactive` warning (SE-0364), Haskell orphan-instance warnings.

**Cost.** §6.9, §15.5, parser `items.rs` (a new block form), M3; OQ-5 gets the coherence rule above. **Verdict: Change (decided).** It is C#-familiar (C# 14) and cleaner.

### 14. Attribute syntax `[Name]`: Consider, medium-low, M

**Now.** §6.11: `[Json]`, `[Cfg(...)]` on "any declaration, any statement". The same `[` starts collection expressions (§7.9), list patterns (§7.8), indexers, and lambda capture lists `[weak this] () => ...` (§7.6 line 833).

**Debt.** Real parser complexity: `at_attribute` (`crates/mora-syntax/src/parser/items.rs:269-299`) scans every bracket group and decides from the token after the last one, and `at_lambda` (`exprs.rs:363`) peeks for `weak`/`move`. Every new feature that starts with `[` must be checked against three meanings. Only C# uses `[...]` for annotations; Java, Kotlin, Swift, Python, TypeScript and Dart all use `@Name`, which matters now that D-53 widened the audience. Also, macro calls already use a sigil (`#Assert`), so a sigil for attribute macros would be consistent.

**Alternative.** `@Json`, `@Cfg(Os = "linux")`. `@` is freed by removing verbatim strings (item 2) and replacing the `@class` identifier escape with Swift/Kotlin-style backticks. **Cost.** M: lexer, `items.rs`, `stmts.rs`, every example in §9/§16, the VS Code grammar. **Verdict: Consider.** Worth deciding before the M4 macro work multiplies attribute usages.

### 15. Default member visibility: Consider, medium, S

**Now.** §4.5: members default to `private`, types to `internal`; the spec writes `public` on nearly every member, even in an app where nothing outside the project can see it.

**Decided: the folder is the default boundary.** Item 10 makes a folder a namespace; an unmarked type or member is visible in its folder, as a Go identifier is visible in its package.

| Modifier | Visible to |
|---|---|
| `private` | this type |
| *(none)* | this folder (namespace) |
| `internal` | this project |
| `public` | everyone |

Project-wide `internal` (Swift's default) was rejected because Mora must scale to very large single projects, up to an operating system kernel: with it, every subsystem would see every other subsystem's internals. Type-level `private` (C#, Rust items) was rejected because apps then write `public` on nearly everything. The folder default gives apps Swift's quiet code and gives a kernel Rust's sealed subsystems: crossing a folder takes `internal` or `public`, which shows in review. Moving a file to another folder can break access to unmarked members; the compiler says so and the fix adds `internal`.

MR0123 uses *effective* accessibility, as C# does, so a `public` member of a folder-visible type can mention folder-visible types. Write scopes follow the same ladder (item 6): bare `mut` is writable wherever the field is readable, and `mut(ns)` names the folder scope when a wider-visible field should be written only in its folder, as in `public mut(ns) Version: int`.

**Cost.** §4.5 and examples; semantic only. **Verdict: Change (decided).**

### 16. `new`: Keep, medium

Dropping `new` (Kotlin, Swift) is tempting because union cases and `Ok(v)` already construct without it. But `new` is the most recognizable C# shape, target-typed `new()` has no equally clean replacement (Swift's `.init()` is worse), and `new` marks construction in a language whose anti-goal is hidden costs. **Keep**, with one small change: allow `[]` for empty `Dictionary`/`HashSet` (C#'s dictionary expressions are heading the same way), so `private Dictionary<int, User> _users = [];` replaces most `new()`.

### 17. Generic constraint syntax: Change (small), medium, S

**Now.** §8.1/§8.2: only `where` clauses. But §15.2 line 1701 already writes `Write<T : IDisplay>(in T)`, an inline bound the grammar does not define. `where T : move` *widens* what `T` accepts (D-23), but reads like a restriction, while C# 13's `allows ref struct` correctly reads as a widening.

**Decided.** `where` stays the only place for bounds; no inline `<T : I>` form. `where` can express everything (several bounds, bounds that relate parameters), so an inline form would only be a second spelling of its short cases, and item 3 already covers those: a bare interface in a parameter is an implicit generic, so `Write<T : IDisplay>(in T)` in §15.2 becomes `Write(value: IDisplay)`. A named `T` is needed only when it ties parameters together, and those signatures read better with `where`.

Widenings get their own word. A generic `T` must be copyable by default (D-23), which shuts out move-only types (`unique class File`) and views (`Span<T>`, `ref struct`). `allows` lifts that default: the generic accepts more types and its body promises not to copy (or let escape) `T`. The `:` side lists what `T` must do; `allows` lists what `T` may be.

```mora
public class Box<T> where T allows move
{
    private mut _item: T?;
    public Put(item: move T) { _item = item; }     // moves: OK
    public Dup(): (T, T) => (_item, _item);        // error: T may be move-only
}

Max<T>(a: T, b: T): T where T : IComparable<T>
Write(value: IDisplay)                              // item 3: no <T> needed
where T : IHashable allows move                     // narrow and widen together
```

`where T : move` and `where T : ref struct` become errors with fixes to `allows move` and `allows ref struct` (§8.2 currently accepts both spellings for views). **Cost.** §8.1, §8.2, §15.2, D-23 text, `types.rs`. **Verdict: Change (decided).**

### 37. Name-first declarations: Change (decided), medium-high, M

**Now.** Every declaration puts the type first, as in C#: fields `public decimal Balance;`, parameters `(decimal amount)`, returns `public decimal Deposit(...)`, locals `let decimal fee = 2.5m;`.

**Why it is debt.** Type-first comes from C, where a declaration mimics use (`int *p`). In Mora it costs three things. Inference reads as a different form: `let x = 5` versus `let int x = 5`, with the type inserted in the middle. The parser must guess declaration or expression: `stmts.rs` scans a whole type to tell `a < b > c;` from `List<int> xs;` (item 2). And long generic types push names to the right, which hurts most in the function types item 9 makes first-class.

**Decided.** The name comes first and the type follows a colon, everywhere:

```mora
public class Account
{
    public mut(private) Balance: decimal;
    public Deposit(amount: decimal): decimal { ... }
    public IsOverdrawn: bool => Balance < 0;
}

let fee: decimal = 2.5m;
let total = Sum(items);            // inferred: the annotation is an optional suffix
let byAge = (a: Person, b: Person) => a.Age - b.Age;
```

- Return types follow the parameter list as `: T`. `=>` cannot be used there, because it is the body arrow (`public Area(): double => W * H;`).
- Modifiers keep their place before the name: `public mut(private) Balance: decimal`.
- `:` already appears in named arguments `f(x: 1)`, in `class A : I`, in `: this(...)` and in `? :`; each sits in a different syntactic position, so none conflicts.
- A type-first declaration gets an MR01xx error with a fix that rewrites it.

Prior art: TypeScript, Python annotations, Kotlin, Swift, Rust and Go all put the name first; only the C family (C, C++, Java, C#) puts the type first.

**Impact.** DevX: one declaration shape with or without inference; names line up; typed lambdas and function types read alike. Familiarity: this is the change a C# developer notices first; the fix makes the correction one keystroke. Parser: `items.rs`, `stmts.rs` and `types.rs` declaration productions are rewritten, and the declaration-or-expression guess goes away. Spec: every code sample changes, so the rewrite is cheapest now, before M2. **Verdict: Change (decided).**

### 18. Allman braces (D-41): Consider, low, S

D-41 makes Allman canonical "as most C# code already uses" (§17.7). Allman is C#'s visual signature but costs vertical space: every block gains a line, and arm-style `switch` and lambdas grow most. Every other mainstream language uses K&R/1TBS, and D-53 widened the audience. The formatter is M7, so the switch is cheap now and expensive after code exists. **Consider; this is a matter of the owner's taste.** The report does not reformat its own examples, to stay comparable with the spec.

---

## Model-level debt (separate from surface syntax)

1. **Interfaces as types are boxes** (item 3). C#'s "program to interfaces" plus no `some`/`any` distinction silently turns the idiomatic signature into the slow one. *The biggest speed-related debt in the spec.* Change.
2. **Exception-shaped APIs without exceptions** (item 1). APIs designed around "throw, someone upstream catches" become process aborts. The same reflex shows in `Dictionary.Add` panicking on a duplicate and in `(C)i` casts panicking; `is`/`as` should be the documented style. Change.
3. **Property-heavy object design** (item 6). Auto-properties model CLR concerns (binary compatibility, reflection, binding) that Mora lacks, and cost a retain per read. Change.
4. **Class-first data modeling** (item 12, settled by D-56). The tour defaults to heap records and classes (`UserStore`, `User`, `Node`, `Counter`) while Mora's speed story is "values are the default" (§10 intro). The standard examples should lead with values and use classes for identity and shared mutable state. Consider for the default; Change for the examples and tour.
5. **Implicit conversions as an extension mechanism** (item 8). C# uses implicit operators where Rust uses a trait. Mora should prefer interfaces plus macros (`IFrom`, `[From]`). Change / Consider.
6. **Declaration-kind proliferation.** `struct`, `record struct`, `class`, `record`, `unique class`, `shared class`, `union`, `indirect union`, `enum`, `ref struct`, `interface`, `static class`: twelve forms. The Mora-specific axis (value / shared / unique / view) is essential and good. The C#-inherited axis (record vs not) duplicates what macros already do: a record is a type with positional fields plus `[Equatable, Hashable, Display]` and `with`. A cleaner end state treats `record` as a *modifier* on struct/class (`record struct`, `record class`, and bare `record` = value under item 12) rather than a kind, which the spec half does already. Consider; the changes above (items 4, 6 and 12) get most of the benefit without new syntax.
7. **Nominal interfaces without retroactive conformance in the MVP** (OQ-5, M11). Without inheritance, interfaces are the only abstraction, and not being able to make `Std` or third-party types implement your interface pushes users toward wrapper classes (a C#/Java pattern). Pull the extension-block form forward (item 13) and decide the coherence ("orphan") rule before M8 libraries. Consider.
8. **"Familiar compatibility" as a design value** (item 2). Several spec sentences justify a feature by "as in C#" or "kept for familiarity" alone (lines 305, 499, 670, 1693, 1722). Under D-53 that is not a reason by itself; familiarity should earn its place by being *also* the best design for Mora, as the next section's items do.

---

## Keep: familiarity that earns its place

These are C#-derived and should stay, because the familiar form is also good for Mora's model, or because changing it costs more than it returns.

- **`;` terminators, braces and parenthesized conditions.** Newline-sensitive grammars (Swift, Kotlin, Go) have edge cases of their own, the parser is done, and nothing about speed or ownership benefits. High confidence.
- **Casing conventions** (PascalCase types and members, camelCase locals, `_field`). No technical downside, and they make std and user code look alike. High.
- **The `I` prefix on interfaces.** In Mora it does a job it does not do in C#: it separates the interface (`IEquatable<T>`) from the macro that implements it (`[Equatable]`, §9.9), which share a stem by design, and in item 3's `any IShape` it makes existentials easy to spot. Medium.
- **`T?` = `Option<T>`, `none`, `??`, `?.`, flow narrowing** (§5.6, D-6, D-54; the empty value was spelled `null` when this review was written). This is the cleanest piece of C# reuse in the spec; Swift reached the same design independently (`nil`, `?`, `??`, `?.`). Note that `.Value`/`HasValue`/`GetValueOrDefault` are `Nullable<T>` names and are fine. High.
- **`let`/`var`** as immutable/mutable. Swift's pairing, and clearer than Rust's `let mut` for this audience (`var` keeps its C# meaning of "inferred local", now also "mutable"). Keep, minus the `T x =` shim (item 2). High.
- **LINQ method names** (`Where`, `Select`, `Aggregate`, `ToList`). Readable, well-known, and D-11 makes them zero-cost. Drop the post-MVP query syntax from the plan: it adds a sub-language for little gain. High.
- **`is` patterns, property, list and relational patterns, `when` guards.** C#'s pattern language is among the best in the industry and maps directly onto unions. High.
- **`async T F()` with cold, unique futures, `await`, `Task.WhenAll`.** Already de-debted from `Task<T>` (D-27). Keep the `Async` suffix too: Mora will have sync and async twins (sync `File` now, async later), which is the reason .NET has the convention. Medium.
- **`Signal<T>` instead of events and delegates; destructors instead of `IDisposable`/`using`** (D-40). Already the cleaner design. High.
- **`~T()` for destructors** (D-17). It is the C# finalizer shape with deterministic semantics; Swift's `deinit` is not clearer, and the migration guide covers the difference. Medium.
- **`enum` separate from `union`.** Integer-backed, `[Flags]`, FFI-layout enums are a different tool from tagged unions. Medium.
- **Top-level statements plus an optional `Main`.** Two forms, but they serve different program sizes and cost nothing. Medium.
- **`$"..."` interpolation, raw strings (now backticks, D-55) and `u8` literals.** Explicit interpolation is safer than Kotlin/Swift's always-on interpolation for JSON-heavy code. Remove only verbatim `@"..."` (item 2). High.
- **`static class`** for `Console`, `File`, `Results`, `Task`. Harmless, discoverable, familiar. Medium.
- **`params ReadOnlySpan<T>`, `default`, `sizeof`, `stackalloc`, `nameof`, `^i`, `a..b`.** Each fits the value and span model. High.
- **`(T)x` casts.** The cast rules are an ambiguity the parser already handles (§14.4), and `x as T` already means "gives `T?`". Medium.
- **`int` for `Length`, `Count` and indexes.** Unsigned sizes (Rust `usize`) cause underflow bugs in application code, and pointer-sized `nint` lengths would break `int i = list.Count`. The 2^31 cap is acceptable for the audience. Medium.
- **Overloading, optional and named arguments.** Well understood, and std relies on them. Medium.
- **`ref` and `in` modes with call-site `ref`.** Explicit, Swift-`inout`-like, and they carry the memory model (§10.3). High.

---

## Spec sections and decisions each Change touches

| Change | Spec sections | Decisions | Roadmap |
|---|---|---|---|
| 1. Option-returning lookups | §2.2, §15.3, §15.4, §15.5, §7.2 (MR0162 text), migration guide (§22 M7) | new D-54 ("lookups return options; panicking names are not provided") | M5 |
| 2. One-way rule / shims | §3.2 (#region), §3.5 (verbatim), §4.7 (`static readonly`), §5.3 (`Runes`), §5.4 and §7.3 (`new[]`), §6.3 (`readonly`), §6.5 (`record class`), §7.10 (`T x`, multi-declarators), §8.2 (`new()`, `ref struct`), §10.4 and §13.3 and §14.6 (`static readonly` mentions), §15.1 (`TryParse`), §15.3 (`TryGetValue`), §17.6 (new MR01xx codes) | new D-55 ("C# spellings Mora replaces are errors with fixes; one spelling per construct"); D-53 cross-reference | M1.5 (diagnostics only) |
| 3. `some` by default, `any` explicit | §5.1, §5.10, §8.3, §8.4, §10.9, §13.6, §15.5, §16.1, §18.4 | new D-56; affects D-11 (strengthens), OQ-10 wording | M1.4 (contextual `any`), M3 |
| 4. Memberwise object initializers | §2.3, §6.2, §6.3, §6.4, §6.9 (constructors, `required`), §9.1, §3.4 (`required`, `init`) | amends D-51; new D-57 | M2/M3 |
| 5. One `switch` with arms | §2.2, §2.4, §6.6, §7.1, §7.7, §7.8, §11.2, §16.2, §17.7 | new D-58 | M1.4 rework, M1.5, M3 |
| 6. Fields with `mut(scope)`; checked `=>` getters; `stored`; no setters | §2.3, §3.4, §6.3, §6.5, §6.9, §9.5, §9.9, §10.4, §16.3, §18.4 | D-15 extended; new D-59 | M1.3 touch-up, M3, M4 (Std.Meta) |
| 7. Derived comparison operators | §6.9, §8.5, §9.9 | new D-60 | M3, M4 |
| 8. `[From]` / `[Error]`; (Consider) `IFrom` for `?` | §5.10, §6.9, §9.9, §11.2, §11.3, §16.2 | D-48 (new std macros); new D-61 | M4, M5 |
| 9. Arrow function types | §3.4 (MR0105 hint), §3.6, §5.8, §10.9, §13.5, §15.1, §15.3, §15.8, §16.1 | D-43 text; new D-62 | M1.4 (`types.rs`), M3 |
| 10. Folder namespaces, manifest dependencies | §2.2, §4.1, §4.2, §4.4, §17.2, §17.3, §18.5 | supersedes D-22; amends D-35; new D-63 | M1.5, M1.6 |
| 11. (Consider) Remove `out` | §7.2, §10.3, §10.14, §12.3, §13.9 | new D-nn | M1.5, M2 |
| 12. Settled by D-56 (`struct` + `class`) | §1.7, §2.2, §2.3, §5.1, §6.5 | D-56 | done in spec |
| 13. (Consider) Extension blocks | §6.9, §6.10, §15.5; OQ-5 | new D-nn; OQ-5 | M1.3, M3 |
| 14. (Consider) `@` attributes | §3.3, §3.5, §3.6, §6.11, §9, §16.3, §17.9 | D-10 text; new D-nn | M1.1, M1.3, M1.4 |
| 15. (Consider) `internal` member default | §4.5 and examples | new D-nn | M1.6 |
| 17. Inline bounds; `allows move` | §8.1, §8.2, §15.2 | D-23 spelling | M1.4, M3 |
| 18. (Consider) K&R braces | §17.7, all examples | D-41 | M7 |

### Appendix: §2.2 under every Change verdict

The acceptance program barely changes, which is good news: the C# debt is concentrated in declarations (§6), the std surface (§15) and the type system's treatment of interfaces (§8.4), not in everyday code.

```mora
// src/Model.mr  (namespace UsersApi, from the folder)
[Json]
public struct User(Id: int, Name: string, Email: string?);

[Json]
public struct NewUser(Name: string, Email: string?);
```

```mora
// src/UserStore.mr
public class UserStore
{
    private mut int _nextId = 1;
    private Dictionary<int, User> _users = new();

    public User Add(NewUser input)
    {
        let user = new User(_nextId, input.Name, input.Email);
        _nextId += 1;
        _users[user.Id] = user;
        return user;
    }

    public User? Find(int id) => _users[id];

    public List<User> All() => _users.Values.OrderBy(u => u.Id).ToList();

    public bool Remove(int id) => _users.Remove(id);
}
```

```mora
// src/Program.mr  (same namespace as the store, so no `using UsersApi;`)
using Std.Net.Http;

let store = new UserStore();
let app = WebApp.Create(args);

app.MapGet("/users", async (Request req) => Results.Json(store.All()));

app.MapGet("/users/{id}", async (Request req) =>
{
    let id = req.RouteValue<int>("id")?;
    return switch (store.Find(id))
    {
        User u => Results.Json(u),
        none => Results.NotFound(),
    };
});

app.MapPost("/users", async (Request req) =>
{
    let input = await req.ReadJsonAsync<NewUser>()?;
    if (input.Name.Length == 0)
    {
        return Results.BadRequest("name is required");
    }
    let user = store.Add(input);
    return Results.Created($"/users/{user.Id}", user);
});

app.MapDelete("/users/{id}", async (Request req) =>
{
    let id = req.RouteValue<int>("id")?;
    return store.Remove(id) ? Results.NoContent() : Results.NotFound();
});

await app.RunAsync("127.0.0.1:8080")?;
```

Differences from the spec's version: no `namespace` lines, no `using UsersApi;`, `_users[id]` instead of `_users.Get(id)`, and a prefix `switch`.
