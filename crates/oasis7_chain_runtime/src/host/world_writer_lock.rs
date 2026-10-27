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
        let root = std::env::temp_dir().join(format!(
            "oasis7-chain-runtime-world-writer-lock-{}-{unique}",
            std::process::id()
        ));
        std::fs::create_dir_all(&root).unwrap();
        let world_dir = root.join("world");
        let mut live_node = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "world_writer_lock::tests::live_writer_child"])
            .env("OASIS7_TEST_WORLD_WRITER_ROOT", &root)
            .spawn().unwrap();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
        while !root.join("ready").exists() {
            assert!(std::time::Instant::now() < deadline);
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        let error = ExclusiveDirectoryProcessLock::try_acquire(&world_dir)
            .err()
            .expect("importer-style guard must fail closed while node is live");
        assert!(error.contains("lock") && error.contains("held"), "{error}");
        std::fs::write(root.join("release"), b"go").unwrap();
        assert!(live_node.wait().unwrap().success());
        ExclusiveDirectoryProcessLock::try_acquire(&world_dir)
            .expect("guard release permits the next writer");
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn live_writer_child() {
        let Some(root) = std::env::var_os("OASIS7_TEST_WORLD_WRITER_ROOT") else { return; };
        let root = std::path::PathBuf::from(root);
        let _guard = acquire_live_world_writer_lock(&root.join("world")).unwrap();
        std::fs::write(root.join("ready"), b"ready").unwrap();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
        while !root.join("release").exists() {
            assert!(std::time::Instant::now() < deadline);
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
    }

}
