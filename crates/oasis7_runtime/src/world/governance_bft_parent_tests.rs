use super::*;
use crate::{Action, GovernanceThresholdSignerPolicy, WorldState};
use ed25519_dalek::{Signer, SigningKey};
use oasis7_agent_api::world_action::GovernanceAuthorizationV1;
use oasis7_consensus::tendermint_v2::{BftSchedulePolicyV2, BftValidatorSetV2};
use oasis7_proto::distributed_bft::v2::{BftScopeV2, BftValidatorV2};
use oasis7_wasm_abi::{ModuleCallFailure, ModuleCallRequest, ModuleOutput, ModuleSandbox};
use std::collections::BTreeMap;

struct Sandbox;
impl ModuleSandbox for Sandbox {
    fn call(&mut self, _: &ModuleCallRequest) -> Result<ModuleOutput, ModuleCallFailure> {
        Ok(ModuleOutput {
            new_state: None,
            effects: vec![],
            emits: vec![],
            tick_lifecycle: None,
            output_bytes: 0,
        })
    }
}

fn fixture() -> (
    World,
    BftHeightProposerScheduleV2,
    SigningKey,
    RuntimeCommittedTickContext,
) {
    let mut world = World::new_with_state(WorldState {
        world_rules: None,
        ..Default::default()
    });
    let controller = SigningKey::from_bytes(&[7; 32]);
    world
        .set_governance_main_token_controller_registry(GovernanceMainTokenControllerRegistry {
            genesis_controller_account_id: "controller".into(),
            controller_signer_policies: BTreeMap::from([(
                "controller".into(),
                GovernanceThresholdSignerPolicy {
                    threshold: 1,
                    allowed_public_keys: [hex::encode(controller.verifying_key().to_bytes())]
                        .into(),
                },
            )]),
            ..Default::default()
        })
        .unwrap();
    let validator = SigningKey::from_bytes(&[9; 32]);
    let set = BftValidatorSetV2::new(
        BftScopeV2 {
            world_id: "bft-world".into(),
            chain_id: "bft-chain".into(),
            genesis_hash: "aa".repeat(32),
        },
        vec![BftValidatorV2 {
            validator_id: "validator".into(),
            public_key_hex: hex::encode(validator.verifying_key().to_bytes()),
            voting_power: 1,
        }],
    )
    .unwrap();
    let schedule = BftHeightProposerScheduleV2::genesis(
        set,
        BftSchedulePolicyV2::new(
            world.governance_execution_policy().epoch_length_ticks,
            "engineering_tendermint_v2",
        )
        .unwrap(),
        1,
        world.current_state_root_hash().unwrap(),
        world.current_manifest_hash().unwrap(),
    )
    .unwrap();
    let context = RuntimeCommittedTickContext {
        height: 1,
        slot: 0,
        epoch: 1,
        node_block_hash: "bb".repeat(32),
        action_root: "cc".repeat(32),
        authority_node_id: world.tick_consensus_authority_source().into(),
        committed_at_unix_ms: 0,
    };
    (world, schedule, controller, context)
}

fn command() -> Action {
    Action::SubmitGovernanceValidatorAdmission {
        controller_account_id: "controller".into(),
        candidate_id: "candidate".into(),
        node_id: "node".into(),
        finality_signer_public_key: hex::encode(
            SigningKey::from_bytes(&[8; 32]).verifying_key().to_bytes(),
        ),
        stake: 25,
        operator_owner: "owner".into(),
        public_manifest_hash: "dd".repeat(32),
    }
}

fn signed(
    world: &World,
    key: &SigningKey,
    action: &Action,
    nonce: u64,
) -> GovernanceAuthorizationV1 {
    let mut proof = world
        .governance_authorization_for_next_commit(nonce)
        .unwrap();
    proof.signatures.insert(
        hex::encode(key.verifying_key().to_bytes()),
        hex::encode(key.sign(&proof.signing_bytes(action).unwrap()).to_bytes()),
    );
    proof
}

#[test]
fn bft_governance_requires_exact_parent_and_explicit_successor_execution() {
    let (mut world, schedule, key, context) = fixture();
    let snapshot = world.snapshot();
    assert!(world.governance_authorization_for_next_commit(1).is_err());
    world.attach_bft_governance_parent_v2(&schedule).unwrap();
    assert_eq!(
        world.snapshot(),
        snapshot,
        "attaching a cursor must not alter persisted World"
    );
    assert!(world.formal_history_reader.is_none());
    let action = command();
    let proof = signed(&world, &key, &action, 1);
    assert_eq!(proof.world_id, "bft-world");
    assert_eq!(proof.chain_id, "bft-chain");
    assert_eq!(proof.parent_record_id, schedule.parent_commit_ref());
    assert_eq!(proof.parent_state_root, schedule.parent_state_root());
    world
        .verify_governance_action_for_next_commit(&action, &proof)
        .unwrap();

    let mut ordinary = world.clone();
    ordinary.submit_governance_action(action.clone(), proof.clone());
    let before = ordinary.snapshot();
    assert!(ordinary.step().is_err());
    assert_eq!(ordinary.snapshot(), before);

    world.begin_bft_execution_tick(&context).unwrap();
    assert!(
        world.step().is_err(),
        "ordinary stepping cannot bypass the begun context"
    );
    world.submit_governance_action(action, proof);
    let mut wrong_context = context.clone();
    wrong_context.node_block_hash = "ee".repeat(32);
    assert!(
        world
            .step_with_modules_for_committed_context(&mut Sandbox, &wrong_context)
            .is_err()
    );
    assert!(world.finish_bft_execution_tick(&context).is_err());
    world
        .step_with_modules_for_committed_context(&mut Sandbox, &context)
        .unwrap();
    world.finish_bft_execution_tick(&context).unwrap();
    assert!(world.bft_governance_parent.is_none());
    assert!(world.finish_bft_execution_tick(&context).is_err());
    assert_eq!(
        world
            .state
            .governance_authorization_nonces
            .get("controller"),
        Some(&1)
    );
    assert!(
        world
            .state
            .governance_validator_admissions
            .contains_key("candidate")
    );
}

