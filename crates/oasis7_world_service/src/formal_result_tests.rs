use super::super::wire::AgentSignerDelegationChangeV1;
use super::*;
use ed25519_dalek::SigningKey;
use oasis7_agent_api::world_action::ResourceKind;
use oasis7_distfs::controlled_authority::formal::business_index::{
    BusinessIndexDeltaV1, BusinessIndexNodeReader, BusinessIndexNodeWriteV1,
};
use oasis7_runtime::{ModuleRuntimeChargeEvent, WorldEvent, WorldEventBody};
use std::collections::BTreeMap;

fn public(n: u8) -> String {
    hex::encode(SigningKey::from_bytes(&[n; 32]).verifying_key().to_bytes())
}

#[derive(Default)]
struct MemoryIndex(BTreeMap<String, Vec<u8>>);

impl BusinessIndexNodeReader for MemoryIndex {
    fn read_node(&self, digest: &str) -> Result<Option<Vec<u8>>, String> {
        Ok(self.0.get(digest).cloned())
    }
}

struct FailingIndexReader;

impl BusinessIndexNodeReader for FailingIndexReader {
    fn read_node(&self, _digest: &str) -> Result<Option<Vec<u8>>, String> {
        Err("injected business CAS read failure".into())
    }
}

impl MemoryIndex {
    fn persist(&mut self, writes: &[BusinessIndexNodeWriteV1]) {
        for write in writes {
            self.0.insert(write.digest.clone(), write.bytes.clone());
        }
    }
}

fn h(byte: u8) -> String {
    hex::encode([byte; 32])
}

fn metadata(position: u64) -> FormalBusinessCommitMetadataV1 {
    FormalBusinessCommitMetadataV1 {
        schema_version: FORMAL_BUSINESS_RESULT_SCHEMA_V1,
        formal_position: position,
        world_id: "world".into(),
        chain_id: "chain".into(),
        genesis_digest: h(1),
        after_state_root: h(2),
        provider_world_id: "world".into(),
        branch_id: "main".into(),
        finality_epoch: 0,
        finality_status: "pending".into(),
        finality_block_hash: None,
        reorg_generation: 0,
        governing_manifest_digest: h(3),
        authority_generation: 2,
        permission_generation: 4,
        event_id_era: 7,
        journal_retention_floor: 10,
        journal_high_watermark: 19,
    }
}

fn outcome() -> FormalBusinessOutcomeLeafV1 {
    FormalBusinessOutcomeLeafV1 {
        schema_version: FORMAL_BUSINESS_RESULT_SCHEMA_V1,
        correlation: RequestCorrelation {
            key: RequestKey {
                world: WorldIdentity {
                    world_id: "world".into(),
                    genesis_digest: h(1),
                },
                verified_subject: h(4),
                operation_domain: "agent_chat".into(),
                nonce_scope: "canonical_owner_chat".into(),
                request_id_or_nonce: "17".into(),
            },
            payload_digest: h(5),
        },
        visibility_scope: "agent:agent-7".into(),
        original_request_digest: h(6),
        action_id: 9,
        committed_height: 12,
        first_formal_position: 13,
        outcome: FormalBusinessOutcomeV1::Committed {
            receipt: serde_json::json!({"opaque_receipt":"private"}),
        },
        canonical_result_cbor_hex: None,
    }
}

fn delegated_world(delegate_key: u8, generation: u64) -> World {
    let mut seed = oasis7_runtime::World::new();
    seed.submit_action(oasis7_runtime::Action::RegisterAgent {
        agent_id: "agent-7".into(),
        pos: oasis7_agent_api::world_action::GeoPos {
            x_cm: 0,
            y_cm: 0,
            z_cm: 0,
        },
    });
    seed.step().unwrap();
    let mut state = serde_json::to_value(seed.state()).unwrap();
    state["starter_oc_claims"]["agent-7"] = serde_json::json!({
        "agent_id": "agent-7",
        "player_id": "owner-7",
        "public_key": public(7),
        "amount": 1,
        "claimed_at": 0,
        "source_treasury_bucket_id": null,
    });
    let state = serde_json::from_value(state).unwrap();
    let mut world = oasis7_runtime::World::new_with_state(state);
    world
        .bind_cognition_runtime("world", "main", 0, None, "pending", 0)
        .unwrap();
    world
        .install_capability_agent_identity("agent-7", "owner-7", 1)
        .unwrap();
    apply_delegate(&mut world, delegate_key, generation);
    world
}

