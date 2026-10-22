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
fn controller_key() -> SigningKey {
    SigningKey::from_bytes(&[71; 32])
}

fn qualified_world() -> World {
    qualified_world_with_threshold(1)
}
fn qualified_world_with_threshold(threshold: u16) -> World {
    let (mut world, mut trust, template, _) = fixtures::actual_world_initial_fixture();
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
    let closure = formal_initial::InitialWorldClosure::from_genesis_world_with_node_bootstrap(
        &world,
        &definition.world,
        &ReleaseSecurityPolicy::default(),
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
    World::from_verified_formal_snapshot(
        snapshot,
        world.journal().clone(),
        parent,
        Arc::new(EmptyNodes),
    )
    .unwrap()
}

fn action(candidate: &str) -> Action {
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
fn signed(world: &World, action: &Action, nonce: u64) -> GovernanceAuthorizationV1 {
    let mut proof = world
        .governance_authorization_for_next_commit(nonce)
        .unwrap();
    resign(&mut proof, action);
    proof
}
fn resign(proof: &mut GovernanceAuthorizationV1, action: &Action) {
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
fn begin(world: &mut World) {
    let context = context(world);
    world.begin_formal_execution_tick(&context).unwrap();
}
fn execute(world: &mut World) -> Result<(), oasis7_runtime::WorldError> {
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
