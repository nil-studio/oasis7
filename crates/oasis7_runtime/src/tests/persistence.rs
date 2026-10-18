use super::super::*;
use super::pos;
use serde_json::json;
use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

#[path = "persistence_generation_tests.rs"]
mod generation_tests;
#[path = "persistence_module_artifact_tests.rs"]
mod module_artifact_tests;
#[path = "persistence_recovery_exact_retry_tests.rs"]
mod recovery_exact_retry_tests;
#[path = "persistence_recovery_tests.rs"]
mod recovery_tests;
#[path = "persistence_resource_tests.rs"]
mod resource_tests;

fn temp_dir(prefix: &str) -> PathBuf {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("duration")
        .as_nanos();
    std::env::temp_dir().join(format!("oasis7-runtime-{prefix}-{unique}"))
}

fn install_test_module(world: &mut World, module_id: &str, wasm_bytes: &[u8]) -> String {
    world.set_policy(PolicySet::allow_all());
    let wasm_hash = util::sha256_hex(wasm_bytes);
    world
        .register_module_artifact(wasm_hash.clone(), wasm_bytes)
        .expect("register module artifact");

    let module_manifest = ModuleManifest {
        module_id: module_id.to_string(),
        name: "Persistence Module".to_string(),
        version: "0.1.0".to_string(),
        kind: ModuleKind::Reducer,
        role: ModuleRole::Rule,
        wasm_hash: wasm_hash.clone(),
        interface_version: "wasm-1".to_string(),
        abi_contract: ModuleAbiContract::default(),
        exports: vec!["reduce".to_string()],
        subscriptions: Vec::new(),
        required_caps: Vec::new(),
        artifact_identity: Some(super::signed_test_artifact_identity(wasm_hash.as_str())),
        limits: ModuleLimits::unbounded(),
    };
    let changes = ModuleChangeSet {
        register: vec![module_manifest.clone()],
        activate: vec![ModuleActivation {
            module_id: module_manifest.module_id.clone(),
            version: module_manifest.version.clone(),
        }],
        ..ModuleChangeSet::default()
    };
    let manifest = Manifest {
        version: 2,
        content: json!({
            "module_changes": changes,
        }),
    };

    let proposal_id = world
        .propose_manifest_update(manifest, "alice")
        .expect("propose module manifest");
    world.shadow_proposal(proposal_id).expect("shadow proposal");
    world
        .approve_proposal(proposal_id, "bob", ProposalDecision::Approve)
        .expect("approve proposal");
    world.apply_proposal(proposal_id).expect("apply proposal");
    wasm_hash
}

#[test]
fn persist_and_restore_world() {
    assert_persistence_recovery_roundtrip();
}

