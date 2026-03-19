use std::collections::HashMap;
use std::ffi::OsStr;
use std::io::{Read, Write};
use std::marker::PhantomData;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::Duration;

use crate::error::{CmdError, Result};
use crate::pipeline::Pipeline;
use crate::result::{Captured, CmdResult, RawOutput, Redirected};

// ── Config types ──

/// Configuration for stdin source.
#[derive(Debug, Clone)]
pub enum StdinConfig {
    /// Inherit stdin from the parent process (default).
    Inherit,
    /// No stdin (`/dev/null`).
    Null,
    /// Provide bytes as stdin.
    Bytes(Vec<u8>),
    /// Read stdin from a file.
    Path(PathBuf),
}

/// Configuration for an output stream (stdout or stderr).
#[derive(Debug, Clone)]
pub enum OutputConfig {
    /// Inherit from the parent process — output goes to the terminal (default).
    Inherit,
    /// Capture into memory for later access via [`CmdResult`].
    Capture,
    /// Discard (`/dev/null`).
    Null,
    /// Write to a file (overwrite).
    File(PathBuf),
    /// Append to a file.
    Append(PathBuf),
    /// Redirect to the other stream (stdout → stderr, or stderr → stdout).
    ToOther,
}

/// Controls whether a non-zero exit code produces an error.
#[derive(Debug, Clone)]
pub enum ThrowBehavior {
    /// Error on any non-zero exit code (default).
    ThrowAlways,
    /// Never error on non-zero exit codes.
    NoThrow,
    /// Don't error for these specific exit codes; error on all others.
    NoThrowOn(Vec<i32>),
}

/// Configuration for command timeout.
#[derive(Debug, Clone)]
pub struct TimeoutConfig {
    /// Total time before sending the initial signal.
    pub duration: Duration,
    /// Signal to send first (default: SIGTERM = 15).
    pub signal: i32,
    /// Grace period after the initial signal before sending SIGKILL.
    /// If `None`, SIGKILL is sent immediately.
    pub kill_after: Option<Duration>,
}

// ── Redirect types ──

/// Marker for the stdout stream. Used as a source in `.redirect(Stdout, target)`
/// or as a target in `.redirect(Stderr, Stdout)`.
#[derive(Debug, Clone, Copy)]
pub struct Stdout;

/// Marker for the stderr stream. Used as a source in `.redirect(Stderr, target)`
/// or as a target in `.redirect(Stdout, Stderr)`.
#[derive(Debug, Clone, Copy)]
pub struct Stderr;

/// Redirect target: discard output (`/dev/null`).
#[derive(Debug, Clone, Copy)]
pub struct Null;

/// Redirect target: append to a file instead of overwriting.
///
/// ```no_run
/// use raxx::{cmd, Stdout, Append};
/// cmd!("echo", "hi").redirect(Stdout, Append("log.txt")).run().unwrap();
/// ```
#[derive(Debug, Clone)]
pub struct Append<T>(pub T);

/// Internal redirect target representation.
pub(crate) enum RedirectTarget {
    File(PathBuf),
    AppendFile(PathBuf),
    Null,
    ToOther,
}

impl RedirectTarget {
    fn to_output_config(self) -> OutputConfig {
        match self {
            RedirectTarget::File(p) => OutputConfig::File(p),
            RedirectTarget::AppendFile(p) => OutputConfig::Append(p),
            RedirectTarget::Null => OutputConfig::Null,
            RedirectTarget::ToOther => OutputConfig::ToOther,
        }
    }
}

// ── From impls for RedirectTarget ──

impl From<&str> for RedirectTarget {
    fn from(s: &str) -> Self {
        RedirectTarget::File(PathBuf::from(s))
    }
}

impl From<String> for RedirectTarget {
    fn from(s: String) -> Self {
        RedirectTarget::File(PathBuf::from(s))
    }
}

impl From<&String> for RedirectTarget {
    fn from(s: &String) -> Self {
        RedirectTarget::File(PathBuf::from(s))
    }
}

impl From<PathBuf> for RedirectTarget {
    fn from(p: PathBuf) -> Self {
        RedirectTarget::File(p)
    }
}

impl From<&Path> for RedirectTarget {
    fn from(p: &Path) -> Self {
        RedirectTarget::File(p.to_path_buf())
    }
}

impl From<&PathBuf> for RedirectTarget {
    fn from(p: &PathBuf) -> Self {
        RedirectTarget::File(p.clone())
    }
}

impl From<Null> for RedirectTarget {
    fn from(_: Null) -> Self {
        RedirectTarget::Null
    }
}

impl From<Stdout> for RedirectTarget {
    fn from(_: Stdout) -> Self {
        RedirectTarget::ToOther
    }
}

impl From<Stderr> for RedirectTarget {
    fn from(_: Stderr) -> Self {
        RedirectTarget::ToOther
    }
}

