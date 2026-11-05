use super::*;

#[test]
fn incremental_archive_gc_preserves_hot_records_and_releases_them_after_eviction() {
    let dir = temp_dir("execution-retention-hot-beyond-archive");
    let records_dir = dir.join("records");
    let store = LocalCasStore::new(dir.join("store"));
    fs::create_dir_all(&records_dir).expect("create records dir");
    const HOT_WINDOW: u64 = 8;
    const INTERVAL: u64 = 2;
    const KEEP: usize = 2;
    let mut originals = Vec::new();
    for height in 1..=24 {
        let mut record =
            persist_test_execution_record_with_store_refs(&records_dir, &store, height);
        // Opaque archive originals: retention fixture, not authenticated proof evidence.
        record.commit_log_ref = Some(
            store
                .put_bytes(format!("archive-log-{height}").as_bytes())
                .unwrap(),
        );
        record.world_head_proof_ref = Some(
            store
                .put_bytes(format!("head-proof-{height}").as_bytes())
                .unwrap(),
        );
        record.world_head_proof_hash = Some(format!("proof-hash-{height}"));
        record.checkpoint_ref =
            maybe_persist_execution_checkpoint_for_record(&records_dir, &record, INTERVAL, KEEP)
                .expect("persist checkpoint");
        persist_execution_bridge_record(&records_dir, &record)
            .expect("persist checkpointed record");
        originals.push(record.clone());
        run_execution_bridge_incremental_retention_maintenance(
            &records_dir,
            &store,
            &record,
            HOT_WINDOW,
            INTERVAL,
            KEEP,
        )
        .expect("incremental retention");
        if height == 6 {
            // A full reconciliation must preserve the same complete hot service view.
            run_execution_bridge_retention_maintenance(&records_dir, &store, HOT_WINDOW)
                .expect("full retention reconciliation");
            // The archive span is only four heights; both originals remain hot.
            for original in &originals[..2] {
                let retained = load_execution_bridge_record(&execution_bridge_record_path(
                    &records_dir,
                    original.height,
                ))
                .expect("load hot record after archive pruning");
                assert_eq!(retained.commit_log_ref, original.commit_log_ref);
                assert_eq!(retained.world_head_proof_ref, original.world_head_proof_ref);
                assert_eq!(
                    retained.world_head_proof_hash,
                    original.world_head_proof_hash
                );
                assert_eq!(retained.external_effect_ref, original.external_effect_ref);
                for reference in [
                    original.commit_log_ref.as_ref().unwrap(),
                    original.world_head_proof_ref.as_ref().unwrap(),
                    original.external_effect_ref.as_ref().unwrap(),
                ] {
                    assert!(
                        store.get_verified(reference).is_ok(),
                        "hot service-view archive dependency was collected"
                    );
                }
                assert_eq!(retained.snapshot_ref, original.snapshot_ref);
                assert_eq!(retained.journal_ref, original.journal_ref);
                for reference in [
                    original.snapshot_ref.as_ref().unwrap(),
                    original.journal_ref.as_ref().unwrap(),
                ] {
                    assert!(
                        store.get_verified(reference).is_ok(),
                        "archive GC removed a still-hot original at height {}",
                        original.height
                    );
                }
                let mirror = original.simulator_mirror.as_ref().unwrap();
                assert!(store.get_verified(&mirror.snapshot_ref).is_ok());
                assert!(store.get_verified(&mirror.journal_ref).is_ok());
            }
        }
        if height == 12 {
            for original in &originals[..2] {
                let evicted = load_execution_bridge_record(&execution_bridge_record_path(
                    &records_dir,
                    original.height,
                ))
                .expect("load evicted record");
                assert!(evicted.commit_log_ref.is_none());
                assert!(evicted.world_head_proof_ref.is_none());
                assert!(evicted.world_head_proof_hash.is_none());
                assert!(evicted.external_effect_ref.is_none());
                for reference in [
                    original.commit_log_ref.as_ref().unwrap(),
                    original.world_head_proof_ref.as_ref().unwrap(),
                    original.external_effect_ref.as_ref().unwrap(),
                ] {
                    assert!(
                        !store.has(reference).unwrap(),
                        "expired archive dependency remained pinned"
                    );
                }
                assert!(evicted.snapshot_ref.is_none());
                assert!(evicted.journal_ref.is_none());
                assert!(evicted.simulator_mirror.is_none());
                let mirror = original.simulator_mirror.as_ref().unwrap();
                for reference in [
                    original.snapshot_ref.as_ref().unwrap(),
                    original.journal_ref.as_ref().unwrap(),
                    &mirror.snapshot_ref,
                    &mirror.journal_ref,
                ] {
                    assert!(
                        !store.has(reference).expect("check evicted original"),
                        "expired hot original remained pinned"
                    );
                }
            }
        }
    }
    let scope_dir = store.root().join("pin_scopes/execution_bridge_v1");
    let shard_count = fs::read_dir(scope_dir).expect("read pin shards").count();
    assert!(
        shard_count <= HOT_WINDOW as usize + KEEP + INTERVAL as usize,
        "hot/archive overlap leaked pin shards: {shard_count}"
    );
    let _ = fs::remove_dir_all(dir);
}
