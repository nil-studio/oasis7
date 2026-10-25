//! Durable V3 provider-lease history over the same V2 witness scope and files.
//!
//! Migration and normal V3 operation are separate entry points. Migration
//! returns only the exact committed event for external pinning; `open_v3`
//! requires that externally pinned V3 floor and is the only serving adapter.
use super::*;
use ed25519_dalek::SigningKey;
use serde::{Deserialize, Serialize};
use std::{
    fs::File,
    path::{Path, PathBuf},
};

const LEDGER_SCHEMA_V3: u32 = 3;
const PENDING_SCHEMA_V3: u32 = 3;
/// The V3 file deliberately reuses the original V2 ledger path. The V2 prefix
/// is retained byte-for-byte as signed entries; V3 begins with migration.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct LedgerFileV3 {
    schema_version: u32,
    v2_entries: Vec<FullSyncIssuerCommittedEntryV2>,
    v3_entries: Vec<FullSyncIssuerCommittedEntryV3>,
}

#[derive(Deserialize)]
#[serde(untagged)]
enum ReadLedgerFile {
    V3(LedgerFileV3),
    V2(super::LedgerFile),
}

#[derive(Deserialize)]
#[serde(untagged)]
enum ReadPendingFile {
    V3(Box<PendingIntentV3>),
    V2(Box<super::PendingIntent>),
}

/// Pending contains the exact candidate and signed observation. Retrying an
/// uncertain CAS must never refresh protected time or re-sign a lease.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
enum PendingIntentV3 {
    Migration {
        schema_version: u32,
        original_v2_query: Box<SignedFullSyncIssuerQueryV2>,
        original_v2_observation: Box<SignedFullSyncIssuerObservationV2>,
        migration: Box<SignedFullSyncIssuerMigrationV3>,
        record: Box<FullSyncIssuerRecordV3>,
    },
    Management {
        schema_version: u32,
        observation: Box<SignedFullSyncIssuerObservationV3>,
        command: Box<SignedFullSyncIssuerCommandV3>,
        record: Box<FullSyncIssuerRecordV3>,
    },
    Lease {
        schema_version: u32,
        observation: Box<SignedFullSyncIssuerObservationV3>,
        request: Box<SignedFullSyncProviderLeaseRequestV3>,
        record: Box<FullSyncIssuerRecordV3>,
    },
}

/// Result of the non-serving migration-recovery entry point.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FullSyncIssuerMigrationResumeV3 {
    Committed(Box<FullSyncIssuerCommittedEntryV3>),
    NotCommittedV2 {
        current_anchor: FullSyncIssuerAnchorV2,
        migration_head_conflict: bool,
    },
}

/// Inputs needed to explicitly migrate the existing V2 issuer store. The
/// witness operation still uses the same scope, lock, ledger and CAS as V2.
pub struct FullSyncIssuerV2MigrationConfig {
    pub directory: PathBuf,
    pub v2_policy: VerifiedFullSyncIssuerPolicyV2,
    pub policy: VerifiedFullSyncIssuerPolicyV3,
    pub v2_issuer_key: SigningKey,
    pub lease_signer: SigningKey,
    pub minimum_v2_anchor: FullSyncIssuerAnchorV2,
}

/// Local durable coordinator after a V3 floor has been externally pinned.
pub struct DurableFullSyncIssuerV3 {
    directory: PathBuf,
    v2_policy: VerifiedFullSyncIssuerPolicyV2,
    policy: VerifiedFullSyncIssuerPolicyV3,
    lease_signer: SigningKey,
    minimum_anchor: FullSyncIssuerAnchorV3,
    _lock: File,
    poisoned: bool,
    #[cfg(test)]
    ledger_byte_limit: usize,
    #[cfg(test)]
    fail_next_local_sync: std::cell::Cell<bool>,
}

