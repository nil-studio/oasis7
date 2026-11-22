use super::*;
use crate::replication::GossipReplicationMessage;
use oasis7_proto::distributed_net::DistributedNetwork;

fn source_lineage_attach_fixture() -> (
    std::path::PathBuf,
    ReplicationRuntime,
    CheckpointLineageEnvelopeV1,
) {
    let world_id = "world-lineage-source-attach-gate";
    let source_dir = temp_dir("lineage-source-attach-gate");
    let pos_config = signed_pos_config_with_signer_seeds(
        vec![PosValidator {
            validator_id: "node-a".to_string(),
            stake: 100,
        }],
        &[("node-a", 230)],
    );
    let replication_config = signed_replication_config(source_dir.clone(), 230);
    let config = NodeConfig::new("node-a", world_id, NodeRole::Sequencer)
        .expect("source config")
        .with_pos_config(pos_config.clone())
        .expect("source pos config")
        .with_replication(replication_config.clone());
    let mut replication = ReplicationRuntime::new(
        config.replication.as_ref().expect("source replication config"),
        "node-a",
    )
    .expect("source replication runtime");
    let checkpoint_height = 64;
    let checkpoint_block_hash = "checkpoint-block-64";
    let execution_block_hash = "execution-block-64";
    let execution_state_root = "execution-state-64";
    let checkpoint_message = replication
        .build_local_commit_message_with_checkpoint(
            "node-a",
            world_id,
            checkpoint_height as i64,
            &committed_decision_with_block_hash(checkpoint_height, checkpoint_block_hash),
            Some(execution_block_hash),
            Some(execution_state_root),
            Some(lineage_checkpoint_bundle(
                world_id,
                checkpoint_height,
                checkpoint_block_hash,
                execution_block_hash,
                execution_state_root,
            )),
        )
        .expect("build source checkpoint")
        .expect("source checkpoint message");
    let payload = super::replication_state_reconcile::parse_replication_commit_payload(
        checkpoint_message.payload.as_slice(),
    )
    .expect("parse source checkpoint payload");
    let descriptor = payload
        .execution_checkpoint
        .as_ref()
        .expect("source checkpoint descriptor")
        .clone();
    let engine = PosNodeEngine::new(&config).expect("source signer engine");
    let head = signed_lineage_head(world_id, "node-a", 230, 128);
    let descriptor_digest = checkpoint_lineage_descriptor_digest(
        world_id,
        descriptor.height,
        payload.block_hash.as_str(),
        descriptor.execution_block_hash.as_str(),
        descriptor.execution_state_root.as_str(),
        descriptor.manifest_ref.as_str(),
        descriptor.manifest_size_bytes,
        &descriptor
            .blobs
            .iter()
            .map(|blob| (blob.content_hash.clone(), blob.size_bytes))
            .collect::<Vec<_>>(),
    )
    .expect("source descriptor digest");
    let checkpoint = CheckpointLineageCheckpointV1 {
        height: descriptor.height,
        block_hash: payload.block_hash.clone(),
        state_root: descriptor.execution_state_root.clone(),
        execution_block_hash: descriptor.execution_block_hash.clone(),
        execution_state_root: descriptor.execution_state_root.clone(),
        descriptor_digest,
        manifest_size: descriptor.manifest_size_bytes,
    };
    let head = CheckpointLineageHeadV1 {
        height: head.height,
        block_hash: head.block_hash,
        state_root: head.execution_state_root.clone().expect("head state root"),
        execution_block_hash: head
            .execution_block_hash
            .clone()
            .expect("head execution block"),
        execution_state_root: head.execution_state_root.expect("head execution state"),
    };
    let round_id = "source-attach-gate-round".to_string();
    let vote = engine
        .build_local_checkpoint_lineage_vote(
            world_id,
            checkpoint.clone(),
            head.clone(),
            round_id.clone(),
        )
        .expect("source lineage vote")
        .vote;
    let envelope = CheckpointLineageEnvelopeV1 {
        schema_version: CHECKPOINT_LINEAGE_ENVELOPE_SCHEMA_V1,
        claim_boundary: CHECKPOINT_LINEAGE_CLAIM_BOUNDARY_V1.to_string(),
        world_id: world_id.to_string(),
        checkpoint,
        head,
        validator_set_hash: engine.validator_set_hash,
        total_stake: engine.total_stake,
        required_stake: engine.required_stake,
        round_id,
        votes: vec![vote],
    };
    (source_dir, replication, envelope)
}

