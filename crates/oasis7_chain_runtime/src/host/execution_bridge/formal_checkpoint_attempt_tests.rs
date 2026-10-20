//! Regression for checkpoint boundaries in one-shot formal executor attempts.
use super::*;
use oasis7_distfs::controlled_authority::formal::business_index::BusinessIndexNodeReader;
use oasis7_runtime::{ReleaseSecurityPolicy, VerifiedFormalRetainedHistoryParentV1, blake3_hex};
use std::{
    collections::BTreeMap,
    fs,
    path::PathBuf,
    sync::Arc,
    time::{SystemTime, UNIX_EPOCH},
};

struct EmptyIndexReader;

impl BusinessIndexNodeReader for EmptyIndexReader {
    fn read_node(&self, _: &str) -> Result<Option<Vec<u8>>, String> {
        Ok(None)
    }
}

struct C0IndexReader {
    nodes: BTreeMap<String, Vec<u8>>,
}

impl BusinessIndexNodeReader for C0IndexReader {
    fn read_node(&self, digest: &str) -> Result<Option<Vec<u8>>, String> {
        Ok(self.nodes.get(digest).cloned())
    }
}

struct ScratchRoot(PathBuf);

impl ScratchRoot {
    fn new() -> Self {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock after epoch")
            .as_nanos();
        let path = std::env::temp_dir()
            .canonicalize()
            .expect("canonical temp directory")
            .join(format!(
                "oasis7-formal-checkpoint-attempt-{}-{nonce}",
                std::process::id()
            ));
        assert!(!path.exists(), "unique test root already exists");
        Self(path)
    }

    fn path(&self) -> &std::path::Path {
        &self.0
    }
}

