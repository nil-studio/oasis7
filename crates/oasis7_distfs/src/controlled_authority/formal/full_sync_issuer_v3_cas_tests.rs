use super::*;

#[derive(Clone)]
enum V3RaceWinner {
    Management(SignedFullSyncIssuerCommandV3),
    Lease(SignedFullSyncProviderLeaseRequestV3),
}

struct RacingWitness {
    inner: SimulatedWitness,
    resume_policy: VerifiedFullSyncIssuerPolicyV3,
    v2_winner_before_migration: Option<SignedFullSyncIssuerCommandV2>,
    fail_next_migration_before_commit: bool,
    v3_winner_before_event: Option<V3RaceWinner>,
    replay_resume_bundle: Option<FullSyncIssuerMigrationResumeBundleV3>,
    resume_queries: Vec<SignedFullSyncIssuerMigrationResumeQueryV3>,
    next_race_nonce: u8,
}

impl RacingWitness {
    fn new(
        old: VerifiedFullSyncIssuerPolicyV2,
        new: VerifiedFullSyncIssuerPolicyV3,
        old_entries: Vec<FullSyncIssuerCommittedEntryV2>,
    ) -> Self {
        Self {
            inner: SimulatedWitness::new(old, new.clone(), old_entries),
            resume_policy: new,
            v2_winner_before_migration: None,
            fail_next_migration_before_commit: false,
            v3_winner_before_event: None,
            replay_resume_bundle: None,
            resume_queries: Vec::new(),
            next_race_nonce: 212,
        }
    }

    fn commit_v2_command(
        &mut self,
        command: SignedFullSyncIssuerCommandV2,
        nonce: u8,
    ) -> Result<FullSyncIssuerCommittedEntryV2, FormalError> {
        let state = self.inner.v2_state()?;
        let query = SignedFullSyncIssuerQueryV2::sign(
            &self.inner.old_policy,
            state.anchor().clone(),
            [nonce; 32],
            &key(3),
        )?;
        let observation = SignedFullSyncIssuerObservationV2::sign(
            FullSyncIssuerObservationBodyV2 {
                schema_version: 2,
                query,
                current_anchor: state.anchor().clone(),
            },
            &key(4),
        )?;
        let record = state
            .prepare(command)?
            .record()
            .cloned()
            .ok_or_else(|| FormalError::Invalid("V2 winner has no candidate".into()))?;
        <Self as FullSyncIssuerWitnessV2>::commit(self, &observation, &record)?;
        self.inner
            .old_entries
            .last()
            .cloned()
            .ok_or_else(|| FormalError::Invalid("V2 winner was not appended".into()))
    }

    fn commit_v3_winner(
        &mut self,
        winner: V3RaceWinner,
        nonce: u8,
    ) -> Result<FullSyncIssuerCommittedEntryV3, FormalError> {
        let anchor = self.inner.v3_anchor()?;
        let query = SignedFullSyncIssuerQueryV3::sign(
            &self.inner.new_policy,
            anchor,
            [nonce; 32],
            &key(7),
        )?;
        let observation = self.inner.observe_v3(&query)?;
        let state = self.inner.state_v3_at(&observation)?;
        let prepared = match winner {
            V3RaceWinner::Management(command) => state.prepare_management(command, &observation)?,
            V3RaceWinner::Lease(request) => state.prepare_lease(request, &observation, &key(7))?,
        };
        let record = prepared
            .record()
            .cloned()
            .ok_or_else(|| FormalError::Invalid("V3 CAS winner has no candidate".into()))?;
        <SimulatedWitness as FullSyncIssuerWitnessV3>::commit_v3(
            &mut self.inner,
            &observation,
            &record,
        )?;
        self.inner
            .new_entries
            .last()
            .cloned()
            .ok_or_else(|| FormalError::Invalid("V3 winner was not appended".into()))
    }
}

impl FullSyncIssuerWitnessV2 for RacingWitness {
    fn observe(
        &mut self,
        query: &SignedFullSyncIssuerQueryV2,
    ) -> Result<SignedFullSyncIssuerObservationV2, FormalError> {
        self.inner.observe(query)
    }

    fn records_after(
        &mut self,
        after: &FullSyncIssuerAnchorV2,
        through: &FullSyncIssuerAnchorV2,
    ) -> Result<Vec<FullSyncIssuerCommittedEntryV2>, FormalError> {
        self.inner.records_after(after, through)
    }

