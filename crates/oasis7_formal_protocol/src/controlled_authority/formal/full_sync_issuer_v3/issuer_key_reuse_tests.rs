use super::*;
use crate::controlled_authority::formal::FullSyncIssuerPolicyTransitionV2;

fn rotated_v2_policy(old: &VerifiedFullSyncIssuerPolicyV2) -> VerifiedFullSyncIssuerPolicyV2 {
    let mut body = old.signed().body.clone();
    body.management_public_key = public(10);
    body.issuer_public_key = public(9);
    body.signer_generation = 2;
    let signed = SignedFullSyncIssuerPolicyV2::sign(body, &key(1)).unwrap();
    VerifiedFullSyncIssuerPolicyV2::verify(&signed, &trust()).unwrap()
}

fn v3_policy_with_lease_signer(
    active: &VerifiedFullSyncIssuerPolicyV2,
    lease_signer: u8,
) -> VerifiedFullSyncIssuerPolicyV3 {
    let trust = trust();
    let signed = SignedFullSyncIssuerPolicyV3::sign(
        FullSyncIssuerPolicyBodyV3 {
            schema_version: 3,
            root_public_key: public(1),
            activation_trust_digest: full_sync_activation_trust_digest_v3(&trust).unwrap(),
            world: active.body().world.clone(),
            authority_epoch: active.body().authority_epoch,
            scope_digest: active.scope_digest().into(),
            allowed_roles: vec![FullSyncReadRoleV2::StateSync],
            provider_public_keys: vec![public(6)],
            management_public_key: public(7),
            lease_signer_public_key: public(lease_signer),
            witness_public_key: active.body().witness_public_key.clone(),
            signer_generation: active.body().signer_generation + 1,
            max_authorization_staleness_ms: 30_000,
            max_clock_uncertainty_ms: 100,
            clock_profile_digest: hash(35),
        },
        &key(1),
    )
    .unwrap();
    VerifiedFullSyncIssuerPolicyV3::verify(&signed, &trust, active).unwrap()
}

