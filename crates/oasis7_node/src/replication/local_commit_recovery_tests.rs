use super::*;

fn decision(height: u64) -> PosDecision {
    PosDecision {
        height,
        slot: height,
        epoch: 0,
        proposer_id: "node-a".into(),
        status: PosConsensusStatus::Committed,
        block_hash: format!("block-{height}"),
        action_root: empty_action_root(),
        committed_actions: vec![],
        approved_stake: 100,
        rejected_stake: 0,
        required_stake: 67,
        total_stake: 100,
    }
}
fn fixture() -> (PathBuf, NodeReplicationConfig, GossipReplicationMessage) {
    fixture_with_proposer("node-a")
}
fn fixture_with_proposer(
    proposer: &str,
) -> (PathBuf, NodeReplicationConfig, GossipReplicationMessage) {
    static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let unique = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let dir = temp_dir(&format!("local-commit-recovery-{unique}"));
    let (private, public) = deterministic_keypair_hex(81);
    let config = NodeReplicationConfig::new(&dir)
        .unwrap()
        .with_signing_keypair(private, public)
        .unwrap();
    let mut runtime = ReplicationRuntime::new(&config, "node-a").unwrap();
    let mut first = decision(1);
    first.proposer_id = proposer.into();
    let mut second = decision(2);
    second.proposer_id = proposer.into();
    runtime
        .build_local_commit_message("node-a", "world-a", 1000, &first, None, None)
        .unwrap();
    let message = runtime
        .build_local_commit_message("node-a", "world-a", 2000, &second, None, None)
        .unwrap()
        .unwrap();
    (dir, config, message)
}
#[test]
fn legacy_local_commit_gap_restores_exact_original_without_advancing_sequence() {
    let (dir, config, expected) = fixture();
    fs::remove_file(config.commit_message_path(2)).unwrap();
    let index = commit_retention::latest_commit_head_index_path_from_root(&dir, "world-a");
    fs::remove_file(index).unwrap();
    // Legacy crash has no future complete-message outbox.
    let outbox = dir.join("replication_local_commit_node-a.json");
    if outbox.exists() {
        fs::remove_file(outbox).unwrap();
    }
    let runtime = ReplicationRuntime::new(&config, "node-a").unwrap();
    let restored = load_commit_message_from_root(&dir, "world-a", 2)
        .unwrap()
        .expect("missing signed original must recover");
    assert_eq!(
        serde_json::to_vec(&restored).unwrap(),
        serde_json::to_vec(&expected).unwrap()
    );
    assert_eq!(runtime.writer_state.last_sequence, expected.record.sequence);
    assert_eq!(
        load_latest_commit_head_index_height_from_root(&dir, "world-a").unwrap(),
        Some(2)
    );
    drop(runtime);
    fs::remove_dir_all(dir).unwrap();
}

