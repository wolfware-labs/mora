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

## Projects and several files

A case is the entry file, `src/Program.mr`, of an app named `app`, whose root namespace is `App`. Lines that start with `//@` change that:

```mora
// Text before the first file line may only be comments.
//@ project kind=lib name=users-api
//@ file src/Store/UserStore.mr
internal class UserStore {
}
//@ file src/Program.mr
using UsersApi.Store.UserStore;
```

- `//@ project` sets the manifest values `kind` (`app` or `lib`), `name`, `namespace` and `entry`, each optional, as `key=value` pairs. It comes before the first `//@ file` line.
- `//@ file` starts a file at the path after it, which starts with `src/`. The lines after it, up to the next `//@ file` line, are its text, so its line numbers start after the `//@ file` line.

Annotations belong to the file they are in, and the snapshot shows each file under its own path. A case without `//@ file` lines is shown under the case's path.