impl Drop for ScratchRoot {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[test]
#[cfg(unix)]
fn formal_profile2_checkpoint_boundary_needs_no_attempt_local_predecessor_record() {
    std::thread::Builder::new()
        .name("formal-checkpoint-boundary".into())
        .stack_size(32 * 1024 * 1024)
        .spawn(formal_profile2_checkpoint_boundary_case)
        .expect("spawn large-stack Wasmtime fixture")
        .join()
        .expect("checkpoint boundary case thread");
}

#[cfg(unix)]
fn formal_profile2_checkpoint_boundary_case() {
    let mut world = wasm_fixture::world_with_formal_history();
    let (_, mut trust, template_anchor, _) = fixtures::actual_world_initial_fixture();
    let start_tick = world.state().time;
    assert!(
        start_tick <= 31,
        "fixture World already beyond bootstrap tick"
    );
    for _ in start_tick..31 {
        world
            .step()
            .expect("advance existing World to approved bootstrap tick");
    }
    assert_eq!(world.state().time, 31);

    let identity = template_anchor.authority().definition().world.clone();
    let recovery_policy = template_anchor.authority().policy().clone();
    let release = ReleaseSecurityPolicy::default();
    let bootstrap_state_root = blake3_hex(&to_cbor(world.snapshot()).unwrap());
    let bootstrap = super::super::super::formal_initial::InitialNodeBootstrapV1 {
        height: 31,
        consensus_block_hash: fixtures::h(81),
        execution_block_hash: fixtures::h(82),
        execution_state_root: bootstrap_state_root.clone(),
    };
    let closure = super::super::super::formal_initial::InitialWorldClosure::
        from_existing_world_with_node_bootstrap(&world, &identity, &release, bootstrap.clone())
            .unwrap();
    let mut definition = template_anchor.authority().definition().clone();
    definition.profile_version = 2;
    let body = closure
        .activation_body(definition, recovery_policy.clone())
        .unwrap();
    trust.world = identity.clone();
    trust.initial_state_root = body.initial_state_root.clone();
    trust.governing_manifest_digest = body.governing_manifest_digest.clone();
    trust.recovery_policy_digest = recovery_policy.digest().unwrap();
    let certificate = SignedInitialActivationV1 {
        signature_hex: fixtures::sign(initial_activation_signing_bytes(&body).unwrap(), 9),
        body,
    };
    let writer = fixtures::key(4);
    let (authority, initial) = closure.authorize(&certificate, &trust, &writer).unwrap();
    let initial_proof = fixtures::qualified_proof(
        initial.candidate.record.clone(),
        None,
        &authority,
        None,
        None,
        4,
        5,
        6,
    );
    let parent =
        VerifiedFormalReadAnchorV1::from_initial(&certificate, &trust, &initial_proof).unwrap();
    let snapshot = world.snapshot();
    let retained_parent =
        VerifiedFormalRetainedHistoryParentV1::verify(&parent, &to_cbor(&snapshot).unwrap())
            .unwrap();
    let world = oasis7::runtime_host::world::from_verified_formal_snapshot(
        snapshot,
        world.journal().clone(),
        retained_parent,
        Arc::new(EmptyIndexReader),
    )
    .unwrap();

    let initial_business =
        oasis7::world_service::formal_result::FormalBusinessResultTreeV1::for_initial_world(
            &world,
            &identity,
            &EmptyIndexReader,
        )
        .unwrap();
    assert_eq!(
        initial_business.tree.root().unwrap(),
        initial.candidate.record.result_root
    );
    let index_reader = C0IndexReader {
        nodes: initial_business
            .delta
            .node_writes
            .iter()
            .map(|write| (write.digest.clone(), write.bytes.clone()))
            .collect(),
    };
    let business_reader =
        super::super::super::formal_business_package::SharedBusinessReader(Arc::new(index_reader));

    let mut original = continuation(&world, parent.head());
    original.context.height = 32;
    original.context.slot = 32;
    original.context.epoch = 1;
    original.context.node_block_hash = fixtures::h(83);
    original.context.committed_at_unix_ms = 32;
    original.predecessor_committed_height = 31;
    original.predecessor_node_block_hash = Some(bootstrap.consensus_block_hash.clone());
    original.predecessor_execution_height = 31;
    original.predecessor_execution_block_hash = Some(bootstrap.execution_block_hash.clone());
    original.predecessor_execution_state_root = Some(bootstrap_state_root);
    original.next_height = 32;
    original.next_slot = 33;
    let action = oasis7_node::NodeConsensusAction::from_payload(
        1,
        "node-a",
        oasis7::consensus_action_payload::encode_consensus_action_payload(
            &oasis7::consensus_action_payload::ConsensusActionPayloadEnvelope::from_runtime_action(
                oasis7_runtime::Action::RegisterAgent {
                    agent_id: "checkpoint-boundary-agent".into(),
                    pos: oasis7_agent_api::world_action::GeoPos::new(0, 0, 0),
                },
            ),
        )
        .unwrap(),
    )
    .unwrap();
    original.context.committed_actions = vec![action];
    original.context.action_root =
        compute_consensus_action_root(&original.context.committed_actions).unwrap();
    original.reserved_action_bytes = original
        .context
        .committed_actions
        .iter()
        .map(|action| action.payload_cbor.len())
        .sum();

    let (frozen_body, frozen_material) = freeze_material_from_parent(
        parent.authority(),
        parent.head(),
        &original,
        &world,
        &release,
        initial_business.tree,
        initial_proof,
        None,
        &initial.material,
    )
    .unwrap();
    assert_eq!(frozen_body.schema_version, 2);
    let frozen = SignedFormalFrozenV1 {
        signature_hex: fixtures::sign(frozen_signing_bytes(&frozen_body).unwrap(), 4),
        body: frozen_body,
    };
    let receiver = |role| FrozenReceiver {
        authority: parent.authority().clone(),
        parent: parent.head().clone(),
        role,
        retained: None,
    };
    let mut coordinator = FormalCoordinator::new(
        receiver(EndpointRole::Primary),
        receiver(EndpointRole::Replica),
        parent.authority().clone(),
        parent.authority().clone(),
        None,
        Some(parent.head().clone()),
    )
    .unwrap();
    let executable = coordinator.freeze(frozen, frozen_material).unwrap();

    let scratch = ScratchRoot::new();
    let candidate = super::super::super::formal_executor::execute_normal_with_business_reader(
        &executable,
        parent.authority(),
        parent.head(),
        scratch.path(),
        &writer,
        Some(&business_reader),
        Some(&parent),
    )
    .unwrap();
    candidate
        .candidate
        .verify_original(parent.authority())
        .unwrap();
    verify_record_material(&candidate.candidate.record, &candidate.material).unwrap();
    verify_formal_candidate(
        &candidate.candidate,
        candidate.frozen.as_ref(),
        parent.authority(),
        Some(parent.head()),
        None,
        FormalCompletionBranchV1::Normal,
    )
    .unwrap();
    let after_snapshot: oasis7_runtime::Snapshot =
        capture::decode_role(&candidate.material, ArtifactRole::Snapshot).unwrap();
    assert_eq!(candidate.candidate.record.schema_version, 3);
    assert_eq!(
        blake3_hex(&candidate.material.snapshot_bytes().unwrap()),
        candidate.candidate.record.after_state_root
    );
    assert!(
        after_snapshot.tick_consensus_archived_record_count > 0,
        "height-32 fixture must carry an archived Tick prefix"
    );
    let result_envelope =
        super::super::super::formal_business_package::envelope(&candidate.material, true)
            .unwrap()
            .unwrap();
    assert_eq!(result_envelope.delta.schema_version, 2);
    assert!(
        result_envelope
            .retained_history_delta
            .as_ref()
            .is_some_and(|delta| !delta.node_writes.is_empty()),
        "archived Tick prefix must be included in the retained-history candidate delta"
    );
    let journal: oasis7_runtime::Journal =
        capture::decode_role(&candidate.material, ArtifactRole::Journal).unwrap();
    assert!(
        journal
            .events
            .iter()
            .any(|event| matches!(event.body, oasis7_runtime::WorldEventBody::ModuleEmitted(_)))
    );

    let mut attempts = fs::read_dir(scratch.path())
        .unwrap()
        .map(|entry| entry.unwrap().path());
    let attempt = attempts.next().expect("one isolated formal attempt");
    assert!(attempts.next().is_none(), "executor created one attempt");
    let records = attempt.join("records");
    let record = super::super::super::checkpoint::load_execution_bridge_record(
        &super::super::super::checkpoint::execution_bridge_record_path(&records, 32),
    )
    .unwrap();
    assert_eq!(record.height, 32);
    assert!(record.checkpoint_ref.is_none());
    assert!(
        !super::super::super::checkpoint::execution_checkpoint_manifest_path(&records, 32).exists(),
        "formal one-shot attempt must not create a legacy checkpoint"
    );
}