    fn commit(
        &mut self,
        observation: &SignedFullSyncIssuerObservationV2,
        record: &FullSyncIssuerRecordV2,
    ) -> Result<SignedFullSyncIssuerReceiptV2, FormalError> {
        observation.verify(&self.inner.old_policy)?;
        if !self.inner.new_entries.is_empty() {
            return Err(FormalError::Rollback(
                "V2 CAS is fenced after V3 migration".into(),
            ));
        }
        let state = self.inner.v2_state()?;
        if observation.body.current_anchor != *state.anchor() {
            return Err(FormalError::Conflict("stale V2 witness CAS".into()));
        }
        let prepared = state.prepare(record.command.clone())?;
        if prepared.record() != Some(record) {
            return Err(FormalError::Conflict("V2 CAS candidate mismatch".into()));
        }
        let receipt = SignedFullSyncIssuerReceiptV2::sign(observation.clone(), record, &key(4))?;
        let entry = prepared.commit(receipt.clone())?.entry().clone();
        self.inner.old_entries.push(entry);
        Ok(receipt)
    }
}

impl FullSyncIssuerWitnessV3 for RacingWitness {
    fn commit_migration_v3(
        &mut self,
        observation: &SignedFullSyncIssuerObservationV2,
        record: &FullSyncIssuerRecordV3,
    ) -> Result<SignedFullSyncIssuerReceiptV3, FormalError> {
        if let Some(winner) = self.v2_winner_before_migration.take() {
            self.commit_v2_command(winner, 211)?;
        }
        if self.fail_next_migration_before_commit {
            self.fail_next_migration_before_commit = false;
            return Err(FormalError::Io(
                "simulated migration failed before witness CAS".into(),
            ));
        }
        self.inner.commit_migration_v3(observation, record)
    }

    fn observe_v3(
        &mut self,
        query: &SignedFullSyncIssuerQueryV3,
    ) -> Result<SignedFullSyncIssuerObservationV3, FormalError> {
        self.inner.observe_v3(query)
    }

    fn history_v3(
        &mut self,
        observation: &SignedFullSyncIssuerObservationV3,
    ) -> Result<FullSyncIssuerHistoryV3, FormalError> {
        self.inner.history_v3(observation)
    }

    fn commit_v3(
        &mut self,
        observation: &SignedFullSyncIssuerObservationV3,
        record: &FullSyncIssuerRecordV3,
    ) -> Result<SignedFullSyncIssuerReceiptV3, FormalError> {
        if let Some(winner) = self.v3_winner_before_event.take() {
            let nonce = self.next_race_nonce;
            self.next_race_nonce = self.next_race_nonce.wrapping_add(1);
            self.commit_v3_winner(winner, nonce)?;
        }
        self.inner.commit_v3(observation, record)
    }

    fn resume_migration_v3(
        &mut self,
        query: &SignedFullSyncIssuerMigrationResumeQueryV3,
    ) -> Result<FullSyncIssuerMigrationResumeBundleV3, FormalError> {
        self.resume_queries.push(query.clone());
        if let Some(bundle) = &self.replay_resume_bundle {
            return Ok(bundle.clone());
        }
        query.verify(&self.inner.old_policy, &self.resume_policy)?;
        let (current_mode_head, original_migration_receipt) = if self.inner.new_entries.is_empty() {
            (
                FullSyncIssuerWitnessModeHeadV3::V2(self.inner.v2_state()?.anchor().clone()),
                None,
            )
        } else {
            let original_migration_receipt = self.inner.new_entries.iter().find_map(|entry| {
                matches!(
                    &entry.record.event,
                    FullSyncIssuerEventV3::Migration(committed)
                        if committed.as_ref() == &query.body.exact_migration
                )
                .then(|| entry.receipt.clone())
            });
            (
                FullSyncIssuerWitnessModeHeadV3::V3(self.inner.v3_anchor()?),
                original_migration_receipt,
            )
        };
        Ok(FullSyncIssuerMigrationResumeBundleV3 {
            observation: SignedFullSyncIssuerMigrationResumeObservationV3::sign(
                FullSyncIssuerMigrationResumeObservationBodyV3 {
                    schema_version: 3,
                    query: query.clone(),
                    current_mode_head,
                },
                &key(4),
            )?,
            history: self.inner.history(),
            original_migration_receipt,
        })
    }
}

fn management_command(
    policy: &VerifiedFullSyncIssuerPolicyV3,
    head: &FullSyncIssuerAnchorV3,
    target: FullSyncIssuerTargetV2,
    generation: u64,
    operation: FullSyncIssuerOperationV3,
    identity: u8,
) -> SignedFullSyncIssuerCommandV3 {
    SignedFullSyncIssuerCommandV3::sign(
        FullSyncIssuerCommandBodyV3 {
            schema_version: 3,
            world: policy.body().world.clone(),
            authority_epoch: policy.body().authority_epoch,
            scope_digest: policy.scope_digest().into(),
            policy_digest: policy.policy_digest().into(),
            signer_generation: policy.body().signer_generation,
            target,
            operation,
            expected_head: head.clone(),
            expected_target_generation: generation,
            request_id: hash(identity),
            command_nonce: hash(identity.wrapping_add(1)),
        },
        &key(6),
    )
    .unwrap()
}

