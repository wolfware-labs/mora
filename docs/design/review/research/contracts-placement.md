# Move contracts beside the signature, not into it

No language has found a single place for contracts that is checked, readable and uncluttered all at once. The languages that came closest split the work. **Types absorb the simple, unary conditions. Detached, compiler-linked specification blocks hold the relational rest. Tooling projects both back onto the declaration for readers.** The empirical record supports that split. About **73% of all .NET Code Contracts clauses only check that data is present** (not null, not blank, not empty), and **null checks appear in 80-100% of C# and Java preconditions**. Mora's no-null types and accepted `newtype ... is` refinements remove most of those clauses before any contract syntax exists. The failures teach a second lesson. .NET Code Contracts did not die because contracts sat in the wrong place. It died because a post-compile rewriter broke whenever the compiler changed. C++26 contracts are in trouble over evaluation semantics, not syntax: the C++26 DIS ballot was disapproved on 23 October 2026, even though the committee's own plenary had approved the draft 114-12-3 in March. **For Mora, the recommended design is a type-level `contract` section, linked to members by name and checked by the compiler.** It has an outcome clause (`fails E when cond`) that turns validation into declared `Result` errors. Contracts declared on an interface are inherited by its implementations. The compiler emits contract metadata that hovers, docs, derived property tests, JSON schemas, `mora diff` and AI agents all read. The signature line never changes, and the function body loses its guard clauses instead of gaining contract statements.

## Seven placements, and each one moves the clutter somewhere else

Every placement in use today keeps the signature clean only by paying somewhere else. The table summarizes the survey, and the paragraphs below explain what each placement costs.

| Placement | Languages | Where the cost lands |
|---|---|---|
| Clauses between signature and body | Eiffel, D, C++26, Dafny, Verus, Whiley, Spec# | Signature grows downward. The body is pushed away from its header |
| Aspects on the declaration in a spec file | Ada/SPARK `.ads`, F* `.fsti` | Two files per unit. The spec file itself is cluttered |
| Attributes or decorators above the declaration | Rust (Prusti, Kani, Creusot, nightly), Python icontract/deal | Contract sits before the reader knows the parameter names |
| Statements in the body | .NET Code Contracts, Kotlin `contract {}`, Swift `precondition`, Zig `assert` | Invisible without tools. Mixed with implementation |
| Special comments | JML `//@`, Liquid Haskell `{-@ @-}`, Gospel `(*@ *)` | Ignored by the base compiler unless a separate tool runs |
| Separate block, module, registry or file | Move `spec`, Clojure `s/fdef`, Racket `contract-out`, JML `.jml`, Prusti `extern_spec` | Linked by name, so drift is possible unless resolution is checked |
| Pushed into types | Ada predicates, Liquid Haskell, F*, Whiley, Dafny subset types | Cannot express relations between parameters or between input and output |

