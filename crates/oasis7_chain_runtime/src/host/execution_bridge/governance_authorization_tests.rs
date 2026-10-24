//! Actual signed profile-v2 anchors exercise governance execution, not deployment durability.
use super::*;
use crate::formal_read_fixtures as fixtures;
use ed25519_dalek::{Signer, SigningKey};
use oasis7_agent_api::world_action::GovernanceAuthorizationV1;
use oasis7_distfs::controlled_authority::formal::{business_index::BusinessIndexNodeReader, *};
use oasis7_runtime::{
    Action, GovernanceMainTokenControllerRegistry, GovernanceThresholdSignerPolicy,
    ReleaseSecurityPolicy, RuntimeCommittedTickContext, VerifiedFormalRetainedHistoryParentV1,
    World,
};
use oasis7_wasm_abi::{ModuleCallFailure, ModuleCallRequest, ModuleOutput, ModuleSandbox};
use std::{collections::BTreeMap, sync::Arc};

struct EmptyNodes;
impl BusinessIndexNodeReader for EmptyNodes {
    fn read_node(&self, _: &str) -> Result<Option<Vec<u8>>, String> {
        Ok(None)
    }
}
struct NoModules;
impl ModuleSandbox for NoModules {
    fn call(&mut self, _: &ModuleCallRequest) -> Result<ModuleOutput, ModuleCallFailure> {
        panic!("governance-only fixture must not invoke modules")
    }
}
pub(crate) fn controller_key() -> SigningKey {
    SigningKey::from_bytes(&[71; 32])
}

