use super::*;
use std::time::Instant;

fn key(n: u8) -> SigningKey {
    SigningKey::from_bytes(&[n; 32])
}

fn public(n: u8) -> String {
    hex::encode(key(n).verifying_key().as_bytes())
}

fn hash(n: u8) -> String {
    hex::encode([n; 32])
}

fn sequence_id(domain: u8, value: u64) -> String {
    let mut bytes = [0u8; 32];
    bytes[0] = domain;
    bytes[24..].copy_from_slice(&value.to_be_bytes());
    hex::encode(bytes)
}

fn sequence_transport_nonce(value: u64) -> [u8; 32] {
    let mut nonce = [0u8; 32];
    nonce[24..].copy_from_slice(&value.to_be_bytes());
    nonce
}

fn world() -> FormalWorldIdentityV1 {
    FormalWorldIdentityV1 {
        world_id: "issuer-ledger-world".into(),
        chain_id: "test-chain".into(),
        genesis_digest: hash(31),
    }
}

fn trust() -> InitialActivationTrustV1 {
    InitialActivationTrustV1 {
        issuer_public_key: public(1),
        world: world(),
        initial_state_root: hash(32),
        governing_manifest_digest: hash(33),
        recovery_policy_digest: hash(34),
    }
}

fn signed_policy(
    generation: u64,
    issuer: u8,
    manager: u8,
    witness: u8,
) -> SignedFullSyncIssuerPolicyV2 {
    SignedFullSyncIssuerPolicyV2::sign(
        FullSyncIssuerPolicyBodyV2 {
            schema_version: 2,
            root_public_key: public(1),
            activation_trust_digest: digest(ACTIVATION_TRUST_DOMAIN, &trust(), MAX_ENTRY_BYTES)
                .unwrap(),
            world: world(),
            authority_epoch: 3,
            allowed_roles: vec![
                FullSyncReadRoleV2::StateSync,
                FullSyncReadRoleV2::FullReadOnly,
            ],
            management_public_key: public(manager),
            issuer_public_key: public(issuer),
            witness_public_key: public(witness),
            signer_generation: generation,
        },
        &key(1),
    )
    .unwrap()
}

fn verified_policy(
    generation: u64,
    issuer: u8,
    manager: u8,
    witness: u8,
) -> VerifiedFullSyncIssuerPolicyV2 {
    verified_policy_with_roles(
        generation,
        issuer,
        manager,
        witness,
        vec![
            FullSyncReadRoleV2::StateSync,
            FullSyncReadRoleV2::FullReadOnly,
        ],
    )
}

fn verified_policy_with_roles(
    generation: u64,
    issuer: u8,
    manager: u8,
    witness: u8,
    allowed_roles: Vec<FullSyncReadRoleV2>,
) -> VerifiedFullSyncIssuerPolicyV2 {
    let mut body = signed_policy(generation, issuer, manager, witness).body;
    body.allowed_roles = allowed_roles;
    let signed = SignedFullSyncIssuerPolicyV2::sign(body, &key(1)).unwrap();
    VerifiedFullSyncIssuerPolicyV2::verify(&signed, &trust()).unwrap()
}

fn target(node: u8, role: FullSyncReadRoleV2) -> FullSyncIssuerTargetV2 {
    FullSyncIssuerTargetV2 {
        node_public_key: public(node),
        role,
    }
}

fn sequence_target(node: u64, role: FullSyncReadRoleV2) -> FullSyncIssuerTargetV2 {
    let mut seed = [0u8; 32];
    seed[..8].copy_from_slice(b"nodekey!");
    seed[24..].copy_from_slice(&node.to_be_bytes());
    FullSyncIssuerTargetV2 {
        node_public_key: hex::encode(SigningKey::from_bytes(&seed).verifying_key().as_bytes()),
        role,
    }
}