impl<T: AsRef<Path>> From<Append<T>> for RedirectTarget {
    fn from(a: Append<T>) -> Self {
        RedirectTarget::AppendFile(a.0.as_ref().to_path_buf())
    }
}

// ── RedirectFrom trait ──

/// Trait for type-safe stream redirection. You typically don't need to import
/// this — use the inherent `.redirect()` method on [`Cmd`] instead.
pub trait RedirectFrom<Source, Target> {
    type Output;
    fn apply_redirect(self, source: Source, target: Target) -> Self::Output;
}

impl<E, T: Into<RedirectTarget>> RedirectFrom<Stdout, T> for Cmd<Captured, E> {
    type Output = Cmd<Redirected, E>;
    fn apply_redirect(self, _: Stdout, target: T) -> Cmd<Redirected, E> {
        let mut inner = self.inner;
        inner.stdout = target.into().to_output_config();
        Cmd {
            inner,
            _phantom: PhantomData,
        }
    }
}

impl<O, T: Into<RedirectTarget>> RedirectFrom<Stderr, T> for Cmd<O, Captured> {
    type Output = Cmd<O, Redirected>;
    fn apply_redirect(self, _: Stderr, target: T) -> Cmd<O, Redirected> {
        let mut inner = self.inner;
        inner.stderr = target.into().to_output_config();
        Cmd {
            inner,
            _phantom: PhantomData,
        }
    }
}

// ── CmdInner ──

/// Reusable options that can be shared across many commands.
///
/// Pass as an optional second argument to [`cmd!`] and [`shell!`] via semicolon syntax:
///
/// ```no_run
/// use raxx::{cmd, shell, CmdOps};
///
/// let ops = CmdOps::new().cwd("/tmp").verbose(true);
/// cmd!("ls"; &ops).run().ok();
/// shell!("echo hello"; &ops).run().ok();
/// ```
///
/// You can also use struct literal syntax to see all available options with
/// their defaults:
///
/// ```no_run
/// use raxx::CmdOps;
/// use std::collections::HashMap;
///
/// let ops = CmdOps {
///     env: HashMap::from([("RUST_LOG".into(), "debug".into())]),
///     cwd: Some("/my/project".into()),
///     shell: None,            // defaults to ("/bin/sh", "-c")
///     verbose: true,          // print commands before running
///     dry: false,             // actually run commands
///     no_err: false,          // propagate errors
///     no_warn: false,         // show warnings
/// };
/// ```
#[derive(Debug, Clone, Default)]
pub struct CmdOps {
    /// Environment variables to set.
    pub env: HashMap<String, String>,
    /// Working directory.
    pub cwd: Option<PathBuf>,
    /// Shell program and flag, e.g. `Some(("/bin/bash".into(), "-c".into()))`.
    /// When `None`, uses `/bin/sh -c`.
    pub shell: Option<(String, String)>,
    /// Print `$ command` to stderr before executing.
    pub verbose: bool,
    /// Print the command but don't execute it.
    pub dry: bool,
    /// Swallow all errors (prints warnings for serious ones like not-found).
    pub no_err: bool,
    /// Swallow all errors silently (no warnings). Implies `no_err`.
    pub no_warn: bool,
}

impl CmdOps {
    /// Create a new empty options set.
    pub fn new() -> Self {
        Self::default()
    }

    /// Set the working directory.
    pub fn cwd<P: AsRef<Path>>(mut self, dir: P) -> Self {
        self.cwd = Some(dir.as_ref().to_path_buf());
        self
    }

    /// Set an environment variable.
    pub fn env<K: Into<String>, V: Into<String>>(mut self, key: K, val: V) -> Self {
        self.env.insert(key.into(), val.into());
        self
    }

    /// Set multiple environment variables.
    pub fn envs<I, K, V>(mut self, vars: I) -> Self
    where
        I: IntoIterator<Item = (K, V)>,
        K: Into<String>,
        V: Into<String>,
    {
        for (k, v) in vars {
            self.env.insert(k.into(), v.into());
        }
        self
    }

    /// Set the shell used by [`shell!`](crate::shell!) and [`Cmd::shell`].
    ///
    /// The first argument is the shell program, the second is the flag that
    /// tells it to execute a command string (typically `"-c"`).
    ///
    /// Defaults to `("/bin/sh", "-c")` when unset.
    ///
    /// ```no_run
    /// use raxx::{shell, CmdOps};
    ///
    /// let ops = CmdOps::new().shell_program("/bin/bash", "-c");
    /// shell!("echo $BASH_VERSION"; &ops).run().ok();
    /// ```
    pub fn shell_program<S1: Into<String>, S2: Into<String>>(mut self, program: S1, flag: S2) -> Self {
        self.shell = Some((program.into(), flag.into()));
        self
    }

    /// Print the command before executing.
    pub fn verbose(mut self, on: bool) -> Self {
        self.verbose = on;
        self
    }

    /// Only print the command, don't actually run it.
    pub fn dry(mut self, on: bool) -> Self {
        self.dry = on;
        self
    }

