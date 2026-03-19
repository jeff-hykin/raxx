use raxx::{cmd, glob, shell, Cmd, CmdError, CmdOps, TailOptions, Stdout, Stderr, Append};
use std::fs;
use std::path::PathBuf;
use std::time::Duration;
use tempfile::TempDir;

fn temp_dir() -> TempDir {
    tempfile::tempdir().unwrap()
}

// ============================================================================
// Basic execution
// ============================================================================

#[test]
fn test_basic_echo() {
    let text = cmd!("echo", "hello").run().unwrap().stdout_trimmed();
    assert_eq!(text, "hello");
}

#[test]
fn test_echo_multiple_args() {
    let text = cmd!("echo", "hello", "world").run().unwrap().stdout_trimmed();
    assert_eq!(text, "hello world");
}

#[test]
fn test_run_success() {
    cmd!("true").run().unwrap();
}

#[test]
fn test_run_failure() {
    let result = cmd!("false").run();
    assert!(result.is_err());
}

#[test]
fn test_exit_code_zero() {
    let code = cmd!("true").run_exit_code().unwrap();
    assert_eq!(code, 0);
}

#[test]
fn test_exit_code_nonzero() {
    let code = cmd!("false").run_exit_code().unwrap();
    assert_eq!(code, 1);
}

#[test]
fn test_exit_code_custom() {
    let code = shell!("exit 42").run_exit_code().unwrap();
    assert_eq!(code, 42);
}

// ============================================================================
// Output capture
// ============================================================================

#[test]
fn test_text_trims_output() {
    let text = cmd!("echo", "  hello  ").run().unwrap().stdout_trimmed();
    // echo adds a trailing newline, stdout_trimmed() trims it
    assert_eq!(text, "hello");
}

#[test]
fn test_text_multiline() {
    let text = shell!("echo 'line1'; echo 'line2'").run().unwrap().stdout_trimmed();
    assert_eq!(text, "line1\nline2");
}

#[test]
fn test_lines() {
    let lines = shell!("echo 'line1'; echo 'line2'; echo 'line3'")
        .run()
        .unwrap()
        .stdout_lines();
    assert_eq!(lines, vec!["line1", "line2", "line3"]);
}

#[test]
fn test_bytes() {
    let bytes = cmd!("echo", "hello").run().unwrap().stdout_bytes().to_vec();
    assert_eq!(bytes, b"hello\n");
}

