//! Protocol-only V3 header-QC checks rooted in a genuinely qualified H.
//!
//! The controlled H embeds the V2 target before it is frozen and signed. The
//! synthetic execution commitments below exercise header/QC identity and
//! schedule chaining only; they do not claim V3 World execution or driver
//! integration.
use super::*;
use ed25519_dalek::SigningKey;
use oasis7_formal_protocol::bft::{
    BFT_FORMAL_EXECUTION_DOMAIN_V1, BftHeaderHeightScheduleV3, BftProfileTargetV2,
    FormalBftCertifiedHeaderV3, FormalBftExecutionCommitmentV1, VerifiedBftHandoffTargetV2,
    verify_formal_bft_handoff_v2, verify_formal_bft_header_certificate_v3,
};
use oasis7_proto::distributed_bft::{
    v2::{BftProposalHeaderV2, BftVotePhaseV2, BftVoteV2, BftVoteValueV2},
    v3::{
        BftBlockHeaderV3, BftHeaderCommitCertificateV3, BftProposalHeaderV3,
        BftValidRoundEvidenceV3, BftVotePhaseV3, BftVoteV3, BftVoteValueV3, actions_cbor_digest_v3,
    },
};

const EMPTY_ACTIONS_CBOR: &[u8] = &[0x80];

#[path = "driver_bft_v3_formal_cli_tests.rs"]
mod cli_execution_tests;
#[path = "driver_bft_v3_formal_execution_tests.rs"]
mod execution_tests;
#[path = "driver_bft_v3_formal_ledger_tests.rs"]
mod ledger_tests;
#[path = "driver_bft_v3_header_transition_tests.rs"]
mod transition_tests;

pub(super) fn qualified_handoff_v3() -> QualifiedH<BftProfileTargetV2> {
    qualified_handoff_with_target_and_setup(
        102,
        |_| {},
        |scope, policy| {
            let validators = (0..4)
                .map(|index| {
                    let key = fixtures::key(120 + index);
                    BftValidatorV2 {
                        validator_id: validator_id(index),
                        public_key_hex: hex::encode(key.verifying_key().to_bytes()),
                        voting_power: 1,
                    }
                })
                .collect();
            let target = BftProfileTargetV2::new(scope, 1, validators, policy)?;
            let handoff = ProfileHandoffV1 {
                next_profile: target.profile.clone(),
                next_profile_version: target.profile_version,
                next_authority_digest: target.target_digest()?,
                valid_from_position: 3,
            };
            Ok((target, handoff))
        },
    )
}

