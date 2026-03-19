//! # raxx
//!
//! A dax-inspired shell scripting library for Rust. Synchronous, Unix-only.
//!
//! Provides two macros for running commands:
//!
//! - [`cmd!`] — Builds a command without invoking a shell. Arguments are passed
//!   directly to the process, so spaces, quotes, and special characters are safe.
//! - [`shell!`] — Passes a string to `/bin/sh -c` for full shell syntax (pipes,
//!   redirects, variables, loops, etc.).
//!
//! Both return a [`Cmd`] builder that supports piping, chaining, IO redirection,
//! environment variables, working directory, timeouts, and more.
//!
//! # Quick Start
//!
//! ```no_run
//! use raxx::{cmd, shell, Stdout, Stderr, Null, Append};
//!
//! # fn main() -> raxx::Result<()> {
//! // Run a command (output goes to terminal by default)
//! cmd!("echo", "hello world").run()?;
//!
//! // Capture stdout
//! let text = cmd!("echo", "hello").run_stdout()?;
//! assert_eq!(text.trim(), "hello");
//!
//! // Shell syntax (capture with .capture().run())
//! let result = shell!("echo hello | tr a-z A-Z").capture().run()?;
//! assert_eq!(result.stdout_trimmed(), "HELLO");
//!
//! // Piping with the builder API
//! let result = cmd!("echo", "hello")
//!     .pipe(cmd!("tr", "a-z", "A-Z"))
//!     .capture().run()?;
//! assert_eq!(result.stdout_trimmed(), "HELLO");
//!
//! // Chaining (&&, ||, ;)
//! let result = cmd!("false")
//!     .or(cmd!("echo", "fallback"))
//!     .capture().run()?;
//! assert_eq!(result.stdout_trimmed(), "fallback");
//!
//! // Redirect stdout to a file
//! cmd!("echo", "hello").redirect(Stdout, "out.txt").run()?;
//!
//! // Environment and working directory
//! let text = shell!("echo $MY_VAR")
//!     .env("MY_VAR", "hello")
//!     .cwd("/tmp")
//!     .run_stdout()?;
//! # Ok(())
//! # }
//! ```
//!
//! # Vector Arguments
//!
//! Both macros accept vectors (or slices) as arguments. Each element is treated
//! as a separate argument. For `shell!`, each element is shell-escaped.
//!
//! ```no_run
//! use raxx::{cmd, shell};
//!
//! # fn main() -> raxx::Result<()> {
//! let files = vec!["file1.txt", "file2.txt"];
//! cmd!("cat", files).run()?;
//!
//! let flags = vec!["--verbose", "--color=always"];
//! shell!("ls", flags).run()?;
//! # Ok(())
//! # }
//! ```
//!
//! # Error Handling
//!
//! Commands return [`CmdError`] on non-zero exit codes by default. Use
//! [`.no_exit_err()`](Cmd::no_throw) or [`.run_exit_code()`](Cmd::run_exit_code)
//! to suppress this.
//!
//! ```no_run
//! # use raxx::cmd;
//! # fn main() -> raxx::Result<()> {
//! let code = cmd!("false").run_exit_code()?; // 1, no error
//! let result = cmd!("false").no_exit_err().run()?; // code=1, no error
//! # Ok(())
//! # }
//! ```
//!
//! See [`CmdError`] for all error variants.

mod cmd;
mod error;
mod glob_util;
mod pipeline;
mod result;
mod tail;

pub use cmd::{Append, Cmd, CmdOps, Null, RedirectFrom, Stderr, Stdout, TimeoutConfig};
pub use error::{CmdError, Result};
pub use glob_util::{glob, glob_esc};
pub use result::{Captured, CmdResult, Redirected};
pub use tail::{TailOptions, TailStream};

/// Trait for types that can be used as arguments in [`cmd!`] and [`shell!`].
///
/// Implemented for single strings (`&str`, `String`, `&String`) and
/// collections (`Vec<T>`, `&[T]`) where elements are string-like.
/// Collections are flattened — each element becomes a separate argument.
pub trait IntoArgs {
    /// Convert into a list of argument strings.
    fn into_args(self) -> Vec<String>;
}

