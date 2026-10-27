use super::*;
use oasis7_formal_protocol::bft::{
    FormalBftCertifiedEntryV1, FormalBftCertifiedPrefixV1, VerifiedFormalBftPrefixV1,
};

fn from_first_cut(
    execution: &FormalBftReadExecutionV1,
    cut: &super::super::FormalBftExecutionCutV1,
) -> VerifiedFormalBftPrefixV1 {
    FormalBftCertifiedPrefixV1 {
        schema_version: 1,
        entries: vec![FormalBftCertifiedEntryV1 {
            certificate: cut.certificate().clone(),
            commitment: cut.commitment().clone(),
        }],
    }
    .verify(&execution.handoff)
    .unwrap()
}

#[test]
fn formal_prefix_executes_the_next_real_world_without_reusing_the_h_parent() {
    std::thread::Builder::new()
        .stack_size(32 * 1024 * 1024)
        .spawn(real_successor_case)
        .unwrap()
        .join()
        .unwrap();
}

fn real_successor_case() {
    let h = qualified_handoff();
    let scratch = Scratch::new();
    let mut execution = open_formal_read(&h, &scratch, h.reader.clone(), true).unwrap();
    let block = execution.build_candidate(Vec::new()).unwrap();
    let cert = certificate(block, execution.schedule());
    let cut = execution.commit_certificate(cert).unwrap();
    let before = cut.world().snapshot();
    let prefix = from_first_cut(&execution, &cut);
    let schedule = prefix.next_height_schedule();
    assert_eq!(schedule.height(), 4);

    let mut rejected = cut.world().clone();
    assert!(
        rejected
            .attach_formal_bft_handoff_parent_v1(&execution.handoff, schedule)
            .is_err()
    );
    assert!(rejected.attach_bft_governance_parent_v2(schedule).is_err());
    assert_eq!(rejected.snapshot(), before);
    let mut attached = cut.world().clone();
    attached
        .attach_formal_bft_prefix_parent_v1(&prefix)
        .unwrap();
    let authorization = attached
        .governance_authorization_for_next_commit(1)
        .unwrap();
    assert_eq!(
        authorization.parent_record_id,
        cut.certificate().proposal.block.block_id().unwrap()
    );
    assert_eq!(
        authorization.parent_state_root,
        cut.commitment().state_root()
    );
    assert_eq!(authorization.parent_position, 3);
    let mut queued = cut.world().clone();
    queued.submit_action(action());
    let error = queued
        .attach_formal_bft_prefix_parent_v1(&prefix)
        .unwrap_err();
    assert!(format!("{error:?}").contains("uncommitted queues"));
    assert_eq!(queued.pending_actions_len(), 1);

    let mut next = cut.certificate().proposal.block.clone();
    next.height = schedule.height();
    next.parent_commit_ref = schedule.parent_commit_ref().into();
    next.active_validator_set_hash = schedule.active_set().set_hash().into();
    next.authority_epoch = schedule.authority_epoch();
    next.manifest_hash = schedule.parent_manifest_hash().into();
    let input =
        super::super::formal_execution_input_hash(&execution.handoff, schedule, &next).unwrap();
    let result = super::super::super::execution::execute_world_transition(
        super::super::super::execution::WorldTransitionRequest {
            parent: cut.world(),
            scope: execution.target.scope(),
            authority_epoch: schedule.authority_epoch(),
            schedule,
            block: &next,
            expected_parent: schedule.parent_commit_ref(),
            input_hash: &input,
            tick_authority: super::super::FORMAL_BFT_EXECUTION_TICK_AUTHORITY,
            formal_handoff: None,
            formal_prefix: Some(&prefix),
        },
    )
    .expect("the shared real World driver must execute H+2 from the certified H+1 parent");
    assert_eq!(result.world.state().time, 4);
    assert_eq!(
        result.world.state().retained_history_root,
        before.state.retained_history_root
    );
    assert_eq!(result.world.pending_actions_len(), 0);
    assert_eq!(result.world.pending_effects_len(), 0);
    assert_eq!(
        cut.world().snapshot(),
        before,
        "preview never changes the committed parent"
    );
    assert_eq!(execution.ledger.record().unwrap().height, 3);
    assert!(
        execution.verified_cut(4).is_err(),
        "the V1 fixed-cut ledger is not reinterpreted"
    );
}