impl DurableFullSyncIssuerV3 {
    /// Opens an already migrated store. This path never initializes migration
    /// and refuses a V2 minimum; callers must pin a V3 anchor first.
    pub fn open_v3(
        directory: &Path,
        v2_policy: VerifiedFullSyncIssuerPolicyV2,
        policy: VerifiedFullSyncIssuerPolicyV3,
        lease_signer: SigningKey,
        minimum_anchor: FullSyncIssuerAnchorV3,
        witness: &mut dyn FullSyncIssuerWitnessV3,
    ) -> Result<Self, FormalError> {
        validate_v3_open_inputs(&v2_policy, &policy, &lease_signer, &minimum_anchor)?;
        let lock = acquire_store_lock(directory)?;
        let mut store = Self {
            directory: directory.into(),
            v2_policy,
            policy,
            lease_signer,
            minimum_anchor,
            _lock: lock,
            poisoned: false,
            #[cfg(test)]
            ledger_byte_limit: MAX_LEDGER_BYTES,
            #[cfg(test)]
            fail_next_local_sync: std::cell::Cell::new(false),
        };
        store.synchronize_v3(witness)?;
        Ok(store)
    }

    /// Performs one explicit V2→V3 migration and returns its exact signed
    /// witness decision. The returned anchor still must be pinned externally;
    /// this function does not return a serving V3 adapter.
    pub fn migrate_from_v2(
        config: FullSyncIssuerV2MigrationConfig,
        migration: SignedFullSyncIssuerMigrationV3,
        witness: &mut dyn FullSyncIssuerWitnessV3,
    ) -> Result<FullSyncIssuerCommittedEntryV3, FormalError> {
        validate_v2_migration_inputs(
            &config.v2_policy,
            &config.policy,
            &config.v2_issuer_key,
            &config.lease_signer,
            &config.minimum_v2_anchor,
            &migration,
        )?;
        let lock = acquire_store_lock(&config.directory)?;
        let coordinator = MigrationCoordinator::new(
            config.directory,
            config.v2_policy,
            config.policy,
            config.v2_issuer_key,
            config.lease_signer,
            config.minimum_v2_anchor,
            lock,
        );
        coordinator.migrate(migration, witness)
    }

    /// Reconciles an uncertain migration using the exact pending migration.
    /// This never constructs or returns a serving issuer.
    pub fn resume_migration_v3(
        directory: &Path,
        v2_policy: VerifiedFullSyncIssuerPolicyV2,
        policy: VerifiedFullSyncIssuerPolicyV3,
        lease_signer: SigningKey,
        migration: SignedFullSyncIssuerMigrationV3,
        witness: &mut dyn FullSyncIssuerWitnessV3,
    ) -> Result<FullSyncIssuerMigrationResumeV3, FormalError> {
        validate_resume_inputs(&v2_policy, &policy, &lease_signer, &migration)?;
        let lock = acquire_store_lock(directory)?;
        // Resume uses the persisted V2 query and does not sign V2 commands.
        let coordinator = MigrationCoordinator::new(
            directory.into(),
            v2_policy,
            policy,
            lease_signer.clone(),
            lease_signer,
            migration.body.expected_v2_head.clone(),
            lock,
        );
        coordinator.resume(migration, witness)
    }

    pub fn minimum_anchor(&self) -> &FullSyncIssuerAnchorV3 {
        &self.minimum_anchor
    }