fn qualified_world() -> World {
    qualified_world_with_threshold(1)
}
fn qualified_world_with_threshold(threshold: u16) -> World {
    qualified_world_and_anchor_with_threshold(threshold).0
}
pub(super) fn qualified_world_and_anchor() -> (World, VerifiedFormalReadAnchorV1) {
    qualified_world_and_anchor_with_threshold(1)
}
fn qualified_world_and_anchor_with_threshold(
    threshold: u16,
) -> (World, VerifiedFormalReadAnchorV1) {
    let (world, anchor, _, _) = history_origin(threshold);
    (world, anchor)
}
pub(crate) fn history_origin(
    threshold: u16,
) -> (
    World,
    VerifiedFormalReadAnchorV1,
    oasis7_distfs::controlled_authority::formal::endpoint::FormalCandidateMaterialV1,
    FormalQualifiedCommitProofV1,
) {
    let (world, anchor, candidate, proof, _, _) = history_origin_with_trust(threshold);
    (world, anchor, candidate, proof)
}
pub(crate) fn history_origin_with_trust(
    threshold: u16,
) -> (
    World,
    VerifiedFormalReadAnchorV1,
    oasis7_distfs::controlled_authority::formal::endpoint::FormalCandidateMaterialV1,
    FormalQualifiedCommitProofV1,
    SignedInitialActivationV1,
    InitialActivationTrustV1,
) {
    history_origin_for_endpoints(threshold, None)
}
pub(crate) fn history_origin_for_endpoints(
    threshold: u16,
    directories: Option<[&std::path::Path; 2]>,
) -> (
    World,
    VerifiedFormalReadAnchorV1,
    oasis7_distfs::controlled_authority::formal::endpoint::FormalCandidateMaterialV1,
    FormalQualifiedCommitProofV1,
    SignedInitialActivationV1,
    InitialActivationTrustV1,
) {
    history_origin_for_endpoints_with_policy(
        threshold,
        directories,
        &ReleaseSecurityPolicy::default(),
    )
}
pub(crate) fn history_origin_for_endpoints_with_policy(
    threshold: u16,
    directories: Option<[&std::path::Path; 2]>,
    release_policy: &ReleaseSecurityPolicy,
) -> (
    World,
    VerifiedFormalReadAnchorV1,
    oasis7_distfs::controlled_authority::formal::endpoint::FormalCandidateMaterialV1,
    FormalQualifiedCommitProofV1,
    SignedInitialActivationV1,
    InitialActivationTrustV1,
) {
    let (mut world, mut trust, template, _) =
        fixtures::actual_world_initial_fixture_with_execution_policy(
            formal_package::execution_policy_material(release_policy)
                .unwrap()
                .2,
        );
    world
        .set_governance_main_token_controller_registry(GovernanceMainTokenControllerRegistry {
            genesis_controller_account_id: "controller".into(),
            controller_signer_policies: BTreeMap::from([(
                "controller".into(),
                GovernanceThresholdSignerPolicy {
                    threshold,
                    allowed_public_keys: [
                        hex::encode(controller_key().verifying_key().as_bytes()),
                        hex::encode(SigningKey::from_bytes(&[73; 32]).verifying_key().as_bytes()),
                    ]
                    .into(),
                },
            )]),
            treasury_bucket_controller_slots: BTreeMap::new(),
            restricted_starter_claim_admin_account_ids: Default::default(),
        })
        .unwrap();
    world.initialize_empty_formal_history().unwrap();
    let mut definition = template.authority().definition().clone();
    definition.profile_version = 2;
    if let Some([primary, replica]) = directories {
        definition.primary.instance.storage_fingerprint =
            oasis7_distfs::controlled_authority::formal::fencing::directory_fingerprint(primary)
                .unwrap();
        definition.replica.instance.storage_fingerprint =
            oasis7_distfs::controlled_authority::formal::fencing::directory_fingerprint(replica)
                .unwrap();
    }
    let closure = formal_initial::InitialWorldClosure::from_genesis_world_with_node_bootstrap(
        &world,
        &definition.world,
        release_policy,
        formal_initial::InitialNodeBootstrapV1 {
            height: 0,
            consensus_block_hash: fixtures::h(81),
            execution_block_hash: fixtures::h(82),
            execution_state_root: blake3_hex(&to_cbor(world.snapshot()).unwrap()),
        },
    )
    .unwrap();
    let body = closure
        .activation_body(definition, template.authority().policy().clone())
        .unwrap();
    trust.initial_state_root = body.initial_state_root.clone();
    trust.governing_manifest_digest = body.governing_manifest_digest.clone();
    let certificate = SignedInitialActivationV1 {
        signature_hex: fixtures::sign(initial_activation_signing_bytes(&body).unwrap(), 9),
        body,
    };
    let (authority, initial) = closure
        .authorize(&certificate, &trust, &fixtures::key(4))
        .unwrap();
    let proof = fixtures::qualified_proof(
        initial.candidate.record.clone(),
        None,
        &authority,
        None,
        None,
        4,
        5,
        6,
    );
    let anchor = VerifiedFormalReadAnchorV1::from_initial(&certificate, &trust, &proof).unwrap();
    let snapshot = world.snapshot();
    let parent =
        VerifiedFormalRetainedHistoryParentV1::verify(&anchor, &to_cbor(&snapshot).unwrap())
            .unwrap();
    let world = World::from_verified_formal_snapshot(
        snapshot,
        world.journal().clone(),
        parent,
        Arc::new(EmptyNodes),
    )
    .unwrap();
    (world, anchor, initial, proof, certificate, trust)
}

pub(crate) fn action(candidate: &str) -> Action {
    Action::SubmitGovernanceValidatorAdmission {
        controller_account_id: "controller".into(),
        candidate_id: candidate.into(),
        node_id: format!("node.{candidate}"),
        finality_signer_public_key: hex::encode(
            SigningKey::from_bytes(&[72; 32]).verifying_key().as_bytes(),
        ),
        stake: 25,
        operator_owner: "ops.fixture".into(),
        public_manifest_hash: "a".repeat(64),
    }
}
pub(crate) fn signed(world: &World, action: &Action, nonce: u64) -> GovernanceAuthorizationV1 {
    let mut proof = world
        .governance_authorization_for_next_commit(nonce)
        .unwrap();
    resign(&mut proof, action);
    proof
}
pub(crate) fn resign(proof: &mut GovernanceAuthorizationV1, action: &Action) {
    let signature = controller_key().sign(&proof.signing_bytes(action).unwrap());
    proof.signatures.insert(
        hex::encode(controller_key().verifying_key().as_bytes()),
        hex::encode(signature.to_bytes()),
    );
}
fn context(world: &World) -> RuntimeCommittedTickContext {
    RuntimeCommittedTickContext {
        height: world.state().time + 1,
        slot: 0,
        epoch: 0,
        node_block_hash: "b".repeat(64),
        action_root: "c".repeat(64),
        authority_node_id: world.tick_consensus_authority_source().into(),
        committed_at_unix_ms: 0,
    }
}
pub(crate) fn begin(world: &mut World) {
    let context = context(world);
    world.begin_formal_execution_tick(&context).unwrap();
}
pub(crate) fn execute(world: &mut World) -> Result<(), oasis7_runtime::WorldError> {
    let mut context = context(world);
    context.height = world.state().time;
    world.step_with_modules_for_committed_context(&mut NoModules, &context)
}