fn command(
    state: &VerifiedFullSyncIssuerStateV2,
    policy: &VerifiedFullSyncIssuerPolicyV2,
    request: u8,
    target: FullSyncIssuerTargetV2,
    operation: FullSyncIssuerOperationV2,
    expected_generation: u64,
    transition: Option<FullSyncIssuerPolicyTransitionV2>,
) -> SignedFullSyncIssuerCommandV2 {
    SignedFullSyncIssuerCommandV2::sign(
        FullSyncIssuerCommandBodyV2 {
            schema_version: 2,
            world: policy.body().world.clone(),
            authority_epoch: policy.body().authority_epoch,
            scope_digest: policy.scope_digest().into(),
            policy_digest: policy.policy_digest().into(),
            signer_generation: policy.body().signer_generation,
            transition,
            target,
            operation,
            expected_head: state.anchor().clone(),
            expected_target_generation: expected_generation,
            request_id: hash(request),
            command_nonce: hash(request.wrapping_add(100)),
        },
        &key(if policy.body().signer_generation == 1 {
            2
        } else {
            9
        }),
    )
    .unwrap()
}

fn commit(
    state: &VerifiedFullSyncIssuerStateV2,
    command: SignedFullSyncIssuerCommandV2,
    policy: &VerifiedFullSyncIssuerPolicyV2,
    nonce: u8,
) -> VerifiedFullSyncIssuerDecisionV2 {
    let prepared = state.prepare(command).unwrap();
    let record = prepared.record().unwrap().clone();
    let query = SignedFullSyncIssuerQueryV2::sign(
        policy,
        record.previous_anchor.clone(),
        [nonce; 32],
        &key(if policy.body().signer_generation == 1 {
            3
        } else {
            8
        }),
    )
    .unwrap();
    let observation = SignedFullSyncIssuerObservationV2::sign(
        FullSyncIssuerObservationBodyV2 {
            schema_version: 2,
            query,
            current_anchor: record.previous_anchor.clone(),
        },
        &key(4),
    )
    .unwrap();
    let receipt = SignedFullSyncIssuerReceiptV2::sign(observation, &record, &key(4)).unwrap();
    prepared.commit(receipt).unwrap()
}

fn sequence_command(
    state: &VerifiedFullSyncIssuerStateV2,
    policy: &VerifiedFullSyncIssuerPolicyV2,
    sequence: u64,
    target: FullSyncIssuerTargetV2,
    operation: FullSyncIssuerOperationV2,
    expected_generation: u64,
) -> SignedFullSyncIssuerCommandV2 {
    SignedFullSyncIssuerCommandV2::sign(
        FullSyncIssuerCommandBodyV2 {
            schema_version: 2,
            world: policy.body().world.clone(),
            authority_epoch: policy.body().authority_epoch,
            scope_digest: policy.scope_digest().into(),
            policy_digest: policy.policy_digest().into(),
            signer_generation: policy.body().signer_generation,
            transition: None,
            target,
            operation,
            expected_head: state.anchor().clone(),
            expected_target_generation: expected_generation,
            request_id: sequence_id(1, sequence),
            command_nonce: sequence_id(2, sequence),
        },
        &key(2),
    )
    .unwrap()
}

fn commit_sequence(
    state: &VerifiedFullSyncIssuerStateV2,
    command: SignedFullSyncIssuerCommandV2,
    policy: &VerifiedFullSyncIssuerPolicyV2,
    sequence: u64,
) -> VerifiedFullSyncIssuerDecisionV2 {
    let prepared = state.prepare(command).unwrap();
    let record = prepared.record().unwrap().clone();
    let query = SignedFullSyncIssuerQueryV2::sign(
        policy,
        record.previous_anchor.clone(),
        sequence_transport_nonce(sequence),
        &key(3),
    )
    .unwrap();
    let observation = SignedFullSyncIssuerObservationV2::sign(
        FullSyncIssuerObservationBodyV2 {
            schema_version: 2,
            query,
            current_anchor: record.previous_anchor.clone(),
        },
        &key(4),
    )
    .unwrap();
    let receipt = SignedFullSyncIssuerReceiptV2::sign(observation, &record, &key(4)).unwrap();
    prepared.commit(receipt).unwrap()
}