    /// Swallow all errors (print warnings for serious ones).
    pub fn no_err(mut self, on: bool) -> Self {
        self.no_err = on;
        self
    }

    /// Swallow all errors silently (no warnings).
    pub fn no_warn(mut self, on: bool) -> Self {
        self.no_warn = on;
        self
    }
}

/// Internal command state. All fields live here; `Cmd<O, E>` is a thin wrapper.
#[derive(Debug, Clone)]
pub(crate) struct CmdInner {
    pub(crate) program: String,
    pub(crate) args: Vec<String>,
    pub(crate) env_vars: HashMap<String, String>,
    pub(crate) env_remove: Vec<String>,
    pub(crate) env_clear: bool,
    pub(crate) cwd: Option<PathBuf>,
    pub(crate) stdin: StdinConfig,
    pub(crate) stdout: OutputConfig,
    pub(crate) stderr: OutputConfig,
    pub(crate) throw: ThrowBehavior,
    pub(crate) timeout: Option<TimeoutConfig>,
    pub(crate) pipeline: Option<Pipeline>,
    /// Stored error from builder methods like `.glob()`, returned at execution time.
    pub(crate) deferred_error: Option<String>,
    /// Swallow all errors (not just exit codes).
    pub(crate) no_err: bool,
    /// Suppress warnings printed when no_err swallows an error.
    pub(crate) no_warn: bool,
    /// Print command before executing.
    pub(crate) verbose: bool,
    /// Print command but don't execute.
    pub(crate) dry: bool,
}

impl CmdInner {
    pub(crate) fn new<S: Into<String>>(program: S) -> Self {
        CmdInner {
            program: program.into(),
            args: Vec::new(),
            env_vars: HashMap::new(),
            env_remove: Vec::new(),
            env_clear: false,
            cwd: None,
            stdin: StdinConfig::Inherit,
            stdout: OutputConfig::Inherit,
            stderr: OutputConfig::Inherit,
            throw: ThrowBehavior::ThrowAlways,
            timeout: None,
            pipeline: None,
            deferred_error: None,
            no_err: false,
            no_warn: false,
            verbose: false,
            dry: false,
        }
    }

    pub(crate) fn display_string(&self) -> String {
        let mut parts = vec![self.program.clone()];
        parts.extend(self.args.iter().map(|a| {
            if a.contains(' ') || a.contains('\'') || a.contains('"') {
                crate::escape_arg(a)
            } else {
                a.clone()
            }
        }));
        parts.join(" ")
    }

    pub(crate) fn execute_inner(&self) -> Result<RawOutput> {
        // Validate cwd
        if let Some(ref cwd) = self.cwd {
            if !cwd.exists() {
                return Err(CmdError::CwdNotFound {
                    path: cwd.display().to_string(),
                });
            }
        }

        let mut command = Command::new(&self.program);
        command.args(&self.args);

        if self.env_clear {
            command.env_clear();
        }
        for (k, v) in &self.env_vars {
            command.env(k, v);
        }
        for k in &self.env_remove {
            command.env_remove(k);
        }

        if let Some(ref cwd) = self.cwd {
            command.current_dir(cwd);
        }

        // Stdin
        match &self.stdin {
            StdinConfig::Inherit => {
                command.stdin(Stdio::inherit());
            }
            StdinConfig::Null => {
                command.stdin(Stdio::null());
            }
            StdinConfig::Bytes(_) | StdinConfig::Path(_) => {
                command.stdin(Stdio::piped());
            }
        }

        // Stdout
        match &self.stdout {
            OutputConfig::Inherit => {
                command.stdout(Stdio::inherit());
            }
            OutputConfig::Null => {
                command.stdout(Stdio::null());
            }
            OutputConfig::Capture
            | OutputConfig::File(_)
            | OutputConfig::Append(_)
            | OutputConfig::ToOther => {
                command.stdout(Stdio::piped());
            }
        }

        // Stderr
        match &self.stderr {
            OutputConfig::Inherit => {
                command.stderr(Stdio::inherit());
            }
            OutputConfig::Null => {
                command.stderr(Stdio::null());
            }
            OutputConfig::Capture
            | OutputConfig::File(_)
            | OutputConfig::Append(_)
            | OutputConfig::ToOther => {
                command.stderr(Stdio::piped());
            }
        }

        // Spawn
        let mut child = match command.spawn() {
            Ok(child) => child,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                return Err(CmdError::NotFound {
                    program: self.program.clone(),
                });
            }
            Err(e) => return Err(CmdError::Io(e)),
        };

        // Write stdin
        match &self.stdin {
            StdinConfig::Bytes(data) => {
                if let Some(mut stdin) = child.stdin.take() {
                    let _ = stdin.write_all(data);
                    drop(stdin);
                }
            }
            StdinConfig::Path(path) => {
                if let Some(mut stdin) = child.stdin.take() {
                    let data = std::fs::read(path)?;
                    let _ = stdin.write_all(&data);
                    drop(stdin);
                }
            }
            _ => {}
        }