pub(super) fn qualified_handoff_with_target_and_setup<T>(
    controller_seed: u8,
    setup: impl FnOnce(&mut RuntimeWorld),
    make_target: impl FnOnce(BftScopeV2, BftSchedulePolicyV2) -> Result<(T, ProfileHandoffV1), String>,
) -> QualifiedH<T> {
    let (_, mut trust, template, _) = fixtures::actual_world_initial_fixture();
    let identity = template.authority().definition().world.clone();
    let mut world = wasm_fixture::world_with_formal_history();
    let controller = fixtures::key(controller_seed);
    world
        .set_governance_main_token_controller_registry(GovernanceMainTokenControllerRegistry {
            genesis_controller_account_id: "formal-bft-controller".into(),
            controller_signer_policies: BTreeMap::from([(
                "formal-bft-controller".into(),
                GovernanceThresholdSignerPolicy {
                    threshold: 1,
                    allowed_public_keys: [hex::encode(controller.verifying_key().to_bytes())]
                        .into(),
                },
            )]),
            ..Default::default()
        })
        .expect("set the H-committed governance controller registry");
    setup(&mut world);
    if world.state().time == 0 {
        world.step().expect("prepare nonzero formal H boundary");
    }
    assert_eq!(world.state().time, 1);

    let release = ReleaseSecurityPolicy::default();
    let bootstrap_snapshot = serde_cbor::to_vec(&world.snapshot()).unwrap();
    let bootstrap = formal_initial::InitialNodeBootstrapV1 {
        height: world.state().time,
        consensus_block_hash: fixtures::h(81),
        execution_block_hash: fixtures::h(82),
        execution_state_root: blake3_hex(&bootstrap_snapshot),
    };
    let closure = formal_initial::InitialWorldClosure::from_existing_world_with_node_bootstrap(
        &world, &identity, &release, bootstrap,
    )
    .expect("prepare profile-v2 activation at tick one");
    let mut definition = template.authority().definition().clone();
    definition.profile_version = 2;
    let policy = template.authority().policy().clone();
    let body = closure
        .activation_body(definition, policy.clone())
        .expect("profile-v2 activation body");
    trust.world = identity.clone();
    trust.initial_state_root = body.initial_state_root.clone();
    trust.governing_manifest_digest = body.governing_manifest_digest.clone();
    trust.recovery_policy_digest = policy.digest().unwrap();
    let certificate = SignedInitialActivationV1 {
        signature_hex: fixtures::sign(initial_activation_signing_bytes(&body).unwrap(), 9),
        body,
    };
    let writer = fixtures::key(4);
    let (authority, initial) = closure
        .authorize(&certificate, &trust, &writer)
        .expect("authorize real profile-v2 initial closure");
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
    let initial_anchor =
        VerifiedFormalReadAnchorV1::from_initial(&certificate, &trust, &initial_proof).unwrap();
    let initial_tree =
        FormalBusinessResultTreeV1::for_initial_world(&world, &identity, &EmptyIndexReader)
            .expect("initial formal business tree");
    assert_eq!(
        initial_tree.tree.root().unwrap(),
        initial.candidate.record.result_root
    );
    let reader = Arc::new(IndexReader(
        initial_tree
            .delta
            .node_writes
            .iter()
            .map(|write| (write.digest.clone(), write.bytes.clone()))
            .collect(),
    ));
    let snapshot = world.snapshot();
    let snapshot_bytes = serde_cbor::to_vec(&snapshot).unwrap();
    let retained = VerifiedFormalRetainedHistoryParentV1::verify(&initial_anchor, &snapshot_bytes)
        .expect("qualified profile-v2 retained history");
    world = RuntimeWorld::from_verified_formal_snapshot(
        snapshot,
        world.journal().clone(),
        retained,
        reader.clone(),
    )
    .unwrap()
    .with_release_security_policy(release.clone());

    let scope = BftScopeV2 {
        world_id: identity.world_id.clone(),
        chain_id: identity.chain_id.clone(),
        genesis_hash: identity.genesis_digest.clone(),
    };
    let target_policy = BftSchedulePolicyV2::new(
        world.governance_execution_policy().epoch_length_ticks,
        "bft",
    )
    .unwrap();
    let (target, handoff) = make_target(scope, target_policy)
        .expect("construct the version-specific BFT target before signing H");
    let continuation = continuation(&world, &initial_anchor);
    let (mut frozen_body, frozen_material) = formal_package::freeze_material_from_parent(
        &authority,
        initial_anchor.head(),
        &continuation,
        &world,
        &release,
        initial_tree.tree,
        initial_proof.clone(),
        None,
        &initial.material,
    )
    .expect("freeze H execution closure");
    frozen_body.handoff = Some(handoff);
    let frozen = SignedFormalFrozenV1 {
        signature_hex: fixtures::sign(frozen_signing_bytes(&frozen_body).unwrap(), 4),
        body: frozen_body,
    };
    let endpoint = |role| FreezeEndpoint {
        authority: authority.clone(),
        parent: initial_anchor.head().clone(),
        role,
    };
    let mut coordinator = FormalCoordinator::new(
        endpoint(EndpointRole::Primary),
        endpoint(EndpointRole::Replica),
        authority.clone(),
        authority.clone(),
        None,
        Some(initial_anchor.head().clone()),
    )
    .unwrap();
    let executable = coordinator.freeze(frozen, frozen_material).unwrap();
    let scratch = Scratch::new();
    let shared_reader = formal_business_package::SharedBusinessReader(reader.clone());
    let candidate = formal_executor::execute_normal_with_business_reader(
        &executable,
        &authority,
        initial_anchor.head(),
        scratch.path(),
        &writer,
        Some(&shared_reader),
        Some(&initial_anchor),
    )
    .expect("execute H against the actual World");
    verify_formal_candidate(
        &candidate.candidate,
        candidate.frozen.as_ref(),
        &authority,
        Some(initial_anchor.head()),
        None,
        FormalCompletionBranchV1::Normal,
    )
    .unwrap();
    let h_proof = fixtures::qualified_proof(
        candidate.candidate.record.clone(),
        candidate.frozen.clone(),
        &authority,
        Some(initial_anchor.head()),
        None,
        4,
        5,
        6,
    );
    let committed =
        verify_formal_qualified(&h_proof, &authority, Some(initial_anchor.head()), None).unwrap();
    let anchor = initial_anchor
        .after_verified_commit(authority, committed)
        .unwrap();
    assert_eq!(anchor.head().record().position, 2);
    assert!(anchor.head().record().handoff.is_some());

    let envelope = formal_business_package::envelope(&candidate.material, true)
        .unwrap()
        .unwrap();
    let mut nodes = reader.0.clone();
    nodes.extend(
        envelope
            .delta
            .node_writes
            .iter()
            .map(|write| (write.digest.clone(), write.bytes.clone())),
    );
    let reader = Arc::new(IndexReader(nodes));
    let checkpoint = anchor.checkpoint();
    let read_trust = FormalReadTrustConfigV1 {
        initial_trust: trust,
        checkpoint_digest: checkpoint.digest().unwrap(),
        checkpoint,
    };
    QualifiedH {
        anchor,
        trust: read_trust,
        material: candidate.material,
        target,
        world,
        reader,
    }
}