fn issue_command(
    state: &VerifiedFullSyncIssuerStateV2,
    policy: &VerifiedFullSyncIssuerPolicyV2,
    request: u8,
    target: FullSyncIssuerTargetV2,
    expected_generation: u64,
) -> SignedFullSyncIssuerCommandV2 {
    command(
        state,
        policy,
        request,
        target,
        FullSyncIssuerOperationV2::Issue {
            not_before_unix_ms: 100,
            expires_unix_ms: 10_000,
        },
        expected_generation,
        None,
    )
}

#[test]
fn issue_retry_is_byte_exact_and_witness_records_close_observed_head() {
    let policy = verified_policy(1, 3, 2, 4);
    let state =
        VerifiedFullSyncIssuerStateV2::from_committed_records(policy.clone(), vec![]).unwrap();
    let grant_target = target(5, FullSyncReadRoleV2::StateSync);
    let command = issue_command(&state, &policy, 7, grant_target.clone(), 0);
    let decision = commit(&state, command.clone(), &policy, 11);

    assert_eq!(decision.record().next_anchor.revision, 1);
    assert_eq!(decision.record().resulting_target_state.generation, 1);
    assert!(decision.record().resulting_target_state.active);
    let retry = decision.state().prepare(command.clone()).unwrap();
    assert_eq!(retry.record(), None);
    assert_eq!(retry.existing(), Some(decision.entry()));
    assert_eq!(retry.next(), Some(&decision.record().next_anchor));

    let mut conflicting = command.body.clone();
    conflicting.operation = FullSyncIssuerOperationV2::Revoke;
    let conflicting = SignedFullSyncIssuerCommandV2::sign(conflicting, &key(2)).unwrap();
    assert!(decision.state().prepare(conflicting).is_err());

    let rebuilt = VerifiedFullSyncIssuerStateV2::from_committed_records(
        policy.clone(),
        vec![decision.entry().clone()],
    )
    .unwrap();
    assert_eq!(rebuilt.anchor(), decision.state().anchor());
    assert_eq!(
        rebuilt.target_state(&grant_target),
        decision.state().target_state(&grant_target)
    );

    let query =
        SignedFullSyncIssuerQueryV2::sign(&policy, state.anchor().clone(), [12; 32], &key(3))
            .unwrap();
    let observation = SignedFullSyncIssuerObservationV2::sign(
        FullSyncIssuerObservationBodyV2 {
            schema_version: 2,
            query,
            current_anchor: rebuilt.anchor().clone(),
        },
        &key(4),
    )
    .unwrap();
    let observed = VerifiedFullSyncIssuerStateV2::from_witness_observation(
        policy.clone(),
        vec![decision.entry().clone()],
        &observation,
    )
    .unwrap();
    assert_eq!(observed.anchor(), rebuilt.anchor());

    let missing =
        VerifiedFullSyncIssuerStateV2::from_witness_observation(policy, vec![], &observation);
    assert!(missing.is_err());
}

#[test]
fn policy_pins_complete_external_activation_trust_and_manager_nonce_is_single_use() {
    let signed = signed_policy(1, 3, 2, 4);
    let mut changed_trust = trust();
    changed_trust.recovery_policy_digest = hash(88);
    assert!(VerifiedFullSyncIssuerPolicyV2::verify(&signed, &changed_trust).is_err());

    let policy = VerifiedFullSyncIssuerPolicyV2::verify(&signed, &trust()).unwrap();
    let initial =
        VerifiedFullSyncIssuerStateV2::from_committed_records(policy.clone(), vec![]).unwrap();
    let first = issue_command(
        &initial,
        &policy,
        90,
        target(5, FullSyncReadRoleV2::StateSync),
        0,
    );
    let committed = commit(&initial, first.clone(), &policy, 91);
    let mut replayed_nonce = issue_command(
        committed.state(),
        &policy,
        92,
        target(6, FullSyncReadRoleV2::StateSync),
        0,
    )
    .body;
    replayed_nonce.command_nonce = first.body.command_nonce;
    let replayed_nonce = SignedFullSyncIssuerCommandV2::sign(replayed_nonce, &key(2)).unwrap();
    assert!(committed.state().prepare(replayed_nonce).is_err());
}