#[test]
fn bft_parent_rejects_world_drift_and_keeps_the_original_controller_policy() {
    let (mut world, schedule, key, _) = fixture();
    let mut wrong_world = world.clone();
    wrong_world.state.time = 1;
    assert!(
        wrong_world
            .attach_bft_governance_parent_v2(&schedule)
            .is_err()
    );
    world.attach_bft_governance_parent_v2(&schedule).unwrap();
    let action = command();
    let valid = signed(&world, &key, &action, 1);
    let mut wrong_scope = valid.clone();
    wrong_scope.world_id = "other-world".into();
    assert!(
        world
            .verify_governance_action_for_next_commit(&action, &wrong_scope)
            .is_err()
    );
    let mut tampered = action.clone();
    let Action::SubmitGovernanceValidatorAdmission { stake, .. } = &mut tampered else {
        unreachable!()
    };
    *stake += 1;
    assert!(
        world
            .verify_governance_action_for_next_commit(&tampered, &valid)
            .is_err()
    );

    let replacement = SigningKey::from_bytes(&[6; 32]);
    world
        .state
        .governance_main_token_controller_registry
        .as_mut()
        .unwrap()
        .controller_signer_policies
        .get_mut("controller")
        .unwrap()
        .allowed_public_keys = [hex::encode(replacement.verifying_key().to_bytes())].into();
    assert!(
        world
            .verify_governance_action_for_next_commit(
                &action,
                &signed(&world, &replacement, &action, 1)
            )
            .is_err()
    );
    world
        .verify_governance_action_for_next_commit(&action, &valid)
        .unwrap();
    let (_, _, _, context) = fixture();
    assert!(
        world.begin_bft_execution_tick(&context).is_err(),
        "mutating the parent cannot authorize a successor"
    );
}

#[test]
fn snapshot_restore_cannot_restore_bft_governance_authority() {
    let (mut world, schedule, key, context) = fixture();
    world.attach_bft_governance_parent_v2(&schedule).unwrap();
    let action = command();
    let proof = signed(&world, &key, &action, 1);
    world.begin_bft_execution_tick(&context).unwrap();

    let mut restored = World::from_snapshot(world.snapshot(), world.journal.clone()).unwrap();
    assert!(restored.bft_governance_parent.is_none());
    assert!(
        restored
            .governance_authorization_for_next_commit(1)
            .is_err()
    );
    assert!(restored.begin_bft_execution_tick(&context).is_err());
    assert!(
        restored
            .verify_governance_action_for_next_commit(&action, &proof)
            .is_err()
    );

    // Only independently recovering and attaching the pinned protocol cursor
    // makes the persisted World eligible for a new candidate execution phase.
    restored.attach_bft_governance_parent_v2(&schedule).unwrap();
    restored
        .verify_governance_action_for_next_commit(&action, &proof)
        .unwrap();
    restored.begin_bft_execution_tick(&context).unwrap();
}

#[test]
fn failed_bft_governance_step_keeps_parent_phase_and_world_unpublished() {
    let (mut world, schedule, key, context) = fixture();
    world.attach_bft_governance_parent_v2(&schedule).unwrap();
    world.begin_bft_execution_tick(&context).unwrap();
    let action = command();
    let proof = signed(&world, &key, &action, 1);
    let mut invalid_candidate = world.clone();
    let mut tampered = action.clone();
    let Action::SubmitGovernanceValidatorAdmission { stake, .. } = &mut tampered else {
        unreachable!()
    };
    *stake += 1;
    invalid_candidate.submit_governance_action(tampered, proof.clone());
    let snapshot = invalid_candidate.snapshot();
    let journal = invalid_candidate.journal.clone();
    assert!(
        invalid_candidate
            .step_with_modules_for_committed_context(&mut Sandbox, &context)
            .is_err()
    );
    assert_eq!(invalid_candidate.snapshot(), snapshot);
    assert_eq!(invalid_candidate.journal, journal);
    assert_eq!(
        invalid_candidate
            .bft_governance_parent
            .as_ref()
            .unwrap()
            .execution,
        Some((context.clone(), false))
    );
    assert!(
        invalid_candidate
            .finish_bft_execution_tick(&context)
            .is_err()
    );

    // A fresh detached candidate from the same parent can still execute the
    // original signed action. A failed attempt grants no published authority.
    world.submit_governance_action(action, proof);
    world
        .step_with_modules_for_committed_context(&mut Sandbox, &context)
        .unwrap();
    world.finish_bft_execution_tick(&context).unwrap();
    assert_eq!(world.state.time, 1);
    assert_eq!(world.state.governance_authorization_nonces["controller"], 1);
}
