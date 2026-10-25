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

fn rotate_v2_policy_and_commit(
    old_policy: &VerifiedFullSyncIssuerPolicyV2,
    old_state: &VerifiedFullSyncIssuerStateV2,
) -> (
    VerifiedFullSyncIssuerPolicyV2,
    VerifiedFullSyncIssuerStateV2,
) {
    let mut body = old_policy.signed().body.clone();
    body.management_public_key = public(10);
    body.issuer_public_key = public(9);
    body.signer_generation = old_policy.body().signer_generation + 1;
    let signed_policy = SignedFullSyncIssuerPolicyV2::sign(body, &key(1)).unwrap();
    let rotated_policy = VerifiedFullSyncIssuerPolicyV2::verify(&signed_policy, &trust()).unwrap();
    let transition = crate::controlled_authority::formal::FullSyncIssuerPolicyTransitionV2 {
        previous_policy_digest: old_policy.policy_digest().into(),
        previous_signer_generation: old_policy.body().signer_generation,
        new_policy_digest: rotated_policy.policy_digest().into(),
        new_signer_generation: rotated_policy.body().signer_generation,
    };
    let rotated_config = VerifiedFullSyncIssuerStateV2::from_committed_records(
        rotated_policy.clone(),
        old_state.records().to_vec(),
    )
    .unwrap();
    let command = SignedFullSyncIssuerCommandV2::sign(
        FullSyncIssuerCommandBodyV2 {
            schema_version: 2,
            world: rotated_policy.body().world.clone(),
            authority_epoch: rotated_policy.body().authority_epoch,
            scope_digest: rotated_policy.scope_digest().into(),
            policy_digest: rotated_policy.policy_digest().into(),
            signer_generation: rotated_policy.body().signer_generation,
            transition: Some(transition),
            target: FullSyncIssuerTargetV2 {
                node_public_key: public(11),
                role: FullSyncReadRoleV2::StateSync,
            },
            operation: FullSyncIssuerOperationV2::Issue {
                not_before_unix_ms: 1_000,
                expires_unix_ms: 10_000,
            },
            expected_head: rotated_config.anchor().clone(),
            expected_target_generation: 0,
            request_id: hash(230),
            command_nonce: hash(231),
        },
        &key(10),
    )
    .unwrap();
    let prepared = rotated_config.prepare(command).unwrap();
    let record = prepared.record().unwrap().clone();
    let query = SignedFullSyncIssuerQueryV2::sign(
        &rotated_policy,
        record.previous_anchor.clone(),
        [232; 32],
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
    (
        rotated_policy,
        prepared.commit(receipt).unwrap().state().clone(),
    )
}

fn v3_policy_after_v2_rotation(
    original_v2: &VerifiedFullSyncIssuerPolicyV2,
    active_v2: &VerifiedFullSyncIssuerPolicyV2,
) -> VerifiedFullSyncIssuerPolicyV3 {
    let mut body = new_policy(original_v2).signed().body.clone();
    body.management_public_key = public(12);
    body.lease_signer_public_key = public(13);
    body.signer_generation = active_v2.body().signer_generation + 1;
    let signed = SignedFullSyncIssuerPolicyV3::sign(body, &key(1)).unwrap();
    VerifiedFullSyncIssuerPolicyV3::verify(&signed, &trust(), active_v2).unwrap()
}

fn signed_migration_by_manager(
    v2_policy: &VerifiedFullSyncIssuerPolicyV2,
    v3_policy: &VerifiedFullSyncIssuerPolicyV3,
    state: &VerifiedFullSyncIssuerStateV2,
    request_id: u8,
    nonce: u8,
    issuer_key: u8,
    manager_key: u8,
) -> (
    SignedFullSyncIssuerMigrationV3,
    SignedFullSyncIssuerObservationV2,
) {
    let query = SignedFullSyncIssuerQueryV2::sign(
        v2_policy,
        state.anchor().clone(),
        [nonce; 32],
        &key(issuer_key),
    )
    .unwrap();
    let observation = SignedFullSyncIssuerObservationV2::sign(
        FullSyncIssuerObservationBodyV2 {
            schema_version: 2,
            query,
            current_anchor: state.anchor().clone(),
        },
        &key(4),
    )
    .unwrap();
    let migration = SignedFullSyncIssuerMigrationV3::sign(
        FullSyncIssuerMigrationBodyV3 {
            schema_version: 3,
            previous_v2_policy_digest: v2_policy.policy_digest().into(),
            previous_v2_signer_generation: v2_policy.body().signer_generation,
            expected_v2_head: state.anchor().clone(),
            new_policy: v3_policy.signed().clone(),
            request_id: hash(request_id),
            manager_nonce: hash(nonce.wrapping_add(1)),
        },
        &key(manager_key),
    )
    .unwrap();
    (migration, observation)
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

#[test]
fn migration_resume_proves_old_request_absent_after_v2_policy_rotation_wins() {
    let old_policy = old_policy();
    let original_state = current_v2_state(&old_policy);
    let requested_policy = new_policy(&old_policy);
    let (requested_migration, original_observation) = signed_migration(
        &old_policy,
        &requested_policy,
        &original_state,
        hash(240),
        241,
    );
    requested_migration
        .verify(&old_policy, &requested_policy)
        .expect("A migration is valid under the old V2 manager at its exact prefix");
    let expected_query = SignedFullSyncIssuerMigrationResumeQueryV3::sign(
        &requested_policy,
        original_observation.body.query.clone(),
        requested_migration.clone(),
        [242; 32],
        &key(8),
    )
    .unwrap();

    let (rotated_policy, rotated_state) = rotate_v2_policy_and_commit(&old_policy, &original_state);
    assert_eq!(rotated_policy.body().signer_generation, 2);
    assert_eq!(
        rotated_state.anchor().revision,
        original_state.anchor().revision + 1
    );
    assert!(
        VerifiedFullSyncIssuerStateV2::from_committed_records(
            old_policy.clone(),
            rotated_state.records().to_vec(),
        )
        .is_err(),
        "serving restore must retain the configured-policy generation fence"
    );
    assert_eq!(
        VerifiedFullSyncIssuerStateV2::from_committed_records(
            rotated_policy.clone(),
            rotated_state.records().to_vec(),
        )
        .unwrap()
        .anchor(),
        rotated_state.anchor(),
        "serving restore with the current rotated policy must still succeed"
    );

    let bundle = FullSyncIssuerMigrationResumeBundleV3 {
        observation: SignedFullSyncIssuerMigrationResumeObservationV3::sign(
            FullSyncIssuerMigrationResumeObservationBodyV3 {
                schema_version: 3,
                query: expected_query.clone(),
                current_mode_head: FullSyncIssuerWitnessModeHeadV3::V2(
                    rotated_state.anchor().clone(),
                ),
            },
            &key(4),
        )
        .unwrap(),
        history: FullSyncIssuerHistoryV3 {
            v2_entries: rotated_state.records().to_vec(),
            v3_entries: Vec::new(),
        },
        original_migration_receipt: None,
    };

    match recover_migration_resume_v3(old_policy, requested_policy, &expected_query, bundle)
        .expect("fresh full-history proof should resolve A as uncommitted after V2 rotation")
    {
        FullSyncIssuerMigrationResumeResultV3::NotCommittedV2 {
            current_state,
            current_anchor,
            migration_head_conflict,
        } => {
            assert_eq!(current_anchor, *rotated_state.anchor());
            assert_eq!(current_state.anchor(), &current_anchor);
            assert!(migration_head_conflict);
        }
        FullSyncIssuerMigrationResumeResultV3::Committed(_) => {
            panic!("the uncommitted A migration must not become a serving V3 migration")
        }
        FullSyncIssuerMigrationResumeResultV3::NotCommittedV3MigrationConflict(_) => {
            panic!("the witness reports V2 mode after the winning policy rotation")
        }
    }
}

#[test]
fn migration_resume_proves_old_request_absent_after_rotated_v2_then_competing_v3_migration() {
    let old_policy = old_policy();
    let original_state = current_v2_state(&old_policy);
    let requested_policy = new_policy(&old_policy);
    let (requested_migration, original_observation) = signed_migration(
        &old_policy,
        &requested_policy,
        &original_state,
        hash(250),
        251,
    );
    let expected_query = SignedFullSyncIssuerMigrationResumeQueryV3::sign(
        &requested_policy,
        original_observation.body.query.clone(),
        requested_migration.clone(),
        [252; 32],
        &key(8),
    )
    .unwrap();

    let (rotated_policy, rotated_state) = rotate_v2_policy_and_commit(&old_policy, &original_state);
    let winning_policy = v3_policy_after_v2_rotation(&old_policy, &rotated_policy);
    let (winning_migration, winning_observation) = signed_migration_by_manager(
        &rotated_policy,
        &winning_policy,
        &rotated_state,
        253,
        254,
        9,
        10,
    );
    let prepared = rotated_state
        .prepare_migration_v3(
            winning_policy.clone(),
            winning_migration,
            &winning_observation,
        )
        .expect("active rotated V2 manager may authorize the competing migration");
    let record = prepared.record().unwrap().clone();
    let receipt = SignedFullSyncIssuerReceiptV3::sign(
        FullSyncIssuerReceiptObservationV3::Migration(winning_observation),
        &record,
        &key(4),
    )
    .unwrap();
    let winning_state = prepared.commit(receipt).unwrap().state().clone();

    let bundle = FullSyncIssuerMigrationResumeBundleV3 {
        observation: SignedFullSyncIssuerMigrationResumeObservationV3::sign(
            FullSyncIssuerMigrationResumeObservationBodyV3 {
                schema_version: 3,
                query: expected_query.clone(),
                current_mode_head: FullSyncIssuerWitnessModeHeadV3::V3(
                    winning_state.anchor().clone(),
                ),
            },
            &key(4),
        )
        .unwrap(),
        history: FullSyncIssuerHistoryV3 {
            v2_entries: rotated_state.records().to_vec(),
            v3_entries: winning_state.entries().to_vec(),
        },
        original_migration_receipt: None,
    };

    match recover_migration_resume_v3(old_policy, requested_policy, &expected_query, bundle)
        .expect("complete rotated V2/V3 history should prove A absent")
    {
        FullSyncIssuerMigrationResumeResultV3::NotCommittedV3MigrationConflict(proof) => {
            assert_eq!(proof.current_anchor(), winning_state.anchor());
            assert_eq!(proof.history().v2_entries, rotated_state.records());
            assert_eq!(proof.history().v3_entries, winning_state.entries());
            assert_eq!(proof.migration_entry().record, record);
            assert_eq!(
                proof.policy().policy_digest(),
                winning_policy.policy_digest()
            );
        }
        FullSyncIssuerMigrationResumeResultV3::Committed(_) => {
            panic!("the uncommitted A migration must not become the V3 winner")
        }
        FullSyncIssuerMigrationResumeResultV3::NotCommittedV2 { .. } => {
            panic!("the witness reports V3 mode after B's migration")
        }
    }
}
