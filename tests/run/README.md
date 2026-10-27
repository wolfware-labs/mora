# Run tests

Each `.mr` file here is a program (§20.1). Its `// expect:` lines give its standard output, one line each, and an optional `// exit:` line gives its exit code, 0 by default:

```mora
// expect: 3
Console.WriteLine(1 + 2);
```

The harness builds and runs the program through every backend the compiler has, the JIT and an AOT build, and compares each run with the expected output.

```sh
cargo test -p mora-testkit --test run -- [filter...]
```