        // Wait with optional timeout
        let status = if let Some(ref tc) = self.timeout {
            let start = std::time::Instant::now();
            let mut signal_sent = false;
            let duration = tc.duration;
            let signal = tc.signal;
            let kill_after = tc.kill_after;
            loop {
                match child.try_wait() {
                    Ok(Some(status)) => {
                        if signal_sent {
                            return Err(CmdError::Timeout { duration });
                        }
                        break Ok(status);
                    }
                    Ok(None) => {
                        let elapsed = start.elapsed();
                        if !signal_sent && elapsed >= duration {
                            unsafe {
                                libc::kill(child.id() as libc::pid_t, signal);
                            }
                            signal_sent = true;
                            if signal == libc::SIGKILL || kill_after.is_none() {
                                let _ = child.wait();
                                return Err(CmdError::Timeout { duration });
                            }
                        } else if signal_sent {
                            if let Some(grace) = kill_after {
                                if elapsed >= duration + grace {
                                    let _ = child.kill();
                                    let _ = child.wait();
                                    return Err(CmdError::Timeout { duration });
                                }
                            }
                        }
                        std::thread::sleep(Duration::from_millis(10));
                    }
                    Err(e) => break Err(e),
                }
            }
        } else {
            child.wait()
        };

        let status = status?;

        // Read captured output
        let mut stdout_bytes = Vec::new();
        let mut stderr_bytes = Vec::new();

        if let Some(mut out) = child.stdout.take() {
            out.read_to_end(&mut stdout_bytes)?;
        }
        if let Some(mut err) = child.stderr.take() {
            err.read_to_end(&mut stderr_bytes)?;
        }

        // Handle file redirects
        match &self.stdout {
            OutputConfig::File(path) => {
                std::fs::write(path, &stdout_bytes)?;
                stdout_bytes.clear();
            }
            OutputConfig::Append(path) => {
                let mut file = std::fs::OpenOptions::new()
                    .create(true)
                    .append(true)
                    .open(path)?;
                file.write_all(&stdout_bytes)?;
                stdout_bytes.clear();
            }
            _ => {}
        }

        match &self.stderr {
            OutputConfig::File(path) => {
                std::fs::write(path, &stderr_bytes)?;
                stderr_bytes.clear();
            }
            OutputConfig::Append(path) => {
                let mut file = std::fs::OpenOptions::new()
                    .create(true)
                    .append(true)
                    .open(path)?;
                file.write_all(&stderr_bytes)?;
                stderr_bytes.clear();
            }
            _ => {}
        }

        // Handle ToOther redirects
        if matches!(self.stderr, OutputConfig::ToOther) {
            stdout_bytes.extend_from_slice(&stderr_bytes);
            stderr_bytes.clear();
        }

        let code = status.code().unwrap_or(-1);

        // Check throw behavior
        let should_throw = match &self.throw {
            ThrowBehavior::ThrowAlways => code != 0,
            ThrowBehavior::NoThrow => false,
            ThrowBehavior::NoThrowOn(codes) => code != 0 && !codes.contains(&code),
        };

        if should_throw {
            let stderr_str = if stderr_bytes.is_empty() {
                None
            } else {
                String::from_utf8(stderr_bytes.clone()).ok()
            };
            return Err(CmdError::ExitStatus {
                code,
                stderr: stderr_str,
            });
        }

        Ok(RawOutput {
            code,
            stdout: stdout_bytes,
            stderr: stderr_bytes,
        })
    }
}

// ── Cmd<O, E> ──

/// A command builder with type-state tracking of stream configuration.
///
/// `O` tracks stdout: [`Captured`] (not redirected, default) or [`Redirected`].
/// `E` tracks stderr: [`Captured`] (not redirected, default) or [`Redirected`].
///
/// By default, stdout and stderr are **forwarded to the terminal** (inherited
/// from the parent process). Use [`.capture()`](Cmd::capture) or convenience
/// methods like [`run_stdout`](Cmd::run_stdout) to capture output into memory.
///
/// Created via [`cmd!`](crate::cmd!), [`shell!`](crate::shell!),
/// [`Cmd::new`], [`Cmd::parse`], or [`Cmd::shell`].
///
/// Nothing is executed until you call one of the execution methods
/// ([`run`](Cmd::run), [`run_stdout`](Cmd::run_stdout), etc.).
pub struct Cmd<O = Captured, E = Captured> {
    pub(crate) inner: CmdInner,
    pub(crate) _phantom: PhantomData<(O, E)>,
}

impl<O, E> Clone for Cmd<O, E> {
    fn clone(&self) -> Self {
        Cmd {
            inner: self.inner.clone(),
            _phantom: PhantomData,
        }
    }
}

impl<O, E> std::fmt::Debug for Cmd<O, E> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Cmd")
            .field("inner", &self.inner)
            .finish()
    }
}

// ── Constructors (return Cmd<Captured, Captured>) ──

