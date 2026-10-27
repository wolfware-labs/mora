# Error code pages

Every diagnostic code has an entry in the registry, `crates/mora-diag/src/registry.rs`. A code that the compiler reports, a UI test expects or an example names also has a page here, `MRnnnn.md`, that explains it to a C# developer. `mora explain MRnnnn` prints the page, and §17.6 of the spec describes the rules.

## Adding a code

1. Pick the next free number in the range for its kind. The ranges are in §17.6 and in `RANGES` in the registry.
2. Name the code in the spec, in the section that defines the rule it enforces.
3. Add it to the `registry!` list in `crates/mora-diag/src/registry.rs`, in numeric order, with a one-line summary in lower case.
4. Report it from the compiler as `mora_diag::codes::MRnnnn`.
5. Add the page below, named after the code.
6. Run `cargo xtask error-codes`.

The check fails when a used code has no page, a page has no registered code, a code sits outside its range, or the registry and the spec disagree.

## Page template

The page has a title, a short explanation, and three sections in this order. The failing example's first line names the code it must report (`// error:` for errors, `// warning:` for warnings and lints). The fixed example's first line gives the output it must print. The test suite compiles both (§20.4).

````markdown
# MRnnnn: one-line summary, as in the registry

What the compiler found and why Mora rejects it, in two or three sentences.
Say what would go wrong at run time, not which rule was broken.

## Coming from C#

What a C# developer would write here, why it works in C#, and what Mora
does instead.

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
