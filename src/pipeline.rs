use crate::cmd::{CmdInner, OutputConfig, StdinConfig, ThrowBehavior};
use crate::error::Result;
use crate::result::RawOutput;

/// Represents a pipeline or chain of commands.
#[derive(Debug, Clone)]
pub(crate) enum Pipeline {
    /// A single command.
    Single(Box<CmdInner>),
    /// Pipe stdout of left into stdin of right (|).
    Pipe(Box<Pipeline>, Box<Pipeline>),
    /// Run right only if left succeeds (&&).
    And(Box<Pipeline>, Box<Pipeline>),
    /// Run right only if left fails (||).
    Or(Box<Pipeline>, Box<Pipeline>),
    /// Run right after left regardless of exit code (;).
    Then(Box<Pipeline>, Box<Pipeline>),
}

impl Pipeline {
    /// Execute the pipeline.
    pub fn execute(&self, capture_stdout: bool, capture_stderr: bool) -> Result<RawOutput> {
        match self {
            Pipeline::Single(inner) => {
                let mut inner = inner.as_ref().clone();
                if capture_stdout {
                    inner.stdout = OutputConfig::Capture;
                }
                if capture_stderr && matches!(inner.stderr, OutputConfig::Inherit) {
                    inner.stderr = OutputConfig::Capture;
                }
                inner.execute_inner()
            }
            Pipeline::Pipe(left, right) => {
                execute_pipe(left, right, capture_stdout, capture_stderr)
            }
            Pipeline::And(left, right) => {
                execute_and(left, right, capture_stdout, capture_stderr)
            }
            Pipeline::Or(left, right) => {
                execute_or(left, right, capture_stdout, capture_stderr)
            }
            Pipeline::Then(left, right) => {
                execute_then(left, right, capture_stdout, capture_stderr)
            }
        }
    }
}

fn execute_pipe(
    left: &Pipeline,
    right: &Pipeline,
    capture_stdout: bool,
    capture_stderr: bool,
) -> Result<RawOutput> {
    // Left always captures stdout to feed to right
    let left_output = left.execute(true, false)?;

    // Feed left's stdout into right's stdin
    match right {
        Pipeline::Single(inner) => {
            let mut inner = inner.as_ref().clone();
            inner.stdin = StdinConfig::Bytes(left_output.stdout);
            if capture_stdout {
                inner.stdout = OutputConfig::Capture;
            }
            if capture_stderr && matches!(inner.stderr, OutputConfig::Inherit) {
                inner.stderr = OutputConfig::Capture;
            }
            inner.execute_inner()
        }
        Pipeline::Pipe(inner_left, inner_right) => {
            let inner_left_output =
                inject_stdin_and_execute(inner_left, left_output.stdout, true, false)?;
            match inner_right.as_ref() {
                Pipeline::Single(inner) => {
                    let mut inner = inner.as_ref().clone();
                    inner.stdin = StdinConfig::Bytes(inner_left_output.stdout);
                    if capture_stdout {
                        inner.stdout = OutputConfig::Capture;
                    }
                    if capture_stderr && matches!(inner.stderr, OutputConfig::Inherit) {
                        inner.stderr = OutputConfig::Capture;
                    }
                    inner.execute_inner()
                }
                _ => {
                    let mut cat = CmdInner::new("cat");
                    cat.stdin = StdinConfig::Bytes(inner_left_output.stdout);
                    execute_pipe(
                        &Pipeline::Single(Box::new(cat)),
                        inner_right,
                        capture_stdout,
                        capture_stderr,
                    )
                }
            }
        }
        _ => inject_stdin_and_execute(right, left_output.stdout, capture_stdout, capture_stderr),
    }
}

/// Inject stdin bytes into the leftmost command of a pipeline and execute.
fn inject_stdin_and_execute(
    pipeline: &Pipeline,
    stdin_bytes: Vec<u8>,
    capture_stdout: bool,
    capture_stderr: bool,
) -> Result<RawOutput> {
    match pipeline {
        Pipeline::Single(inner) => {
            let mut inner = inner.as_ref().clone();
            inner.stdin = StdinConfig::Bytes(stdin_bytes);
            if capture_stdout {
                inner.stdout = OutputConfig::Capture;
            }
            if capture_stderr && matches!(inner.stderr, OutputConfig::Inherit) {
                inner.stderr = OutputConfig::Capture;
            }
            inner.execute_inner()
        }
        Pipeline::Pipe(left, right) => {
            let left_output =
                inject_stdin_and_execute(left, stdin_bytes, true, false)?;
            execute_pipe_with_bytes(right, left_output.stdout, capture_stdout, capture_stderr)
        }
        _ => pipeline.execute(capture_stdout, capture_stderr),
    }
}

fn execute_pipe_with_bytes(
    right: &Pipeline,
    stdin_bytes: Vec<u8>,
    capture_stdout: bool,
    capture_stderr: bool,
) -> Result<RawOutput> {
    match right {
        Pipeline::Single(inner) => {
            let mut inner = inner.as_ref().clone();
            inner.stdin = StdinConfig::Bytes(stdin_bytes);
            if capture_stdout {
                inner.stdout = OutputConfig::Capture;
            }
            if capture_stderr && matches!(inner.stderr, OutputConfig::Inherit) {
                inner.stderr = OutputConfig::Capture;
            }
            inner.execute_inner()
        }
        _ => inject_stdin_and_execute(right, stdin_bytes, capture_stdout, capture_stderr),
    }
}

fn execute_and(
    left: &Pipeline,
    right: &Pipeline,
    capture_stdout: bool,
    capture_stderr: bool,
) -> Result<RawOutput> {
    let left_output = left.execute(capture_stdout, capture_stderr)?;
    if left_output.code == 0 {
        right.execute(capture_stdout, capture_stderr)
    } else {
        Ok(left_output)
    }
}

fn execute_or(
    left: &Pipeline,
    right: &Pipeline,
    capture_stdout: bool,
    capture_stderr: bool,
) -> Result<RawOutput> {
    let left_result = execute_no_throw(left, capture_stdout, capture_stderr);
    match left_result {
        Ok(output) if output.code == 0 => Ok(output),
        _ => right.execute(capture_stdout, capture_stderr),
    }
}

/// Execute a pipeline but don't throw on non-zero exit codes.
fn execute_no_throw(
    pipeline: &Pipeline,
    capture_stdout: bool,
    capture_stderr: bool,
) -> Result<RawOutput> {
    match pipeline {
        Pipeline::Single(inner) => {
            let mut inner = inner.as_ref().clone();
            inner.throw = ThrowBehavior::NoThrow;
            if capture_stdout {
                inner.stdout = OutputConfig::Capture;
            }
            if capture_stderr && matches!(inner.stderr, OutputConfig::Inherit) {
                inner.stderr = OutputConfig::Capture;
            }
            inner.execute_inner()
        }
        _ => pipeline.execute(capture_stdout, capture_stderr),
    }
}

fn execute_then(
    left: &Pipeline,
    right: &Pipeline,
    capture_stdout: bool,
    capture_stderr: bool,
) -> Result<RawOutput> {
    let _ = execute_no_throw(left, false, false);
    right.execute(capture_stdout, capture_stderr)
}
