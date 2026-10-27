# UI tests

Each `.mr` file here is one case (§20.1). Mark every line that must report a diagnostic with its severity and code after the code on that line:

```mora
let b = a;
Use(a); //~ ERROR MR0300
```

A line can carry several annotations. Text after the code is a comment. The harness finds `//~` anywhere on a line, inside a string literal or a block comment too, because it reads lines rather than tokens; spell such text another way, for example `"/" + "/~"`. A diagnostic without an annotation fails the case, and so does an annotation that nothing reported.

The rendered output is in the `.stderr` file beside the case. A case that reports nothing has none. After a change to the output, update the snapshots and review the diff:

```sh
cargo test -p mora-testkit --test ui -- --bless [filter...]
```

Filters run only the cases whose path contains one of them.