fn provider_request(
    policy: &VerifiedFullSyncIssuerPolicyV3,
    head: &FullSyncIssuerAnchorV3,
    target: FullSyncIssuerTargetV2,
    incarnation: u8,
    lease_id: u8,
    request_id: u8,
) -> SignedFullSyncProviderLeaseRequestV3 {
    SignedFullSyncProviderLeaseRequestV3::sign(
        FullSyncProviderLeaseRequestBodyV3 {
            schema_version: 3,
            world: policy.body().world.clone(),
            authority_epoch: policy.body().authority_epoch,
            scope_digest: policy.scope_digest().into(),
            policy_digest: policy.policy_digest().into(),
            signer_generation: policy.body().signer_generation,
            provider_public_key: public(8),
            provider_incarnation: hash(incarnation),
            lease_id: hash(lease_id),
            request_id: hash(request_id),
            request_nonce: hash(request_id.wrapping_add(1)),
            expected_head: head.clone(),
            target,
            target_generation: 1,
        },
        &key(8),
    )
    .unwrap()
}

fn competing_policy(old: &VerifiedFullSyncIssuerPolicyV2) -> VerifiedFullSyncIssuerPolicyV3 {
    let mut body = new_policy(old).signed().body.clone();
    body.management_public_key = public(9);
    let signed = SignedFullSyncIssuerPolicyV3::sign(body, &key(1)).unwrap();
    VerifiedFullSyncIssuerPolicyV3::verify(&signed, &trust(), old).unwrap()
}

fn migrate_fixture(
    label: &str,
) -> (
    TempDir,
    VerifiedFullSyncIssuerPolicyV2,
    VerifiedFullSyncIssuerPolicyV3,
    RacingWitness,
    FullSyncIssuerCommittedEntryV3,
) {
    let directory = TempDir::new(label);
    let old = old_policy();
    let new = new_policy(&old);
    let old_prefix = vec![seeded_v2_entry(&old)];
    let old_state =
        VerifiedFullSyncIssuerStateV2::from_committed_records(old.clone(), old_prefix.clone())
            .unwrap();
    let migration = super::migration(&old, &new, old_state.anchor().clone(), 214);
    let mut witness = RacingWitness::new(old.clone(), new.clone(), old_prefix);
    let entry = DurableFullSyncIssuerV3::migrate_from_v2(
        migration_config(
            directory.path(),
            old.clone(),
            new.clone(),
            key(3),
            key(7),
            v2_genesis(&old),
        ),
        migration,
        &mut witness,
    )
    .unwrap();
    (directory, old, new, witness, entry)
}

fn competing_migrations_fixture(
    label: &str,
) -> (
    TempDir,
    VerifiedFullSyncIssuerPolicyV2,
    VerifiedFullSyncIssuerPolicyV3,
    VerifiedFullSyncIssuerPolicyV3,
    RacingWitness,
    SignedFullSyncIssuerMigrationV3,
    SignedFullSyncIssuerMigrationV3,
    FullSyncIssuerCommittedEntryV3,
) {
    let old = old_policy();
    let new = new_policy(&old);
    let winner_policy = competing_policy(&old);
    let prefix = vec![seeded_v2_entry(&old)];
    let original_state =
        VerifiedFullSyncIssuerStateV2::from_committed_records(old.clone(), prefix.clone()).unwrap();
    let a_directory = TempDir::new(&format!("{label}-a"));
    let migration_a = super::migration(&old, &new, original_state.anchor().clone(), 232);
    let mut witness = RacingWitness::new(old.clone(), new.clone(), prefix);
    witness.v2_winner_before_migration = Some(v2_issue_command(&old, &original_state, 17, 234));

    assert!(
        DurableFullSyncIssuerV3::migrate_from_v2(
            migration_config(
                a_directory.path(),
                old.clone(),
                new.clone(),
                key(3),
                key(7),
                v2_genesis(&old),
            ),
            migration_a.clone(),
            &mut witness,
        )
        .is_err()
    );
    assert!(a_directory.path().join(PENDING_FILE).is_file());
    assert_eq!(witness.inner.old_entries.len(), 2);
    assert!(witness.inner.new_entries.is_empty());

    // B migrates the advanced V2 head under a distinct, independently
    // root-verified policy in the same original witness scope.
    let b_directory = TempDir::new(&format!("{label}-b"));
    witness.inner.new_policy = winner_policy.clone();
    let b_state = VerifiedFullSyncIssuerStateV2::from_committed_records(
        old.clone(),
        witness.inner.old_entries.clone(),
    )
    .unwrap();
    let migration_b = super::migration(&old, &winner_policy, b_state.anchor().clone(), 236);
    assert_ne!(migration_a.body.request_id, migration_b.body.request_id);
    let entry_b = DurableFullSyncIssuerV3::migrate_from_v2(
        migration_config(
            b_directory.path(),
            old.clone(),
            winner_policy.clone(),
            key(3),
            key(7),
            v2_genesis(&old),
        ),
        migration_b,
        &mut witness,
    )
    .unwrap();
    let migration_b = match &entry_b.record.event {
        FullSyncIssuerEventV3::Migration(migration) => migration.as_ref().clone(),
        _ => panic!("the only V3 event should be B's migration"),
    };
    (
        a_directory,
        old,
        new,
        winner_policy,
        witness,
        migration_a,
        migration_b,
        entry_b,
    )
}