#[test]
fn only_policy_manager_can_authorize_allowed_role_and_exact_target() {
    let mut restricted_body = signed_policy(1, 3, 2, 4).body;
    restricted_body.allowed_roles = vec![FullSyncReadRoleV2::StateSync];
    let restricted_signed = SignedFullSyncIssuerPolicyV2::sign(restricted_body, &key(1)).unwrap();
    let restricted = VerifiedFullSyncIssuerPolicyV2::verify(&restricted_signed, &trust()).unwrap();
    let restricted_state =
        VerifiedFullSyncIssuerStateV2::from_committed_records(restricted.clone(), vec![]).unwrap();
    let forbidden_role = issue_command(
        &restricted_state,
        &restricted,
        91,
        target(5, FullSyncReadRoleV2::FullReadOnly),
        0,
    );
    assert!(restricted_state.prepare(forbidden_role).is_err());

    let policy = verified_policy(1, 3, 2, 4);
    let state =
        VerifiedFullSyncIssuerStateV2::from_committed_records(policy.clone(), vec![]).unwrap();
    let manager_command = issue_command(
        &state,
        &policy,
        92,
        target(5, FullSyncReadRoleV2::StateSync),
        0,
    );

    let mut changed_target = manager_command.clone();
    changed_target.body.target.node_public_key = public(6);
    assert!(state.prepare(changed_target).is_err());

    // The target node (5), a putative grant holder (6), and the provider/issuer
    // (3) cannot sign management commands in place of the configured manager (2).
    for unauthorized_signer in [key(5), key(6), key(3)] {
        let unauthorized =
            SignedFullSyncIssuerCommandV2::sign(manager_command.body.clone(), &unauthorized_signer)
                .unwrap();
        assert!(state.prepare(unauthorized).is_err());
    }
}

#[test]
fn target_generations_issue_replace_revoke_and_tombstone_are_monotonic() {
    let policy = verified_policy(1, 3, 2, 4);
    let initial =
        VerifiedFullSyncIssuerStateV2::from_committed_records(policy.clone(), vec![]).unwrap();
    let grant_target = target(5, FullSyncReadRoleV2::FullReadOnly);

    let issued = commit(
        &initial,
        issue_command(&initial, &policy, 20, grant_target.clone(), 0),
        &policy,
        21,
    );
    let replace = command(
        issued.state(),
        &policy,
        22,
        grant_target.clone(),
        FullSyncIssuerOperationV2::Replace {
            not_before_unix_ms: 200,
            expires_unix_ms: 20_000,
        },
        1,
        None,
    );
    let replaced = commit(issued.state(), replace, &policy, 23);
    assert_eq!(replaced.record().resulting_target_state.generation, 2);

    let revoke = command(
        replaced.state(),
        &policy,
        24,
        grant_target.clone(),
        FullSyncIssuerOperationV2::Revoke,
        2,
        None,
    );
    let revoked = commit(replaced.state(), revoke, &policy, 25);
    assert_eq!(revoked.record().resulting_target_state.generation, 3);
    assert!(!revoked.record().resulting_target_state.active);

    let second_revoke = command(
        revoked.state(),
        &policy,
        26,
        grant_target.clone(),
        FullSyncIssuerOperationV2::Revoke,
        3,
        None,
    );
    let revoked_again = commit(revoked.state(), second_revoke, &policy, 27);
    assert_eq!(revoked_again.record().resulting_target_state.generation, 4);

    let reissued = commit(
        revoked_again.state(),
        issue_command(revoked_again.state(), &policy, 28, grant_target.clone(), 4),
        &policy,
        29,
    );
    assert_eq!(reissued.record().resulting_target_state.generation, 5);

    let absent = target(6, FullSyncReadRoleV2::StateSync);
    let absent_revoke = command(
        reissued.state(),
        &policy,
        30,
        absent.clone(),
        FullSyncIssuerOperationV2::Revoke,
        0,
        None,
    );
    let tombstone = commit(reissued.state(), absent_revoke, &policy, 31);
    assert_eq!(tombstone.record().resulting_target_state.generation, 1);
    assert!(!tombstone.record().resulting_target_state.active);

    let replace_inactive = command(
        tombstone.state(),
        &policy,
        32,
        absent,
        FullSyncIssuerOperationV2::Replace {
            not_before_unix_ms: 300,
            expires_unix_ms: 30_000,
        },
        1,
        None,
    );
    assert!(tombstone.state().prepare(replace_inactive).is_err());
    let issue_active = issue_command(tombstone.state(), &policy, 33, grant_target, 5);
    assert!(tombstone.state().prepare(issue_active).is_err());
}