fn action() -> Action {
    Action::SubmitGovernanceValidatorAdmission {
        controller_account_id: "formal-bft-controller".into(),
        candidate_id: "prefix-parent-policy".into(),
        node_id: "prefix-node".into(),
        finality_signer_public_key: hex::encode(fixtures::key(201).verifying_key().to_bytes()),
        stake: 25,
        operator_owner: "prefix-operator".into(),
        public_manifest_hash: "31".repeat(32),
    }
}

fn proof(
    world: &RuntimeWorld,
    key: &ed25519_dalek::SigningKey,
) -> oasis7_agent_api::world_action::GovernanceAuthorizationV1 {
    let mut proof = world.governance_authorization_for_next_commit(1).unwrap();
    proof.signatures.insert(
        hex::encode(key.verifying_key().to_bytes()),
        hex::encode(
            key.sign(&proof.signing_bytes(&action()).unwrap())
                .to_bytes(),
        ),
    );
    proof
}

#[test]
fn formal_prefix_parent_policy_is_snapshot_bound_and_captured_before_mutation() {
    std::thread::Builder::new()
        .stack_size(32 * 1024 * 1024)
        .spawn(parent_policy_case)
        .unwrap()
        .join()
        .unwrap();
}

fn parent_policy_case() {
    let h = qualified_handoff();
    let scratch = Scratch::new();
    let mut execution = open_formal_read(&h, &scratch, h.reader.clone(), true).unwrap();
    let block = execution.build_candidate(Vec::new()).unwrap();
    let cert = certificate(block, execution.schedule());
    let cut = execution.commit_certificate(cert).unwrap();
    let original_prefix = from_first_cut(&execution, &cut);
    let replacement = fixtures::key(203);
    let mut changed = cut.world().clone();
    let mut registry = changed
        .governance_main_token_controller_registry()
        .unwrap()
        .clone();
    registry
        .controller_signer_policies
        .get_mut("formal-bft-controller")
        .unwrap()
        .allowed_public_keys = [hex::encode(replacement.verifying_key().to_bytes())].into();
    changed
        .set_governance_main_token_controller_registry(registry.clone())
        .unwrap();
    let drifted = changed.snapshot();
    assert!(
        changed
            .attach_formal_bft_prefix_parent_v1(&original_prefix)
            .is_err()
    );
    assert_eq!(
        changed.snapshot(),
        drifted,
        "a rejected drift cannot install execution authority"
    );

    let fixture = certified_fixture_parent(&execution, &cut, &changed);
    changed
        .attach_formal_bft_prefix_parent_v1(&fixture)
        .unwrap();
    let authorized = proof(&changed, &replacement);
    changed
        .verify_governance_action_for_next_commit(&action(), &authorized)
        .unwrap();
    assert!(
        changed
            .verify_governance_action_for_next_commit(
                &action(),
                &proof(&changed, &fixtures::key(102))
            )
            .is_err(),
        "the H controller key must not authorize against a different certified parent policy"
    );

    let later = fixtures::key(204);
    registry
        .controller_signer_policies
        .get_mut("formal-bft-controller")
        .unwrap()
        .allowed_public_keys = [hex::encode(later.verifying_key().to_bytes())].into();
    changed
        .set_governance_main_token_controller_registry(registry)
        .unwrap();
    assert!(
        changed
            .verify_governance_action_for_next_commit(&action(), &proof(&changed, &later))
            .is_err()
    );
    changed
        .verify_governance_action_for_next_commit(&action(), &authorized)
        .unwrap();
    assert!(
        changed
            .begin_bft_execution_tick(&RuntimeCommittedTickContext {
                height: fixture.next_height_schedule().height(),
                slot: 3,
                epoch: fixture.next_height_schedule().authority_epoch(),
                node_block_hash: "41".repeat(32),
                action_root: "42".repeat(32),
                authority_node_id: changed.tick_consensus_authority_source().into(),
                committed_at_unix_ms: 0,
            })
            .is_err(),
        "post-attachment parent drift must still prevent execution"
    );
}

