use super::*;
use ed25519_dalek::SigningKey;
use oasis7_formal_protocol::controlled_authority::formal::{
    FormalWorldIdentityV1, InitialActivationTrustV1,
};
use std::{
    fs,
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
    time::{SystemTime, UNIX_EPOCH},
};

fn key(value: u8) -> SigningKey {
    SigningKey::from_bytes(&[value; 32])
}

fn public(value: u8) -> String {
    hex::encode(key(value).verifying_key().as_bytes())
}

fn hash(value: u8) -> String {
    hex::encode([value; 32])
}

fn trust() -> InitialActivationTrustV1 {
    InitialActivationTrustV1 {
        issuer_public_key: public(1),
        world: FormalWorldIdentityV1 {
            world_id: "durable-full-sync-provider-lease-test".into(),
            chain_id: "provider-lease-test-chain".into(),
            genesis_digest: hash(31),
        },
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
            activation_trust_digest: full_sync_activation_trust_digest_v2(&trust).unwrap(),
            world: trust.world.clone(),
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
            world: trust.world.clone(),
            authority_epoch: 7,
            scope_digest: old.scope_digest().into(),
            allowed_roles: vec![FullSyncReadRoleV2::StateSync],
            provider_public_keys: vec![public(8)],
            management_public_key: public(6),
            lease_signer_public_key: public(7),
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

fn v2_genesis(policy: &VerifiedFullSyncIssuerPolicyV2) -> FullSyncIssuerAnchorV2 {
    VerifiedFullSyncIssuerStateV2::from_committed_records(policy.clone(), vec![])
        .unwrap()
        .anchor()
        .clone()
}

fn migration_config(
    directory: &Path,
    v2_policy: VerifiedFullSyncIssuerPolicyV2,
    policy: VerifiedFullSyncIssuerPolicyV3,
    v2_issuer_key: SigningKey,
    lease_signer: SigningKey,
    minimum_v2_anchor: FullSyncIssuerAnchorV2,
) -> FullSyncIssuerV2MigrationConfig {
    FullSyncIssuerV2MigrationConfig {
        directory: directory.into(),
        v2_policy,
        policy,
        v2_issuer_key,
        lease_signer,
        minimum_v2_anchor,
    }
}

fn target(node_key: u8) -> FullSyncIssuerTargetV2 {
    FullSyncIssuerTargetV2 {
        node_public_key: public(node_key),
        role: FullSyncReadRoleV2::StateSync,
    }
}

fn seeded_v2_entry(policy: &VerifiedFullSyncIssuerPolicyV2) -> FullSyncIssuerCommittedEntryV2 {
    let state =
        VerifiedFullSyncIssuerStateV2::from_committed_records(policy.clone(), vec![]).unwrap();
    let command = SignedFullSyncIssuerCommandV2::sign(
        FullSyncIssuerCommandBodyV2 {
            schema_version: 2,
            world: policy.body().world.clone(),
            authority_epoch: policy.body().authority_epoch,
            scope_digest: policy.scope_digest().into(),
            policy_digest: policy.policy_digest().into(),
            signer_generation: policy.body().signer_generation,
            transition: None,
            target: target(9),
            operation: FullSyncIssuerOperationV2::Issue {
                not_before_unix_ms: 1_000,
                expires_unix_ms: 90_000,
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
    let query =
        SignedFullSyncIssuerQueryV2::sign(policy, state.anchor().clone(), [42; 32], &key(3))
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
    let receipt = SignedFullSyncIssuerReceiptV2::sign(observation, &record, &key(4)).unwrap();
    prepared.commit(receipt).unwrap().entry().clone()
}

fn v2_issue_command(
    policy: &VerifiedFullSyncIssuerPolicyV2,
    state: &VerifiedFullSyncIssuerStateV2,
    target_key: u8,
    request_id: u8,
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
            target: target(target_key),
            operation: FullSyncIssuerOperationV2::Issue {
                not_before_unix_ms: 1_000,
                expires_unix_ms: 90_000,
            },
            expected_head: state.anchor().clone(),
            expected_target_generation: 0,
            request_id: hash(request_id),
            command_nonce: hash(request_id.wrapping_add(1)),
        },
        &key(2),
    )
    .unwrap()
}

fn committed_v2_command(
    policy: &VerifiedFullSyncIssuerPolicyV2,
    state: &VerifiedFullSyncIssuerStateV2,
    command: SignedFullSyncIssuerCommandV2,
    nonce: u8,
) -> FullSyncIssuerCommittedEntryV2 {
    let prepared = state.prepare(command).unwrap();
    let record = prepared.record().unwrap().clone();
    let query =
        SignedFullSyncIssuerQueryV2::sign(policy, state.anchor().clone(), [nonce; 32], &key(3))
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
    let receipt = SignedFullSyncIssuerReceiptV2::sign(observation, &record, &key(4)).unwrap();
    prepared.commit(receipt).unwrap().entry().clone()
}

fn v2_candidate(
    policy: &VerifiedFullSyncIssuerPolicyV2,
    state: &VerifiedFullSyncIssuerStateV2,
    target_key: u8,
    request_id: u8,
) -> (SignedFullSyncIssuerCommandV2, FullSyncIssuerRecordV2) {
    let command = v2_issue_command(policy, state, target_key, request_id);
    let record = state
        .prepare(command.clone())
        .unwrap()
        .record()
        .unwrap()
        .clone();
    (command, record)
}

fn write_v2_pending_fixture(
    directory: &Path,
    entries: Vec<FullSyncIssuerCommittedEntryV2>,
    command: SignedFullSyncIssuerCommandV2,
    record: FullSyncIssuerRecordV2,
) {
    let ledger = super::super::LedgerFile {
        schema_version: 2,
        entries,
    };
    let ledger_bytes = serde_json::to_vec(&ledger).unwrap();
    atomic_bounded(
        directory,
        &directory.join(LEDGER_FILE),
        &ledger_bytes,
        MAX_LEDGER_BYTES,
    )
    .unwrap();
    let pending = super::super::PendingIntent {
        schema_version: 2,
        command,
        record,
    };
    let pending_bytes = serde_json::to_vec(&pending).unwrap();
    atomic_bounded(
        directory,
        &directory.join(PENDING_FILE),
        &pending_bytes,
        MAX_LEDGER_BYTES,
    )
    .unwrap();
}

fn migration(
    old: &VerifiedFullSyncIssuerPolicyV2,
    new: &VerifiedFullSyncIssuerPolicyV3,
    expected_v2_head: FullSyncIssuerAnchorV2,
    request_id: u8,
) -> SignedFullSyncIssuerMigrationV3 {
    SignedFullSyncIssuerMigrationV3::sign(
        FullSyncIssuerMigrationBodyV3 {
            schema_version: 3,
            previous_v2_policy_digest: old.policy_digest().into(),
            previous_v2_signer_generation: old.body().signer_generation,
            expected_v2_head,
            new_policy: new.signed().clone(),
            request_id: hash(request_id),
            manager_nonce: hash(request_id.wrapping_add(1)),
        },
        &key(2),
    )
    .unwrap()
}

struct TempDir(PathBuf);

impl TempDir {
    fn new(label: &str) -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let root = fs::canonicalize(std::env::temp_dir()).unwrap();
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = root.join(format!(
            "o7-full-sync-provider-v3-{label}-{}-{now}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&path, fs::Permissions::from_mode(0o700)).unwrap();
        }
        Self(path)
    }

    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

/// This test witness is deliberately in-process and simulated. It exercises
/// adapter/protocol sequencing; it is not evidence of an independent durable
/// witness fault domain.
struct SimulatedWitness {
    old_policy: VerifiedFullSyncIssuerPolicyV2,
    new_policy: VerifiedFullSyncIssuerPolicyV3,
    old_entries: Vec<FullSyncIssuerCommittedEntryV2>,
    new_entries: Vec<FullSyncIssuerCommittedEntryV3>,
    lose_next_migration_reply: bool,
    lose_next_event_reply: bool,
    fail_next_event_before_commit: bool,
    migration_commits: usize,
    event_commits: usize,
    v2_query_minimums: Vec<FullSyncIssuerAnchorV2>,
    v3_observations: Vec<SignedFullSyncIssuerObservationV3>,
    event_attempts: Vec<(SignedFullSyncIssuerObservationV3, FullSyncIssuerRecordV3)>,
    next_protected_time_ms: u64,
}

impl SimulatedWitness {
    fn new(
        old_policy: VerifiedFullSyncIssuerPolicyV2,
        new_policy: VerifiedFullSyncIssuerPolicyV3,
        old_entries: Vec<FullSyncIssuerCommittedEntryV2>,
    ) -> Self {
        Self {
            old_policy,
            new_policy,
            old_entries,
            new_entries: Vec::new(),
            lose_next_migration_reply: false,
            lose_next_event_reply: false,
            fail_next_event_before_commit: false,
            migration_commits: 0,
            event_commits: 0,
            v2_query_minimums: Vec::new(),
            v3_observations: Vec::new(),
            event_attempts: Vec::new(),
            next_protected_time_ms: 1_000,
        }
    }

    fn v2_state(&self) -> Result<VerifiedFullSyncIssuerStateV2, FormalError> {
        VerifiedFullSyncIssuerStateV2::from_committed_records(
            self.old_policy.clone(),
            self.old_entries.clone(),
        )
    }

    fn v3_anchor(&self) -> Result<FullSyncIssuerAnchorV3, FormalError> {
        self.new_entries
            .last()
            .map(|entry| entry.record.next_anchor.clone())
            .ok_or_else(|| FormalError::Invalid("simulated witness is still in V2 mode".into()))
    }

    fn history(&self) -> FullSyncIssuerHistoryV3 {
        FullSyncIssuerHistoryV3 {
            v2_entries: self.old_entries.clone(),
            v3_entries: self.new_entries.clone(),
        }
    }

    fn state_v3_at(
        &self,
        observation: &SignedFullSyncIssuerObservationV3,
    ) -> Result<VerifiedFullSyncIssuerStateV3, FormalError> {
        VerifiedFullSyncIssuerStateV3::from_witness_observation(
            self.old_policy.clone(),
            self.new_policy.clone(),
            self.history(),
            observation,
        )
    }
}

impl FullSyncIssuerWitnessV2 for SimulatedWitness {
    fn observe(
        &mut self,
        query: &SignedFullSyncIssuerQueryV2,
    ) -> Result<SignedFullSyncIssuerObservationV2, FormalError> {
        if !self.new_entries.is_empty() {
            return Err(FormalError::Rollback(
                "simulated V2 API is fenced after migration".into(),
            ));
        }
        query.verify(&self.old_policy)?;
        self.v2_query_minimums
            .push(query.body.minimum_anchor.clone());
        SignedFullSyncIssuerObservationV2::sign(
            FullSyncIssuerObservationBodyV2 {
                schema_version: 2,
                query: query.clone(),
                current_anchor: self.v2_state()?.anchor().clone(),
            },
            &key(4),
        )
    }

    fn records_after(
        &mut self,
        after: &FullSyncIssuerAnchorV2,
        through: &FullSyncIssuerAnchorV2,
    ) -> Result<Vec<FullSyncIssuerCommittedEntryV2>, FormalError> {
        if !self.new_entries.is_empty() {
            return Err(FormalError::Rollback(
                "simulated V2 history API is fenced after migration".into(),
            ));
        }
        if after != &v2_genesis(&self.old_policy) || through != self.v2_state()?.anchor() {
            return Err(FormalError::Rollback(
                "simulated V2 history cursor mismatch".into(),
            ));
        }
        Ok(self.old_entries.clone())
    }

    fn commit(
        &mut self,
        _observation: &SignedFullSyncIssuerObservationV2,
        _record: &FullSyncIssuerRecordV2,
    ) -> Result<SignedFullSyncIssuerReceiptV2, FormalError> {
        Err(FormalError::Rollback(
            "V2 commits are not used in this adapter fixture".into(),
        ))
    }
}

impl FullSyncIssuerWitnessV3 for SimulatedWitness {
    fn commit_migration_v3(
        &mut self,
        observation: &SignedFullSyncIssuerObservationV2,
        record: &FullSyncIssuerRecordV3,
    ) -> Result<SignedFullSyncIssuerReceiptV3, FormalError> {
        observation.verify(&self.old_policy)?;
        if let Some(entry) = self.new_entries.first()
            && entry.record == *record
            && entry.receipt.body.observation
                == FullSyncIssuerReceiptObservationV3::Migration(observation.clone())
        {
            return Ok(entry.receipt.clone());
        }
        if !self.new_entries.is_empty() {
            return Err(FormalError::Conflict(
                "simulated migration CAS already chose a different entry".into(),
            ));
        }
        let migration = match &record.event {
            FullSyncIssuerEventV3::Migration(migration) => migration.as_ref().clone(),
            _ => {
                return Err(FormalError::Invalid(
                    "simulated migration CAS received wrong event".into(),
                ));
            }
        };
        let prepared = self.v2_state()?.prepare_migration_v3(
            self.new_policy.clone(),
            migration,
            observation,
        )?;
        if prepared.record() != Some(record) {
            return Err(FormalError::Conflict(
                "simulated migration candidate mismatch".into(),
            ));
        }
        let receipt = SignedFullSyncIssuerReceiptV3::sign(
            FullSyncIssuerReceiptObservationV3::Migration(observation.clone()),
            record,
            &key(4),
        )?;
        let entry = prepared.commit(receipt.clone())?.entry().clone();
        self.new_entries.push(entry);
        self.migration_commits += 1;
        if self.lose_next_migration_reply {
            self.lose_next_migration_reply = false;
            return Err(FormalError::Io(
                "simulated migration committed but reply was lost".into(),
            ));
        }
        Ok(receipt)
    }

    fn observe_v3(
        &mut self,
        query: &SignedFullSyncIssuerQueryV3,
    ) -> Result<SignedFullSyncIssuerObservationV3, FormalError> {
        if self.new_entries.is_empty() {
            return Err(FormalError::Rollback(
                "simulated V3 API unavailable before migration".into(),
            ));
        }
        query.verify(&self.new_policy)?;
        self.next_protected_time_ms = self.next_protected_time_ms.saturating_add(1_000);
        let protected_time = self.next_protected_time_ms;
        let observation = SignedFullSyncIssuerObservationV3::sign(
            FullSyncIssuerObservationBodyV3 {
                schema_version: 3,
                query: query.clone(),
                current_anchor: self.v3_anchor()?,
                protected_time: FullSyncIssuerProtectedTimeIntervalV3 {
                    earliest_unix_ms: protected_time,
                    latest_unix_ms: protected_time,
                    clock_profile_digest: self.new_policy.body().clock_profile_digest.clone(),
                },
            },
            &key(4),
        )?;
        self.v3_observations.push(observation.clone());
        Ok(observation)
    }

    fn history_v3(
        &mut self,
        observation: &SignedFullSyncIssuerObservationV3,
    ) -> Result<FullSyncIssuerHistoryV3, FormalError> {
        observation.verify(&self.new_policy)?;
        if observation.body.current_anchor != self.v3_anchor()? {
            return Err(FormalError::Rollback(
                "simulated V3 history does not close observation".into(),
            ));
        }
        Ok(self.history())
    }

    fn commit_v3(
        &mut self,
        observation: &SignedFullSyncIssuerObservationV3,
        record: &FullSyncIssuerRecordV3,
    ) -> Result<SignedFullSyncIssuerReceiptV3, FormalError> {
        observation.verify(&self.new_policy)?;
        if let Some(existing) = self.new_entries.iter().find(|entry| {
            entry.record == *record
                && entry.receipt.body.observation
                    == FullSyncIssuerReceiptObservationV3::V3(observation.clone())
        }) {
            return Ok(existing.receipt.clone());
        }
        if observation.body.current_anchor != self.v3_anchor()? {
            return Err(FormalError::Conflict("simulated V3 stale CAS".into()));
        }
        let state = self.state_v3_at(observation)?;
        let prepared = match &record.event {
            FullSyncIssuerEventV3::Management(command) => {
                state.prepare_management(command.as_ref().clone(), observation)?
            }
            FullSyncIssuerEventV3::Lease { request, .. } => {
                state.prepare_lease(request.as_ref().clone(), observation, &key(7))?
            }
            FullSyncIssuerEventV3::Migration(_) => {
                return Err(FormalError::Invalid(
                    "migration must use the V2-to-V3 CAS".into(),
                ));
            }
        };
        if prepared.record() != Some(record) {
            return Err(FormalError::Conflict(
                "simulated V3 event candidate mismatch".into(),
            ));
        }
        self.event_attempts
            .push((observation.clone(), record.clone()));
        if self.fail_next_event_before_commit {
            self.fail_next_event_before_commit = false;
            return Err(FormalError::Io(
                "simulated V3 event failed before witness commit".into(),
            ));
        }
        let receipt = SignedFullSyncIssuerReceiptV3::sign(
            FullSyncIssuerReceiptObservationV3::V3(observation.clone()),
            record,
            &key(4),
        )?;
        self.new_entries
            .push(prepared.commit(receipt.clone())?.entry().clone());
        self.event_commits += 1;
        if self.lose_next_event_reply {
            self.lose_next_event_reply = false;
            return Err(FormalError::Io(
                "simulated V3 event committed but reply was lost".into(),
            ));
        }
        Ok(receipt)
    }

    fn resume_migration_v3(
        &mut self,
        query: &SignedFullSyncIssuerMigrationResumeQueryV3,
    ) -> Result<FullSyncIssuerMigrationResumeBundleV3, FormalError> {
        query.verify(&self.old_policy, &self.new_policy)?;
        let (current_mode_head, original_migration_receipt) = if self.new_entries.is_empty() {
            (
                FullSyncIssuerWitnessModeHeadV3::V2(self.v2_state()?.anchor().clone()),
                None,
            )
        } else {
            let original_migration_receipt = self.new_entries.iter().find_map(|entry| {
                matches!(
                    &entry.record.event,
                    FullSyncIssuerEventV3::Migration(committed)
                        if committed.as_ref() == &query.body.exact_migration
                )
                .then(|| entry.receipt.clone())
            });
            (
                FullSyncIssuerWitnessModeHeadV3::V3(self.v3_anchor()?),
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
            history: self.history(),
            original_migration_receipt,
        })
    }
}

#[test]
fn adapter_migration_lost_reply_and_lease_restart_recover_exact_history() {
    let temp = TempDir::new("migration-lease-reopen");
    let old = old_policy();
    let new = new_policy(&old);
    let old_entry = seeded_v2_entry(&old);
    let v2_prefix = vec![old_entry.clone()];
    let v2_state =
        VerifiedFullSyncIssuerStateV2::from_committed_records(old.clone(), v2_prefix.clone())
            .unwrap();
    let expected_head = v2_state.anchor().clone();
    let minimum_v2 = v2_genesis(&old);
    let migration = migration(&old, &new, expected_head.clone(), 50);
    let mut witness = SimulatedWitness::new(old.clone(), new.clone(), v2_prefix.clone());
    witness.lose_next_migration_reply = true;

    let first = DurableFullSyncIssuerV3::migrate_from_v2(
        migration_config(
            temp.path(),
            old.clone(),
            new.clone(),
            key(3),
            key(7),
            minimum_v2,
        ),
        migration.clone(),
        &mut witness,
    );
    assert!(first.is_err(), "the fixture must lose the post-CAS reply");
    assert!(temp.path().join(PENDING_FILE).is_file());
    assert_eq!(witness.migration_commits, 1);
    assert_eq!(witness.v2_query_minimums.last(), Some(&expected_head));

    let expected_migration = witness.new_entries[0].clone();
    let recovered = DurableFullSyncIssuerV3::resume_migration_v3(
        temp.path(),
        old.clone(),
        new.clone(),
        key(7),
        migration.clone(),
        &mut witness,
    )
    .unwrap();
    assert_eq!(
        recovered,
        FullSyncIssuerMigrationResumeV3::Committed(Box::new(expected_migration.clone()))
    );
    assert!(!temp.path().join(PENDING_FILE).exists());
    let local = read_local_ledger(temp.path()).unwrap().unwrap();
    let ReadLedgerFile::V3(local) = local else {
        panic!("migration atomically upgrades the original ledger path");
    };
    assert_eq!(local.v2_entries, v2_prefix);
    assert_eq!(local.v3_entries, witness.new_entries);
    let valid_v3_bytes = fs::read(temp.path().join(LEDGER_FILE)).unwrap();
    let mut missing_migration = local.clone();
    missing_migration.v3_entries.clear();
    let malformed_bytes = serde_json::to_vec(&missing_migration).unwrap();
    atomic_bounded(
        temp.path(),
        &temp.path().join(LEDGER_FILE),
        &malformed_bytes,
        MAX_LEDGER_BYTES,
    )
    .unwrap();
    assert!(
        DurableFullSyncIssuerV3::open_v3(
            temp.path(),
            old.clone(),
            new.clone(),
            key(7),
            expected_migration.record.next_anchor.clone(),
            &mut witness,
        )
        .is_err()
    );
    assert_eq!(
        fs::read(temp.path().join(LEDGER_FILE)).unwrap(),
        malformed_bytes
    );
    atomic_bounded(
        temp.path(),
        &temp.path().join(LEDGER_FILE),
        &valid_v3_bytes,
        MAX_LEDGER_BYTES,
    )
    .unwrap();

    let legacy_query =
        SignedFullSyncIssuerQueryV2::sign(&old, expected_head, [51; 32], &key(3)).unwrap();
    assert!(witness.observe(&legacy_query).is_err(), "V2 mode is fenced");

    let pinned_v3_minimum = expected_migration.record.next_anchor.clone();
    let mut non_prefix_minimum = pinned_v3_minimum.clone();
    non_prefix_minimum.state_digest = hash(82);
    assert_eq!(
        non_prefix_minimum.scope_digest,
        pinned_v3_minimum.scope_digest
    );
    assert_eq!(
        non_prefix_minimum.policy_digest,
        pinned_v3_minimum.policy_digest
    );
    let ledger_before_bad_minimum = fs::read(temp.path().join(LEDGER_FILE)).unwrap();
    assert!(
        DurableFullSyncIssuerV3::open_v3(
            temp.path(),
            old.clone(),
            new.clone(),
            key(7),
            non_prefix_minimum,
            &mut witness,
        )
        .is_err()
    );
    assert_eq!(
        fs::read(temp.path().join(LEDGER_FILE)).unwrap(),
        ledger_before_bad_minimum,
        "a same-scope, structurally valid but non-prefix minimum must not repair local history"
    );

    let mut issuer = DurableFullSyncIssuerV3::open_v3(
        temp.path(),
        old.clone(),
        new.clone(),
        key(7),
        pinned_v3_minimum.clone(),
        &mut witness,
    )
    .unwrap();
    let migrated = issuer.synchronize_v3(&mut witness).unwrap();
    let grant_target = target(10);
    let grant = SignedFullSyncIssuerCommandV3::sign(
        FullSyncIssuerCommandBodyV3 {
            schema_version: 3,
            world: new.body().world.clone(),
            authority_epoch: new.body().authority_epoch,
            scope_digest: new.scope_digest().into(),
            policy_digest: new.policy_digest().into(),
            signer_generation: new.body().signer_generation,
            target: grant_target.clone(),
            operation: FullSyncIssuerOperationV3::Issue {
                not_before_unix_ms: 1_000,
                expires_unix_ms: 90_000,
            },
            expected_head: migrated.anchor().clone(),
            expected_target_generation: 0,
            request_id: hash(60),
            command_nonce: hash(61),
        },
        &key(6),
    )
    .unwrap();
    issuer.manage(grant, &mut witness).unwrap();
    let state = issuer.synchronize_v3(&mut witness).unwrap();
    let second_target = target(11);
    let second_grant = SignedFullSyncIssuerCommandV3::sign(
        FullSyncIssuerCommandBodyV3 {
            schema_version: 3,
            world: new.body().world.clone(),
            authority_epoch: new.body().authority_epoch,
            scope_digest: new.scope_digest().into(),
            policy_digest: new.policy_digest().into(),
            signer_generation: new.body().signer_generation,
            target: second_target.clone(),
            operation: FullSyncIssuerOperationV3::Issue {
                not_before_unix_ms: 1_000,
                expires_unix_ms: 90_000,
            },
            expected_head: state.anchor().clone(),
            expected_target_generation: 0,
            request_id: hash(62),
            command_nonce: hash(63),
        },
        &key(6),
    )
    .unwrap();
    issuer.manage(second_grant, &mut witness).unwrap();
    let state = issuer.synchronize_v3(&mut witness).unwrap();
    let request = SignedFullSyncProviderLeaseRequestV3::sign(
        FullSyncProviderLeaseRequestBodyV3 {
            schema_version: 3,
            world: new.body().world.clone(),
            authority_epoch: new.body().authority_epoch,
            scope_digest: new.scope_digest().into(),
            policy_digest: new.policy_digest().into(),
            signer_generation: new.body().signer_generation,
            provider_public_key: public(8),
            provider_incarnation: hash(70),
            lease_id: hash(71),
            request_id: hash(72),
            request_nonce: hash(73),
            expected_head: state.anchor().clone(),
            target: grant_target,
            target_generation: 1,
        },
        &key(8),
    )
    .unwrap();
    let commits_before_capacity_rejection = witness.event_commits;
    let attempts_before_capacity_rejection = witness.event_attempts.len();
    issuer.ledger_byte_limit = 1;
    assert!(issuer.issue_lease(request.clone(), &mut witness).is_err());
    assert_eq!(witness.event_commits, commits_before_capacity_rejection);
    assert!(
        !temp.path().join(PENDING_FILE).exists(),
        "capacity failure must precede the durable intent and witness CAS"
    );
    issuer.ledger_byte_limit = MAX_LEDGER_BYTES;
    witness.fail_next_event_before_commit = true;
    assert!(issuer.issue_lease(request.clone(), &mut witness).is_err());
    assert!(temp.path().join(PENDING_FILE).exists());
    assert_eq!(witness.event_commits, commits_before_capacity_rejection);
    assert_eq!(
        witness.event_attempts.len(),
        attempts_before_capacity_rejection + 1
    );
    let original_attempt = witness.event_attempts[attempts_before_capacity_rejection].clone();
    let original_observed_time = original_attempt.0.body.protected_time.latest_unix_ms;
    drop(issuer);

    let mut reopened = DurableFullSyncIssuerV3::open_v3(
        temp.path(),
        old.clone(),
        new.clone(),
        key(7),
        pinned_v3_minimum.clone(),
        &mut witness,
    )
    .unwrap();
    assert!(
        temp.path().join(PENDING_FILE).exists(),
        "same-head non-commit must retain the exact pending intent for retry"
    );
    let lease_entry = reopened.issue_lease(request.clone(), &mut witness).unwrap();
    assert_eq!(lease_entry.record, original_attempt.1);
    assert_eq!(
        lease_entry.receipt.body.observation,
        FullSyncIssuerReceiptObservationV3::V3(original_attempt.0.clone())
    );
    assert_eq!(
        witness.event_attempts.len(),
        attempts_before_capacity_rejection + 2
    );
    assert_eq!(
        witness.event_attempts[attempts_before_capacity_rejection + 1],
        original_attempt
    );
    assert!(
        witness
            .v3_observations
            .last()
            .unwrap()
            .body
            .protected_time
            .latest_unix_ms
            > original_observed_time,
        "fresh observations after reopen must advance time, so exact retry proves pending reuse"
    );
    assert_eq!(witness.event_commits, commits_before_capacity_rejection + 1);

    let second_state = reopened.synchronize_v3(&mut witness).unwrap();
    let second_request = SignedFullSyncProviderLeaseRequestV3::sign(
        FullSyncProviderLeaseRequestBodyV3 {
            schema_version: 3,
            world: new.body().world.clone(),
            authority_epoch: new.body().authority_epoch,
            scope_digest: new.scope_digest().into(),
            policy_digest: new.policy_digest().into(),
            signer_generation: new.body().signer_generation,
            provider_public_key: public(8),
            provider_incarnation: hash(74),
            lease_id: hash(75),
            request_id: hash(76),
            request_nonce: hash(77),
            expected_head: second_state.anchor().clone(),
            target: second_target,
            target_generation: 1,
        },
        &key(8),
    )
    .unwrap();
    witness.lose_next_event_reply = true;
    assert!(
        reopened
            .issue_lease(second_request.clone(), &mut witness)
            .is_err()
    );
    assert!(temp.path().join(PENDING_FILE).exists());
    let lost_reply_entry = witness.new_entries.last().unwrap().clone();
    let commit_count = witness.event_commits;
    drop(reopened);

    let mut recovered = DurableFullSyncIssuerV3::open_v3(
        temp.path(),
        old,
        new,
        key(7),
        pinned_v3_minimum,
        &mut witness,
    )
    .unwrap();
    assert!(!temp.path().join(PENDING_FILE).exists());
    let exact_retry = recovered.issue_lease(second_request, &mut witness).unwrap();
    assert_eq!(exact_retry, lost_reply_entry);
    assert_eq!(witness.event_commits, commit_count);
    assert_eq!(
        recovered
            .synchronize_v3(&mut witness)
            .unwrap()
            .leases()
            .len(),
        2
    );
}

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

#[cfg(test)]
#[path = "full_sync_issuer_v3_cas_tests.rs"]
mod cas_tests;