impl IntoArgs for &str {
    fn into_args(self) -> Vec<String> {
        vec![self.to_string()]
    }
}

impl IntoArgs for String {
    fn into_args(self) -> Vec<String> {
        vec![self]
    }
}

impl IntoArgs for &String {
    fn into_args(self) -> Vec<String> {
        vec![self.clone()]
    }
}

impl IntoArgs for std::path::PathBuf {
    fn into_args(self) -> Vec<String> {
        vec![self.to_string_lossy().to_string()]
    }
}

impl IntoArgs for &std::path::Path {
    fn into_args(self) -> Vec<String> {
        vec![self.to_string_lossy().to_string()]
    }
}

impl<T: AsRef<std::ffi::OsStr>> IntoArgs for Vec<T> {
    fn into_args(self) -> Vec<String> {
        self.iter()
            .map(|s| s.as_ref().to_string_lossy().to_string())
            .collect()
    }
}

impl<T: AsRef<std::ffi::OsStr>> IntoArgs for &[T] {
    fn into_args(self) -> Vec<String> {
        self.iter()
            .map(|s| s.as_ref().to_string_lossy().to_string())
            .collect()
    }
}

// Fixed-size arrays
impl<T: AsRef<std::ffi::OsStr>, const N: usize> IntoArgs for [T; N] {
    fn into_args(self) -> Vec<String> {
        self.iter()
            .map(|s| s.as_ref().to_string_lossy().to_string())
            .collect()
    }
}

impl<T: AsRef<std::ffi::OsStr>, const N: usize> IntoArgs for &[T; N] {
    fn into_args(self) -> Vec<String> {
        self.iter()
            .map(|s| s.as_ref().to_string_lossy().to_string())
            .collect()
    }
}

/// Create a command with safe argument handling.
///
/// The first argument is the program name. Additional arguments are appended
/// as-is — they are **not** interpreted by a shell, so spaces, quotes, globs,
/// and special characters are passed through safely.
///
/// Vectors and slices are flattened — each element becomes a separate argument.
///
/// # Examples
///
/// ```no_run
/// use raxx::cmd;
///
/// # fn main() -> raxx::Result<()> {
/// // Simple command
/// cmd!("echo", "hello").run()?;
///
/// // Variables with spaces are safe
/// let name = "hello world";
/// let text = cmd!("echo", name).run_stdout()?;
/// assert_eq!(text.trim(), "hello world");
///
/// // Vectors are flattened into separate args
/// let files = vec!["a.txt", "b.txt", "c.txt"];
/// cmd!("cat", files).run()?;
/// # Ok(())
/// # }
/// ```
#[macro_export]
macro_rules! cmd {
    // With ops: cmd!("echo", "hi"; &ops)
    ($cmd:expr $(, $arg:expr)* ; $ops:expr) => {
        $crate::Cmd::new($cmd)$(.push_args($arg))*.with_ops($ops)
    };
    // Without ops (existing)
    ($cmd:expr $(, $arg:expr)* $(,)?) => {
        $crate::Cmd::new($cmd)$(.push_args($arg))*
    };
}

/// Create a shell command via `/bin/sh -c`.
///
/// The first argument is the base shell command string. It supports three modes:
///
/// **1. Interpolation mode** — named `{var}` placeholders are auto-escaped:
/// ```no_run
/// use raxx::shell;
/// # fn main() -> raxx::Result<()> {
/// let name = "hello world";
/// let text = shell!("echo {name} | tr a-z A-Z").run_stdout()?;
/// // Executes: /bin/sh -c "echo 'hello world' | tr a-z A-Z"
/// # Ok(())
/// # }
/// ```
///
/// **2. Append mode** — extra args are escaped and appended:
/// ```no_run
/// use raxx::shell;
/// # fn main() -> raxx::Result<()> {
/// let pattern = "hello world";
/// shell!("grep", pattern, "file.txt").run()?;
/// // Executes: /bin/sh -c "grep 'hello world' file.txt"
/// # Ok(())
/// # }
/// ```
///
/// **3. Plain mode** — no placeholders, no extra args:
/// ```no_run
/// use raxx::shell;
/// # fn main() -> raxx::Result<()> {
/// let text = shell!("echo hello | tr a-z A-Z").run_stdout()?;
/// assert_eq!(text.trim(), "HELLO");
/// # Ok(())
/// # }
/// ```
///
/// Vectors and slices are flattened — each element is escaped and appended
/// as a separate shell argument.
///
/// # Security
///
/// Named placeholders (`{var}`) and extra arguments are automatically escaped.
/// The base command string (first argument, outside of placeholders) is passed
/// to the shell as-is. **Do not** interpolate untrusted input into the literal
/// parts of the format string.
pub use raxx_macros::shell;

