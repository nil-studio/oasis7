use std::path::Path;

use oasis7::runtime_host::auth_storage::ExclusiveDirectoryProcessLock;

pub(super) fn acquire_live_world_writer_lock(
    world_dir: &Path,
) -> Result<ExclusiveDirectoryProcessLock, String> {
    ExclusiveDirectoryProcessLock::try_acquire(world_dir).map_err(|error| {
        format!(
            "acquire live execution world writer lock for {} failed: {error}",
            world_dir.display()
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn live_node_guard_excludes_an_importer_guard_for_the_same_world() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("duration")
            .as_nanos();
        let world_dir = std::env::temp_dir().join(format!(
            "oasis7-chain-runtime-world-writer-lock-{}-{unique}",
            std::process::id()
        ));
        let live_node = acquire_live_world_writer_lock(&world_dir).expect("live node owns world");
        let error = ExclusiveDirectoryProcessLock::try_acquire(&world_dir)
            .err()
            .expect("importer-style guard must fail closed while node is live");
        assert!(error.contains("lock") && error.contains("held"), "{error}");
        drop(live_node);
        ExclusiveDirectoryProcessLock::try_acquire(&world_dir)
            .expect("guard release permits the next writer");
        let mut lock_path = world_dir.as_os_str().to_owned();
        lock_path.push(".lock");
        let _ = std::fs::remove_dir_all(std::path::PathBuf::from(lock_path));
    }
}
