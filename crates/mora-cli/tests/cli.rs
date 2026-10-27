#![allow(clippy::unwrap_used)]
// Helpers below unwrap too, and `allow-unwrap-in-tests` covers only test functions.

//! Runs the `mora` executable as a user would (§20.1, CLI layer).

use std::process::{Command, Output};

fn mora(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_mora"))
        .args(args)
        .env_remove("NO_COLOR")
        .output()
        .unwrap()
}

fn stdout(output: &Output) -> String {
    String::from_utf8(output.stdout.clone()).unwrap()
}

fn stderr(output: &Output) -> String {
    String::from_utf8(output.stderr.clone()).unwrap()
}

/// The MVP commands of §17.1.
const COMMANDS: &[&str] = &[
    "new", "build", "run", "check", "test", "fmt", "repl", "explain", "lsp", "demangle", "version",
];

#[test]
fn help_lists_every_command_and_global_option() {
    let output = mora(&["--help"]);
    assert_eq!(output.status.code(), Some(0));
    let help = stdout(&output);
    for command in COMMANDS {
        assert!(
            help.lines()
                .any(|line| line.trim_start().starts_with(command)),
            "{command} missing from:\n{help}"
        );
    }
    assert!(help.contains("--color <COLOR>"), "{help}");
    assert!(help.contains("--message-format <MESSAGE_FORMAT>"), "{help}");
}

#[test]
fn version_prints_the_version_commit_and_runtime_targets() {
    let output = mora(&["version"]);
    assert_eq!(output.status.code(), Some(0));
    let text = stdout(&output);
    let lines: Vec<&str> = text.lines().collect();
    assert_eq!(lines[0], format!("mora {}", env!("CARGO_PKG_VERSION")));
    let commit = lines[1].strip_prefix("commit: ").unwrap();
    assert!(!commit.is_empty(), "{text}");
    // Under target/, beside no toolchain lib/, nothing is installed.
    assert_eq!(lines[2], "runtime targets: none");
}

#[test]
fn version_in_json_is_one_object() {
    let output = mora(&["version", "--message-format", "json"]);
    assert_eq!(output.status.code(), Some(0));
    let text = stdout(&output);
    let line = text.trim_end();
    assert!(!line.contains('\n'), "{text}");
    assert!(
        line.starts_with(&format!(
            "{{\"version\":\"{}\",\"commit\":\"",
            env!("CARGO_PKG_VERSION")
        )),
        "{text}"
    );
    assert!(line.ends_with(",\"runtime_targets\":[]}"), "{text}");
}

#[test]
fn an_unknown_command_is_a_usage_error() {
    let output = mora(&["frobnicate"]);
    assert_eq!(output.status.code(), Some(2));
    assert!(stderr(&output).contains("unrecognized subcommand 'frobnicate'"));
}

#[test]
fn post_mvp_commands_are_unknown() {
    for command in ["fetch", "publish", "caps"] {
        assert_eq!(mora(&[command]).status.code(), Some(2), "{command}");
    }
}

#[test]
fn a_bad_option_value_is_a_usage_error() {
    let output = mora(&["--color", "purple", "version"]);
    assert_eq!(output.status.code(), Some(2));
    assert!(stderr(&output).contains("invalid value 'purple'"));
}

#[test]
fn commands_not_built_yet_say_so_and_exit_2() {
    let calls: &[&[&str]] = &[
        &["new", "app"],
        &["build", "--release", "--emit", "mir"],
        &["run", "--", "arg"],
        &["check"],
        &["test", "parses"],
        &["fmt", "--check"],
        &["repl"],
        &["explain", "MR0310"],
        &["lsp"],
        &["demangle", "_M3Foo"],
    ];
    for args in calls {
        let output = mora(args);
        assert_eq!(output.status.code(), Some(2), "{args:?}");
        assert_eq!(
            stderr(&output),
            format!("error: `mora {}` is not implemented yet\n", args[0]),
            "{args:?}"
        );
    }
}

#[test]
fn color_follows_the_option() {
    let colored = stderr(&mora(&["--color", "always", "check"]));
    assert!(
        colored.starts_with("\u{1b}[1;31merror\u{1b}[0m:"),
        "{colored:?}"
    );
    let plain = stderr(&mora(&["--color", "never", "check"]));
    assert!(!plain.contains('\u{1b}'), "{plain:?}");
    // Not a terminal: auto means no color.
    let auto = stderr(&mora(&["check"]));
    assert!(!auto.contains('\u{1b}'), "{auto:?}");
}
