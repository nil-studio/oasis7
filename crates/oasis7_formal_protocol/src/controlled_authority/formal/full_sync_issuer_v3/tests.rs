use super::*;
use crate::controlled_authority::formal::full_sync_issuer::{
    FullSyncIssuerCommandBodyV2, FullSyncIssuerObservationBodyV2, FullSyncIssuerOperationV2,
    FullSyncIssuerPolicyBodyV2, FullSyncIssuerTargetV2, FullSyncReadRoleV2,
    SignedFullSyncIssuerCommandV2, SignedFullSyncIssuerObservationV2, SignedFullSyncIssuerPolicyV2,
    SignedFullSyncIssuerQueryV2, SignedFullSyncIssuerReceiptV2, VerifiedFullSyncIssuerPolicyV2,
    VerifiedFullSyncIssuerStateV2,
};
use crate::controlled_authority::formal::{
    FormalError, FormalWorldIdentityV1, InitialActivationTrustV1,
};
use ed25519_dalek::SigningKey;

#[path = "issuer_key_reuse_tests.rs"]
mod issuer_key_reuse_tests;
#[path = "migration_resume_tests.rs"]
mod migration_resume_tests;

fn key(value: u8) -> SigningKey {
    SigningKey::from_bytes(&[value; 32])
}

fn public(value: u8) -> String {
    hex::encode(key(value).verifying_key().as_bytes())
}

fn hash(value: u8) -> String {
    hex::encode([value; 32])
}