fn attach_insufficient_nonfresh_lineage_envelope(
    source_replication: &mut ReplicationRuntime,
    source_config: &NodeConfig,
    world_id: &str,
    checkpoint_height: u64,
    checkpoint_block_hash: &str,
    execution_block_hash: &str,
    execution_state_root: &str,
) {
    let message = source_replication
        .load_commit_message_by_height(world_id, checkpoint_height)
        .expect("load retained checkpoint for insufficient envelope")
        .expect("retained checkpoint persisted");
    let payload = super::replication_state_reconcile::parse_replication_commit_payload(
        message.payload.as_slice(),
    )
    .expect("parse retained checkpoint for insufficient envelope");
    let descriptor = payload
        .execution_checkpoint
        .as_ref()
        .expect("retained checkpoint descriptor")
        .clone();
    let descriptor_digest = checkpoint_lineage_descriptor_digest(
        world_id,
        descriptor.height,
        payload.block_hash.as_str(),
        descriptor.execution_block_hash.as_str(),
        descriptor.execution_state_root.as_str(),
        descriptor.manifest_ref.as_str(),
        descriptor.manifest_size_bytes,
        &descriptor
            .blobs
            .iter()
            .map(|blob| (blob.content_hash.clone(), blob.size_bytes))
            .collect::<Vec<_>>(),
    )
    .expect("compute retained descriptor digest");
    assert_eq!(payload.block_hash, checkpoint_block_hash);
    assert_eq!(descriptor.execution_block_hash, execution_block_hash);
    assert_eq!(descriptor.execution_state_root, execution_state_root);
    let checkpoint = CheckpointLineageCheckpointV1 {
        height: descriptor.height,
        block_hash: payload.block_hash.clone(),
        state_root: descriptor.execution_state_root.clone(),
        execution_block_hash: descriptor.execution_block_hash.clone(),
        execution_state_root: descriptor.execution_state_root.clone(),
        descriptor_digest,
        manifest_size: descriptor.manifest_size_bytes,
    };
    let head = CheckpointLineageHeadV1 {
        height: checkpoint_height,
        block_hash: checkpoint_block_hash.to_string(),
        state_root: execution_state_root.to_string(),
        execution_block_hash: execution_block_hash.to_string(),
        execution_state_root: execution_state_root.to_string(),
    };
    let engine = PosNodeEngine::new(source_config).expect("source authority engine");
    let round_id = "nonfresh-insufficient-lineage-round".to_string();
    let vote = engine
        .build_local_checkpoint_lineage_vote(
            world_id,
            checkpoint.clone(),
            head.clone(),
            round_id.clone(),
        )
        .expect("build insufficient stake vote")
        .vote;
    let envelope = CheckpointLineageEnvelopeV1 {
        schema_version: CHECKPOINT_LINEAGE_ENVELOPE_SCHEMA_V1,
        claim_boundary: CHECKPOINT_LINEAGE_CLAIM_BOUNDARY_V1.to_string(),
        world_id: world_id.to_string(),
        checkpoint,
        head,
        validator_set_hash: engine.validator_set_hash,
        total_stake: engine.total_stake,
        required_stake: engine.required_stake,
        round_id,
        // node-a's 60 stake is below the two-validator 67 stake threshold.
        votes: vec![vote],
    };
    source_replication
        .attach_checkpoint_lineage_envelope("node-a", world_id, &envelope)
        .expect("attach insufficient lineage sidecar")
        .expect("source checkpoint sidecar attached");
    let attached = source_replication
        .load_commit_message_by_height(world_id, checkpoint_height)
        .expect("reload sidecar checkpoint")
        .expect("sidecar checkpoint remains persisted");
    let attached_payload = super::replication_state_reconcile::parse_replication_commit_payload(
        attached.payload.as_slice(),
    )
    .expect("parse sidecar checkpoint");
    assert!(
        attached_payload.lineage_envelope.is_some(),
        "insufficient lineage sidecar must be present in the signed source payload"
    );
}

fn local_lineage_envelope_for_message(
    engine: &PosNodeEngine,
    world_id: &str,
    message: &super::replication::GossipReplicationMessage,
    head_height: u64,
    head_block_hash: &str,
    head_execution_block_hash: &str,
    head_execution_state_root: &str,
) -> CheckpointLineageEnvelopeV1 {
    let payload = super::replication_state_reconcile::parse_replication_commit_payload(
        message.payload.as_slice(),
    )
    .expect("parse local lineage message");
    let descriptor = payload
        .execution_checkpoint
        .as_ref()
        .expect("local lineage checkpoint descriptor");
    let descriptor_digest = checkpoint_lineage_descriptor_digest(
        world_id,
        descriptor.height,
        payload.block_hash.as_str(),
        descriptor.execution_block_hash.as_str(),
        descriptor.execution_state_root.as_str(),
        descriptor.manifest_ref.as_str(),
        descriptor.manifest_size_bytes,
        &descriptor
            .blobs
            .iter()
            .map(|blob| (blob.content_hash.clone(), blob.size_bytes))
            .collect::<Vec<_>>(),
    )
    .expect("compute local lineage descriptor digest");
    let checkpoint = CheckpointLineageCheckpointV1 {
        height: descriptor.height,
        block_hash: payload.block_hash.clone(),
        state_root: descriptor.execution_state_root.clone(),
        execution_block_hash: descriptor.execution_block_hash.clone(),
        execution_state_root: descriptor.execution_state_root.clone(),
        descriptor_digest,
        manifest_size: descriptor.manifest_size_bytes,
    };
    let head = CheckpointLineageHeadV1 {
        height: head_height,
        block_hash: head_block_hash.to_string(),
        state_root: head_execution_state_root.to_string(),
        execution_block_hash: head_execution_block_hash.to_string(),
        execution_state_root: head_execution_state_root.to_string(),
    };
    let round_id = format!("retention-round-{}-{}", checkpoint.height, head.height);
    let vote = engine
        .build_local_checkpoint_lineage_vote(
            world_id,
            checkpoint.clone(),
            head.clone(),
            round_id.clone(),
        )
        .expect("build local retention lineage vote")
        .vote;
    CheckpointLineageEnvelopeV1 {
        schema_version: CHECKPOINT_LINEAGE_ENVELOPE_SCHEMA_V1,
        claim_boundary: CHECKPOINT_LINEAGE_CLAIM_BOUNDARY_V1.to_string(),
        world_id: world_id.to_string(),
        checkpoint,
        head,
        validator_set_hash: engine.validator_set_hash.clone(),
        total_stake: engine.total_stake,
        required_stake: engine.required_stake,
        round_id,
        votes: vec![vote],
    }
}

