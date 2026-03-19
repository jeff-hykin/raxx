use std::path::PathBuf;

/// Find files matching a glob pattern.
///
/// Supports `*` (match any characters in a single path component) and `**`
/// (match zero or more directories recursively). Returns sorted, deduplicated
/// paths.
///
/// The pattern is resolved relative to the current working directory unless
/// it is absolute.
///
/// # Examples
///
/// ```no_run
/// use raxx::glob;
///
/// // All .rs files in src/
/// let files = glob("src/*.rs").unwrap();
///
/// // All .rs files recursively
/// let files = glob("src/**/*.rs").unwrap();
///
/// // Use with cmd! to operate on matched files
/// use raxx::cmd;
/// let files = glob("src/**/*.rs").unwrap();
/// if !files.is_empty() {
///     cmd!("wc", "-l").push_args(files).run().unwrap();
/// }
/// ```
///
/// # Combining with `cmd!` and `shell!`
///
/// The returned `Vec<PathBuf>` can be converted to strings for use with
/// the macros:
///
/// ```no_run
/// use raxx::{cmd, glob};
///
/// let files: Vec<String> = glob("src/**/*.rs").unwrap()
///     .into_iter()
///     .map(|p| p.to_string_lossy().to_string())
///     .collect();
/// cmd!("cat", files).run().unwrap();
/// ```
/// Escape a string so it is treated as a literal path component in a glob pattern.
///
/// Escapes the special characters `*`, `?`, `[`, and `]` by wrapping each in
/// square brackets (e.g. `*` → `[*]`), which is the standard glob escaping
/// convention.
///
/// This is useful when you have a user-provided directory path that may contain
/// glob metacharacters and you want to append a pattern suffix to it.
///
/// # Examples
///
/// ```
/// use raxx::glob_esc;
///
/// assert_eq!(glob_esc("normal/path"), "normal/path");
/// assert_eq!(glob_esc("dir[1]"), "dir[[]1[]]");
/// assert_eq!(glob_esc("file*.txt"), "file[*].txt");
/// assert_eq!(glob_esc("what?"), "what[?]");
/// ```
///
/// ```no_run
/// use raxx::{glob, glob_esc};
///
/// // Safe even if `dir` contains glob metacharacters
/// let dir = "my[project]";
/// let files = glob(&format!("{}/*.rs", glob_esc(dir))).unwrap();
/// ```
pub fn glob_esc(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '*' | '?' => {
                out.push('[');
                out.push(c);
                out.push(']');
            }
            '[' => out.push_str("[[]"),
            ']' => out.push_str("[]]"),
            _ => out.push(c),
        }
    }
    out
}

pub fn glob(pattern: &str) -> crate::Result<Vec<PathBuf>> {
    let entries = glob::glob(pattern).map_err(|e| {
        crate::CmdError::Io(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            format!("Invalid glob pattern: {e}"),
        ))
    })?;

    let mut paths: Vec<PathBuf> = Vec::new();
    for entry in entries {
        match entry {
            Ok(path) => paths.push(path),
            Err(e) => {
                return Err(crate::CmdError::Io(std::io::Error::new(
                    std::io::ErrorKind::Other,
                    format!("Glob error: {e}"),
                )));
            }
        }
    }
    paths.sort();
    paths.dedup();
    if paths.is_empty() {
        return Err(crate::CmdError::GlobNoMatches {
            pattern: pattern.to_string(),
        });
    }
    Ok(paths)
}
