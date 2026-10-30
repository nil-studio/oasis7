//! Test-child readiness must become visible only after its complete payload.
use std::os::unix::fs::OpenOptionsExt;
use std::{
    fs::{self, File, OpenOptions},
    io::{self, Write},
    path::Path,
};

pub(crate) fn publish_ready_marker(path: &Path) -> Result<(), String> {
    publish_with(path, |file| file.write_all(b"ready"))
        .map_err(|error| format!("publish child readiness {}: {error}", path.display()))
}

fn publish_with(path: &Path, write: impl FnOnce(&mut File) -> io::Result<()>) -> io::Result<()> {
    use std::sync::atomic::{AtomicU64, Ordering};
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let temporary = path.with_extension(format!(
        "ready-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    let mut file = OpenOptions::new()
        .create_new(true)
        .write(true)
        .mode(0o600)
        .open(&temporary)?;
    // Only a path successfully created by this invocation is owned for cleanup.
    struct Staging<'a>(&'a Path);
    impl Drop for Staging<'_> {
        fn drop(&mut self) {
            let _ = fs::remove_file(self.0);
        }
    }
    let _staging = Staging(&temporary);
    write(&mut file)?;
    file.sync_all()?;
    drop(file);
    // A same-directory hard link publishes the complete file without replacing
    // an existing marker or symlink. Readiness is not a durable protocol receipt.
    fs::hard_link(&temporary, path)?;
    fs::remove_file(&temporary)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::{PermissionsExt, symlink};
    use std::{
        path::PathBuf,
        sync::{
            atomic::{AtomicU64, Ordering},
            mpsc,
        },
        thread,
        time::Duration,
    };
    struct Root(PathBuf);
    impl Root {
        fn new() -> Self {
            static NEXT: AtomicU64 = AtomicU64::new(0);
            let path = std::env::temp_dir().canonicalize().unwrap().join(format!(
                "oasis7-ready-marker-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            fs::create_dir(&path).unwrap();
            fs::set_permissions(&path, fs::Permissions::from_mode(0o700)).unwrap();
            Self(path)
        }
    }
    impl Drop for Root {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn incomplete_marker_is_not_visible_to_parent() {
        let root = Root::new();
        let path = root.0.join("ready");
        let target = path.clone();
        let (staged_tx, staged_rx) = mpsc::channel();
        let (resume_tx, resume_rx) = mpsc::channel();
        let writer = thread::spawn(move || {
            publish_with(&target, |file| {
                file.write_all(b"re")?;
                staged_tx.send(()).unwrap();
                resume_rx.recv_timeout(Duration::from_secs(5)).unwrap();
                file.write_all(b"ady")
            })
        });
        staged_rx.recv_timeout(Duration::from_secs(5)).unwrap();
        let visible = path.exists();
        resume_tx.send(()).unwrap();
        writer.join().unwrap().unwrap();
        assert!(!visible, "parent observed an incomplete readiness marker");
        assert_eq!(fs::read(&path).unwrap(), b"ready");
        assert_eq!(
            fs::metadata(path).unwrap().permissions().mode() & 0o777,
            0o600
        );
    }

    #[test]
    fn failed_write_does_not_publish_or_leave_staging_files() {
        let root = Root::new();
        let path = root.0.join("ready");
        let result = publish_with(&path, |file| {
            file.write_all(b"re")?;
            Err(io::Error::other("injected write failure"))
        });
        assert!(result.is_err());
        assert!(!path.exists());
        assert_eq!(fs::read_dir(&root.0).unwrap().count(), 0);
    }

    #[test]
    fn existing_marker_and_symlink_are_not_replaced() {
        let root = Root::new();
        let path = root.0.join("ready");
        fs::write(&path, b"original").unwrap();
        assert!(publish_ready_marker(&path).is_err());
        assert_eq!(fs::read(&path).unwrap(), b"original");
        let link = root.0.join("link");
        symlink(&path, &link).unwrap();
        assert!(publish_ready_marker(&link).is_err());
        assert!(fs::symlink_metadata(link).unwrap().file_type().is_symlink());
        assert_eq!(fs::read(&path).unwrap(), b"original");
        assert_eq!(fs::read_dir(&root.0).unwrap().count(), 2);
    }
}