impl Cmd {
    /// Create a new command with the given program name.
    pub fn new<S: Into<String>>(program: S) -> Self {
        Cmd {
            inner: CmdInner::new(program),
            _phantom: PhantomData,
        }
    }

    /// Parse a command string by splitting on whitespace.
    pub fn parse(cmd_str: &str) -> Self {
        let parts: Vec<&str> = cmd_str.split_whitespace().collect();
        if parts.is_empty() {
            return Cmd::new("");
        }
        let mut cmd = Cmd::new(parts[0]);
        for part in &parts[1..] {
            cmd.inner.args.push(part.to_string());
        }
        cmd
    }

    /// Create a shell command passed to `/bin/sh -c`.
    pub fn shell(cmd_str: &str) -> Self {
        let mut cmd = Cmd::new("/bin/sh");
        cmd.inner.args = vec!["-c".to_string(), cmd_str.to_string()];
        cmd
    }

    fn with_pipeline(pipeline: Pipeline) -> Self {
        let mut cmd = Cmd::new("__pipeline__");
        cmd.inner.pipeline = Some(pipeline);
        cmd
    }
}

// ── Builder methods (for all O, E) ──

impl<O, E> Cmd<O, E> {
    // ── Arguments ──

    /// Append a single argument.
    pub fn arg<S: AsRef<OsStr>>(mut self, arg: S) -> Self {
        self.inner
            .args
            .push(arg.as_ref().to_string_lossy().to_string());
        self
    }