#[test]
fn root_authorized_generation_change_is_one_cas_and_fences_old_issuer_key() {
    let old_policy = verified_policy(1, 3, 2, 4);
    let initial =
        VerifiedFullSyncIssuerStateV2::from_committed_records(old_policy.clone(), vec![]).unwrap();
    let first = commit(
        &initial,
        issue_command(
            &initial,
            &old_policy,
            40,
            target(5, FullSyncReadRoleV2::StateSync),
            0,
        ),
        &old_policy,
        41,
    );

    let new_policy = verified_policy(2, 8, 9, 4);
    let transition = FullSyncIssuerPolicyTransitionV2 {
        previous_policy_digest: old_policy.policy_digest().into(),
        previous_signer_generation: 1,
        new_policy_digest: new_policy.policy_digest().into(),
        new_signer_generation: 2,
    };
    let upgraded_configuration = VerifiedFullSyncIssuerStateV2::from_committed_records(
        new_policy.clone(),
        first.state().records().to_vec(),
    )
    .unwrap();
    let new_command = command(
        &upgraded_configuration,
        &new_policy,
        42,
        target(6, FullSyncReadRoleV2::StateSync),
        FullSyncIssuerOperationV2::Issue {
            not_before_unix_ms: 400,
            expires_unix_ms: 40_000,
        },
        0,
        Some(transition),
    );
    let upgraded = commit(&upgraded_configuration, new_command, &new_policy, 43);
    assert_eq!(upgraded.record().previous_anchor, *first.state().anchor());
    assert_eq!(
        upgraded.record().next_anchor.revision,
        first.state().anchor().revision + 1
    );
    assert_eq!(upgraded.record().next_anchor.signer_generation, 2);
    assert_eq!(
        upgraded.record().next_anchor.policy_digest.as_deref(),
        Some(new_policy.policy_digest())
    );

    let reconstructed = VerifiedFullSyncIssuerStateV2::from_committed_records(
        new_policy.clone(),
        vec![first.entry().clone(), upgraded.entry().clone()],
    )
    .unwrap();
    assert_eq!(reconstructed.anchor(), upgraded.state().anchor());
    assert!(
        VerifiedFullSyncIssuerStateV2::from_committed_records(
            old_policy.clone(),
            vec![first.entry().clone(), upgraded.entry().clone()],
        )
        .is_err()
    );
    let old_generation_command = issue_command(
        upgraded.state(),
        &old_policy,
        44,
        target(7, FullSyncReadRoleV2::StateSync),
        0,
    );
    assert!(upgraded.state().prepare(old_generation_command).is_err());

    let reused_issuer = verified_policy(2, 3, 9, 4);
    assert!(
        VerifiedFullSyncIssuerStateV2::from_committed_records(
            reused_issuer,
            vec![first.entry().clone()],
        )
        .is_err()
    );

    let different_same_generation = verified_policy(1, 8, 2, 4);
    assert!(
        VerifiedFullSyncIssuerStateV2::from_committed_records(
            different_same_generation,
            vec![first.entry().clone()],
        )
        .is_err()
    );

    let changed_witness = verified_policy(2, 8, 9, 7);
    assert!(
        VerifiedFullSyncIssuerStateV2::from_committed_records(
            changed_witness,
            vec![first.entry().clone()],
        )
        .is_err()
    );
}