/// Internal helper for `shell!` macro — appends escaped arguments to a shell
/// command string. Not intended for direct use.
#[doc(hidden)]
pub fn _append_shell_args<A: IntoArgs>(cmd: &mut String, args: A) {
    for arg in args.into_args() {
        cmd.push(' ');
        cmd.push_str(&escape_arg(&arg));
    }
}

/// Trait for escaping values for shell interpolation in `shell!` format strings.
///
/// Scalars are escaped as a single argument. Vectors/slices have each element
/// escaped individually and joined with spaces.
#[doc(hidden)]
pub trait EscapeForShell {
    fn escape_for_shell(&self) -> String;
}

impl EscapeForShell for &str {
    fn escape_for_shell(&self) -> String {
        escape_arg(self)
    }
}

impl EscapeForShell for String {
    fn escape_for_shell(&self) -> String {
        escape_arg(self)
    }
}

impl EscapeForShell for &String {
    fn escape_for_shell(&self) -> String {
        escape_arg(self)
    }
}

impl EscapeForShell for std::path::PathBuf {
    fn escape_for_shell(&self) -> String {
        escape_arg(&self.to_string_lossy())
    }
}

impl EscapeForShell for &std::path::Path {
    fn escape_for_shell(&self) -> String {
        escape_arg(&self.to_string_lossy())
    }
}

impl<T: AsRef<std::ffi::OsStr>> EscapeForShell for Vec<T> {
    fn escape_for_shell(&self) -> String {
        self.iter()
            .map(|s| escape_arg(&s.as_ref().to_string_lossy()))
            .collect::<Vec<_>>()
            .join(" ")
    }
}

impl<T: AsRef<std::ffi::OsStr>> EscapeForShell for &[T] {
    fn escape_for_shell(&self) -> String {
        self.iter()
            .map(|s| escape_arg(&s.as_ref().to_string_lossy()))
            .collect::<Vec<_>>()
            .join(" ")
    }
}

impl<T: AsRef<std::ffi::OsStr>, const N: usize> EscapeForShell for [T; N] {
    fn escape_for_shell(&self) -> String {
        self.iter()
            .map(|s| escape_arg(&s.as_ref().to_string_lossy()))
            .collect::<Vec<_>>()
            .join(" ")
    }
}

impl<T: AsRef<std::ffi::OsStr>, const N: usize> EscapeForShell for &[T; N] {
    fn escape_for_shell(&self) -> String {
        self.iter()
            .map(|s| escape_arg(&s.as_ref().to_string_lossy()))
            .collect::<Vec<_>>()
            .join(" ")
    }
}

/// Escape a string for safe use in a shell command.
///
/// Returns the string unchanged if it contains only safe characters
/// (alphanumeric, `-`, `_`, `=`, `/`, `.`, `,`, `:`, `@`). Otherwise
/// wraps it in single quotes, escaping any embedded single quotes.
///
/// Empty strings return `''`.
///
/// # Examples
///
/// ```
/// use raxx::escape_arg;
///
/// assert_eq!(escape_arg("hello"), "hello");
/// assert_eq!(escape_arg("hello world"), "'hello world'");
/// assert_eq!(escape_arg(""), "''");
/// assert_eq!(escape_arg("it's"), "'it'\"'\"'s'");
/// ```
pub fn escape_arg(arg: &str) -> String {
    if arg.is_empty() {
        return "''".to_string();
    }
    if arg
        .chars()
        .all(|c| matches!(c, 'a'..='z' | 'A'..='Z' | '0'..='9' | '-' | '_' | '=' | '/' | '.' | ',' | ':' | '@'))
    {
        return arg.to_string();
    }
    format!("'{}'", arg.replace('\'', "'\"'\"'"))
}