    /// Append multiple arguments from an iterator.
    pub fn args<I, S>(mut self, args: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: AsRef<OsStr>,
    {
        for a in args {
            self.inner
                .args
                .push(a.as_ref().to_string_lossy().to_string());
        }
        self
    }

    /// Append arguments from anything that implements [`IntoArgs`](crate::IntoArgs).
    pub fn push_args<A: crate::IntoArgs>(mut self, args: A) -> Self {
        for a in args.into_args() {
            self.inner.args.push(a);
        }
        self
    }

    /// Expand a glob pattern and append all matching files as arguments.
    ///
    /// The glob is expanded immediately. If the pattern is invalid, matches
    /// zero files, or encounters a permission error, the error is deferred
    /// and returned when the command is executed.
    ///
    /// ```no_run
    /// # use raxx::Cmd;
    /// let text = Cmd::new("echo")
    ///     .arg("hello")
    ///     .glob("src/**/*.rs")
    ///     .run_stdout().unwrap();
    /// ```
    pub fn glob(mut self, pattern: &str) -> Self {
        match crate::glob_util::glob(pattern) {
            Ok(paths) => {
                for p in paths {
                    self.inner.args.push(p.to_string_lossy().to_string());
                }
            }
            Err(e) => {
                if self.inner.deferred_error.is_none() {
                    self.inner.deferred_error = Some(format!("{e}"));
                }
            }
        }
        self
    }

    /// Internal: set a deferred error (used by the `shell!` macro for inline glob errors).
    #[doc(hidden)]
    pub fn _set_deferred_error(mut self, msg: String) -> Self {
        if self.inner.deferred_error.is_none() {
            self.inner.deferred_error = Some(msg);
        }
        self
    }

    // ── Environment ──

    /// Set an environment variable for the command.
    pub fn env<K: Into<String>, V: Into<String>>(mut self, key: K, val: V) -> Self {
        self.inner.env_vars.insert(key.into(), val.into());
        self
    }

    /// Set multiple environment variables from an iterator.
    pub fn envs<I, K, V>(mut self, vars: I) -> Self
    where
        I: IntoIterator<Item = (K, V)>,
        K: Into<String>,
        V: Into<String>,
    {
        for (k, v) in vars {
            self.inner.env_vars.insert(k.into(), v.into());
        }
        self
    }

    /// Remove an environment variable.
    pub fn env_remove<K: Into<String>>(mut self, key: K) -> Self {
        self.inner.env_remove.push(key.into());
        self
    }

    /// Clear all environment variables.
    pub fn env_clear(mut self) -> Self {
        self.inner.env_clear = true;
        self
    }

    // ── Working directory ──

    /// Set the working directory for the command.
    pub fn cwd<P: AsRef<Path>>(mut self, dir: P) -> Self {
        self.inner.cwd = Some(dir.as_ref().to_path_buf());
        self
    }

    // ── Stdin ──

    /// Set stdin to a UTF-8 string.
    pub fn stdin_text<S: Into<String>>(mut self, text: S) -> Self {
        self.inner.stdin = StdinConfig::Bytes(text.into().into_bytes());
        self
    }

    /// Set stdin to raw bytes.
    pub fn stdin_bytes<B: Into<Vec<u8>>>(mut self, bytes: B) -> Self {
        self.inner.stdin = StdinConfig::Bytes(bytes.into());
        self
    }

    /// Set stdin to read from a file.
    pub fn stdin_path<P: AsRef<Path>>(mut self, path: P) -> Self {
        self.inner.stdin = StdinConfig::Path(path.as_ref().to_path_buf());
        self
    }

    /// Set stdin to `/dev/null`.
    pub fn stdin_null(mut self) -> Self {
        self.inner.stdin = StdinConfig::Null;
        self
    }

    // ── Behavior ──

    /// Swallow **all** errors: exit-code errors, command-not-found, permission
    /// errors, IO errors, timeouts, etc.  Serious errors (not-found, permission)
    /// print a warning to stderr.  Use [`.no_nothin()`](Cmd::no_nothin) to
    /// silence those warnings too.
    pub fn no_err(mut self) -> Self {
        self.inner.no_err = true;
        self
    }

    /// Like [`.no_err()`](Cmd::no_err) but also silences the warnings.
    pub fn no_nothin(mut self) -> Self {
        self.inner.no_err = true;
        self.inner.no_warn = true;
        self
    }

    /// Don't error on non-zero exit codes (but still errors on IO/not-found/etc).
    ///
    /// Prefer [`.no_err()`](Cmd::no_err) to swallow all errors, or
    /// [`.run_no_exit_err()`](Cmd::run_no_exit_err) as an execution shorthand.
    pub fn no_exit_err(mut self) -> Self {
        self.inner.throw = ThrowBehavior::NoThrow;
        self
    }

    /// Don't error for specific exit codes.
    pub fn no_exit_err_on(mut self, codes: &[i32]) -> Self {
        self.inner.throw = ThrowBehavior::NoThrowOn(codes.to_vec());
        self
    }

    /// Apply a shared [`CmdOps`] configuration to this command.
    pub fn with_ops(mut self, ops: &CmdOps) -> Self {
        for (k, v) in &ops.env {
            self.inner.env_vars.insert(k.clone(), v.clone());
        }
        if let Some(ref cwd) = ops.cwd {
            self.inner.cwd = Some(cwd.clone());
        }
        // Replace shell program if this is a shell command (program=/bin/sh, args[0]=-c)
        if let Some((ref shell_prog, ref shell_flag)) = ops.shell {
            if self.inner.program == "/bin/sh"
                && self.inner.args.first().map(|s| s.as_str()) == Some("-c")
            {
                self.inner.program = shell_prog.clone();
                self.inner.args[0] = shell_flag.clone();
            }
        }
        if ops.verbose {
            self.inner.verbose = true;
        }
        if ops.dry {
            self.inner.dry = true;
        }
        if ops.no_err {
            self.inner.no_err = true;
        }
        if ops.no_warn {
            self.inner.no_warn = true;
        }
        self
    }

    /// Set a timeout with SIGTERM then SIGKILL after 2s grace.
    pub fn timeout(mut self, duration: Duration) -> Self {
        self.inner.timeout = Some(TimeoutConfig {
            duration,
            signal: 15,
            kill_after: Some(Duration::from_secs(2)),
        });
        self
    }

    /// Set a timeout with a specific signal and optional SIGKILL grace period.
    pub fn timeout_signal(
        mut self,
        duration: Duration,
        signal: i32,
        kill_after: Option<Duration>,
    ) -> Self {
        self.inner.timeout = Some(TimeoutConfig {
            duration,
            signal,
            kill_after,
        });
        self
    }

    // ── Redirect (inherent method wrapping the trait) ──

    /// Redirect a stream to a target.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use raxx::{cmd, Stdout, Stderr, Null, Append};
    ///
    /// # fn main() -> raxx::Result<()> {
    /// // Stdout to file
    /// cmd!("echo", "hi").redirect(Stdout, "out.txt").run()?;
    ///
    /// // Stderr to /dev/null
    /// cmd!("echo", "hi").redirect(Stderr, Null).run()?;
    ///
    /// // Stderr to stdout
    /// cmd!("echo", "hi").redirect(Stderr, Stdout).run()?;
    ///
    /// // Stdout append
    /// cmd!("echo", "hi").redirect(Stdout, Append("log.txt")).run()?;
    /// # Ok(())
    /// # }
    /// ```
    pub fn redirect<S, T>(
        self,
        source: S,
        target: T,
    ) -> <Self as RedirectFrom<S, T>>::Output
    where
        Self: RedirectFrom<S, T>,
    {
        RedirectFrom::apply_redirect(self, source, target)
    }

    /// Swap stdout and stderr configurations (and type parameters).
    pub fn swap_streams(self) -> Cmd<E, O> {
        let mut inner = self.inner;
        std::mem::swap(&mut inner.stdout, &mut inner.stderr);
        Cmd {
            inner,
            _phantom: PhantomData,
        }
    }

    // ── Composition (always returns Cmd<Captured, Captured>) ──

    /// Pipe this command's stdout into another command's stdin (`|`).
    pub fn pipe<O2, E2>(self, other: Cmd<O2, E2>) -> Cmd {
        let left = self.into_pipeline();
        let right = other.into_pipeline();
        Cmd::with_pipeline(Pipeline::Pipe(Box::new(left), Box::new(right)))
    }

    /// Run another command only if this one succeeds (`&&`).
    pub fn and<O2, E2>(self, other: Cmd<O2, E2>) -> Cmd {
        let left = self.into_pipeline();
        let right = other.into_pipeline();
        Cmd::with_pipeline(Pipeline::And(Box::new(left), Box::new(right)))
    }

    /// Run another command only if this one fails (`||`).
    pub fn or<O2, E2>(self, other: Cmd<O2, E2>) -> Cmd {
        let left = self.into_pipeline();
        let right = other.into_pipeline();
        Cmd::with_pipeline(Pipeline::Or(Box::new(left), Box::new(right)))
    }

    /// Run another command after this one regardless of exit code (`;`).
    pub fn then<O2, E2>(self, other: Cmd<O2, E2>) -> Cmd {
        let left = self.into_pipeline();
        let right = other.into_pipeline();
        Cmd::with_pipeline(Pipeline::Then(Box::new(left), Box::new(right)))
    }

    fn into_pipeline(self) -> Pipeline {
        if let Some(p) = self.inner.pipeline {
            p
        } else {
            Pipeline::Single(Box::new(self.inner))
        }
    }

    // ── Execution ──

    fn execute_to_raw(self) -> Result<RawOutput> {
        let no_err = self.inner.no_err;
        let no_warn = self.inner.no_warn;

        let result = self.execute_to_raw_inner();

        match result {
            Ok(raw) => Ok(raw),
            Err(e) if no_err => {
                if !no_warn {
                    eprintln!("[raxx warning] {}", e);
                }
                Ok(RawOutput {
                    code: -1,
                    stdout: Vec::new(),
                    stderr: Vec::new(),
                })
            }
            Err(e) => Err(e),
        }
    }

    fn execute_to_raw_inner(mut self) -> Result<RawOutput> {
        // Check for deferred errors (e.g. from .glob() or shell! glob interpolation)
        if let Some(ref err_msg) = self.inner.deferred_error {
            return Err(CmdError::Io(std::io::Error::new(
                std::io::ErrorKind::Other,
                err_msg.clone(),
            )));
        }

        // Verbose / dry mode
        if self.inner.verbose || self.inner.dry {
            eprintln!("$ {}", self.inner.display_string());
        }
        if self.inner.dry {
            return Ok(RawOutput {
                code: 0,
                stdout: Vec::new(),
                stderr: Vec::new(),
            });
        }

        if let Some(pipeline) = self.inner.pipeline.take() {
            let capture_stdout = matches!(self.inner.stdout, OutputConfig::Capture);
            let capture_stderr = matches!(self.inner.stderr, OutputConfig::Capture);
            let result = pipeline.execute(capture_stdout, capture_stderr);
            let no_throw = matches!(self.inner.throw, ThrowBehavior::NoThrow);
            match result {
                Ok(raw) => Ok(raw),
                Err(CmdError::ExitStatus { code, .. }) if no_throw => Ok(RawOutput {
                    code,
                    stdout: Vec::new(),
                    stderr: Vec::new(),
                }),
                Err(e) => Err(e),
            }
        } else {
            self.inner.execute_inner()
        }
    }

    /// Execute the command and return the full result.
    ///
    /// By default, stdout and stderr are forwarded to the terminal (not
    /// captured). Use [`.capture()`](Cmd::capture) before `.run()` to
    /// capture them, or use [`run_stdout`](Cmd::run_stdout) /
    /// [`run_out`](Cmd::run_out) which auto-capture.
    pub fn run(self) -> Result<CmdResult<O, E>> {
        let raw = self.execute_to_raw()?;
        Ok(CmdResult::from_raw(raw))
    }

    /// Execute and return just the exit code. Never errors on non-zero codes.
    pub fn run_exit_code(mut self) -> Result<i32> {
        self.inner.throw = ThrowBehavior::NoThrow;
        if self.inner.pipeline.is_some() {
            self.inner.stdout = OutputConfig::Null;
        }
        let raw = self.execute_to_raw()?;
        Ok(raw.code)
    }

    /// Execute and return whether the command succeeded (exit code 0).
    /// Never errors on non-zero codes.
    pub fn run_success(mut self) -> Result<bool> {
        self.inner.throw = ThrowBehavior::NoThrow;
        let raw = self.execute_to_raw()?;
        Ok(raw.code == 0)
    }

    /// Execute the command, ignoring non-zero exit codes.
    ///
    /// Unlike [`.no_err()`](Cmd::no_err), this only suppresses exit-code errors.
    /// IO errors, command-not-found, etc. still propagate.
    pub fn run_no_exit_err(mut self) -> Result<CmdResult<O, E>> {
        self.inner.throw = ThrowBehavior::NoThrow;
        let raw = self.execute_to_raw()?;
        Ok(CmdResult::from_raw(raw))
    }

    /// Execute the command, ignoring non-zero exit codes.
    /// Alias for [`.run_no_exit_err()`](Cmd::run_no_exit_err).
    pub fn run_ignore_code(self) -> Result<CmdResult<O, E>> {
        self.run_no_exit_err()
    }

    /// Execute the command, swallowing all errors silently.
    ///
    /// Always returns a [`CmdResult`] — never panics, never returns `Err`.
    /// If the command fails for any reason (not found, bad exit code, IO error,
    /// etc.), returns a stub result with `code = -1` and empty output.
    ///
    /// ```no_run
    /// use raxx::cmd;
    ///
    /// // No ? or unwrap needed
    /// let result = cmd!("rm", "maybe.txt").run_and_forget();
    /// ```
    pub fn run_and_forget(self) -> CmdResult<O, E> {
        let result = self.no_err().run();
        // no_err guarantees Ok, but handle defensively
        match result {
            Ok(r) => r,
            Err(_) => CmdResult::from_raw(RawOutput {
                code: -1,
                stdout: Vec::new(),
                stderr: Vec::new(),
            }),
        }
    }
}

// ── Stdout-specific methods (O = Captured) ──

impl<E> Cmd<Captured, E> {
    /// Execute and return stdout as an untrimmed string.
    ///
    /// Automatically captures stdout (even though the default is to forward
    /// to the terminal).
    pub fn run_stdout(self) -> Result<String> {
        let result = self.capture_stdout().run()?;
        Ok(result.stdout())
    }