#[test]
fn governance_authorization_qualified_parent_executes_and_persists_proof() {
    let mut world = qualified_world();
    let command = action("valid");
    let proof = signed(&world, &command, 1);
    assert_eq!(proof.parent_position, 1);
    assert_eq!(proof.valid_from_position, 2);
    assert_eq!(
        world.state().time,
        0,
        "formal positions differ from local tick"
    );
    begin(&mut world);
    world.submit_governance_action(command.clone(), proof.clone());
    execute(&mut world).unwrap();
    assert!(
        world
            .governance_validator_admissions()
            .contains_key("valid")
    );
    assert_eq!(
        world
            .state()
            .governance_authorization_nonces
            .get("controller"),
        Some(&1)
    );
    let bytes = to_cbor(world.snapshot()).unwrap();
    let snapshot: oasis7_runtime::Snapshot = serde_cbor::from_slice(&bytes).unwrap();
    assert_eq!(
        snapshot
            .state
            .governance_authorization_nonces
            .get("controller"),
        Some(&1)
    );
    assert!(world.journal().events.iter().any(|event| matches!(
        &event.body,
        oasis7_runtime::WorldEventBody::Governance(oasis7_runtime::GovernanceEvent::ValidatorAdmissionSubmitted { authorization: Some(saved), .. }) if saved.authorization == proof && saved.action.as_ref() == &command
    )));
}

#[test]
fn governance_authorization_rejects_resigned_wrong_context_and_forged_action() {
    for case in 0..7 {
        let mut world = qualified_world();
        let mut command = action("invalid");
        let mut proof = signed(&world, &command, 1);
        match case {
            0 => {
                proof.world_id = "other-world".into();
                resign(&mut proof, &command);
            }
            1 => {
                proof.parent_record_id = "d".repeat(64);
                resign(&mut proof, &command);
            }
            2 => {
                proof.policy_digest = "e".repeat(64);
                resign(&mut proof, &command);
            }
            3 => {
                proof
                    .signatures
                    .values_mut()
                    .for_each(|signature| *signature = "0".repeat(128));
            }
            4 => {
                if let Action::SubmitGovernanceValidatorAdmission { stake, .. } = &mut command {
                    *stake += 1;
                }
            }
            5 => proof.signatures.clear(),
            _ => {
                proof.valid_from_position = 3;
                proof.valid_through_position = 4;
                resign(&mut proof, &command);
            }
        }
        begin(&mut world);
        world.submit_governance_action(command, proof);
        let journal = world.journal().clone();
        assert!(execute(&mut world).is_err(), "case {case}");
        assert!(world.governance_validator_admissions().is_empty());
        assert!(world.state().governance_authorization_nonces.is_empty());
        assert_eq!(to_cbor(world.journal()).unwrap(), to_cbor(journal).unwrap());
    }
}

#[test]
fn governance_authorization_duplicate_nonce_rolls_back_whole_batch() {
    let mut world = qualified_world();
    let first = action("first");
    let second = action("second");
    begin(&mut world);
    world.submit_governance_action(first.clone(), signed(&world, &first, 1));
    world.submit_governance_action(second.clone(), signed(&world, &second, 1));
    let journal = to_cbor(world.journal()).unwrap();
    assert!(execute(&mut world).is_err());
    assert!(world.governance_validator_admissions().is_empty());
    assert!(world.state().governance_authorization_nonces.is_empty());
    assert_eq!(to_cbor(world.journal()).unwrap(), journal);
}