#[test]
fn source_attach_rejects_any_canonical_checkpoint_identity_mismatch_before_resigning() {
    let mut accepted_mismatches = Vec::new();
    for mismatch in [
        "block_hash",
        "state_root",
        "execution_block_hash",
        "execution_state_root",
        "descriptor_digest",
        "manifest_size",
    ] {
        let (source_dir, mut replication, mut envelope) = source_lineage_attach_fixture();
        match mismatch {
            "block_hash" => envelope.checkpoint.block_hash = "tampered-block".to_string(),
            "state_root" => envelope.checkpoint.state_root = "tampered-state".to_string(),
            "execution_block_hash" => {
                envelope.checkpoint.execution_block_hash = "tampered-execution-block".to_string()
            }
            "execution_state_root" => {
                envelope.checkpoint.execution_state_root = "tampered-execution-state".to_string()
            }
            "descriptor_digest" => {
                envelope.checkpoint.descriptor_digest = "tampered-descriptor".to_string()
            }
            "manifest_size" => envelope.checkpoint.manifest_size += 1,
            _ => unreachable!(),
        }
        let result = replication.attach_checkpoint_lineage_envelope(
            "node-a",
            "world-lineage-source-attach-gate",
            &envelope,
        );
        if matches!(result, Ok(Some(_))) {
            accepted_mismatches.push(mismatch);
            let _ = fs::remove_dir_all(source_dir);
            continue;
        }
        let stored = replication
            .load_commit_message_by_height("world-lineage-source-attach-gate", 64)
            .expect("inspect source checkpoint")
            .expect("source checkpoint remains persisted");
        let stored_payload =
            super::replication_state_reconcile::parse_replication_commit_payload(
                stored.payload.as_slice(),
            )
            .expect("parse persisted source checkpoint");
        assert!(
            stored_payload.lineage_envelope.is_none(),
            "rejected {mismatch} mismatch must not persist a lineage sidecar"
        );
        let _ = fs::remove_dir_all(source_dir);
    }
    assert!(
        accepted_mismatches.is_empty(),
        "source attach accepted canonical C mismatches before re-signing: {accepted_mismatches:?}"
    );
}

#[test]
fn unsigned_exact_height_fetch_head_cannot_install_checkpoint_without_lineage_authority() {
    let _nonce_lock = lock_checkpoint_probe_nonce();
    let _probe_nonce = CheckpointProbeNonceGuard::install();
    let world_id = "world-unsigned-exact-head-lineage-gate";
    let source_dir = temp_dir("unsigned-exact-head-source");
    let observer_dir = temp_dir("unsigned-exact-head-observer");
    let (_, source_public_key) = deterministic_keypair_hex(231);
    let (_, observer_public_key) = deterministic_keypair_hex(232);
    let pos_config = signed_pos_config_with_signer_seeds(
        vec![PosValidator {
            validator_id: "node-a".to_string(),
            stake: 100,
        }],
        &[("node-a", 231)],
    );
    let source_replication_config = signed_replication_config(source_dir.clone(), 231)
        .with_remote_writer_allowlist(vec![observer_public_key.clone()])
        .expect("source writer allowlist")
        .with_fetch_requester_allowlist(vec![observer_public_key.clone()])
        .expect("source fetch allowlist");
    let observer_replication_config = signed_replication_config(observer_dir.clone(), 232)
        .with_remote_writer_allowlist(vec![source_public_key.clone()])
        .expect("observer writer allowlist")
        .with_fetch_requester_allowlist(vec![source_public_key])
        .expect("observer fetch allowlist");
    let source_config = NodeConfig::new("node-a", world_id, NodeRole::Sequencer)
        .expect("source config")
        .with_pos_config(pos_config.clone())
        .expect("source pos config")
        .with_replication(source_replication_config.clone());
    let observer_config = NodeConfig::new("node-b", world_id, NodeRole::Observer)
        .expect("observer config")
        .with_pos_config(pos_config.clone())
        .expect("observer pos config")
        .with_require_execution_on_commit(true)
        .with_replication(observer_replication_config.clone());
    let mut source_replication = ReplicationRuntime::new(
        source_config.replication.as_ref().expect("source replication"),
        "node-a",
    )
    .expect("source runtime");
    let checkpoint_height = 64;
    let checkpoint_block_hash = "unsigned-head-checkpoint-block-64";
    let execution_block_hash = "unsigned-head-execution-block-64";
    let execution_state_root = "unsigned-head-execution-state-64";
    let _checkpoint_message = source_replication
        .build_local_commit_message_with_checkpoint(
            "node-a",
            world_id,
            checkpoint_height as i64,
            &committed_decision_with_block_hash(checkpoint_height, checkpoint_block_hash),
            Some(execution_block_hash),
            Some(execution_state_root),
            Some(lineage_checkpoint_bundle(
                world_id,
                checkpoint_height,
                checkpoint_block_hash,
                execution_block_hash,
                execution_state_root,
            )),
        )
        .expect("build unsigned-head checkpoint")
        .expect("unsigned-head checkpoint message");
    let network: Arc<
        dyn oasis7_proto::distributed_net::DistributedNetwork<WorldError> + Send + Sync,
    > = Arc::new(AdvertisedHeadRetainedCheckpointNetwork {
        inner: Arc::new(TestInMemoryNetwork::default()),
        advertised_head: super::replication::FetchHeadResponse {
            found: true,
            head: Some(super::replication::ReplicationHeadSummary {
                world_id: world_id.to_string(),
                height: checkpoint_height,
                block_hash: checkpoint_block_hash.to_string(),
                state_root: execution_state_root.to_string(),
                timestamp_ms: checkpoint_height as i64,
            }),
        },
    });
    register_replication_fetch_handlers(
        &NodeReplicationNetworkHandle::new(Arc::clone(&network)),
        &source_replication_config,
        world_id,
        &source_config.network_policy,
    )
    .expect("register unsigned-head checkpoint provider");
    let handle = NodeReplicationNetworkHandle::new(Arc::clone(&network));
    let mut endpoint = ReplicationNetworkEndpoint::new(
        &handle,
        world_id,
        true,
        &observer_config.network_policy,
    )
    .expect("observer endpoint");
    let mut observer_replication = ReplicationRuntime::new(
        observer_config
            .replication
            .as_ref()
            .expect("observer replication"),
        "node-b",
    )
    .expect("observer runtime");
    let mut observer_engine = PosNodeEngine::new(&observer_config).expect("observer engine");
    let mut execution_hook = RetainedBoundaryExecutionHook {
        installed: Vec::new(),
        incremental_commits: Vec::new(),
    };
    observer_engine
        .tick(
            "node-b",
            world_id,
            100,
            None,
            Some(&mut observer_replication),
            Some(&mut endpoint),
            None,
            Vec::new(),
            Some(&mut execution_hook),
        )
        .expect("unsigned exact-height preflight tick");
    assert!(
        execution_hook.installed.is_empty(),
        "signature-empty exact-height FetchHead must not install C without H-to-C lineage: installed={:?}",
        execution_hook.installed
    );
    assert!(execution_hook.incremental_commits.is_empty());
    assert_eq!(observer_engine.committed_height, 0);
    assert_eq!(observer_engine.replication_persisted_height, 0);
    assert!(
        !observer_dir
            .join("checkpoint-verification")
            .join(format!("{checkpoint_height}.json"))
            .exists(),
        "unsigned exact-height FetchHead must not produce a checkpoint receipt"
    );
    let _ = fs::remove_dir_all(source_dir);
    let _ = fs::remove_dir_all(observer_dir);
}