fn open_issuer(
    directory: &TempDir,
    old: &VerifiedFullSyncIssuerPolicyV2,
    new: &VerifiedFullSyncIssuerPolicyV3,
    migration_entry: &FullSyncIssuerCommittedEntryV3,
    witness: &mut RacingWitness,
) -> DurableFullSyncIssuerV3 {
    DurableFullSyncIssuerV3::open_v3(
        directory.path(),
        old.clone(),
        new.clone(),
        key(7),
        migration_entry.record.next_anchor.clone(),
        witness,
    )
    .unwrap()
}

fn pending_migration_v2_query(directory: &TempDir) -> SignedFullSyncIssuerQueryV2 {
    let bytes = fs::read(directory.path().join(PENDING_FILE)).unwrap();
    let pending: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    serde_json::from_value(pending["original_v2_query"].clone()).unwrap()
}

fn snapshot_store_files(directory: &Path) -> Vec<(String, Vec<u8>)> {
    let mut files: Vec<_> = fs::read_dir(directory)
        .unwrap()
        .map(|entry| {
            let path = entry.unwrap().path();
            (
                path.file_name().unwrap().to_string_lossy().into_owned(),
                fs::read(path).unwrap(),
            )
        })
        .collect();
    files.sort_by(|left, right| left.0.cmp(&right.0));
    files
}

fn stale_resume_bundle(
    witness: &mut RacingWitness,
    policy: &VerifiedFullSyncIssuerPolicyV3,
    original_v2_query: SignedFullSyncIssuerQueryV2,
    migration: SignedFullSyncIssuerMigrationV3,
    nonce: u8,
) -> FullSyncIssuerMigrationResumeBundleV3 {
    let stale_query = SignedFullSyncIssuerMigrationResumeQueryV3::sign(
        policy,
        original_v2_query,
        migration,
        [nonce; 32],
        &key(7),
    )
    .unwrap();
    witness.inner.resume_migration_v3(&stale_query).unwrap()
}

#[test]
fn v2_commit_and_migration_races_use_one_compare_and_swap_fence() {
    // A V2 command wins the exact head before the already prepared migration
    // reaches CAS. The migration must lose and its original resume must report
    // the complete advanced V2 history without rebasing.
    let directory = TempDir::new("v2-wins-migration-cas");
    let old = old_policy();
    let new = new_policy(&old);
    let prefix = vec![seeded_v2_entry(&old)];
    let state =
        VerifiedFullSyncIssuerStateV2::from_committed_records(old.clone(), prefix.clone()).unwrap();
    let winner = v2_issue_command(&old, &state, 12, 216);
    let migration = super::migration(&old, &new, state.anchor().clone(), 218);
    let mut witness = RacingWitness::new(old.clone(), new.clone(), prefix);
    witness.v2_winner_before_migration = Some(winner);

    assert!(
        DurableFullSyncIssuerV3::migrate_from_v2(
            migration_config(
                directory.path(),
                old.clone(),
                new.clone(),
                key(3),
                key(7),
                v2_genesis(&old),
            ),
            migration.clone(),
            &mut witness,
        )
        .is_err()
    );
    assert_eq!(witness.inner.old_entries.len(), 2);
    assert!(witness.inner.new_entries.is_empty());
    assert_eq!(witness.inner.migration_commits, 0);
    assert!(directory.path().join(PENDING_FILE).is_file());

    let resumed = DurableFullSyncIssuerV3::resume_migration_v3(
        directory.path(),
        old.clone(),
        new,
        key(7),
        migration,
        &mut witness,
    )
    .unwrap();
    assert!(matches!(
        resumed,
        FullSyncIssuerMigrationResumeV3::NotCommittedV2 {
            migration_head_conflict: true,
            ..
        }
    ));
    assert!(!directory.path().join(PENDING_FILE).exists());
    let ReadLedgerFile::V2(local) = read_local_ledger(directory.path()).unwrap().unwrap() else {
        panic!("a V2-winning CAS must remain in V2 mode");
    };
    assert_eq!(local.entries, witness.inner.old_entries);

    // Conversely, once migration wins, a valid V2 candidate signed against
    // the old head cannot commit afterward or alter the V3 history.
    let directory = TempDir::new("migration-wins-v2-cas");
    let old = old_policy();
    let new = new_policy(&old);
    let prefix = vec![seeded_v2_entry(&old)];
    let state =
        VerifiedFullSyncIssuerStateV2::from_committed_records(old.clone(), prefix.clone()).unwrap();
    let stale_query =
        SignedFullSyncIssuerQueryV2::sign(&old, state.anchor().clone(), [219; 32], &key(3))
            .unwrap();
    let stale_observation = SignedFullSyncIssuerObservationV2::sign(
        FullSyncIssuerObservationBodyV2 {
            schema_version: 2,
            query: stale_query,
            current_anchor: state.anchor().clone(),
        },
        &key(4),
    )
    .unwrap();
    let (stale_command, stale_record) = v2_candidate(&old, &state, 13, 220);
    assert_eq!(stale_command.body.expected_head, *state.anchor());
    let migration = super::migration(&old, &new, state.anchor().clone(), 222);
    let mut witness = RacingWitness::new(old.clone(), new.clone(), prefix);
    let migration_entry = DurableFullSyncIssuerV3::migrate_from_v2(
        migration_config(
            directory.path(),
            old.clone(),
            new,
            key(3),
            key(7),
            v2_genesis(&old),
        ),
        migration,
        &mut witness,
    )
    .unwrap();
    let v2_count = witness.inner.old_entries.len();
    let v3_history = witness.inner.new_entries.clone();
    assert!(
        <RacingWitness as FullSyncIssuerWitnessV2>::commit(
            &mut witness,
            &stale_observation,
            &stale_record,
        )
        .is_err()
    );
    assert_eq!(witness.inner.old_entries.len(), v2_count);
    assert_eq!(witness.inner.new_entries, v3_history);
    assert_eq!(witness.inner.new_entries.first(), Some(&migration_entry));
}

