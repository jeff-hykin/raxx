# Raxx

A dax-inspired shell scripting library for Rust. Synchronous, Unix-only.

Inspired by [dax](https://github.com/dsherret/dax) for Deno. Provides ergonomic macros and a builder API for running shell commands with safe argument escaping, piping, chaining, IO redirection, environment control, and more.

## Quick Start

```toml
[dependencies]
raxx = "0.1"
```

```rust
use raxx::{cmd, shell, CmdOps, Stdout, Null};
use std::time::Duration;

fn main() -> raxx::Result<()> {
    let name = "escape'd arg";
    let text = shell!("echo hello {name} | tr a-z A-Z")
        .pipe(cmd!("head", "-5"))
        .run_stdout()?;
    Ok(())
}
```

## Feature Overview

```rust
// Vector arg (gets flattened into separate args, each are escaped)
let files = vec!["file one.txt", "file two.txt"];
shell!("cat {files} | wc -l").run()?;
    
// globbing
shell!(r#"wc -l {glob("src/**/*.rs")}"#).run()?;

// scrolling 5 lines of stdout (show progress without flooding the terminal)
cmd!("cargo", "build")
    .run_with_tail("Building...", "Build complete", 5)?;

// chaining, piping
let text = shell!("grep -r {args} src/")
    .or(cmd!("echo", "no matches"))
    .pipe(cmd!("head", "-5"))
    .run_stdout()?;

// Timeout: sends SIGTERM, then SIGKILL after 5s grace period
cmd!("sleep", "60")
    .timeout(Duration::from_secs(10))
    .run()?;

// Check success without throwing
let result = cmd!("cargo", "test").run_no_exit_err()?;
if result.success() {
    println!("all tests passed");
}

// Shared options
let ops = CmdOps {
    env: HashMap::from([("RUST_LOG".into(), "debug".into())]),
    cwd: Some("/my/project".into()),
    shell: None,            // defaults to ("/bin/sh", "-c")
    verbose: true,          // print commands before running
    dry: false,             // actually run commands
    no_err: false,          // propagate errors
    no_warn: false,         // show warnings
};

cmd!("cargo", "build"; &ops).run()?;
cmd!("cargo", "test"; &ops).run()?;
```

## Two Macros

### `cmd!` — Shellless Commands

When possible, perfer `cmd!` over `shell!` for performance and clarity.

```rust
use raxx::cmd;

// Simple
cmd!("echo", "hello").run()?;

// Each argument is separate
cmd!("grep", "-rn", "pattern", "src/").run()?;

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

## Shell Interpolation with Escaping

The `shell!` macro is for convenience. Warning: every interpolation is escaped as a shell argument. E.g. `shell!("echo hi > {file_path}")` is equivalent to `shell!("echo hi > './file/path'")`.

```rust
use raxx::shell;

let weird_filename = "hello;rm -rf /";
shell!("cat {weird_filename}").run()?;
// no rm -rf / in the output 

// Vectors are escaped element-by-element and joined with spaces
let files = vec!["my file.txt", "other file.txt"];
shell!("cat {files} | wc -l").run()?;
// Executes: /bin/sh -c "cat 'my file.txt' 'other file.txt' | wc -l"

// Shell $VAR syntax still works alongside {var} interpolation
let greeting = "hello";
shell!("X=world; echo {greeting} $X").run()?;
// prints: hello world

// safe glob expansion — permission errors and 0-matches surface in run
shell!("wc -l {glob(\"src/**/*.rs\")}").run()?;
// Expands to something like: /bin/sh -c "wc -l src/cmd.rs src/lib.rs ..."

// Optional flags based on a boolean
let verbose = true;
shell!("grep -r {flag_if(\"-v\", verbose)} pattern src/").run()?;
// When verbose=true: grep -r -v pattern src/
// When verbose=false: grep -r  pattern src/
```

The literal parts of the format string (outside `{var}` placeholders) are passed
to the shell as-is, so pipes, redirects, and shell features work normally.

### Inline Functions in `shell!`

The `shell!` macro supports two inline function calls inside `{...}` placeholders:

- **`{glob("pattern")}`** — Expands a glob pattern at runtime. Matched files are shell-escaped and inserted. If the glob matches nothing or encounters an error, the error is deferred until the command is executed (`.run()?`), keeping the builder chain intact.

- **`{flag_if("flag", condition)}`** — Conditionally inserts a flag. If `condition` is `true`, the flag is shell-escaped and inserted; if `false`, nothing is inserted.

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
Returns an error on permission failures or when zero files match.

```rust
use raxx::{cmd, glob, shell, Cmd};

// All .txt files in a directory
let files = glob("docs/*.txt")?;

// Recursive — all .rs files under src/
let files = glob("src/**/*.rs")?;

// Use glob results directly with cmd! — no .collect() needed
let files = glob("src/**/*.rs")?;
cmd!("wc", "-l", files).run()?;

// Use glob results directly with shell! interpolation
let files = glob("tests/**/*.rs")?;
shell!("cat {files}").pipe(cmd!("wc", "-l")).run()?;

// Use .glob() on the builder — errors are deferred to execution time
let text = Cmd::new("echo")
    .arg("hello")
    .glob("people/*")
    .run_stdout()?;
```

## Shared Options (`CmdOps`)

When you're running many commands with the same working directory, environment, or debug settings, create a `CmdOps` and pass it to `cmd!` or `shell!` with a semicolon:

```rust
use raxx::{cmd, shell, CmdOps};

// Create shared options
let ops = CmdOps::new()
    .cwd("/my/project")
    .env("RUST_LOG", "debug")
    .verbose(true);   // prints each command before running

// Use with cmd! and shell! — semicolon separates args from ops
cmd!("cargo", "build"; &ops).run()?;
cmd!("cargo", "test"; &ops).run()?;
shell!("echo $RUST_LOG"; &ops).run()?;

// Works with all cmd! features (args, vecs, etc.)
let extra_flags = vec!["--release", "--all-targets"];
cmd!("cargo", "build", extra_flags; &ops).run()?;
```

### Struct Literal Syntax

You can also construct `CmdOps` with struct literal syntax to see every
option and its default at a glance:

```rust
use raxx::CmdOps;
use std::collections::HashMap;

let ops = CmdOps {
    env: HashMap::from([("RUST_LOG".into(), "debug".into())]),
    cwd: Some("/my/project".into()),
    shell: None,            // defaults to ("/bin/sh", "-c")
    verbose: true,          // print commands before running
    dry: false,             // actually run commands
    no_err: false,          // propagate errors
    no_warn: false,         // show warnings
};

// Use `..Default::default()` to only set what you need
let ops = CmdOps {
    verbose: true,
    dry: true,
    ..Default::default()
};
```

### Overriding and Updating

`CmdOps` has public fields and builder methods, so you can derive new configs from existing ones either way:

```rust
use raxx::{cmd, CmdOps};

// Base config for the project
let base = CmdOps::new()
    .cwd("/my/project")
    .env("RUST_LOG", "info");

// Override for debugging — clone and extend
let debug = base.clone()
    .verbose(true)
    .env("RUST_LOG", "trace");

// Override for CI — quiet and non-failing
let ci = base.clone()
    .no_err(true)
    .no_warn(true);

// Or override fields directly
let mut custom = base.clone();
custom.verbose = true;
custom.dry = true;

// Per-command overrides still work alongside ops
cmd!("cargo", "test"; &base)
    .env("EXTRA", "val")      // adds to the ops env
    .cwd("/override/path")    // overrides the ops cwd
    .run()?;
```

### Custom Shell

By default, `shell!` uses `/bin/sh -c`. Override it via `.shell_program()`:

```rust
use raxx::{shell, CmdOps};

// Use bash for all shell! commands
let ops = CmdOps::new().shell_program("/bin/bash", "-c");
shell!("echo $BASH_VERSION"; &ops).run()?;

// Use zsh
let zsh = CmdOps::new().shell_program("/bin/zsh", "-c");
shell!("echo $ZSH_VERSION"; &zsh).run()?;

// Combine with other options
let ops = CmdOps::new()
    .shell_program("/bin/bash", "-c")
    .cwd("/my/project")
    .verbose(true);
shell!("echo hello"; &ops).run()?;
```

This only affects shell commands (`shell!` and `Cmd::shell`). Regular `cmd!` calls are not changed.

### Dry Run Mode

Print commands without running them — great for debugging scripts:

```rust
use raxx::{cmd, shell, CmdOps};

let dry = CmdOps::new().dry(true);

// These print "$ echo hello" and "$ cargo build" to stderr
// but don't actually execute
cmd!("echo", "hello"; &dry).run()?;
cmd!("cargo", "build", "--release"; &dry).run()?;
shell!("echo hello | wc -c"; &dry).run()?;
```

### Available Options

| Method | Description |
|---|---|
| `.cwd(path)` | Set working directory |
| `.env(key, val)` | Set an environment variable |
| `.envs(pairs)` | Set multiple environment variables |
| `.shell_program(prog, flag)` | Override the shell for `shell!` (default: `/bin/sh`, `-c`) |
| `.verbose(true)` | Print `$ command` to stderr before running |
| `.dry(true)` | Print command but don't run (returns empty `Ok`) |
| `.no_err(true)` | Swallow all errors (prints warnings) |
| `.no_warn(true)` | Swallow all errors silently |

## Capturing Output

```rust
use raxx::cmd;

// Stdout as string (untrimmed)
let text = cmd!("echo", "hello").run_stdout()?;

// Full result with both streams
let result = cmd!("echo", "hello").run()?;
println!("code: {}", result.code);
println!("stdout: {}", result.stdout_trimmed());
println!("stderr: {}", result.stderr_trimmed());

// Lines, bytes, JSON
let lines = result.stdout_lines();
let bytes = result.stdout_bytes();
let val: serde_json::Value = cmd!("echo", r#"{"key": "value"}"#)
    .run()?.stdout_json()?;

// Exit code without throwing
let code = cmd!("false").run_exit_code()?;
assert_eq!(code, 1);
```

## IO Redirection

Use `.redirect(Source, Target)` for type-safe stream redirection:

```rust
use raxx::{cmd, shell, Stdout, Stderr, Null, Append};

// Stdout to file
cmd!("echo", "hello").redirect(Stdout, "output.txt").run()?;

// Append to file
cmd!("echo", "more").redirect(Stdout, Append("output.txt")).run()?;

// Discard stdout
cmd!("echo", "quiet").redirect(Stdout, Null).run()?;

// Stderr to /dev/null
shell!("echo err >&2").redirect(Stderr, Null).run()?;

// Stderr to stdout (merge)
shell!("echo err >&2").redirect(Stderr, Stdout).run()?;

// Convenience: quiet() suppresses both
cmd!("noisy-command").quiet().run()?;
cmd!("noisy-command").quiet_stdout().run()?;
cmd!("noisy-command").quiet_stderr().run()?;
```

Accessing a redirected stream is a **compile error**:

```rust,compile_fail
// This won't compile — stdout was redirected to a file
cmd!("echo", "hi").redirect(Stdout, "f.txt").run()?.stdout();
```

## Piping

Chain commands together with `.pipe()`. Each command's stdout feeds into the next command's stdin.

```rust
use raxx::cmd;

let text = cmd!("echo", "hello world")
    .pipe(cmd!("tr", "a-z", "A-Z"))
    .run_stdout()?;
assert_eq!(text.trim(), "HELLO WORLD");

// Multi-stage pipelines
let text = cmd!("echo", "3\n1\n2")
    .pipe(cmd!("sort"))
    .pipe(cmd!("head", "-n", "1"))
    .run_stdout()?;
assert_eq!(text.trim(), "1");
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
    .run_stdout()?;
assert_eq!(text.trim(), "fallback");

// ; — run second regardless
cmd!("false")
    .then(cmd!("echo", "always runs"))
    .run()?;

// Complex chains
let text = cmd!("false")
    .and(cmd!("echo", "a"))
    .or(cmd!("echo", "b"))
    .run_stdout()?;
assert_eq!(text.trim(), "b");
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
let text = cmd!("cat").stdin_text("hello").run_stdout()?;

// From bytes
let text = cmd!("cat").stdin_bytes(b"hello".to_vec()).run_stdout()?;

// From a file
let text = cmd!("cat").stdin_path("input.txt").run_stdout()?;

// No stdin (/dev/null)
cmd!("cat").stdin_null().run()?;
```

## Error Handling

By default, commands return `Err` on non-zero exit codes.

```rust
use raxx::{cmd, shell, CmdError};

// Default: error on non-zero
let err = cmd!("false").run().unwrap_err();

// no_exit_err: suppress exit-code errors only
let result = cmd!("false").no_exit_err().run()?;
assert_eq!(result.code, 1);

// no_err: swallow ALL errors (prints warnings for serious ones)
cmd!("__nonexistent__").no_err().run()?;  // Ok, prints warning

// no_nothin: swallow ALL errors silently
cmd!("__nonexistent__").no_nothin().run()?;  // Ok, no output

// run_no_exit_err / run_ignore_code: execution shorthands
let result = cmd!("false").run_no_exit_err()?;
assert_eq!(result.code, 1);

let result = cmd!("false").run_ignore_code()?;
assert_eq!(result.code, 1);

// Suppress specific exit codes only
let result = shell!("exit 42").no_exit_err_on(&[42]).run()?;
assert_eq!(result.code, 42);

// Just get the exit code (never throws)
let code = cmd!("false").run_exit_code()?;
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
    Ok(_) => {}
}
```

## Timeout

By default, `.timeout()` sends SIGTERM first, then SIGKILL after a 2-second grace period:

```rust
use raxx::cmd;
use std::time::Duration;

// SIGTERM after 2s, SIGKILL 2s later if still alive
cmd!("sleep", "100")
    .timeout(Duration::from_secs(2))
    .run()?; // returns CmdError::Timeout
```

For full control, use `.timeout_signal()` to choose the signal and grace period:

```rust
use raxx::cmd;
use std::time::Duration;

// Send SIGINT after 10s, then SIGKILL after 5s grace period
cmd!("my-server")
    .timeout_signal(Duration::from_secs(10), libc::SIGINT, Some(Duration::from_secs(5)))
    .run()?;

// Immediate SIGKILL with no grace period
cmd!("runaway-process")
    .timeout_signal(Duration::from_secs(2), libc::SIGKILL, None)
    .run()?;
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
    .run_stdout()?;

let text = Cmd::parse("grep -rn")
    .arg("pattern")
    .arg("src/")
    .cwd("/my/project")
    .run_stdout()?;
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

## Glob Escaping

When building glob patterns from user-provided or dynamic paths, use `glob_esc` to escape
metacharacters (`*`, `?`, `[`, `]`) so they're treated as literal characters:

```rust
use raxx::{glob, glob_esc};

// Without escaping, brackets in "my[project]" would be interpreted as a character class
let dir = "my[project]";
let files = glob(&format!("{}/*.rs", glob_esc(dir)))?;

// Common pattern: escape a base path, then append a glob suffix
let base = std::env::var("PROJECT_DIR").unwrap_or(".".into());
let files = glob(&format!("{}/**/*.rs", glob_esc(&base)))?;

// Works with cmd! and shell! too
use raxx::{cmd, shell};
let user_dir = "path/with spaces*and[brackets]";
let pattern = format!("{}/*.txt", glob_esc(user_dir));
cmd!("wc", "-l").glob(&pattern).run()?;
```

What gets escaped:

```rust
use raxx::glob_esc;

assert_eq!(glob_esc("normal/path"), "normal/path");
assert_eq!(glob_esc("dir[1]"), "dir[[]1[]]");
assert_eq!(glob_esc("file*.txt"), "file[*].txt");
assert_eq!(glob_esc("what?"), "what[?]");
```

## CmdResult

The `.run()` method returns `CmdResult<O, E>` with type-safe access to captured streams. If a stream was redirected, its accessors are unavailable at **compile time**.

| Method | Available when | Returns |
|---|---|---|
| `result.code` | always | `i32` — exit code |
| `result.success()` | always | `bool` — true if code == 0 |
| `result.stdout()` | stdout captured | `String` (untrimmed) |
| `result.stdout_trimmed()` | stdout captured | `String` |
| `result.stdout_bytes()` | stdout captured | `&[u8]` |
| `result.stdout_lines()` | stdout captured | `Vec<String>` |
| `result.stdout_json::<T>()` | stdout captured | `Result<T>` |
| `result.stderr()` | stderr captured | `String` (untrimmed) |
| `result.stderr_trimmed()` | stderr captured | `String` |
| `result.stderr_bytes()` | stderr captured | `&[u8]` |
| `result.stderr_lines()` | stderr captured | `Vec<String>` |
| `result.stderr_json::<T>()` | stderr captured | `Result<T>` |
| `result.out()` | both captured | `String` (stdout+stderr) |

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
| `GlobNoMatches { pattern }` | Glob matched zero files |

`CmdError` implements `Display`, `Debug`, `Error`, and `From` conversions for `io::Error`, `FromUtf8Error`, and `serde_json::Error`.

## Full API Summary

### Macros

| Macro | Description |
|---|---|
| `cmd!("prog", arg1, arg2)` | Safe command — no shell, args passed directly |
| `cmd!("prog", vec)` | Vector elements are flattened into separate args |
| `cmd!("prog", args; &ops)` | With shared `CmdOps` |
| `shell!("cmd string")` | Shell command via `/bin/sh -c` |
| `shell!("cmd", arg1, vec)` | Extra args are shell-escaped and appended |
| `shell!("cmd"; &ops)` | With shared `CmdOps` |
| `shell!("... {glob(\"pat\")} ...")` | Inline glob expansion (deferred errors) |
| `shell!("... {flag_if(\"f\", b)} ...")` | Conditional flag insertion |

### Functions

| Function | Description |
|---|---|
| `glob("pattern")` | Find files matching `*`, `**`, `?`, `[...]` patterns |
| `glob_esc("str")` | Escape glob metacharacters (`*`, `?`, `[`, `]`) in a path |
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
| | `.glob(pattern)` | Expand glob and append matched files |
| **Env** | `.env(key, val)` | Set env var |
| | `.envs(pairs)` | Set multiple env vars |
| | `.env_remove(key)` | Remove env var |
| | `.env_clear()` | Clear all env vars |
| **Cwd** | `.cwd(path)` | Set working directory |
| **Stdin** | `.stdin_text(str)` | Stdin from string |
| | `.stdin_bytes(bytes)` | Stdin from bytes |
| | `.stdin_path(path)` | Stdin from file |
| | `.stdin_null()` | No stdin |
| **Redirect** | `.redirect(Stdout, target)` | Redirect stdout to file/Null/Stderr/Append |
| | `.redirect(Stderr, target)` | Redirect stderr to file/Null/Stdout/Append |
| | `.quiet()` | Suppress both streams |
| | `.quiet_stdout()` | Suppress stdout |
| | `.quiet_stderr()` | Suppress stderr |
| | `.swap_streams()` | Swap stdout ↔ stderr |
| **Behavior** | `.no_exit_err()` | Don't error on non-zero exit codes |
| | `.no_exit_err_on(&[codes])` | Don't error for specific codes |
| | `.no_err()` | Swallow all errors (warn on serious) |
| | `.no_nothin()` | Swallow all errors silently |
| | `.timeout(duration)` | SIGTERM after duration, SIGKILL after 2s grace |
| | `.timeout_signal(dur, sig, grace)` | Custom signal and grace period |
| | `.with_ops(&ops)` | Apply shared `CmdOps` |
| **Compose** | `.pipe(other)` | Pipe stdout to other's stdin |
| | `.and(other)` | Run other only on success (`&&`) |
| | `.or(other)` | Run other only on failure (`\|\|`) |
| | `.then(other)` | Run other regardless (`;`) |
| **Execute** | `.run()` | Run, return `Result<CmdResult>` |
| | `.run_stdout()` | Stdout as `String` (untrimmed) |
| | `.run_stderr()` | Stderr as `String` (untrimmed) |
| | `.run_out()` | Stdout + stderr concatenated |
| | `.run_stdout_json::<T>()` | Parse stdout as JSON |
| | `.run_stderr_json::<T>()` | Parse stderr as JSON |
| | `.run_exit_code()` | Get exit code, never throws |
| | `.run_success()` | Returns `bool`, never throws |
| | `.run_no_exit_err()` | Like `.run()` but ignores exit codes |
| | `.run_ignore_code()` | Alias for `.run_no_exit_err()` |
| | `.run_with_tail(title, done, n)` | Spinner + last N lines of stdout |
| | `.run_with_tail_opts(opts)` | Spinner with full `TailOptions` |

## Platform

Unix only. Tested on macOS and Linux. The `shell!` macro requires `/bin/sh`.

## License

MIT