    /// Execute and parse stdout as JSON.
    ///
    /// Automatically captures stdout.
    pub fn run_stdout_json<T: serde::de::DeserializeOwned>(self) -> Result<T> {
        let result = self.capture_stdout().run()?;
        result.stdout_json()
    }

    /// Explicitly capture stdout into memory so it's available on the result.
    ///
    /// By default stdout is forwarded to the terminal. Call this (or use
    /// [`run_stdout`](Cmd::run_stdout)) to capture it.
    pub fn capture_stdout(mut self) -> Self {
        if matches!(self.inner.stdout, OutputConfig::Inherit) {
            self.inner.stdout = OutputConfig::Capture;
        }
        self
    }

    /// Suppress stdout (redirect to `/dev/null`).
    pub fn quiet_stdout(self) -> Cmd<Redirected, E> {
        self.redirect(Stdout, Null)
    }
}

// ── Stderr-specific methods (E = Captured) ──

impl<O> Cmd<O, Captured> {
    /// Execute and return stderr as an untrimmed string.
    ///
    /// Automatically captures stderr.
    pub fn run_stderr(self) -> Result<String> {
        let result = self.capture_stderr().run()?;
        Ok(result.stderr())
    }

    /// Execute and parse stderr as JSON.
    ///
    /// Automatically captures stderr.
    pub fn run_stderr_json<T: serde::de::DeserializeOwned>(self) -> Result<T> {
        let result = self.capture_stderr().run()?;
        result.stderr_json()
    }