fn remove_signed_tail(dir: &Path, config: &NodeReplicationConfig) {
    fs::remove_file(config.commit_message_path(2)).unwrap();
    let outbox = dir.join("replication_local_commit_node-a.json");
    if outbox.exists() {
        fs::remove_file(outbox).unwrap();
    }
}
#[test]
fn signed_original_recovers_both_sides_of_guard_publication() {
    for applied in [false, true] {
        let (dir, config, expected) = fixture();
        remove_signed_tail(&dir, &config);
        let mut runtime = ReplicationRuntime::from_config(&config, "node-a").unwrap();
        runtime
            .stage_local_commit_original("node-a", &expected)
            .unwrap();
        if !applied {
            // Inject the crash before CAS/guard publication, preserving original signed intent.
            runtime.guard.last_sequence = 1;
            runtime.writer_state.last_sequence = 1;
            runtime.writer_state.last_replicated_height = 1;
            runtime.persist_state("node-a").unwrap();
            let index_path = config.store_root().join("files_index.json");
            let mut index: serde_json::Value =
                serde_json::from_slice(&fs::read(&index_path).unwrap()).unwrap();
            index["files"]
                .as_object_mut()
                .unwrap()
                .remove(&expected.record.path);
            fs::write(index_path, serde_json::to_vec(&index).unwrap()).unwrap();
        }
        drop(runtime);
        let restored = ReplicationRuntime::new(&config, "node-a").unwrap();
        let message = load_commit_message_from_root(&dir, "world-a", 2)
            .unwrap()
            .unwrap();
        assert_eq!(message, expected);
        assert_eq!(restored.guard.last_sequence, expected.record.sequence);
        assert_eq!(restored.writer_state.last_replicated_height, 2);
        drop(restored);
        let again = ReplicationRuntime::new(&config, "node-a").unwrap();
        assert_eq!(again.writer_state.last_sequence, 2);
        drop(again);
        fs::remove_dir_all(dir).unwrap();
    }
}
#[test]
fn legacy_gap_rejects_non_unique_sequence_and_unfinished_old_amendment() {
    for ambiguous_sequence in [false, true] {
        let (dir, config, _) = fixture();
        remove_signed_tail(&dir, &config);
        let mut runtime = ReplicationRuntime::from_config(&config, "node-a").unwrap();
        if ambiguous_sequence {
            runtime.guard.last_sequence = 3;
            runtime.writer_state.last_sequence = 3;
            runtime.persist_state("node-a").unwrap();
        } else {
            runtime
                .store
                .write_file(
                    "consensus/commits/00000000000000000001.json",
                    b"unfinished old amendment",
                )
                .unwrap();
        }
        drop(runtime);
        assert!(ReplicationRuntime::new(&config, "node-a").is_err());
        assert!(!config.commit_message_path(2).exists());
        fs::remove_dir_all(dir).unwrap();
    }
}
#[test]
fn legacy_gap_rejects_foreign_world_payload_and_wrong_signer() {
    for foreign_world in [false, true] {
        let (dir, config, _) = fixture();
        remove_signed_tail(&dir, &config);
        let runtime = ReplicationRuntime::from_config(&config, "node-a").unwrap();
        let mut restart_config = config.clone();
        if foreign_world {
            let path = "consensus/commits/00000000000000000002.json";
            let mut p: ReplicatedCommitPayload =
                serde_json::from_slice(&runtime.store.read_file(path).unwrap()).unwrap();
            p.world_id = "foreign-world".into();
            runtime
                .store
                .write_file(path, &serde_json::to_vec(&p).unwrap())
                .unwrap();
        } else {
            let (private, public) = deterministic_keypair_hex(82);
            restart_config = restart_config
                .with_signing_keypair(private, public)
                .unwrap();
        }
        drop(runtime);
        assert!(ReplicationRuntime::new(&restart_config, "node-a").is_err());
        assert!(!config.commit_message_path(2).exists());
        fs::remove_dir_all(dir).unwrap();
    }
}
#[test]
fn pending_original_rejects_tampering_and_conflicting_signed_height() {
    for conflict in [false, true] {
        let (dir, config, expected) = fixture();
        let runtime = ReplicationRuntime::from_config(&config, "node-a").unwrap();
        runtime
            .stage_local_commit_original("node-a", &expected)
            .unwrap();
        if conflict {
            let mut other = expected.clone();
            other.payload.push(b' ');
            fs::write(
                config.commit_message_path(2),
                serde_json::to_vec(&other).unwrap(),
            )
            .unwrap();
        } else {
            let outbox = runtime.local_commit_original_path("node-a");
            let mut value: serde_json::Value =
                serde_json::from_slice(&fs::read(&outbox).unwrap()).unwrap();
            value["message"]["record"]["sequence"] = 99.into();
            fs::write(outbox, serde_json::to_vec(&value).unwrap()).unwrap();
        }
        drop(runtime);
        assert!(ReplicationRuntime::new(&config, "node-a").is_err());
        fs::remove_dir_all(dir).unwrap();
    }
}