fn validator_id(index: u8) -> String {
    format!("validator-{}", index + 1)
}

#[test]
fn wire3_target_is_committed_by_real_h_and_wire2_target_cannot_reuse_it() {
    std::thread::Builder::new()
        .name("formal-bft-v3-handoff-target".into())
        .stack_size(32 * 1024 * 1024)
        .spawn(wire3_target_case)
        .unwrap()
        .join()
        .unwrap();
}

fn wire3_target_case() {
    let h = qualified_handoff_v3();
    let target_bytes = h.target.canonical_bytes().unwrap();
    assert_eq!(
        BftProfileTargetV2::decode_canonical(&target_bytes).unwrap(),
        h.target
    );
    assert_eq!(h.target.schema_version, 2);
    assert_eq!(h.target.profile, "bft");
    assert_eq!(h.target.profile_version, 1);
    assert_eq!(h.target.wire_version, 3);
    assert_eq!(
        h.anchor
            .head()
            .record()
            .handoff
            .as_ref()
            .unwrap()
            .next_authority_digest,
        h.target.target_digest().unwrap()
    );

    let handoff = verify_formal_bft_handoff_v2(&h.anchor, h.target.clone())
        .expect("real qualified H names the exact canonical wire-3 target");
    assert_eq!(handoff.handoff_position(), h.anchor.head().head().position);
    assert_eq!(
        handoff.activation_height().unwrap(),
        h.anchor.head().head().position + 1
    );
    let schedule = initial_schedule(&handoff);
    assert_eq!(schedule.height(), handoff.activation_height().unwrap());
    assert_eq!(
        schedule.parent_commit_ref(),
        &h.anchor.head().head().record_id
    );

    let old_target = BftProfileTargetV1::new(
        h.target.scope.clone(),
        h.target.authority_epoch,
        h.target.initial_validators.clone(),
        h.target.schedule_policy.clone(),
    )
    .unwrap();
    assert_ne!(
        old_target.target_digest().unwrap(),
        h.target.target_digest().unwrap()
    );
    assert!(verify_formal_bft_handoff_v1(&h.anchor, old_target).is_err());

    let mut wrong_wire = h.target.clone();
    wrong_wire.wire_version = 2;
    assert!(wrong_wire.validate().is_err());
}

#[test]
fn signed_header_qcs_chain_three_heights_and_accept_valid_round_evidence() {
    let h = qualified_handoff_v3();
    let handoff = verify_formal_bft_handoff_v2(&h.anchor, h.target.clone()).unwrap();
    let mut schedule = initial_schedule(&handoff);
    let first_height = schedule.height();

    for offset in 0..3 {
        let commitment = commitment_for(&schedule, &handoff, offset);
        let round = if offset == 0 { 1 } else { 0 };
        let valid_round = (offset == 0).then_some(0);
        let entry = signed_entry(&schedule, commitment, round, valid_round, &[0, 1, 2], None);
        let encoded = entry.canonical_bytes().unwrap();
        assert_eq!(
            FormalBftCertifiedHeaderV3::decode_canonical(&encoded).unwrap(),
            entry
        );
        let token = verify_formal_bft_header_certificate_v3(&schedule, &entry)
            .expect("weighted V3 QC advances the exact scheduled header");
        assert_eq!(token.certified_header(), &entry);
        assert_eq!(token.schedule().height(), first_height + offset);
        assert_eq!(
            token.next_height_schedule().height(),
            first_height + offset + 1
        );
        assert_eq!(
            token.next_height_schedule().parent_commit_ref(),
            token.block_id()
        );
        schedule = token.next_height_schedule().clone();
    }
}