fn apply_delegate(world: &mut World, delegate_key: u8, generation: u64) {
    let delegation = AgentSignerDelegationChangeV1 {
        world_id: "world".into(),
        branch_id: "main".into(),
        agent_id: "agent-7".into(),
        owner_binding: "owner-7".into(),
        agent_identity_generation: 1,
        generation,
        delegate_public_key: public(delegate_key),
        revoked: false,
        nonce: generation,
    };
    let signed = authority::sign_read_request("delegation", delegation, &h(7)).unwrap();
    world.apply_agent_signer_delegation(&signed).unwrap();
}

fn leaves(
    metadata: &FormalBusinessCommitMetadataV1,
    outcome: &FormalBusinessOutcomeLeafV1,
) -> Vec<BusinessIndexLeafV1> {
    let mut leaves = vec![
        index_leaf(
            metadata_key_digest(),
            BusinessIndexLeafKindV1::CommitMetadata,
            &TreeLeafV1::CommitMetadata(metadata.clone()),
        )
        .unwrap(),
        index_leaf(
            result_index_key(&outcome.correlation.key).unwrap(),
            BusinessIndexLeafKindV1::Outcome,
            &TreeLeafV1::Outcome(outcome.clone()),
        )
        .unwrap(),
    ];
    leaves.sort_by(|left, right| left.key_digest.cmp(&right.key_digest));
    leaves
}

fn changes_context(
    source_sequence: u64,
    source_era: u64,
    retention_floor: u64,
) -> (
    FormalReadAnchorRefV1,
    FormalBusinessChangesCursorV1,
    ReadWorldViewRequest,
    ReadWorldViewResponse<WorldServiceProjection>,
) {
    let world = WorldIdentity {
        world_id: "world".into(),
        genesis_digest: h(1),
    };
    let binding = ExecutionBinding {
        provider_world_id: "world".into(),
        branch_id: "main".into(),
        finality_ref: "finality".into(),
        reorg_generation: 0,
        governing_manifest_ref: format!("blake3:{}", h(3)),
        authority_generation: 2,
        permission_generation: 4,
    };
    let source_commit = CommitRef {
        world: world.clone(),
        binding: binding.clone(),
        position: 10,
        execution_block_hash: h(10),
        state_root_ref: h(11),
    };
    let target_commit = CommitRef {
        world: world.clone(),
        binding,
        position: 12,
        execution_block_hash: h(12),
        state_root_ref: h(13),
    };
    let anchor = FormalReadAnchorRefV1 {
        position: source_commit.position,
        record_id: source_commit.execution_block_hash.clone(),
        active_authority_id: h(14),
    };
    let scope = "agent:agent-7".to_owned();
    let stream_id = authority::request_digest("formal-events-v1", &(&world, &scope)).unwrap();
    let source = FormalBusinessChangesCursorV1 {
        stream_id: stream_id.clone(),
        scope_id: scope.clone(),
        event_id_era: source_era,
        sequence: source_sequence,
        commit: source_commit,
    };
    let request = ReadWorldViewRequest {
        observation_nonce: Some("07".repeat(32)),
        formal_anchor: Some(anchor.clone()),
        contract_version: 1,
        world,
        scope_id: scope.clone(),
        min_commit: None,
        fixed_commit: None,
        deadline_unix_ms: None,
    };
    let header = FormalProjectionCommitHeaderV1 {
        authority_generation: 2,
        permission_generation: 4,
        event_id_era: 7,
        journal_retention_floor: retention_floor,
        journal_high_watermark: 50,
        governance_membership: None,
    };
    let event = |id| WorldEvent {
        id,
        time: id,
        caused_by: None,
        body: WorldEventBody::ModuleRuntimeCharged(ModuleRuntimeChargeEvent {
            module_id: "module".into(),
            trace_id: format!("trace-{id}"),
            payer_agent_id: "agent-7".into(),
            compute_fee_kind: ResourceKind::Data,
            compute_fee_amount: 1,
            electricity_fee_kind: ResourceKind::Electricity,
            electricity_fee_amount: 1,
            input_bytes: 1,
            output_bytes: 1,
            effect_count: 0,
            emit_count: 0,
        }),
    };
    let projection = WorldServiceProjection {
        native_delegation_authorizations: Vec::new(),
        formal_commit_header: Some(header),
        canonical_agent_owner: None,
        canonical_agent_chat: None,
        game_task_control: None,
        feedback_history: None,
        state: oasis7_runtime::World::new().state().clone(),
        events: vec![event(12), event(25)],
        runtime_binding: None,
        agent_context: None,
        scheduler_wakes: Vec::new(),
        continuations: Vec::new(),
        cognition_leases: Vec::new(),
        continuation_contexts: Default::default(),
    };
    let target_view = ReadWorldViewResponse {
        formal_profile: Some(FormalReadProfileV1::controlled()),
        formal_proof: Some(FormalReadSidecarV1 {
            schema_version: 1,
            profile: FormalReadProfileV1::controlled(),
            anchor: anchor.clone(),
            history_cbor_hex: String::new(),
            projection: FormalProjectionInclusionV1 {
                schema_version: 1,
                audience: FormalProjectionAudienceV1 {
                    scope_id: scope.clone(),
                    reader_public_key: None,
                },
                leaf_index: 0,
                leaf_count: 1,
                siblings: Vec::new(),
            },
        }),
        execution_evidence_scope: None,
        contract_version: 1,
        version: ProjectionVersion {
            commit: target_commit.clone(),
            projection_revision: h(15),
            visibility_scope: scope.clone(),
        },
        logical_tick: 50,
        continuation: EventCursor {
            stream_id,
            scope_id: scope,
            era: 7,
            sequence: 50,
            commit: target_commit,
        },
        view: projection,
    };
    (anchor, source, request, target_view)
}

