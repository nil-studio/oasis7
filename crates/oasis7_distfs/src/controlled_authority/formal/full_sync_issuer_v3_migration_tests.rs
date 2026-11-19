use super::*;

#[test]
fn migration_resolves_v2_pending_only_with_complete_witness_proof() {
    let old = old_policy();
    let new = new_policy(&old);

    // A pending command present exactly in the authenticated V2 history is
    // recovered as committed. Migration is stopped until a fresh manager
    // decision is supplied, and the durable V2 cache is repaired first.
    let committed_dir = TempDir::new("migration-v2-pending-committed");
    let committed_prefix = vec![seeded_v2_entry(&old)];
    let committed_prefix_state = VerifiedFullSyncIssuerStateV2::from_committed_records(
        old.clone(),
        committed_prefix.clone(),
    )
    .unwrap();
    let (committed_command, committed_record) = v2_candidate(&old, &committed_prefix_state, 10, 90);
    let recovered_entry =
        committed_v2_command(&old, &committed_prefix_state, committed_command.clone(), 91);
    let mut committed_witness = SimulatedWitness::new(
        old.clone(),
        new.clone(),
        [committed_prefix.clone(), vec![recovered_entry.clone()]].concat(),
    );
    write_v2_pending_fixture(
        committed_dir.path(),
        committed_prefix.clone(),
        committed_command,
        committed_record,
    );
    let committed_head = committed_witness.v2_state().unwrap().anchor().clone();
    let committed_migration = migration(&old, &new, committed_head, 92);
    assert!(
        DurableFullSyncIssuerV3::migrate_from_v2(
            migration_config(
                committed_dir.path(),
                old.clone(),
                new.clone(),
                key(3),
                key(7),
                v2_genesis(&old),
            ),
            committed_migration,
            &mut committed_witness,
        )
        .is_err()
    );
    assert!(!committed_dir.path().join(PENDING_FILE).exists());
    assert_eq!(committed_witness.migration_commits, 0);
    let ReadLedgerFile::V2(committed_cache) =
        read_local_ledger(committed_dir.path()).unwrap().unwrap()
    else {
        panic!("resolving a V2 command must not upgrade around its decision");
    };
    assert_eq!(committed_cache.entries, committed_witness.old_entries);
    assert_eq!(committed_cache.entries.last(), Some(&recovered_entry));

    // If the exact pending CAS did not commit and the head is unchanged, it
    // remains unresolved. Migration must neither clear it nor supersede it.
    let same_head_dir = TempDir::new("migration-v2-pending-same-head");
    let same_head_prefix = vec![seeded_v2_entry(&old)];
    let same_head_state = VerifiedFullSyncIssuerStateV2::from_committed_records(
        old.clone(),
        same_head_prefix.clone(),
    )
    .unwrap();
    let (same_head_command, same_head_record) = v2_candidate(&old, &same_head_state, 10, 100);
    let mut same_head_witness =
        SimulatedWitness::new(old.clone(), new.clone(), same_head_prefix.clone());
    write_v2_pending_fixture(
        same_head_dir.path(),
        same_head_prefix,
        same_head_command,
        same_head_record,
    );
    let same_head_pending = fs::read(same_head_dir.path().join(PENDING_FILE)).unwrap();
    let same_head_ledger = fs::read(same_head_dir.path().join(LEDGER_FILE)).unwrap();
    assert!(
        DurableFullSyncIssuerV3::migrate_from_v2(
            migration_config(
                same_head_dir.path(),
                old.clone(),
                new.clone(),
                key(3),
                key(7),
                v2_genesis(&old),
            ),
            migration(&old, &new, same_head_state.anchor().clone(), 102),
            &mut same_head_witness,
        )
        .is_err()
    );
    assert_eq!(
        fs::read(same_head_dir.path().join(PENDING_FILE)).unwrap(),
        same_head_pending
    );
    assert_eq!(
        fs::read(same_head_dir.path().join(LEDGER_FILE)).unwrap(),
        same_head_ledger
    );
    assert_eq!(same_head_witness.migration_commits, 0);

    // A different valid V2 event advanced the head and the pending request ID
    // is absent. Replaying the candidate at its exact predecessor proves its
    // CAS lost, permitting cache repair and pending retirement, but not an
    // automatic migration using the stale approval.
    let advanced_dir = TempDir::new("migration-v2-pending-advanced");
    let advanced_prefix = vec![seeded_v2_entry(&old)];
    let advanced_prefix_state =
        VerifiedFullSyncIssuerStateV2::from_committed_records(old.clone(), advanced_prefix.clone())
            .unwrap();
    let (loser_command, loser_record) = v2_candidate(&old, &advanced_prefix_state, 10, 110);
    let winner_command = v2_issue_command(&old, &advanced_prefix_state, 11, 112);
    let winner = committed_v2_command(&old, &advanced_prefix_state, winner_command, 113);
    let mut advanced_witness = SimulatedWitness::new(
        old.clone(),
        new.clone(),
        [advanced_prefix.clone(), vec![winner]].concat(),
    );
    write_v2_pending_fixture(
        advanced_dir.path(),
        advanced_prefix,
        loser_command,
        loser_record,
    );
    let advanced_head = advanced_witness.v2_state().unwrap().anchor().clone();
    assert!(
        DurableFullSyncIssuerV3::migrate_from_v2(
            migration_config(
                advanced_dir.path(),
                old.clone(),
                new.clone(),
                key(3),
                key(7),
                v2_genesis(&old),
            ),
            migration(&old, &new, advanced_head, 114),
            &mut advanced_witness,
        )
        .is_err()
    );
    assert!(!advanced_dir.path().join(PENDING_FILE).exists());
    assert_eq!(advanced_witness.migration_commits, 0);
    let ReadLedgerFile::V2(advanced_cache) =
        read_local_ledger(advanced_dir.path()).unwrap().unwrap()
    else {
        panic!("a proven lost V2 CAS must be repaired within V2 mode");
    };
    assert_eq!(advanced_cache.entries, advanced_witness.old_entries);
}

#[test]
fn migration_refuses_a_validly_encoded_local_fork_without_repairing_over_it() {
    let temp = TempDir::new("migration-local-fork");
    let old = old_policy();
    let new = new_policy(&old);
    let entry = seeded_v2_entry(&old);
    let v2_prefix = vec![entry.clone()];
    let v2_state =
        VerifiedFullSyncIssuerStateV2::from_committed_records(old.clone(), v2_prefix.clone())
            .unwrap();
    let migration = migration(&old, &new, v2_state.anchor().clone(), 80);
    let mut witness = SimulatedWitness::new(old.clone(), new.clone(), v2_prefix.clone());

    let mut forked = entry;
    forked.record.next_anchor.state_digest = hash(81);
    let local = super::super::LedgerFile {
        schema_version: 2,
        entries: vec![forked],
    };
    let bytes = serde_json::to_vec(&local).unwrap();
    atomic_bounded(
        temp.path(),
        &temp.path().join(LEDGER_FILE),
        &bytes,
        MAX_LEDGER_BYTES,
    )
    .unwrap();

    let result = DurableFullSyncIssuerV3::migrate_from_v2(
        migration_config(
            temp.path(),
            old.clone(),
            new,
            key(3),
            key(7),
            v2_genesis(&old),
        ),
        migration,
        &mut witness,
    );
    assert!(result.is_err(), "a local history fork must stop migration");
    assert_eq!(fs::read(temp.path().join(LEDGER_FILE)).unwrap(), bytes);
    assert_eq!(witness.migration_commits, 0);
    assert!(!temp.path().join(PENDING_FILE).exists());
}
