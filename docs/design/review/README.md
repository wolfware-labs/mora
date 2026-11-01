# Design review

In October 2026 the author reviewed Mora's design in three studies, each measured against the v0.1 draft of [the specification](../mora.md). Together they cover 104 items, and every item was decided on 2026-10-31.

| Study | Question | Items |
|---|---|---|
| [Inherited design debt](debt.md) | Where does Mora copy its inspiration out of habit, and is a cleaner design worth the change? | 37 |
| [The language of the future](future.md) | Which patterns do all mainstream languages still handle badly, and what makes Mora a good language when much code is written and reviewed with AI agents? | 36 |
| [Fast by default](performance.md) | How can the compiler and runtime make natural Mora code fast and lean with no tuning, and which of those wins can Mora publish as guarantees? | 31 |

Three research reports back individual decisions:

- [Computed getters and hidden costs](research/computed-getters.md), behind the state model of debt item 6.
- [Contracts placement](research/contracts-placement.md), behind the `contract` sections of future item 7.
- [Stable plugin ABI](research/stable-plugin-abi.md), behind future item 36.

## Reading the verdicts

Each item ends with a verdict:

| Verdict | Meaning |
|---|---|
| Change | Replace the current design with the proposal |
| Keep | The current design stays as it is |
| Adopt for MVP | Build it before v0.1 |
| Adopt post-MVP | Build it after v0.1; the MVP must not rule it out |
| Explore | Worth a prototype or more research before deciding |
| Reject | Not built, with the reason recorded |

A verdict marked "Decided" in the text records the author's final call where it differs from the first proposal.

## The specification wins

The studies are a record of the reasoning, not the specification. Where a study disagrees with [`mora.md`](../mora.md) or its decisions register, the specification is right. Decisions that have reached the specification so far:

| Decision | Summary |
|---|---|
| D-29 | `Dictionary` and `HashSet` iterate in insertion order (widened to `HashSet` after the review) |
| D-54 | The empty option is written `none` |
| D-55 | Raw strings are written with backticks |
| D-56 | Types are declared as `struct` (a value) or `class` (identity); `record` is removed |
| D-57 | A type without a constructor is created memberwise with `new T { ... }` |
| D-58 | Declarations put the name first: `Balance: decimal`, `Deposit(amount: decimal): decimal` |
| D-59 | The compiler knows only Mora syntax |
| D-60 | The MVP runtime is thread-ready, not threaded |
| D-61 | Values derived from object identity come from a per-process counter, never from an address |
| D-62 | `newtype` declares a new value type with an optional pattern refinement |
| D-63 | Contracts live in `contract` sections linked to members by name |
| D-64 | Unit tests live in `test` sections or sibling `.tests.mr` files |

D-59 overrides every passage in the studies that proposes a dedicated error code, note or fix for another language's syntax. Code written in another language gets the ordinary syntax error.

The migration guide that some passages mention was dropped. The documentation keeps the install guide, the language tour, the standard library reference, the error pages and the macro guide.