#[test]
fn lease_revoke_cas_races_preserve_overlapping_incarnations_and_history() {
    let (directory, old, new, mut witness, migration_entry) =
        migrate_fixture("lease-revoke-cas-races");
    let mut issuer = open_issuer(&directory, &old, &new, &migration_entry, &mut witness);
    let active_target = target(14);
    let initial = issuer.synchronize_v3(&mut witness).unwrap();
    let grant = management_command(
        &new,
        initial.anchor(),
        active_target.clone(),
        0,
        FullSyncIssuerOperationV3::Issue {
            not_before_unix_ms: 1_000,
            expires_unix_ms: 90_000,
        },
        224,
    );
    issuer.manage(grant, &mut witness).unwrap();

    // First lease commits normally. A second incarnation is submitted at the
    // same head as a revoke; the lease wins CAS, so revoke must remain pending
    // and then fail on its stale expected head without erasing either lease.
    let state = issuer.synchronize_v3(&mut witness).unwrap();
    let lease_a = provider_request(&new, state.anchor(), active_target.clone(), 226, 228, 230);
    issuer.issue_lease(lease_a, &mut witness).unwrap();
    let state = issuer.synchronize_v3(&mut witness).unwrap();
    let revoke_a = management_command(
        &new,
        state.anchor(),
        active_target.clone(),
        1,
        FullSyncIssuerOperationV3::Revoke,
        232,
    );
    let lease_b = provider_request(&new, state.anchor(), active_target.clone(), 234, 236, 238);
    witness.v3_winner_before_event = Some(V3RaceWinner::Lease(lease_b));
    assert!(issuer.manage(revoke_a.clone(), &mut witness).is_err());
    assert!(directory.path().join(PENDING_FILE).is_file());
    assert_eq!(witness.inner.event_commits, 3); // grant, lease A, lease B
    drop(issuer);

    let mut issuer = open_issuer(&directory, &old, &new, &migration_entry, &mut witness);
    assert!(issuer.manage(revoke_a, &mut witness).is_err());
    assert!(!directory.path().join(PENDING_FILE).exists());
    let state = issuer.synchronize_v3(&mut witness).unwrap();
    assert!(state.target_state(&active_target).unwrap().active);
    let leases: Vec<_> = state.leases().values().collect();
    assert_eq!(
        leases.len(),
        2,
        "both signed lease decisions remain enumerable"
    );
    let first = leases
        .iter()
        .find(|lease| lease.request.body.provider_incarnation == hash(226))
        .expect("first provider incarnation remains in history");
    let second = leases
        .iter()
        .find(|lease| lease.request.body.provider_incarnation == hash(234))
        .expect("second provider incarnation remains in history");
    assert_ne!(
        first.request.body.provider_incarnation,
        second.request.body.provider_incarnation
    );
    assert!(
        first
            .lease
            .body
            .not_before_unix_ms
            .max(second.lease.body.not_before_unix_ms)
            < first
                .lease
                .body
                .expires_unix_ms
                .min(second.lease.body.expires_unix_ms),
        "the two retained leases have an overlapping validity interval"
    );

    // Reverse the race: a revoke wins before a third lease's CAS. The pending
    // lease is proven absent and must not be rebased over the revocation.
    let state = issuer.synchronize_v3(&mut witness).unwrap();
    let lease_c = provider_request(&new, state.anchor(), active_target.clone(), 240, 242, 244);
    let revoke_b = management_command(
        &new,
        state.anchor(),
        active_target.clone(),
        1,
        FullSyncIssuerOperationV3::Revoke,
        246,
    );
    witness.v3_winner_before_event = Some(V3RaceWinner::Management(revoke_b));
    assert!(issuer.issue_lease(lease_c.clone(), &mut witness).is_err());
    assert!(directory.path().join(PENDING_FILE).is_file());
    assert_eq!(witness.inner.event_commits, 4);
    drop(issuer);

    let mut issuer = open_issuer(&directory, &old, &new, &migration_entry, &mut witness);
    assert!(issuer.issue_lease(lease_c, &mut witness).is_err());
    assert!(!directory.path().join(PENDING_FILE).exists());
    let final_state = issuer.synchronize_v3(&mut witness).unwrap();
    assert!(!final_state.target_state(&active_target).unwrap().active);
    assert_eq!(final_state.leases().len(), 2);
    assert_eq!(witness.inner.event_commits, 4);
}

