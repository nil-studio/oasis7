use super::*;

fn alternate_policy(old: &VerifiedFullSyncIssuerPolicyV2) -> VerifiedFullSyncIssuerPolicyV3 {
    let mut body = new_policy(old).signed().body.clone();
    body.management_public_key = public(9);
    let signed = SignedFullSyncIssuerPolicyV3::sign(body, &key(1)).unwrap();
    VerifiedFullSyncIssuerPolicyV3::verify(&signed, &trust(), old).unwrap()
}

fn signed_migration(
    old: &VerifiedFullSyncIssuerPolicyV2,
    policy: &VerifiedFullSyncIssuerPolicyV3,
    state: &VerifiedFullSyncIssuerStateV2,
    request_id: String,
    nonce: u8,
) -> (
    SignedFullSyncIssuerMigrationV3,
    SignedFullSyncIssuerObservationV2,
) {
    let observation = v2_observation(old, state.anchor(), nonce);
    let migration = SignedFullSyncIssuerMigrationV3::sign(
        FullSyncIssuerMigrationBodyV3 {
            schema_version: 3,
            previous_v2_policy_digest: old.policy_digest().into(),
            previous_v2_signer_generation: old.body().signer_generation,
            expected_v2_head: state.anchor().clone(),
            new_policy: policy.signed().clone(),
            request_id,
            manager_nonce: hash(nonce.wrapping_add(1)),
        },
        &key(2),
    )
    .unwrap();
    (migration, observation)
}

fn commit_migration(
    old: &VerifiedFullSyncIssuerPolicyV2,
    policy: &VerifiedFullSyncIssuerPolicyV3,
    state: &VerifiedFullSyncIssuerStateV2,
    request_id: String,
    nonce: u8,
) -> (
    VerifiedFullSyncIssuerStateV3,
    FullSyncIssuerCommittedEntryV3,
) {
    let (migration, observation) = signed_migration(old, policy, state, request_id, nonce);
    let prepared = state
        .prepare_migration_v3(policy.clone(), migration, &observation)
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

fn v2_winner(
    policy: &VerifiedFullSyncIssuerPolicyV2,
    state: &VerifiedFullSyncIssuerStateV2,
) -> VerifiedFullSyncIssuerStateV2 {
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
            operation: FullSyncIssuerOperationV2::Replace {
                not_before_unix_ms: 1_000,
                expires_unix_ms: 10_000,
            },
            expected_head: state.anchor().clone(),
            expected_target_generation: 1,
            request_id: hash(203),
            command_nonce: hash(204),
        },
        &key(2),
    )
    .unwrap();
    let prepared = state.prepare(command).unwrap();
    let record = prepared.record().unwrap().clone();
    let observation = v2_observation(policy, state.anchor(), 205);
    let receipt = SignedFullSyncIssuerReceiptV2::sign(observation, &record, &key(4)).unwrap();
    prepared.commit(receipt).unwrap().state().clone()
}

#[test]
fn v3_migration_resume_proves_competing_migration_and_rejects_request_id_reuse() {
    let old = old_policy();
    let requested_policy = new_policy(&old);
    let original_v2_state = current_v2_state(&old);
    let (requested_migration, original_observation) =
        signed_migration(&old, &requested_policy, &original_v2_state, hash(201), 200);
    let expected_query = SignedFullSyncIssuerMigrationResumeQueryV3::sign(
        &requested_policy,
        original_observation.body.query.clone(),
        requested_migration.clone(),
        [202; 32],
        &key(8),
    )
    .unwrap();

    let v2_winner = v2_winner(&old, &original_v2_state);
    let competing_policy = alternate_policy(&old);
    let (competing_state, competing_entry) =
        commit_migration(&old, &competing_policy, &v2_winner, hash(206), 207);
    let bundle = FullSyncIssuerMigrationResumeBundleV3 {
        observation: SignedFullSyncIssuerMigrationResumeObservationV3::sign(
            FullSyncIssuerMigrationResumeObservationBodyV3 {
                schema_version: 3,
                query: expected_query.clone(),
                current_mode_head: FullSyncIssuerWitnessModeHeadV3::V3(
                    competing_state.anchor().clone(),
                ),
            },
            &key(4),
        )
        .unwrap(),
        history: FullSyncIssuerHistoryV3 {
            v2_entries: v2_winner.records().to_vec(),
            v3_entries: competing_state.entries().to_vec(),
        },
        original_migration_receipt: None,
    };

    match recover_migration_resume_v3(
        old.clone(),
        requested_policy.clone(),
        &expected_query,
        bundle,
    )
    .unwrap()
    {
        FullSyncIssuerMigrationResumeResultV3::NotCommittedV3MigrationConflict(proof) => {
            assert_eq!(proof.current_anchor(), competing_state.anchor());
            assert_eq!(proof.migration_entry(), &competing_entry);
            assert_eq!(
                proof.policy().policy_digest(),
                competing_policy.policy_digest()
            );
            assert_eq!(proof.history().v2_entries.as_slice(), v2_winner.records());
            assert_eq!(
                proof.history().v3_entries.as_slice(),
                competing_state.entries()
            );
        }
        FullSyncIssuerMigrationResumeResultV3::Committed(_) => {
            panic!("a different root-authorized migration must not recover A as committed")
        }
        FullSyncIssuerMigrationResumeResultV3::NotCommittedV2 { .. } => {
            panic!("the witness response is in V3 mode")
        }
    }

    let (same_id_state, _) = commit_migration(
        &old,
        &competing_policy,
        &v2_winner,
        requested_migration.body.request_id.clone(),
        208,
    );
    let reused_id_bundle = FullSyncIssuerMigrationResumeBundleV3 {
        observation: SignedFullSyncIssuerMigrationResumeObservationV3::sign(
            FullSyncIssuerMigrationResumeObservationBodyV3 {
                schema_version: 3,
                query: expected_query.clone(),
                current_mode_head: FullSyncIssuerWitnessModeHeadV3::V3(
                    same_id_state.anchor().clone(),
                ),
            },
            &key(4),
        )
        .unwrap(),
        history: FullSyncIssuerHistoryV3 {
            v2_entries: v2_winner.records().to_vec(),
            v3_entries: same_id_state.entries().to_vec(),
        },
        original_migration_receipt: None,
    };
    assert!(
        recover_migration_resume_v3(old, requested_policy, &expected_query, reused_id_bundle,)
            .is_err()
    );
}