fn rotated_policy_state(
    old: &VerifiedFullSyncIssuerPolicyV2,
) -> (
    VerifiedFullSyncIssuerPolicyV2,
    VerifiedFullSyncIssuerStateV2,
) {
    let initial = current_v2_state(old);
    let next_policy = rotated_v2_policy(old);
    let transition = FullSyncIssuerPolicyTransitionV2 {
        previous_policy_digest: old.policy_digest().into(),
        previous_signer_generation: old.body().signer_generation,
        new_policy_digest: next_policy.policy_digest().into(),
        new_signer_generation: next_policy.body().signer_generation,
    };
    let upgraded_config = VerifiedFullSyncIssuerStateV2::from_committed_records(
        next_policy.clone(),
        initial.records().to_vec(),
    )
    .unwrap();
    let command = SignedFullSyncIssuerCommandV2::sign(
        FullSyncIssuerCommandBodyV2 {
            schema_version: 2,
            world: next_policy.body().world.clone(),
            authority_epoch: next_policy.body().authority_epoch,
            scope_digest: next_policy.scope_digest().into(),
            policy_digest: next_policy.policy_digest().into(),
            signer_generation: next_policy.body().signer_generation,
            transition: Some(transition),
            target: FullSyncIssuerTargetV2 {
                node_public_key: public(11),
                role: FullSyncReadRoleV2::StateSync,
            },
            operation: FullSyncIssuerOperationV2::Issue {
                not_before_unix_ms: 1_000,
                expires_unix_ms: 10_000,
            },
            expected_head: upgraded_config.anchor().clone(),
            expected_target_generation: 0,
            request_id: hash(220),
            command_nonce: hash(221),
        },
        &key(10),
    )
    .unwrap();
    let prepared = upgraded_config.prepare(command).unwrap();
    let record = prepared.record().unwrap().clone();
    let query = SignedFullSyncIssuerQueryV2::sign(
        &next_policy,
        record.previous_anchor.clone(),
        [222; 32],
        &key(9),
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
    let state = prepared.commit(receipt).unwrap().state().clone();
    (next_policy, state)
}

fn migration_observation(
    policy: &VerifiedFullSyncIssuerPolicyV2,
    state: &VerifiedFullSyncIssuerStateV2,
    nonce: u8,
) -> SignedFullSyncIssuerObservationV2 {
    let query =
        SignedFullSyncIssuerQueryV2::sign(policy, state.anchor().clone(), [nonce; 32], &key(9))
            .unwrap();
    SignedFullSyncIssuerObservationV2::sign(
        FullSyncIssuerObservationBodyV2 {
            schema_version: 2,
            query,
            current_anchor: state.anchor().clone(),
        },
        &key(4),
    )
    .unwrap()
}

fn migration(
    active: &VerifiedFullSyncIssuerPolicyV2,
    state: &VerifiedFullSyncIssuerStateV2,
    v3_policy: &VerifiedFullSyncIssuerPolicyV3,
    request_id: u8,
    manager: u8,
) -> (
    SignedFullSyncIssuerMigrationV3,
    SignedFullSyncIssuerObservationV2,
) {
    let observation = migration_observation(active, state, request_id.wrapping_add(30));
    let migration = SignedFullSyncIssuerMigrationV3::sign(
        FullSyncIssuerMigrationBodyV3 {
            schema_version: 3,
            previous_v2_policy_digest: active.policy_digest().into(),
            previous_v2_signer_generation: active.body().signer_generation,
            expected_v2_head: state.anchor().clone(),
            new_policy: v3_policy.signed().clone(),
            request_id: hash(request_id),
            manager_nonce: hash(request_id.wrapping_add(40)),
        },
        &key(manager),
    )
    .unwrap();
    (migration, observation)
}

#[test]
fn v3_migration_uses_active_v2_manager_and_fences_retired_issuer_keys() {
    let old = old_policy();
    let (active_v2_policy, state) = rotated_policy_state(&old);
    assert_eq!(state.records().len(), 2);
    assert_eq!(state.anchor().signer_generation, 2);
    assert_eq!(
        state.records()[0].record.policy.body.issuer_public_key,
        public(3)
    );
    assert_eq!(
        state.records()[1].record.policy.body.issuer_public_key,
        public(9)
    );
    assert_eq!(active_v2_policy.body().issuer_public_key, public(9));

    let fresh_v3_policy = v3_policy_with_lease_signer(&active_v2_policy, 8);
    let (active_manager_migration, active_manager_observation) =
        migration(&active_v2_policy, &state, &fresh_v3_policy, 230, 10);
    let prepared = state
        .prepare_migration_v3(
            fresh_v3_policy.clone(),
            active_manager_migration.clone(),
            &active_manager_observation,
        )
        .expect("migration must accept the active rotated V2 manager");
    let record = prepared.record().unwrap().clone();
    let receipt = SignedFullSyncIssuerReceiptV3::sign(
        FullSyncIssuerReceiptObservationV3::Migration(active_manager_observation.clone()),
        &record,
        &key(4),
    )
    .unwrap();
    let committed = prepared.commit(receipt).unwrap();
    assert_eq!(committed.entry().record, record);

    let (old_manager_migration, old_manager_observation) =
        migration(&active_v2_policy, &state, &fresh_v3_policy, 231, 2);
    assert!(
        state
            .prepare_migration_v3(
                fresh_v3_policy,
                old_manager_migration,
                &old_manager_observation,
            )
            .is_err()
    );

    let reused_v3_policy = v3_policy_with_lease_signer(&active_v2_policy, 3);
    let (reused_old_issuer_migration, reused_observation) =
        migration(&active_v2_policy, &state, &reused_v3_policy, 232, 10);
    reused_old_issuer_migration
        .verify(&active_v2_policy, &reused_v3_policy)
        .expect("active manager signature and root-signed policy must be valid");
    assert!(
        state
            .prepare_migration_v3(
                reused_v3_policy,
                reused_old_issuer_migration,
                &reused_observation,
            )
            .is_err(),
        "V3 must not reuse a V2 issuer key retired by the verified prefix"
    );
}
