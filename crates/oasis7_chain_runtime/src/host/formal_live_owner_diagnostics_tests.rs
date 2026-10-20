use super::*;
use std::{io::Write, os::unix::fs::PermissionsExt, process::Command};

struct Directory(PathBuf);
impl Directory {
    fn new() -> Self {
        let path = std::fs::canonicalize(std::env::temp_dir())
            .unwrap()
            .join(format!(
                "oasis7-owner-output-{}",
                crate::node_lifecycle::local_io::random_id().unwrap()
            ));
        Self(path)
    }
}
impl Drop for Directory {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[test]
fn owner_output_files_survive_launcher_diagnostics_drop() {
    let directory = Directory::new();
    let diagnostics = WorkerDiagnostics::prepare(&directory.0).unwrap();
    let stdout_path = diagnostics.stdout_path.clone();
    let stderr_path = diagnostics.stderr_path.clone();
    let (stdout, stderr) = diagnostics.stdio().unwrap();
    let mut child = Command::new("/bin/sh")
        .args([
            "-c",
            "sleep 0.05; printf owner-still-alive; printf cold-read-diagnostic >&2",
        ])
        .stdout(stdout)
        .stderr(stderr)
        .spawn()
        .unwrap();
    // No launcher-owned reader, diagnostic object or output handle survives.
    // The worker's inherited file descriptors must keep both streams usable.
    drop(diagnostics);
    assert!(child.wait().unwrap().success());
    assert_eq!(std::fs::read(&stdout_path).unwrap(), b"owner-still-alive");
    assert_eq!(
        std::fs::read(&stderr_path).unwrap(),
        b"cold-read-diagnostic"
    );
    for path in [&stdout_path, &stderr_path] {
        assert_eq!(
            std::fs::metadata(path).unwrap().permissions().mode() & 0o777,
            0o600
        );
    }
    assert_eq!(
        std::fs::metadata(&directory.0)
            .unwrap()
            .permissions()
            .mode()
            & 0o777,
        0o700
    );
}

#[test]
fn owner_output_tail_is_bounded_and_never_moves_the_writer_offset() {
    let directory = Directory::new();
    let diagnostics = WorkerDiagnostics::prepare(&directory.0).unwrap();
    let mut stderr = diagnostics.stderr.try_clone().unwrap();
    stderr.write_all(&vec![b'x'; STDERR_LIMIT * 2]).unwrap();
    let before = stderr.metadata().unwrap().len();
    assert_eq!(
        WorkerDiagnostics::tail(&diagnostics.stderr).unwrap().len(),
        STDERR_LIMIT / 2
    );
    let text = diagnostics.text().unwrap();
    assert_eq!(text.chars().count(), 2048);
    stderr.write_all(b"last-worker-output").unwrap();
    assert_eq!(stderr.metadata().unwrap().len(), before + 18);
    assert!(
        std::fs::read(&diagnostics.stderr_path)
            .unwrap()
            .ends_with(b"last-worker-output")
    );
    let second = WorkerDiagnostics::prepare(&directory.0).unwrap();
    assert_ne!(diagnostics.stdout_path, second.stdout_path);
    assert_ne!(diagnostics.stderr_path, second.stderr_path);
}

#[test]
fn owner_output_rejects_a_nonprivate_parent() {
    let directory = Directory::new();
    std::fs::create_dir(&directory.0).unwrap();
    std::fs::set_permissions(&directory.0, std::fs::Permissions::from_mode(0o755)).unwrap();
    assert!(WorkerDiagnostics::prepare(&directory.0).is_err());
}
