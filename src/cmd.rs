use std::collections::HashMap;
use std::ffi::OsStr;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::Duration;

use crate::error::{CmdError, Result};
use crate::pipeline::Pipeline;
use crate::result::CmdOutput;

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
    /// Capture into memory for later access via [`CmdOutput`].
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

/// A command builder.
///
/// Created via [`cmd!`](crate::cmd!), [`shell!`](crate::shell!),
/// [`Cmd::new`], [`Cmd::parse`], or [`Cmd::shell`].
///
/// Every builder method consumes `self` and returns a new `Cmd`, so you can
/// chain calls freely. Nothing is executed until you call one of the execution
/// methods ([`run`](Cmd::run), [`text`](Cmd::text), [`output`](Cmd::output), etc.).
///
/// # Examples
///
/// ```no_run
/// use raxx::{cmd, Cmd};
///
/// # fn main() -> raxx::Result<()> {
/// // Via macro
/// let text = cmd!("echo", "hello").text()?;
///
/// // Via builder
/// let text = Cmd::new("echo").arg("hello").text()?;
/// # Ok(())
/// # }
/// ```
#[derive(Debug, Clone)]
pub struct Cmd {
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
    pub(crate) timeout: Option<Duration>,
    pub(crate) pipeline: Option<Pipeline>,
}

impl Cmd {
    /// Create a new command with the given program name.
    ///
    /// ```no_run
    /// # use raxx::Cmd;
    /// let text = Cmd::new("echo").arg("hello").text().unwrap();
    /// assert_eq!(text, "hello");
    /// ```
    pub fn new<S: Into<String>>(program: S) -> Self {
        Cmd {
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
        }
    }

    /// Parse a command string by splitting on whitespace.
    ///
    /// The first token becomes the program, the rest become arguments. This
    /// is what [`cmd!`](crate::cmd!) calls internally.
    ///
    /// ```no_run
    /// # use raxx::Cmd;
    /// let text = Cmd::parse("echo hello world").text().unwrap();
    /// assert_eq!(text, "hello world");
    /// ```
    pub fn parse(cmd_str: &str) -> Self {
        let parts: Vec<&str> = cmd_str.split_whitespace().collect();
        if parts.is_empty() {
            return Cmd::new("");
        }
        let mut cmd = Cmd::new(parts[0]);
        for part in &parts[1..] {
            cmd.args.push(part.to_string());
        }
        cmd
    }

    /// Create a shell command passed to `/bin/sh -c`.
    ///
    /// This is what [`shell!`](crate::shell!) calls internally.
    ///
    /// ```no_run
    /// # use raxx::Cmd;
    /// let text = Cmd::shell("echo hello | tr a-z A-Z").text().unwrap();
    /// assert_eq!(text, "HELLO");
    /// ```
    pub fn shell(cmd_str: &str) -> Self {
        let mut cmd = Cmd::new("/bin/sh");
        cmd.args = vec!["-c".to_string(), cmd_str.to_string()];
        cmd
    }

    // ── Argument building ──

    /// Append a single argument.
    ///
    /// The argument is passed directly to the process without shell
    /// interpretation, so spaces and special characters are safe.
    ///
    /// ```no_run
    /// # use raxx::Cmd;
    /// let text = Cmd::new("echo").arg("hello world").text().unwrap();
    /// assert_eq!(text, "hello world");
    /// ```
    pub fn arg<S: AsRef<OsStr>>(mut self, arg: S) -> Self {
        self.args.push(arg.as_ref().to_string_lossy().to_string());
        self
    }

