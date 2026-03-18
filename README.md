# raxx

A dax-inspired shell scripting library for Rust. Synchronous, Unix-only.

Inspired by [dax](https://github.com/dsherret/dax) for Deno. Provides ergonomic macros and a builder API for running shell commands with safe argument escaping, piping, chaining, IO redirection, environment control, and more.

## Quick Start

```toml
[dependencies]
raxx = "0.1"
```

```rust
use raxx::{cmd, shell};

fn main() -> raxx::Result<()> {
    // Run a simple command
    cmd!("echo", "hello world").run()?;

    // Capture output as a string
    let text = cmd!("echo", "hello").text()?;
    assert_eq!(text, "hello");

    // Use shell syntax for pipes and redirects
    let text = shell!("echo hello | tr a-z A-Z").text()?;
    assert_eq!(text, "HELLO");

    Ok(())
}
```

## Two Macros

### `cmd!` — Safe commands

The `cmd!` macro builds a command **without** invoking a shell. The first argument is split on whitespace into the program and literal arguments. Additional arguments are appended as-is — no shell interpretation, no glob expansion, no variable substitution. This makes it safe for user-provided input.

```rust
use raxx::cmd;

// Simple
cmd!("echo", "hello").run()?;

// First arg is split on whitespace
cmd!("grep -rn", "pattern", "src/").run()?;

// Variables are safe — spaces, quotes, globs are passed literally
let user_input = "Robert'); DROP TABLE students;--";
cmd!("echo", user_input).run()?;  // prints the string literally
```

### `shell!` — Full shell syntax

The `shell!` macro passes the string to `/bin/sh -c`, giving you pipes, redirects, variables, loops, and everything else the shell supports.

```rust
use raxx::shell;

shell!("echo hello | tr a-z A-Z").run()?;
shell!("for i in 1 2 3; do echo $i; done").run()?;
shell!("VAR=hello && echo $VAR").run()?;
```

## Vector Arguments

Both macros accept vectors (or slices) as arguments. Each element becomes a separate argument. For `shell!`, each element is automatically shell-escaped, so user input and filenames with spaces are safe.

```rust
use raxx::{cmd, shell};

// Pass a vec — each element becomes a separate argument
let files = vec!["file1.txt", "file2.txt", "file3.txt"];
cmd!("cat", files).run()?;
// equivalent to: cat file1.txt file2.txt file3.txt

// Works with shell! too — elements are shell-escaped
let flags = vec!["--verbose", "--color=always"];
shell!("ls", flags, "/tmp").run()?;
// equivalent to: /bin/sh -c "ls --verbose --color=always /tmp"

// Mix scalars and vectors freely
let extra = vec!["-l", "-a"];
cmd!("ls", extra, "/tmp").run()?;

// Great for forwarding part of argv
let user_args: Vec<String> = std::env::args().skip(1).collect();
cmd!("grep", user_args).run()?;

// Special characters in vectors are safe — no injection
let evil = vec!["hello; rm -rf /", "$(whoami)"];
shell!("echo", evil).run()?;
// prints literally: hello; rm -rf / $(whoami)
```

## Glob

Find files matching `*`, `**`, `?`, and `[...]` patterns. Results are sorted.

```rust
use raxx::{cmd, glob, shell};

// All .txt files in a directory
let files = glob("docs/*.txt")?;

// Recursive — all .rs files under src/
let files = glob("src/**/*.rs")?;

// Use glob results with cmd!
let files: Vec<String> = glob("src/**/*.rs")?
    .into_iter()
    .map(|p| p.to_string_lossy().to_string())
    .collect();
cmd!("wc", "-l", files).run()?;

// Use glob results with shell!
let files: Vec<String> = glob("tests/**/*.rs")?
    .into_iter()
    .map(|p| p.to_string_lossy().to_string())
    .collect();
shell!("cat", files).pipe(cmd!("wc", "-l")).run()?;
```

## Capturing Output

```rust
use raxx::cmd;

// Trimmed string
let text = cmd!("echo", "hello").text()?;

// Lines as Vec<String>
let lines = cmd!("echo", "a\nb\nc").lines()?;

// Raw bytes
let bytes = cmd!("echo", "hello").bytes()?;

// Parse as JSON (requires the value to implement serde::Deserialize)
let val: serde_json::Value = cmd!("echo", r#"{"key": "value"}"#).json()?;

// Exit code without throwing
let code = cmd!("false").status_code()?;
assert_eq!(code, 1);

// Full output with both streams
let output = cmd!("echo", "hello")
    .stdout_capture()
    .stderr_capture()
    .output()?;
println!("code: {}", output.code);
println!("stdout: {}", output.stdout_text()?);
println!("stderr: {}", output.stderr_text()?);
```

## Piping

Chain commands together with `.pipe()`. Each command's stdout feeds into the next command's stdin.

```rust
use raxx::cmd;

let text = cmd!("echo", "hello world")
    .pipe(cmd!("tr", "a-z", "A-Z"))
    .text()?;
assert_eq!(text, "HELLO WORLD");

// Multi-stage pipelines
let text = cmd!("echo", "3\n1\n2")
    .pipe(cmd!("sort"))
    .pipe(cmd!("head", "-n", "1"))
    .text()?;
assert_eq!(text, "1");
```

## Chaining

Compose commands with boolean logic, like `&&`, `||`, and `;` in the shell.

```rust
use raxx::cmd;

// && — run second only if first succeeds
cmd!("true").and(cmd!("echo", "success")).run()?;

// || — run second only if first fails
let text = cmd!("false")
    .or(cmd!("echo", "fallback"))
    .text()?;
assert_eq!(text, "fallback");

// ; — run second regardless
cmd!("false")
    .then(cmd!("echo", "always runs"))
    .run()?;

// Complex chains
let text = cmd!("false")
    .and(cmd!("echo", "a"))
    .or(cmd!("echo", "b"))
    .text()?;
assert_eq!(text, "b");
```

## Environment Variables

```rust
use raxx::{cmd, shell};

// Set a single variable
shell!("echo $MY_VAR")
    .env("MY_VAR", "hello")
    .run()?;

// Set multiple variables
shell!("echo $A $B")
    .envs([("A", "1"), ("B", "2")])
    .run()?;

// Remove a variable
cmd!("env")
    .env("VAR", "set")
    .env_remove("VAR")
    .run()?;

// Start with a clean environment
cmd!("env")
    .env_clear()
    .env("ONLY_THIS", "yes")
    .run()?;
```

## Working Directory

```rust
use raxx::cmd;

cmd!("ls").cwd("/tmp").run()?;
cmd!("cat", "relative/file.txt").cwd("/my/project").run()?;
```

An error is returned if the directory does not exist.

## Stdin

```rust
use raxx::cmd;

// From a string
let text = cmd!("cat").stdin_text("hello").text()?;

// From bytes
let text = cmd!("cat").stdin_bytes(b"hello".to_vec()).text()?;

// From a file
let text = cmd!("cat").stdin_path("input.txt").text()?;

// No stdin (/dev/null)
cmd!("cat").stdin_null().run()?;
```

## Stdout Redirection

```rust
use raxx::cmd;

// Write to a file (overwrite)
cmd!("echo", "hello").stdout_path("output.txt").run()?;

// Append to a file
cmd!("echo", "more").stdout_append("output.txt").run()?;

// Discard
cmd!("echo", "quiet").stdout_null().run()?;

// Redirect stdout to stderr
cmd!("echo", "to stderr").stdout_to_stderr().run()?;

// Capture into memory (used internally by .text(), .bytes(), etc.)
let output = cmd!("echo", "hello").stdout_capture().output()?;
assert_eq!(output.stdout_text()?, "hello");
```

## Stderr Redirection

```rust
use raxx::{cmd, shell};

// Write to a file
shell!("echo err >&2").stderr_path("errors.txt").run()?;

// Append
shell!("echo err >&2").stderr_append("errors.txt").run()?;

// Discard
shell!("echo err >&2").stderr_null().run()?;

// Merge stderr into stdout
let output = shell!("echo out; echo err >&2")
    .stderr_to_stdout()
    .stdout_capture()
    .output()?;
let text = output.stdout_text()?;
assert!(text.contains("out") && text.contains("err"));

// Capture stderr
let output = shell!("echo err >&2")
    .stderr_capture()
    .output()?;
assert_eq!(output.stderr_text()?, "err");
```

## Error Handling

By default, commands throw `CmdError::ExitStatus` on non-zero exit codes.

```rust
use raxx::{cmd, shell, CmdError};

// Default: error on non-zero
let err = cmd!("false").run().unwrap_err();

// Suppress all non-zero errors
let output = cmd!("false").no_throw().output()?;
assert_eq!(output.code, 1);

// Suppress specific exit codes
let output = shell!("exit 42").no_throw_on(&[42]).output()?;
assert_eq!(output.code, 42);

// Just get the exit code (never throws)
let code = cmd!("false").status_code()?;
assert_eq!(code, 1);

// Match on error types
match cmd!("nonexistent_program").run() {
    Err(CmdError::NotFound { program }) => {
        eprintln!("not found: {program}");
    }
    Err(CmdError::ExitStatus { code, stderr }) => {
        eprintln!("exited {code}");
    }
    Err(CmdError::Timeout { duration }) => {
        eprintln!("timed out after {:?}", duration);
    }
    Err(CmdError::CwdNotFound { path }) => {
        eprintln!("bad cwd: {path}");
    }
    Err(other) => eprintln!("{other}"),
    Ok(()) => {}
}
```

## Timeout

```rust
use raxx::cmd;
use std::time::Duration;

cmd!("sleep", "100")
    .timeout(Duration::from_secs(2))
    .run()?; // returns CmdError::Timeout
```

## Quiet Mode

Suppress output without capturing it.

```rust
use raxx::cmd;

cmd!("echo", "shh").quiet().run()?;           // suppress both
cmd!("echo", "shh").quiet_stdout().run()?;     // suppress stdout only
cmd!("echo", "shh").quiet_stderr().run()?;     // suppress stderr only
```

## Spinner with Tail (`run_with_tail`)

Run a command with a live spinner that streams the last N lines of stdout. Great for long-running builds, deploys, or tests where you want progress feedback without flooding the terminal.

```rust
use raxx::{cmd, shell, TailOptions};

// Simple usage: title, done message, number of tail lines
cmd!("cargo", "build", "--release")
    .run_with_tail("Building...", "Build complete", 5)?;

// With environment and cwd
cmd!("make", "-j8")
    .cwd("./project")
    .env("CC", "clang")
    .run_with_tail("Compiling...", "Compiled", 3)?;

// Full control with TailOptions
cmd!("cargo", "test")
    .run_with_tail_opts(
        TailOptions::new("Testing...", "All tests passed")
            .lines(10)
            .spinner("⠋⠙⠹⠸⠼⠴⠦⠧⠇⠏")
            .tick_ms(100),
    )?;
```

While running, the terminal shows:

```
  ◐  Building...
     Compiling raxx v0.1.0
     Compiling serde v1.0.228
     Compiling serde_json v1.0.149
```

On success, the spinner clears and prints the done message. On failure, returns `CmdError::ExitStatus` with captured stderr.

## Builder API

Every method on `Cmd` consumes `self` and returns a new `Cmd`, so you can chain freely. You can also use `Cmd::new` and `Cmd::parse` directly instead of macros.

```rust
use raxx::Cmd;

let text = Cmd::new("echo")
    .arg("hello")
    .arg("world")
    .text()?;

let text = Cmd::parse("grep -rn")
    .arg("pattern")
    .arg("src/")
    .cwd("/my/project")
    .text()?;
```

## Argument Escaping

The `cmd!` macro passes arguments directly to the process without shell interpretation. The `escape_arg` function is available for manual escaping when building shell strings.

```rust
use raxx::escape_arg;

assert_eq!(escape_arg("hello"), "hello");
assert_eq!(escape_arg("hello world"), "'hello world'");
assert_eq!(escape_arg(""), "''");
assert_eq!(escape_arg("it's"), "'it'\"'\"'s'");
```

## CmdOutput

The `.output()` method returns a `CmdOutput` struct with full access to exit code and captured streams.

| Method | Returns |
|---|---|
| `output.code` | `i32` — exit code |
| `output.stdout` | `Vec<u8>` — raw stdout bytes |
| `output.stderr` | `Vec<u8>` — raw stderr bytes |
| `output.stdout_text()` | `Result<String>` — trimmed |
| `output.stderr_text()` | `Result<String>` — trimmed |
| `output.stdout_text_raw()` | `Result<String>` — not trimmed |
| `output.stderr_text_raw()` | `Result<String>` — not trimmed |
| `output.stdout_json::<T>()` | `Result<T>` — JSON parse |
| `output.stderr_json::<T>()` | `Result<T>` — JSON parse |
| `output.stdout_lines()` | `Result<Vec<String>>` |
| `output.stderr_lines()` | `Result<Vec<String>>` |
| `output.success()` | `bool` — true if code == 0 |

## CmdError

| Variant | When |
|---|---|
| `ExitStatus { code, stderr }` | Non-zero exit code |
| `Signal { signal }` | Killed by signal |
| `Timeout { duration }` | Command timed out |
| `Io(io::Error)` | IO failure |
| `Utf8(FromUtf8Error)` | Output wasn't valid UTF-8 |
| `Json(serde_json::Error)` | JSON parse failed |
| `NotFound { program }` | Program not in PATH |
| `CwdNotFound { path }` | Working directory doesn't exist |
| `BrokenPipe { upstream_code }` | Upstream pipe command failed |

`CmdError` implements `Display`, `Debug`, `Error`, and `From` conversions for `io::Error`, `FromUtf8Error`, and `serde_json::Error`.

## Full API Summary

### Macros

| Macro | Description |
|---|---|
| `cmd!("prog", arg1, arg2)` | Safe command — no shell, args passed directly |
| `cmd!("prog", vec)` | Vector elements are flattened into separate args |
| `shell!("cmd string")` | Shell command via `/bin/sh -c` |
| `shell!("cmd", arg1, vec)` | Extra args are shell-escaped and appended |

### Functions

| Function | Description |
|---|---|
| `glob("pattern")` | Find files matching `*`, `**`, `?`, `[...]` patterns |
| `escape_arg("str")` | Shell-escape a string |

### `Cmd` Methods

| Category | Method | Description |
|---|---|---|
| **Create** | `Cmd::new(program)` | New command with program name |
| | `Cmd::parse("prog arg1 arg2")` | Split string on whitespace |
| | `Cmd::shell("shell string")` | Via `/bin/sh -c` |
| **Args** | `.arg(val)` | Append one argument |
| | `.args(iter)` | Append multiple arguments from iterator |
| | `.push_args(val_or_vec)` | Append scalar or flatten vector |
| **Env** | `.env(key, val)` | Set env var |
| | `.envs(pairs)` | Set multiple env vars |
| | `.env_remove(key)` | Remove env var |
| | `.env_clear()` | Clear all env vars |
| **Cwd** | `.cwd(path)` | Set working directory |
| **Stdin** | `.stdin_text(str)` | Stdin from string |
| | `.stdin_bytes(bytes)` | Stdin from bytes |
| | `.stdin_path(path)` | Stdin from file |
| | `.stdin_null()` | No stdin |
| **Stdout** | `.stdout_capture()` | Capture to memory |
| | `.stdout_path(path)` | Write to file |
| | `.stdout_append(path)` | Append to file |
| | `.stdout_null()` | Discard |
| | `.stdout_to_stderr()` | Redirect to stderr |
| **Stderr** | `.stderr_capture()` | Capture to memory |
| | `.stderr_path(path)` | Write to file |
| | `.stderr_append(path)` | Append to file |
| | `.stderr_null()` | Discard |
| | `.stderr_to_stdout()` | Redirect to stdout |
| **Behavior** | `.no_throw()` | Don't error on non-zero exit |
| | `.no_throw_on(&[codes])` | Don't error for specific codes |
| | `.quiet()` | Suppress all output |
| | `.quiet_stdout()` | Suppress stdout |
| | `.quiet_stderr()` | Suppress stderr |
| | `.timeout(duration)` | Kill after duration |
| **Compose** | `.pipe(other)` | Pipe stdout to other's stdin |
| | `.and(other)` | Run other only on success (`&&`) |
| | `.or(other)` | Run other only on failure (`\|\|`) |
| | `.then(other)` | Run other regardless (`;`) |
| **Execute** | `.run()` | Run, return `Result<()>` |
| | `.text()` | Capture stdout as trimmed `String` |
| | `.lines()` | Capture stdout as `Vec<String>` |
| | `.bytes()` | Capture stdout as `Vec<u8>` |
| | `.json::<T>()` | Parse stdout as JSON |
| | `.status_code()` | Get exit code, never throws |
| | `.output()` | Full `CmdOutput` |
| | `.run_with_tail(title, done, n)` | Spinner + last N lines of stdout |
| | `.run_with_tail_opts(opts)` | Spinner with full `TailOptions` |

## Platform

Unix only. Tested on macOS and Linux. The `shell!` macro requires `/bin/sh`.

## License

MIT
