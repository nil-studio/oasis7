//! Worker output must outlive the short-lived CLI which launches the owner.
use std::{
    fs::{File, OpenOptions},
    os::unix::fs::{FileExt, OpenOptionsExt},
    path::{Path, PathBuf},
    process::Stdio,
};
const STDERR_LIMIT: usize = 16 * 1024;

/// Private per-attempt output files remain open in the worker after the CLI
/// exits. Only diagnostic reads are bounded; these are ordinary runtime logs,
/// not a promise of bounded on-disk log retention.
pub(super) struct WorkerDiagnostics {
    stdout: File,
    stderr: File,
    stdout_path: PathBuf,
    stderr_path: PathBuf,
}

impl WorkerDiagnostics {
    pub(super) fn prepare(directory: &Path) -> Result<Self, String> {
        use super::super::super::node_lifecycle::local_io as io;
        io::private_dir(directory)?;
        let attempt = io::random_id()?;
        let stdout_path = directory.join(format!("worker-{attempt}.stdout.log"));
        let stderr_path = directory.join(format!("worker-{attempt}.stderr.log"));
        let open = |path: &Path| {
            let file = OpenOptions::new()
                .read(true)
                .append(true)
                .create_new(true)
                .mode(0o600)
                .custom_flags(libc::O_NOFOLLOW)
                .open(path)
                .map_err(|error| {
                    format!("formal owner private output {}: {error}", path.display())
                })?;
            super::super::super::controlled_live_config::require_private_owned(path, false)?;
            Ok::<_, String>(file)
        };
        Ok(Self {
            stdout: open(&stdout_path)?,
            stderr: open(&stderr_path)?,
            stdout_path,
            stderr_path,
        })
    }

    pub(super) fn stdio(&self) -> Result<(Stdio, Stdio), String> {
        Ok((
            Stdio::from(self.stdout.try_clone().map_err(|error| error.to_string())?),
            Stdio::from(self.stderr.try_clone().map_err(|error| error.to_string())?),
        ))
    }

    fn tail(file: &File) -> std::io::Result<Vec<u8>> {
        // Positional reads cannot move the worker's shared output offset.
        // O_APPEND on the inherited write handles also preserves append-only
        // output while the CLI samples diagnostics during startup.
        let length = file.metadata()?.len();
        let offset = length.saturating_sub((STDERR_LIMIT / 2) as u64);
        let count = (length - offset) as usize;
        let mut bytes = vec![0; count];
        let mut read = 0;
        while read < bytes.len() {
            let count = file.read_at(&mut bytes[read..], offset + read as u64)?;
            if count == 0 {
                break;
            }
            read += count;
        }
        bytes.truncate(read);
        Ok(bytes)
    }

    fn text(&self) -> Option<String> {
        let mut bytes = Self::tail(&self.stderr).ok()?;
        bytes.extend(Self::tail(&self.stdout).ok()?);
        if bytes.is_empty() {
            return None;
        }
        let text = String::from_utf8_lossy(&bytes);
        let diagnostic: String = text
            .chars()
            .filter(|character| !character.is_control() || matches!(character, '\n' | '\t' | '\r'))
            .take(2048)
            .collect();
        (!diagnostic.trim().is_empty()).then_some(diagnostic)
    }
}

pub(super) fn append_worker_diagnostic(message: &str, diagnostics: &WorkerDiagnostics) -> String {
    let location = format!(
        "worker stdout={} stderr={}",
        diagnostics.stdout_path.display(),
        diagnostics.stderr_path.display()
    );
    match diagnostics.text() {
        Some(stderr) => format!("{message}; {location}; worker stdout/stderr: {stderr}"),
        None => format!("{message}; {location}"),
    }
}

#[cfg(test)]
#[path = "formal_live_owner_diagnostics_tests.rs"]
mod tests;