#[test]
fn nonfresh_unsigned_exact_height_fetch_head_cannot_install_checkpoint_without_lineage_authority() {
    let _nonce_lock = lock_checkpoint_probe_nonce();
    let _probe_nonce = CheckpointProbeNonceGuard::install();
    let world_id = "world-nonfresh-unsigned-exact-head-lineage-gate";
    let source_dir = temp_dir("nonfresh-unsigned-exact-head-source");
    let observer_dir = temp_dir("nonfresh-unsigned-exact-head-observer");
    let (_, source_public_key) = deterministic_keypair_hex(234);
    let (_, observer_public_key) = deterministic_keypair_hex(235);
    let pos_config = signed_pos_config_with_signer_seeds(
        vec![PosValidator {
            validator_id: "node-a".to_string(),
            stake: 100,
        }],
        &[("node-a", 234)],
    );
    let source_replication_config = signed_replication_config(source_dir.clone(), 234)
        .with_remote_writer_allowlist(vec![observer_public_key.clone()])
        .expect("source writer allowlist")
        .with_fetch_requester_allowlist(vec![observer_public_key.clone()])
        .expect("source fetch allowlist");
    let observer_replication_config = signed_replication_config(observer_dir.clone(), 235)
        .with_remote_writer_allowlist(vec![source_public_key.clone()])
        .expect("observer writer allowlist")
        .with_fetch_requester_allowlist(vec![source_public_key])
        .expect("observer fetch allowlist");
    let source_config = NodeConfig::new("node-a", world_id, NodeRole::Sequencer)
        .expect("source config")
        .with_pos_config(pos_config.clone())
        .expect("source pos config")
        .with_replication(source_replication_config.clone());
    let observer_config = NodeConfig::new("node-b", world_id, NodeRole::Observer)
        .expect("observer config")
        .with_pos_config(pos_config)
        .expect("observer pos config")
        .with_require_execution_on_commit(true)
        .with_replication(observer_replication_config.clone());
    let mut source_replication = ReplicationRuntime::new(
        source_config.replication.as_ref().expect("source replication"),
        "node-a",
    )
    .expect("source runtime");
    let checkpoint_height = 64;
    let checkpoint_block_hash = "nonfresh-unsigned-head-checkpoint-block-64";
    let execution_block_hash = "nonfresh-unsigned-head-execution-block-64";
    let execution_state_root = "nonfresh-unsigned-head-execution-state-64";
    source_replication
        .build_local_commit_message_with_checkpoint(
            "node-a",
            world_id,
            checkpoint_height as i64,
            &committed_decision_with_block_hash(checkpoint_height, checkpoint_block_hash),
            Some(execution_block_hash),
            Some(execution_state_root),
            Some(lineage_checkpoint_bundle(
                world_id,
                checkpoint_height,
                checkpoint_block_hash,
                execution_block_hash,
                execution_state_root,
            )),
        )
        .expect("build nonfresh unsigned-head checkpoint")
        .expect("nonfresh unsigned-head checkpoint message");
    let network: Arc<
        dyn oasis7_proto::distributed_net::DistributedNetwork<WorldError> + Send + Sync,
    > = Arc::new(AdvertisedHeadRetainedCheckpointNetwork {
        inner: Arc::new(TestInMemoryNetwork::default()),
        advertised_head: super::replication::FetchHeadResponse {
            found: true,
            head: Some(super::replication::ReplicationHeadSummary {
                world_id: world_id.to_string(),
                height: checkpoint_height,
                block_hash: checkpoint_block_hash.to_string(),
                state_root: execution_state_root.to_string(),
                timestamp_ms: checkpoint_height as i64,
            }),
        },
    });
    register_replication_fetch_handlers(
        &NodeReplicationNetworkHandle::new(Arc::clone(&network)),
        &source_replication_config,
        world_id,
        &source_config.network_policy,
    )
    .expect("register nonfresh unsigned-head provider");
    let handle = NodeReplicationNetworkHandle::new(Arc::clone(&network));
    let endpoint = ReplicationNetworkEndpoint::new(
        &handle,
        world_id,
        true,
        &observer_config.network_policy,
    )
    .expect("observer endpoint");
    let mut observer_replication = ReplicationRuntime::new(
        observer_config
            .replication
            .as_ref()
            .expect("observer replication"),
        "node-b",
    )
    .expect("observer runtime");
    let mut observer_engine = PosNodeEngine::new(&observer_config).expect("observer engine");
    // Make the observer non-fresh: the guard must remain global after an
    // already-established low-height execution cursor, not only at bootstrap.
    observer_engine.committed_height = 1;
    observer_engine.replication_persisted_height = 1;
    observer_engine.last_execution_height = 1;
    observer_engine.next_height = 2;
    observer_engine.network_committed_height = checkpoint_height;
    let mut execution_hook = RetainedBoundaryExecutionHook {
        installed: Vec::new(),
        incremental_commits: Vec::new(),
    };
    observer_engine
        .sync_missing_replication_commits(
            &endpoint,
            "node-b",
            world_id,
            Some(&mut observer_replication),
            Some(&mut execution_hook),
        )
        .expect("nonfresh unsigned exact-height sync must fail closed without error");
    assert!(execution_hook.installed.is_empty());
    assert!(execution_hook.incremental_commits.is_empty());
    assert_eq!(observer_engine.committed_height, 1);
    assert_eq!(observer_engine.replication_persisted_height, 1);
    assert!(
        !observer_dir
            .join("checkpoint-verification")
            .join(format!("{checkpoint_height}.json"))
            .exists(),
        "nonfresh unsigned exact-height FetchHead must not produce a checkpoint receipt"
    );
    let _ = fs::remove_dir_all(source_dir);
    let _ = fs::remove_dir_all(observer_dir);
}