fn assert_persistence_recovery_roundtrip() {
    let mut world = World::new();
    world.submit_action(Action::RegisterAgent {
        agent_id: "agent-1".to_string(),
        pos: pos(0, 0),
    });
    world.step().unwrap();

    let dir = temp_dir("persist-restore");

    world.save_to_dir(&dir).unwrap();

    let restored = World::load_from_dir(&dir).unwrap();
    assert_eq!(restored.state(), world.state());
    assert_eq!(
        restored.tick_consensus_records(),
        world.tick_consensus_records()
    );
    restored
        .verify_tick_consensus_chain()
        .expect("verify persisted tick consensus chain");

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn persist_and_restore_world_defaults_to_module_store_roundtrip() {
    let mut world = World::new();
    let wasm_hash = install_test_module(&mut world, "m.persistence.default", b"persist-default");
    let module_record_key = ModuleRegistry::record_key("m.persistence.default", "0.1.0");
    let dir = temp_dir("persist-module-store-default");

    world
        .save_to_dir(&dir)
        .expect("save with default module store");
    assert!(
        dir.join("module_registry.json").exists(),
        "default save should persist module registry"
    );
    assert!(
        dir.join("modules")
            .join(format!("{wasm_hash}.wasm"))
            .exists(),
        "default save should persist module artifact bytes"
    );

    let mut restored = World::load_from_dir(&dir).expect("load with default module store");
    assert!(
        restored
            .module_registry()
            .records
            .contains_key(&module_record_key)
    );
    let artifact = restored
        .load_module(&wasm_hash)
        .expect("module bytes hydrated from default load");
    assert_eq!(artifact.bytes, b"persist-default".to_vec().into());

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn load_from_dir_ignores_tampered_module_cache_when_snapshot_has_inline_bytes() {
    let mut world = World::new();
    let wasm_hash = install_test_module(&mut world, "m.persistence.tamper", b"persist-tamper");
    let dir = temp_dir("persist-module-store-tamper");

    world.save_to_dir(&dir).expect("save world");
    fs::write(
        dir.join("modules").join(format!("{wasm_hash}.wasm")),
        b"tampered-bytes",
    )
    .expect("tamper module artifact");

    let mut restored = World::load_from_dir(&dir).expect("snapshot artifact remains canonical");
    let artifact = restored
        .load_module(&wasm_hash)
        .expect("inline snapshot bytes should be used");
    assert_eq!(artifact.bytes, b"persist-tamper".to_vec().into());

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn load_from_dir_without_module_store_restores_inline_module_artifacts() {
    let mut world = World::new();
    let wasm_hash = install_test_module(&mut world, "m.persistence.legacy", b"persist-legacy");
    let module_record_key = ModuleRegistry::record_key("m.persistence.legacy", "0.1.0");
    let dir = temp_dir("persist-module-store-legacy");

    world.save_to_dir(&dir).expect("save world");
    fs::remove_file(dir.join("module_registry.json")).expect("remove module registry");
    fs::remove_dir_all(dir.join("modules")).expect("remove module store modules dir");

    let mut restored = World::load_from_dir(&dir).expect("load without module store");
    assert!(
        restored
            .module_registry()
            .records
            .contains_key(&module_record_key)
    );
    let artifact = restored
        .load_module(&wasm_hash)
        .expect("inline snapshot should hydrate module bytes");
    assert_eq!(artifact.bytes, b"persist-legacy".to_vec().into());

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn persist_writes_distfs_sidecar_and_restores_without_json_files() {
    let mut world = World::new();
    world.submit_action(Action::RegisterAgent {
        agent_id: "agent-1".to_string(),
        pos: pos(0, 0),
    });
    world.step().unwrap();

    let dir = temp_dir("persist-distfs-sidecar");
    world.save_to_dir(&dir).expect("save world with sidecar");

    assert!(dir.join("snapshot.manifest.json").exists());
    assert!(dir.join("journal.segments.json").exists());
    assert!(dir.join(".distfs-state").exists());

    fs::remove_file(dir.join("snapshot.json")).expect("remove legacy snapshot");
    fs::remove_file(dir.join("journal.json")).expect("remove legacy journal");

    let restored = World::load_from_dir(&dir).expect("restore from distfs sidecar");
    assert_eq!(restored.state(), world.state());
    let audit_value: serde_json::Value = serde_json::from_slice(
        &fs::read(dir.join("distfs.recovery.audit.json")).expect("read distfs audit"),
    )
    .expect("decode distfs audit");
    assert_eq!(
        audit_value.get("status").and_then(|value| value.as_str()),
        Some("distfs_restored")
    );
    assert!(audit_value.get("reason").is_none());
    assert!(
        audit_value
            .get("timestamp_ms")
            .and_then(|value| value.as_i64())
            .is_some()
    );

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn persist_splits_tick_consensus_records_into_hot_snapshot_and_archive() {
    let mut world = World::new();
    for _ in 0..140 {
        world.step().expect("step");
    }
    let full_snapshot = world.snapshot();
    let full_snapshot_json_len = full_snapshot
        .to_json()
        .expect("serialize full snapshot")
        .len();
    assert!(full_snapshot.tick_consensus_records.len() > 128);

    let dir = temp_dir("persist-tick-consensus-archive");
    world
        .save_to_dir(&dir)
        .expect("save world with tick archive");

    let persisted_snapshot: serde_json::Value = serde_json::from_slice(
        &fs::read(dir.join("snapshot.json")).expect("read persisted snapshot json"),
    )
    .expect("decode persisted snapshot json");
    let hot_records = persisted_snapshot
        .get("tick_consensus_records")
        .and_then(|value| value.as_array())
        .expect("hot tick consensus records");
    let total_record_count = persisted_snapshot
        .get("tick_consensus_total_record_count")
        .and_then(|value| value.as_u64())
        .expect("tick consensus total record count") as usize;
    let archived_record_count = persisted_snapshot
        .get("tick_consensus_archived_record_count")
        .and_then(|value| value.as_u64())
        .expect("tick consensus archived record count") as usize;
    assert_eq!(total_record_count, world.tick_consensus_records().len());
    assert_eq!(
        hot_records.len() + archived_record_count,
        total_record_count
    );
    assert!(hot_records.len() < total_record_count);
    let persisted_snapshot_json_len = fs::read(dir.join("snapshot.json"))
        .expect("read snapshot json bytes")
        .len();
    assert!(persisted_snapshot_json_len < full_snapshot_json_len);

    let archive_index: serde_json::Value = serde_json::from_slice(
        &fs::read(dir.join("tick-consensus.archive.index.json"))
            .expect("read tick consensus archive index json"),
    )
    .expect("decode tick consensus archive index json");
    let archived_segments = archive_index
        .get("archived_segments")
        .and_then(|value| value.as_array())
        .expect("archived tick consensus segments");
    let indexed_record_count = archived_segments
        .iter()
        .map(|segment| {
            segment
                .get("record_count")
                .and_then(|value| value.as_u64())
                .expect("segment record count") as usize
        })
        .sum::<usize>();
    assert_eq!(indexed_record_count, archived_record_count);
    assert!(dir.join("tick-consensus.archive.json").exists());
    assert!(dir.join("tick-consensus.archive.index.json").exists());
    assert!(dir.join("tick-consensus.archive.segments").exists());

    let restored = World::load_from_dir(&dir).expect("restore world with tick archive");
    assert_eq!(
        restored.tick_consensus_records(),
        world.tick_consensus_records()
    );
    restored
        .verify_tick_consensus_chain()
        .expect("verify restored tick consensus chain");

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn persist_writes_tick_consensus_archive_index_and_segments() {
    #[derive(serde::Serialize, serde::Deserialize)]
    struct ArchiveSegmentFile {
        records: Vec<TickConsensusRecord>,
    }

    let mut world = World::new();
    for _ in 0..260 {
        world.step().expect("step");
    }

    let dir = temp_dir("persist-tick-consensus-archive-index");
    world
        .save_to_dir(&dir)
        .expect("save world with archive index");

    let persisted_snapshot: serde_json::Value = serde_json::from_slice(
        &fs::read(dir.join("snapshot.json")).expect("read persisted snapshot json"),
    )
    .expect("decode persisted snapshot json");
    let archive_index: serde_json::Value = serde_json::from_slice(
        &fs::read(dir.join("tick-consensus.archive.index.json"))
            .expect("read tick consensus archive index json"),
    )
    .expect("decode tick consensus archive index json");
    assert_eq!(
        archive_index
            .get("hot_from_tick")
            .and_then(|value| value.as_u64()),
        persisted_snapshot
            .get("tick_consensus_hot_from_tick")
            .and_then(|value| value.as_u64())
    );
    assert_eq!(
        archive_index
            .get("hot_to_tick")
            .and_then(|value| value.as_u64()),
        persisted_snapshot
            .get("tick_consensus_hot_to_tick")
            .and_then(|value| value.as_u64())
    );
    let archived_segments = archive_index
        .get("archived_segments")
        .and_then(|value| value.as_array())
        .expect("archived tick consensus segments");
    assert!(archived_segments.len() >= 2);

    for segment in archived_segments {
        let relative_path = segment
            .get("relative_path")
            .and_then(|value| value.as_str())
            .expect("segment relative path");
        let from_tick = segment
            .get("from_tick")
            .and_then(|value| value.as_u64())
            .expect("segment from tick");
        let to_tick = segment
            .get("to_tick")
            .and_then(|value| value.as_u64())
            .expect("segment to tick");
        let record_count = segment
            .get("record_count")
            .and_then(|value| value.as_u64())
            .expect("segment record count") as usize;
        let expected_content_hash = segment
            .get("content_hash")
            .and_then(|value| value.as_str())
            .expect("segment content hash");
        let expected_anchor = segment
            .get("hash_chain_anchor")
            .and_then(|value| value.as_str())
            .expect("segment anchor");
        let segment_file: ArchiveSegmentFile = serde_json::from_slice(
            &fs::read(dir.join(relative_path)).expect("read archive segment file"),
        )
        .expect("decode archive segment file");
        assert_eq!(segment_file.records.len(), record_count);
        assert_eq!(
            segment_file
                .records
                .first()
                .map(|record| record.block.header.tick),
            Some(from_tick)
        );
        assert_eq!(
            segment_file
                .records
                .last()
                .map(|record| record.block.header.tick),
            Some(to_tick)
        );
        assert_eq!(
            util::hash_json(&segment_file).expect("hash archive segment file"),
            expected_content_hash
        );
        assert_eq!(
            segment_file
                .records
                .last()
                .map(|record| record.certificate.block_hash.as_str()),
            Some(expected_anchor)
        );
    }

    let restored = World::load_from_dir(&dir).expect("restore world with archive index");
    assert_eq!(
        restored.tick_consensus_records(),
        world.tick_consensus_records()
    );

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn persist_reads_tick_consensus_archive_range_and_verifies_archive() {
    let mut world = World::new();
    for _ in 0..260 {
        world.step().expect("step");
    }

    let dir = temp_dir("persist-tick-consensus-archive-range");
    world
        .save_to_dir(&dir)
        .expect("save world with archive range");

    let range_records = World::load_tick_consensus_records_from_dir(&dir, Some(64), Some(192))
        .expect("load tick consensus range from dir");
    let expected_records = world
        .tick_consensus_records()
        .iter()
        .filter(|record| {
            let tick = record.block.header.tick;
            (64..=192).contains(&tick)
        })
        .cloned()
        .collect::<Vec<_>>();
    assert_eq!(range_records, expected_records);
    World::verify_tick_consensus_archive_from_dir(&dir)
        .expect("verify tick consensus archive from dir");

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn persist_loads_legacy_tick_consensus_archive_without_index() {
    #[derive(serde::Serialize)]
    struct LegacyArchiveFile {
        archived_records: Vec<TickConsensusRecord>,
    }

    let mut world = World::new();
    for _ in 0..180 {
        world.step().expect("step");
    }

    let dir = temp_dir("persist-tick-consensus-legacy-archive");
    world
        .save_to_dir(&dir)
        .expect("save world with archive index");
    fs::remove_file(
        dir.join(".distfs-state")
            .join("sidecar-generations")
            .join("index.json"),
    )
    .expect("remove sidecar generation index");

    let archived_record_count = world.tick_consensus_records().len().saturating_sub(128);
    let legacy_archive = LegacyArchiveFile {
        archived_records: world.tick_consensus_records()[..archived_record_count].to_vec(),
    };
    fs::write(
        dir.join("tick-consensus.archive.json"),
        serde_json::to_vec_pretty(&legacy_archive).expect("encode legacy archive"),
    )
    .expect("write legacy archive");
    fs::remove_file(dir.join("tick-consensus.archive.index.json")).expect("remove archive index");
    fs::remove_dir_all(dir.join("tick-consensus.archive.segments"))
        .expect("remove archive segments");

    let restored = World::load_from_dir(&dir).expect("restore world from legacy archive");
    assert_eq!(
        restored.tick_consensus_records(),
        world.tick_consensus_records()
    );
    let loaded_records = World::load_tick_consensus_records_from_dir(&dir, None, None)
        .expect("load tick consensus records from legacy archive");
    assert_eq!(loaded_records, world.tick_consensus_records());

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn persist_uses_legacy_tick_consensus_archive_when_index_segment_is_missing() {
    let mut world = World::new();
    for _ in 0..260 {
        world.step().expect("step");
    }

    let dir = temp_dir("persist-tick-consensus-missing-segment-legacy-fallback");
    world
        .save_to_dir(&dir)
        .expect("save world with archive index");
    fs::remove_file(
        dir.join(".distfs-state")
            .join("sidecar-generations")
            .join("index.json"),
    )
    .expect("remove sidecar generation index");

    let archive_index: serde_json::Value = serde_json::from_slice(
        &fs::read(dir.join("tick-consensus.archive.index.json"))
            .expect("read tick consensus archive index json"),
    )
    .expect("decode tick consensus archive index json");
    let missing_segment_path = archive_index
        .get("archived_segments")
        .and_then(|value| value.as_array())
        .and_then(|segments| segments.first())
        .and_then(|segment| segment.get("relative_path"))
        .and_then(|value| value.as_str())
        .expect("first segment relative path");
    fs::remove_file(dir.join(missing_segment_path)).expect("remove archive segment");

    let restored = World::load_from_dir(&dir).expect("restore world from legacy archive");
    assert_eq!(
        restored.tick_consensus_records(),
        world.tick_consensus_records()
    );

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn persist_rejects_tampered_selected_tick_consensus_archive_hash() {
    let mut world = World::new();
    for _ in 0..260 {
        world.step().expect("step");
    }

    let dir = temp_dir("persist-tick-consensus-archive-tamper");
    world
        .save_to_dir(&dir)
        .expect("save world with archive index");

    let generation_index: serde_json::Value = serde_json::from_slice(
        &fs::read(
            dir.join(".distfs-state")
                .join("sidecar-generations")
                .join("index.json"),
        )
        .expect("read sidecar generation index"),
    )
    .expect("decode sidecar generation index");
    let latest_generation = generation_index["latest_generation"]
        .as_str()
        .expect("latest generation id");
    let archive_ref =
        generation_index["generations"][latest_generation]["tick_consensus_archive_ref"]
            .as_str()
            .expect("selected archive ref");
    let cas_path = dir
        .join(".distfs-state")
        .join("blobs")
        .join(format!("{archive_ref}.blob"));
    let mut archive_bytes = fs::read(&cas_path).expect("read selected archive blob");
    archive_bytes[0] ^= 0x01;
    fs::write(&cas_path, archive_bytes).expect("write tampered archive blob");

    let err = World::verify_tick_consensus_archive_from_dir(&dir)
        .expect_err("tampered archive segment should be rejected");
    assert!(matches!(
        err,
        WorldError::DistributedValidationFailed { .. }
    ));
    let WorldError::DistributedValidationFailed { reason } = err else {
        unreachable!("validated above");
    };
    assert!(
        reason.contains("content hash mismatch")
            || reason.contains("block hash mismatch")
            || reason.contains("no valid latest or rollback-safe generation"),
        "unexpected error reason: {reason}"
    );

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn persist_does_not_write_tick_consensus_archive_within_hot_limit() {
    let mut world = World::new();
    for _ in 0..8 {
        world.step().expect("step");
    }

    let dir = temp_dir("persist-tick-consensus-hot-only");
    world
        .save_to_dir(&dir)
        .expect("save world within hot limit");

    assert!(!dir.join("tick-consensus.archive.json").exists());
    let persisted_snapshot: serde_json::Value = serde_json::from_slice(
        &fs::read(dir.join("snapshot.json")).expect("read persisted snapshot json"),
    )
    .expect("decode persisted snapshot json");
    assert_eq!(
        persisted_snapshot
            .get("tick_consensus_archived_record_count")
            .and_then(|value| value.as_u64()),
        Some(0)
    );
    assert_eq!(
        persisted_snapshot
            .get("tick_consensus_total_record_count")
            .and_then(|value| value.as_u64()),
        Some(world.tick_consensus_records().len() as u64)
    );

    let restored = World::load_from_dir(&dir).expect("restore world within hot limit");
    assert_eq!(
        restored.tick_consensus_records(),
        world.tick_consensus_records()
    );

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn persist_writes_sidecar_generation_index_and_pinset() {
    let mut world = World::new();
    world.submit_action(Action::RegisterAgent {
        agent_id: "agent-sidecar".to_string(),
        pos: pos(0, 0),
    });
    world.step().expect("step");

    let dir = temp_dir("persist-sidecar-generation-index");
    world
        .save_to_dir(&dir)
        .expect("save world with sidecar generation index");

    let index_path = dir.join(".distfs-state/sidecar-generations/index.json");
    let index: serde_json::Value = serde_json::from_slice(
        &fs::read(index_path.as_path()).expect("read sidecar generation index"),
    )
    .expect("decode sidecar generation index");
    let latest_generation = index
        .get("latest_generation")
        .and_then(|value| value.as_str())
        .expect("latest generation");
    assert!(index.get("rollback_safe_generation").is_none());
    assert!(
        dir.join(".distfs-state/sidecar-generations/generation.tmp")
            .exists()
    );
    assert!(
        dir.join(format!(
            ".distfs-state/sidecar-generations/generations/{latest_generation}.json"
        ))
        .exists()
    );

    let generation = index
        .get("generations")
        .and_then(|value| value.get(latest_generation))
        .expect("latest generation entry");
    let pinned_blob_hashes = generation
        .get("pinned_blob_hashes")
        .and_then(|value| value.as_array())
        .expect("pinned blob hashes");
    assert!(!pinned_blob_hashes.is_empty());
    let manifest: oasis7_proto::distributed::SnapshotManifest = serde_json::from_slice(
        &fs::read(dir.join("snapshot.manifest.json")).expect("read snapshot manifest"),
    )
    .expect("decode snapshot manifest");
    let journal_segments: Vec<oasis7_proto::distributed_storage::JournalSegmentRef> =
        serde_json::from_slice(
            &fs::read(dir.join("journal.segments.json")).expect("read journal segments"),
        )
        .expect("decode journal segments");
    let expected_pins = manifest
        .chunks
        .iter()
        .map(|chunk| chunk.content_hash.clone())
        .chain(
            journal_segments
                .iter()
                .map(|segment| segment.content_hash.clone()),
        )
        .collect::<std::collections::BTreeSet<_>>();
    let actual_pins = pinned_blob_hashes
        .iter()
        .map(|value| value.as_str().expect("pin string").to_string())
        .collect::<std::collections::BTreeSet<_>>();
    assert_eq!(actual_pins, expected_pins);

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn persist_sidecar_generation_record_points_to_generation_local_payloads() {
    let mut world = World::new();
    world.submit_action(Action::RegisterAgent {
        agent_id: "agent-sidecar-local-payload".to_string(),
        pos: pos(0, 0),
    });
    world.step().expect("step");

    let dir = temp_dir("persist-sidecar-generation-local-payload");
    world.save_to_dir(&dir).expect("save world");

    let index: serde_json::Value = serde_json::from_slice(
        &fs::read(dir.join(".distfs-state/sidecar-generations/index.json"))
            .expect("read sidecar generation index"),
    )
    .expect("decode sidecar generation index");
    let latest_generation = index
        .get("latest_generation")
        .and_then(|value| value.as_str())
        .expect("latest generation");
    let generation_record: serde_json::Value = serde_json::from_slice(
        &fs::read(dir.join(format!(
            ".distfs-state/sidecar-generations/generations/{latest_generation}.json"
        )))
        .expect("read sidecar generation record"),
    )
    .expect("decode sidecar generation record");
    let snapshot_manifest_path = generation_record
        .get("snapshot_manifest_path")
        .and_then(|value| value.as_str())
        .expect("snapshot manifest path");
    let journal_segments_path = generation_record
        .get("journal_segments_path")
        .and_then(|value| value.as_str())
        .expect("journal segments path");
    assert!(snapshot_manifest_path.contains(&format!("payloads/{latest_generation}/")));
    assert!(journal_segments_path.contains(&format!("payloads/{latest_generation}/")));
    assert!(
        dir.join(".distfs-state")
            .join(snapshot_manifest_path)
            .exists()
    );
    assert!(
        dir.join(".distfs-state")
            .join(journal_segments_path)
            .exists()
    );

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn persist_sidecar_generation_switch_keeps_latest_and_rollback_safe_only() {
    let mut world = World::new();
    let dir = temp_dir("persist-sidecar-generation-keep-two");

    for step_index in 0..3 {
        world.submit_action(Action::RegisterAgent {
            agent_id: format!("agent-sidecar-keep-{step_index}"),
            pos: pos(step_index as i64, step_index as i64),
        });
        world.step().expect("step before save");
        world.save_to_dir(&dir).expect("save world");
    }

    let index: serde_json::Value = serde_json::from_slice(
        &fs::read(dir.join(".distfs-state/sidecar-generations/index.json"))
            .expect("read sidecar generation index"),
    )
    .expect("decode sidecar generation index");
    let latest_generation = index
        .get("latest_generation")
        .and_then(|value| value.as_str())
        .expect("latest generation")
        .to_string();
    let rollback_safe_generation = index
        .get("rollback_safe_generation")
        .and_then(|value| value.as_str())
        .expect("rollback safe generation")
        .to_string();
    let generations = index
        .get("generations")
        .and_then(|value| value.as_object())
        .expect("generation map");
    assert_eq!(generations.len(), 2);
    assert!(generations.contains_key(latest_generation.as_str()));
    assert!(generations.contains_key(rollback_safe_generation.as_str()));
    assert!(
        dir.join(format!(
            ".distfs-state/sidecar-generations/payloads/{latest_generation}"
        ))
        .exists()
    );
    assert!(
        dir.join(format!(
            ".distfs-state/sidecar-generations/payloads/{rollback_safe_generation}"
        ))
        .exists()
    );
    let staging_entries =
        fs::read_dir(dir.join(".distfs-state/sidecar-generations/generation.tmp"))
            .expect("read staging dir")
            .count();
    assert_eq!(staging_entries, 0);

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn persist_sidecar_generation_sweep_keeps_only_retained_blobs() {
    let mut world = World::new();
    let dir = temp_dir("persist-sidecar-generation-sweep");

    for step_index in 0..3 {
        world.submit_action(Action::RegisterAgent {
            agent_id: format!("agent-sidecar-sweep-{step_index}"),
            pos: pos(step_index as i64, step_index as i64),
        });
        world.step().expect("step before save");
        world.save_to_dir(&dir).expect("save world");
    }

    let index: serde_json::Value = serde_json::from_slice(
        &fs::read(dir.join(".distfs-state/sidecar-generations/index.json"))
            .expect("read sidecar generation index"),
    )
    .expect("decode sidecar generation index");
    let latest_generation = index
        .get("latest_generation")
        .and_then(|value| value.as_str())
        .expect("latest generation")
        .to_string();
    let rollback_safe_generation = index
        .get("rollback_safe_generation")
        .and_then(|value| value.as_str())
        .expect("rollback safe generation")
        .to_string();
    let generations = index
        .get("generations")
        .and_then(|value| value.as_object())
        .expect("generation map");
    let retained_blob_hashes = [
        latest_generation.as_str(),
        rollback_safe_generation.as_str(),
    ]
    .into_iter()
    .flat_map(|generation_id| {
        generations
            .get(generation_id)
            .and_then(|value| value.get("pinned_blob_hashes"))
            .and_then(|value| value.as_array())
            .expect("generation pinned blob hashes")
            .iter()
            .map(|value| value.as_str().expect("pin string").to_string())
            .collect::<Vec<_>>()
    })
    .collect::<std::collections::BTreeSet<_>>();
    let actual_blob_hashes = LocalCasStore::new(dir.join(".distfs-state"))
        .list_blob_hashes()
        .expect("list blob hashes")
        .into_iter()
        .collect::<std::collections::BTreeSet<_>>();
    assert_eq!(actual_blob_hashes, retained_blob_hashes);
    assert_eq!(
        index
            .get("last_gc_result")
            .and_then(|value| value.get("status"))
            .and_then(|value| value.as_str()),
        Some("success")
    );
    assert!(
        index
            .get("last_gc_result")
            .and_then(|value| value.get("error"))
            .is_none()
    );

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn persist_sidecar_generation_gc_failure_preserves_latest_and_rollback_blobs() {
    let mut world = World::new();
    let dir = temp_dir("persist-sidecar-generation-gc-failure");

    for step_index in 0..2 {
        world.submit_action(Action::RegisterAgent {
            agent_id: format!("agent-sidecar-gc-failure-{step_index}"),
            pos: pos(step_index as i64, step_index as i64),
        });
        world.step().expect("step before save");
        world.save_to_dir(&dir).expect("save world");
    }

    let second_index: serde_json::Value = serde_json::from_slice(
        &fs::read(dir.join(".distfs-state/sidecar-generations/index.json"))
            .expect("read second sidecar generation index"),
    )
    .expect("decode second sidecar generation index");
    let second_latest_generation = second_index
        .get("latest_generation")
        .and_then(|value| value.as_str())
        .expect("second latest generation")
        .to_string();
    fs::write(
        dir.join(format!(
            ".distfs-state/sidecar-generations/payloads/{second_latest_generation}/journal.segments.json"
        )),
        b"not-json",
    )
    .expect("corrupt rollback-safe generation payload");

    world.submit_action(Action::RegisterAgent {
        agent_id: "agent-sidecar-gc-failure-2".to_string(),
        pos: pos(2, 2),
    });
    world.step().expect("third step before save");
    world
        .save_to_dir(&dir)
        .expect("save world should degrade instead of failing");

    let third_index: serde_json::Value = serde_json::from_slice(
        &fs::read(dir.join(".distfs-state/sidecar-generations/index.json"))
            .expect("read third sidecar generation index"),
    )
    .expect("decode third sidecar generation index");
    let latest_generation = third_index
        .get("latest_generation")
        .and_then(|value| value.as_str())
        .expect("latest generation")
        .to_string();
    let rollback_safe_generation = third_index
        .get("rollback_safe_generation")
        .and_then(|value| value.as_str())
        .expect("rollback safe generation")
        .to_string();
    assert_eq!(rollback_safe_generation, second_latest_generation);
    assert_eq!(
        third_index
            .get("last_gc_result")
            .and_then(|value| value.get("status"))
            .and_then(|value| value.as_str()),
        Some("failed")
    );
    assert!(
        third_index
            .get("last_gc_result")
            .and_then(|value| value.get("error"))
            .and_then(|value| value.as_str())
            .is_some()
    );

    let generations = third_index
        .get("generations")
        .and_then(|value| value.as_object())
        .expect("generation map");
    let retained_blob_hashes = [
        latest_generation.as_str(),
        rollback_safe_generation.as_str(),
    ]
    .into_iter()
    .flat_map(|generation_id| {
        generations
            .get(generation_id)
            .and_then(|value| value.get("pinned_blob_hashes"))
            .and_then(|value| value.as_array())
            .expect("generation pinned blob hashes")
            .iter()
            .map(|value| value.as_str().expect("pin string").to_string())
            .collect::<Vec<_>>()
    })
    .collect::<std::collections::BTreeSet<_>>();
    let actual_blob_hashes = LocalCasStore::new(dir.join(".distfs-state"))
        .list_blob_hashes()
        .expect("list blob hashes")
        .into_iter()
        .collect::<std::collections::BTreeSet<_>>();
    assert!(retained_blob_hashes.is_subset(&actual_blob_hashes));

    let restored =
        World::load_from_dir(&dir).expect("restore from latest generation after gc failure");
    assert_eq!(restored.state(), world.state());

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn persist_sidecar_generation_interrupted_save_rolls_back_and_retry_cleans_staging() {
    let mut world = World::new();
    world.submit_action(Action::RegisterAgent {
        agent_id: "agent-sidecar-interrupt-0".to_string(),
        pos: pos(0, 0),
    });
    world.step().expect("first step");

    let dir = temp_dir("persist-sidecar-generation-interrupt");
    world.save_to_dir(&dir).expect("first save");
    let first_restored = World::load_from_dir(&dir).expect("load first saved world");
    let first_state = first_restored.state().clone();
    let first_index: serde_json::Value = serde_json::from_slice(
        &fs::read(dir.join(".distfs-state/sidecar-generations/index.json"))
            .expect("read first sidecar generation index"),
    )
    .expect("decode first sidecar generation index");
    let first_latest_generation = first_index
        .get("latest_generation")
        .and_then(|value| value.as_str())
        .expect("first latest generation")
        .to_string();

    fs::write(
        dir.join(".distfs-state/sidecar-generations/.test-fail-after-stage"),
        b"1",
    )
    .expect("install sidecar failpoint");
    world.submit_action(Action::RegisterAgent {
        agent_id: "agent-sidecar-interrupt-1".to_string(),
        pos: pos(1, 1),
    });
    world.step().expect("second step");
    let err = world
        .save_to_dir(&dir)
        .expect_err("interrupted save should fail after staging");
    assert!(matches!(
        err,
        WorldError::DistributedValidationFailed { .. }
    ));
    assert!(
        fs::read_dir(dir.join(".distfs-state/sidecar-generations/generation.tmp"))
            .expect("read staging dir after interrupted save")
            .count()
            > 0
    );

    let rolled_back = World::load_from_dir(&dir).expect("load after interrupted save");
    assert_eq!(rolled_back.state(), &first_state);
    let interrupted_index: serde_json::Value = serde_json::from_slice(
        &fs::read(dir.join(".distfs-state/sidecar-generations/index.json"))
            .expect("read interrupted sidecar generation index"),
    )
    .expect("decode interrupted sidecar generation index");
    assert_eq!(
        interrupted_index
            .get("latest_generation")
            .and_then(|value| value.as_str()),
        Some(first_latest_generation.as_str())
    );

    fs::remove_file(dir.join(".distfs-state/sidecar-generations/.test-fail-after-stage"))
        .expect("remove sidecar failpoint");
    world
        .save_to_dir(&dir)
        .expect("retry save after interruption");

    let staging_entries =
        fs::read_dir(dir.join(".distfs-state/sidecar-generations/generation.tmp"))
            .expect("read staging dir after retry")
            .count();
    assert_eq!(staging_entries, 0);
    let restored = World::load_from_dir(&dir).expect("load after retry save");
    assert_eq!(restored.state(), world.state());

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn persist_sidecar_generation_retry_cleans_partial_staging_and_orphan_blob() {
    let mut world = World::new();
    world.submit_action(Action::RegisterAgent {
        agent_id: "agent-sidecar-partial-0".to_string(),
        pos: pos(0, 0),
    });
    world.step().expect("first step");

    let dir = temp_dir("persist-sidecar-generation-partial-staging");
    world.save_to_dir(&dir).expect("first save");

    let partial_staging_dir =
        dir.join(".distfs-state/sidecar-generations/generation.tmp/interrupted-partial");
    fs::create_dir_all(partial_staging_dir.as_path()).expect("create partial staging dir");
    fs::write(
        partial_staging_dir.join("snapshot.manifest.json"),
        br#"{"partial""#,
    )
    .expect("write partial snapshot manifest");
    fs::write(
        partial_staging_dir.join("journal.segments.json"),
        br#"[{"partial""#,
    )
    .expect("write partial journal segments");
    fs::write(partial_staging_dir.join("generation.json"), b"not-json")
        .expect("write partial generation record");

    let orphan_bytes = b"sidecar-orphan-after-interrupt";
    let orphan_hash = oasis7_distfs::blake3_hex(orphan_bytes);
    let orphan_blob_path = dir.join(format!(".distfs-state/blobs/{orphan_hash}.blob"));
    fs::write(orphan_blob_path.as_path(), orphan_bytes).expect("write orphan blob");

    world.submit_action(Action::RegisterAgent {
        agent_id: "agent-sidecar-partial-1".to_string(),
        pos: pos(1, 1),
    });
    world.step().expect("second step");
    world
        .save_to_dir(&dir)
        .expect("retry save after partial staging");

    let staging_entries =
        fs::read_dir(dir.join(".distfs-state/sidecar-generations/generation.tmp"))
            .expect("read staging dir after cleanup")
            .count();
    assert_eq!(staging_entries, 0);
    assert!(!orphan_blob_path.exists());
    let restored = World::load_from_dir(&dir).expect("load after cleanup save");
    assert_eq!(restored.state(), world.state());

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn persist_updates_sidecar_generation_index_with_rollback_safe_generation() {
    let mut world = World::new();
    world.submit_action(Action::RegisterAgent {
        agent_id: "agent-sidecar-2".to_string(),
        pos: pos(0, 0),
    });
    world.step().expect("first step");

    let dir = temp_dir("persist-sidecar-generation-rollback-safe");
    world.save_to_dir(&dir).expect("first save");

    let first_index: serde_json::Value = serde_json::from_slice(
        &fs::read(dir.join(".distfs-state/sidecar-generations/index.json"))
            .expect("read first sidecar generation index"),
    )
    .expect("decode first sidecar generation index");
    let first_latest = first_index
        .get("latest_generation")
        .and_then(|value| value.as_str())
        .expect("first latest generation")
        .to_string();

    world.submit_action(Action::RegisterAgent {
        agent_id: "agent-sidecar-3".to_string(),
        pos: pos(1, 1),
    });
    world.step().expect("second step");
    world.save_to_dir(&dir).expect("second save");

    let second_index: serde_json::Value = serde_json::from_slice(
        &fs::read(dir.join(".distfs-state/sidecar-generations/index.json"))
            .expect("read second sidecar generation index"),
    )
    .expect("decode second sidecar generation index");
    let latest_generation = second_index
        .get("latest_generation")
        .and_then(|value| value.as_str())
        .expect("latest generation");
    let rollback_safe_generation = second_index
        .get("rollback_safe_generation")
        .and_then(|value| value.as_str())
        .expect("rollback safe generation");
    assert_ne!(latest_generation, rollback_safe_generation);
    assert_eq!(rollback_safe_generation, first_latest);
    assert!(
        second_index
            .get("generations")
            .and_then(|value| value.get(latest_generation))
            .is_some()
    );
    assert!(
        second_index
            .get("generations")
            .and_then(|value| value.get(rollback_safe_generation))
            .is_some()
    );

    let _ = fs::remove_dir_all(&dir);
}