#[test]
fn migration_resume_rejects_stale_v2_and_v3_mode_bundles() {
    // First prepare a pending migration that loses to a V2 head advance. The
    // cached bundle is correctly signed but bound to a different resume nonce.
    let directory = TempDir::new("replay-v2-resume-response");
    let old = old_policy();
    let new = new_policy(&old);
    let prefix = vec![seeded_v2_entry(&old)];
    let state =
        VerifiedFullSyncIssuerStateV2::from_committed_records(old.clone(), prefix.clone()).unwrap();
    let migration = super::migration(&old, &new, state.anchor().clone(), 248);
    let mut witness = RacingWitness::new(old.clone(), new.clone(), prefix);
    witness.v2_winner_before_migration = Some(v2_issue_command(&old, &state, 15, 250));
    assert!(
        DurableFullSyncIssuerV3::migrate_from_v2(
            migration_config(
                directory.path(),
                old.clone(),
                new.clone(),
                key(3),
                key(7),
                v2_genesis(&old),
            ),
            migration.clone(),
            &mut witness,
        )
        .is_err()
    );
    let original_v2_query = pending_migration_v2_query(&directory);
    let stale = stale_resume_bundle(
        &mut witness,
        &new,
        original_v2_query,
        migration.clone(),
        252,
    );
    let stale_query = stale.observation.body.query.clone();
    witness.replay_resume_bundle = Some(stale);
    let pending_before = fs::read(directory.path().join(PENDING_FILE)).unwrap();
    let ledger_before = fs::read(directory.path().join(LEDGER_FILE)).unwrap();

    let result = DurableFullSyncIssuerV3::resume_migration_v3(
        directory.path(),
        old.clone(),
        new.clone(),
        key(7),
        migration,
        &mut witness,
    );
    assert!(
        result.is_err(),
        "a V2-mode response for an old nonce is replayed"
    );
    assert_ne!(witness.resume_queries.last(), Some(&stale_query));
    assert_eq!(
        fs::read(directory.path().join(PENDING_FILE)).unwrap(),
        pending_before
    );
    assert_eq!(
        fs::read(directory.path().join(LEDGER_FILE)).unwrap(),
        ledger_before
    );

    // Repeat with a V3-mode bundle after migration committed but its response
    // was lost. A fresh recovery nonce must not accept the old committed reply.
    let directory = TempDir::new("replay-v3-resume-response");
    let old = old_policy();
    let new = new_policy(&old);
    let prefix = vec![seeded_v2_entry(&old)];
    let state =
        VerifiedFullSyncIssuerStateV2::from_committed_records(old.clone(), prefix.clone()).unwrap();
    let migration = super::migration(&old, &new, state.anchor().clone(), 254);
    let mut witness = RacingWitness::new(old.clone(), new.clone(), prefix);
    witness.inner.lose_next_migration_reply = true;
    assert!(
        DurableFullSyncIssuerV3::migrate_from_v2(
            migration_config(
                directory.path(),
                old.clone(),
                new.clone(),
                key(3),
                key(7),
                v2_genesis(&old),
            ),
            migration.clone(),
            &mut witness,
        )
        .is_err()
    );
    let original_v2_query = pending_migration_v2_query(&directory);
    let stale = stale_resume_bundle(
        &mut witness,
        &new,
        original_v2_query,
        migration.clone(),
        255,
    );
    let stale_query = stale.observation.body.query.clone();
    witness.replay_resume_bundle = Some(stale);
    let pending_before = fs::read(directory.path().join(PENDING_FILE)).unwrap();
    let ledger_before = fs::read(directory.path().join(LEDGER_FILE)).unwrap();

    let result = DurableFullSyncIssuerV3::resume_migration_v3(
        directory.path(),
        old,
        new,
        key(7),
        migration,
        &mut witness,
    );
    assert!(
        result.is_err(),
        "a V3-mode committed response for an old nonce is replayed"
    );
    assert_ne!(witness.resume_queries.last(), Some(&stale_query));
    assert_eq!(
        fs::read(directory.path().join(PENDING_FILE)).unwrap(),
        pending_before
    );
    assert_eq!(
        fs::read(directory.path().join(LEDGER_FILE)).unwrap(),
        ledger_before
    );
}