#[test]
fn nonfresh_unsigned_exact_height_fetch_head_rejects_present_insufficient_lineage_envelope() {
    let _nonce_lock = lock_checkpoint_probe_nonce();
    let _probe_nonce = CheckpointProbeNonceGuard::install();
    let world_id = "world-nonfresh-unsigned-present-lineage-gate";
    let source_dir = temp_dir("nonfresh-unsigned-present-source");
    let observer_dir = temp_dir("nonfresh-unsigned-present-observer");
    let (_, source_public_key) = deterministic_keypair_hex(236);
    let (_, observer_public_key) = deterministic_keypair_hex(237);
    let pos_config = signed_pos_config_with_signer_seeds(
        vec![
            PosValidator {
                validator_id: "node-a".to_string(),
                stake: 60,
            },
            PosValidator {
                validator_id: "node-b".to_string(),
                stake: 40,
            },
        ],
        &[("node-a", 236), ("node-b", 237)],
    );
    let source_replication_config = signed_replication_config(source_dir.clone(), 236)
        .with_remote_writer_allowlist(vec![observer_public_key.clone()])
        .expect("source writer allowlist")
        .with_fetch_requester_allowlist(vec![observer_public_key.clone()])
        .expect("source fetch allowlist");
    let observer_replication_config = signed_replication_config(observer_dir.clone(), 237)
        .with_remote_writer_allowlist(vec![source_public_key.clone()])
        .expect("observer writer allowlist")
        .with_fetch_requester_allowlist(vec![source_public_key])
        .expect("observer fetch allowlist");
    let source_config = NodeConfig::new("node-a", world_id, NodeRole::Sequencer)
        .expect("source config")
        .with_pos_config(pos_config.clone())
        .expect("source pos config")
        .with_replication(source_replication_config.clone());
    let observer_config = NodeConfig::new("node-b", world_id, NodeRole::Observer)
        .expect("observer config")
        .with_pos_config(pos_config)
        .expect("observer pos config")
        .with_require_execution_on_commit(false)
        .with_replication(observer_replication_config.clone());
    let mut source_replication = ReplicationRuntime::new(
        source_config.replication.as_ref().expect("source replication"),
        "node-a",
    )
    .expect("source runtime");
    let checkpoint_height = 64;
    let checkpoint_block_hash = "nonfresh-present-head-checkpoint-block-64";
    let execution_block_hash = "nonfresh-present-head-execution-block-64";
    let execution_state_root = "nonfresh-present-head-execution-state-64";
    source_replication
        .build_local_commit_message_with_checkpoint(
            "node-a",
            world_id,
            checkpoint_height as i64,
            &committed_decision_with_block_hash(checkpoint_height, checkpoint_block_hash),
            Some(execution_block_hash),
            Some(execution_state_root),
            Some(lineage_checkpoint_bundle(
                world_id,
                checkpoint_height,
                checkpoint_block_hash,
                execution_block_hash,
                execution_state_root,
            )),
        )
        .expect("build nonfresh present-head checkpoint")
        .expect("nonfresh present-head checkpoint message");
    attach_insufficient_nonfresh_lineage_envelope(
        &mut source_replication,
        &source_config,
        world_id,
        checkpoint_height,
        checkpoint_block_hash,
        execution_block_hash,
        execution_state_root,
    );
    let network: Arc<
        dyn oasis7_proto::distributed_net::DistributedNetwork<WorldError> + Send + Sync,
    > = Arc::new(AdvertisedHeadRetainedCheckpointNetwork {
        inner: Arc::new(TestInMemoryNetwork::default()),
        advertised_head: super::replication::FetchHeadResponse {
            found: true,
            head: Some(super::replication::ReplicationHeadSummary {
                world_id: world_id.to_string(),
                height: checkpoint_height,
                block_hash: checkpoint_block_hash.to_string(),
                state_root: execution_state_root.to_string(),
                timestamp_ms: checkpoint_height as i64,
            }),
        },
    });
    register_replication_fetch_handlers(
        &NodeReplicationNetworkHandle::new(Arc::clone(&network)),
        &source_replication_config,
        world_id,
        &source_config.network_policy,
    )
    .expect("register nonfresh present-head provider");
    let handle = NodeReplicationNetworkHandle::new(Arc::clone(&network));
    let endpoint = ReplicationNetworkEndpoint::new(
        &handle,
        world_id,
        true,
        &observer_config.network_policy,
    )
    .expect("observer endpoint");
    let mut observer_replication = ReplicationRuntime::new(
        observer_config
            .replication
            .as_ref()
            .expect("observer replication"),
        "node-b",
    )
    .expect("observer runtime");
    let mut observer_engine = PosNodeEngine::new(&observer_config).expect("observer engine");
    observer_engine.committed_height = 1;
    observer_engine.replication_persisted_height = 0;
    observer_engine.last_execution_height = 1;
    observer_engine.next_height = 1;
    observer_engine.network_committed_height = checkpoint_height;
    let mut execution_hook = RetainedBoundaryExecutionHook {
        installed: Vec::new(),
        incremental_commits: Vec::new(),
    };
    observer_engine
        .sync_missing_replication_commits(
            &endpoint,
            "node-b",
            world_id,
            Some(&mut observer_replication),
            Some(&mut execution_hook),
        )
        .expect("nonfresh unsigned present-envelope sync must fail closed without error");
    let receipt_exists = observer_dir
        .join("checkpoint-verification")
        .join(format!("{checkpoint_height}.json"))
        .exists();
    assert!(
        execution_hook.installed.is_empty() && !receipt_exists,
        "present insufficient-quorum envelope must not install C or write a receipt: installed={:?} receipt={receipt_exists}",
        execution_hook.installed,
    );
    assert!(execution_hook.incremental_commits.is_empty());
    assert_eq!(observer_engine.committed_height, 1);
    assert_eq!(observer_engine.replication_persisted_height, 0);
    assert!(
        !observer_dir
            .join("checkpoint-verification")
            .join(format!("{checkpoint_height}.json"))
            .exists(),
        "present insufficient-quorum envelope must not produce a checkpoint receipt"
    );
    let _ = fs::remove_dir_all(source_dir);
    let _ = fs::remove_dir_all(observer_dir);
}