#[test]
fn first_signed_original_preserves_seeded_epoch_before_any_writer_file() {
    let (dir, config, _) = fixture();
    let expected = load_commit_message_from_root(&dir, "world-a", 1)
        .unwrap()
        .unwrap();
    let runtime = ReplicationRuntime::from_config(&config, "node-a").unwrap();
    runtime
        .stage_local_commit_original("node-a", &expected)
        .unwrap();
    drop(runtime);
    for name in [
        "replication_guard.json",
        "replication_writer_state_node-a.json",
        "replication_commit_worlds.json",
    ] {
        let path = dir.join(name);
        if path.exists() {
            fs::remove_file(path).unwrap();
        }
    }
    for name in ["replication_commit_messages", "replication_commit_heads"] {
        fs::remove_dir_all(dir.join(name)).unwrap();
    }
    let store = LocalCasStore::new(config.store_root());
    let index = config.store_root().join("files_index.json");
    let mut value: serde_json::Value = serde_json::from_slice(&fs::read(&index).unwrap()).unwrap();
    value["files"] = serde_json::json!({});
    fs::write(index, serde_json::to_vec(&value).unwrap()).unwrap();
    drop(store);
    let restored = ReplicationRuntime::new(&config, "node-a").unwrap();
    assert_eq!(
        restored.writer_state.writer_epoch,
        expected.record.writer_epoch
    );
    assert_eq!(
        load_commit_message_from_root(&dir, "world-a", 1)
            .unwrap()
            .unwrap(),
        expected
    );
    drop(restored);
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn pending_original_checks_cas_even_when_signed_message_and_writer_state_exist() {
    for missing in [false, true] {
        let (dir, config, expected) = fixture();
        let runtime = ReplicationRuntime::from_config(&config, "node-a").unwrap();
        runtime
            .stage_local_commit_original("node-a", &expected)
            .unwrap();
        if missing {
            let path = config.store_root().join("files_index.json");
            let mut index: serde_json::Value =
                serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
            index["files"]
                .as_object_mut()
                .unwrap()
                .remove(&expected.record.path);
            fs::write(path, serde_json::to_vec(&index).unwrap()).unwrap();
        } else {
            runtime
                .store
                .write_file(&expected.record.path, b"conflicting CAS")
                .unwrap();
        }
        drop(runtime);
        assert!(ReplicationRuntime::new(&config, "node-a").is_err());
        fs::remove_dir_all(dir).unwrap();
    }
}

#[test]
fn first_original_cannot_lower_an_explicit_zero_counter_epoch() {
    let (dir, config, _) = fixture();
    let expected = load_commit_message_from_root(&dir, "world-a", 1)
        .unwrap()
        .unwrap();
    let mut runtime = ReplicationRuntime::from_config(&config, "node-a").unwrap();
    runtime
        .stage_local_commit_original("node-a", &expected)
        .unwrap();
    runtime.guard = SingleWriterReplicationGuard::default();
    runtime.writer_state.writer_epoch = expected.record.writer_epoch + 1;
    runtime.writer_state.last_sequence = 0;
    runtime.writer_state.last_replicated_height = 0;
    runtime.persist_state("node-a").unwrap();
    let before = fs::read(config.writer_state_path("node-a")).unwrap();
    drop(runtime);
    // Remove the signed original so recovery cannot use the completed-message path.
    fs::remove_file(config.commit_message_path(1)).unwrap();
    fs::remove_file(config.commit_message_path(2)).unwrap();
    fs::remove_dir_all(dir.join("replication_commit_heads")).unwrap();
    let error = ReplicationRuntime::new(&config, "node-a")
        .map_or_else(|e| e, |_| panic!("explicit epoch must reject"));
    assert!(
        matches!(error,NodeError::Replication { reason } if reason.contains("original is not the exact next writer record"))
    );
    assert_eq!(
        before,
        fs::read(config.writer_state_path("node-a")).unwrap()
    );
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn legacy_storage_writer_keeps_the_original_external_proposer() {
    let (dir, config, expected) = fixture_with_proposer("other-proposer");
    remove_signed_tail(&dir, &config);
    let restored = ReplicationRuntime::new(&config, "node-a").unwrap();
    assert_eq!(
        load_commit_message_from_root(&dir, "world-a", 2)
            .unwrap()
            .unwrap(),
        expected
    );
    drop(restored);
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn pending_original_never_overwrites_conflicting_cas_before_guard_publication() {
    let (dir, config, expected) = fixture();
    remove_signed_tail(&dir, &config);
    let mut runtime = ReplicationRuntime::from_config(&config, "node-a").unwrap();
    runtime
        .stage_local_commit_original("node-a", &expected)
        .unwrap();
    runtime.guard.last_sequence = 1;
    runtime.writer_state.last_sequence = 1;
    runtime.writer_state.last_replicated_height = 1;
    runtime.persist_state("node-a").unwrap();
    runtime
        .store
        .write_file(&expected.record.path, b"conflicting before guard")
        .unwrap();
    let before = fs::read(config.guard_state_path()).unwrap();
    drop(runtime);
    assert!(ReplicationRuntime::new(&config, "node-a").is_err());
    assert_eq!(before, fs::read(config.guard_state_path()).unwrap());
    let store = LocalCasStore::new(config.store_root());
    assert_eq!(
        store.read_file(&expected.record.path).unwrap(),
        b"conflicting before guard"
    );
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn legacy_gap_rejects_foreign_writer_history_even_with_identical_cas_payload() {
    let (dir, config, _) = fixture();
    remove_signed_tail(&dir, &config);
    let mut foreign = load_commit_message_from_root(&dir, "world-a", 1)
        .unwrap()
        .unwrap();
    let (private, public) = deterministic_keypair_hex(83);
    let signer = ReplicationSigningKey {
        signing_key: signing_key_from_hex(&private).unwrap(),
        public_key_hex: public.clone(),
    };
    foreign.record.writer_id = public.clone();
    foreign.public_key_hex = Some(public);
    foreign.signature_hex = Some(sign_replication_message(&foreign, &signer).unwrap());
    fs::write(
        config.commit_message_path(1),
        serde_json::to_vec(&foreign).unwrap(),
    )
    .unwrap();
    let error = ReplicationRuntime::new(&config, "node-a")
        .map_or_else(|e| e, |_| panic!("foreign writer ambiguity must reject"));
    assert!(matches!(error,NodeError::Replication { reason } if reason.contains("foreign writer")));
    assert!(!config.commit_message_path(2).exists());
    fs::remove_dir_all(dir).unwrap();
}

struct ReplayHook {
    restored: Vec<u64>,
    replayed: Vec<u64>,
}
impl crate::NodeExecutionHook for ReplayHook {
    fn on_commit(
        &mut self,
        c: crate::NodeExecutionCommitContext,
    ) -> Result<crate::NodeExecutionCommitResult, String> {
        assert_eq!(c.committed_at_unix_ms, 1000 * c.height as i64);
        self.replayed.push(c.height);
        Ok(crate::NodeExecutionCommitResult {
            execution_height: c.height,
            execution_block_hash: format!("exec-block-{}", c.height),
            execution_state_root: format!("exec-state-{}", c.height),
        })
    }
    fn restore_to_height(&mut self, _: &str, h: u64) -> Result<bool, String> {
        self.restored.push(h);
        Ok(true)
    }
    fn restore_to_verified_commit(
        &mut self,
        c: &crate::NodeExecutionCommitContext,
        e: &crate::NodeExecutionCommitResult,
    ) -> Result<bool, String> {
        assert_eq!(c.node_block_hash, format!("block-{}", c.height));
        assert_eq!(e.execution_state_root, format!("exec-state-{}", c.height));
        self.restored.push(c.height);
        Ok(true)
    }
}
fn same_height_fixture(recovered: bool) -> (PathBuf, ReplicationRuntime, crate::PosNodeEngine) {
    static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let dir = temp_dir(&format!(
        "same-height-publication-replay-{}",
        NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    ));
    let (private, public) = deterministic_keypair_hex(84);
    let rc = NodeReplicationConfig::new(&dir)
        .unwrap()
        .with_signing_keypair(private, public)
        .unwrap();
    let mut replication = ReplicationRuntime::new(&rc, "node-a").unwrap();
    for h in 1..=2 {
        replication
            .build_local_commit_message(
                "node-a",
                "world-a",
                1000 * h as i64,
                &decision(h),
                Some(&format!("exec-block-{h}")),
                Some(&format!("exec-state-{h}")),
            )
            .unwrap();
    }
    drop(replication);
    if recovered {
        remove_signed_tail(&dir, &rc);
    }
    let replication = ReplicationRuntime::new(&rc, "node-a").unwrap();
    let pos = crate::NodePosConfig::ethereum_like(vec![crate::PosValidator {
        validator_id: "node-a".into(),
        stake: 100,
    }])
    .with_validator_signer_public_keys(BTreeMap::from([(
        "node-a".into(),
        deterministic_keypair_hex(84).1,
    )]))
    .unwrap();
    let cfg = crate::NodeConfig::new("node-a", "world-a", crate::NodeRole::Sequencer)
        .unwrap()
        .with_pos_config(pos)
        .unwrap()
        .with_replication(rc);
    let mut engine = crate::PosNodeEngine::new(&cfg).unwrap();
    engine.committed_height = 2;
    engine.network_committed_height = 2;
    engine.next_height = 3;
    engine.last_committed_block_hash = Some("unreplicated-reexecution".into());
    engine.last_execution_height = 2;
    engine.last_execution_block_hash = Some("wrong-exec".into());
    engine.last_execution_state_root = Some("wrong-root".into());
    (dir, replication, engine)
}
#[test]
fn recovered_publication_replays_same_height_conflict_from_verified_predecessor() {
    for execution_only in [false, true] {
        let (dir, replication, mut engine) = same_height_fixture(true);
        if execution_only {
            engine.last_committed_block_hash = Some("block-2".into());
        }
        let mut hook = ReplayHook {
            restored: vec![],
            replayed: vec![],
        };
        crate::replication_state_reconcile::reconcile_engine_with_persisted_replication(
            &mut engine,
            &replication,
            "world-a",
            Some(&mut hook),
        )
        .unwrap();
        assert_eq!(engine.last_committed_block_hash.as_deref(), Some("block-2"));
        assert_eq!(
            engine.last_execution_state_root.as_deref(),
            Some("exec-state-2")
        );
        assert_eq!(hook.restored, vec![1]);
        assert_eq!(hook.replayed, vec![2]);
        drop(replication);
        fs::remove_dir_all(dir).unwrap();
    }
}
#[test]
fn same_height_conflict_without_local_recovery_proof_remains_rejected() {
    let (dir, replication, mut engine) = same_height_fixture(false);
    let mut hook = ReplayHook {
        restored: vec![],
        replayed: vec![],
    };
    let error = crate::replication_state_reconcile::reconcile_engine_with_persisted_replication(
        &mut engine,
        &replication,
        "world-a",
        Some(&mut hook),
    )
    .unwrap_err();
    assert!(
        matches!(error,NodeError::Replication{reason} if reason.contains("no exact local recovery proof"))
    );
    assert!(hook.restored.is_empty());
    assert!(hook.replayed.is_empty());
    drop(replication);
    fs::remove_dir_all(dir).unwrap();
}
#[test]
fn recovered_publication_rejects_unverified_predecessor_without_restoring() {
    let (dir, replication, mut engine) = same_height_fixture(true);
    let path = replication.config.commit_message_path(1);
    let mut message: serde_json::Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    message["signature_hex"] = "00".repeat(64).into();
    fs::write(path, serde_json::to_vec(&message).unwrap()).unwrap();
    let mut hook = ReplayHook {
        restored: vec![],
        replayed: vec![],
    };
    assert!(
        crate::replication_state_reconcile::reconcile_engine_with_persisted_replication(
            &mut engine,
            &replication,
            "world-a",
            Some(&mut hook)
        )
        .is_err()
    );
    assert!(hook.restored.is_empty());
    assert!(hook.replayed.is_empty());
    drop(replication);
    fs::remove_dir_all(dir).unwrap();
}