#[test]
fn v3_qc_rejects_insufficient_duplicate_wrong_policy_and_wrong_proposer_key() {
    let h = qualified_handoff_v3();
    let handoff = verify_formal_bft_handoff_v2(&h.anchor, h.target.clone()).unwrap();
    let schedule = initial_schedule(&handoff);

    let under_quorum = signed_entry(
        &schedule,
        commitment_for(&schedule, &handoff, 0),
        0,
        None,
        &[0, 1],
        None,
    );
    for vote in &under_quorum.certificate.precommits {
        vote.verify_signature().unwrap();
    }
    let error = verify_formal_bft_header_certificate_v3(&schedule, &under_quorum).unwrap_err();
    assert!(error.contains("lacks quorum"));

    let duplicate = signed_entry(
        &schedule,
        commitment_for(&schedule, &handoff, 0),
        0,
        None,
        &[0, 0, 1],
        None,
    );
    for vote in &duplicate.certificate.precommits {
        vote.verify_signature().unwrap();
    }
    let error = verify_formal_bft_header_certificate_v3(&schedule, &duplicate).unwrap_err();
    assert!(error.contains("repeats a validator"));

    let mut wrong_policy = commitment_for(&schedule, &handoff, 0);
    wrong_policy.schedule_policy_hash = hash("other-schedule-policy");
    let wrong_policy = signed_entry(&schedule, wrong_policy, 0, None, &[0, 1, 2], None);
    wrong_policy
        .certificate
        .proposal
        .verify_signature()
        .unwrap();
    for vote in &wrong_policy.certificate.precommits {
        vote.verify_signature().unwrap();
    }
    assert!(verify_formal_bft_header_certificate_v3(&schedule, &wrong_policy).is_err());

    let wrong_key = signed_entry(
        &schedule,
        commitment_for(&schedule, &handoff, 0),
        0,
        None,
        &[0, 1, 2],
        Some(fixtures::key(127)),
    );
    wrong_key.certificate.proposal.verify_signature().unwrap();
    assert!(
        verify_formal_bft_header_certificate_v3(&schedule, &wrong_key)
            .unwrap_err()
            .contains("key does not match")
    );
}