#[test]
fn governance_authorization_enforces_committed_threshold() {
    let world = qualified_world_with_threshold(2);
    let command = action("threshold");
    let mut proof = signed(&world, &command, 1);
    let mut insufficient = world.clone();
    begin(&mut insufficient);
    insufficient.submit_governance_action(command.clone(), proof.clone());
    assert!(execute(&mut insufficient).is_err());
    assert!(insufficient.governance_validator_admissions().is_empty());
    let second = SigningKey::from_bytes(&[73; 32]);
    let signature = second.sign(&proof.signing_bytes(&command).unwrap());
    proof.signatures.insert(
        hex::encode(second.verifying_key().as_bytes()),
        hex::encode(signature.to_bytes()),
    );
    let mut accepted = world;
    begin(&mut accepted);
    accepted.submit_governance_action(command, proof);
    execute(&mut accepted).unwrap();
    assert!(
        accepted
            .governance_validator_admissions()
            .contains_key("threshold")
    );
}

#[test]
fn governance_authorization_all_four_actions_share_frozen_parent() {
    let mut world = qualified_world();
    let commands = [
        action("lifecycle"),
        Action::ApproveGovernanceValidatorAdmission {
            controller_account_id: "controller".into(),
            candidate_id: "lifecycle".into(),
        },
        Action::ActivateGovernanceValidatorAdmission {
            controller_account_id: "controller".into(),
            candidate_id: "lifecycle".into(),
            activation_epoch: 100,
        },
        Action::RevokeGovernanceValidatorAdmission {
            controller_account_id: "controller".into(),
            candidate_id: "lifecycle".into(),
            node_id: "node.lifecycle".into(),
            reason: "operator rotation scheduled".into(),
        },
    ];
    begin(&mut world);
    for (index, command) in commands.iter().enumerate() {
        world.submit_governance_action(command.clone(), signed(&world, command, index as u64 + 1));
    }
    execute(&mut world).unwrap();
    let record = &world.governance_validator_admissions()["lifecycle"];
    assert_eq!(
        record.status,
        oasis7_runtime::GovernanceValidatorAdmissionStatus::Revoked
    );
    assert_eq!(
        world
            .state()
            .governance_authorization_nonces
            .get("controller"),
        Some(&4)
    );
    let records: Vec<_> = world
        .journal()
        .events
        .iter()
        .filter_map(|event| {
            use oasis7_runtime::{GovernanceEvent as G, WorldEventBody as B};
            match &event.body {
                B::Governance(
                    G::ValidatorAdmissionSubmitted { authorization, .. }
                    | G::ValidatorAdmissionApproved { authorization, .. }
                    | G::ValidatorAdmissionActivated { authorization, .. }
                    | G::ValidatorAdmissionRevoked { authorization, .. },
                ) => authorization.as_ref(),
                _ => None,
            }
        })
        .collect();
    assert_eq!(records.len(), 4);
    for (index, record) in records.iter().enumerate() {
        assert_eq!(record.action.as_ref(), &commands[index]);
        assert_eq!(record.authorization.nonce, index as u64 + 1);
        assert_eq!(record.authorization.parent_position, 1);
    }
}

#[test]
fn governance_authorization_current_policy_cannot_authorize_itself() {
    for attacker in [false, true] {
        let mut world = qualified_world();
        let command = action("policy");
        let mut proof = signed(&world, &command, 1);
        let context = context(&world);
        world.begin_formal_execution_tick(&context).unwrap();
        let new_signer = SigningKey::from_bytes(&[74; 32]);
        let mut registry = world
            .governance_main_token_controller_registry()
            .unwrap()
            .clone();
        registry
            .controller_signer_policies
            .get_mut("controller")
            .unwrap()
            .allowed_public_keys = [hex::encode(new_signer.verifying_key().as_bytes())].into();
        world
            .set_governance_main_token_controller_registry(registry.clone())
            .unwrap();
        if attacker {
            proof.policy_digest = blake3_hex(
                &serde_cbor::to_vec(&("oasis7.governance.controller-policy.v1", &registry))
                    .unwrap(),
            );
            proof.signatures.clear();
            let signature = new_signer.sign(&proof.signing_bytes(&command).unwrap());
            proof.signatures.insert(
                hex::encode(new_signer.verifying_key().as_bytes()),
                hex::encode(signature.to_bytes()),
            );
        }
        world.submit_governance_action(command, proof);
        let result = world.step_with_modules_for_committed_context(&mut NoModules, &context);
        if attacker {
            assert!(result.is_err());
            assert!(world.governance_validator_admissions().is_empty());
            assert!(world.state().governance_authorization_nonces.is_empty());
        } else {
            result.unwrap();
            assert!(
                world
                    .governance_validator_admissions()
                    .contains_key("policy")
            );
        }
    }
}