// Protocol-only signed claims, NOT a replayed cut. Every use also checks the
// real driver rejects this World, so this helper cannot stand in for execution.
fn certified_fixture_parent(
    execution: &FormalBftReadExecutionV1,
    cut: &super::super::FormalBftExecutionCutV1,
    changed: &RuntimeWorld,
) -> VerifiedFormalBftPrefixV1 {
    let mut commitment = cut.commitment().clone();
    commitment.execution_state_root =
        blake3_hex(&oasis7_world_state::codec::snapshot_cbor(&changed.snapshot()).unwrap());
    commitment.state_root = changed.execution_state_identity().unwrap().state_root_hash;
    commitment.post_manifest_hash = changed.current_manifest_hash().unwrap();
    let mut block = cut.certificate().proposal.block.clone();
    block.execution_state_root = commitment.execution_state_root.clone();
    block.state_root = commitment.state_root.clone();
    block.post_manifest_hash = commitment.post_manifest_hash.clone();
    block.execution_block_hash = commitment.digest().unwrap();
    assert!(
        execution.preview_candidate(&block).is_err(),
        "QC-bound claims do not replace actual World replay"
    );
    FormalBftCertifiedPrefixV1 {
        schema_version: 1,
        entries: vec![FormalBftCertifiedEntryV1 {
            certificate: certificate(block, execution.schedule()),
            commitment,
        }],
    }
    .verify(&execution.handoff)
    .unwrap()
}

#[test]
fn formal_prefix_parent_requires_actual_world_scope() {
    std::thread::Builder::new()
        .stack_size(32 * 1024 * 1024)
        .spawn(|| {
            let h = qualified_handoff();
            let scratch = Scratch::new();
            let mut execution = open_formal_read(&h, &scratch, h.reader.clone(), true).unwrap();
            let block = execution.build_candidate(Vec::new()).unwrap();
            let cut = execution
                .commit_certificate(certificate(block, execution.schedule()))
                .unwrap();
            let manifest = cut.world().chain_resource_manifest().clone();
            let snapshot = cut.world().snapshot_with_chain_resource_context(
                oasis7_runtime::ChainResourceDerivationContext {
                    world_id: "different-prefix-world",
                    chain_id: &manifest.chain_id,
                    genesis_ref: manifest.genesis_ref.as_deref(),
                    created_at_height: manifest.created_at_height,
                    manifest_height: manifest.manifest_height,
                    commit_block_hash: manifest.created_at_block_hash.as_deref(),
                    tick: cut.world().state().time,
                },
                manifest.world_config_hash,
                manifest.generation_algorithm_hash,
            );
            let mut wrong = cut
                .world()
                .restore_candidate_snapshot_with_attached_formal_history(
                    snapshot,
                    cut.world().journal().clone(),
                )
                .unwrap();
            let before = wrong.snapshot();
            let fixture = certified_fixture_parent(&execution, &cut, &wrong);
            assert_ne!(
                wrong.chain_resource_manifest().world_id,
                fixture.handoff().scope().world_id
            );
            wrong
                .attach_formal_bft_prefix_parent_v1(&fixture)
                .expect_err("a signed protocol claim cannot authorize a different actual World");
            assert_eq!(wrong.snapshot(), before);
        })
        .unwrap()
        .join()
        .unwrap();
}