#[test]
fn narrowed_policy_can_revoke_known_old_role_but_cannot_issue_or_revoke_absent_old_role() {
    let old_policy = verified_policy(1, 3, 2, 4);
    let initial =
        VerifiedFullSyncIssuerStateV2::from_committed_records(old_policy.clone(), vec![]).unwrap();
    let old_role_target = target(5, FullSyncReadRoleV2::StateSync);
    let issued = commit(
        &initial,
        issue_command(&initial, &old_policy, 70, old_role_target.clone(), 0),
        &old_policy,
        71,
    );

    let narrowed_policy =
        verified_policy_with_roles(2, 8, 9, 4, vec![FullSyncReadRoleV2::FullReadOnly]);
    let transition = FullSyncIssuerPolicyTransitionV2 {
        previous_policy_digest: old_policy.policy_digest().into(),
        previous_signer_generation: 1,
        new_policy_digest: narrowed_policy.policy_digest().into(),
        new_signer_generation: 2,
    };
    let upgraded_configuration = VerifiedFullSyncIssuerStateV2::from_committed_records(
        narrowed_policy.clone(),
        issued.state().records().to_vec(),
    )
    .unwrap();
    let revoke = command(
        &upgraded_configuration,
        &narrowed_policy,
        72,
        old_role_target.clone(),
        FullSyncIssuerOperationV2::Revoke,
        1,
        Some(transition),
    );
    let revoked = commit(&upgraded_configuration, revoke, &narrowed_policy, 73);
    let revoked_state = revoked.state().target_state(&old_role_target).unwrap();
    assert_eq!(revoked_state.generation, 2);
    assert!(!revoked_state.active);

    let issue_removed_role = command(
        revoked.state(),
        &narrowed_policy,
        74,
        target(6, FullSyncReadRoleV2::StateSync),
        FullSyncIssuerOperationV2::Issue {
            not_before_unix_ms: 500,
            expires_unix_ms: 50_000,
        },
        0,
        None,
    );
    assert!(revoked.state().prepare(issue_removed_role).is_err());

    let revoke_absent_removed_role = command(
        revoked.state(),
        &narrowed_policy,
        75,
        target(7, FullSyncReadRoleV2::StateSync),
        FullSyncIssuerOperationV2::Revoke,
        0,
        None,
    );
    assert!(revoked.state().prepare(revoke_absent_removed_role).is_err());

    let old_generation_command = issue_command(
        revoked.state(),
        &old_policy,
        76,
        target(8, FullSyncReadRoleV2::StateSync),
        0,
    );
    assert!(revoked.state().prepare(old_generation_command).is_err());
}

