use std::marker::PhantomData;

/// Marker type: stream data is captured and available on the result.
#[derive(Debug, Clone, Copy)]
pub struct Captured;

/// Marker type: stream was redirected (to file/null/other) — data not available.
#[derive(Debug, Clone, Copy)]
pub struct Redirected;

/// Internal raw output from command execution.
#[derive(Debug, Clone)]
pub(crate) struct RawOutput {
    pub code: i32,
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
}

/// The result of a command execution, with type-state tracking of stream availability.
///
/// - `O` tracks stdout: [`Captured`] means stdout accessors are available.
/// - `E` tracks stderr: [`Captured`] means stderr accessors are available.
///
/// If a stream was redirected (to a file, `/dev/null`, or the other stream),
/// its accessor methods are not available — attempting to call them is a **compile error**.
///
/// # Examples
///
/// ```no_run
/// use raxx::cmd;
///
/// # fn main() -> raxx::Result<()> {
/// let result = cmd!("echo", "hello").capture().run()?;
/// assert_eq!(result.stdout_trimmed(), "hello");
/// assert!(result.success());
/// # Ok(())
/// # }
/// ```
pub struct CmdResult<O = Captured, E = Captured> {
    /// The exit code of the command.
    pub code: i32,
    pub(crate) stdout_data: Vec<u8>,
    pub(crate) stderr_data: Vec<u8>,
    _phantom: PhantomData<(O, E)>,
}

impl<O, E> Clone for CmdResult<O, E> {
    fn clone(&self) -> Self {
        CmdResult {
            code: self.code,
            stdout_data: self.stdout_data.clone(),
            stderr_data: self.stderr_data.clone(),
            _phantom: PhantomData,
        }
    }
}

impl<O, E> std::fmt::Debug for CmdResult<O, E> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CmdResult")
            .field("code", &self.code)
            .field("stdout_len", &self.stdout_data.len())
            .field("stderr_len", &self.stderr_data.len())
            .finish()
    }
}

impl<O, E> CmdResult<O, E> {
    pub(crate) fn from_raw(raw: RawOutput) -> Self {
        CmdResult {
            code: raw.code,
            stdout_data: raw.stdout,
            stderr_data: raw.stderr,
            _phantom: PhantomData,
        }
    }

    /// Check if the command succeeded (exit code 0).
    pub fn success(&self) -> bool {
        self.code == 0
    }
}

// ── Stdout accessors (only when O = Captured) ──

impl<E> CmdResult<Captured, E> {
    /// Get stdout as an untrimmed string.
    pub fn stdout(&self) -> String {
        String::from_utf8_lossy(&self.stdout_data).to_string()
    }

    /// Get stdout as a trimmed string.
    pub fn stdout_trimmed(&self) -> String {
        self.stdout().trim().to_string()
    }

    /// Get stdout as raw bytes.
    pub fn stdout_bytes(&self) -> &[u8] {
        &self.stdout_data
    }

    /// Get stdout split into lines (from trimmed output).
    pub fn stdout_lines(&self) -> Vec<String> {
        self.stdout_trimmed()
            .lines()
            .map(String::from)
            .collect()
    }

    /// Parse stdout as JSON.
    pub fn stdout_json<T: serde::de::DeserializeOwned>(&self) -> crate::Result<T> {
        Ok(serde_json::from_slice(&self.stdout_data)?)
    }
}

// ── Stderr accessors (only when E = Captured) ──

impl<O> CmdResult<O, Captured> {
    /// Get stderr as an untrimmed string.
    pub fn stderr(&self) -> String {
        String::from_utf8_lossy(&self.stderr_data).to_string()
    }

    /// Get stderr as a trimmed string.
    pub fn stderr_trimmed(&self) -> String {
        self.stderr().trim().to_string()
    }

    /// Get stderr as raw bytes.
    pub fn stderr_bytes(&self) -> &[u8] {
        &self.stderr_data
    }

    /// Get stderr split into lines (from trimmed output).
    pub fn stderr_lines(&self) -> Vec<String> {
        self.stderr_trimmed()
            .lines()
            .map(String::from)
            .collect()
    }

    /// Parse stderr as JSON.
    pub fn stderr_json<T: serde::de::DeserializeOwned>(&self) -> crate::Result<T> {
        Ok(serde_json::from_slice(&self.stderr_data)?)
    }
}

// ── Combined accessors (only when both captured) ──

impl CmdResult<Captured, Captured> {
    /// Get stdout + stderr concatenated.
    pub fn out(&self) -> String {
        let mut s = self.stdout();
        s.push_str(&self.stderr());
        s
    }
}