#[test]
fn governance_authorization_raw_action_missing_proof_is_rejected() {
    let mut world = qualified_world();
    begin(&mut world);
    world.submit_action(action("unsigned"));
    let before = to_cbor(world.journal()).unwrap();
    assert!(execute(&mut world).is_err());
    assert!(world.governance_validator_admissions().is_empty());
    assert!(world.state().governance_authorization_nonces.is_empty());
    assert_eq!(to_cbor(world.journal()).unwrap(), before);
}

#[test]
fn governance_authorization_real_node_driver_ingress_rejects_before_queueing() {
    use oasis7::consensus_action_payload::{
        ConsensusActionAuthEnvelope, ConsensusActionPayloadBody, ConsensusActionPayloadEnvelope,
    };
    let world = qualified_world();
    let command = action("ingress");
    let valid = signed(&world, &command, 1);
    struct Scratch(std::path::PathBuf);
    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
    let root = Scratch(std::env::temp_dir().canonicalize().unwrap().join(format!(
        "oasis7-governance-ingress-{}-{}", std::process::id(),
        std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos(),
    )));
    std::fs::create_dir(&root.0).unwrap();
    let driver = driver::NodeRuntimeExecutionDriver::new_with_sandbox(
        root.0.join("state.cbor"),
        root.0.join("world"),
        root.0.join("records"),
        root.0.join("storage"),
        ExecutionBridgeState::default(),
        world,
        Box::new(NoModules),
        32,
        32,
        4,
    );
    let node = oasis7_node::NodeRuntime::new(
        oasis7_node::NodeConfig::new(
            "governance-ingress",
            &valid.world_id,
            oasis7_node::NodeRole::Observer,
        )
        .unwrap(),
    )
    .with_execution_hook(driver);
    let payload = |action: Action, proof: Option<GovernanceAuthorizationV1>| {
        serde_cbor::to_vec(&ConsensusActionPayloadEnvelope {
            version: 1,
            gameplay_submission_origin: None,
            auth: proof.map(ConsensusActionAuthEnvelope::GovernanceAction),
            body: ConsensusActionPayloadBody::RuntimeAction { action },
        })
        .unwrap()
    };
    for case in 0..3 {
        let mut proof = valid.clone();
        match case {
            0 => proof
                .signatures
                .values_mut()
                .for_each(|signature| *signature = "0".repeat(128)),
            1 => {
                proof.world_id = "wrong-world".into();
                resign(&mut proof, &command);
            }
            _ => {
                proof.parent_record_id = "f".repeat(64);
                resign(&mut proof, &command);
            }
        }
        assert!(
            node.submit_consensus_action_payload(1, payload(command.clone(), Some(proof)))
                .is_err(),
            "case {case}"
        );
    }
    // Reusing the same action ID and nonce proves rejected requests did not
    // consume the production ingress reservation or poison the next request.
    node.submit_consensus_action_payload(1, payload(command, Some(valid)))
        .unwrap();
    node.submit_consensus_action_payload(
        2,
        payload(
            Action::RegisterAgent {
                agent_id: "unrelated".into(),
                pos: oasis7_agent_api::world_action::GeoPos::new(0, 0, 0),
            },
            None,
        ),
    )
    .unwrap();
    drop(node);
}