    /// Applies a signed governance event through the shared witness CAS.
    pub fn manage(
        &mut self,
        command: SignedFullSyncIssuerCommandV3,
        witness: &mut dyn FullSyncIssuerWitnessV3,
    ) -> Result<FullSyncIssuerCommittedEntryV3, FormalError> {
        self.ensure_healthy()?;
        let state = self.synchronize_v3(witness)?;
        let pending = match read_pending_any(&self.directory)? {
            Some(ReadPendingFile::V3(pending)) => Some(*pending),
            Some(ReadPendingFile::V2(_)) => {
                return Err(FormalError::Conflict(
                    "unresolved V2 intent must be recovered through the V2 migration path".into(),
                ));
            }
            None => None,
        };
        if let Some(PendingIntentV3::Management {
            command: original, ..
        }) = &pending
            && original.as_ref() != &command
        {
            return Err(FormalError::Conflict(
                "unresolved V3 intent only accepts the original signed request".into(),
            ));
        }
        if pending
            .as_ref()
            .is_some_and(|pending| !pending_matches_management(pending, &command))
        {
            return Err(FormalError::Conflict(
                "another V3 event is pending witness reconciliation".into(),
            ));
        }

        let observation = match &pending {
            Some(PendingIntentV3::Management { observation, .. }) => observation.as_ref().clone(),
            _ => self.observe_v3_for_head(witness, state.anchor())?,
        };
        let prepared = state.prepare_management(command.clone(), &observation)?;
        if let Some(existing) = prepared.existing() {
            return Ok(existing.clone());
        }
        let record = prepared
            .record()
            .cloned()
            .ok_or_else(|| invalid("V3 command has no candidate"))?;
        self.preflight_projected_ledger(state.v2_prefix(), state.entries(), &record, &observation)?;
        self.persist_pending_v3(PendingIntentV3::Management {
            schema_version: PENDING_SCHEMA_V3,
            observation: Box::new(observation.clone()),
            command: Box::new(command),
            record: Box::new(record.clone()),
        })?;
        self.commit_v3(witness, &state, prepared, observation, record)
    }

    /// Issues a provider lease only through the shared V3 event CAS. The exact
    /// pending signed time observation is reused after any uncertain response.
    pub fn issue_lease(
        &mut self,
        request: SignedFullSyncProviderLeaseRequestV3,
        witness: &mut dyn FullSyncIssuerWitnessV3,
    ) -> Result<FullSyncIssuerCommittedEntryV3, FormalError> {
        self.ensure_healthy()?;
        let state = self.synchronize_v3(witness)?;
        let pending = match read_pending_any(&self.directory)? {
            Some(ReadPendingFile::V3(pending)) => Some(*pending),
            Some(ReadPendingFile::V2(_)) => {
                return Err(FormalError::Conflict(
                    "unresolved V2 intent must be recovered through the V2 migration path".into(),
                ));
            }
            None => None,
        };
        if pending
            .as_ref()
            .is_some_and(|pending| !pending_matches_lease(pending, &request))
        {
            return Err(FormalError::Conflict(
                "unresolved V3 intent only accepts the original signed request".into(),
            ));
        }
        let observation = match &pending {
            Some(PendingIntentV3::Lease { observation, .. }) => observation.as_ref().clone(),
            _ => self.observe_v3_for_head(witness, state.anchor())?,
        };
        let prepared = state.prepare_lease(request.clone(), &observation, &self.lease_signer)?;
        if let Some(existing) = prepared.existing() {
            return Ok(existing.clone());
        }
        let record = prepared
            .record()
            .cloned()
            .ok_or_else(|| invalid("V3 lease has no candidate"))?;
        self.preflight_projected_ledger(state.v2_prefix(), state.entries(), &record, &observation)?;
        self.persist_pending_v3(PendingIntentV3::Lease {
            schema_version: PENDING_SCHEMA_V3,
            observation: Box::new(observation.clone()),
            request: Box::new(request),
            record: Box::new(record.clone()),
        })?;
        self.commit_v3(witness, &state, prepared, observation, record)
    }

    fn commit_v3(
        &mut self,
        witness: &mut dyn FullSyncIssuerWitnessV3,
        _state: &VerifiedFullSyncIssuerStateV3,
        prepared: PreparedFullSyncIssuerEventV3,
        observation: SignedFullSyncIssuerObservationV3,
        record: FullSyncIssuerRecordV3,
    ) -> Result<FullSyncIssuerCommittedEntryV3, FormalError> {
        let receipt = match witness.commit_v3(&observation, &record) {
            Ok(receipt) => receipt,
            Err(error) => {
                self.poisoned = true;
                return Err(error);
            }
        };
        let decision = match prepared.commit(receipt) {
            Ok(decision) => decision,
            Err(error) => {
                self.poisoned = true;
                return Err(error);
            }
        };
        if let Err(error) =
            self.persist_history(decision.state().v2_prefix(), decision.state().entries())
        {
            self.poisoned = true;
            return Err(error);
        }
        if let Err(error) = clear_pending_v3(&self.directory) {
            self.poisoned = true;
            return Err(error);
        }
        Ok(decision.entry().clone())
    }

