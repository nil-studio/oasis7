use oasis7_distfs::incremental_page_store::PageIoBudget;
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT_DIRECTORY: AtomicU64 = AtomicU64::new(0);
pub(crate) struct Directory(pub(crate) std::path::PathBuf);
impl Directory {
    pub(crate) fn new() -> Self {
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let sequence = NEXT_DIRECTORY.fetch_add(1, Ordering::Relaxed);
        let base = std::env::var_os("RUNNER_TEMP")
            .map(std::path::PathBuf::from)
            .and_then(|p| p.canonicalize().ok())
            .filter(|p| p.is_absolute() && p.is_dir())
            .unwrap_or_else(std::env::temp_dir);
        let path = base.join(format!(
            "oasis7-industry-jobs-{}-{nonce}-{sequence}",
            std::process::id()
        ));
        std::fs::create_dir(&path).unwrap();
        Self(path)
    }
}
impl Drop for Directory {
    fn drop(&mut self) {
        std::fs::remove_dir_all(&self.0).unwrap();
    }
}
pub(crate) fn physical() -> PageIoBudget {
    PageIoBudget {
        reads_remaining: 100_000,
        read_bytes_remaining: 256 << 20,
        writes_remaining: 100_000,
        write_bytes_remaining: 256 << 20,
    }
}
