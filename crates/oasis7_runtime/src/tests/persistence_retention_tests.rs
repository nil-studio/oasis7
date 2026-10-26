use super::*;
use std::time::{Duration, Instant};

struct Scratch(PathBuf);
impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
struct ReleasePause(PathBuf);
impl Drop for ReleasePause {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.0);
    }
}

#[test]
fn sidecar_publication_and_gc_result_writeback_share_one_retention_transaction() {
    let scratch = Scratch(temp_dir("sidecar-retention-transaction"));
    World::new().save_to_dir(&scratch.0).unwrap();
    let mut first = World::new();
    first.submit_action(Action::RegisterAgent {
        agent_id: "first-writer".into(),
        pos: pos(0, 0),
    });
    first.step().unwrap();
    let mut second = first.clone();
    second.submit_action(Action::RegisterAgent {
        agent_id: "second-writer".into(),
        pos: pos(1, 1),
    });
    second.step().unwrap();
    let pause = scratch
        .0
        .join(".distfs-state/sidecar-generations/.test-pause-after-index-commit");
    fs::write(&pause, []).unwrap();
    std::thread::scope(|scope| {
        let release = ReleasePause(pause.clone());
        let first_dir = scratch.0.clone();
        let writer_a = scope.spawn(move || first.save_to_dir(&first_dir));
        let deadline = Instant::now() + Duration::from_secs(10);
        while !pause.with_extension("reached").exists() {
            assert!(
                Instant::now() < deadline,
                "first publisher did not reach committed-index seam"
            );
            std::thread::sleep(Duration::from_millis(5));
        }
        let probe = fs::OpenOptions::new()
            .read(true)
            .write(true)
            .open(scratch.0.join(".distfs-state/.retention.lock"))
            .unwrap();
        let transaction_held = match probe.try_lock() {
            Ok(()) => false,
            Err(std::fs::TryLockError::WouldBlock) => true,
            Err(error) => panic!("retention probe failed: {error}"),
        };
        drop(probe);
        let (completed_tx, completed_rx) = std::sync::mpsc::channel();
        let second_dir = scratch.0.clone();
        let writer_b = scope.spawn(move || {
            let result = second.save_to_dir(&second_dir);
            completed_tx.send(result.is_ok()).unwrap();
            result
        });
        if !transaction_held {
            // Force the original bad interleaving: B fully publishes while A
            // still holds its stale in-memory index before GC/writeback.
            assert!(completed_rx.recv_timeout(Duration::from_secs(10)).unwrap());
        }
        drop(release);
        writer_a.join().unwrap().unwrap();
        writer_b.join().unwrap().unwrap();
        assert!(
            transaction_held,
            "sidecar publication must hold retention guard through GC-result writeback"
        );
    });
    let restored = World::load_from_dir(&scratch.0).unwrap();
    assert!(restored.state().agents.contains_key("first-writer"));
    assert!(restored.state().agents.contains_key("second-writer"));
}

#[test]
fn sidecar_revalidates_staged_material_before_index_publication_and_retries_original() {
    let scratch = Scratch(temp_dir("sidecar-retention-missing-staging"));
    World::new().save_to_dir(&scratch.0).unwrap();
    let index_path = scratch
        .0
        .join(".distfs-state/sidecar-generations/index.json");
    let before = fs::read(&index_path).unwrap();
    let index: serde_json::Value = serde_json::from_slice(&before).unwrap();
    let retained: std::collections::BTreeSet<_> = index["generations"]
        .as_object()
        .unwrap()
        .values()
        .flat_map(|record| record["pinned_blob_hashes"].as_array().unwrap())
        .map(|hash| hash.as_str().unwrap().to_owned())
        .collect();
    let mut world = World::new();
    world.submit_action(Action::RegisterAgent {
        agent_id: "original-retry".into(),
        pos: pos(0, 0),
    });
    world.step().unwrap();
    let pause = scratch
        .0
        .join(".distfs-state/sidecar-generations/.test-pause-before-retention-lock");
    fs::write(&pause, []).unwrap();
    let world = std::thread::scope(|scope| {
        let release = ReleasePause(pause.clone());
        let dir = scratch.0.clone();
        let writer = scope.spawn(move || {
            let result = world.save_to_dir(&dir);
            (world, result)
        });
        let deadline = Instant::now() + Duration::from_secs(10);
        while !pause.with_extension("reached").exists() {
            assert!(Instant::now() < deadline, "writer did not finish staging");
            std::thread::sleep(Duration::from_millis(5));
        }
        let store = LocalCasStore::new(scratch.0.join(".distfs-state"));
        let guard = store.acquire_retention_guard().unwrap();
        let staged = fs::read_dir(
            scratch
                .0
                .join(".distfs-state/sidecar-generations/generation.tmp"),
        )
        .unwrap()
        .next()
        .unwrap()
        .unwrap()
        .path();
        let candidate: serde_json::Value =
            serde_json::from_slice(&fs::read(staged.join("generation.json")).unwrap()).unwrap();
        let uncommitted = candidate["pinned_blob_hashes"]
            .as_array()
            .unwrap()
            .iter()
            .map(|hash| hash.as_str().unwrap())
            .find(|hash| !retained.contains(*hash))
            .unwrap();
        // A root-aware maintenance pass may collect an unpublished candidate,
        // while preserving every already committed generation's references.
        fs::remove_file(store.blobs_dir().join(format!("{uncommitted}.blob"))).unwrap();
        drop(guard);
        drop(release);
        let (world, result) = writer.join().unwrap();
        assert!(
            result.is_err(),
            "missing candidate must not publish an index"
        );
        world
    });
    assert_eq!(fs::read(&index_path).unwrap(), before);
    assert!(
        !World::load_from_dir(&scratch.0)
            .unwrap()
            .state()
            .agents
            .contains_key("original-retry")
    );
    world.save_to_dir(&scratch.0).unwrap();
    assert!(
        World::load_from_dir(&scratch.0)
            .unwrap()
            .state()
            .agents
            .contains_key("original-retry")
    );
}
