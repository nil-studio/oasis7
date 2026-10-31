//! Publication barrier for the private provider checkpoint, before actor dispatch.
//! This file is also tested in isolation; it has no Runtime or provider dependency.
use std::{
    fs,
    io,
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
    let parent = path.parent().filter(|p| !p.as_os_str().is_empty()).unwrap_or(Path::new("."));
    fs::create_dir_all(parent)?;
    let serial = NEXT_TEMP.fetch_add(1, Ordering::Relaxed);
    let temp = path.with_extension(format!("tmp-{}-{serial}", std::process::id()));
    let result = (|| {
        fs::write(&temp, bytes)?;
        fs::rename(&temp, path)?;
        after_publication()?;
        Ok(())
    })();
    if result.is_err() {
        // An uncertain canonical publication is never rolled back.
        let _ = fs::remove_file(temp);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    struct Directory(std::path::PathBuf);
    impl Directory {
        fn new() -> Self {
            let serial = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
            let path = std::env::temp_dir().join(format!("oasis7-provider-barrier-{}-{serial}", std::process::id()));
            fs::create_dir(&path).unwrap();
            Self(path)
        }
    }
    impl Drop for Directory {
        fn drop(&mut self) { let _ = fs::remove_dir_all(&self.0); }
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
        assert!(result.is_err(), "same-length corruption must fail the pre-dispatch barrier");
    }

    #[test]
    fn exact_original_survives_reopen_and_replacement() {
        let directory = Directory::new();
        let path = directory.0.join("nested").join("lineage.json");
        for original in [b"original-request".as_slice(), b"original-response-with-identity"] {
            publish(&path, original).unwrap();
            assert_eq!(fs::read(&path).unwrap(), original);
        }
    }
}