#[test]
fn v3_qc_binds_activation_parent_header_round_phase_and_signature_domain() {
    let h = qualified_handoff_v3();
    let handoff = verify_formal_bft_handoff_v2(&h.anchor, h.target.clone()).unwrap();
    let schedule = initial_schedule(&handoff);
    let base_commitment = commitment_for(&schedule, &handoff, 0);

    let mut wrong_parent = base_commitment.clone();
    wrong_parent.parent_semantic_state_root = hash("wrong-parent-semantic-root");
    let wrong_parent = signed_entry(&schedule, wrong_parent, 0, None, &[0, 1, 2], None);
    assert!(
        verify_formal_bft_header_certificate_v3(&schedule, &wrong_parent)
            .unwrap_err()
            .contains("does not extend its admitted parent")
    );

    let mut wrong_activation = base_commitment.clone();
    wrong_activation.activation_digest = hash("another-activation");
    let wrong_activation = signed_entry(&schedule, wrong_activation, 0, None, &[0, 1, 2], None);
    assert!(verify_formal_bft_header_certificate_v3(&schedule, &wrong_activation).is_err());

    let valid_round = signed_entry(
        &schedule,
        base_commitment.clone(),
        1,
        Some(0),
        &[0, 1, 2],
        None,
    );
    verify_formal_bft_header_certificate_v3(&schedule, &valid_round)
        .expect("round-one proposal carries signed round-zero prevote evidence");

    let mut wrong_round = valid_round.certificate.clone();
    let evidence = wrong_round.valid_round_evidence.as_mut().unwrap();
    let voter_id = evidence.prevotes[0].validator_id.clone();
    evidence.prevotes[0] = signed_vote(
        &schedule,
        &base_commitment,
        &valid_round.header.block_id().unwrap(),
        &voter_id,
        1,
        BftVotePhaseV3::Prevote,
        &key_for_id(&voter_id),
    );
    evidence.prevotes[0].verify_signature().unwrap();
    assert!(
        FormalBftCertifiedHeaderV3::new(
            wrong_round,
            valid_round.header.clone(),
            base_commitment.clone(),
        )
        .is_err()
    );

    let mut wrong_phase = valid_round.certificate.clone();
    let evidence = wrong_phase.valid_round_evidence.as_mut().unwrap();
    let voter_id = evidence.prevotes[0].validator_id.clone();
    evidence.prevotes[0] = signed_vote(
        &schedule,
        &base_commitment,
        &valid_round.header.block_id().unwrap(),
        &voter_id,
        0,
        BftVotePhaseV3::Precommit,
        &key_for_id(&voter_id),
    );
    evidence.prevotes[0].verify_signature().unwrap();
    assert!(
        FormalBftCertifiedHeaderV3::new(
            wrong_phase,
            valid_round.header.clone(),
            base_commitment.clone(),
        )
        .is_err()
    );

    let wrong_header_id_certificate = valid_round.certificate.clone();
    wrong_header_id_certificate
        .proposal
        .verify_signature()
        .unwrap();
    let mut wrong_header = valid_round.header.clone();
    wrong_header.actions_cbor_digest = hash("another-private-payload");
    assert!(
        FormalBftCertifiedHeaderV3::new(
            wrong_header_id_certificate,
            wrong_header,
            base_commitment.clone(),
        )
        .is_err()
    );

    let wrong_commitment_hash_certificate = valid_round.certificate.clone();
    let mut wrong_commitment_hash_header = valid_round.header.clone();
    wrong_commitment_hash_header.execution_commitment_hash = hash("another-execution-preimage");
    assert!(
        FormalBftCertifiedHeaderV3::new(
            wrong_commitment_hash_certificate,
            wrong_commitment_hash_header,
            base_commitment.clone(),
        )
        .is_err()
    );

    let mut legacy_proposal = valid_round.clone();
    let proposal = &legacy_proposal.certificate.proposal;
    let old_proposal = BftProposalHeaderV2::sign(
        proposal.scope.clone(),
        proposal.height,
        proposal.round,
        proposal.active_validator_set_hash.clone(),
        proposal.schedule_policy_hash.clone(),
        proposal.proposer_id.clone(),
        proposal.block_id.clone(),
        proposal.valid_round,
        &key_for_id(&proposal.proposer_id),
    )
    .unwrap();
    old_proposal.verify_signature().unwrap();
    legacy_proposal.certificate.proposal.signature_hex = old_proposal.signature_hex;
    assert!(verify_formal_bft_header_certificate_v3(&schedule, &legacy_proposal).is_err());

    let mut legacy_vote = valid_round.clone();
    let vote = &legacy_vote.certificate.precommits[0];
    let old_vote = BftVoteV2::sign(
        vote.scope.clone(),
        vote.height,
        vote.round,
        BftVotePhaseV2::Precommit,
        vote.active_validator_set_hash.clone(),
        vote.schedule_policy_hash.clone(),
        BftVoteValueV2::Block {
            block_id: match &vote.value {
                BftVoteValueV3::Block { block_id } => block_id.clone(),
                BftVoteValueV3::Nil => unreachable!("fixture vote commits a block"),
            },
        },
        vote.validator_id.clone(),
        &key_for_id(&vote.validator_id),
    )
    .unwrap();
    old_vote.verify_signature().unwrap();
    legacy_vote.certificate.precommits[0].signature_hex = old_vote.signature_hex;
    assert!(verify_formal_bft_header_certificate_v3(&schedule, &legacy_vote).is_err());
}

fn initial_schedule(handoff: &VerifiedBftHandoffTargetV2) -> BftHeaderHeightScheduleV3 {
    // This test verifies the H-bound target and protocol schedule/QC rules.
    // Its replay roots are explicit protocol fixtures, not a claim that a V3
    // driver has restored H or executed these synthetic successor commitments.
    handoff
        .initial_schedule_after_handoff_replay(
            &hash("protocol-only-replayed-H-semantic-root"),
            &hash("protocol-only-replayed-H-world-manifest"),
        )
        .unwrap()
}