#[test]
fn issuer_ledger_1024_valid_history_boundary() {
    let policy = verified_policy(1, 3, 2, 4);
    let build_started = Instant::now();
    let mut state =
        VerifiedFullSyncIssuerStateV2::from_committed_records(policy.clone(), vec![]).unwrap();
    for sequence in 1..=MAX_LEDGER_RECORDS as u64 {
        let command = sequence_command(
            &state,
            &policy,
            sequence,
            sequence_target(sequence, FullSyncReadRoleV2::FullReadOnly),
            FullSyncIssuerOperationV2::Issue {
                not_before_unix_ms: 100,
                expires_unix_ms: 10_000,
            },
            0,
        );
        let decision = commit_sequence(&state, command, &policy, sequence);
        state = decision.state;
    }
    let build_ms = build_started.elapsed().as_millis();
    assert_eq!(state.records().len(), MAX_LEDGER_RECORDS);
    assert_eq!(state.records().len(), MAX_TARGETS);

    let committed_records = state.records().to_vec();
    let replay_started = Instant::now();
    let replayed =
        VerifiedFullSyncIssuerStateV2::from_committed_records(policy.clone(), committed_records)
            .unwrap();
    let full_replay_ms = replay_started.elapsed().as_millis();
    assert_eq!(replayed.anchor(), state.anchor());
    eprintln!(
        "issuer_ledger_1024_valid_history_boundary measured_build_ms={build_ms} full_replay_ms={full_replay_ms}"
    );

    let extra_target_command = sequence_command(
        &state,
        &policy,
        MAX_LEDGER_RECORDS as u64 + 1,
        sequence_target(MAX_TARGETS as u64 + 1, FullSyncReadRoleV2::FullReadOnly),
        FullSyncIssuerOperationV2::Issue {
            not_before_unix_ms: 100,
            expires_unix_ms: 10_000,
        },
        0,
    );
    let full_anchor = state.anchor().clone();
    assert!(matches!(
        state.prepare(extra_target_command),
        Err(FormalError::Invalid(reason)) if reason == "full-sync target capacity"
    ));
    assert_eq!(state.anchor(), &full_anchor);

    let extra_record_command = sequence_command(
        &state,
        &policy,
        MAX_LEDGER_RECORDS as u64 + 2,
        sequence_target(1, FullSyncReadRoleV2::FullReadOnly),
        FullSyncIssuerOperationV2::Replace {
            not_before_unix_ms: 200,
            expires_unix_ms: 20_000,
        },
        1,
    );
    assert!(matches!(
        state.prepare(extra_record_command),
        Err(FormalError::Invalid(reason)) if reason == "full-sync record capacity"
    ));
    assert_eq!(state.anchor(), &full_anchor);

    let mut over_limit_records = state.records().to_vec();
    over_limit_records.push(over_limit_records[0].clone());
    assert!(matches!(
        VerifiedFullSyncIssuerStateV2::from_committed_records(policy, over_limit_records),
        Err(FormalError::Invalid(reason)) if reason == "full-sync issuer record count bound"
    ));
}

#[test]
fn replay_rejects_corrupt_state_receipt_and_unbound_command_head() {
    let policy = verified_policy(1, 3, 2, 4);
    let initial =
        VerifiedFullSyncIssuerStateV2::from_committed_records(policy.clone(), vec![]).unwrap();
    let issued = commit(
        &initial,
        issue_command(
            &initial,
            &policy,
            50,
            target(5, FullSyncReadRoleV2::StateSync),
            0,
        ),
        &policy,
        51,
    );

    let mut bad_state = issued.entry().clone();
    bad_state.record.resulting_target_state.generation = 2;
    assert!(
        VerifiedFullSyncIssuerStateV2::from_committed_records(policy.clone(), vec![bad_state])
            .is_err()
    );

    let mut bad_receipt = issued.entry().clone();
    bad_receipt.receipt.body.request_id = hash(52);
    assert!(
        VerifiedFullSyncIssuerStateV2::from_committed_records(policy.clone(), vec![bad_receipt])
            .is_err()
    );

    let mut bad_head = issued.entry().clone();
    bad_head.record.command.body.expected_head.log_digest = hash(53);
    bad_head.record.command.signature_hex = hex::encode(
        key(2)
            .sign(
                &encode(
                    COMMAND_DOMAIN,
                    &bad_head.record.command.body,
                    MAX_ENTRY_BYTES,
                )
                .unwrap(),
            )
            .to_bytes(),
    );
    assert!(VerifiedFullSyncIssuerStateV2::from_committed_records(policy, vec![bad_head]).is_err());
}
