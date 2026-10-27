# Mora

[![Coverage](https://img.shields.io/codecov/c/github/wolfware-labs/mora?logo=codecov&label=coverage)](https://codecov.io/gh/wolfware-labs/mora)

Mora is a programming language that reads and writes like modern C#, manages memory with reference counting and single ownership instead of a garbage collector, and compiles to one native executable. The compiler, runtime, language server and package tool are written in Rust.

```mora
Console.WriteLine("Hello, world!");
```

## Status

Mora is in design and early development. Nothing here is ready to install.

- [Specification](docs/design/mora.md): the language, its memory model, runtime, standard library and toolchain, as the MVP is built against them.
- [Roadmap](ROADMAP.md): milestones M0 to M13, with every MVP deliverable tracked as a [GitHub issue](https://github.com/wolfware-labs/mora/issues).

The MVP is done when a REST API with JSON bodies, written in Mora, builds into a native executable on Linux, macOS and Windows, and a C# developer can install the toolchain with one command and get diagnostics in VS Code.

## Contributing

See [CONTRIBUTING.md](CONTRIBUTING.md). Contributions require signing the [Contributor Licence Agreement](CLA.md), and security problems go through [SECURITY.md](SECURITY.md).

## License

Licensed under either of [Apache License, Version 2.0](LICENSE-APACHE) or [MIT license](LICENSE-MIT) at your option.