fn commitment_for(
    schedule: &BftHeaderHeightScheduleV3,
    handoff: &VerifiedBftHandoffTargetV2,
    offset: u64,
) -> FormalBftExecutionCommitmentV1 {
    let height = schedule.height();
    let root = |label: &str| hash(&format!("{label}-height-{height}-offset-{offset}"));
    FormalBftExecutionCommitmentV1 {
        domain: BFT_FORMAL_EXECUTION_DOMAIN_V1.into(),
        schema_version: 1,
        profile: "bft".into(),
        profile_version: 1,
        activation_digest: handoff.activation_digest().into(),
        scope: schedule.scope().clone(),
        height,
        parent_commit_ref: schedule.parent_commit_ref().into(),
        parent_tick: schedule.parent_tick(),
        parent_semantic_state_root: schedule.parent_state_root().into(),
        authority_epoch: schedule.authority_epoch(),
        active_validator_set_hash: schedule.active_set().set_hash().into(),
        schedule_policy_hash: schedule.policy_hash().into(),
        manifest_hash: schedule.parent_manifest_hash().into(),
        post_manifest_hash: root("post-manifest"),
        action_root: root("action"),
        event_root: root("event"),
        execution_state_root: root("execution-state"),
        state_root: root("semantic-state"),
        projection_root: root("projection"),
        result_root: root("business-result"),
        transition: None,
        transition_ref: None,
    }
}

fn signed_entry(
    schedule: &BftHeaderHeightScheduleV3,
    commitment: FormalBftExecutionCommitmentV1,
    round: u64,
    valid_round: Option<u64>,
    precommit_indices: &[u8],
    proposer_key_override: Option<SigningKey>,
) -> FormalBftCertifiedHeaderV3 {
    let header = BftBlockHeaderV3 {
        protocol_version: 3,
        execution_commitment_hash: commitment.digest().unwrap(),
        actions_cbor_digest: actions_cbor_digest_v3(EMPTY_ACTIONS_CBOR).unwrap(),
    };
    let block_id = header.block_id().unwrap();
    let proposer_id = schedule.expected_proposer_for_round(round).unwrap();
    let proposal_key = proposer_key_override.unwrap_or_else(|| key_for_id(&proposer_id));
    let proposal = BftProposalHeaderV3::sign(
        commitment.scope.clone(),
        commitment.height,
        round,
        commitment.active_validator_set_hash.clone(),
        commitment.schedule_policy_hash.clone(),
        proposer_id,
        block_id.clone(),
        valid_round,
        &proposal_key,
    )
    .unwrap();
    let valid_round_evidence = valid_round.map(|valid_round| {
        let old_proposer = schedule.expected_proposer_for_round(valid_round).unwrap();
        let old_proposal = BftProposalHeaderV3::sign(
            commitment.scope.clone(),
            commitment.height,
            valid_round,
            commitment.active_validator_set_hash.clone(),
            commitment.schedule_policy_hash.clone(),
            old_proposer.clone(),
            block_id.clone(),
            None,
            &key_for_id(&old_proposer),
        )
        .unwrap();
        BftValidRoundEvidenceV3 {
            proposal: old_proposal,
            prevotes: [0, 1, 2]
                .into_iter()
                .map(|index| {
                    signed_vote(
                        schedule,
                        &commitment,
                        &block_id,
                        &validator_id(index),
                        valid_round,
                        BftVotePhaseV3::Prevote,
                        &key_for_id(&validator_id(index)),
                    )
                })
                .collect(),
        }
    });
    let precommits = precommit_indices
        .iter()
        .map(|index| {
            let id = validator_id(*index);
            signed_vote(
                schedule,
                &commitment,
                &block_id,
                &id,
                round,
                BftVotePhaseV3::Precommit,
                &key_for_id(&id),
            )
        })
        .collect();
    FormalBftCertifiedHeaderV3::new(
        BftHeaderCommitCertificateV3 {
            proposal,
            valid_round_evidence,
            precommits,
        },
        header,
        commitment,
    )
    .unwrap()
}

fn signed_vote(
    schedule: &BftHeaderHeightScheduleV3,
    commitment: &FormalBftExecutionCommitmentV1,
    block_id: &str,
    validator_id: &str,
    round: u64,
    phase: BftVotePhaseV3,
    key: &SigningKey,
) -> BftVoteV3 {
    BftVoteV3::sign(
        schedule.scope().clone(),
        schedule.height(),
        round,
        phase,
        commitment.active_validator_set_hash.clone(),
        commitment.schedule_policy_hash.clone(),
        BftVoteValueV3::Block {
            block_id: block_id.into(),
        },
        validator_id.into(),
        key,
    )
    .unwrap()
}

fn key_for_id(validator_id: &str) -> SigningKey {
    let one_based = validator_id
        .strip_prefix("validator-")
        .expect("fixture validator prefix")
        .parse::<u8>()
        .expect("fixture validator number");
    fixtures::key(119 + one_based)
}

fn hash(label: &str) -> String {
    blake3::hash(label.as_bytes()).to_hex().to_string()
}