#[test]
fn validator_lifecycle_publishes_signed_lineage_vote_after_retained_c_then_authenticated_h() {
    let world_id = "world-validator-lineage-lifecycle";
    let source_dir = temp_dir("validator-lineage-lifecycle-source");
    let runtime_dir = temp_dir("validator-lineage-lifecycle-runtime");
    let pos_config = signed_pos_config_with_signer_seeds(
        vec![PosValidator {
            validator_id: "node-a".to_string(),
            stake: 100,
        }],
        &[("node-a", 233)],
    );
    let source_replication_config = signed_replication_config(source_dir.clone(), 233);
    let source_config = NodeConfig::new("node-a", world_id, NodeRole::Sequencer)
        .expect("validator config")
        .with_pos_config(pos_config.clone())
        .expect("validator pos config")
        .with_auto_attest_all_validators(true)
        .with_replication(source_replication_config);
    let mut source_replication = ReplicationRuntime::new(
        source_config.replication.as_ref().expect("replication config"),
        "node-a",
    )
    .expect("source replication runtime");
    let retained_checkpoint = source_replication
        .build_local_commit_message_with_checkpoint(
            "node-a",
            world_id,
            64,
            &committed_decision_with_block_hash(64, "lifecycle-checkpoint-block-64"),
            Some("lifecycle-execution-block-64"),
            Some("lifecycle-execution-state-64"),
            Some(lineage_checkpoint_bundle(
                world_id,
                64,
                "lifecycle-checkpoint-block-64",
                "lifecycle-execution-block-64",
                "lifecycle-execution-state-64",
            )),
        )
        .expect("build retained lifecycle checkpoint")
        .expect("retained lifecycle checkpoint message");
    let network_impl = Arc::new(TestInMemoryNetwork::default());
    let network: Arc<
        dyn oasis7_proto::distributed_net::DistributedNetwork<WorldError> + Send + Sync,
    > = network_impl.clone();
    let replication_config = signed_replication_config(runtime_dir.clone(), 233);
    let config = NodeConfig::new("node-a", world_id, NodeRole::Sequencer)
        .expect("validator runtime config")
        .with_tick_interval(Duration::from_millis(10))
        .expect("validator tick")
        .with_pos_config(pos_config)
        .expect("validator runtime pos config")
        .with_auto_attest_all_validators(true)
        .with_replication(replication_config);
    let handle = NodeReplicationNetworkHandle::new(Arc::clone(&network));
    let lineage_topic = super::network_bridge::default_consensus_lineage_topic(world_id);
    let lineage_subscription = network_impl
        .subscribe(lineage_topic.as_str())
        .expect("lineage subscription");
    let replication_endpoint = ReplicationNetworkEndpoint::new(
        &handle,
        world_id,
        false,
        &config.network_policy,
    )
    .expect("validator replication endpoint");
    let consensus_endpoint = ConsensusNetworkEndpoint::new(
        &handle,
        world_id,
        false,
        &config.network_policy,
    )
    .expect("validator consensus endpoint");
    let mut runtime = with_noop_execution_hook(NodeRuntime::new(config))
        .with_replication_network(NodeReplicationNetworkHandle::new(Arc::clone(&network)))
        .with_replication_network_consensus_enabled(true);
    runtime.start().expect("start validator runtime");

    replication_endpoint
        .publish_replication(&retained_checkpoint)
        .expect("publish retained C before authenticated H");
    let source_path = runtime_dir.join("checkpoint-lineage").join("source-64.json");
    let retained_persisted =
        wait_until(Instant::now() + Duration::from_secs(10), || source_path.is_file());
    if !retained_persisted {
        let snapshot = runtime.snapshot();
        let _ = runtime.stop();
        let _ = fs::remove_dir_all(source_dir);
        let _ = fs::remove_dir_all(runtime_dir);
        panic!(
            "validator runtime must persist retained C before publishing authenticated H: tick_count={} known_peer_heads={} network_committed_height={} last_error={:?}",
            snapshot.tick_count,
            snapshot.consensus.known_peer_heads,
            snapshot.consensus.network_committed_height,
            snapshot.last_error
        );
    }
    let head = signed_lineage_head(world_id, "node-a", 233, 128);
    consensus_endpoint
        .publish_commit(&head)
        .expect("publish authenticated H after retained C");
    let vote_seen = wait_until(Instant::now() + Duration::from_secs(10), || {
        lineage_subscription.drain().into_iter().any(|payload| {
            serde_json::from_slice::<GossipMessage>(payload.as_slice())
                .ok()
                .is_some_and(|message| {
                    matches!(
                        message,
                        GossipMessage::CheckpointLineageVote(vote)
                            if vote.world_id == world_id
                                && vote.checkpoint.height == 64
                                && vote.head.height == 128
                                && !vote.vote.signature_hex.is_empty()
                    )
                })
        })
    });
    let snapshot = runtime.snapshot();
    runtime.stop().expect("stop validator runtime");
    assert!(
        vote_seen,
        "normal validator lifecycle must publish a signed C→H lineage vote after retained C and authenticated H: tick_count={} known_peer_heads={} network_committed_height={} last_error={:?}",
        snapshot.tick_count,
        snapshot.consensus.known_peer_heads,
        snapshot.consensus.network_committed_height,
        snapshot.last_error
    );
    let _ = fs::remove_dir_all(source_dir);
    let _ = fs::remove_dir_all(runtime_dir);
}