#[test]
fn competing_v2_winner_can_migrate_and_retire_a_pending_migration() {
    let (a_directory, old, new, winner_policy, mut witness, migration_a, migration_b, entry_b) =
        competing_migrations_fixture("migration-a-b-race");
    let outcome = DurableFullSyncIssuerV3::resume_migration_v3(
        a_directory.path(),
        old.clone(),
        new.clone(),
        key(7),
        migration_a,
        &mut witness,
    );
    assert!(
        matches!(outcome, Err(FormalError::Conflict(_))),
        "A must terminate as a conflict when B's authenticated migration won: {outcome:?}"
    );
    assert!(
        !a_directory.path().join(PENDING_FILE).exists(),
        "the proven-uncommitted A intent should be durably retired"
    );
    let ReadLedgerFile::V3(local) = read_local_ledger(a_directory.path()).unwrap().unwrap() else {
        panic!("A should repair its valid prefix from the full authenticated V3 history");
    };
    assert_eq!(local.v2_entries, witness.inner.old_entries);
    assert_eq!(local.v3_entries, witness.inner.new_entries);
    assert_eq!(local.v3_entries, vec![entry_b.clone()]);
    assert_eq!(
        entry_b.record.event,
        FullSyncIssuerEventV3::Migration(Box::new(migration_b))
    );
    let reopened = DurableFullSyncIssuerV3::open_v3(
        a_directory.path(),
        old,
        winner_policy,
        key(7),
        entry_b.record.next_anchor.clone(),
        &mut witness,
    )
    .unwrap();
    assert_eq!(reopened.minimum_anchor(), &entry_b.record.next_anchor);
}

#[test]
fn competing_migration_keeps_a_forked_local_v2_prefix_and_pending_intact() {
    let (a_directory, old, new, _winner_policy, mut witness, migration_a, _migration_b, _entry_b) =
        competing_migrations_fixture("migration-a-b-local-fork");
    let pending_before = fs::read(a_directory.path().join(PENDING_FILE)).unwrap();
    let mut local: serde_json::Value =
        serde_json::from_slice(&fs::read(a_directory.path().join(LEDGER_FILE)).unwrap()).unwrap();
    local["entries"][0]["record"]["next_anchor"]["state_digest"] =
        serde_json::Value::String(hash(251));
    let forked_local = serde_json::to_vec(&local).unwrap();
    fs::write(a_directory.path().join(LEDGER_FILE), &forked_local).unwrap();

    let outcome = DurableFullSyncIssuerV3::resume_migration_v3(
        a_directory.path(),
        old,
        new,
        key(7),
        migration_a,
        &mut witness,
    );
    assert!(
        matches!(outcome, Err(FormalError::Rollback(_))),
        "a full remote history must not authorize overwriting a local V2 fork: {outcome:?}"
    );
    assert_eq!(
        fs::read(a_directory.path().join(PENDING_FILE)).unwrap(),
        pending_before
    );
    assert_eq!(
        fs::read(a_directory.path().join(LEDGER_FILE)).unwrap(),
        forked_local
    );
}