fn world() -> FormalWorldIdentityV1 {
    FormalWorldIdentityV1 {
        world_id: "issuer-v3-migration-world".into(),
        chain_id: "issuer-v3-test-chain".into(),
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

fn old_policy() -> VerifiedFullSyncIssuerPolicyV2 {
    let trust = trust();
    let signed = SignedFullSyncIssuerPolicyV2::sign(
        FullSyncIssuerPolicyBodyV2 {
            schema_version: 2,
            root_public_key: public(1),
            activation_trust_digest:
                crate::controlled_authority::formal::full_sync_issuer::
                    full_sync_activation_trust_digest_v2(&trust)
                    .unwrap(),
            world: world(),
            authority_epoch: 7,
            allowed_roles: vec![FullSyncReadRoleV2::StateSync],
            management_public_key: public(2),
            issuer_public_key: public(3),
            witness_public_key: public(4),
            signer_generation: 1,
        },
        &key(1),
    )
    .unwrap();
    VerifiedFullSyncIssuerPolicyV2::verify(&signed, &trust).unwrap()
}

fn new_policy(old: &VerifiedFullSyncIssuerPolicyV2) -> VerifiedFullSyncIssuerPolicyV3 {
    let trust = trust();
    let signed = SignedFullSyncIssuerPolicyV3::sign(
        FullSyncIssuerPolicyBodyV3 {
            schema_version: 3,
            root_public_key: public(1),
            activation_trust_digest: full_sync_activation_trust_digest_v3(&trust).unwrap(),
            world: world(),
            authority_epoch: 7,
            scope_digest: old.scope_digest().into(),
            allowed_roles: vec![FullSyncReadRoleV2::StateSync],
            provider_public_keys: vec![public(6)],
            management_public_key: public(7),
            lease_signer_public_key: public(8),
            witness_public_key: public(4),
            signer_generation: 2,
            max_authorization_staleness_ms: 30_000,
            max_clock_uncertainty_ms: 100,
            clock_profile_digest: hash(35),
        },
        &key(1),
    )
    .unwrap();
    VerifiedFullSyncIssuerPolicyV3::verify(&signed, &trust, old).unwrap()
}

fn current_v2_state(policy: &VerifiedFullSyncIssuerPolicyV2) -> VerifiedFullSyncIssuerStateV2 {
    let state =
        VerifiedFullSyncIssuerStateV2::from_committed_records(policy.clone(), vec![]).unwrap();
    let target = FullSyncIssuerTargetV2 {
        node_public_key: public(5),
        role: FullSyncReadRoleV2::StateSync,
    };
    let command = SignedFullSyncIssuerCommandV2::sign(
        FullSyncIssuerCommandBodyV2 {
            schema_version: 2,
            world: policy.body().world.clone(),
            authority_epoch: policy.body().authority_epoch,
            scope_digest: policy.scope_digest().into(),
            policy_digest: policy.policy_digest().into(),
            signer_generation: policy.body().signer_generation,
            transition: None,
            target,
            operation: FullSyncIssuerOperationV2::Issue {
                not_before_unix_ms: 1_000,
                expires_unix_ms: 10_000,
            },
            expected_head: state.anchor().clone(),
            expected_target_generation: 0,
            request_id: hash(40),
            command_nonce: hash(41),
        },
        &key(2),
    )
    .unwrap();
    let prepared = state.prepare(command).unwrap();
    let record = prepared.record().unwrap().clone();
    let query = SignedFullSyncIssuerQueryV2::sign(
        policy,
        record.previous_anchor.clone(),
        [42; 32],
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
    prepared.commit(receipt).unwrap().state().clone()
}

fn capacity_bytes(domain: u8, sequence: u64) -> [u8; 32] {
    let mut bytes = [0; 32];
    bytes[0] = domain;
    bytes[24..].copy_from_slice(&sequence.to_be_bytes());
    bytes
}

fn capacity_id(domain: u8, sequence: u64) -> String {
    hex::encode(capacity_bytes(domain, sequence))
}

fn v2_state_with_capacity_history(
    policy: &VerifiedFullSyncIssuerPolicyV2,
    count: u64,
) -> VerifiedFullSyncIssuerStateV2 {
    let mut state =
        VerifiedFullSyncIssuerStateV2::from_committed_records(policy.clone(), vec![]).unwrap();
    let target = FullSyncIssuerTargetV2 {
        node_public_key: public(5),
        role: FullSyncReadRoleV2::StateSync,
    };
    for sequence in 1..=count {
        let operation = if sequence == 1 {
            FullSyncIssuerOperationV2::Issue {
                not_before_unix_ms: 1_000,
                expires_unix_ms: 1_000_000,
            }
        } else {
            FullSyncIssuerOperationV2::Replace {
                not_before_unix_ms: 1_000,
                expires_unix_ms: 1_000_000,
            }
        };
        let command = SignedFullSyncIssuerCommandV2::sign(
            FullSyncIssuerCommandBodyV2 {
                schema_version: 2,
                world: policy.body().world.clone(),
                authority_epoch: policy.body().authority_epoch,
                scope_digest: policy.scope_digest().into(),
                policy_digest: policy.policy_digest().into(),
                signer_generation: policy.body().signer_generation,
                transition: None,
                target: target.clone(),
                operation,
                expected_head: state.anchor().clone(),
                expected_target_generation: sequence - 1,
                request_id: capacity_id(70, sequence),
                command_nonce: capacity_id(71, sequence),
            },
            &key(2),
        )
        .unwrap();
        let prepared = state.prepare(command).unwrap();
        let record = prepared.record().unwrap().clone();
        let query = SignedFullSyncIssuerQueryV2::sign(
            policy,
            record.previous_anchor.clone(),
            capacity_bytes(72, sequence),
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
        state = prepared.commit(receipt).unwrap().state().clone();
    }
    state
}

fn v2_observation(
    policy: &VerifiedFullSyncIssuerPolicyV2,
    anchor: &crate::controlled_authority::formal::full_sync_issuer::FullSyncIssuerAnchorV2,
    nonce: u8,
) -> SignedFullSyncIssuerObservationV2 {
    let query =
        SignedFullSyncIssuerQueryV2::sign(policy, anchor.clone(), [nonce; 32], &key(3)).unwrap();
    SignedFullSyncIssuerObservationV2::sign(
        FullSyncIssuerObservationBodyV2 {
            schema_version: 2,
            query,
            current_anchor: anchor.clone(),
        },
        &key(4),
    )
    .unwrap()
}

fn v3_observation(
    state: &VerifiedFullSyncIssuerStateV3,
    nonce: u8,
    earliest: u64,
    latest: u64,
) -> SignedFullSyncIssuerObservationV3 {
    v3_observation_with_profile(
        state,
        nonce,
        earliest,
        latest,
        state.policy().body().clock_profile_digest.clone(),
    )
}

fn v3_observation_with_profile(
    state: &VerifiedFullSyncIssuerStateV3,
    nonce: u8,
    earliest: u64,
    latest: u64,
    clock_profile_digest: String,
) -> SignedFullSyncIssuerObservationV3 {
    let query = SignedFullSyncIssuerQueryV3::sign(
        state.policy(),
        state.anchor().clone(),
        [nonce; 32],
        &key(8),
    )
    .unwrap();
    SignedFullSyncIssuerObservationV3::sign(
        FullSyncIssuerObservationBodyV3 {
            schema_version: 3,
            query,
            current_anchor: state.anchor().clone(),
            protected_time: FullSyncIssuerProtectedTimeIntervalV3 {
                earliest_unix_ms: earliest,
                latest_unix_ms: latest,
                clock_profile_digest,
            },
        },
        &key(4),
    )
    .unwrap()
}

fn provider_lease_request(
    state: &VerifiedFullSyncIssuerStateV3,
    target: &FullSyncIssuerTargetV2,
    request_id: u8,
    request_nonce: u8,
    lease_id: u8,
) -> SignedFullSyncProviderLeaseRequestV3 {
    SignedFullSyncProviderLeaseRequestV3::sign(
        FullSyncProviderLeaseRequestBodyV3 {
            schema_version: 3,
            world: world(),
            authority_epoch: state.policy().body().authority_epoch,
            scope_digest: state.policy().scope_digest().into(),
            policy_digest: state.policy().policy_digest().into(),
            signer_generation: state.policy().body().signer_generation,
            provider_public_key: public(6),
            provider_incarnation: hash(53),
            lease_id: hash(lease_id),
            request_id: hash(request_id),
            request_nonce: hash(request_nonce),
            expected_head: state.anchor().clone(),
            target: target.clone(),
            target_generation: state.target_state(target).unwrap().generation,
        },
        &key(6),
    )
    .unwrap()
}

fn migrated_state(
    old: &VerifiedFullSyncIssuerPolicyV2,
    v3: &VerifiedFullSyncIssuerPolicyV3,
    v2_state: &VerifiedFullSyncIssuerStateV2,
    nonce: u8,
) -> (
    VerifiedFullSyncIssuerStateV3,
    FullSyncIssuerCommittedEntryV3,
) {
    let observation = v2_observation(old, v2_state.anchor(), nonce);
    let migration = SignedFullSyncIssuerMigrationV3::sign(
        FullSyncIssuerMigrationBodyV3 {
            schema_version: 3,
            previous_v2_policy_digest: old.policy_digest().into(),
            previous_v2_signer_generation: old.body().signer_generation,
            expected_v2_head: v2_state.anchor().clone(),
            new_policy: v3.signed().clone(),
            request_id: hash(nonce.wrapping_add(1)),
            manager_nonce: hash(nonce.wrapping_add(2)),
        },
        &key(2),
    )
    .unwrap();
    let prepared = v2_state
        .prepare_migration_v3(v3.clone(), migration, &observation)
        .unwrap();
    let record = prepared.record().unwrap().clone();
    let receipt = SignedFullSyncIssuerReceiptV3::sign(
        FullSyncIssuerReceiptObservationV3::Migration(observation),
        &record,
        &key(4),
    )
    .unwrap();
    let decision = prepared.commit(receipt).unwrap();
    (decision.state().clone(), decision.entry().clone())
}

#[test]
fn v3_migration_prepares_one_successor_after_verified_v2_prefix() {
    let old = old_policy();
    let state = current_v2_state(&old);
    let v3 = new_policy(&old);
    let expected_v2_head = state.anchor().clone();
    let query =
        SignedFullSyncIssuerQueryV2::sign(&old, expected_v2_head.clone(), [43; 32], &key(3))
            .unwrap();
    let observation = SignedFullSyncIssuerObservationV2::sign(
        FullSyncIssuerObservationBodyV2 {
            schema_version: 2,
            query,
            current_anchor: expected_v2_head.clone(),
        },
        &key(4),
    )
    .unwrap();
    let migration = SignedFullSyncIssuerMigrationV3::sign(
        FullSyncIssuerMigrationBodyV3 {
            schema_version: 3,
            previous_v2_policy_digest: old.policy_digest().into(),
            previous_v2_signer_generation: old.body().signer_generation,
            expected_v2_head: expected_v2_head.clone(),
            new_policy: v3.signed().clone(),
            request_id: hash(44),
            manager_nonce: hash(45),
        },
        &key(2),
    )
    .unwrap();

    let prepared = state
        .prepare_migration_v3(v3, migration, &observation)
        .expect("valid manager-approved migration should prepare");
    let record = prepared.record().expect("migration is a new V3 event");
    assert_eq!(
        record.previous_anchor,
        FullSyncIssuerPreviousAnchorV3::V2(expected_v2_head.clone())
    );
    assert_eq!(record.next_anchor.revision, expected_v2_head.revision + 1);
    assert!(matches!(&record.event, FullSyncIssuerEventV3::Migration(_)));
}

#[test]
fn v3_migration_and_lease_share_the_lifecycle_record_capacity() {
    let old = old_policy();
    let v3 = new_policy(&old);
    let full_v2 = v2_state_with_capacity_history(&old, 1_024);
    assert_eq!(full_v2.records().len(), 1_024);

    let full_anchor = full_v2.anchor().clone();
    let migration = SignedFullSyncIssuerMigrationV3::sign(
        FullSyncIssuerMigrationBodyV3 {
            schema_version: 3,
            previous_v2_policy_digest: old.policy_digest().into(),
            previous_v2_signer_generation: old.body().signer_generation,
            expected_v2_head: full_anchor.clone(),
            new_policy: v3.signed().clone(),
            request_id: hash(130),
            manager_nonce: hash(131),
        },
        &key(2),
    )
    .unwrap();
    let observation = v2_observation(&old, &full_anchor, 132);
    assert!(matches!(
        full_v2.prepare_migration_v3(v3.clone(), migration, &observation),
        Err(FormalError::Invalid(reason))
            if reason == "full-sync V3 total ledger record capacity"
    ));
    assert_eq!(full_v2.anchor(), &full_anchor);
    assert_eq!(full_v2.records().len(), 1_024);

    let v2_1023 = VerifiedFullSyncIssuerStateV2::from_committed_records(
        old.clone(),
        full_v2.records()[..1_023].to_vec(),
    )
    .unwrap();
    let (v3_at_capacity, _) = migrated_state(&old, &v3, &v2_1023, 133);
    assert_eq!(v3_at_capacity.v2_prefix().len(), 1_023);
    assert_eq!(v3_at_capacity.entries().len(), 1);
    let full_v3_anchor = v3_at_capacity.anchor().clone();
    let target = FullSyncIssuerTargetV2 {
        node_public_key: public(5),
        role: FullSyncReadRoleV2::StateSync,
    };
    let observation = v3_observation(&v3_at_capacity, 134, 2_000, 2_100);
    let request = provider_lease_request(&v3_at_capacity, &target, 135, 136, 137);
    assert!(matches!(
        v3_at_capacity.prepare_lease(request, &observation, &key(8)),
        Err(FormalError::Invalid(reason))
            if reason == "full-sync V3 total ledger record capacity"
    ));
    assert_eq!(v3_at_capacity.anchor(), &full_v3_anchor);
    assert_eq!(
        v3_at_capacity.v2_prefix().len() + v3_at_capacity.entries().len(),
        1_024
    );

    let v2_1022 = VerifiedFullSyncIssuerStateV2::from_committed_records(
        old.clone(),
        full_v2.records()[..1_022].to_vec(),
    )
    .unwrap();
    let (v3_one_slot_left, _) = migrated_state(&old, &v3, &v2_1022, 138);
    assert_eq!(
        v3_one_slot_left.v2_prefix().len() + v3_one_slot_left.entries().len(),
        1_023
    );
    let observation = v3_observation(&v3_one_slot_left, 139, 2_000, 2_100);
    let request = provider_lease_request(&v3_one_slot_left, &target, 140, 141, 142);
    let prepared = v3_one_slot_left
        .prepare_lease(request, &observation, &key(8))
        .expect("the final lifecycle record slot must remain usable");
    let record = prepared.record().unwrap().clone();
    let receipt = SignedFullSyncIssuerReceiptV3::sign(
        FullSyncIssuerReceiptObservationV3::V3(observation),
        &record,
        &key(4),
    )
    .unwrap();
    let at_full_capacity = prepared.commit(receipt).unwrap().state().clone();
    assert_eq!(at_full_capacity.v2_prefix().len(), 1_022);
    assert_eq!(at_full_capacity.entries().len(), 2);

    let full_anchor = at_full_capacity.anchor().clone();
    let observation = v3_observation(&at_full_capacity, 143, 2_050, 2_150);
    let request = provider_lease_request(&at_full_capacity, &target, 144, 145, 146);
    assert!(matches!(
        at_full_capacity.prepare_lease(request, &observation, &key(8)),
        Err(FormalError::Invalid(reason))
            if reason == "full-sync V3 total ledger record capacity"
    ));
    assert_eq!(at_full_capacity.anchor(), &full_anchor);
    assert_eq!(
        at_full_capacity.v2_prefix().len() + at_full_capacity.entries().len(),
        1_024
    );
}

#[test]
fn v3_migration_accepts_root_verified_configured_policy_at_empty_v2_genesis() {
    let old = old_policy();
    let state = VerifiedFullSyncIssuerStateV2::from_committed_records(old.clone(), vec![]).unwrap();
    let v3 = new_policy(&old);
    let observation = v2_observation(&old, state.anchor(), 81);
    let migration = SignedFullSyncIssuerMigrationV3::sign(
        FullSyncIssuerMigrationBodyV3 {
            schema_version: 3,
            previous_v2_policy_digest: old.policy_digest().into(),
            previous_v2_signer_generation: old.body().signer_generation,
            expected_v2_head: state.anchor().clone(),
            new_policy: v3.signed().clone(),
            request_id: hash(82),
            manager_nonce: hash(83),
        },
        &key(2),
    )
    .unwrap();

    let prepared = state
        .prepare_migration_v3(v3, migration, &observation)
        .expect("configured root-verified V2 policy governs only untouched genesis");
    let record = prepared
        .record()
        .expect("migration is a new V3 event")
        .clone();
    assert_eq!(record.next_anchor.revision, 1);
    let receipt = SignedFullSyncIssuerReceiptV3::sign(
        FullSyncIssuerReceiptObservationV3::Migration(observation),
        &record,
        &key(4),
    )
    .unwrap();
    let committed = prepared.commit(receipt).unwrap();
    assert!(committed.state().v2_prefix().is_empty());
    assert_eq!(committed.state().entries().len(), 1);
    assert_eq!(committed.state().anchor(), &record.next_anchor);
}

#[test]
fn v3_provider_lease_rejects_unusable_clock_and_expired_grant() {
    let old = old_policy();
    let v2 = current_v2_state(&old);
    let v3 = new_policy(&old);
    let (state, _) = migrated_state(&old, &v3, &v2, 84);
    let target = FullSyncIssuerTargetV2 {
        node_public_key: public(5),
        role: FullSyncReadRoleV2::StateSync,
    };
    let bad_clock_cases = [
        (85, 2_101, 2_100, v3.body().clock_profile_digest.clone()),
        (86, 2_000, 2_101, v3.body().clock_profile_digest.clone()),
        (87, 2_000, 2_100, hash(99)),
    ];
    for (nonce, earliest, latest, profile_digest) in bad_clock_cases {
        let observation =
            v3_observation_with_profile(&state, nonce, earliest, latest, profile_digest);
        let request = provider_lease_request(&state, &target, nonce + 10, nonce + 20, nonce + 30);
        assert!(
            state.prepare_lease(request, &observation, &key(8)).is_err(),
            "invalid witness interval/profile must not produce a provider lease"
        );
    }

    let expired_observation = v3_observation(&state, 88, 10_001, 10_001);
    let request = provider_lease_request(&state, &target, 98, 99, 100);
    assert!(
        state
            .prepare_lease(request, &expired_observation, &key(8))
            .is_err(),
        "a valid clock interval after the grant expiry cannot produce a lease"
    );
}

#[test]
fn v3_provider_lease_requires_allowlisted_provider_proof_of_possession() {
    let old = old_policy();
    let v2 = current_v2_state(&old);
    let v3 = new_policy(&old);
    let (state, _) = migrated_state(&old, &v3, &v2, 101);
    let target = FullSyncIssuerTargetV2 {
        node_public_key: public(5),
        role: FullSyncReadRoleV2::StateSync,
    };
    let observation = v3_observation(&state, 102, 2_000, 2_100);

    let mut unapproved = provider_lease_request(&state, &target, 103, 104, 105).body;
    unapproved.provider_public_key = public(9);
    let unapproved = SignedFullSyncProviderLeaseRequestV3::sign(unapproved, &key(9)).unwrap();
    assert!(
        state
            .prepare_lease(unapproved, &observation, &key(8))
            .is_err()
    );

    let approved_body = provider_lease_request(&state, &target, 106, 107, 108).body;
    let wrong_signer = SignedFullSyncProviderLeaseRequestV3::sign(approved_body, &key(9)).unwrap();
    assert!(
        state
            .prepare_lease(wrong_signer, &observation, &key(8))
            .is_err()
    );
}

#[test]
fn v3_global_request_identity_cannot_be_reused_by_a_different_event() {
    let old = old_policy();
    let v2 = current_v2_state(&old);
    let v3 = new_policy(&old);
    let (state, _) = migrated_state(&old, &v3, &v2, 109);
    let target = FullSyncIssuerTargetV2 {
        node_public_key: public(5),
        role: FullSyncReadRoleV2::StateSync,
    };
    let observation = v3_observation(&state, 111, 2_000, 2_100);
    let reused_migration_id = hash(110);
    let command = SignedFullSyncIssuerCommandV3::sign(
        FullSyncIssuerCommandBodyV3 {
            schema_version: 3,
            world: world(),
            authority_epoch: v3.body().authority_epoch,
            scope_digest: v3.scope_digest().into(),
            policy_digest: v3.policy_digest().into(),
            signer_generation: v3.body().signer_generation,
            target,
            operation: FullSyncIssuerOperationV3::Revoke,
            expected_head: state.anchor().clone(),
            expected_target_generation: 1,
            request_id: reused_migration_id,
            command_nonce: hash(112),
        },
        &key(7),
    )
    .unwrap();
    assert!(matches!(
        state.prepare_management(command, &observation),
        Err(FormalError::Conflict(_))
    ));
}

#[test]
fn v3_migration_rejects_wrong_v2_head_and_root_policy() {
    let old = old_policy();
    let state = current_v2_state(&old);
    let v3 = new_policy(&old);
    let observation = v2_observation(&old, state.anchor(), 117);

    let mut wrong_head = state.anchor().clone();
    wrong_head.revision += 1;
    let head_migration = SignedFullSyncIssuerMigrationV3::sign(
        FullSyncIssuerMigrationBodyV3 {
            schema_version: 3,
            previous_v2_policy_digest: old.policy_digest().into(),
            previous_v2_signer_generation: old.body().signer_generation,
            expected_v2_head: wrong_head,
            new_policy: v3.signed().clone(),
            request_id: hash(118),
            manager_nonce: hash(119),
        },
        &key(2),
    )
    .unwrap();
    assert!(matches!(
        state.prepare_migration_v3(v3.clone(), head_migration, &observation),
        Err(FormalError::Conflict(_))
    ));

    let mut wrong_body = v3.signed().body.clone();
    wrong_body.clock_profile_digest = hash(120);
    let wrong_signed = SignedFullSyncIssuerPolicyV3::sign(wrong_body, &key(1)).unwrap();
    let wrong_policy = VerifiedFullSyncIssuerPolicyV3::verify(&wrong_signed, &trust(), &old)
        .expect("root can sign a different policy for a negative binding test");
    let policy_migration = SignedFullSyncIssuerMigrationV3::sign(
        FullSyncIssuerMigrationBodyV3 {
            schema_version: 3,
            previous_v2_policy_digest: old.policy_digest().into(),
            previous_v2_signer_generation: old.body().signer_generation,
            expected_v2_head: state.anchor().clone(),
            new_policy: wrong_policy.signed().clone(),
            request_id: hash(121),
            manager_nonce: hash(122),
        },
        &key(2),
    )
    .unwrap();
    assert!(
        state
            .prepare_migration_v3(v3, policy_migration, &observation)
            .is_err()
    );
}

#[test]
fn v3_policy_rejects_a_different_activation_trust_for_the_same_world() {
    let old = old_policy();
    let mut alternate_trust = trust();
    alternate_trust.issuer_public_key = public(9);
    alternate_trust.initial_state_root = hash(201);

    let mut body = new_policy(&old).signed().body.clone();
    body.root_public_key = alternate_trust.issuer_public_key.clone();
    body.activation_trust_digest = full_sync_activation_trust_digest_v3(&alternate_trust).unwrap();
    let signed = SignedFullSyncIssuerPolicyV3::sign(body, &key(9)).unwrap();

    assert_eq!(alternate_trust.world, old.body().world);
    assert!(VerifiedFullSyncIssuerPolicyV3::verify(&signed, &alternate_trust, &old).is_err());
}

#[test]
fn v3_policy_rejects_a_different_initial_state_pin_with_the_same_root() {
    let old = old_policy();
    let mut alternate_trust = trust();
    alternate_trust.initial_state_root = hash(202);

    let mut body = new_policy(&old).signed().body.clone();
    body.activation_trust_digest = full_sync_activation_trust_digest_v3(&alternate_trust).unwrap();
    let signed = SignedFullSyncIssuerPolicyV3::sign(body, &key(1)).unwrap();

    assert_eq!(alternate_trust.world, old.body().world);
    assert_eq!(
        alternate_trust.issuer_public_key,
        old.trust().issuer_public_key
    );
    assert!(VerifiedFullSyncIssuerPolicyV3::verify(&signed, &alternate_trust, &old).is_err());
}

#[test]
fn v3_replay_rejects_tampered_receipt_and_record_history() {
    let old = old_policy();
    let v2 = current_v2_state(&old);
    let v3 = new_policy(&old);
    let (state, _) = migrated_state(&old, &v3, &v2, 123);
    let observation = v3_observation(&state, 125, 11_000, 11_050);
    let history = FullSyncIssuerHistoryV3 {
        v2_entries: v2.records().to_vec(),
        v3_entries: state.entries().to_vec(),
    };

    let mut bad_receipt = history.clone();
    bad_receipt.v3_entries[0].receipt.signature_hex = "00".into();
    assert!(
        VerifiedFullSyncIssuerStateV3::from_witness_observation(
            old.clone(),
            v3.clone(),
            bad_receipt,
            &observation,
        )
        .is_err()
    );

    let mut bad_record = history;
    bad_record.v3_entries[0].record.next_anchor.log_digest = hash(126);
    assert!(
        VerifiedFullSyncIssuerStateV3::from_witness_observation(old, v3, bad_record, &observation,)
            .is_err()
    );
}

#[test]
fn v3_management_and_lease_replay_preserve_revoked_historical_lease() {
    let old = old_policy();
    let v2 = current_v2_state(&old);
    let v3 = new_policy(&old);
    let v2_prefix = v2.records().to_vec();
    let (migrated, migration_entry) = migrated_state(&old, &v3, &v2, 50);
    let lease_observation = v3_observation(&migrated, 52, 2_000, 2_100);
    let target = FullSyncIssuerTargetV2 {
        node_public_key: public(5),
        role: FullSyncReadRoleV2::StateSync,
    };
    let request = SignedFullSyncProviderLeaseRequestV3::sign(
        FullSyncProviderLeaseRequestBodyV3 {
            schema_version: 3,
            world: world(),
            authority_epoch: 7,
            scope_digest: v3.scope_digest().into(),
            policy_digest: v3.policy_digest().into(),
            signer_generation: v3.body().signer_generation,
            provider_public_key: public(6),
            provider_incarnation: hash(53),
            lease_id: hash(54),
            request_id: hash(55),
            request_nonce: hash(56),
            expected_head: migrated.anchor().clone(),
            target: target.clone(),
            target_generation: 1,
        },
        &key(6),
    )
    .unwrap();
    let lease_prepared = migrated
        .prepare_lease(request.clone(), &lease_observation, &key(8))
        .unwrap();
    let lease_record = lease_prepared.record().unwrap().clone();
    let lease = match &lease_record.event {
        FullSyncIssuerEventV3::Lease { lease, .. } => lease,
        _ => panic!("provider request must produce a lease event"),
    };
    assert_eq!(lease.body.not_before_unix_ms, 2_100);
    assert_eq!(lease.body.expires_unix_ms, 10_000);
    let lease_receipt = SignedFullSyncIssuerReceiptV3::sign(
        FullSyncIssuerReceiptObservationV3::V3(lease_observation.clone()),
        &lease_record,
        &key(4),
    )
    .unwrap();
    let leased = lease_prepared
        .commit(lease_receipt)
        .unwrap()
        .state()
        .clone();

    let management_observation = v3_observation(&leased, 57, 11_000, 11_050);
    let revoke = SignedFullSyncIssuerCommandV3::sign(
        FullSyncIssuerCommandBodyV3 {
            schema_version: 3,
            world: world(),
            authority_epoch: 7,
            scope_digest: v3.scope_digest().into(),
            policy_digest: v3.policy_digest().into(),
            signer_generation: v3.body().signer_generation,
            target: target.clone(),
            operation: FullSyncIssuerOperationV3::Revoke,
            expected_head: leased.anchor().clone(),
            expected_target_generation: 1,
            request_id: hash(58),
            command_nonce: hash(59),
        },
        &key(7),
    )
    .unwrap();
    let revoke_prepared = leased
        .prepare_management(revoke, &management_observation)
        .unwrap();
    let revoke_record = revoke_prepared.record().unwrap().clone();
    let revoke_receipt = SignedFullSyncIssuerReceiptV3::sign(
        FullSyncIssuerReceiptObservationV3::V3(management_observation),
        &revoke_record,
        &key(4),
    )
    .unwrap();
    let revoked = revoke_prepared
        .commit(revoke_receipt)
        .unwrap()
        .state()
        .clone();
    assert!(!revoked.target_state(&target).unwrap().active);
    assert_eq!(
        revoked.leases().len(),
        1,
        "revoke must retain lease evidence"
    );
    let post_revoke_request = provider_lease_request(&revoked, &target, 113, 114, 115);
    let post_revoke_observation = v3_observation(&revoked, 116, 11_000, 11_050);
    assert!(matches!(
        revoked.prepare_lease(post_revoke_request, &post_revoke_observation, &key(8)),
        Err(FormalError::Conflict(_))
    ));

    let historical_retry = revoked
        .prepare_lease(request.clone(), &lease_observation, &key(8))
        .expect("exact old request recovers the original decision before current checks");
    assert_eq!(historical_retry.record(), None);
    assert_eq!(historical_retry.existing().unwrap().record, lease_record);
    let mut conflicting = request.body.clone();
    conflicting.provider_incarnation = hash(60);
    let conflicting = SignedFullSyncProviderLeaseRequestV3::sign(conflicting, &key(6)).unwrap();
    assert!(matches!(
        revoked.prepare_lease(conflicting, &lease_observation, &key(8)),
        Err(FormalError::Conflict(_))
    ));

    let query_observation = v3_observation(&revoked, 61, 11_000, 11_050);
    let history = FullSyncIssuerHistoryV3 {
        v2_entries: v2_prefix,
        v3_entries: revoked.entries().to_vec(),
    };
    let replayed = VerifiedFullSyncIssuerStateV3::from_witness_observation(
        old,
        v3,
        history,
        &query_observation,
    )
    .unwrap();
    assert_eq!(replayed.anchor(), revoked.anchor());
    assert!(!replayed.target_state(&target).unwrap().active);
    assert_eq!(replayed.leases().len(), 1);
    assert_eq!(replayed.entries()[0], migration_entry);
}

#[test]
fn v3_migration_resume_distinguishes_v2_noncommit_from_committed_receipt() {
    let old = old_policy();
    let v2 = current_v2_state(&old);
    let v3 = new_policy(&old);
    let original_observation = v2_observation(&old, v2.anchor(), 70);
    let migration = SignedFullSyncIssuerMigrationV3::sign(
        FullSyncIssuerMigrationBodyV3 {
            schema_version: 3,
            previous_v2_policy_digest: old.policy_digest().into(),
            previous_v2_signer_generation: old.body().signer_generation,
            expected_v2_head: v2.anchor().clone(),
            new_policy: v3.signed().clone(),
            request_id: hash(71),
            manager_nonce: hash(72),
        },
        &key(2),
    )
    .unwrap();
    let resume_query = SignedFullSyncIssuerMigrationResumeQueryV3::sign(
        &v3,
        original_observation.body.query.clone(),
        migration.clone(),
        [73; 32],
        &key(8),
    )
    .unwrap();
    let not_committed = FullSyncIssuerMigrationResumeBundleV3 {
        observation: SignedFullSyncIssuerMigrationResumeObservationV3::sign(
            FullSyncIssuerMigrationResumeObservationBodyV3 {
                schema_version: 3,
                query: resume_query.clone(),
                current_mode_head: FullSyncIssuerWitnessModeHeadV3::V2(v2.anchor().clone()),
            },
            &key(4),
        )
        .unwrap(),
        history: FullSyncIssuerHistoryV3 {
            v2_entries: v2.records().to_vec(),
            v3_entries: vec![],
        },
        original_migration_receipt: None,
    };
    match recover_migration_resume_v3(old.clone(), v3.clone(), &resume_query, not_committed)
        .unwrap()
    {
        FullSyncIssuerMigrationResumeResultV3::NotCommittedV2 {
            current_anchor,
            migration_head_conflict,
            ..
        } => {
            assert_eq!(current_anchor, *v2.anchor());
            assert!(!migration_head_conflict);
        }
        FullSyncIssuerMigrationResumeResultV3::Committed(_) => {
            panic!("V2 witness mode must not report a committed migration")
        }
        FullSyncIssuerMigrationResumeResultV3::NotCommittedV3MigrationConflict(_) => {
            panic!("V2 witness mode cannot report a competing V3 migration")
        }
    }

    let v2_prefix = v2.records().to_vec();
    let (v3_state, migration_entry) = migrated_state(&old, &v3, &v2, 74);
    let committed_observation = match &migration_entry.receipt.body.observation {
        FullSyncIssuerReceiptObservationV3::Migration(observation) => observation,
        FullSyncIssuerReceiptObservationV3::V3(_) => panic!("migration receipt must use V2"),
    };
    let committed_resume_query = SignedFullSyncIssuerMigrationResumeQueryV3::sign(
        &v3,
        committed_observation.body.query.clone(),
        match &migration_entry.record.event {
            FullSyncIssuerEventV3::Migration(migration) => migration.as_ref().clone(),
            _ => panic!("first V3 entry must be the migration"),
        },
        [77; 32],
        &key(8),
    )
    .unwrap();
    let committed = FullSyncIssuerMigrationResumeBundleV3 {
        observation: SignedFullSyncIssuerMigrationResumeObservationV3::sign(
            FullSyncIssuerMigrationResumeObservationBodyV3 {
                schema_version: 3,
                query: committed_resume_query.clone(),
                current_mode_head: FullSyncIssuerWitnessModeHeadV3::V3(v3_state.anchor().clone()),
            },
            &key(4),
        )
        .unwrap(),
        history: FullSyncIssuerHistoryV3 {
            v2_entries: v2_prefix,
            v3_entries: v3_state.entries().to_vec(),
        },
        original_migration_receipt: Some(migration_entry.receipt.clone()),
    };
    match recover_migration_resume_v3(old, v3, &committed_resume_query, committed).unwrap() {
        FullSyncIssuerMigrationResumeResultV3::Committed(resume) => {
            let recovered = &resume.migration_entry;
            let original_receipt = &resume.original_receipt;
            let anchor = &resume.anchor;
            assert_eq!(recovered, &migration_entry);
            assert_eq!(original_receipt, &migration_entry.receipt);
            assert_eq!(anchor, &migration_entry.record.next_anchor);
        }
        FullSyncIssuerMigrationResumeResultV3::NotCommittedV2 { .. } => {
            panic!("V3 witness mode must return the committed migration receipt")
        }
        FullSyncIssuerMigrationResumeResultV3::NotCommittedV3MigrationConflict(_) => {
            panic!("the requested migration is committed, not a losing conflict")
        }
    }
}

#[test]
fn v3_migration_resume_rejects_a_replayed_v2_mode_response() {
    let old = old_policy();
    let v2 = current_v2_state(&old);
    let v3 = new_policy(&old);
    let original_observation = v2_observation(&old, v2.anchor(), 80);
    let migration = SignedFullSyncIssuerMigrationV3::sign(
        FullSyncIssuerMigrationBodyV3 {
            schema_version: 3,
            previous_v2_policy_digest: old.policy_digest().into(),
            previous_v2_signer_generation: old.body().signer_generation,
            expected_v2_head: v2.anchor().clone(),
            new_policy: v3.signed().clone(),
            request_id: hash(81),
            manager_nonce: hash(82),
        },
        &key(2),
    )
    .unwrap();
    let original_query = SignedFullSyncIssuerMigrationResumeQueryV3::sign(
        &v3,
        original_observation.body.query.clone(),
        migration.clone(),
        [83; 32],
        &key(8),
    )
    .unwrap();
    let expected_query = SignedFullSyncIssuerMigrationResumeQueryV3::sign(
        &v3,
        original_observation.body.query.clone(),
        migration,
        [84; 32],
        &key(8),
    )
    .unwrap();
    let replayed_response = FullSyncIssuerMigrationResumeBundleV3 {
        observation: SignedFullSyncIssuerMigrationResumeObservationV3::sign(
            FullSyncIssuerMigrationResumeObservationBodyV3 {
                schema_version: 3,
                query: original_query,
                current_mode_head: FullSyncIssuerWitnessModeHeadV3::V2(v2.anchor().clone()),
            },
            &key(4),
        )
        .unwrap(),
        history: FullSyncIssuerHistoryV3 {
            v2_entries: v2.records().to_vec(),
            v3_entries: vec![],
        },
        original_migration_receipt: None,
    };

    assert!(recover_migration_resume_v3(old, v3, &expected_query, replayed_response).is_err());
}

#[test]
fn v3_migration_resume_rejects_a_replayed_v3_mode_response() {
    let old = old_policy();
    let v2 = current_v2_state(&old);
    let v3 = new_policy(&old);
    let (v3_state, migration_entry) = migrated_state(&old, &v3, &v2, 85);
    let original_observation = match &migration_entry.receipt.body.observation {
        FullSyncIssuerReceiptObservationV3::Migration(observation) => observation,
        FullSyncIssuerReceiptObservationV3::V3(_) => panic!("migration receipt must use V2"),
    };
    let migration = match &migration_entry.record.event {
        FullSyncIssuerEventV3::Migration(migration) => migration.as_ref().clone(),
        _ => panic!("first V3 entry must be the migration"),
    };
    let original_query = SignedFullSyncIssuerMigrationResumeQueryV3::sign(
        &v3,
        original_observation.body.query.clone(),
        migration.clone(),
        [86; 32],
        &key(8),
    )
    .unwrap();
    let expected_query = SignedFullSyncIssuerMigrationResumeQueryV3::sign(
        &v3,
        original_observation.body.query.clone(),
        migration,
        [87; 32],
        &key(8),
    )
    .unwrap();
    let replayed_response = FullSyncIssuerMigrationResumeBundleV3 {
        observation: SignedFullSyncIssuerMigrationResumeObservationV3::sign(
            FullSyncIssuerMigrationResumeObservationBodyV3 {
                schema_version: 3,
                query: original_query,
                current_mode_head: FullSyncIssuerWitnessModeHeadV3::V3(v3_state.anchor().clone()),
            },
            &key(4),
        )
        .unwrap(),
        history: FullSyncIssuerHistoryV3 {
            v2_entries: v2.records().to_vec(),
            v3_entries: v3_state.entries().to_vec(),
        },
        original_migration_receipt: Some(migration_entry.receipt.clone()),
    };

    assert!(recover_migration_resume_v3(old, v3, &expected_query, replayed_response).is_err());
}
