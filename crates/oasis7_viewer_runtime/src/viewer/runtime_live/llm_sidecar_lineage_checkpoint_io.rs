//! Publication barrier for the private provider checkpoint, before actor dispatch.
//! This file is also tested in isolation; it has no Runtime or provider dependency.
use std::{
    fs,
    io::{self, Read, Write},
    path::Path,
    sync::atomic::{AtomicU64, Ordering},
};

static NEXT_TEMP: AtomicU64 = AtomicU64::new(0);

pub(super) fn publish(path: &Path, bytes: &[u8]) -> io::Result<()> {
    publish_observed(path, bytes, || Ok(()))
}

fn publish_observed(
    path: &Path,
    bytes: &[u8],
    after_publication: impl FnOnce() -> io::Result<()>,
) -> io::Result<()> {
    let parent = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    fs::create_dir_all(parent)?;
    let serial = NEXT_TEMP.fetch_add(1, Ordering::Relaxed);
    let temp = path.with_extension(format!("tmp-{}-{serial}", std::process::id()));
    // Own the temporary inode before arming cleanup; create_new failure must
    // never remove another writer's existing temporary file.
    let mut options = fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(&temp)?;
    let result = (|| {
        file.write_all(bytes)?;
        file.sync_all()?;
        drop(file);
        fs::rename(&temp, path)?;
        // Unix is the existing formal durable deployment profile. Sync the
        // complete directory chain because create_dir_all may have created
        // several ancestors. Other platforms retain their existing directory
        // durability scope; this helper grants no cross-platform IWS receipt.
        #[cfg(unix)]
        sync_directories(parent)?;
        after_publication()?;
        let mut published = fs::File::open(path)?;
        if !published.metadata()?.is_file() || published.metadata()?.len() != bytes.len() as u64 {
            return Err(io::Error::other(
                "provider checkpoint readback length/type mismatch",
            ));
        }
        let mut scratch = [0u8; 8192];
        for expected in bytes.chunks(scratch.len()) {
            let actual = &mut scratch[..expected.len()];
            published.read_exact(actual)?;
            if actual != expected {
                return Err(io::Error::other(
                    "provider checkpoint readback content mismatch",
                ));
            }
        }
        if published.read(&mut scratch[..1])? != 0 {
            return Err(io::Error::other(
                "provider checkpoint readback trailing bytes",
            ));
        }
        Ok(())
    })();
    if result.is_err() {
        // An uncertain canonical publication is never rolled back.
        let _ = fs::remove_file(temp);
    }
    result
}

#[cfg(unix)]
fn sync_directories(mut directory: &Path) -> io::Result<()> {
    loop {
        fs::File::open(directory)?.sync_all()?;
        let Some(parent) = directory.parent().filter(|p| !p.as_os_str().is_empty()) else {
            // A relative single-component path was itself synced above; its
            // directory entry is owned by the current working directory.
            if directory != Path::new(".") && !directory.is_absolute() {
                fs::File::open(".")?.sync_all()?;
            }
            return Ok(());
        };
        directory = parent;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    struct Directory(std::path::PathBuf);
    impl Directory {
        fn new() -> Self {
            let serial = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos();
            let path = std::env::temp_dir().join(format!(
                "oasis7-provider-barrier-{}-{serial}",
                std::process::id()
            ));
            fs::create_dir(&path).unwrap();
            Self(path)
        }
    }
    impl Drop for Directory {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn changed_published_bytes_cannot_grant_dispatch_permission() {
        let directory = Directory::new();
        let path = directory.0.join("lineage.json");
        let original = br#"{"original":"request-and-response","phase":"active"}"#;
        let result = publish_observed(&path, original, || {
            let mut changed = original.to_vec();
            changed[0] ^= 1;
            fs::write(&path, changed)
        });
        assert!(
            result.is_err(),
            "same-length corruption must fail the pre-dispatch barrier"
        );
    }

    #[test]
    fn exact_original_survives_reopen_and_replacement() {
        let directory = Directory::new();
        let path = directory.0.join("nested").join("lineage.json");
        for original in [
            b"original-request".as_slice(),
            b"original-response-with-identity",
        ] {
            publish(&path, original).unwrap();
            assert_eq!(fs::read(&path).unwrap(), original);
        }
    }
    #[test]
    fn uncertain_publication_keeps_original_identity_for_reconciliation() {
        let directory = Directory::new();
        let path = directory.0.join("lineage.json");
        let original = b"original-active-turn";
        assert!(
            publish_observed(&path, original, || Err(io::Error::other(
                "injected failure"
            )))
            .is_err()
        );
        assert_eq!(fs::read(&path).unwrap(), original);
        publish(&path, original).unwrap();
        assert_eq!(fs::read(&path).unwrap(), original);
    }

    #[test]
    fn short_and_extended_publications_are_rejected() {
        let directory = Directory::new();
        let path = directory.0.join("lineage.json");
        for changed in [b"short".as_slice(), b"original-active-turn-extra"] {
            assert!(
                publish_observed(&path, b"original-active-turn", || fs::write(&path, changed))
                    .is_err()
            );
        }
    }
}