#[test]
fn competing_migration_with_non_prefix_history_does_not_clear_pending() {
    let old = old_policy();
    let new = new_policy(&old);
    let prefix = vec![seeded_v2_entry(&old)];

    // A's original migration is bound to one valid V2 branch at revision 2.
    let a_directory = TempDir::new("migration-a-non-prefix");
    let mut witness = RacingWitness::new(old.clone(), new.clone(), prefix.clone());
    let a_prefix_state =
        VerifiedFullSyncIssuerStateV2::from_committed_records(old.clone(), prefix.clone()).unwrap();
    witness
        .commit_v2_command(v2_issue_command(&old, &a_prefix_state, 18, 238), 239)
        .unwrap();
    let a_state = VerifiedFullSyncIssuerStateV2::from_committed_records(
        old.clone(),
        witness.inner.old_entries.clone(),
    )
    .unwrap();
    let migration_a = super::migration(&old, &new, a_state.anchor().clone(), 240);
    witness.fail_next_migration_before_commit = true;
    assert!(
        DurableFullSyncIssuerV3::migrate_from_v2(
            migration_config(
                a_directory.path(),
                old.clone(),
                new.clone(),
                key(3),
                key(7),
                v2_genesis(&old),
            ),
            migration_a.clone(),
            &mut witness,
        )
        .is_err()
    );
    let pending_before = fs::read(a_directory.path().join(PENDING_FILE)).unwrap();
    let ledger_before = fs::read(a_directory.path().join(LEDGER_FILE)).unwrap();

    // B has a different, individually valid signed V2 entry at revision 2 and
    // migrates that fork. The response is authentic but cannot prove A's head.
    let mut branch_b = RacingWitness::new(old.clone(), new.clone(), prefix);
    let branch_b_state = branch_b.inner.v2_state().unwrap();
    branch_b
        .commit_v2_command(v2_issue_command(&old, &branch_b_state, 19, 242), 243)
        .unwrap();
    assert_ne!(branch_b.inner.old_entries[1], witness.inner.old_entries[1]);
    witness.inner.old_entries = branch_b.inner.old_entries;
    let b_state = witness.inner.v2_state().unwrap();
    let migration_b = super::migration(&old, &new, b_state.anchor().clone(), 244);
    let b_directory = TempDir::new("migration-b-non-prefix");
    let entry_b = DurableFullSyncIssuerV3::migrate_from_v2(
        migration_config(
            b_directory.path(),
            old.clone(),
            new.clone(),
            key(3),
            key(7),
            v2_genesis(&old),
        ),
        migration_b,
        &mut witness,
    )
    .unwrap();
    let remote_after_b = witness.inner.new_entries.clone();
    assert_eq!(remote_after_b, vec![entry_b]);

    let outcome = DurableFullSyncIssuerV3::resume_migration_v3(
        a_directory.path(),
        old,
        new,
        key(7),
        migration_a,
        &mut witness,
    );
    assert!(outcome.is_err(), "A's non-prefix V2 head must fail closed");
    assert_eq!(
        fs::read(a_directory.path().join(PENDING_FILE)).unwrap(),
        pending_before
    );
    assert_eq!(
        fs::read(a_directory.path().join(LEDGER_FILE)).unwrap(),
        ledger_before
    );
    assert_eq!(witness.inner.new_entries, remote_after_b);
}

#[test]
fn competing_migration_does_not_clear_a_corrupt_original_pending_record() {
    let (a_directory, old, new, _winner_policy, mut witness, migration_a, _migration_b, _entry_b) =
        competing_migrations_fixture("migration-a-b-corrupt-pending");
    let mut pending: serde_json::Value =
        serde_json::from_slice(&fs::read(a_directory.path().join(PENDING_FILE)).unwrap()).unwrap();
    pending["record"]["next_anchor"]["state_digest"] = serde_json::Value::String(hash(253));
    let corrupt_pending = serde_json::to_vec(&pending).unwrap();
    fs::write(a_directory.path().join(PENDING_FILE), &corrupt_pending).unwrap();
    let ledger_before = fs::read(a_directory.path().join(LEDGER_FILE)).unwrap();

    let outcome = DurableFullSyncIssuerV3::resume_migration_v3(
        a_directory.path(),
        old,
        new,
        key(7),
        migration_a,
        &mut witness,
    );
    assert!(
        outcome.is_err(),
        "corrupt pending evidence must stop recovery"
    );
    assert_eq!(
        fs::read(a_directory.path().join(PENDING_FILE)).unwrap(),
        corrupt_pending
    );
    assert_eq!(
        fs::read(a_directory.path().join(LEDGER_FILE)).unwrap(),
        ledger_before
    );
}

#[test]
fn legacy_v2_adapter_refuses_schema_three_without_mutating_the_store() {
    let (directory, old, _new, mut witness, _migration_entry) =
        migrate_fixture("v2-open-refuses-v3-ledger");
    let before = snapshot_store_files(directory.path());
    let result = DurableFullSyncIssuerV2::open(
        directory.path(),
        old.clone(),
        key(3),
        v2_genesis(&old),
        &mut witness,
    );
    assert!(
        matches!(result, Err(FormalError::Invalid(_))),
        "V2 must fail on the schema-3 material itself, not an incidental lock error"
    );
    assert_eq!(snapshot_store_files(directory.path()), before);
    assert_eq!(witness.inner.new_entries.len(), 1);
}