#[test]
fn result_and_commit_metadata_are_authenticated_by_sparse_paths() {
    let metadata = metadata(13);
    let outcome = outcome();
    let mut index = MemoryIndex::default();
    let delta = BusinessIndexDeltaV1::build(
        BusinessIndexRootV1::empty().unwrap(),
        leaves(&metadata, &outcome),
        &index,
    )
    .unwrap();
    index.persist(&delta.node_writes);
    let tree = FormalBusinessResultTreeV1 {
        schema_version: FORMAL_BUSINESS_RESULT_SCHEMA_V1,
        metadata: metadata.clone(),
        index_root: delta.new,
    };
    let proof = tree
        .proof(&result_index_key(&outcome.correlation.key).unwrap(), &index)
        .unwrap();
    let root = tree.root().unwrap();
    assert_eq!(proof.outcome, outcome);
    assert_eq!(proof.commit_metadata, metadata);
    assert_eq!(proof.outcome_inclusion.siblings.len(), 256);
    assert_eq!(proof.metadata_inclusion.siblings.len(), 256);
    verify_inclusion(
        &TreeLeafV1::CommitMetadata(metadata.clone()),
        &proof.metadata_inclusion,
        &metadata_key_digest(),
        BusinessIndexLeafKindV1::CommitMetadata,
        &root,
    )
    .unwrap();
    verify_inclusion(
        &TreeLeafV1::Outcome(outcome.clone()),
        &proof.outcome_inclusion,
        &result_index_key(&outcome.correlation.key).unwrap(),
        BusinessIndexLeafKindV1::Outcome,
        &root,
    )
    .unwrap();

    let mut tampered = proof.outcome.clone();
    tampered.outcome = FormalBusinessOutcomeV1::Committed {
        receipt: serde_json::json!({"opaque_receipt":"changed"}),
    };
    assert!(
        verify_inclusion(
            &TreeLeafV1::Outcome(tampered),
            &proof.outcome_inclusion,
            &result_index_key(&outcome.correlation.key).unwrap(),
            BusinessIndexLeafKindV1::Outcome,
            &root,
        )
        .is_err()
    );
}

#[test]
fn business_result_proof_distinguishes_missing_leaf_from_cas_failure() {
    let metadata = metadata(13);
    let outcome = outcome();
    let mut index = MemoryIndex::default();
    let delta = BusinessIndexDeltaV1::build(
        BusinessIndexRootV1::empty().unwrap(),
        leaves(&metadata, &outcome),
        &index,
    )
    .unwrap();
    index.persist(&delta.node_writes);
    let tree = FormalBusinessResultTreeV1 {
        schema_version: FORMAL_BUSINESS_RESULT_SCHEMA_V1,
        metadata,
        index_root: delta.new,
    };
    let mut absent_key = outcome.correlation.key.clone();
    absent_key.request_id_or_nonce = "absent-request".into();
    let absent_digest = result_index_key(&absent_key).unwrap();

    assert!(
        tree.outcome_for_key_if_present(&absent_digest, &index)
            .unwrap()
            .is_none()
    );
    assert_eq!(
        tree.outcome_for_key_if_present(
            &result_index_key(&outcome.correlation.key).unwrap(),
            &FailingIndexReader
        )
        .unwrap_err(),
        "injected business CAS read failure"
    );
}

#[test]
fn private_result_scope_and_metadata_generations_are_explicit() {
    let leaf = outcome();
    leaf.validate().unwrap();
    assert_eq!(leaf.visibility_scope, "agent:agent-7");
    let before = metadata(13);
    let mut changed = before.clone();
    changed.permission_generation += 1;
    assert!(changed.validate().is_ok());
    assert_ne!(
        index_leaf(
            metadata_key_digest(),
            BusinessIndexLeafKindV1::CommitMetadata,
            &TreeLeafV1::CommitMetadata(before),
        )
        .unwrap()
        .canonical_value,
        index_leaf(
            metadata_key_digest(),
            BusinessIndexLeafKindV1::CommitMetadata,
            &TreeLeafV1::CommitMetadata(changed),
        )
        .unwrap()
        .canonical_value
    );
}