#[test]
fn tampered_source_lineage_cache_fails_closed_before_vote_publication() {
    let world_id = "world-tampered-source-lineage-cache";
    let source_dir = temp_dir("tampered-source-lineage-cache");
    let pos_config = signed_pos_config_with_signer_seeds(
        vec![PosValidator {
            validator_id: "node-a".to_string(),
            stake: 100,
        }],
        &[("node-a", 239)],
    );
    let replication_config = signed_replication_config(source_dir.clone(), 239);
    let config = NodeConfig::new("node-a", world_id, NodeRole::Sequencer)
        .expect("source config")
        .with_pos_config(pos_config)
        .expect("source pos config")
        .with_replication(replication_config.clone());
    let mut replication = ReplicationRuntime::new(
        config.replication.as_ref().expect("source replication config"),
        "node-a",
    )
    .expect("source replication runtime");
    let checkpoint_height = 64;
    let checkpoint_block_hash = "tampered-cache-checkpoint-block-64";
    let execution_block_hash = "tampered-cache-execution-block-64";
    let execution_state_root = "tampered-cache-execution-state-64";
    let message = replication
        .build_local_commit_message_with_checkpoint(
            "node-a",
            world_id,
            checkpoint_height as i64,
            &committed_decision_with_block_hash(checkpoint_height, checkpoint_block_hash),
            Some(execution_block_hash),
            Some(execution_state_root),
            Some(lineage_checkpoint_bundle(
                world_id,
                checkpoint_height,
                checkpoint_block_hash,
                execution_block_hash,
                execution_state_root,
            )),
        )
        .expect("build source checkpoint")
        .expect("source checkpoint message");
    replication
        .persist_local_checkpoint_message_for_lineage("node-a", world_id, &message)
        .expect("persist source lineage cache");
    let canonical_path = source_dir
        .join("replication_commit_messages")
        .join(format!("{checkpoint_height:020}.json"));
    fs::remove_file(canonical_path).expect("remove canonical message to force source-cache fallback");
    assert!(
        replication
            .load_commit_message_by_height(world_id, checkpoint_height)
            .expect("check canonical message removal")
            .is_none(),
        "source cache fixture must be the only retained checkpoint source"
    );
    let source_cache_path = source_dir
        .join("checkpoint-lineage")
        .join(format!("source-{checkpoint_height}.json"));
    let mut tampered: GossipReplicationMessage = serde_json::from_slice(
        fs::read(source_cache_path.as_path())
            .expect("read source lineage cache")
            .as_slice(),
    )
    .expect("decode source lineage cache");
    tampered.record.content_hash = "tampered-content-hash".to_string();
    tampered.signature_hex = Some("00".repeat(64));
    fs::write(
        source_cache_path.as_path(),
        serde_json::to_vec(&tampered).expect("encode tampered source lineage cache"),
    )
    .expect("write tampered source lineage cache");

    let network_impl = Arc::new(TestInMemoryNetwork::default());
    let network: Arc<
        dyn oasis7_proto::distributed_net::DistributedNetwork<WorldError> + Send + Sync,
    > = network_impl.clone();
    let lineage_topic = super::network_bridge::default_consensus_lineage_topic(world_id);
    let lineage_subscription = network_impl
        .subscribe(lineage_topic.as_str())
        .expect("lineage subscription");
    let handle = NodeReplicationNetworkHandle::new(Arc::clone(&network));
    let consensus_endpoint = ConsensusNetworkEndpoint::new(
        &handle,
        world_id,
        false,
        &config.network_policy,
    )
    .expect("consensus endpoint");
    let mut engine = PosNodeEngine::new(&config).expect("source engine");
    let head = signed_lineage_head(world_id, "node-a", 239, 128);
    engine.observe_peer_commit_message(&head);
    let publish_result = engine.maybe_publish_local_checkpoint_lineage_vote(
        Some(&consensus_endpoint),
        None,
        "node-a",
        world_id,
        Some(&mut replication),
    );
    let tampered_vote_seen = wait_until(Instant::now() + Duration::from_millis(250), || {
        lineage_subscription.drain().into_iter().any(|payload| {
            serde_json::from_slice::<GossipMessage>(payload.as_slice())
                .ok()
                .is_some_and(|message| {
                    matches!(message, GossipMessage::CheckpointLineageVote(vote)
                        if vote.world_id == world_id
                            && vote.checkpoint.height == checkpoint_height
                            && vote.head.height == 128)
                })
        })
    });
    assert!(
        !tampered_vote_seen,
        "tampered source cache must fail closed before lineage vote publication: tampered_vote_seen={tampered_vote_seen} result={publish_result:?}"
    );
    let _ = fs::remove_dir_all(source_dir);
}

