//! Read-only physical inventory for one caller-selected storage domain.
//! This does not establish history completeness, root liveness or domain independence.
use serde::Serialize;
use std::collections::BTreeSet;
use std::fs;
#[cfg(unix)]
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};

#[derive(Debug, Default, Serialize)]
pub struct StorageInventory {
    pub complete: bool,
    pub file_paths: u64,
    pub unique_files: u64,
    pub directories: u64,
    /// File lengths summed by name, including hard-link aliases.
    pub apparent_bytes: u64,
    /// File lengths summed once per device/inode within this domain.
    pub unique_apparent_bytes: u64,
    /// Files and directories, once per device/inode; unavailable off Unix.
    pub allocated_bytes: Option<u64>,
    pub errors: Vec<String>,
}

/// Call on a quiescent tree, once for each independently declared domain.
/// Never follows symlinks. Incomplete measurements retain partial counters and
/// explicit errors; callers must not interpret them as a capacity baseline.
pub fn inventory_storage(root: &Path) -> StorageInventory {
    let mut result = StorageInventory {
        complete: true,
        allocated_bytes: cfg!(unix).then_some(0),
        ..Default::default()
    };
    #[cfg(not(unix))]
    result
        .errors
        .push("device/inode and allocated bytes unavailable on this platform".into());
    let mut pending = vec![PathBuf::from(root)];
    let mut seen = BTreeSet::new();
    while let Some(path) = pending.pop() {
        let metadata = match fs::symlink_metadata(&path) {
            Ok(metadata) => metadata,
            Err(error) => {
                result.errors.push(format!("{}: {error}", path.display()));
                continue;
            }
        };
        if !metadata.is_file() && !metadata.is_dir() {
            result.errors.push(format!(
                "{}: unsupported file type (symlinks are not followed)",
                path.display()
            ));
            continue;
        }
        #[cfg(unix)]
        let first = seen.insert((metadata.dev(), metadata.ino()));
        #[cfg(not(unix))]
        let first = seen.insert(path.clone());
        if metadata.is_file() {
            result.file_paths += 1;
            result.apparent_bytes += metadata.len();
            if first {
                result.unique_files += 1;
                result.unique_apparent_bytes += metadata.len();
            }
        }
        #[cfg(unix)]
        if first {
            let sum = metadata
                .blocks()
                .checked_mul(512)
                .and_then(|bytes| result.allocated_bytes?.checked_add(bytes));
            if sum.is_none() {
                result.errors.push(format!(
                    "{}: allocated byte counter overflow",
                    path.display()
                ));
            }
            result.allocated_bytes = sum;
        }
        if metadata.is_dir() && first {
            result.directories += 1;
            match fs::read_dir(&path) {
                Ok(entries) => {
                    for entry in entries {
                        match entry {
                            Ok(entry) => pending.push(entry.path()),
                            Err(error) => {
                                result.errors.push(format!("{}: {error}", path.display()))
                            }
                        }
                    }
                }
                Err(error) => result.errors.push(format!("{}: {error}", path.display())),
            }
        }
    }
    result.errors.sort();
    result.complete = result.errors.is_empty();
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};
    static NEXT: AtomicU64 = AtomicU64::new(0);
    struct Fixture(PathBuf);
    impl Fixture {
        fn new() -> Self {
            let path = std::env::temp_dir().join(format!(
                "o7-inventory-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            fs::create_dir(&path).unwrap();
            Self(path)
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            fs::remove_dir_all(&self.0).unwrap();
        }
    }
    #[test]
    fn missing_root_is_incomplete() {
        let fixture = Fixture::new();
        let result = inventory_storage(&fixture.0.join("missing"));
        assert!(!result.complete);
        assert!(result.errors.iter().any(|error| error.contains("missing")));
    }
    #[cfg(unix)]
    #[test]
    fn hardlinks_count_once_per_domain_and_sparse_allocation_is_distinct() {
        let fixture = Fixture::new();
        let path = fixture.0.join("pack");
        let file = fs::File::create(&path).unwrap();
        file.set_len(16 * 1024 * 1024).unwrap();
        fs::hard_link(&path, fixture.0.join("alias")).unwrap();
        let result = inventory_storage(&fixture.0);
        assert!(result.complete, "{:?}", result.errors);
        assert_eq!(result.file_paths, 2);
        assert_eq!(result.unique_files, 1);
        assert_eq!(result.apparent_bytes, 32 * 1024 * 1024);
        assert_eq!(result.unique_apparent_bytes, 16 * 1024 * 1024);
        let expected = fs::metadata(&path).unwrap().blocks() * 512
            + fs::metadata(&fixture.0).unwrap().blocks() * 512;
        assert_eq!(result.allocated_bytes, Some(expected));
        // A second domain is measured independently even if its fixture aliases
        // the same inode. This does not attest that the domains are independent.
        assert_eq!(inventory_storage(&path).unique_files, 1);
    }
    #[cfg(unix)]
    #[test]
    fn symlinks_are_reported_without_traversing_the_target() {
        let fixture = Fixture::new();
        std::os::unix::fs::symlink(&fixture.0, fixture.0.join("loop")).unwrap();
        let result = inventory_storage(&fixture.0);
        assert!(!result.complete);
        assert_eq!(result.directories, 1);
        assert_eq!(result.errors.len(), 1);
    }
}