    /// Append multiple arguments from an iterator.
    ///
    /// ```no_run
    /// # use raxx::Cmd;
    /// let items = vec!["a", "b", "c"];
    /// let text = Cmd::new("echo").args(items).text().unwrap();
    /// assert_eq!(text, "a b c");
    /// ```
    pub fn args<I, S>(mut self, args: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: AsRef<OsStr>,
    {
        for a in args {
            self.args.push(a.as_ref().to_string_lossy().to_string());
        }
        self
    }

    /// Append arguments from anything that implements [`IntoArgs`](crate::IntoArgs).
    ///
    /// This handles both single values (`&str`, `String`) and collections
    /// (`Vec<T>`, `&[T]`), flattening collections into individual arguments.
    /// Used internally by the [`cmd!`](crate::cmd!) and [`shell!`](crate::shell!)
    /// macros.
    ///
    /// ```no_run
    /// # use raxx::Cmd;
    /// let files = vec!["a.txt", "b.txt"];
    /// let text = Cmd::new("echo").push_args(files).text().unwrap();
    /// assert_eq!(text, "a.txt b.txt");
    /// ```
    pub fn push_args<A: crate::IntoArgs>(mut self, args: A) -> Self {
        for a in args.into_args() {
            self.args.push(a);
        }
        self
    }

    // ── Environment ──

    /// Set an environment variable for the command.
    ///
    /// ```no_run
    /// # use raxx::shell;
    /// let text = shell!("echo $VAR").env("VAR", "hello").text().unwrap();
    /// assert_eq!(text, "hello");
    /// ```
    pub fn env<K: Into<String>, V: Into<String>>(mut self, key: K, val: V) -> Self {
        self.env_vars.insert(key.into(), val.into());
        self
    }

    /// Set multiple environment variables from an iterator of `(key, value)` pairs.
    ///
    /// ```no_run
    /// # use raxx::shell;
    /// let text = shell!("echo $A $B")
    ///     .envs([("A", "1"), ("B", "2")])
    ///     .text().unwrap();
    /// assert_eq!(text, "1 2");
    /// ```
    pub fn envs<I, K, V>(mut self, vars: I) -> Self
    where
        I: IntoIterator<Item = (K, V)>,
        K: Into<String>,
        V: Into<String>,
    {
        for (k, v) in vars {
            self.env_vars.insert(k.into(), v.into());
        }
        self
    }

    /// Remove an environment variable (unset it for the child process).
    pub fn env_remove<K: Into<String>>(mut self, key: K) -> Self {
        self.env_remove.push(key.into());
        self
    }

    /// Clear all environment variables, starting the child with an empty
    /// environment. Use with [`.env()`](Cmd::env) to set only the variables
    /// you need.
    pub fn env_clear(mut self) -> Self {
        self.env_clear = true;
        self
    }

    // ── Working directory ──

    /// Set the working directory for the command.
    ///
    /// Returns [`CmdError::CwdNotFound`] at execution time if the directory
    /// does not exist.
    pub fn cwd<P: AsRef<Path>>(mut self, dir: P) -> Self {
        self.cwd = Some(dir.as_ref().to_path_buf());
        self
    }

    // ── Stdin ──

    /// Set stdin to a UTF-8 string.
    ///
    /// ```no_run
    /// # use raxx::cmd;
    /// let text = cmd!("cat").stdin_text("hello").text().unwrap();
    /// assert_eq!(text, "hello");
    /// ```
    pub fn stdin_text<S: Into<String>>(mut self, text: S) -> Self {
        self.stdin = StdinConfig::Bytes(text.into().into_bytes());
        self
    }

    /// Set stdin to raw bytes.
    pub fn stdin_bytes<B: Into<Vec<u8>>>(mut self, bytes: B) -> Self {
        self.stdin = StdinConfig::Bytes(bytes.into());
        self
    }

    /// Set stdin to read from a file.
    pub fn stdin_path<P: AsRef<Path>>(mut self, path: P) -> Self {
        self.stdin = StdinConfig::Path(path.as_ref().to_path_buf());
        self
    }

    /// Set stdin to `/dev/null`.
    pub fn stdin_null(mut self) -> Self {
        self.stdin = StdinConfig::Null;
        self
    }

    // ── Stdout ──

    /// Capture stdout into memory.
    ///
    /// After execution, access the captured bytes via [`CmdOutput::stdout`]
    /// or helper methods like [`CmdOutput::stdout_text`].
    ///
    /// Note: the convenience methods [`.text()`](Cmd::text), [`.lines()`](Cmd::lines),
    /// [`.bytes()`](Cmd::bytes), and [`.json()`](Cmd::json) call this automatically.
    pub fn stdout_capture(mut self) -> Self {
        self.stdout = OutputConfig::Capture;
        self
    }

    /// Redirect stdout to a file (overwrite).
    pub fn stdout_path<P: AsRef<Path>>(mut self, path: P) -> Self {
        self.stdout = OutputConfig::File(path.as_ref().to_path_buf());
        self
    }

    /// Redirect stdout to a file (append).
    pub fn stdout_append<P: AsRef<Path>>(mut self, path: P) -> Self {
        self.stdout = OutputConfig::Append(path.as_ref().to_path_buf());
        self
    }

    /// Discard stdout (`/dev/null`).
    pub fn stdout_null(mut self) -> Self {
        self.stdout = OutputConfig::Null;
        self
    }

    /// Redirect stdout to stderr.
    pub fn stdout_to_stderr(mut self) -> Self {
        self.stdout = OutputConfig::ToOther;
        self
    }

    // ── Stderr ──

    /// Capture stderr into memory.
    pub fn stderr_capture(mut self) -> Self {
        self.stderr = OutputConfig::Capture;
        self
    }

    /// Redirect stderr to a file (overwrite).
    pub fn stderr_path<P: AsRef<Path>>(mut self, path: P) -> Self {
        self.stderr = OutputConfig::File(path.as_ref().to_path_buf());
        self
    }

    /// Redirect stderr to a file (append).
    pub fn stderr_append<P: AsRef<Path>>(mut self, path: P) -> Self {
        self.stderr = OutputConfig::Append(path.as_ref().to_path_buf());
        self
    }

    /// Discard stderr (`/dev/null`).
    pub fn stderr_null(mut self) -> Self {
        self.stderr = OutputConfig::Null;
        self
    }

    /// Redirect stderr to stdout.
    pub fn stderr_to_stdout(mut self) -> Self {
        self.stderr = OutputConfig::ToOther;
        self
    }

    // ── Behavior ──

    /// Don't return an error on non-zero exit codes.
    ///
    /// By default, any non-zero exit code produces [`CmdError::ExitStatus`].
    /// With `.no_throw()`, the command always returns `Ok` and you can check
    /// [`CmdOutput::code`] yourself.
    ///
    /// ```no_run
    /// # use raxx::cmd;
    /// let output = cmd!("false").no_throw().output().unwrap();
    /// assert_eq!(output.code, 1);
    /// ```
    pub fn no_throw(mut self) -> Self {
        self.throw = ThrowBehavior::NoThrow;
        self
    }

    /// Don't error for specific exit codes; error on all other non-zero codes.
    ///
    /// ```no_run
    /// # use raxx::shell;
    /// let output = shell!("exit 42").no_throw_on(&[42]).output().unwrap();
    /// assert_eq!(output.code, 42);
    /// ```
    pub fn no_throw_on(mut self, codes: &[i32]) -> Self {
        self.throw = ThrowBehavior::NoThrowOn(codes.to_vec());
        self
    }

    /// Suppress both stdout and stderr (redirect to `/dev/null`).
    ///
    /// Only affects streams that are currently set to `Inherit`. Does not
    /// override explicit captures or redirects.
    pub fn quiet(self) -> Self {
        self.quiet_stdout().quiet_stderr()
    }

    /// Suppress stdout only.
    pub fn quiet_stdout(mut self) -> Self {
        if matches!(self.stdout, OutputConfig::Inherit) {
            self.stdout = OutputConfig::Null;
        }
        self
    }

    /// Suppress stderr only.
    pub fn quiet_stderr(mut self) -> Self {
        if matches!(self.stderr, OutputConfig::Inherit) {
            self.stderr = OutputConfig::Null;
        }
        self
    }

    /// Set a timeout. If the command doesn't finish within the duration,
    /// it is killed and [`CmdError::Timeout`] is returned.
    pub fn timeout(mut self, duration: Duration) -> Self {
        self.timeout = Some(duration);
        self
    }

    // ── Composition ──

    fn with_pipeline(pipeline: Pipeline) -> Self {
        let mut cmd = Cmd::new("__pipeline__");
        cmd.pipeline = Some(pipeline);
        cmd
    }

    /// Pipe this command's stdout into another command's stdin (`|`).
    ///
    /// ```no_run
    /// # use raxx::cmd;
    /// let text = cmd!("echo", "hello")
    ///     .pipe(cmd!("tr", "a-z", "A-Z"))
    ///     .text().unwrap();
    /// assert_eq!(text, "HELLO");
    /// ```
    pub fn pipe(self, other: Cmd) -> Cmd {
        let left = self.into_pipeline();
        let right = other.into_pipeline();
        Cmd::with_pipeline(Pipeline::Pipe(Box::new(left), Box::new(right)))
    }

    /// Run another command only if this one succeeds, exit code 0 (`&&`).
    ///
    /// ```no_run
    /// # use raxx::cmd;
    /// cmd!("true").and(cmd!("echo", "success")).run().unwrap();
    /// ```
    pub fn and(self, other: Cmd) -> Cmd {
        let left = self.into_pipeline();
        let right = other.into_pipeline();
        Cmd::with_pipeline(Pipeline::And(Box::new(left), Box::new(right)))
    }

    /// Run another command only if this one fails, non-zero exit (`||`).
    ///
    /// ```no_run
    /// # use raxx::cmd;
    /// let text = cmd!("false").or(cmd!("echo", "fallback")).text().unwrap();
    /// assert_eq!(text, "fallback");
    /// ```
    pub fn or(self, other: Cmd) -> Cmd {
        let left = self.into_pipeline();
        let right = other.into_pipeline();
        Cmd::with_pipeline(Pipeline::Or(Box::new(left), Box::new(right)))
    }

    /// Run another command after this one regardless of exit code (`;`).
    pub fn then(self, other: Cmd) -> Cmd {
        let left = self.into_pipeline();
        let right = other.into_pipeline();
        Cmd::with_pipeline(Pipeline::Then(Box::new(left), Box::new(right)))
    }

    fn into_pipeline(self) -> Pipeline {
        if let Some(p) = self.pipeline {
            p
        } else {
            Pipeline::Single(Box::new(self))
        }
    }

    // ── Execution ──

    /// Execute the command, inheriting stdio. Returns `Ok(())` on success.
    ///
    /// This is the simplest way to run a command when you don't need its
    /// output. Stdout and stderr go to the terminal.
    pub fn run(self) -> Result<()> {
        let _ = self.output()?;
        Ok(())
    }

    /// Execute and capture stdout as a trimmed UTF-8 string.
    ///
    /// Leading and trailing whitespace (including the trailing newline) is
    /// removed. Equivalent to `.stdout_capture().output()?.stdout_text()`.
    ///
    /// ```no_run
    /// # use raxx::cmd;
    /// let text = cmd!("echo", "hello").text().unwrap();
    /// assert_eq!(text, "hello");
    /// ```
    pub fn text(self) -> Result<String> {
        let output = self.stdout_capture().output()?;
        output.stdout_text()
    }

    /// Execute and capture stdout as lines (`Vec<String>`).
    ///
    /// Output is trimmed, then split on newlines.
    pub fn lines(self) -> Result<Vec<String>> {
        let output = self.stdout_capture().output()?;
        output.stdout_lines()
    }

    /// Execute and capture stdout as raw bytes.
    ///
    /// Unlike [`.text()`](Cmd::text), this does **not** trim the output.
    pub fn bytes(self) -> Result<Vec<u8>> {
        let output = self.stdout_capture().output()?;
        Ok(output.stdout)
    }

    /// Execute and parse stdout as JSON.
    ///
    /// The type parameter must implement `serde::Deserialize`.
    ///
    /// ```no_run
    /// # use raxx::cmd;
    /// let val: serde_json::Value = cmd!("echo", r#"{"a":1}"#).json().unwrap();
    /// assert_eq!(val["a"], 1);
    /// ```
    pub fn json<T: serde::de::DeserializeOwned>(self) -> Result<T> {
        let output = self.stdout_capture().output()?;
        output.stdout_json()
    }

    /// Execute and return just the exit code. Never returns an error for
    /// non-zero exit codes.
    ///
    /// ```no_run
    /// # use raxx::cmd;
    /// let code = cmd!("false").status_code().unwrap();
    /// assert_eq!(code, 1);
    /// ```
    pub fn status_code(self) -> Result<i32> {
        let mut cmd = self.no_throw();
        if cmd.pipeline.is_some() {
            cmd.stdout = OutputConfig::Null;
        }
        let output = cmd.output()?;
        Ok(output.code)
    }

    /// Execute the command and return the full [`CmdOutput`].
    ///
    /// Use [`.stdout_capture()`](Cmd::stdout_capture) and/or
    /// [`.stderr_capture()`](Cmd::stderr_capture) before calling this to
    /// populate [`CmdOutput::stdout`] and [`CmdOutput::stderr`].
    pub fn output(self) -> Result<CmdOutput> {
        if let Some(ref pipeline) = self.pipeline {
            let capture = matches!(self.stdout, OutputConfig::Capture);
            let no_throw = matches!(self.throw, ThrowBehavior::NoThrow);
            let result = pipeline.execute(capture);
            if no_throw {
                match result {
                    Err(CmdError::ExitStatus { code, .. }) => {
                        return Ok(CmdOutput {
                            code,
                            stdout: Vec::new(),
                            stderr: Vec::new(),
                        });
                    }
                    other => return other,
                }
            }
            return result;
        }
        self.execute_inner()
    }

    pub(crate) fn execute_inner(&self) -> Result<CmdOutput> {
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
        let status = if let Some(duration) = self.timeout {
            let start = std::time::Instant::now();
            loop {
                match child.try_wait() {
                    Ok(Some(status)) => break Ok(status),
                    Ok(None) => {
                        if start.elapsed() >= duration {
                            let _ = child.kill();
                            let _ = child.wait();
                            return Err(CmdError::Timeout { duration });
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

        Ok(CmdOutput {
            code,
            stdout: stdout_bytes,
            stderr: stderr_bytes,
        })
    }
}
