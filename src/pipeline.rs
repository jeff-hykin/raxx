use crate::cmd::Cmd;
use crate::error::Result;
use crate::result::CmdOutput;

/// Represents a pipeline or chain of commands.
#[derive(Debug, Clone)]
pub enum Pipeline {
    /// A single command.
    Single(Box<Cmd>),
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
    /// Execute the pipeline, capturing stdout of the final command.
    pub fn execute(&self, capture_stdout: bool) -> Result<CmdOutput> {
        match self {
            Pipeline::Single(cmd) => {
                if capture_stdout {
                    cmd.clone().stdout_capture().execute_inner()
                } else {
                    cmd.execute_inner()
                }
            }
            Pipeline::Pipe(left, right) => execute_pipe(left, right, capture_stdout),
            Pipeline::And(left, right) => execute_and(left, right, capture_stdout),
            Pipeline::Or(left, right) => execute_or(left, right, capture_stdout),
            Pipeline::Then(left, right) => execute_then(left, right, capture_stdout),
        }
    }
}

fn execute_pipe(left: &Pipeline, right: &Pipeline, capture_stdout: bool) -> Result<CmdOutput> {
    // Left always captures stdout to feed to right
    let left_output = left.execute(true)?;

    // Feed left's stdout into right's stdin
    match right {
        Pipeline::Single(cmd) => {
            let mut cmd = cmd.as_ref().clone().stdin_bytes(left_output.stdout);
            if capture_stdout {
                cmd = cmd.stdout_capture();
            }
            cmd.execute_inner()
        }
        Pipeline::Pipe(inner_left, inner_right) => {
            // For nested pipes like a | b | c, we need to feed into the leftmost of the right side
            // Recursively: execute right pipeline with left's output as stdin
            // We'll handle this by wrapping the left output through the pipeline
            let inner_left_output = inject_stdin_and_execute(inner_left, left_output.stdout, true)?;
            let right_cmd = match inner_right.as_ref() {
                Pipeline::Single(cmd) => {
                    let mut cmd = cmd.as_ref().clone().stdin_bytes(inner_left_output.stdout);
                    if capture_stdout {
                        cmd = cmd.stdout_capture();
                    }
                    cmd.execute_inner()
                }
                _ => {
                    execute_pipe(
                        &Pipeline::Single(Box::new(
                            Cmd::new("cat").stdin_bytes(inner_left_output.stdout),
                        )),
                        inner_right,
                        capture_stdout,
                    )
                }
            };
            right_cmd
        }
        _ => {
            // For other pipeline types on the right, inject stdin into the first command
            inject_stdin_and_execute(right, left_output.stdout, capture_stdout)
        }
    }
}

/// Inject stdin bytes into the leftmost command of a pipeline and execute.
fn inject_stdin_and_execute(
    pipeline: &Pipeline,
    stdin_bytes: Vec<u8>,
    capture_stdout: bool,
) -> Result<CmdOutput> {
    match pipeline {
        Pipeline::Single(cmd) => {
            let mut cmd = cmd.as_ref().clone().stdin_bytes(stdin_bytes);
            if capture_stdout {
                cmd = cmd.stdout_capture();
            }
            cmd.execute_inner()
        }
        Pipeline::Pipe(left, right) => {
            let left_output = inject_stdin_and_execute(left, stdin_bytes, true)?;
            execute_pipe_with_bytes(right, left_output.stdout, capture_stdout)
        }
        _ => {
            // For and/or/then, inject into the first command
            // This is a simplification
            pipeline.execute(capture_stdout)
        }
    }
}

fn execute_pipe_with_bytes(
    right: &Pipeline,
    stdin_bytes: Vec<u8>,
    capture_stdout: bool,
) -> Result<CmdOutput> {
    match right {
        Pipeline::Single(cmd) => {
            let mut cmd = cmd.as_ref().clone().stdin_bytes(stdin_bytes);
            if capture_stdout {
                cmd = cmd.stdout_capture();
            }
            cmd.execute_inner()
        }
        _ => inject_stdin_and_execute(right, stdin_bytes, capture_stdout),
    }
}

fn execute_and(left: &Pipeline, right: &Pipeline, capture_stdout: bool) -> Result<CmdOutput> {
    // Left runs with no_throw to check its exit code
    let left_output = left.execute(capture_stdout)?;
    if left_output.code == 0 {
        right.execute(capture_stdout)
    } else {
        Ok(left_output)
    }
}

fn execute_or(left: &Pipeline, right: &Pipeline, capture_stdout: bool) -> Result<CmdOutput> {
    // For or, left needs no_throw behavior so we can check its code
    // But Single commands might throw... we need to handle this
    let left_result = execute_no_throw(left, capture_stdout);
    match left_result {
        Ok(output) if output.code == 0 => Ok(output),
        _ => right.execute(capture_stdout),
    }
}

/// Execute a pipeline but don't throw on non-zero exit codes.
fn execute_no_throw(pipeline: &Pipeline, capture_stdout: bool) -> Result<CmdOutput> {
    match pipeline {
        Pipeline::Single(cmd) => {
            let mut cmd = cmd.as_ref().clone().no_throw();
            if capture_stdout {
                cmd = cmd.stdout_capture();
            }
            cmd.execute_inner()
        }
        _ => pipeline.execute(capture_stdout),
    }
}

fn execute_then(left: &Pipeline, right: &Pipeline, capture_stdout: bool) -> Result<CmdOutput> {
    let _ = execute_no_throw(left, false);
    right.execute(capture_stdout)
}