    /// Explicitly capture stderr into memory so it's available on the result.
    ///
    /// By default stderr is forwarded to the terminal. Call this (or use
    /// [`run_stderr`](Cmd::run_stderr)) to capture it.
    pub fn capture_stderr(mut self) -> Self {
        if matches!(self.inner.stderr, OutputConfig::Inherit) {
            self.inner.stderr = OutputConfig::Capture;
        }
        self
    }

    /// Suppress stderr (redirect to `/dev/null`).
    pub fn quiet_stderr(self) -> Cmd<O, Redirected> {
        self.redirect(Stderr, Null)
    }
}

// ── Both-captured methods ──

impl Cmd<Captured, Captured> {
    /// Execute and return stdout + stderr concatenated.
    ///
    /// Automatically captures both streams.
    pub fn run_out(self) -> Result<String> {
        let result = self.capture().run()?;
        Ok(result.out())
    }

    /// Explicitly capture both stdout and stderr into memory.
    ///
    /// By default both streams are forwarded to the terminal. Call this
    /// before [`.run()`](Cmd::run) to make stdout/stderr data available
    /// on the result.
    ///
    /// ```no_run
    /// use raxx::cmd;
    ///
    /// # fn main() -> raxx::Result<()> {
    /// let result = cmd!("echo", "hello").capture().run()?;
    /// println!("{}", result.stdout_trimmed());
    /// # Ok(())
    /// # }
    /// ```
    pub fn capture(mut self) -> Self {
        if matches!(self.inner.stdout, OutputConfig::Inherit) {
            self.inner.stdout = OutputConfig::Capture;
        }
        if matches!(self.inner.stderr, OutputConfig::Inherit) {
            self.inner.stderr = OutputConfig::Capture;
        }
        self
    }

    /// Suppress both stdout and stderr (redirect to `/dev/null`).
    pub fn quiet(self) -> Cmd<Redirected, Redirected> {
        self.redirect(Stdout, Null).redirect(Stderr, Null)
    }
}
