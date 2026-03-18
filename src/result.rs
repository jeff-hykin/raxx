/// The output of a command execution.
#[derive(Debug, Clone)]
pub struct CmdOutput {
    /// The exit code of the command.
    pub code: i32,
    /// Captured stdout bytes (empty if stdout was not captured).
    pub stdout: Vec<u8>,
    /// Captured stderr bytes (empty if stderr was not captured).
    pub stderr: Vec<u8>,
}

impl CmdOutput {
    /// Get stdout as a trimmed UTF-8 string.
    pub fn stdout_text(&self) -> crate::Result<String> {
        let s = String::from_utf8(self.stdout.clone())?;
        Ok(s.trim().to_string())
    }

    /// Get stderr as a trimmed UTF-8 string.
    pub fn stderr_text(&self) -> crate::Result<String> {
        let s = String::from_utf8(self.stderr.clone())?;
        Ok(s.trim().to_string())
    }

    /// Get stdout as a UTF-8 string (not trimmed).
    pub fn stdout_text_raw(&self) -> crate::Result<String> {
        Ok(String::from_utf8(self.stdout.clone())?)
    }

    /// Get stderr as a UTF-8 string (not trimmed).
    pub fn stderr_text_raw(&self) -> crate::Result<String> {
        Ok(String::from_utf8(self.stderr.clone())?)
    }

    /// Parse stdout as JSON.
    pub fn stdout_json<T: serde::de::DeserializeOwned>(&self) -> crate::Result<T> {
        Ok(serde_json::from_slice(&self.stdout)?)
    }

    /// Parse stderr as JSON.
    pub fn stderr_json<T: serde::de::DeserializeOwned>(&self) -> crate::Result<T> {
        Ok(serde_json::from_slice(&self.stderr)?)
    }

    /// Get stdout split into lines (trimmed, empty lines removed).
    pub fn stdout_lines(&self) -> crate::Result<Vec<String>> {
        let text = self.stdout_text()?;
        Ok(text.lines().map(|l| l.to_string()).collect())
    }

    /// Get stderr split into lines (trimmed, empty lines removed).
    pub fn stderr_lines(&self) -> crate::Result<Vec<String>> {
        let text = self.stderr_text()?;
        Ok(text.lines().map(|l| l.to_string()).collect())
    }

    /// Check if the command succeeded (exit code 0).
    pub fn success(&self) -> bool {
        self.code == 0
    }
}