#[test]
fn checkpoint_lineage_cache_prunes_stale_source_and_envelope_files_preserving_latest() {
    let world_id = "world-lineage-cache-retention";
    let source_dir = temp_dir("lineage-cache-retention");
    let pos_config = signed_pos_config_with_signer_seeds(
        vec![PosValidator {
            validator_id: "node-a".to_string(),
            stake: 100,
        }],
        &[("node-a", 240)],
    );
    let replication_config = signed_replication_config(source_dir.clone(), 240)
        .with_max_hot_commit_messages(2)
        .expect("bounded hot commit retention");
    let config = NodeConfig::new("node-a", world_id, NodeRole::Sequencer)
        .expect("retention config")
        .with_pos_config(pos_config)
        .expect("retention pos config")
        .with_replication(replication_config.clone());
    let mut replication = ReplicationRuntime::new(
        config.replication.as_ref().expect("retention replication config"),
        "node-a",
    )
    .expect("retention replication runtime");
    let engine = PosNodeEngine::new(&config).expect("retention authority engine");

    let mut envelope_keys = Vec::new();
    for checkpoint_height in [64_u64, 128, 192] {
        let checkpoint_block_hash = format!("retention-checkpoint-block-{checkpoint_height}");
        let execution_block_hash = format!("retention-execution-block-{checkpoint_height}");
        let execution_state_root = format!("retention-execution-state-{checkpoint_height}");
        let message = replication
            .build_local_commit_message_with_checkpoint(
                "node-a",
                world_id,
                checkpoint_height as i64,
                &committed_decision_with_block_hash(
                    checkpoint_height,
                    checkpoint_block_hash.as_str(),
                ),
                Some(execution_block_hash.as_str()),
                Some(execution_state_root.as_str()),
                Some(lineage_checkpoint_bundle(
                    world_id,
                    checkpoint_height,
                    checkpoint_block_hash.as_str(),
                    execution_block_hash.as_str(),
                    execution_state_root.as_str(),
                )),
            )
            .expect("build retained checkpoint")
            .expect("retained checkpoint message");
        replication
            .persist_local_checkpoint_message_for_lineage("node-a", world_id, &message)
            .expect("persist retained source lineage cache");
        let head_height = checkpoint_height + 64;
        let envelope = local_lineage_envelope_for_message(
            &engine,
            world_id,
            &message,
            head_height,
            format!("retention-head-block-{head_height}").as_str(),
            format!("retention-head-execution-block-{head_height}").as_str(),
            format!("retention-head-execution-state-{head_height}").as_str(),
        );
        envelope_keys.push(
            replication
                .persist_checkpoint_lineage_envelope(&envelope)
                .expect("persist retained lineage envelope"),
        );
    }

    let lineage_dir = source_dir.join("checkpoint-lineage");
    assert!(
        !lineage_dir.join("source-64.json").exists(),
        "source cache must prune below the retained checkpoint boundary"
    );
    assert!(
        lineage_dir.join("source-128.json").exists(),
        "source cache must preserve the older retained rollback candidate"
    );
    assert!(
        lineage_dir.join("source-192.json").exists(),
        "source cache must preserve the newest verified source"
    );
    assert!(
        !lineage_dir.join(format!("{}.json", envelope_keys[0])).exists(),
        "lineage envelope cache must prune the stale checkpoint envelope"
    );
    assert!(
        lineage_dir
            .join(format!("{}.json", envelope_keys[1]))
            .exists(),
        "lineage envelope cache must preserve the retained rollback envelope"
    );
    assert!(
        lineage_dir
            .join(format!("{}.json", envelope_keys[2]))
            .exists(),
        "lineage envelope cache must preserve the newest verified envelope"
    );
    assert!(
        replication
            .load_commit_message_by_height(world_id, 64)
            .expect("load pruned hot checkpoint through retention boundary")
            .is_some(),
        "pruning lineage sidecars must not remove the rollback/candidate checkpoint source"
    );
    assert!(
        source_dir
            .join("replication_commit_messages")
            .join("00000000000000000128.json")
            .exists()
            && source_dir
                .join("replication_commit_messages")
                .join("00000000000000000192.json")
                .exists(),
        "the configured hot rollback/candidate boundary must remain available"
    );

    let _ = fs::remove_dir_all(source_dir);
}