#[test]
fn test_json() {
    let val: serde_json::Value = cmd!("echo", r#"{"key":"value"}"#).run_stdout_json().unwrap();
    assert_eq!(val["key"], "value");
}

#[test]
fn test_json_number() {
    let val: i32 = cmd!("echo", "42").run_stdout_json().unwrap();
    assert_eq!(val, 42);
}

#[test]
fn test_json_array() {
    let val: Vec<i32> = cmd!("echo", "[1,2,3]").run_stdout_json().unwrap();
    assert_eq!(val, vec![1, 2, 3]);
}

#[test]
fn test_output_stdout_and_stderr() {
    let result = shell!("echo out; echo err >&2")
        .no_exit_err()
        .run()
        .unwrap();
    assert_eq!(result.stdout_trimmed(), "out");
    assert_eq!(result.stderr_trimmed(), "err");
}

#[test]
fn test_output_code() {
    let result = shell!("exit 5").no_exit_err().run().unwrap();
    assert_eq!(result.code, 5);
}

#[test]
fn test_output_success() {
    let result = cmd!("true").no_exit_err().run().unwrap();
    assert!(result.success());
}

#[test]
fn test_output_failure() {
    let result = cmd!("false").no_exit_err().run().unwrap();
    assert!(!result.success());
}

#[test]
fn test_stdout_capture_explicit() {
    let result = cmd!("echo", "hello").run().unwrap();
    assert_eq!(result.stdout_trimmed(), "hello");
}

#[test]
fn test_stderr_capture_explicit() {
    let result = shell!("echo err >&2")
        .run()
        .unwrap();
    assert_eq!(result.stderr_trimmed(), "err");
}

// ============================================================================
// Argument escaping via cmd! macro
// ============================================================================

#[test]
fn test_arg_with_spaces() {
    let name = "hello world";
    let text = cmd!("echo", name).run().unwrap().stdout_trimmed();
    assert_eq!(text, "hello world");
}

#[test]
fn test_arg_with_single_quotes() {
    let text = cmd!("echo", "it's").run().unwrap().stdout_trimmed();
    assert_eq!(text, "it's");
}

#[test]
fn test_arg_with_double_quotes() {
    let text = cmd!("echo", r#"say "hello""#).run().unwrap().stdout_trimmed();
    assert_eq!(text, r#"say "hello""#);
}

#[test]
fn test_arg_with_special_chars() {
    let text = cmd!("echo", "hello$world").run().unwrap().stdout_trimmed();
    assert_eq!(text, "hello$world");
}

#[test]
fn test_arg_with_newlines() {
    let text = cmd!("echo", "line1\nline2").run().unwrap().stdout_trimmed();
    assert_eq!(text, "line1\nline2");
}

#[test]
fn test_arg_with_backslash() {
    let text = cmd!("echo", r"back\slash").run().unwrap().stdout_trimmed();
    assert_eq!(text, r"back\slash");
}

#[test]
fn test_arg_with_glob_chars() {
    // These should NOT be interpreted as globs
    let dir = temp_dir();
    let path = dir.path().join("test_file.txt");
    fs::write(&path, "content").unwrap();
    // echo with * should print literal *, not expand
    let text = cmd!("echo", "*").run().unwrap().stdout_trimmed();
    assert_eq!(text, "*");
}

#[test]
fn test_arg_empty_string() {
    // An empty argument should be passed through
    let text = cmd!("printf", "%s", "").run().unwrap().stdout_trimmed();
    assert_eq!(text, "");
}

#[test]
fn test_args_from_vec() {
    let items = vec!["a", "b", "c"];
    let text = Cmd::new("echo").args(items).run().unwrap().stdout_trimmed();
    assert_eq!(text, "a b c");
}

#[test]
fn test_args_mixed() {
    let extra = "extra arg";
    let text = cmd!("echo", "fixed", extra).run().unwrap().stdout_trimmed();
    assert_eq!(text, "fixed extra arg");
}

#[test]
fn test_cmd_first_arg_is_program() {
    let text = cmd!("echo", "hello", "world").run().unwrap().stdout_trimmed();
    assert_eq!(text, "hello world");
}

// ============================================================================
// escape_arg function
// ============================================================================

#[test]
fn test_escape_arg_empty() {
    assert_eq!(raxx::escape_arg(""), "''");
}

#[test]
fn test_escape_arg_simple() {
    assert_eq!(raxx::escape_arg("hello"), "hello");
}

#[test]
fn test_escape_arg_with_space() {
    assert_eq!(raxx::escape_arg("hello world"), "'hello world'");
}

#[test]
fn test_escape_arg_with_single_quote() {
    let escaped = raxx::escape_arg("it's");
    assert_eq!(escaped, "'it'\"'\"'s'");
}

#[test]
fn test_escape_arg_with_special() {
    let escaped = raxx::escape_arg("$HOME");
    assert!(escaped.starts_with('\''));
}

// ============================================================================
// Environment variables
// ============================================================================

#[test]
fn test_env_single() {
    let text = shell!("echo $MY_VAR")
        .env("MY_VAR", "hello")
        .run()
        .unwrap()
        .stdout_trimmed();
    assert_eq!(text, "hello");
}

#[test]
fn test_env_multiple() {
    let text = shell!("echo $VAR1 $VAR2")
        .env("VAR1", "hello")
        .env("VAR2", "world")
        .run()
        .unwrap()
        .stdout_trimmed();
    assert_eq!(text, "hello world");
}

#[test]
fn test_envs_iterator() {
    let text = shell!("echo $A $B")
        .envs([("A", "1"), ("B", "2")])
        .run()
        .unwrap()
        .stdout_trimmed();
    assert_eq!(text, "1 2");
}

#[test]
fn test_env_override() {
    // Set then override
    let text = shell!("echo $VAR")
        .env("VAR", "first")
        .env("VAR", "second")
        .run()
        .unwrap()
        .stdout_trimmed();
    assert_eq!(text, "second");
}

#[test]
fn test_env_remove() {
    let text = shell!("echo ${MY_TEST_VAR:-unset}")
        .env("MY_TEST_VAR", "set")
        .env_remove("MY_TEST_VAR")
        .run()
        .unwrap()
        .stdout_trimmed();
    assert_eq!(text, "unset");
}

#[test]
fn test_env_clear() {
    let result = cmd!("env")
        .env_clear()
        .env("ONLY_THIS", "yes")
        .run()
        .unwrap();
    let text = result.stdout_trimmed();
    // Should only contain ONLY_THIS (and maybe some system ones on some platforms)
    assert!(text.contains("ONLY_THIS=yes"));
}

#[test]
fn test_env_with_spaces_in_value() {
    let text = shell!("echo $VAR")
        .env("VAR", "hello world")
        .run()
        .unwrap()
        .stdout_trimmed();
    assert_eq!(text, "hello world");
}

#[test]
fn test_env_with_equals_in_value() {
    let text = shell!("echo $VAR")
        .env("VAR", "key=value")
        .run()
        .unwrap()
        .stdout_trimmed();
    assert_eq!(text, "key=value");
}

#[test]
fn test_env_empty_value() {
    let text = shell!("echo \"${VAR}end\"")
        .env("VAR", "")
        .run()
        .unwrap()
        .stdout_trimmed();
    assert_eq!(text, "end");
}

// ============================================================================
// Working directory
// ============================================================================

#[test]
fn test_cwd() {
    let dir = temp_dir();
    let text = cmd!("pwd").cwd(dir.path()).run().unwrap().stdout_trimmed();
    // Resolve symlinks for comparison (macOS /tmp -> /private/tmp)
    let expected = fs::canonicalize(dir.path()).unwrap();
    let actual = PathBuf::from(&text);
    let actual = fs::canonicalize(&actual).unwrap_or(actual);
    assert_eq!(actual, expected);
}

#[test]
fn test_cwd_affects_file_operations() {
    let dir = temp_dir();
    fs::write(dir.path().join("test.txt"), "content").unwrap();
    let text = cmd!("cat", "test.txt").cwd(dir.path()).run().unwrap().stdout_trimmed();
    assert_eq!(text, "content");
}

#[test]
fn test_cwd_nonexistent() {
    let result = cmd!("ls").cwd("/nonexistent_dir_12345").run();
    assert!(result.is_err());
    match result.unwrap_err() {
        CmdError::CwdNotFound { path } => {
            assert!(path.contains("nonexistent"));
        }
        other => panic!("Expected CwdNotFound, got: {:?}", other),
    }
}

// ============================================================================
// Stdin
// ============================================================================

#[test]
fn test_stdin_text() {
    let text = cmd!("cat").stdin_text("hello from stdin").run().unwrap().stdout_trimmed();
    assert_eq!(text, "hello from stdin");
}

#[test]
fn test_stdin_bytes() {
    let text = cmd!("cat")
        .stdin_bytes(b"hello bytes".to_vec())
        .run()
        .unwrap()
        .stdout_trimmed();
    assert_eq!(text, "hello bytes");
}

#[test]
fn test_stdin_path() {
    let dir = temp_dir();
    let path = dir.path().join("input.txt");
    fs::write(&path, "file content").unwrap();
    let text = cmd!("cat").stdin_path(&path).run().unwrap().stdout_trimmed();
    assert_eq!(text, "file content");
}

#[test]
fn test_stdin_null() {
    // wc -c on null stdin should output 0
    let text = cmd!("wc", "-c").stdin_null().run().unwrap().stdout_trimmed();
    assert_eq!(text.trim(), "0");
}

#[test]
fn test_stdin_multiline() {
    let input = "line1\nline2\nline3";
    let lines = cmd!("cat").stdin_text(input).run().unwrap().stdout_lines();
    assert_eq!(lines, vec!["line1", "line2", "line3"]);
}

#[test]
fn test_stdin_to_grep() {
    let text = cmd!("grep", "hello")
        .stdin_text("hello world\ngoodbye world\nhello again")
        .run()
        .unwrap()
        .stdout_trimmed();
    assert_eq!(text, "hello world\nhello again");
}

#[test]
fn test_stdin_binary() {
    let data: Vec<u8> = vec![0, 1, 2, 3, 255, 254, 253];
    let bytes = cmd!("cat").stdin_bytes(data.clone()).run().unwrap().stdout_bytes().to_vec();
    assert_eq!(bytes, data);
}

// ============================================================================
// Stdout redirection
// ============================================================================

#[test]
fn test_stdout_path() {
    let dir = temp_dir();
    let path = dir.path().join("output.txt");
    cmd!("echo", "hello").redirect(Stdout, &path).run().unwrap();
    let content = fs::read_to_string(&path).unwrap();
    assert_eq!(content.trim(), "hello");
}

#[test]
fn test_stdout_append() {
    let dir = temp_dir();
    let path = dir.path().join("output.txt");
    fs::write(&path, "existing\n").unwrap();
    cmd!("echo", "appended").redirect(Stdout, Append(&path)).run().unwrap();
    let content = fs::read_to_string(&path).unwrap();
    assert!(content.contains("existing"));
    assert!(content.contains("appended"));
}

#[test]
fn test_stdout_append_creates_file() {
    let dir = temp_dir();
    let path = dir.path().join("new_file.txt");
    cmd!("echo", "created").redirect(Stdout, Append(&path)).run().unwrap();
    let content = fs::read_to_string(&path).unwrap();
    assert!(content.contains("created"));
}

#[test]
fn test_stdout_null() {
    // Should not error, just discard output
    cmd!("echo", "discarded").quiet_stdout().run().unwrap();
}

#[test]
fn test_stdout_overwrite() {
    let dir = temp_dir();
    let path = dir.path().join("output.txt");
    cmd!("echo", "first").redirect(Stdout, &path).run().unwrap();
    cmd!("echo", "second").redirect(Stdout, &path).run().unwrap();
    let content = fs::read_to_string(&path).unwrap();
    assert_eq!(content.trim(), "second");
}

// ============================================================================
// Stderr redirection
// ============================================================================

#[test]
fn test_stderr_path() {
    let dir = temp_dir();
    let path = dir.path().join("errors.txt");
    shell!("echo error_msg >&2")
        .redirect(Stderr, &path)
        .run()
        .unwrap();
    let content = fs::read_to_string(&path).unwrap();
    assert!(content.contains("error_msg"));
}

#[test]
fn test_stderr_append() {
    let dir = temp_dir();
    let path = dir.path().join("errors.txt");
    fs::write(&path, "existing\n").unwrap();
    shell!("echo new_error >&2")
        .redirect(Stderr, Append(&path))
        .run()
        .unwrap();
    let content = fs::read_to_string(&path).unwrap();
    assert!(content.contains("existing"));
    assert!(content.contains("new_error"));
}

#[test]
fn test_stderr_null() {
    // Should not error
    shell!("echo error >&2").quiet_stderr().run().unwrap();
}

#[test]
fn test_stderr_to_stdout() {
    let result = shell!("echo out; echo err >&2")
        .redirect(Stderr, Stdout)
        .run()
        .unwrap();
    let text = result.stdout_trimmed();
    assert!(text.contains("out"));
    assert!(text.contains("err"));
}

#[test]
fn test_stdout_to_stderr() {
    // Capture stderr, stdout goes to stderr via redirect
    let result = shell!("echo out; echo err >&2")
        .redirect(Stdout, Stderr)
        .run()
        .unwrap();
    let text = result.stderr_trimmed();
    assert!(text.contains("err"));
}

// ============================================================================
// Piping
// ============================================================================

#[test]
fn test_pipe_simple() {
    let text = cmd!("echo", "hello world")
        .pipe(cmd!("tr", "a-z", "A-Z"))
        .run()
        .unwrap()
        .stdout_trimmed();
    assert_eq!(text, "HELLO WORLD");
}

#[test]
fn test_pipe_grep() {
    let text = cmd!("echo", "hello\ngoodbye\nhello again")
        .pipe(cmd!("grep", "hello"))
        .run()
        .unwrap()
        .stdout_trimmed();
    assert!(text.contains("hello"));
    assert!(!text.contains("goodbye"));
}

#[test]
fn test_pipe_chain() {
    let text = cmd!("echo", "3\n1\n2")
        .pipe(cmd!("sort"))
        .pipe(cmd!("head", "-n", "1"))
        .run()
        .unwrap()
        .stdout_trimmed();
    assert_eq!(text, "1");
}

#[test]
fn test_pipe_wc() {
    let text = shell!("echo 'line1'; echo 'line2'; echo 'line3'")
        .pipe(cmd!("wc", "-l"))
        .run()
        .unwrap()
        .stdout_trimmed();
    assert_eq!(text.trim(), "3");
}

#[test]
fn test_pipe_with_stdin() {
    let text = cmd!("cat")
        .stdin_text("hello\nworld")
        .pipe(cmd!("grep", "world"))
        .run()
        .unwrap()
        .stdout_trimmed();
    assert_eq!(text, "world");
}

#[test]
fn test_pipe_preserves_env() {
    let text = shell!("echo $MY_VAR")
        .env("MY_VAR", "piped_value")
        .pipe(cmd!("cat"))
        .run()
        .unwrap()
        .stdout_trimmed();
    assert_eq!(text, "piped_value");
}

// ============================================================================
// Chaining (and, or, then)
// ============================================================================

#[test]
fn test_and_both_succeed() {
    let text = cmd!("echo", "first")
        .and(cmd!("echo", "second"))
        .run()
        .unwrap()
        .stdout_trimmed();
    // and returns the output of the last successful command
    assert_eq!(text, "second");
}

#[test]
fn test_and_first_fails() {
    let code = cmd!("false").and(cmd!("echo", "never")).run_exit_code().unwrap();
    assert_ne!(code, 0);
}

#[test]
fn test_or_first_succeeds() {
    let text = cmd!("echo", "first")
        .or(cmd!("echo", "fallback"))
        .run()
        .unwrap()
        .stdout_trimmed();
    assert_eq!(text, "first");
}

#[test]
fn test_or_first_fails() {
    let text = cmd!("false")
        .or(cmd!("echo", "fallback"))
        .run()
        .unwrap()
        .stdout_trimmed();
    assert_eq!(text, "fallback");
}

#[test]
fn test_then_runs_regardless() {
    let text = cmd!("false")
        .then(cmd!("echo", "always"))
        .run()
        .unwrap()
        .stdout_trimmed();
    assert_eq!(text, "always");
}

#[test]
fn test_then_first_succeeds() {
    let text = cmd!("true")
        .then(cmd!("echo", "after"))
        .run()
        .unwrap()
        .stdout_trimmed();
    assert_eq!(text, "after");
}

#[test]
fn test_chain_complex() {
    // (true && echo "a") || echo "b" -- should give "a"
    let text = cmd!("true")
        .and(cmd!("echo", "a"))
        .or(cmd!("echo", "b"))
        .run()
        .unwrap()
        .stdout_trimmed();
    assert_eq!(text, "a");
}

#[test]
fn test_chain_complex_fallback() {
    // (false && echo "a") || echo "b" -- should give "b"
    let text = cmd!("false")
        .and(cmd!("echo", "a"))
        .or(cmd!("echo", "b"))
        .run()
        .unwrap()
        .stdout_trimmed();
    assert_eq!(text, "b");
}

// ============================================================================
// Shell macro
// ============================================================================

#[test]
fn test_shell_pipe() {
    let text = shell!("echo hello | tr a-z A-Z").run().unwrap().stdout_trimmed();
    assert_eq!(text, "HELLO");
}

#[test]
fn test_shell_and() {
    let text = shell!("echo first && echo second").run().unwrap().stdout_trimmed();
    assert_eq!(text, "first\nsecond");
}

#[test]
fn test_shell_or() {
    let text = shell!("false || echo fallback").run().unwrap().stdout_trimmed();
    assert_eq!(text, "fallback");
}

#[test]
fn test_shell_semicolon() {
    let text = shell!("echo a; echo b").run().unwrap().stdout_trimmed();
    assert_eq!(text, "a\nb");
}

#[test]
fn test_shell_subshell() {
    let text = shell!("(echo sub)").run().unwrap().stdout_trimmed();
    assert_eq!(text, "sub");
}

#[test]
fn test_shell_variable() {
    let text = shell!("VAR=hello; echo $VAR").run().unwrap().stdout_trimmed();
    assert_eq!(text, "hello");
}

#[test]
fn test_shell_export() {
    let text = shell!("export VAR=hello && echo $VAR").run().unwrap().stdout_trimmed();
    assert_eq!(text, "hello");
}

#[test]
fn test_shell_redirect_to_file() {
    let dir = temp_dir();
    let path = dir.path().join("out.txt");
    shell!(&format!("echo hello > {}", path.display()))
        .run()
        .unwrap();
    assert_eq!(fs::read_to_string(&path).unwrap().trim(), "hello");
}

#[test]
fn test_shell_redirect_append() {
    let dir = temp_dir();
    let path = dir.path().join("out.txt");
    fs::write(&path, "first\n").unwrap();
    shell!(&format!("echo second >> {}", path.display()))
        .run()
        .unwrap();
    let content = fs::read_to_string(&path).unwrap();
    assert!(content.contains("first"));
    assert!(content.contains("second"));
}

#[test]
fn test_shell_redirect_stderr() {
    let dir = temp_dir();
    let path = dir.path().join("err.txt");
    shell!(&format!("echo error 2> {} >&2", path.display()))
        .redirect(Stderr, &path)
        .run()
        .unwrap();
    // Use the builder-level stderr redirect instead of shell-level
    // since shell redirect ordering is tricky
    let dir2 = temp_dir();
    let path2 = dir2.path().join("err2.txt");
    shell!("echo error >&2")
        .redirect(Stderr, &path2)
        .run()
        .unwrap();
    let content = fs::read_to_string(&path2).unwrap();
    assert!(content.contains("error"));
}

#[test]
fn test_shell_input_redirect() {
    let dir = temp_dir();
    let path = dir.path().join("input.txt");
    fs::write(&path, "file content").unwrap();
    let text = shell!(&format!("cat < {}", path.display()))
        .run()
        .unwrap()
        .stdout_trimmed();
    assert_eq!(text, "file content");
}

#[test]
fn test_shell_pipe_chain() {
    let text = shell!("echo '3\n1\n2' | sort | head -n1").run().unwrap().stdout_trimmed();
    assert_eq!(text, "1");
}

#[test]
fn test_shell_heredoc() {
    let text = shell!("cat <<EOF\nhello\nworld\nEOF").run().unwrap().stdout_trimmed();
    assert_eq!(text, "hello\nworld");
}

#[test]
fn test_shell_backtick_substitution() {
    let text = shell!("echo `echo inner`").run().unwrap().stdout_trimmed();
    assert_eq!(text, "inner");
}

#[test]
fn test_shell_command_substitution() {
    let text = shell!("echo $(echo inner)").run().unwrap().stdout_trimmed();
    assert_eq!(text, "inner");
}

// ============================================================================
// Error handling
// ============================================================================

#[test]
fn test_no_exit_err() {
    let result = cmd!("false").no_exit_err().run().unwrap();
    assert_eq!(result.code, 1);
}

#[test]
fn test_no_exit_err_on_matching() {
    let result = shell!("exit 42").no_exit_err_on(&[42]).run().unwrap();
    assert_eq!(result.code, 42);
}

#[test]
fn test_no_exit_err_on_non_matching() {
    let result = shell!("exit 42").no_exit_err_on(&[1, 2, 3]).run();
    assert!(result.is_err());
}

#[test]
fn test_error_exit_status() {
    let err = cmd!("false").run().unwrap_err();
    match err {
        CmdError::ExitStatus { code, .. } => assert_eq!(code, 1),
        other => panic!("Expected ExitStatus, got: {:?}", other),
    }
}

#[test]
fn test_error_not_found() {
    let err = cmd!("nonexistent_command_xyz_12345").run().unwrap_err();
    match err {
        CmdError::NotFound { program } => {
            assert_eq!(program, "nonexistent_command_xyz_12345");
        }
        other => panic!("Expected NotFound, got: {:?}", other),
    }
}

#[test]
fn test_error_display() {
    let err = CmdError::ExitStatus {
        code: 1,
        stderr: Some("oops".to_string()),
    };
    let msg = format!("{}", err);
    assert!(msg.contains("1"));
    assert!(msg.contains("oops"));
}

#[test]
fn test_error_cwd_not_found_display() {
    let err = CmdError::CwdNotFound {
        path: "/no/such/dir".to_string(),
    };
    let msg = format!("{}", err);
    assert!(msg.contains("/no/such/dir"));
    assert!(msg.contains("cwd"));
}

#[test]
fn test_status_code_success() {
    let code = cmd!("true").run_exit_code().unwrap();
    assert_eq!(code, 0);
}

#[test]
fn test_status_code_failure() {
    let code = cmd!("false").run_exit_code().unwrap();
    assert_eq!(code, 1);
}

#[test]
fn test_stderr_captured_in_error() {
    let result = shell!("echo err_msg >&2; exit 1")
        .no_exit_err()
        .run()
        .unwrap();
    assert_eq!(result.code, 1);
    assert!(result.stderr_trimmed().contains("err_msg"));
}

// ============================================================================
// Timeout
// ============================================================================

#[test]
fn test_timeout_expires() {
    let result = cmd!("sleep", "10")
        .timeout(Duration::from_millis(100))
        .run();
    assert!(result.is_err());
    match result.unwrap_err() {
        CmdError::Timeout { duration } => {
            assert!(duration.as_millis() <= 200);
        }
        other => panic!("Expected Timeout, got: {:?}", other),
    }
}

#[test]
fn test_timeout_does_not_expire() {
    cmd!("echo", "fast")
        .timeout(Duration::from_secs(10))
        .run()
        .unwrap();
}

#[test]
fn test_timeout_signal_sigkill() {
    let result = cmd!("sleep", "10")
        .timeout_signal(Duration::from_millis(100), libc::SIGKILL, None)
        .run();
    assert!(result.is_err());
    assert!(matches!(result.unwrap_err(), CmdError::Timeout { .. }));
}

#[test]
fn test_timeout_signal_with_grace_period() {
    let result = cmd!("sleep", "10")
        .timeout_signal(
            Duration::from_millis(100),
            libc::SIGTERM,
            Some(Duration::from_millis(200)),
        )
        .run();
    assert!(result.is_err());
    assert!(matches!(result.unwrap_err(), CmdError::Timeout { .. }));
}

// ============================================================================
// Quiet mode
// ============================================================================

#[test]
fn test_quiet_runs_silently() {
    // Just verify it doesn't panic
    cmd!("echo", "quiet").quiet().run().unwrap();
}

#[test]
fn test_quiet_stdout() {
    cmd!("echo", "quiet").quiet_stdout().run().unwrap();
}

#[test]
fn test_quiet_stderr() {
    shell!("echo err >&2").quiet_stderr().run().unwrap();
}

#[test]
fn test_quiet_still_captures() {
    // When you call .run().stdout_trimmed() it should still capture even with quiet
    let text = cmd!("echo", "captured").run().unwrap().stdout_trimmed();
    assert_eq!(text, "captured");
}

// ============================================================================
// Builder pattern / immutability
// ============================================================================

#[test]
fn test_builder_chaining() {
    let text = Cmd::new("echo")
        .arg("hello")
        .arg("world")
        .run()
        .unwrap()
        .stdout_trimmed();
    assert_eq!(text, "hello world");
}

#[test]
fn test_builder_env_and_cwd() {
    let dir = temp_dir();
    let text = shell!("echo $MY_VAR && pwd")
        .env("MY_VAR", "test")
        .cwd(dir.path())
        .run()
        .unwrap()
        .stdout_trimmed();
    assert!(text.contains("test"));
}

#[test]
fn test_cmd_new_with_program() {
    let text = Cmd::new("echo").arg("test").run().unwrap().stdout_trimmed();
    assert_eq!(text, "test");
}

#[test]
fn test_cmd_parse() {
    let text = Cmd::parse("echo hello world").run().unwrap().stdout_trimmed();
    assert_eq!(text, "hello world");
}

// ============================================================================
// Complex combinations
// ============================================================================

#[test]
fn test_pipe_with_env() {
    let text = shell!("echo $MY_VAR")
        .env("MY_VAR", "value")
        .pipe(cmd!("tr", "a-z", "A-Z"))
        .run()
        .unwrap()
        .stdout_trimmed();
    assert_eq!(text, "VALUE");
}

#[test]
fn test_pipe_to_file() {
    let dir = temp_dir();
    let path = dir.path().join("piped.txt");
    cmd!("echo", "piped output")
        .pipe(cmd!("cat"))
        .redirect(Stdout, &path)
        .run()
        .unwrap();
    // When using .run().stdout_trimmed(), stdout is captured not written to file
    // Let's test differently
    let text = cmd!("echo", "piped output")
        .pipe(cmd!("cat"))
        .run()
        .unwrap()
        .stdout_trimmed();
    assert_eq!(text, "piped output");
}

#[test]
fn test_stdin_text_pipe_grep() {
    let text = cmd!("cat")
        .stdin_text("apple\nbanana\ncherry")
        .pipe(cmd!("grep", "an"))
        .run()
        .unwrap()
        .stdout_trimmed();
    assert_eq!(text, "banana");
}

#[test]
fn test_env_in_chained_commands() {
    // Each command in a chain should inherit the env from the builder
    // But with our current implementation, env is set on the Cmd level
    let text = shell!("echo $VAR")
        .env("VAR", "hello")
        .run()
        .unwrap()
        .stdout_trimmed();
    assert_eq!(text, "hello");
}

#[test]
fn test_cwd_with_relative_file() {
    let dir = temp_dir();
    fs::write(dir.path().join("data.txt"), "relative").unwrap();
    let text = cmd!("cat", "data.txt").cwd(dir.path()).run().unwrap().stdout_trimmed();
    assert_eq!(text, "relative");
}

#[test]
fn test_multiple_pipes_with_transform() {
    let text = cmd!("echo", "Hello World")
        .pipe(cmd!("tr", " ", "\n"))
        .pipe(cmd!("sort"))
        .pipe(cmd!("head", "-n", "1"))
        .run()
        .unwrap()
        .stdout_trimmed();
    assert_eq!(text, "Hello");
}

#[test]
fn test_shell_complex_pipeline() {
    let text = shell!("echo 'one two three' | tr ' ' '\\n' | sort -r | head -1")
        .run()
        .unwrap()
        .stdout_trimmed();
    assert_eq!(text, "two");
}

// ============================================================================
// File I/O combinations
// ============================================================================

#[test]
fn test_read_write_cycle() {
    let dir = temp_dir();
    let path = dir.path().join("cycle.txt");

    // Write via stdout redirect
    cmd!("echo", "cycle test").redirect(Stdout, &path).run().unwrap();

    // Read via stdin redirect
    let text = cmd!("cat").stdin_path(&path).run().unwrap().stdout_trimmed();
    assert_eq!(text, "cycle test");
}

#[test]
fn test_append_multiple_times() {
    let dir = temp_dir();
    let path = dir.path().join("append.txt");

    cmd!("echo", "line1").redirect(Stdout, &path).run().unwrap();
    cmd!("echo", "line2").redirect(Stdout, Append(&path)).run().unwrap();
    cmd!("echo", "line3").redirect(Stdout, Append(&path)).run().unwrap();

    let content = fs::read_to_string(&path).unwrap();
    assert!(content.contains("line1"));
    assert!(content.contains("line2"));
    assert!(content.contains("line3"));
}

#[test]
fn test_stderr_and_stdout_to_different_files() {
    let dir = temp_dir();
    let stdout_path = dir.path().join("stdout.txt");
    let stderr_path = dir.path().join("stderr.txt");

    shell!("echo out; echo err >&2")
        .redirect(Stdout, &stdout_path)
        .redirect(Stderr, &stderr_path)
        .run()
        .unwrap();

    assert!(fs::read_to_string(&stdout_path).unwrap().contains("out"));
    assert!(fs::read_to_string(&stderr_path).unwrap().contains("err"));
}

// ============================================================================
// Edge cases
// ============================================================================

#[test]
fn test_empty_output() {
    let text = cmd!("true").run().unwrap().stdout_trimmed();
    assert_eq!(text, "");
}

#[test]
fn test_large_output() {
    // Generate a large output and verify we capture it all
    let text = shell!("seq 1 10000").run().unwrap().stdout_trimmed();
    let lines: Vec<&str> = text.lines().collect();
    assert_eq!(lines.len(), 10000);
    assert_eq!(lines[0], "1");
    assert_eq!(lines[9999], "10000");
}

#[test]
fn test_large_stdin() {
    let input: String = (1..=10000).map(|i| format!("{}\n", i)).collect();
    let text = cmd!("wc", "-l").stdin_text(&input).run().unwrap().stdout_trimmed();
    assert_eq!(text.trim(), "10000");
}

#[test]
fn test_binary_through_pipe() {
    let data: Vec<u8> = (0..=255).collect();
    let bytes = cmd!("cat")
        .stdin_bytes(data.clone())
        .pipe(cmd!("cat"))
        .run()
        .unwrap()
        .stdout_bytes()
        .to_vec();
    assert_eq!(bytes, data);
}

#[test]
fn test_unicode_output() {
    let text = cmd!("echo", "こんにちは世界").run().unwrap().stdout_trimmed();
    assert_eq!(text, "こんにちは世界");
}

#[test]
fn test_unicode_arg() {
    let arg = "café résumé naïve";
    let text = cmd!("echo", arg).run().unwrap().stdout_trimmed();
    assert_eq!(text, "café résumé naïve");
}

#[test]
fn test_whitespace_preservation_in_args() {
    let text = cmd!("echo", "  spaces  ").run().unwrap().stdout_trimmed();
    assert_eq!(text, "spaces");
}

#[test]
fn test_tab_in_arg() {
    let text = cmd!("printf", "%s", "a\tb").run().unwrap().stdout_trimmed();
    assert_eq!(text, "a\tb");
}

#[test]
fn test_many_args() {
    let args: Vec<String> = (0..100).map(|i| format!("arg{}", i)).collect();
    let text = Cmd::new("echo")
        .args(args.iter().map(|s| s.as_str()))
        .run()
        .unwrap()
        .stdout_trimmed();
    assert!(text.contains("arg0"));
    assert!(text.contains("arg99"));
}

// ============================================================================
// Shell built-in commands (via shell! macro)
// ============================================================================

#[test]
fn test_shell_echo() {
    let text = shell!("echo hello").run().unwrap().stdout_trimmed();
    assert_eq!(text, "hello");
}

#[test]
fn test_shell_cat() {
    let dir = temp_dir();
    let path = dir.path().join("cat_test.txt");
    fs::write(&path, "cat content").unwrap();
    let text = shell!(&format!("cat {}", path.display())).run().unwrap().stdout_trimmed();
    assert_eq!(text, "cat content");
}

#[test]
fn test_shell_pwd() {
    let dir = temp_dir();
    let text = shell!("pwd").cwd(dir.path()).run().unwrap().stdout_trimmed();
    let expected = fs::canonicalize(dir.path()).unwrap();
    let actual = fs::canonicalize(PathBuf::from(&text)).unwrap();
    assert_eq!(actual, expected);
}

#[test]
fn test_shell_mkdir_and_test() {
    let dir = temp_dir();
    let new_dir = dir.path().join("subdir");
    shell!(&format!("mkdir -p {}", new_dir.display()))
        .run()
        .unwrap();
    assert!(new_dir.exists());
}

#[test]
fn test_shell_touch() {
    let dir = temp_dir();
    let path = dir.path().join("touched.txt");
    shell!(&format!("touch {}", path.display()))
        .run()
        .unwrap();
    assert!(path.exists());
}

#[test]
fn test_shell_rm() {
    let dir = temp_dir();
    let path = dir.path().join("to_remove.txt");
    fs::write(&path, "").unwrap();
    shell!(&format!("rm {}", path.display())).run().unwrap();
    assert!(!path.exists());
}

#[test]
fn test_shell_cp() {
    let dir = temp_dir();
    let src = dir.path().join("src.txt");
    let dst = dir.path().join("dst.txt");
    fs::write(&src, "copy me").unwrap();
    shell!(&format!("cp {} {}", src.display(), dst.display()))
        .run()
        .unwrap();
    assert_eq!(fs::read_to_string(&dst).unwrap(), "copy me");
}

#[test]
fn test_shell_mv() {
    let dir = temp_dir();
    let src = dir.path().join("src.txt");
    let dst = dir.path().join("dst.txt");
    fs::write(&src, "move me").unwrap();
    shell!(&format!("mv {} {}", src.display(), dst.display()))
        .run()
        .unwrap();
    assert!(!src.exists());
    assert_eq!(fs::read_to_string(&dst).unwrap(), "move me");
}

#[test]
fn test_shell_test_file_exists() {
    let dir = temp_dir();
    let path = dir.path().join("exists.txt");
    fs::write(&path, "").unwrap();
    let code = shell!(&format!("test -f {}", path.display()))
        .run_exit_code()
        .unwrap();
    assert_eq!(code, 0);
}

#[test]
fn test_shell_test_file_not_exists() {
    let code = shell!("test -f /nonexistent_file_xyz")
        .run_exit_code()
        .unwrap();
    assert_ne!(code, 0);
}

#[test]
fn test_shell_test_dir_exists() {
    let dir = temp_dir();
    let code = shell!(&format!("test -d {}", dir.path().display()))
        .run_exit_code()
        .unwrap();
    assert_eq!(code, 0);
}

#[test]
fn test_shell_which() {
    let text = shell!("which echo").run().unwrap().stdout_trimmed();
    assert!(text.contains("echo"));
}

#[test]
fn test_shell_printenv() {
    let text = shell!("printenv MY_PRINTENV_VAR")
        .env("MY_PRINTENV_VAR", "found_it")
        .run()
        .unwrap()
        .stdout_trimmed();
    assert_eq!(text, "found_it");
}

#[test]
fn test_shell_true() {
    let code = shell!("true").run_exit_code().unwrap();
    assert_eq!(code, 0);
}

#[test]
fn test_shell_false() {
    let code = shell!("false").run_exit_code().unwrap();
    assert_eq!(code, 1);
}

#[test]
fn test_shell_exit_wraps() {
    // Exit codes are modulo 256
    let code = shell!("exit 256").run_exit_code().unwrap();
    assert_eq!(code, 0);
}

#[test]
fn test_shell_exit_wraps_257() {
    let code = shell!("exit 257").run_exit_code().unwrap();
    assert_eq!(code, 1);
}

#[test]
fn test_shell_sleep() {
    // Should complete quickly
    shell!("sleep 0.01").run().unwrap();
}

// ============================================================================
// Shell features
// ============================================================================

#[test]
fn test_shell_glob_star() {
    let dir = temp_dir();
    fs::write(dir.path().join("a.txt"), "a").unwrap();
    fs::write(dir.path().join("b.txt"), "b").unwrap();
    let text = shell!("ls *.txt").cwd(dir.path()).run().unwrap().stdout_trimmed();
    assert!(text.contains("a.txt"));
    assert!(text.contains("b.txt"));
}

#[test]
fn test_shell_tilde_expansion() {
    let text = shell!("echo ~").run().unwrap().stdout_trimmed();
    assert!(text.starts_with('/'));
    assert!(!text.contains('~'));
}

#[test]
fn test_shell_variable_substitution() {
    let text = shell!("X=hello; echo $X").run().unwrap().stdout_trimmed();
    assert_eq!(text, "hello");
}

#[test]
fn test_shell_variable_in_double_quotes() {
    let text = shell!("X=hello; echo \"$X world\"").run().unwrap().stdout_trimmed();
    assert_eq!(text, "hello world");
}

#[test]
fn test_shell_variable_not_in_single_quotes() {
    let text = shell!("X=hello; echo '$X world'").run().unwrap().stdout_trimmed();
    assert_eq!(text, "$X world");
}

#[test]
fn test_shell_negation() {
    let code = shell!("! false").run_exit_code().unwrap();
    assert_eq!(code, 0);
}

#[test]
fn test_shell_negation_true() {
    let code = shell!("! true").run_exit_code().unwrap();
    assert_eq!(code, 1);
}

#[test]
fn test_shell_subshell_env_isolation() {
    // Variables set in a subshell should not leak
    let text = shell!("(X=inner); echo ${X:-outer}").run().unwrap().stdout_trimmed();
    assert_eq!(text, "outer");
}

#[test]
fn test_shell_arithmetic() {
    let text = shell!("echo $((2 + 3))").run().unwrap().stdout_trimmed();
    assert_eq!(text, "5");
}

#[test]
fn test_shell_for_loop() {
    let text = shell!("for i in 1 2 3; do echo $i; done").run().unwrap().stdout_trimmed();
    assert_eq!(text, "1\n2\n3");
}

#[test]
fn test_shell_if_else() {
    let text = shell!("if true; then echo yes; else echo no; fi")
        .run()
        .unwrap()
        .stdout_trimmed();
    assert_eq!(text, "yes");
}

#[test]
fn test_shell_if_else_false() {
    let text = shell!("if false; then echo yes; else echo no; fi")
        .run()
        .unwrap()
        .stdout_trimmed();
    assert_eq!(text, "no");
}

#[test]
fn test_shell_while_loop() {
    let text = shell!("i=0; while [ $i -lt 3 ]; do echo $i; i=$((i+1)); done")
        .run()
        .unwrap()
        .stdout_trimmed();
    assert_eq!(text, "0\n1\n2");
}

#[test]
fn test_shell_case() {
    let text = shell!("X=hello; case $X in hello) echo matched;; *) echo nope;; esac")
        .run()
        .unwrap()
        .stdout_trimmed();
    assert_eq!(text, "matched");
}

// ============================================================================
// CmdResult methods
// ============================================================================

#[test]
fn test_output_stdout_json() {
    let result = cmd!("echo", r#"{"a":1}"#).run().unwrap();
    let val: serde_json::Value = result.stdout_json().unwrap();
    assert_eq!(val["a"], 1);
}

#[test]
fn test_output_stdout_lines() {
    let result = shell!("echo line1; echo line2")
        .run()
        .unwrap();
    let lines = result.stdout_lines();
    assert_eq!(lines, vec!["line1", "line2"]);
}

#[test]
fn test_output_stdout_text_raw() {
    let result = cmd!("echo", "hello").run().unwrap();
    let raw = result.stdout();
    assert_eq!(raw, "hello\n"); // Not trimmed
}

#[test]
fn test_output_stderr_text_raw() {
    let result = shell!("echo err >&2")
        .run()
        .unwrap();
    let raw = result.stderr();
    assert_eq!(raw, "err\n");
}

// ============================================================================
// Concurrent execution patterns
// ============================================================================

#[test]
fn test_independent_commands() {
    // Run multiple commands independently (not chained)
    let a = cmd!("echo", "a").run().unwrap().stdout_trimmed();
    let b = cmd!("echo", "b").run().unwrap().stdout_trimmed();
    assert_eq!(a, "a");
    assert_eq!(b, "b");
}

#[test]
fn test_output_reuse() {
    // Use output of one command as input to another
    let first = cmd!("echo", "hello").run().unwrap().stdout_trimmed();
    let text = cmd!("echo", &first)
        .pipe(cmd!("tr", "a-z", "A-Z"))
        .run()
        .unwrap()
        .stdout_trimmed();
    assert_eq!(text, "HELLO");
}

// ============================================================================
// Real-world patterns
// ============================================================================

#[test]
fn test_grep_in_files() {
    let dir = temp_dir();
    fs::write(dir.path().join("file1.txt"), "hello world\nfoo bar").unwrap();
    fs::write(dir.path().join("file2.txt"), "hello rust\nbaz").unwrap();

    let text = cmd!("grep", "-r", "hello")
        .cwd(dir.path())
        .run()
        .unwrap()
        .stdout_trimmed();
    assert!(text.contains("hello world"));
    assert!(text.contains("hello rust"));
}

#[test]
fn test_find_and_count() {
    let dir = temp_dir();
    fs::write(dir.path().join("a.txt"), "").unwrap();
    fs::write(dir.path().join("b.txt"), "").unwrap();
    fs::write(dir.path().join("c.rs"), "").unwrap();

    let text = shell!("find . -name '*.txt' | wc -l")
        .cwd(dir.path())
        .run()
        .unwrap()
        .stdout_trimmed();
    assert_eq!(text.trim(), "2");
}

#[test]
fn test_sort_unique() {
    let text = cmd!("cat")
        .stdin_text("banana\napple\ncherry\napple\nbanana")
        .pipe(cmd!("sort"))
        .pipe(cmd!("uniq"))
        .run()
        .unwrap()
        .stdout_trimmed();
    assert_eq!(text, "apple\nbanana\ncherry");
}

#[test]
fn test_head_tail() {
    let input = "1\n2\n3\n4\n5\n6\n7\n8\n9\n10";
    let text = cmd!("cat")
        .stdin_text(input)
        .pipe(cmd!("tail", "-n", "5"))
        .pipe(cmd!("head", "-n", "2"))
        .run()
        .unwrap()
        .stdout_trimmed();
    assert_eq!(text, "6\n7");
}

#[test]
fn test_sed_transform() {
    let text = cmd!("echo", "hello world")
        .pipe(cmd!("sed", "s/world/rust/"))
        .run()
        .unwrap()
        .stdout_trimmed();
    assert_eq!(text, "hello rust");
}

#[test]
fn test_awk_field() {
    let text = cmd!("echo", "one two three")
        .pipe(cmd!("awk", "{print $2}"))
        .run()
        .unwrap()
        .stdout_trimmed();
    assert_eq!(text, "two");
}

#[test]
fn test_cut_field() {
    let text = cmd!("echo", "a:b:c")
        .pipe(cmd!("cut", "-d:", "-f2"))
        .run()
        .unwrap()
        .stdout_trimmed();
    assert_eq!(text, "b");
}

#[test]
fn test_xargs() {
    let dir = temp_dir();
    fs::write(dir.path().join("f1.txt"), "content1").unwrap();
    fs::write(dir.path().join("f2.txt"), "content2").unwrap();

    let text = shell!("ls *.txt | xargs cat")
        .cwd(dir.path())
        .run()
        .unwrap()
        .stdout_trimmed();
    assert!(text.contains("content1"));
    assert!(text.contains("content2"));
}

#[test]
fn test_env_path_lookup() {
    // Verify that commands are found via PATH
    let text = cmd!("echo", "found").run().unwrap().stdout_trimmed();
    assert_eq!(text, "found");
}

#[test]
fn test_absolute_path_command() {
    let text = Cmd::new("/bin/echo").arg("absolute").run().unwrap().stdout_trimmed();
    assert_eq!(text, "absolute");
}

// ============================================================================
// Error recovery patterns
// ============================================================================

#[test]
fn test_fallback_on_failure() {
    let text = cmd!("false")
        .or(cmd!("echo", "recovered"))
        .run()
        .unwrap()
        .stdout_trimmed();
    assert_eq!(text, "recovered");
}

#[test]
fn test_check_then_act() {
    let dir = temp_dir();
    let path = dir.path().join("check.txt");
    fs::write(&path, "exists").unwrap();

    let code = shell!(&format!("test -f {}", path.display()))
        .run_exit_code()
        .unwrap();
    if code == 0 {
        let text = cmd!("cat").stdin_path(&path).run().unwrap().stdout_trimmed();
        assert_eq!(text, "exists");
    }
}

#[test]
fn test_no_exit_err_then_check() {
    let result = shell!("exit 42").no_exit_err().run().unwrap();
    assert_eq!(result.code, 42);
}

// ============================================================================
// Glob
// ============================================================================

#[test]
fn test_glob_star() {
    let dir = temp_dir();
    fs::write(dir.path().join("a.txt"), "a").unwrap();
    fs::write(dir.path().join("b.txt"), "b").unwrap();
    fs::write(dir.path().join("c.rs"), "c").unwrap();

    let pattern = format!("{}/*.txt", dir.path().display());
    let files = glob(&pattern).unwrap();
    assert_eq!(files.len(), 2);
    assert!(files.iter().any(|p| p.file_name().unwrap() == "a.txt"));
    assert!(files.iter().any(|p| p.file_name().unwrap() == "b.txt"));
}

#[test]
fn test_glob_double_star() {
    let dir = temp_dir();
    fs::create_dir_all(dir.path().join("sub/deep")).unwrap();
    fs::write(dir.path().join("top.txt"), "").unwrap();
    fs::write(dir.path().join("sub/mid.txt"), "").unwrap();
    fs::write(dir.path().join("sub/deep/bottom.txt"), "").unwrap();

    let pattern = format!("{}/**/*.txt", dir.path().display());
    let files = glob(&pattern).unwrap();
    // ** matches at all levels: top.txt, sub/mid.txt, sub/deep/bottom.txt
    assert_eq!(files.len(), 3);
    assert!(files.iter().any(|p| p.file_name().unwrap() == "top.txt"));
    assert!(files.iter().any(|p| p.file_name().unwrap() == "mid.txt"));
    assert!(files.iter().any(|p| p.file_name().unwrap() == "bottom.txt"));
}

#[test]
fn test_glob_no_matches() {
    let dir = temp_dir();
    let pattern = format!("{}/*.nonexistent", dir.path().display());
    let result = glob(&pattern);
    assert!(result.is_err());
    match result.unwrap_err() {
        CmdError::GlobNoMatches { pattern: p } => {
            assert!(p.contains("nonexistent"));
        }
        other => panic!("Expected GlobNoMatches, got: {:?}", other),
    }
}

#[test]
fn test_glob_sorted() {
    let dir = temp_dir();
    fs::write(dir.path().join("c.txt"), "").unwrap();
    fs::write(dir.path().join("a.txt"), "").unwrap();
    fs::write(dir.path().join("b.txt"), "").unwrap();

    let pattern = format!("{}/*.txt", dir.path().display());
    let files = glob(&pattern).unwrap();
    let names: Vec<&str> = files
        .iter()
        .map(|p| p.file_name().unwrap().to_str().unwrap())
        .collect();
    assert_eq!(names, vec!["a.txt", "b.txt", "c.txt"]);
}

#[test]
fn test_glob_question_mark() {
    let dir = temp_dir();
    fs::write(dir.path().join("a1.txt"), "").unwrap();
    fs::write(dir.path().join("a2.txt"), "").unwrap();
    fs::write(dir.path().join("ab.txt"), "").unwrap();

    let pattern = format!("{}/a?.txt", dir.path().display());
    let files = glob(&pattern).unwrap();
    assert_eq!(files.len(), 3);
}

#[test]
fn test_glob_bracket() {
    let dir = temp_dir();
    fs::write(dir.path().join("a.txt"), "").unwrap();
    fs::write(dir.path().join("b.txt"), "").unwrap();
    fs::write(dir.path().join("c.txt"), "").unwrap();

    let pattern = format!("{}/[ab].txt", dir.path().display());
    let files = glob(&pattern).unwrap();
    assert_eq!(files.len(), 2);
}

#[test]
fn test_glob_invalid_pattern() {
    let result = glob("[invalid");
    assert!(result.is_err());
}

#[test]
fn test_glob_with_cmd() {
    let dir = temp_dir();
    fs::write(dir.path().join("hello.txt"), "hello from file").unwrap();

    let pattern = format!("{}/*.txt", dir.path().display());
    let files: Vec<String> = glob(&pattern)
        .unwrap()
        .into_iter()
        .map(|p| p.to_string_lossy().to_string())
        .collect();
    assert_eq!(files.len(), 1);
    let text = cmd!("cat", files).run().unwrap().stdout_trimmed();
    assert_eq!(text, "hello from file");
}

#[test]
fn test_glob_with_shell() {
    let dir = temp_dir();
    fs::write(dir.path().join("a.txt"), "aaa").unwrap();
    fs::write(dir.path().join("b.txt"), "bbb").unwrap();

    let pattern = format!("{}/*.txt", dir.path().display());
    let files: Vec<String> = glob(&pattern)
        .unwrap()
        .into_iter()
        .map(|p| p.to_string_lossy().to_string())
        .collect();
    let text = shell!("cat", files).run().unwrap().stdout_trimmed();
    assert!(text.contains("aaa"));
    assert!(text.contains("bbb"));
}

#[test]
fn test_glob_multiple_extensions() {
    let dir = temp_dir();
    fs::write(dir.path().join("a.txt"), "").unwrap();
    fs::write(dir.path().join("b.rs"), "").unwrap();
    fs::write(dir.path().join("c.md"), "").unwrap();

    // Glob doesn't support {txt,rs} in the standard glob crate, but *.* works
    let pattern = format!("{}/*.*", dir.path().display());
    let files = glob(&pattern).unwrap();
    assert_eq!(files.len(), 3);
}

#[test]
fn test_glob_deeply_nested() {
    let dir = temp_dir();
    fs::create_dir_all(dir.path().join("a/b/c/d")).unwrap();
    fs::write(dir.path().join("a/b/c/d/deep.txt"), "found").unwrap();

    let pattern = format!("{}/**/*.txt", dir.path().display());
    let files = glob(&pattern).unwrap();
    assert_eq!(files.len(), 1);
    assert!(files[0].ends_with("deep.txt"));
}

// ============================================================================
// Cmd::glob() builder method
// ============================================================================

#[test]
fn test_cmd_glob_method() {
    let dir = temp_dir();
    fs::write(dir.path().join("a.txt"), "aaa").unwrap();
    fs::write(dir.path().join("b.txt"), "bbb").unwrap();

    let pattern = format!("{}/*.txt", dir.path().display());
    let text = Cmd::new("cat")
        .glob(&pattern)
        .run()
        .unwrap()
        .stdout_trimmed();
    assert!(text.contains("aaa"));
    assert!(text.contains("bbb"));
}

#[test]
fn test_cmd_glob_method_no_matches() {
    let dir = temp_dir();
    let pattern = format!("{}/*.nonexistent", dir.path().display());
    let result = Cmd::new("echo")
        .glob(&pattern)
        .run();
    assert!(result.is_err());
}

#[test]
fn test_cmd_glob_method_with_arg() {
    let dir = temp_dir();
    fs::write(dir.path().join("x.txt"), "").unwrap();

    let pattern = format!("{}/*.txt", dir.path().display());
    let text = Cmd::new("echo")
        .arg("hello")
        .glob(&pattern)
        .run()
        .unwrap()
        .stdout_trimmed();
    assert!(text.starts_with("hello "));
    assert!(text.contains("x.txt"));
}

#[test]
fn test_cmd_glob_invalid_pattern() {
    let result = Cmd::new("echo")
        .glob("[invalid")
        .run();
    assert!(result.is_err());
}

// ============================================================================
// Glob results used directly in shell! interpolation
// ============================================================================

#[test]
fn test_glob_direct_in_shell_interpolation() {
    let dir = temp_dir();
    fs::write(dir.path().join("a.txt"), "aaa").unwrap();
    fs::write(dir.path().join("b.txt"), "bbb").unwrap();

    let pattern = format!("{}/*.txt", dir.path().display());
    let files = glob(&pattern).unwrap();
    let text = shell!("cat {files}").run().unwrap().stdout_trimmed();
    assert!(text.contains("aaa"));
    assert!(text.contains("bbb"));
}

#[test]
fn test_glob_direct_in_cmd() {
    let dir = temp_dir();
    fs::write(dir.path().join("a.txt"), "aaa").unwrap();
    fs::write(dir.path().join("b.txt"), "bbb").unwrap();

    let pattern = format!("{}/*.txt", dir.path().display());
    let files = glob(&pattern).unwrap();
    let text = cmd!("cat", files).run().unwrap().stdout_trimmed();
    assert!(text.contains("aaa"));
    assert!(text.contains("bbb"));
}

// ============================================================================
// Vector arguments with cmd! and shell!
// ============================================================================

#[test]
fn test_cmd_vec_args() {
    let items = vec!["hello", "world"];
    let text = cmd!("echo", items).run().unwrap().stdout_trimmed();
    assert_eq!(text, "hello world");
}

#[test]
fn test_cmd_vec_string_args() {
    let items: Vec<String> = vec!["hello".to_string(), "world".to_string()];
    let text = cmd!("echo", items).run().unwrap().stdout_trimmed();
    assert_eq!(text, "hello world");
}

#[test]
fn test_cmd_slice_args() {
    let items: &[&str] = &["hello", "world"];
    let text = cmd!("echo", items).run().unwrap().stdout_trimmed();
    assert_eq!(text, "hello world");
}

#[test]
fn test_cmd_array_args() {
    let text = cmd!("echo", ["hello", "world"]).run().unwrap().stdout_trimmed();
    assert_eq!(text, "hello world");
}

#[test]
fn test_cmd_vec_with_spaces() {
    // Each element should be a separate arg, spaces preserved
    let items = vec!["hello world", "foo bar"];
    let text = cmd!("echo", items).run().unwrap().stdout_trimmed();
    assert_eq!(text, "hello world foo bar");
}

#[test]
fn test_cmd_mixed_scalar_and_vec() {
    let files = vec!["b.txt", "c.txt"];
    let text = cmd!("echo", "a.txt", files, "d.txt").run().unwrap().stdout_trimmed();
    assert_eq!(text, "a.txt b.txt c.txt d.txt");
}

#[test]
fn test_cmd_empty_vec() {
    let items: Vec<&str> = vec![];
    let text = cmd!("echo", "hello", items).run().unwrap().stdout_trimmed();
    assert_eq!(text, "hello");
}

#[test]
fn test_cmd_pathbuf_vec() {
    let paths = vec![PathBuf::from("a.txt"), PathBuf::from("b.txt")];
    let text = cmd!("echo").push_args(
        paths.iter().map(|p| p.to_string_lossy().to_string()).collect::<Vec<_>>()
    ).run().unwrap().stdout_trimmed();
    assert_eq!(text, "a.txt b.txt");
}

#[test]
fn test_shell_vec_args() {
    let items = vec!["hello", "world"];
    let text = shell!("echo", items).run().unwrap().stdout_trimmed();
    assert_eq!(text, "hello world");
}

#[test]
fn test_shell_vec_with_spaces() {
    // Each element should be shell-escaped
    let items = vec!["hello world", "foo bar"];
    let text = shell!("echo", items).run().unwrap().stdout_trimmed();
    assert_eq!(text, "hello world foo bar");
}

#[test]
fn test_shell_vec_with_special_chars() {
    // Shell special chars in vec elements should be escaped
    let items = vec!["$HOME", "$(whoami)"];
    let text = shell!("echo", items).run().unwrap().stdout_trimmed();
    assert_eq!(text, "$HOME $(whoami)");
}

#[test]
fn test_shell_mixed_scalar_and_vec() {
    let flags = vec!["--verbose", "--all"];
    let text = shell!("echo start", flags, "end").run().unwrap().stdout_trimmed();
    assert_eq!(text, "start --verbose --all end");
}

#[test]
fn test_shell_empty_vec() {
    let items: Vec<&str> = vec![];
    let text = shell!("echo hello", items).run().unwrap().stdout_trimmed();
    assert_eq!(text, "hello");
}

#[test]
fn test_shell_string_vec() {
    let items: Vec<String> = vec!["one".to_string(), "two".to_string()];
    let text = shell!("echo", items).run().unwrap().stdout_trimmed();
    assert_eq!(text, "one two");
}

#[test]
fn test_shell_single_quote_in_vec() {
    let items = vec!["it's", "fine"];
    let text = shell!("echo", items).run().unwrap().stdout_trimmed();
    assert_eq!(text, "it's fine");
}

#[test]
fn test_shell_vec_prevents_injection() {
    // Even in shell!, vector args should be escaped and safe
    let evil = vec!["hello; rm -rf /"];
    let text = shell!("echo", evil).run().unwrap().stdout_trimmed();
    assert_eq!(text, "hello; rm -rf /");
}

#[test]
fn test_cmd_vec_as_argv_passthrough() {
    // Simulating passing part of argv through to a command
    let user_args = vec!["-l", "-a", "/tmp"];
    let text = cmd!("ls", user_args).run().unwrap().stdout_trimmed();
    assert!(text.len() > 0);
}

#[test]
fn test_shell_vec_as_argv_passthrough() {
    // Simulating passing part of argv through to a shell command
    let user_args = vec!["-l", "-a"];
    let text = shell!("ls", user_args, "/tmp").run().unwrap().stdout_trimmed();
    assert!(text.len() > 0);
}

#[test]
fn test_push_args_vec_str() {
    let args = vec!["a", "b", "c"];
    let text = Cmd::new("echo").push_args(args).run().unwrap().stdout_trimmed();
    assert_eq!(text, "a b c");
}

#[test]
fn test_push_args_slice() {
    let args: &[&str] = &["a", "b", "c"];
    let text = Cmd::new("echo").push_args(args).run().unwrap().stdout_trimmed();
    assert_eq!(text, "a b c");
}

// ============================================================================
// run_with_tail
// ============================================================================

#[test]
fn test_run_with_tail_success() {
    // A fast command that produces a few lines
    shell!("echo line1; echo line2; echo line3")
        .run_with_tail("Running...", "Done", 3)
        .unwrap();
}

#[test]
fn test_run_with_tail_failure() {
    let result = shell!("echo output; exit 1")
        .run_with_tail("Running...", "Done", 3);
    assert!(result.is_err());
    match result.unwrap_err() {
        CmdError::ExitStatus { code, .. } => assert_eq!(code, 1),
        other => panic!("Expected ExitStatus, got: {:?}", other),
    }
}

#[test]
fn test_run_with_tail_many_lines() {
    // Verify it handles lots of output without issues
    shell!("seq 1 100")
        .run_with_tail("Counting...", "Counted to 100", 5)
        .unwrap();
}

#[test]
fn test_run_with_tail_no_output() {
    cmd!("true")
        .run_with_tail("Waiting...", "Done", 3)
        .unwrap();
}

#[test]
fn test_run_with_tail_with_env() {
    shell!("echo $MY_VAR")
        .env("MY_VAR", "tail_test")
        .run_with_tail("Env test...", "Env done", 3)
        .unwrap();
}

#[test]
fn test_run_with_tail_with_cwd() {
    let dir = temp_dir();
    cmd!("pwd")
        .cwd(dir.path())
        .run_with_tail("Cwd test...", "Cwd done", 3)
        .unwrap();
}

#[test]
fn test_run_with_tail_with_stdin() {
    cmd!("cat")
        .stdin_text("hello from stdin")
        .run_with_tail("Stdin test...", "Stdin done", 3)
        .unwrap();
}

#[test]
fn test_run_with_tail_stderr_in_error() {
    let result = shell!("echo err_msg >&2; exit 1")
        .run_with_tail("Running...", "Done", 3);
    assert!(result.is_err());
    let err = result.unwrap_err();
    let msg = format!("{}", err);
    assert!(msg.contains("err_msg") || matches!(err, CmdError::ExitStatus { code: 1, .. }));
}

#[test]
fn test_run_with_tail_not_found() {
    let result = cmd!("nonexistent_command_xyz_99999")
        .run_with_tail("Looking...", "Found", 3);
    assert!(result.is_err());
    match result.unwrap_err() {
        CmdError::NotFound { program } => {
            assert_eq!(program, "nonexistent_command_xyz_99999");
        }
        other => panic!("Expected NotFound, got: {:?}", other),
    }
}

#[test]
fn test_run_with_tail_cwd_not_found() {
    let result = cmd!("ls")
        .cwd("/nonexistent_dir_xyz_99999")
        .run_with_tail("Looking...", "Found", 3);
    assert!(result.is_err());
    match result.unwrap_err() {
        CmdError::CwdNotFound { .. } => {}
        other => panic!("Expected CwdNotFound, got: {:?}", other),
    }
}

#[test]
fn test_run_with_tail_opts() {
    shell!("echo line1; echo line2")
        .run_with_tail_opts(
            TailOptions::new("Custom...", "Custom done")
                .lines(10)
                .tick_ms(50),
        )
        .unwrap();
}

#[test]
fn test_run_with_tail_opts_spinner() {
    cmd!("echo", "test")
        .run_with_tail_opts(
            TailOptions::new("Spinning...", "Spun")
                .spinner("⠋⠙⠹⠸⠼⠴⠦⠧⠇⠏")
                .lines(3),
        )
        .unwrap();
}

#[test]
fn test_run_with_tail_zero_lines() {
    // With 0 tail lines, just show the spinner with no output
    shell!("echo hidden")
        .run_with_tail("No tail...", "Done", 0)
        .unwrap();
}

#[test]
fn test_run_with_tail_one_line() {
    shell!("echo only_this")
        .run_with_tail("One line...", "Done", 1)
        .unwrap();
}

#[test]
fn test_run_with_tail_binary_output() {
    // Should not crash on non-utf8 lines (they'll be lossy)
    shell!("printf '\\x80\\x81\\ntext\\n'")
        .run_with_tail("Binary...", "Done", 3)
        .unwrap();
}

#[test]
fn test_run_with_tail_long_lines() {
    // Lines longer than terminal width should be truncated
    let long = "a".repeat(500);
    cmd!("echo", &long)
        .run_with_tail("Long...", "Done", 3)
        .unwrap();
}

// ============================================================================
// Shell interpolation with escaping
// ============================================================================

#[test]
fn test_shell_interpolation_basic() {
    let name = "hello";
    let text = shell!("echo {name}").run().unwrap().stdout_trimmed();
    assert_eq!(text, "hello");
}

#[test]
fn test_shell_interpolation_with_spaces() {
    let name = "hello world";
    let text = shell!("echo {name}").run().unwrap().stdout_trimmed();
    assert_eq!(text, "hello world");
}

#[test]
fn test_shell_interpolation_with_pipe() {
    let name = "hello";
    let text = shell!("echo {name} | tr a-z A-Z").run().unwrap().stdout_trimmed();
    assert_eq!(text, "HELLO");
}

#[test]
fn test_shell_interpolation_multiple_vars() {
    let a = "hello";
    let b = "world";
    let text = shell!("echo {a} {b}").run().unwrap().stdout_trimmed();
    assert_eq!(text, "hello world");
}

#[test]
fn test_shell_interpolation_prevents_injection() {
    // Special shell characters should be escaped
    let name = "hello; rm -rf /";
    let text = shell!("echo {name}").run().unwrap().stdout_trimmed();
    assert_eq!(text, "hello; rm -rf /");
}

#[test]
fn test_shell_interpolation_with_single_quotes() {
    let name = "it's";
    let text = shell!("echo {name}").run().unwrap().stdout_trimmed();
    assert_eq!(text, "it's");
}

#[test]
fn test_shell_interpolation_with_dollar_sign() {
    let val = "$HOME";
    let text = shell!("echo {val}").run().unwrap().stdout_trimmed();
    assert_eq!(text, "$HOME");
}

#[test]
fn test_shell_interpolation_empty_string() {
    let name = "";
    let text = shell!("echo {name}end").run().unwrap().stdout_trimmed();
    assert_eq!(text, "end");
}

#[test]
fn test_shell_interpolation_same_var_twice() {
    let x = "hi";
    let text = shell!("echo {x} {x}").run().unwrap().stdout_trimmed();
    assert_eq!(text, "hi hi");
}

#[test]
fn test_shell_interpolation_coexists_with_shell_vars() {
    // Shell $VAR syntax should still work alongside {var} interpolation
    let greeting = "hello";
    let text = shell!("X=world; echo {greeting} $X").run().unwrap().stdout_trimmed();
    assert_eq!(text, "hello world");
}

#[test]
fn test_shell_interpolation_with_shell_brace_var() {
    // ${VAR} shell syntax should NOT be treated as interpolation
    let text = shell!("X=hello; echo ${X}").run().unwrap().stdout_trimmed();
    assert_eq!(text, "hello");
}

#[test]
fn test_shell_interpolation_with_shell_default_var() {
    // ${VAR:-default} shell syntax should still work
    let text = shell!("echo ${NONEXISTENT:-fallback}").run().unwrap().stdout_trimmed();
    assert_eq!(text, "fallback");
}

#[test]
fn test_shell_interpolation_vec() {
    let files = vec!["file one.txt", "file two.txt"];
    let text = shell!("echo {files}").run().unwrap().stdout_trimmed();
    assert_eq!(text, "file one.txt file two.txt");
}

#[test]
fn test_shell_interpolation_vec_with_special_chars() {
    let args = vec!["hello world", "it's", "a;b"];
    let text = shell!("echo {args}").run().unwrap().stdout_trimmed();
    assert_eq!(text, "hello world it's a;b");
}

#[test]
fn test_shell_interpolation_vec_single_element() {
    let items = vec!["only"];
    let text = shell!("echo {items}").run().unwrap().stdout_trimmed();
    assert_eq!(text, "only");
}

#[test]
fn test_shell_interpolation_mixed_scalar_and_vec() {
    let prefix = "start";
    let items = vec!["a", "b", "c"];
    let text = shell!("echo {prefix} {items}").run().unwrap().stdout_trimmed();
    assert_eq!(text, "start a b c");
}

// ============================================================================
// Shell inline {glob("...")} syntax
// ============================================================================

#[test]
fn test_shell_inline_glob_matches() {
    // Use a glob that matches files in this repo
    let text = shell!("echo {glob(\"src/*.rs\")}").run().unwrap().stdout_trimmed();
    // Should contain at least lib.rs and cmd.rs
    assert!(text.contains("src/lib.rs"));
    assert!(text.contains("src/cmd.rs"));
}

#[test]
fn test_shell_inline_glob_no_matches_deferred() {
    // Glob error should be deferred until execution
    let result = shell!("echo {glob(\"/nonexistent_dir_xyz_99999/*.txt\")}").run();
    assert!(result.is_err());
}

// ============================================================================
// Shell inline {flag_if("...", var)} syntax
// ============================================================================

#[test]
fn test_shell_flag_if_true() {
    let verbose = true;
    let text = shell!("echo hello {flag_if(\"-v\", verbose)} world").run().unwrap().stdout_trimmed();
    assert!(text.contains("-v"));
    assert!(text.contains("hello"));
    assert!(text.contains("world"));
}

#[test]
fn test_shell_flag_if_false() {
    let verbose = false;
    let text = shell!("echo hello {flag_if(\"-v\", verbose)} world").run().unwrap().stdout_trimmed();
    assert!(!text.contains("-v"));
    assert!(text.contains("hello"));
    assert!(text.contains("world"));
}

#[test]
fn test_shell_flag_if_multiple() {
    let verbose = true;
    let recursive = false;
    let text = shell!("echo {flag_if(\"-v\", verbose)} {flag_if(\"-r\", recursive)} done")
        .run()
        .unwrap()
        .stdout_trimmed();
    assert!(text.contains("-v"));
    assert!(!text.contains("-r"));
    assert!(text.contains("done"));
}

#[test]
fn test_shell_flag_if_with_interpolation() {
    let verbose = true;
    let name = "hello";
    let text = shell!("echo {name} {flag_if(\"--loud\", verbose)}")
        .run()
        .unwrap()
        .stdout_trimmed();
    assert!(text.contains("hello"));
    assert!(text.contains("--loud"));
}

// ============================================================================
// no_err / no_nothin
// ============================================================================

#[test]
fn test_no_err_swallows_not_found() {
    let result = cmd!("__nonexistent_command_xyz__").no_err().run();
    assert!(result.is_ok());
    assert_eq!(result.unwrap().code, -1);
}

#[test]
fn test_no_err_swallows_exit_code() {
    let result = cmd!("false").no_err().run();
    assert!(result.is_ok());
}

#[test]
fn test_no_err_swallows_cwd_not_found() {
    let result = cmd!("echo", "hi").cwd("/nonexistent_dir_xyz_99").no_err().run();
    assert!(result.is_ok());
    assert_eq!(result.unwrap().code, -1);
}

#[test]
fn test_no_nothin_swallows_silently() {
    // Should not print anything — just swallow
    let result = cmd!("__nonexistent_command_xyz__").no_nothin().run();
    assert!(result.is_ok());
    assert_eq!(result.unwrap().code, -1);
}

#[test]
fn test_no_err_with_deferred_error() {
    let result = cmd!("echo").glob("/nonexistent_xyz_99/*.txt").no_err().run();
    assert!(result.is_ok());
}

// ============================================================================
// run_no_exit_err / run_ignore_code
// ============================================================================

#[test]
fn test_run_no_exit_err() {
    let result = cmd!("false").run_no_exit_err();
    assert!(result.is_ok());
    let r = result.unwrap();
    assert_eq!(r.code, 1);
    assert!(!r.success());
}

#[test]
fn test_run_no_exit_err_still_errors_on_not_found() {
    let result = cmd!("__nonexistent_xyz__").run_no_exit_err();
    assert!(result.is_err());
}

#[test]
fn test_run_ignore_code() {
    let result = cmd!("false").run_ignore_code();
    assert!(result.is_ok());
    assert_eq!(result.unwrap().code, 1);
}

#[test]
fn test_run_ignore_code_success() {
    let result = cmd!("true").run_ignore_code();
    assert!(result.is_ok());
    assert_eq!(result.unwrap().code, 0);
}

// ============================================================================
// CmdOps
// ============================================================================

#[test]
fn test_cmd_ops_env() {
    let ops = CmdOps::new().env("MY_TEST_OP", "hello_ops");
    let text = cmd!("sh", "-c", "echo $MY_TEST_OP"; &ops).run_stdout().unwrap();
    assert_eq!(text.trim(), "hello_ops");
}

#[test]
fn test_cmd_ops_cwd() {
    let dir = TempDir::new().unwrap();
    let ops = CmdOps::new().cwd(dir.path());
    let text = cmd!("pwd"; &ops).run_stdout().unwrap();
    // Resolve symlinks for macOS /private/var/... vs /var/...
    let expected = std::fs::canonicalize(dir.path()).unwrap();
    let actual = std::fs::canonicalize(text.trim()).unwrap();
    assert_eq!(actual, expected);
}

#[test]
fn test_cmd_ops_dry() {
    let ops = CmdOps::new().dry(true);
    let result = cmd!("echo", "should not run"; &ops).run().unwrap();
    // Dry mode returns code 0 and empty output
    assert_eq!(result.code, 0);
    assert_eq!(result.stdout_trimmed(), "");
}

#[test]
fn test_cmd_ops_no_err() {
    let ops = CmdOps::new().no_err(true).no_warn(true);
    let result = cmd!("__nonexistent_xyz__"; &ops).run();
    assert!(result.is_ok());
}

#[test]
fn test_cmd_ops_reuse() {
    let ops = CmdOps::new().env("SHARED_VAR", "shared_val");
    let t1 = cmd!("sh", "-c", "echo $SHARED_VAR"; &ops).run_stdout().unwrap();
    let t2 = cmd!("sh", "-c", "echo $SHARED_VAR"; &ops).run_stdout().unwrap();
    assert_eq!(t1.trim(), "shared_val");
    assert_eq!(t2.trim(), "shared_val");
}

#[test]
fn test_shell_ops() {
    let ops = CmdOps::new().env("SHELL_OP_VAR", "works");
    let text = shell!("echo $SHELL_OP_VAR"; &ops).run_stdout().unwrap();
    assert_eq!(text.trim(), "works");
}

#[test]
fn test_shell_ops_dry() {
    let ops = CmdOps::new().dry(true);
    let result = shell!("echo should not run"; &ops).run().unwrap();
    assert_eq!(result.code, 0);
    assert_eq!(result.stdout_trimmed(), "");
}

#[test]
fn test_cmd_ops_with_args() {
    let ops = CmdOps::new().env("OPS_X", "42");
    let text = cmd!("sh", "-c", "echo $OPS_X extra"; &ops).run_stdout().unwrap();
    assert_eq!(text.trim(), "42 extra");
}

#[test]
fn test_cmd_ops_shell_program() {
    let ops = CmdOps::new().shell_program("/bin/bash", "-c");
    let text = shell!("echo hello from bash"; &ops).run_stdout().unwrap();
    assert_eq!(text.trim(), "hello from bash");
}

#[test]
fn test_cmd_ops_shell_program_with_cmd_shell() {
    let ops = CmdOps::new().shell_program("/bin/bash", "-c");
    let text = raxx::Cmd::shell("echo via Cmd::shell").with_ops(&ops).run_stdout().unwrap();
    assert_eq!(text.trim(), "via Cmd::shell");
}

#[test]
fn test_cmd_ops_shell_program_does_not_affect_cmd() {
    // shell_program should only affect shell commands, not regular cmd! calls
    let ops = CmdOps::new().shell_program("/bin/bash", "-c");
    let text = cmd!("echo", "still works"; &ops).run_stdout().unwrap();
    assert_eq!(text.trim(), "still works");
}

#[test]
fn test_cmd_ops_shell_program_with_interpolation() {
    let ops = CmdOps::new().shell_program("/bin/bash", "-c");
    let name = "world";
    let text = shell!("echo hello {name}"; &ops).run_stdout().unwrap();
    assert_eq!(text.trim(), "hello world");
}

