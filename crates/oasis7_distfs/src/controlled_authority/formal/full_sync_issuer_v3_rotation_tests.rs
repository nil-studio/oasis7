use super::*;

fn rotated_v2_policy(previous: &VerifiedFullSyncIssuerPolicyV2) -> VerifiedFullSyncIssuerPolicyV2 {
    let mut body = previous.signed().body.clone();
    body.signer_generation += 1;
    body.issuer_public_key = super::super::public(8);
    body.management_public_key = super::super::public(9);
    let signed = SignedFullSyncIssuerPolicyV2::sign(body, &super::super::key(1)).unwrap();
    VerifiedFullSyncIssuerPolicyV2::verify(&signed, &super::super::trust()).unwrap()
}

fn generation_three_v3_policy(
    active_v2: &VerifiedFullSyncIssuerPolicyV2,
) -> VerifiedFullSyncIssuerPolicyV3 {
    let initial_v2 = super::super::old_policy();
    let mut body = super::super::new_policy(&initial_v2).signed().body.clone();
    body.signer_generation = active_v2.body().signer_generation + 1;
    body.management_public_key = super::super::public(10);
    body.lease_signer_public_key = super::super::public(11);
    body.provider_public_keys = vec![super::super::public(12)];
    let signed = SignedFullSyncIssuerPolicyV3::sign(body, &super::super::key(1)).unwrap();
    VerifiedFullSyncIssuerPolicyV3::verify(&signed, &super::super::trust(), active_v2).unwrap()
}

fn leave_a_migration_pending(
    label: &str,
) -> (
    TempDir,
    VerifiedFullSyncIssuerPolicyV2,
    VerifiedFullSyncIssuerPolicyV3,
    RacingWitness,
    SignedFullSyncIssuerMigrationV3,
) {
    let directory = TempDir::new(&format!("{label}-a"));
    let old = super::super::old_policy();
    let a_policy = super::super::new_policy(&old);
    let prefix = vec![super::super::seeded_v2_entry(&old)];
    let state =
        VerifiedFullSyncIssuerStateV2::from_committed_records(old.clone(), prefix.clone()).unwrap();
    let a_migration = super::super::migration(&old, &a_policy, state.anchor().clone(), 181);
    let mut witness = RacingWitness::new(old.clone(), a_policy.clone(), prefix);
    witness.fail_next_migration_before_commit = true;
    assert!(
        DurableFullSyncIssuerV3::migrate_from_v2(
            migration_config(
                directory.path(),
                old.clone(),
                a_policy.clone(),
                super::super::key(3),
                super::super::key(7),
                v2_genesis(&old),
            ),
            a_migration.clone(),
            &mut witness,
        )
        .is_err()
    );
    assert!(directory.path().join(PENDING_FILE).is_file());
    assert!(witness.inner.new_entries.is_empty());
    (directory, old, a_policy, witness, a_migration)
}

#[test]
fn pending_migration_reconciles_after_v2_policy_rotation_wins() {
    let (directory, old, a_policy, mut witness, a_migration) =
        leave_a_migration_pending("migration-a-v2-rotation-wins");
    let current_v2 = rotated_v2_policy(&old);
    witness
        .commit_v2_policy_rotation(current_v2.clone(), 183)
        .unwrap();
    let current_state = VerifiedFullSyncIssuerStateV2::from_committed_records(
        current_v2.clone(),
        witness.inner.old_entries.clone(),
    )
    .unwrap();
    let expected_current_anchor = current_state.anchor().clone();

    let recovered = DurableFullSyncIssuerV3::resume_migration_v3(
        directory.path(),
        old.clone(),
        a_policy,
        super::super::key(7),
        a_migration,
        &mut witness,
    )
    .unwrap();
    assert!(matches!(
        recovered,
        FullSyncIssuerMigrationResumeV3::NotCommittedV2 {
            ref current_anchor,
            migration_head_conflict: true,
        } if current_anchor == &expected_current_anchor
    ));
    assert!(!directory.path().join(PENDING_FILE).exists());
    let ReadLedgerFile::V2(local) = read_local_ledger(directory.path()).unwrap().unwrap() else {
        panic!("a V2 policy rotation winner must remain a V2 ledger");
    };
    assert_eq!(local.entries, witness.inner.old_entries);

    let before_stale_open = snapshot_store_files(directory.path());
    assert!(
        DurableFullSyncIssuerV2::open(
            directory.path(),
            old.clone(),
            super::super::key(3),
            v2_genesis(&old),
            &mut witness,
        )
        .is_err()
    );
    assert_eq!(snapshot_store_files(directory.path()), before_stale_open);

    let current = DurableFullSyncIssuerV2::open(
        directory.path(),
        current_v2,
        super::super::key(8),
        expected_current_anchor.clone(),
        &mut witness,
    )
    .unwrap();
    assert_eq!(current.minimum_anchor(), &expected_current_anchor);
}

#[test]
fn pending_migration_reconciles_after_rotated_v2_and_generation_three_migration_win() {
    let (directory, old, a_policy, mut witness, a_migration) =
        leave_a_migration_pending("migration-a-rotated-v3-winner");
    let current_v2 = rotated_v2_policy(&old);
    witness
        .commit_v2_policy_rotation(current_v2.clone(), 185)
        .unwrap();
    let winner_policy = generation_three_v3_policy(&current_v2);
    witness.inner.new_policy = winner_policy.clone();

    let current_v2_state = VerifiedFullSyncIssuerStateV2::from_committed_records(
        current_v2.clone(),
        witness.inner.old_entries.clone(),
    )
    .unwrap();
    let b_migration = super::super::migration(
        &current_v2,
        &winner_policy,
        current_v2_state.anchor().clone(),
        187,
    );
    let b_directory = TempDir::new("migration-a-rotated-v3-winner-b");
    let b_entry = DurableFullSyncIssuerV3::migrate_from_v2(
        migration_config(
            b_directory.path(),
            current_v2.clone(),
            winner_policy.clone(),
            super::super::key(8),
            super::super::key(11),
            v2_genesis(&current_v2),
        ),
        b_migration,
        &mut witness,
    )
    .unwrap();
    assert_eq!(b_entry.record.next_anchor.signer_generation, 3);
    assert_eq!(witness.inner.new_entries, vec![b_entry.clone()]);

    let recovered = DurableFullSyncIssuerV3::resume_migration_v3(
        directory.path(),
        old,
        a_policy,
        super::super::key(7),
        a_migration,
        &mut witness,
    );
    assert!(
        matches!(&recovered, Err(FormalError::Conflict(_))),
        "A must retire as a non-serving conflict after B's gen-3 migration wins: {recovered:?}"
    );
    assert!(!directory.path().join(PENDING_FILE).exists());
    let ReadLedgerFile::V3(local) = read_local_ledger(directory.path()).unwrap().unwrap() else {
        panic!("A must fsync the authenticated V3 winner history before clearing its intent");
    };
    assert_eq!(local.v2_entries, witness.inner.old_entries);
    assert_eq!(local.v3_entries, witness.inner.new_entries);
    assert_eq!(local.v3_entries, vec![b_entry.clone()]);

    let pinned = b_entry.record.next_anchor.clone();
    let reopened = DurableFullSyncIssuerV3::open_v3(
        directory.path(),
        current_v2,
        winner_policy,
        super::super::key(11),
        pinned.clone(),
        &mut witness,
    )
    .unwrap();
    assert_eq!(reopened.minimum_anchor(), &pinned);
}
