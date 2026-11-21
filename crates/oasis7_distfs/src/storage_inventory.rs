//! Read-only physical inventory for one caller-selected storage domain.
//! This does not establish history completeness, root liveness or domain independence.
use serde::Serialize;
use std::collections::BTreeSet;
use std::fs;
#[cfg(unix)]
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};
use std::time::Instant;

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
    inventory_storage_with_entry_limit(root, usize::MAX)
}

/// Bound metadata traversal. Directory entries are streamed depth first rather
/// than queued by width; open directory iterators scale with depth, and inode
/// deduplication still scales with unique entries. Exhaustion is an incomplete
/// inventory, never a complete capacity baseline.
pub fn inventory_storage_with_entry_limit(root: &Path, maximum_entries: usize) -> StorageInventory {
    inventory_storage_with_limits(root, maximum_entries, None)
}

/// Apply one caller-owned absolute deadline as well as the entry limit.
/// Checks are cooperative between synchronous filesystem operations; a blocked
/// filesystem call cannot be preempted, and a late return remains incomplete.
pub fn inventory_storage_with_limits(
    root: &Path,
    maximum_entries: usize,
    deadline: Option<Instant>,
) -> StorageInventory {
    inventory_storage_with_clock(root, maximum_entries, deadline, Instant::now)
}

fn inventory_storage_with_clock(
    root: &Path,
    maximum_entries: usize,
    deadline: Option<Instant>,
    mut now: impl FnMut() -> Instant,
) -> StorageInventory {
    let mut result = StorageInventory {
        complete: true,
        allocated_bytes: cfg!(unix).then_some(0),
        ..Default::default()
    };
    #[cfg(not(unix))]
    result
        .errors
        .push("device/inode and allocated bytes unavailable on this platform".into());
    let mut pending = InventoryPaths {
        root: Some(root.to_path_buf()),
        directories: Vec::new(),
    };
    let mut seen = BTreeSet::new();
    let mut visited = 0usize;
    loop {
        if deadline.is_some_and(|deadline| now() >= deadline) {
            result
                .errors
                .push("inventory walltime budget exhausted".into());
            break;
        }
        let entry = pending.next();
        if deadline.is_some_and(|deadline| now() >= deadline) {
            result
                .errors
                .push("inventory walltime budget exhausted".into());
            break;
        }
        let Some(entry) = entry else {
            break;
        };
        if visited >= maximum_entries {
            result.errors.push(format!(
                "inventory entry budget exhausted: {maximum_entries}"
            ));
            break;
        }
        visited += 1;
        let path = match entry {
            Ok(path) => path,
            Err(error) => {
                result.errors.push(error);
                continue;
            }
        };
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
                Ok(entries) => pending.directories.push((path, entries)),
                Err(error) => result.errors.push(format!("{}: {error}", path.display())),
            }
        }
    }
    result.errors.sort();
    if deadline.is_some_and(|deadline| now() >= deadline)
        && !result.errors.iter().any(|error| error.contains("walltime"))
    {
        result
            .errors
            .push("inventory walltime budget exhausted".into());
    }
    result.complete = result.errors.is_empty();
    result
}

