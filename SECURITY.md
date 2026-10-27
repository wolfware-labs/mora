# Security policy

## Supported versions

Mora has no release yet. Security fixes land on `main`, and in the latest published version once releases start.

## Reporting a vulnerability

Report vulnerabilities privately through GitHub: open the repository's **Security** tab and choose **Report a vulnerability**. Do not open a public issue for a security problem.

A useful report includes:

- the affected version or commit,
- the platform and target,
- a minimal Mora program or project that reproduces the problem,
- the impact you expect.

You can expect a first reply within 7 days. Once a fix is ready, the advisory is published together with the release that contains it, and reporters are credited unless they ask not to be.

## Scope

**In scope:**

- Safe Mora code that breaks the memory-safety guarantees of [§10.14 of the specification](docs/design/mora.md#1014-soundness-summary): a use after free, a double free, a data race, or reading memory a value does not own, without FFI and outside the standard library's private `unsafe` dialect. These are compiler or runtime bugs, and we treat them as security issues.
- A macro escaping its sandbox ([§9.7](docs/design/mora.md#97-running-macros)): reading files, the network, the environment, the clock or randomness during a build.
- The HTTP server in the standard library ([§16.1](docs/design/mora.md#161-http-server-stdnethttp)): request smuggling, a crash or panic caused by a request, or memory use beyond the documented limits and body budget.
- The toolchain distribution ([§19.3](docs/design/mora.md#193-installation)): install scripts, release archives, checksums and signatures, and `mora self update`.

**Out of scope:**

- Problems caused by `extern` functions and other FFI, which are trusted by design ([§13.9](docs/design/mora.md#139-foreign-functions)).
- Denial of service through the compiler by a program the user chose to build, such as one that takes a long time to type-check.

Bugs in third-party dependencies should be reported upstream, but let us know if Mora is affected.
