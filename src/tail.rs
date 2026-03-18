use std::collections::VecDeque;
use std::io::BufRead;
use std::process::{Command, Stdio};

use crate::cmd::Cmd;
use crate::error::{CmdError, Result};

/// Options for [`Cmd::run_with_tail`].
#[derive(Debug, Clone)]
pub struct TailOptions {
    /// Message shown on the spinner while running.
    pub title: String,
    /// Message printed on success.
    pub done: String,
    /// Number of trailing stdout lines to display below the spinner.
    pub tail_lines: usize,
    /// Spinner tick characters.
    pub spinner_chars: String,
    /// Spinner tick interval in milliseconds.
    pub tick_ms: u64,
}

impl TailOptions {
    /// Create new tail options with the given title and done message.
    pub fn new(title: impl Into<String>, done: impl Into<String>) -> Self {
        TailOptions {
            title: title.into(),
            done: done.into(),
            tail_lines: 5,
            spinner_chars: "◒◐◓◑◒".to_string(),
            tick_ms: 80,
        }
    }

    /// Set the number of trailing lines to show (default: 5).
    pub fn lines(mut self, n: usize) -> Self {
        self.tail_lines = n;
        self
    }

    /// Set the spinner characters (default: "◒◐◓◑◒").
    pub fn spinner(mut self, chars: impl Into<String>) -> Self {
        self.spinner_chars = chars.into();
        self
    }

    /// Set the spinner tick interval in milliseconds (default: 80).
    pub fn tick_ms(mut self, ms: u64) -> Self {
        self.tick_ms = ms;
        self
    }
}

impl Cmd {
    /// Run the command with a spinner that streams the last N lines of stdout.
    ///
    /// Shows an animated spinner with the title while the command runs.
    /// Stdout lines are tailed below the spinner (dimmed). Stderr is captured
    /// in the background and included in the error on failure.
    ///
    /// On success, the spinner is cleared and the done message is printed.
    /// On failure, returns [`CmdError::ExitStatus`] with captured stderr.
    ///
    /// This method ignores any previously set stdout/stderr configuration
    /// (it needs to pipe both internally).
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use raxx::cmd;
    ///
    /// // Simple usage
    /// cmd!("cargo", "build")
    ///     .run_with_tail("Building...", "Build complete", 5)
    ///     .unwrap();
    ///
    /// // With TailOptions for more control
    /// use raxx::TailOptions;
    /// cmd!("cargo", "test")
    ///     .run_with_tail_opts(TailOptions::new("Testing...", "Tests passed").lines(10))
    ///     .unwrap();
    /// ```
    pub fn run_with_tail(self, title: &str, done: &str, tail_lines: usize) -> Result<()> {
        self.run_with_tail_opts(TailOptions {
            title: title.to_string(),
            done: done.to_string(),
            tail_lines,
            ..TailOptions::new("", "")
        })
    }

    /// Run the command with a spinner, using [`TailOptions`] for full control.
    pub fn run_with_tail_opts(self, opts: TailOptions) -> Result<()> {
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
            crate::cmd::StdinConfig::Inherit => {
                command.stdin(Stdio::inherit());
            }
            crate::cmd::StdinConfig::Null => {
                command.stdin(Stdio::null());
            }
            crate::cmd::StdinConfig::Bytes(_) | crate::cmd::StdinConfig::Path(_) => {
                command.stdin(Stdio::piped());
            }
        }

        // Always pipe stdout and stderr for tailing
        command.stdout(Stdio::piped());
        command.stderr(Stdio::piped());

        let mut child = match command.spawn() {
            Ok(child) => child,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                return Err(CmdError::NotFound {
                    program: self.program.clone(),
                });
            }
            Err(e) => return Err(CmdError::Io(e)),
        };

        // Write stdin if needed
        if let crate::cmd::StdinConfig::Bytes(ref data) = self.stdin {
            if let Some(mut stdin) = child.stdin.take() {
                use std::io::Write;
                let _ = stdin.write_all(data);
                drop(stdin);
            }
        } else if let crate::cmd::StdinConfig::Path(ref path) = self.stdin {
            if let Some(mut stdin) = child.stdin.take() {
                use std::io::Write;
                let data = std::fs::read(path)?;
                let _ = stdin.write_all(&data);
                drop(stdin);
            }
        }

        let stdout = child.stdout.take();
        let stderr = child.stderr.take();

        // Set up spinner
        let pb = indicatif::ProgressBar::new_spinner();
        pb.set_style(
            indicatif::ProgressStyle::with_template("  {spinner}  {msg}")
                .unwrap()
                .tick_chars(&opts.spinner_chars),
        );
        pb.enable_steady_tick(std::time::Duration::from_millis(opts.tick_ms));
        pb.set_message(opts.title.clone());

        let mut tail: VecDeque<String> = VecDeque::with_capacity(opts.tail_lines + 1);

        // Capture stderr in background thread
        let stderr_handle = std::thread::spawn(move || -> String {
            let mut buf = String::new();
            if let Some(se) = stderr {
                let reader = std::io::BufReader::new(se);
                for line in reader.lines() {
                    if let Ok(l) = line {
                        if !buf.is_empty() {
                            buf.push('\n');
                        }
                        buf.push_str(&l);
                    }
                }
            }
            buf
        });

        // Stream stdout lines and update spinner
        let title = opts.title.clone();
        let tail_lines = opts.tail_lines;
        if let Some(so) = stdout {
            let reader = std::io::BufReader::new(so);
            for line in reader.lines() {
                let line = match line {
                    Ok(l) => l,
                    Err(_) => break,
                };
                if tail.len() >= tail_lines {
                    tail.pop_front();
                }
                tail.push_back(line);

                let mut msg = title.clone();
                for tl in &tail {
                    let width = console::Term::stderr()
                        .size_checked()
                        .map(|(_, w)| w as usize)
                        .unwrap_or(80)
                        .saturating_sub(10);
                    let disp = if tl.len() > width {
                        format!("{}…", &tl[..width.saturating_sub(1)])
                    } else {
                        tl.clone()
                    };
                    msg.push_str(&format!("\n     \x1b[2m{disp}\x1b[0m"));
                }
                pb.set_message(msg);
            }
        }

        let all_stderr = stderr_handle.join().unwrap_or_default();
        let status = child.wait()?;

        pb.finish_and_clear();

        if status.success() {
            eprintln!("  \x1b[32m✓\x1b[0m  {}", opts.done);
            Ok(())
        } else {
            let code = status.code().unwrap_or(-1);
            eprintln!("  \x1b[31m✗\x1b[0m  {}: failed", title);
            let display = self.display_string();
            let stderr_msg = if all_stderr.trim().is_empty() {
                None
            } else {
                Some(format!("{}\n{}", display, all_stderr.trim()))
            };
            Err(CmdError::ExitStatus {
                code,
                stderr: stderr_msg,
            })
        }
    }

    /// Build a display-friendly representation of the command.
    fn display_string(&self) -> String {
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
}