    fn synchronize_v3(
        &mut self,
        witness: &mut dyn FullSyncIssuerWitnessV3,
    ) -> Result<VerifiedFullSyncIssuerStateV3, FormalError> {
        self.ensure_healthy()?;
        let local = read_local_ledger(&self.directory)?;
        let pending = read_pending_any(&self.directory)?;
        let observation = self.observe_v3_for_minimum(witness)?;
        let history = witness.history_v3(&observation)?;
        enforce_remote_bounds(&history)?;
        let remote = VerifiedFullSyncIssuerStateV3::from_witness_observation(
            self.v2_policy.clone(),
            self.policy.clone(),
            history,
            &observation,
        )?;
        verify_minimum_prefix(&remote, &self.minimum_anchor)?;
        verify_local_prefix(local.as_ref(), &remote)?;
        self.reconcile_pending_v3(pending.as_ref(), &remote)?;
        let repair = local
            .as_ref()
            .is_none_or(|ledger| !ledger_equals_state(ledger, &remote));
        if repair {
            self.persist_history(remote.v2_prefix(), remote.entries())?;
        }
        if pending_needs_clear(pending.as_ref(), &remote)? {
            clear_pending_v3(&self.directory)?;
        }
        Ok(remote)
    }

    fn reconcile_pending_v3(
        &self,
        pending: Option<&ReadPendingFile>,
        state: &VerifiedFullSyncIssuerStateV3,
    ) -> Result<(), FormalError> {
        let Some(pending) = pending else {
            return Ok(());
        };
        let pending = match pending {
            ReadPendingFile::V3(pending) => pending.as_ref(),
            ReadPendingFile::V2(pending) => {
                return self.validate_legacy_pending_after_migration(state, pending);
            }
        };
        validate_pending_schema(pending)?;
        match pending {
            PendingIntentV3::Migration {
                record,
                migration,
                original_v2_query,
                original_v2_observation,
                ..
            } => {
                migration::verify_migration_pending(
                    &self.v2_policy,
                    &self.policy,
                    original_v2_query,
                    original_v2_observation,
                    migration,
                    record,
                )?;
                let committed_entry = state.entries().iter().find(|entry| {
                    entry.record.event
                        == FullSyncIssuerEventV3::Migration(Box::new(migration.as_ref().clone()))
                        && entry.record == *record.as_ref()
                });
                let committed = committed_entry.is_some();
                if let Some(entry) = committed_entry
                    && entry.receipt.body.observation
                        != FullSyncIssuerReceiptObservationV3::Migration(
                            original_v2_observation.as_ref().clone(),
                        )
                {
                    return Err(FormalError::Conflict(
                        "committed migration receipt differs from the original pending observation"
                            .into(),
                    ));
                }
                if !committed {
                    return Err(FormalError::Conflict(
                        "V3 open cannot resolve an uncommitted migration; use migration resume"
                            .into(),
                    ));
                }
            }
            PendingIntentV3::Management {
                command,
                record,
                observation,
                ..
            } => {
                self.validate_event_pending(state, pending, observation)?;
                if let Some(entry) = find_event_by_request_id(state, &command.body.request_id)?
                    && (entry.record.event
                        != FullSyncIssuerEventV3::Management(Box::new(command.as_ref().clone()))
                        || entry.record != *record.as_ref())
                {
                    return Err(FormalError::Conflict(
                        "V3 pending request identity conflicts with witness history".into(),
                    ));
                }
            }
            PendingIntentV3::Lease {
                request,
                record,
                observation,
                ..
            } => {
                self.validate_event_pending(state, pending, observation)?;
                if !matches!(
                    &record.event,
                    FullSyncIssuerEventV3::Lease { request: candidate, .. }
                        if candidate.as_ref() == request.as_ref()
                ) {
                    return Err(invalid("pending lease candidate has wrong event/request"));
                }
                if let Some(entry) = find_event_by_request_id(state, &request.body.request_id)?
                    && entry.record != *record.as_ref()
                {
                    return Err(FormalError::Conflict(
                        "V3 pending lease identity conflicts with witness history".into(),
                    ));
                }
            }
        }
        Ok(())
    }

