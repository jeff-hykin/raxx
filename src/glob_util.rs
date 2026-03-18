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
    Ok(paths)
}
