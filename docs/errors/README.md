# Error code pages

Every diagnostic code has an entry in the registry, `crates/mora-diag/src/registry.rs`. A code that the compiler reports, a UI test expects or an example names also has a page here, `MRnnnn.md`, that explains it. §17.6 of the spec describes the rules. `mora explain MRnnnn` will print the page once the command exists (M7).

## Adding a code

1. Pick the next free number in the range for its kind. The ranges are in §17.6 and in `RANGES` in the registry.
2. Name the code in the spec, in the section that defines the rule it enforces.
3. Add it to the `registry!` list in `crates/mora-diag/src/registry.rs`, in numeric order, with a one-line summary in lower case.
4. Report it from the compiler as `mora_diag::codes::MRnnnn`, written out in full at each use. The check rejects `Code::new`, braced imports and glob imports of `codes` outside `mora-diag`, because they hide uses.
5. Add the page below, named after the code.
6. Run `cargo xtask error-codes`.

The check fails when a used code has no page, a page has no registered code, a code sits outside its range, the registry and the spec disagree, or a crate names a code in a way the check cannot follow.

## Page template

The page has a title, a short explanation, and two sections in this order. The failing example's first line names the code it must report (`// error:` for errors, `// warning:` for warnings and lints). The fixed example's first line gives the output it must print. `cargo xtask error-codes` checks these first lines and the layout; `cargo xtask spec-examples` compiles and runs both examples (§20.4); while the compiler cannot build them yet, list their ids (`MRnnnn#1`, `MRnnnn#2`) in `tests/spec-examples.pending`.

````markdown
# MRnnnn: one-line summary, as in the registry

What the compiler found and why Mora rejects it, in two or three sentences.
Say what would go wrong at run time, not which rule was broken.

What to write instead, in a short paragraph. Mention another language only
when that is where the habit comes from.

## Failing example

```mora
// error: MRnnnn
...
```

## Fixed example

```mora
// expect: ...
...
```
````