    fn validate_legacy_pending_after_migration(
        &self,
        state: &VerifiedFullSyncIssuerStateV3,
        pending: &super::PendingIntent,
    ) -> Result<(), FormalError> {
        if pending.schema_version != 2
            || pending.command != pending.record.command
            || pending.record.previous_anchor != pending.command.body.expected_head
        {
            return Err(invalid(
                "legacy V2 pending schema/command/predecessor mismatch",
            ));
        }
        let same_id: Vec<_> = state
            .v2_prefix()
            .iter()
            .filter(|entry| entry.record.command.body.request_id == pending.command.body.request_id)
            .collect();
        match same_id.as_slice() {
            [entry] => {
                if entry.record.command != pending.command || entry.record != pending.record {
                    return Err(FormalError::Conflict(
                        "V2 witness history conflicts with legacy pending identity".into(),
                    ));
                }
            }
            [] => {
                let revision = usize::try_from(pending.record.previous_anchor.revision)
                    .map_err(|_| invalid("legacy V2 pending revision conversion"))?;
                if revision > state.v2_prefix().len() {
                    return Err(FormalError::Rollback(
                        "legacy V2 pending head is ahead of migrated witness history".into(),
                    ));
                }
                let prefix = VerifiedFullSyncIssuerStateV2::from_committed_records(
                    self.v2_policy.clone(),
                    state.v2_prefix()[..revision].to_vec(),
                )?;
                let prepared = prefix.prepare(pending.command.clone())?;
                if prefix.anchor() != &pending.record.previous_anchor
                    || prepared.record() != Some(&pending.record)
                {
                    return Err(FormalError::Conflict(
                        "legacy V2 pending is not a valid candidate at its proven prefix".into(),
                    ));
                }
            }
            _ => {
                return Err(invalid(
                    "V2 witness repeats legacy pending request identity",
                ));
            }
        }
        // Migration atomically fences all future V2 commits, so after the full
        // V3 chain proves this prefix, an absent request is definitely retired.
        Ok(())
    }

    fn validate_event_pending(
        &self,
        remote: &VerifiedFullSyncIssuerStateV3,
        pending: &PendingIntentV3,
        observation: &SignedFullSyncIssuerObservationV3,
    ) -> Result<(), FormalError> {
        let record =
            pending_record(pending).ok_or_else(|| invalid("V3 pending event record missing"))?;
        let prefix_history = history_at_previous(remote, &record.previous_anchor)?;
        let prefix_state = VerifiedFullSyncIssuerStateV3::from_witness_observation(
            self.v2_policy.clone(),
            self.policy.clone(),
            prefix_history,
            observation,
        )?;
        let reproduced = match pending {
            PendingIntentV3::Management { command, .. } => {
                prefix_state.prepare_management(command.as_ref().clone(), observation)?
            }
            PendingIntentV3::Lease { request, .. } => prefix_state.prepare_lease(
                request.as_ref().clone(),
                observation,
                &self.lease_signer,
            )?,
            PendingIntentV3::Migration { .. } => {
                return Err(invalid("migration is not a V3 suffix event"));
            }
        };
        if reproduced.record() != Some(record) {
            return Err(FormalError::Conflict(
                "pending V3 event does not reproduce at its signed observed prefix".into(),
            ));
        }
        Ok(())
    }

    fn observe_v3_for_minimum(
        &self,
        witness: &mut dyn FullSyncIssuerWitnessV3,
    ) -> Result<SignedFullSyncIssuerObservationV3, FormalError> {
        let query = SignedFullSyncIssuerQueryV3::sign(
            &self.policy,
            self.minimum_anchor.clone(),
            random_nonce()?,
            &self.lease_signer,
        )?;
        let observation = witness.observe_v3(&query)?;
        if observation.body.query != query {
            return Err(invalid("V3 witness response does not bind fresh query"));
        }
        observation.verify(&self.policy)?;
        Ok(observation)
    }