**Clauses after the signature are the mainstream choice, and the debate there has been about compactness, not location.** Eiffel puts `require` before `do` and `ensure` after the body, and its class `invariant` "is implicitly added to both the precondition and postcondition of every exported routine" ([eiffel.org](https://www.eiffel.org/doc/eiffel/ET-_Design_by_Contract_%28tm%29%2C_Assertions_and_Exceptions)). D started with statement blocks (`in { assert(...); } out(r) { ... } do { ... }`). DIP1009 then added the one-line forms `in (cond, "msg")` and `out (r; cond)`, because most contracts are "simple Boolean expressions" that needed "a more concise syntax". It also allowed bodyless interface declarations such as `int fun(int a) in(a > 0);` ([DIP1009](https://github.com/dlang/DIPs/blob/master/DIPs/accepted/DIP1009.md); [dlang.org](https://dlang.org/spec/function.html)). C++26's P2961 placed `pre(...)` and `post(r: ...)` "at the very end of a function declaration", after `requires`. It argued that this follows natural reading order: "first ... the full function signature, then any compile-time constraints (the requires clause), and finally any runtime constraints." It rejected the older `[[pre: ...]]` attributes because contracts "are not attributes" and because attribute position puts them before the trailing return type and `override` ([P2961R2](https://www.open-std.org/jtc1/sc22/wg21/docs/papers/2023/p2961r2.pdf)). C++ also allows a redeclaration to omit the contract, so a header can carry it while the `.cpp` definition stays clean ([cppreference](https://en.cppreference.com/w/cpp/language/function.html)). That is a small, real precedent for "state the contract once, elsewhere".

**Eiffel's real answer to clutter is a generated view, not syntax.** EiffelStudio's contract view shows signatures and contracts with the bodies removed, and its flat contract view adds inherited features ([Contract view](https://svn.eiffel.com/eiffel-org/trunk/documentation/18.07/eiffelstudio/eiffelstudio-reference/formatted-information-about-compiled-classes-and-features/class-views/contract-view.wiki); [Flat Contract view](https://eiffel.org/doc/eiffelstudio/Flat_Contract_view)). Ada does the same with a file boundary. `Pre`, `Post`, `Global` and `Depends` sit on the declaration in the package spec, and the body may restate them over hidden state with `Refined_Post` ([SPARK UG](https://docs.adacore.com/spark2014-docs/html/ug/en/source/subprogram_contracts.html)). Ada's strongest declutter tool is the predicate subtype. `subtype Even is Integer with Dynamic_Predicate => Even mod 2 = 0` makes every `Even` parameter carry its precondition implicitly, with no `Pre` aspect at all ([Ada RM 3.2.4](https://ada-auth.org/standards/22rm/html/RM-3-2-4.html)). SPARK's `Ghost` functions let a long formula be named once and used in many contracts ([SPARK UG: Ghost Code](https://docs.adacore.com/spark2014-docs/html/ug/en/source/specification_features.html)).

**The cleanest separation comes from verification and dynamic languages, not from mainstream ones.** Move attaches `spec increment { requires ...; aborts_if ...; ensures ...; }` to a function by name. The block can live in a separate `spec M { }` module in the same or another file, reusable `spec schema`s are pulled in with `include`, and `apply Schema to * except initialize` weaves one rule into a whole family of functions ([Move spec language](https://github.com/move-language/move/blob/main/language/move-prover/doc/user/spec-lang.md)). Move's `aborts_if ... with code` is a precise model of a contract over failure outcomes, not just over success. Clojure's `s/fdef` registers `:args`, `:ret` and `:fn` specs in a registry apart from the `defn`. The same object then drives instrumentation, which checks arguments only, and generative testing, where `stest/check` runs 1,000 generated cases by default. `(doc f)` prints a `Spec` section under the docstring, so the spec is visible at the declaration even though it is written elsewhere ([clojure.org spec guide](https://clojure.org/guides/spec)). Malli goes further: its instrumentation "also emits clj-kondo type configs for all `defn`s", so one runtime schema also feeds a static linter ([Malli](https://github.com/metosin/malli/blob/master/docs/function-schemas.md)). Racket puts contracts in the export list with `contract-out`, checks them only when values cross the module boundary, and blames the party that broke the agreement ([Racket Guide 7.1](https://docs.racket-lang.org/guide/contract-boundaries.html)).

Two smaller ideas matter for Mora. **Gospel splits `requires` from `checks`.** A violated `requires` is the caller's bug. A `checks` clause makes the function raise `Invalid_argument` and leave state unchanged, so the caller must handle it ([Gospel function contracts](https://ocaml-gospel.github.io/gospel/language/function-contracts)). That is exactly the line between a panic and a `Result` error. **Interface contracts without syntax become boilerplate.** Code Contracts needed a fake abstract `[ContractClassFor]` class with dummy bodies to attach contracts to an interface ([Microsoft Learn](https://learn.microsoft.com/en-us/dotnet/framework/debug-trace-profile/code-contracts)). Eiffel's `require else` and `ensure then` encode Liskov's rule directly: an override may only weaken a precondition and strengthen a postcondition ([eiffel.org: Inheritance](https://www.eiffel.org/doc/eiffel/ET-_Inheritance)). Rust treats contracts as tool metadata from the outset. Its nightly experiment lists "an interface for external tools to retrieve contract specs" as a remaining step ([rust-lang/rust#128044](https://github.com/rust-lang/rust/issues/128044)).

## Most real contracts are presence checks that types can absorb

**The evidence says a contract feature lives or dies on the cheapest clauses, and those belong in types.** Schiller et al. studied 90 C# projects with 43,823 Code Contracts clauses. They found **68% preconditions, 26% postconditions and 6% object invariants**, and **73% of all clauses "just check for the presence of data"**. A further 18% of postconditions merely restated getters or setters ([Schiller et al., ICSE 2014](https://homes.cs.washington.edu/~mernst/pubs/contract-specifications-icse2014.pdf)). The authors warn that "nullness contracts may be 'crowding out' application-specific contracts" ([Schiller et al.](https://homes.cs.washington.edu/~mernst/pubs/contract-specifications-icse2014.pdf)). Estler et al. covered 21 Eiffel, C# and Java projects. They found that "the overwhelming majority of contracts involves Void/null checks": 80-96% of C# preconditions and 88-100% of Java preconditions include one. Quantifiers are "practically never" used in pre- or postconditions, and `old` is essentially absent from C# postconditions ([Estler et al., FM 2014](https://arxiv.org/pdf/1211.4775)). Their conclusion is a design brief: contracts in practice "are short and simple", and "a little specification can go a long way" ([Estler et al.](https://arxiv.org/pdf/1211.4775)).

**Postconditions are rare where they are expensive to write, not where they are unneeded.** In Eiffel, where `ensure`, `old` and `Result` are built in, there is "no difference in the median percentages of routines with precondition and with postcondition" ([Estler et al.](https://arxiv.org/pdf/1211.4775)). In Java, which has no home for postconditions, Dietrich et al. counted **22,969 preconditions against 112 postconditions and 100 invariants** across Maven's 200 most popular projects ([Dietrich et al., ECOOP 2017](https://drops.dagstuhl.de/storage/00lipics/lipics-vol074-ecoop2017/LIPIcs.ECOOP.2017.9/LIPIcs.ECOOP.2017.9.pdf)). Daikon inferred more postconditions than preconditions on the same kind of code, so the bias toward preconditions "cannot be attributed to an absence of potential postconditions" ([Dietrich et al.](https://drops.dagstuhl.de/entities/document/10.4230/LIPIcs.ECOOP.2017.9)). Java's dominant contract construct is still `if (x == null) throw new IllegalArgumentException` (15,720 occurrences in 155 programs). Usage is highly concentrated (Gini 0.74), with "no evidence that the adoption of contracts is increasing", although projects that adopt contracts keep and expand them ([Dietrich et al.](https://drops.dagstuhl.de/storage/00lipics/lipics-vol074-ecoop2017/LIPIcs.ECOOP.2017.9/LIPIcs.ECOOP.2017.9.pdf)). Python shows the same thing from the other side: a 2022 dataset had to hand-build 514 icontract functions because there was "impractically little Python code with contracts in the wild" ([Zhang et al.](https://users.ece.utexas.edu/~gligoric/papers/ZhangETAL22PythonByContractDataset.pdf)).

**Contracts are stable, which makes them good API metadata.** Estler et al. found "specification changes are quite infrequent compared to implementation changes for the same routine", and when contracts change they are more often strengthened than weakened ([Estler et al.](https://arxiv.org/pdf/1211.4775)). Dietrich et al. found real cases of unsafe subtyping and unsafe API evolution, meaning preconditions that were strengthened in overrides or in later versions ([Dietrich et al.](https://drops.dagstuhl.de/entities/document/10.4230/LIPIcs.ECOOP.2017.9)). For Mora, this means `mora diff` would rarely flag contract changes, and when it did they would matter. Prose specifications drift: @tComment found 29 methods across 7 projects whose Javadoc null or exception claims contradicted the code ([Tan et al., ICST 2012](https://www.cs.purdue.edu/homes/lintan/publications/@tComment-icst12.pdf)). Practitioners also say *where* they want contracts. One .NET team wrote "no contracts … for private methods", because validated input made them "code bloat and run-time overhead", and Dietrich et al. attribute the precondition bias to libraries needing "defensive API surfaces to deal with unknown clients" ([Schiller et al.](https://homes.cs.washington.edu/~mernst/pubs/contract-specifications-icse2014.pdf); [Dietrich et al.](https://drops.dagstuhl.de/storage/00lipics/lipics-vol074-ecoop2017/LIPIcs.ECOOP.2017.9/LIPIcs.ECOOP.2017.9.pdf)).

**AI agents are a new consumer, and they favour short, local contracts.** GPT-4 produced correct postconditions for up to 96% of EvalPlus problems, and those postconditions distinguished up to 81% of buggy programs ([nl2postcond, FSE 2024](https://arxiv.org/abs/2310.01831v2)). Verified-code generation from formal specs succeeded about 82% of the time in Dafny but 44% in Verus ([Vericoding](https://arxiv.org/pdf/2509.22908)). On compositional multi-function specifications, success fell below 5% for some models ([DafnyComp](https://arxiv.org/pdf/2509.23061)). The researchers found no controlled study showing that contracts improve code an LLM generates for *callers*. The defensible claim is narrower: a machine-readable contract gives an agent the obligations and an oracle without its having to read the body.

Taken together, the data sorts contract clauses into three tiers. Mora's types already handle the first tier, which is presence checks plus most unary range checks (non-empty, positive, in bounds). The second tier is relational preconditions (`start <= end`, `amount <= Balance`) and state-dependent failure, which the researchers estimate at roughly 8-52% of clauses per project using Schiller's "application-specific" bucket. That is the real job for contract syntax. The third tier is postconditions, which are cheap to express once the language gives them a home. Quantifiers and `old` should work, but the syntax should not be built around them. The researchers found no study that splits the second tier precisely, and no controlled readability study of any placement. Placement arguments in the literature, P2961 and DIP1009 included, rest on design rationale, not measurement.

## Code Contracts died of plumbing, and C++26 is stalling on semantics

**.NET Code Contracts failed on architecture first and placement second.** Microsoft's Yaakov Hyman wrote in 2017 that "the rewriter tool does not work with .NET Core and the new project system … The project is abandoned and all but officially discontinued", and called it "an extraordinarily complex academic project" ([dotnet/runtime#23869](https://github.com/dotnet/runtime/issues/23869)). Roslyn-era changes to async and iterator code generation broke ccrewrite's IL pattern matching ([Habr](https://habr.com/en/articles/263235)). The API stayed in the BCL but compiled to nothing without `CONTRACTS_FULL`, which left users "misled into thinking the contracts worked" ([dotnet/runtime#23869](https://github.com/dotnet/runtime/issues/23869)). The case studies add the human costs. Teams found the static checker "slow" with "too many false positive warnings". It required contracts on all code plus stubs for external assemblies. Async debugging broke because the rewriter mangled debug info. Postconditions such as `Contract.Ensures(!Contract.Result<bool>() || (Contract.ValueAtReturn(out rslt) != null && …))` were "inscrutable to the untrained eye" ([Schiller et al.](https://homes.cs.washington.edu/~mernst/pubs/contract-specifications-icse2014.pdf)). Only 90 of 44,440 indexed open-source C# projects used it ([Schiller et al.](https://homes.cs.washington.edu/~mernst/pubs/contract-specifications-icse2014.pdf)). The official successor is "Nullable reference types" ([Microsoft Learn](https://learn.microsoft.com/en-us/dotnet/framework/debug-trace-profile/code-contracts)), a type-level encoding that confirms the first tier above, and the repository was archived in July 2023 ([microsoft/CodeContracts](https://github.com/microsoft/CodeContracts)).

The design lessons are specific. **Contracts must be part of the compiler, not a post-build step. Postconditions need declarative syntax, not library calls in the body. Interface contracts need a real home, not a companion class. A checker must never be required for correctness and must not produce false positives that get it switched off. A contract must never silently compile to nothing.** Placement shows up in only one of these lessons, the separate contract classes. The useful idea in Code Contracts was its ecosystem reach: contract reference assemblies, and a doc generator that "augments existing XML documentation files with contract information" ([Microsoft Learn](https://learn.microsoft.com/en-us/dotnet/framework/debug-trace-profile/code-contracts)). It even recognized leading `if … throw` guards followed by `Contract.EndContractBlock()` as "legacy requires", which is the only mainstream precedent for lifting body code into the public contract.

**C++ shows the opposite failure: syntax everyone accepted, semantics nobody could agree on.** The timeline reconciles the two figures in the notes. P2900R14 entered the C++26 working draft at Hagenberg in February 2025 ([Herb Sutter](https://herbsutter.com/2025/02/)). Kona in November 2025 kept it with "strong consensus" and two fixes, despite "10+ NB comments asking to remove it" ([Herb Sutter, Kona](https://herbsutter.com/2025/11/10/trip-report-november-2025-iso-c-standards-meeting-kona-usa/); [Arthur O'Dwyer](https://quuxplusone.github.io/blog/2025/11/18/kona-trip-report/)). In March 2026 the WG21 plenary approved the finished C++26 draft, contracts included, by **114-12-3**, as reported by The Register and DevClass ([The Register](https://theregister.com/2026/03/31/cplusplus26_approved/); [DevClass](https://www.devclass.com/development/2026/04/01/contracts-are-in-c26-despite-disquiet-over-their-value/5213555)). That vote sent the draft to ISO's national bodies. **The DIS ballot among those national bodies closed on 23 October 2026 and was disapproved.** Only 12 of 20 P-members voted in favour, below the required two-thirds, and 9 of 22 voted No, above the 25% limit. Reportedly it is the first failed C++ DIS ballot. Denmark, Finland, Italy, Japan, Spain, Sweden and others voted No, with comments that were not yet public. The observer expects contracts to be behind them ([Train-IT](https://train-it.eu/blog/cpp26-contracts-the-ballot-result/)). The two numbers are therefore not in conflict. **The committee's experts approved, and the national bodies later rejected.** As of 30 October 2026, C++26 is unpublished. Daveed Vandevoorde favours removing contracts while keeping library hardening, Timur Doumler's P4433R0 argues to keep them ([P4433R0](https://www.open-std.org/jtc1/sc22/wg21/docs/papers/2026/p4433r0.pdf)), and the decision falls to the November 2026 Búzios meeting. One caveat: the 114-12-3 count comes from press summaries the researchers did not fully fetch, and one note found it only on a secondary blog ([youngju.dev](https://www.youngju.dev/transcribe/2026-07-16-cpp26-contracts-shipped-anyway.en)). Treat it as well reported but not confirmed from WG21 minutes.

The objections are about meaning, not placement. Stroustrup and co-authors' P4334R0 calls P2900 a "1980s approach". It objects that the "ignore" semantic "removes the checks exactly where the code runs for real", that implementation-defined choices among ignore, observe, enforce and quick-enforce make behaviour unpredictable, and that constification breaks ordinary predicates. It notes that virtual functions and `old` are deferred to C++29, that GCC 16.1 ships contracts only as experimental and Clang not at all, and that "no shipping production codebase uses pre/post/contract_assert" ([P4334R0](https://www.open-std.org/jtc1/sc22/wg21/docs/papers/2026/p4334r0.pdf)). The same paper cites fleet-wide hardened-library checks at about 0.3% overhead at Google, an argument that cheap precondition checks can stay on in production ([P4334R0](https://www.open-std.org/jtc1/sc22/wg21/docs/papers/2026/p4334r0.pdf)). **The lesson for Mora is to fix one evaluation semantics in the language definition and never make it implementation-defined.** Mora should also support inheritance and `old` from the first version, because these are the gaps C++ is still arguing about.

## Mora can keep the signature clean by letting the compiler reattach contracts

The candidates below use one running example. They assume Mora's accepted features: no null, `newtype` refinements, `where` for generics, `Result` errors, effect inference, derived property tests and `mora diff`. Every candidate keeps `Cents` as a refined newtype, so the unary condition `amount >= 0` never appears as contract text.

```mora
newtype Cents : long is >= 0
```

### 1. Type-level `contract` section with outcome clauses (recommended)

```mora
public class Account {
    public mut(private) Balance: Cents
    public mut(private) Frozen: bool
    public Entries: List<Entry>

    public mut Withdraw(amount: Cents): Result<(), AccountError> {
        Balance -= amount;
        return Ok(());
    }

    public Range(start: int, end: int): Statement {
        return Statement(Entries[start..end])
    }

    contract {
        invariant settled: Frozen implies Entries.IsEmpty

        Withdraw(amount)
            fails .Frozen       when Frozen
            fails .Insufficient when amount > Balance
            ensures Balance == old(Balance) - amount

        Range(start, end)
            requires 0 <= start <= end <= Entries.Count
            ensures result.Count == end - start
    }
}
```

**How it reads.** The `contract` section belongs to the type and lists the type's promises, much like Eiffel's flat contract view, except that the author writes it. Each entry links to a member by name. The parameter list (`Withdraw(amount)`) binds the names used in the clauses and disambiguates overloads. A name that resolves to no member, or to more than one, is a compile error, the same rule Move's prover applies to dangling spec names ([Move](https://github.com/move-language/move/blob/main/language/move-prover/doc/user/spec-lang.md)). Free functions get the same section at file or module level. **There are four clause kinds, each with one fixed meaning.** `requires` is a caller obligation. It is enforced in every build, and a violation panics and blames the caller. The compiler elides the check where it can prove it statically, which is the shape of the hardened-library data point. `fails E when cond` is Gospel's `checks` adapted to `Result`. The compiler generates the guard, returns `Err(E)` before the body runs, and records the case in metadata. That is why `Withdraw`'s body is a single line. `ensures` is a supplier promise. It is checked in debug and test builds and exercised by derived property tests, and it is trusted in release. Clojure makes the same split, instrumenting arguments cheaply and leaving `:ret` and `:fn` to generative tests ([clojure.org](https://clojure.org/guides/spec)). `invariant` holds at entry and exit of public members, as in Eiffel, and its label doubles as the panic message. Preconditions and `fails` conditions may reference only members at least as visible as the function, because "clients cannot validate such a contract before they call the method" ([Microsoft Learn](https://learn.microsoft.com/en-us/dotnet/framework/debug-trace-profile/code-contracts)).

**Interfaces carry contracts, and implementations inherit them.** An implementation may only append `requires else` (OR-ed) or `ensures also` (AND-ed). Liskov substitution then holds by construction, closing the unsafe-subtyping gap Dietrich et al. measured.

```mora
public interface IStack<T> {
    mut Push(item: T)
    mut Pop(): Option<T>
    Count: int

    contract {
        Push(item) ensures Count == old(Count) + 1
        Pop()      ensures (result is none) == (old(Count) == 0)
    }
}
```

**Why it ranks first.** It is the only candidate that leaves both the signature line *and* the body cleaner than they are today. It removes Code Contracts' companion-class boilerplate and Move's need for a separate module, and it gives every consumer one artifact. The compiler serializes the section into package metadata. Hover and code lens render it under the signature (`fails Frozen, Insufficient · ensures Balance == old(Balance) - amount`). `mora doc` prints it. Property tests generate inputs from `Cents` and `requires`, then check `ensures`, the `fails` mapping and invariants, in the style of `stest/check` and Ortac's QCheck-STM ([ortac](https://ocaml.org/p/ortac-runtime-qcheck-stm)). JSON schemas take bounds from newtype refinements and `requires` clauses that compare a single parameter with a constant. AI agents read obligations without reading the body. **`mora diff` gets precise breakage rules.** A strengthened `requires`, a weakened `ensures` or `invariant`, or a new `fails` case is breaking. The reverse changes are compatible, and a change the tool cannot decide is reported for review. **Costs.** The contract sits apart from the method, so a reader must look in two places. The mitigation is tooling (inline code lens), with Eiffel's experience behind it. Renaming a member must rename its contract entry. Generated `fails` guards are control flow outside the body. They are declarative, ordered as written and visible in hover, but they are new. A large type's section can grow long, so Mora should allow more than one `contract` section per type, letting an author place an entry next to its member when they prefer.

### 2. Checked contract lines in doc comments

```mora
/// Withdraws money from the account.
/// fails Frozen when Frozen
/// fails Insufficient when amount > Balance
/// ensures Balance == old(Balance) - amount
public mut Withdraw(amount: Cents): Result<(), AccountError> {
    Balance -= amount;
    return Ok(());
}
```

The compiler parses contract lines in `///` comments as real syntax, with the same clause semantics as candidate 1. It keeps the contract next to the member, where readers already look for API documentation, and it unifies docs and contracts, which addresses the @tComment drift. **Costs.** The contract sits *above* the signature, which is the reading-order objection P2961 made against attributes. Code in comments looks optional, and Ada moved SPARK contracts out of comments into language aspects precisely so ordinary compilers would check them ([Wikipedia: SPARK](https://en.wikipedia.org/wiki/SPARK_(programming_language))). Formatters, refactorings and syntax highlighting all have to treat these comments as code. It is a reasonable second choice and a natural *rendering* of candidate 1 in generated docs. As a source of truth it is weaker.

### 3. Companion contract file

```mora
// account.contract.mora
contract for Account {
    Withdraw(amount) fails .Insufficient when amount > Balance
}
```

This is the pattern of Ada `.ads`, F* `.fsti`, JML `.jml` and Liquid Haskell's `_LHAssumptions` modules ([LiquidHaskell](https://ucsd-progsys.github.io/liquidhaskell/specifications/)). It leaves source files untouched. **Costs.** There are two files per unit and drift between them, it is the strongest form of the "look elsewhere" problem, and in Code Contracts' interface version it was friction. **Keep it for one job: specifying code you do not own**, such as FFI bindings and pinned third-party packages, the role Prusti's and Creusot's `extern_spec` play.

### 4. Boundary contracts on the module's export surface

```mora
// in a folder's public surface declaration
export Account.Withdraw contract fails .Insufficient when amount > Balance
```

Racket proves that boundary-only checking keeps internal calls fast and gives precise blame ([Racket Guide](https://docs.racket-lang.org/guide/contract-boundaries.html)). As a *placement* it scatters a type's contract across export lists, and Mora's folder-default visibility has no single export file to hold it. **Adopt the idea as a checking policy for candidate 1, not as a syntax.** Where a `requires` cannot be proven, the check runs at the callee for cross-folder and cross-package calls. Inside a folder it is trusted in release builds, and blame always names the caller.

### 5. Attributes above the signature

```mora
[Requires(amount <= Balance)]
[Ensures(Balance == old(Balance) - amount)]
public mut Withdraw(amount: Cents): Result<(), AccountError> { ... }
```

This is cheap to add to the grammar, and Rust's tools and Python's decorators all use it. P2961's critique applies directly. Attributes are conventionally ignorable, they come before the reader has seen the parameter names, and stacks of brackets bury the clauses ([P2961R2](https://www.open-std.org/jtc1/sc22/wg21/docs/papers/2023/p2961r2.pdf)). It moves clutter upward instead of removing it.

### 6. Prologue-lifted guards

```mora
public mut Withdraw(amount: Cents): Result<(), AccountError> {
    require amount <= Balance else .Insufficient;
    Balance -= amount;
    return Ok(());
}
```

This exports leading guard statements as the public contract, a reliable version of Code Contracts' "legacy requires". The user already rejected in-body `require` statements, and the evidence agrees on one point: the contract becomes visible only through tools, and the boundary between "contract" and "implementation" depends on statement order. **Use it as a migration path instead.** An IDE quick-fix turns an existing leading guard into a `fails` entry in the contract section.

### 7. Inferred contracts as inlay hints

The compiler can suggest candidate preconditions from indexing, division and unwrapping, in the way Daikon reports "likely program invariants" ([Daikon](https://plse.cs.washington.edu/daikon/download/doc/daikon.html)). One action promotes a hint into the contract section. **Inferred contracts must never become API on their own**, because a body edit would then silently become a breaking change for `mora diff`. This is tooling that feeds candidate 1, not a language placement.

### Ranking

| Rank | Candidate | Signature clean | Body clean | Visible without tools | Machine-readable | Main risk |
|---|---|---|---|---|---|---|
| 1 | Type-level `contract` section with `fails`/`requires`/`ensures`/`invariant` | Yes | Cleaner than today (guards removed) | Same file, one section | Compiler metadata | Distance from the member; generated guards are new control flow |
| 2 | Checked doc-comment contract lines | Yes | Yes | Directly above member | Yes | Code-in-comments ambiguity; reading order |
| 3 | Companion contract file | Yes | Yes | No | Yes | Two-file drift; use for FFI and third-party code only |
| 4 | Boundary contracts on the export surface | Yes | Yes | Scattered | Yes | No single export site in Mora; adopt as checking policy |
| 5 | Attributes above signature | Shifted upward | Yes | Yes | Yes | Ignorable-looking, buries clauses |
| 6 | Prologue-lifted guards | Yes | No | Only via tools | Yes | Rejected; keep as quick-fix |
| 7 | Inferred hints | Yes | Yes | IDE only | Advisory | Must never become API |

**The recommendation is candidate 1 with candidate 3 for code Mora does not own, candidate 4 as the release-build checking policy, and candidates 6 and 7 as IDE tooling that writes into the contract section.** Three decisions need prototyping before this goes into the spec. The first is whether `fails` clauses run before or after `requires`. The proposal is in declared order, all before the body. The second is how `mora diff` decides implication between changed clauses. A syntactic rule set (added conjunct, narrowed constant) covers most real changes, since Estler et al. found most changes are clause additions, and anything else is reported for review. The third is whether `ensures` should also be enforced in release builds at package boundaries. The evidence on overhead covers preconditions only, so leave that as a build option with one documented default rather than an implementation-defined mode.

## Conclusion

The placement debate has been framed as a choice of position: before, after, inside or beside the signature. The evidence moves the question elsewhere. Placement mattered far less than whether a contract was *cheap to write, impossible to silently disable, and read by tools other than its own checker*. Code Contracts failed every one of those tests while sitting in the method body. C++26 passed the syntax test and is failing the semantics test. Clojure's spec and Move's spec blocks sit far from the code and work, because the language resolves the link and many consumers reuse the result. Once Mora's types absorb the presence checks that make up most real contracts, the remaining contract text is small enough to live in one named section per type without readers feeling its distance.

The idea that is new to Mora is the `fails ... when` clause. Mainstream contract systems treat validation that throws (Java's `IllegalArgumentException`, .NET's legacy requires) as a lesser, defensive copy of the "real" precondition. In a `Result`-based language that validation *is* the public contract. Declaring it once lets the compiler generate the guard, the doc entry, the test oracle, the error-case list in JSON and the breakage rule in `mora diff`. Contracts then remove code from the body instead of adding text to the signature, which addresses the adoption gap the studies keep finding: developers rarely write specifications that cost them extra lines.