#[test]
fn current_delegate_can_authorize_old_result_after_rotation_without_old_delegate_authority() {
    let old_world = delegated_world(8, 1);
    let mut current_world = old_world.clone();
    apply_delegate(&mut current_world, 9, 2);
    let leaf = outcome();

    authorize_world_result_reader(&current_world, &leaf, "agent:agent-7", &public(9)).unwrap();
    assert!(authorize_world_result_reader(&old_world, &leaf, "agent:agent-7", &public(9)).is_err());
    assert!(
        authorize_world_result_reader(&current_world, &leaf, "agent:agent-7", &public(8)).is_err()
    );
    authorize_world_result_reader(&old_world, &leaf, "agent:agent-7", &public(8)).unwrap();
}

#[test]
fn historical_outcome_equality_covers_the_full_immutable_leaf() {
    let current = outcome();
    verify_historical_outcome_equality(&current, &current).unwrap();

    let mut changed = current.clone();
    changed.canonical_result_cbor_hex = Some("00".into());
    assert!(verify_historical_outcome_equality(&current, &changed).is_err());

    let mut changed_position = current.clone();
    changed_position.first_formal_position += 1;
    assert!(verify_historical_outcome_equality(&current, &changed_position).is_err());
}

#[test]
fn gameplay_result_scope_is_private_to_the_target_agent_and_unknown_payloads_fail_closed() {
    let request = oasis7_proto::viewer::GameplayActionRequest {
        action_id: "inspect".into(),
        target_agent_id: "agent-private".into(),
        actor_agent_id: Some("agent-owner".into()),
        player_id: "owner".into(),
        public_key: None,
        auth: None,
    };
    let payload = WorldServicePayloadV1::GameplayJson(serde_json::to_vec(&request).unwrap());
    assert_eq!(
        outcome_agent_id(&oasis7_runtime::World::new(), &payload).unwrap(),
        "agent-private"
    );
    assert!(
        outcome_agent_id(
            &oasis7_runtime::World::new(),
            &WorldServicePayloadV1::GameplayJson(vec![1, 2, 3])
        )
        .is_err()
    );
}

#[test]
fn original_request_commitment_includes_the_entire_envelope() {
    let original = SubmitIntentRequest {
        contract_version: 1,
        correlation: outcome().correlation,
        deadline_unix_ms: Some(50_000),
        signed_payload: WorldServicePayloadV1::GameplayJson(vec![1, 2, 3]),
    };
    let mut changed = original.clone();
    changed.deadline_unix_ms = Some(50_001);
    let original_digest = tree_hash("formal-business-original-request-v1", &original).unwrap();
    let changed_digest = tree_hash("formal-business-original-request-v1", &changed).unwrap();
    assert_ne!(original_digest, changed_digest);
}

#[test]
fn formal_changes_pages_prove_sparse_ids_and_advance_empty_ranges_to_global_watermark() {
    let (anchor, source, request, target_view) = changes_context(10, 7, 8);
    let first = build_formal_changes_page(
        anchor.clone(),
        source.clone(),
        request.clone(),
        target_view.clone(),
        1,
        1_000_000,
    )
    .unwrap();
    assert_eq!(
        first
            .changes
            .iter()
            .map(|change| change.cursor.sequence)
            .collect::<Vec<_>>(),
        vec![12]
    );
    assert_eq!(first.to_inclusive.sequence, 12);
    assert_eq!(first.next_cursor.sequence, 12);

    let complete =
        build_formal_changes_page(anchor, source, request, target_view, 10, 1_000_000).unwrap();
    assert_eq!(
        complete
            .changes
            .iter()
            .map(|change| change.cursor.sequence)
            .collect::<Vec<_>>(),
        vec![12, 25]
    );
    assert_eq!(complete.to_inclusive.sequence, 50);
    assert_eq!(complete.next_cursor.sequence, 50);
}

#[test]
fn formal_changes_returns_history_unavailable_for_expired_or_wrong_era_cursor() {
    let (anchor, mut source, request, target_view) = changes_context(6, 7, 8);
    let expired = build_formal_changes_page(
        anchor.clone(),
        source.clone(),
        request.clone(),
        target_view.clone(),
        10,
        1_000_000,
    )
    .unwrap_err();
    assert!(expired.starts_with("HistoryUnavailable"));

    source.sequence = 10;
    source.event_id_era = 6;
    let wrong_era =
        build_formal_changes_page(anchor, source, request, target_view, 10, 1_000_000).unwrap_err();
    assert!(wrong_era.starts_with("HistoryUnavailable"));
}