// Keep only active ancestors, never all sibling paths from a wide directory.
struct InventoryPaths {
    root: Option<PathBuf>,
    directories: Vec<(PathBuf, fs::ReadDir)>,
}
impl Iterator for InventoryPaths {
    type Item = Result<PathBuf, String>;
    fn next(&mut self) -> Option<Self::Item> {
        if let Some(root) = self.root.take() {
            return Some(Ok(root));
        }
        while let Some((path, entries)) = self.directories.last_mut() {
            if let Some(entry) = entries.next() {
                return Some(
                    entry
                        .map(|entry| entry.path())
                        .map_err(|error| format!("{}: {error}", path.display())),
                );
            }
            self.directories.pop();
        }
        None
    }
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
    fn deadline_exhaustion_never_produces_complete_inventory() {
        let fixture = Fixture::new();
        fs::write(fixture.0.join("retained"), b"retained").unwrap();
        let expired = inventory_storage_with_limits(&fixture.0, 10, Some(Instant::now()));
        assert!(!expired.complete);
        assert_eq!(expired.file_paths, 0);
        assert!(
            expired
                .errors
                .iter()
                .any(|error| error.contains("walltime"))
        );
        let complete = inventory_storage_with_limits(
            &fixture.0,
            2,
            Some(Instant::now() + std::time::Duration::from_secs(60)),
        );
        assert!(complete.complete, "{:?}", complete.errors);
        assert_eq!(complete.file_paths, 1);
        assert!(!inventory_storage_with_limits(&fixture.0, 1, None).complete);
    }
    #[test]
    fn deadline_during_traversal_keeps_partial_counters() {
        let fixture = Fixture::new();
        fs::write(fixture.0.join("first"), b"first").unwrap();
        fs::write(fixture.0.join("second"), b"second").unwrap();
        let before = Instant::now();
        let expired = before + std::time::Duration::from_secs(1);
        let mut checks = 0;
        let result = inventory_storage_with_clock(&fixture.0, 3, Some(expired), || {
            checks += 1;
            if checks <= 4 { before } else { expired }
        });
        assert!(!result.complete);
        assert_eq!(result.directories, 1);
        assert_eq!(result.file_paths, 1);
        assert!(result.errors.iter().any(|error| error.contains("walltime")));
    }
    #[test]
    fn wide_nested_tree_preserves_exact_entry_boundary() {
        let fixture = Fixture::new();
        let nested = fixture.0.join("nested");
        fs::create_dir(&nested).unwrap();
        for index in 0..1024 {
            fs::write(fixture.0.join(format!("wide-{index}")), b"a").unwrap();
            fs::write(nested.join(format!("nested-{index}")), b"bc").unwrap();
        }
        // Root + nested directory + every name in both directories.
        let exact_entries = 2050;
        let exact = inventory_storage_with_entry_limit(&fixture.0, exact_entries);
        assert!(exact.complete, "{:?}", exact.errors);
        assert_eq!(exact.directories, 2);
        assert_eq!(exact.file_paths, 2048);
        assert_eq!(exact.unique_files, 2048);
        assert_eq!(exact.apparent_bytes, 3072);
        assert_eq!(exact.unique_apparent_bytes, 3072);
        let partial = inventory_storage_with_entry_limit(&fixture.0, exact_entries - 1);
        assert!(!partial.complete);
        assert_eq!(partial.file_paths + partial.directories, 2049);
        assert!(
            partial
                .errors
                .iter()
                .any(|error| error.contains("budget exhausted"))
        );
    }
    #[test]
    fn empty_directory_uses_one_entry() {
        let fixture = Fixture::new();
        let result = inventory_storage_with_entry_limit(&fixture.0, 1);
        assert!(result.complete, "{:?}", result.errors);
        assert_eq!(result.directories, 1);
        assert_eq!(result.file_paths, 0);
    }
    #[test]
    fn missing_root_is_incomplete() {
        let fixture = Fixture::new();
        let result = inventory_storage(&fixture.0.join("missing"));
        assert!(!result.complete);
        assert!(result.errors.iter().any(|error| error.contains("missing")));
    }
    #[test]
    fn bounded_inventory_exhaustion_cannot_be_a_complete_capacity_baseline() {
        let fixture = Fixture::new();
        fs::write(fixture.0.join("first"), b"first").unwrap();
        fs::write(fixture.0.join("second"), b"second").unwrap();
        let partial = inventory_storage_with_entry_limit(&fixture.0, 2);
        assert!(!partial.complete);
        assert!(
            partial
                .errors
                .iter()
                .any(|error| error.contains("entry budget exhausted"))
        );
        assert!(partial.file_paths + partial.directories <= 2);
        let exact = inventory_storage_with_entry_limit(&fixture.0, 3);
        assert!(exact.complete, "{:?}", exact.errors);
        assert_eq!(exact.file_paths, 2);
        assert_eq!(exact.unique_apparent_bytes, 11);
        assert!(!inventory_storage_with_entry_limit(&fixture.0, 0).complete);
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
    fn hardlinks_across_nested_directories_keep_every_name() {
        let fixture = Fixture::new();
        let left = fixture.0.join("left");
        let right = fixture.0.join("right");
        fs::create_dir(&left).unwrap();
        fs::create_dir(&right).unwrap();
        fs::write(left.join("original"), b"original").unwrap();
        fs::hard_link(left.join("original"), right.join("alias")).unwrap();
        let result = inventory_storage_with_entry_limit(&fixture.0, 5);
        assert!(result.complete, "{:?}", result.errors);
        assert_eq!(result.directories, 3);
        assert_eq!(result.file_paths, 2);
        assert_eq!(result.unique_files, 1);
        assert_eq!(result.apparent_bytes, 16);
        assert_eq!(result.unique_apparent_bytes, 8);
        let expected = [&fixture.0, &left, &right, &left.join("original")]
            .into_iter()
            .map(|path| fs::metadata(path).unwrap().blocks() * 512)
            .sum();
        assert_eq!(result.allocated_bytes, Some(expected));
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