/// Genuine signed archives for host replay tests. Execution is the real runtime;
/// endpoint receipts use fixture keys and do not certify deployment key custody.
pub(crate) fn history_successor(
    before: &World,
    parent: &VerifiedFormalReadAnchorV1,
    mut after: World,
    parent_candidate: &oasis7_distfs::controlled_authority::formal::endpoint::FormalCandidateMaterialV1,
    parent_proof: &FormalQualifiedCommitProofV1,
) -> (
    World,
    VerifiedFormalReadAnchorV1,
    oasis7_distfs::controlled_authority::formal::endpoint::FormalCandidateMaterialV1,
    FormalQualifiedCommitProofV1,
) {
    use oasis7_distfs::controlled_authority::replicated_protocol::{ArtifactObject, ArtifactRole};
    let height = before.state().time + 1;
    let continuation = oasis7_node::NodeLocalExecutionContinuation {
        schema_version: 1,
        context: oasis7_node::NodeExecutionCommitContext {
            world_id: parent.head().record().world.world_id.clone(),
            node_id: "node-a".into(),
            proposer_id: "node-a".into(),
            height,
            slot: 0,
            epoch: 0,
            node_block_hash: fixtures::h(90),
            action_root: oasis7_node::compute_consensus_action_root(&[]).unwrap(),
            committed_actions: vec![],
            committed_at_unix_ms: 0,
        },
        predecessor_committed_height: height - 1,
        predecessor_node_block_hash: Some(fixtures::h(91)),
        predecessor_execution_height: height - 1,
        predecessor_execution_block_hash: Some(fixtures::h(92)),
        predecessor_execution_state_root: Some(blake3_hex(&to_cbor(before.snapshot()).unwrap())),
        next_height: height + 1,
        next_slot: 1,
        reserved_action_bytes: 0,
        approved_stake: 1,
        rejected_stake: 0,
        required_stake: 1,
        total_stake: 1,
    };
    let authority = parent.authority();
    let hardened = ReleaseSecurityPolicy::production_hardened();
    let release_policy = if formal_package::execution_policy_material(&hardened)
        .unwrap()
        .2
        == parent.head().record().execution_policy_digest
    {
        hardened
    } else {
        ReleaseSecurityPolicy::default()
    };
    let business_tree = super::formal_business_package::tree(
        &parent_candidate.material,
        parent.head().record(),
        before,
        true,
    )
    .unwrap()
    .unwrap();
    let (body, frozen_material) = formal_package::freeze_material_from_parent(
        authority,
        parent.head(),
        &continuation,
        before,
        &release_policy,
        business_tree.clone(),
        parent_proof.clone(),
        None,
        &parent_candidate.material,
    )
    .unwrap();
    let receipt = |role, binding: &EndpointSignerBindingV1, n| {
        let body = FormalFrozenReceiptBodyV1 {
            authority_id: authority.authority_id().unwrap(),
            role,
            instance_digest: binding.instance.digest().unwrap(),
            reservation_id: body.reservation_id().unwrap(),
        };
        FormalFrozenReceiptV1 {
            signature_hex: fixtures::sign(frozen_receipt_signing_bytes(&body).unwrap(), n),
            body,
        }
    };
    let frozen = FormalFrozenCertificateV1 {
        primary: receipt(
            oasis7_distfs::controlled_authority::replicated_protocol::EndpointRole::Primary,
            &authority.definition().primary,
            5,
        ),
        replica: receipt(
            oasis7_distfs::controlled_authority::replicated_protocol::EndpointRole::Replica,
            &authority.definition().replica,
            6,
        ),
        reservation: SignedFormalFrozenV1 {
            signature_hex: fixtures::sign(frozen_signing_bytes(&body).unwrap(), 4),
            body: body.clone(),
        },
    };
    let mut material = frozen_material.clone();
    for (role, bytes) in [
        (ArtifactRole::Snapshot, to_cbor(after.snapshot()).unwrap()),
        (ArtifactRole::Journal, to_cbor(after.journal()).unwrap()),
    ] {
        let content_hash = blake3_hex(&bytes);
        material.roots.insert(role, content_hash.clone());
        if !material
            .objects
            .iter()
            .any(|o| o.content_hash == content_hash)
        {
            material.objects.push(ArtifactObject {
                content_hash,
                bytes,
                references: vec![],
            });
        }
    }
    material
        .objects
        .sort_by(|a, b| a.content_hash.cmp(&b.content_hash));
    material.after_state_root = blake3_hex(&to_cbor(after.snapshot()).unwrap());
    let mut record = parent.head().record().clone();
    record.kind = FormalRecordKindV1::Execution;
    record.position = body.next_position;
    record.parent_record_id = body.parent.record_id.clone();
    record.request_digest = body.request_digest;
    record.payload_digest = material.payload_digest.clone();
    record.frozen_reservation_id = Some(frozen.reservation.body.reservation_id().unwrap());
    record.runtime_manifest_digest = body.runtime_manifest_digest;
    record.before_state_root = material.before_state_root.clone();
    record.after_state_root = material.after_state_root.clone();
    record.artifact_roots = material.roots.clone();
    record.projection_root =
        oasis7::world_service::formal_projection::commitments_for_world(&after)
            .unwrap()
            .root()
            .unwrap();
    use oasis7_distfs::controlled_authority::formal::business_material::{
        FORMAL_BUSINESS_ENVELOPE_DOMAIN_V1, FormalBusinessResultEnvelopeV1,
    };
    struct ArchiveNodes<'a>(
        &'a oasis7_distfs::controlled_authority::replicated_protocol::ClosedRecord,
    );
    impl BusinessIndexNodeReader for ArchiveNodes<'_> {
        fn read_node(&self, digest: &str) -> Result<Option<Vec<u8>>, String> {
            for object in &self.0.objects {
                if let Ok(envelope) = FormalBusinessResultEnvelopeV1::decode(&object.bytes)
                    && let Some(node) = envelope
                        .delta
                        .node_writes
                        .iter()
                        .find(|node| node.digest == digest)
                {
                    return Ok(Some(node.bytes.clone()));
                }
            }
            Ok(None)
        }
    }
    let update =
        oasis7::world_service::formal_result::FormalBusinessResultTreeV1::extend_with_parent_proof(
            &after,
            &record,
            &business_tree,
            parent_proof,
            &[],
            &ArchiveNodes(&parent_candidate.material),
        )
        .unwrap();
    record.result_root = update.tree.root().unwrap();
    let old_result = material.roots[&ArtifactRole::Result].clone();
    let envelope = FormalBusinessResultEnvelopeV1 {
        schema_version: 1,
        domain: FORMAL_BUSINESS_ENVELOPE_DOMAIN_V1.into(),
        execution_result_cbor: super::formal_business_package::envelope(
            &parent_candidate.material,
            true,
        )
        .unwrap()
        .unwrap()
        .execution_result_cbor,
        tree_descriptor_cbor: update.tree.encode_cbor().unwrap(),
        delta: update.delta,
        retained_history_delta: None,
    }
    .encode()
    .unwrap();
    let digest = blake3_hex(&envelope);
    let mut references = vec![
        old_result,
        material.roots[&ArtifactRole::Snapshot].clone(),
        material.roots[&ArtifactRole::Journal].clone(),
    ];
    references.sort();
    references.dedup();
    material.objects.push(ArtifactObject {
        content_hash: digest.clone(),
        bytes: envelope,
        references,
    });
    material.roots.insert(ArtifactRole::Result, digest);
    material
        .objects
        .sort_by(|a, b| a.content_hash.cmp(&b.content_hash));
    record.artifact_roots = material.roots.clone();
    verify_record_material(&record, &material).unwrap();
    let proof = fixtures::qualified_proof(
        record.clone(),
        Some(frozen.clone()),
        authority,
        Some(parent.head()),
        None,
        4,
        5,
        6,
    );
    let committed = verify_formal_qualified(&proof, authority, Some(parent.head()), None).unwrap();
    let anchor = parent
        .after_verified_commit(authority.clone(), committed)
        .unwrap();
    let bytes = to_cbor(after.snapshot()).unwrap();
    let token = VerifiedFormalRetainedHistoryParentV1::verify(&anchor, &bytes).unwrap();
    after = World::from_verified_formal_snapshot(
        after.snapshot(),
        after.journal().clone(),
        token,
        Arc::new(EmptyNodes),
    )
    .unwrap();
    let candidate =
        oasis7_distfs::controlled_authority::formal::endpoint::FormalCandidateMaterialV1 {
            candidate: SignedCanonicalCandidateV1 {
                record,
                writer_signature_hex: proof.decision.writer_signature_hex.clone(),
            },
            material,
            frozen: Some(frozen),
            frozen_material: Some(frozen_material),
            branch: FormalCompletionBranchV1::Normal,
        };
    (after, anchor, candidate, proof)
}