    fn observe_v3_for_head(
        &self,
        witness: &mut dyn FullSyncIssuerWitnessV3,
        head: &FullSyncIssuerAnchorV3,
    ) -> Result<SignedFullSyncIssuerObservationV3, FormalError> {
        let query = SignedFullSyncIssuerQueryV3::sign(
            &self.policy,
            head.clone(),
            random_nonce()?,
            &self.lease_signer,
        )?;
        let observation = witness.observe_v3(&query)?;
        if observation.body.query != query {
            return Err(invalid("V3 witness response does not bind fresh query"));
        }
        observation.verify(&self.policy)?;
        if observation.body.current_anchor != *head {
            return Err(FormalError::Conflict(
                "V3 witness head changed before intent CAS".into(),
            ));
        }
        Ok(observation)
    }

    fn preflight_projected_ledger(
        &self,
        v2_entries: &[FullSyncIssuerCommittedEntryV2],
        v3_entries: &[FullSyncIssuerCommittedEntryV3],
        record: &FullSyncIssuerRecordV3,
        observation: &SignedFullSyncIssuerObservationV3,
    ) -> Result<(), FormalError> {
        let mut projected = v3_entries.to_vec();
        projected.push(FullSyncIssuerCommittedEntryV3 {
            record: record.clone(),
            receipt: placeholder_receipt(record, observation)?,
        });
        enforce_count(v2_entries.len(), projected.len())?;
        let ledger = LedgerFileV3 {
            schema_version: LEDGER_SCHEMA_V3,
            v2_entries: v2_entries.to_vec(),
            v3_entries: projected,
        };
        let _ = serialize_bounded(&ledger, self.ledger_byte_limit())?;
        Ok(())
    }

    fn persist_history(
        &self,
        v2_entries: &[FullSyncIssuerCommittedEntryV2],
        v3_entries: &[FullSyncIssuerCommittedEntryV3],
    ) -> Result<(), FormalError> {
        enforce_count(v2_entries.len(), v3_entries.len())?;
        let ledger = LedgerFileV3 {
            schema_version: LEDGER_SCHEMA_V3,
            v2_entries: v2_entries.to_vec(),
            v3_entries: v3_entries.to_vec(),
        };
        let bytes = serialize_bounded(&ledger, self.ledger_byte_limit())?;
        #[cfg(test)]
        if self.fail_next_local_sync.replace(false) {
            return Err(FormalError::Io(
                "simulated V3 local ledger fsync failure after witness decision".into(),
            ));
        }
        atomic_bounded(
            &self.directory,
            &self.directory.join(LEDGER_FILE),
            &bytes,
            MAX_LEDGER_BYTES,
        )
    }

    fn persist_pending_v3(&mut self, pending: PendingIntentV3) -> Result<(), FormalError> {
        validate_pending_schema(&pending)?;
        let bytes = serialize_bounded(&pending, MAX_LEDGER_BYTES)?;
        if let Err(error) = atomic_bounded(
            &self.directory,
            &self.directory.join(PENDING_FILE),
            &bytes,
            MAX_LEDGER_BYTES,
        ) {
            self.poisoned = true;
            return Err(error);
        }
        Ok(())
    }

    fn ledger_byte_limit(&self) -> usize {
        #[cfg(test)]
        {
            self.ledger_byte_limit
        }
        #[cfg(not(test))]
        {
            MAX_LEDGER_BYTES
        }
    }

    fn ensure_healthy(&self) -> Result<(), FormalError> {
        if self.poisoned {
            Err(FormalError::Io(
                "V3 full-sync issuer requires reopen and original-request reconciliation".into(),
            ))
        } else {
            Ok(())
        }
    }
}

#[path = "full_sync_issuer_v3_migration.rs"]
mod migration;
#[path = "full_sync_issuer_v3_storage.rs"]
mod storage;
use migration::MigrationCoordinator;
use storage::*;

#[cfg(test)]
#[path = "full_sync_issuer_v3_tests.rs"]
mod tests;
