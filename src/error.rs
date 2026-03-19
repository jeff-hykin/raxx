use std::fmt;

/// Result type alias for raxx operations.
pub type Result<T> = std::result::Result<T, CmdError>;

/// Error type for command execution failures.
#[derive(Debug)]
pub enum CmdError {
    /// Command exited with a non-zero status code.
    ExitStatus {
        code: i32,
        stderr: Option<String>,
    },
    /// Command was killed by a signal.
    Signal {
        signal: i32,
    },
    /// Command timed out.
    Timeout {
        duration: std::time::Duration,
    },
    /// IO error during command execution.
    Io(std::io::Error),
    /// UTF-8 decoding error.
    Utf8(std::string::FromUtf8Error),
    /// JSON deserialization error.
    Json(serde_json::Error),
    /// The command was not found.
    NotFound {
        program: String,
    },
    /// The specified working directory does not exist.
    CwdNotFound {
        path: String,
    },
    /// Stdin pipe broken (upstream command failed).
    BrokenPipe {
        upstream_code: i32,
    },
    /// Glob pattern matched zero files.
    GlobNoMatches {
        pattern: String,
    },
}

impl fmt::Display for CmdError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CmdError::ExitStatus { code, stderr } => {
                write!(f, "Exited with code: {code}")?;
                if let Some(stderr) = stderr {
                    if !stderr.is_empty() {
                        write!(f, "\nstderr: {stderr}")?;
                    }
                }
                Ok(())
            }
            CmdError::Signal { signal } => write!(f, "Killed by signal: {signal}"),
            CmdError::Timeout { duration } => {
                write!(f, "Timed out after {:.1}s", duration.as_secs_f64())
            }
            CmdError::Io(e) => write!(f, "IO error: {e}"),
            CmdError::Utf8(e) => write!(f, "UTF-8 error: {e}"),
            CmdError::Json(e) => write!(f, "JSON error: {e}"),
            CmdError::NotFound { program } => {
                write!(f, "Command not found: {program}")
            }
            CmdError::CwdNotFound { path } => {
                write!(
                    f,
                    "Failed to launch command because the cwd does not exist: {path}"
                )
            }
            CmdError::BrokenPipe { upstream_code } => {
                write!(
                    f,
                    "Stdin pipe broken. Upstream exited with code: {upstream_code}"
                )
            }
            CmdError::GlobNoMatches { pattern } => {
                write!(f, "Glob pattern matched zero files: {pattern}")
            }
        }
    }
}

impl std::error::Error for CmdError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            CmdError::Io(e) => Some(e),
            CmdError::Utf8(e) => Some(e),
            CmdError::Json(e) => Some(e),
            _ => None,
        }
    }
}

impl From<std::io::Error> for CmdError {
    fn from(e: std::io::Error) -> Self {
        CmdError::Io(e)
    }
}

impl From<std::string::FromUtf8Error> for CmdError {
    fn from(e: std::string::FromUtf8Error) -> Self {
        CmdError::Utf8(e)
    }
}

impl From<serde_json::Error> for CmdError {
    fn from(e: serde_json::Error) -> Self {
        CmdError::Json(e)
    }
}
