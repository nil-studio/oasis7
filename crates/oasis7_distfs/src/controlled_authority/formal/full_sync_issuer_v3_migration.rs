use super::*;

pub(super) struct MigrationCoordinator {
    directory: PathBuf,
    v2_policy: VerifiedFullSyncIssuerPolicyV2,
    policy: VerifiedFullSyncIssuerPolicyV3,
    v2_issuer_key: SigningKey,
    lease_signer: SigningKey,
    minimum_v2_anchor: FullSyncIssuerAnchorV2,
    _lock: File,
    poisoned: bool,
    #[cfg(test)]
    ledger_byte_limit: usize,
    #[cfg(test)]
    fail_next_local_sync: std::cell::Cell<bool>,
}

impl MigrationCoordinator {
    pub(super) fn new(
        directory: PathBuf,
        v2_policy: VerifiedFullSyncIssuerPolicyV2,
        policy: VerifiedFullSyncIssuerPolicyV3,
        v2_issuer_key: SigningKey,
        lease_signer: SigningKey,
        minimum_v2_anchor: FullSyncIssuerAnchorV2,
        lock: File,
    ) -> Self {
        Self {
            directory,
            v2_policy,
            policy,
            v2_issuer_key,
            lease_signer,
            minimum_v2_anchor,
            _lock: lock,
            poisoned: false,
            #[cfg(test)]
            ledger_byte_limit: MAX_LEDGER_BYTES,
            #[cfg(test)]
            fail_next_local_sync: std::cell::Cell::new(false),
        }
    }
    pub(super) fn migrate(
        self,
        migration: SignedFullSyncIssuerMigrationV3,
        witness: &mut dyn FullSyncIssuerWitnessV3,
    ) -> Result<FullSyncIssuerCommittedEntryV3, FormalError> {
        self.ensure_healthy()?;
        let pending = read_pending_any(&self.directory)?;
        match pending {
            Some(ReadPendingFile::V3(pending)) => match *pending {
                PendingIntentV3::Migration {
                    original_v2_query,
                    original_v2_observation,
                    migration: original,
                    record,
                    ..
                } => {
                    if original.as_ref() != &migration {
                        return Err(FormalError::Conflict(
                            "unresolved V3 migration only accepts the original signed request"
                                .into(),
                        ));
                    }
                    self.retry_or_recover_migration(
                        *original_v2_query,
                        *original_v2_observation,
                        *original,
                        *record,
                        witness,
                    )
                }
                PendingIntentV3::Management { .. } | PendingIntentV3::Lease { .. } => {
                    Err(FormalError::Conflict(
                        "another full-sync issuer event is pending witness reconciliation".into(),
                    ))
                }
            },
            Some(ReadPendingFile::V2(_)) => self.begin_migration(migration, witness),
            None => self.begin_migration(migration, witness),
        }
    }

    fn begin_migration(
        &self,
        migration: SignedFullSyncIssuerMigrationV3,
        witness: &mut dyn FullSyncIssuerWitnessV3,
    ) -> Result<FullSyncIssuerCommittedEntryV3, FormalError> {
        let local = read_local_ledger(&self.directory)?;
        let local_v2 = match local {
            Some(ReadLedgerFile::V2(local)) if local.schema_version == 2 => Some(local),
            Some(ReadLedgerFile::V2(_)) => {
                return Err(invalid("V2 ledger schema version before migration"));
            }
            Some(ReadLedgerFile::V3(_)) => {
                return Err(FormalError::Conflict(
                    "V3 migration already exists; use V3 resume/open".into(),
                ));
            }
            None => None,
        };

        // A V2 pending command must be resolved before an irreversible mode
        // switch. Never supersede it with the migration CAS.
        let legacy_pending = read_optional_bounded::<super::PendingIntent>(
            &self.directory.join(PENDING_FILE),
            MAX_LEDGER_BYTES,
        )?;
        if let Some(pending) = legacy_pending {
            return self.resolve_legacy_pending_before_migration(pending, witness);
        }

        // Bind the durable migration intent to the exact V2 CAS head. The
        // externally pinned minimum can be older; it is checked independently
        // against the complete reconstructed history below. This exact query
        // minimum is also required for lossless migration-response recovery.
        let (query, observation, state) =
            self.observe_v2_and_reconstruct(witness, migration.body.expected_v2_head.clone())?;
        verify_v2_minimum_prefix(&self.v2_policy, &state, &self.minimum_v2_anchor)?;
        let local_is_fork = local_v2.as_ref().is_some_and(|local| {
            local.entries.len() > state.records().len()
                || local
                    .entries
                    .iter()
                    .zip(state.records())
                    .any(|(local, remote)| local != remote)
        });
        if local_is_fork {
            return Err(FormalError::Rollback(
                "local V2 issuer history is ahead of or forks the witness".into(),
            ));
        }
        if local_v2.is_none() {
            self.persist_v2_entries(state.records())?;
        }
        if state.anchor() != &migration.body.expected_v2_head {
            return Err(FormalError::Conflict(
                "migration request does not bind the exact current V2 head".into(),
            ));
        }
        let prepared =
            state.prepare_migration_v3(self.policy.clone(), migration.clone(), &observation)?;
        let record = prepared
            .record()
            .cloned()
            .ok_or_else(|| invalid("V3 migration has no deterministic record"))?;
        preflight_migration(&state, &record, &observation, self.ledger_byte_limit())?;
        let intent = PendingIntentV3::Migration {
            schema_version: PENDING_SCHEMA_V3,
            original_v2_query: Box::new(query),
            original_v2_observation: Box::new(observation.clone()),
            migration: Box::new(migration),
            record: Box::new(record.clone()),
        };
        self.persist_pending(intent)?;
        self.commit_migration(prepared, observation, record, witness)
    }

    fn retry_or_recover_migration(
        &self,
        original_query: SignedFullSyncIssuerQueryV2,
        original_observation: SignedFullSyncIssuerObservationV2,
        migration: SignedFullSyncIssuerMigrationV3,
        record: FullSyncIssuerRecordV3,
        witness: &mut dyn FullSyncIssuerWitnessV3,
    ) -> Result<FullSyncIssuerCommittedEntryV3, FormalError> {
        verify_migration_pending(
            &self.v2_policy,
            &self.policy,
            &original_query,
            &original_observation,
            &migration,
            &record,
        )?;
        let resume = self.resume_from_original(
            original_query.clone(),
            migration.clone(),
            Some(&record),
            witness,
        )?;
        match resume {
            FullSyncIssuerMigrationResumeV3::Committed(entry) => Ok(*entry),
            FullSyncIssuerMigrationResumeV3::NotCommittedV2 {
                current_anchor,
                migration_head_conflict,
            } => {
                if migration_head_conflict || current_anchor != *previous_anchor_v2(&record)? {
                    return Err(FormalError::Conflict(
                        "V2 advanced before migration; original migration request is retired"
                            .into(),
                    ));
                }
                // Rebuild the deterministic event from the complete V2 head;
                // submit exactly the original observation and candidate.
                let state = self.reconstruct_v2_at_observation(&original_observation, witness)?;
                let prepared = state.prepare_migration_v3(
                    self.policy.clone(),
                    migration,
                    &original_observation,
                )?;
                if prepared.record() != Some(&record) {
                    return Err(FormalError::Conflict(
                        "pending migration candidate does not reproduce at its original head"
                            .into(),
                    ));
                }
                self.commit_migration(prepared, original_observation, record, witness)
            }
        }
    }

    fn commit_migration(
        &self,
        prepared: PreparedFullSyncIssuerEventV3,
        observation: SignedFullSyncIssuerObservationV2,
        record: FullSyncIssuerRecordV3,
        witness: &mut dyn FullSyncIssuerWitnessV3,
    ) -> Result<FullSyncIssuerCommittedEntryV3, FormalError> {
        // The pending file is intentionally retained on any witness error. A
        // fresh resume query and exact original request are the only recovery path.
        let receipt = witness.commit_migration_v3(&observation, &record)?;
        let decision = prepared.commit(receipt)?;
        self.persist_history(decision.state().v2_prefix(), decision.state().entries())?;
        clear_pending_v3(&self.directory)?;
        Ok(decision.entry().clone())
    }

    pub(super) fn resume(
        &self,
        migration: SignedFullSyncIssuerMigrationV3,
        witness: &mut dyn FullSyncIssuerWitnessV3,
    ) -> Result<FullSyncIssuerMigrationResumeV3, FormalError> {
        self.ensure_healthy()?;
        let (original_query, pending_record) = self.original_migration_material(&migration)?;
        let result = self.resume_from_original(
            original_query,
            migration.clone(),
            pending_record.as_ref(),
            witness,
        )?;
        match result {
            FullSyncIssuerMigrationResumeV3::Committed(entry) => {
                Ok(FullSyncIssuerMigrationResumeV3::Committed(entry))
            }
            FullSyncIssuerMigrationResumeV3::NotCommittedV2 {
                current_anchor,
                migration_head_conflict,
            } => Ok(FullSyncIssuerMigrationResumeV3::NotCommittedV2 {
                current_anchor,
                migration_head_conflict,
            }),
        }
    }

    fn resume_from_original(
        &self,
        original_query: SignedFullSyncIssuerQueryV2,
        migration: SignedFullSyncIssuerMigrationV3,
        expected_record: Option<&FullSyncIssuerRecordV3>,
        witness: &mut dyn FullSyncIssuerWitnessV3,
    ) -> Result<FullSyncIssuerMigrationResumeV3, FormalError> {
        let query = SignedFullSyncIssuerMigrationResumeQueryV3::sign(
            &self.policy,
            original_query.clone(),
            migration.clone(),
            random_nonce()?,
            &self.lease_signer,
        )?;
        let bundle = witness.resume_migration_v3(&query)?;
        enforce_remote_bounds(&bundle.history)?;
        match recover_migration_resume_v3(
            self.v2_policy.clone(),
            self.policy.clone(),
            &query,
            bundle,
        )? {
            FullSyncIssuerMigrationResumeResultV3::Committed(committed) => {
                let FullSyncIssuerCommittedMigrationResumeV3 {
                    history,
                    migration_entry,
                    original_receipt,
                    anchor,
                } = *committed;
                if expected_record.is_some_and(|record| record != &migration_entry.record) {
                    return Err(FormalError::Conflict(
                        "recovered migration differs from durable pending candidate".into(),
                    ));
                }
                if migration_entry.receipt != original_receipt
                    || migration_entry.record.next_anchor != anchor
                {
                    return Err(invalid(
                        "migration resume did not recover the exact receipt/anchor",
                    ));
                }
                verify_local_prefix_of_history(
                    read_local_ledger(&self.directory)?.as_ref(),
                    &history,
                )?;
                self.persist_history(&history.v2_entries, &history.v3_entries)?;
                clear_pending_v3(&self.directory)?;
                Ok(FullSyncIssuerMigrationResumeV3::Committed(Box::new(
                    migration_entry,
                )))
            }
            FullSyncIssuerMigrationResumeResultV3::NotCommittedV3MigrationConflict(conflict) => {
                let history = conflict.history();
                let Some(expected_record) = expected_record else {
                    return Err(FormalError::Rollback(
                        "competing V3 migration cannot retire a request without its exact pending candidate".into(),
                    ));
                };
                let Some(ReadPendingFile::V3(pending)) = read_pending_any(&self.directory)? else {
                    return Err(FormalError::Rollback(
                        "competing V3 migration requires the original durable pending intent"
                            .into(),
                    ));
                };
                let PendingIntentV3::Migration {
                    original_v2_query,
                    original_v2_observation,
                    migration: original_migration,
                    record: original_record,
                    ..
                } = *pending
                else {
                    return Err(FormalError::Conflict(
                        "competing V3 migration found a different pending issuer event".into(),
                    ));
                };
                if original_v2_query.as_ref() != &query.body.original_v2_query
                    || original_migration.as_ref() != &migration
                    || original_record.as_ref() != expected_record
                {
                    return Err(FormalError::Conflict(
                        "local pending migration differs from the original resume request".into(),
                    ));
                }
                verify_migration_pending(
                    &self.v2_policy,
                    &self.policy,
                    original_v2_query.as_ref(),
                    original_v2_observation.as_ref(),
                    original_migration.as_ref(),
                    original_record.as_ref(),
                )?;
                self.verify_migration_candidate_in_history(
                    history,
                    original_v2_query.as_ref(),
                    original_v2_observation.as_ref(),
                    original_migration.as_ref(),
                    original_record.as_ref(),
                )?;
                if Some(conflict.migration_entry()) != history.v3_entries.first()
                    || conflict.migration_entry().record.policy != *conflict.policy().signed()
                    || history
                        .v3_entries
                        .last()
                        .map(|entry| &entry.record.next_anchor)
                        != Some(conflict.current_anchor())
                {
                    return Err(invalid(
                        "competing migration policy, entry or current head differs from authenticated history",
                    ));
                }
                verify_local_prefix_of_history(
                    read_local_ledger(&self.directory)?.as_ref(),
                    history,
                )?;
                // The protocol helper proved the fresh response, complete V2
                // prefix, independent winner policy, full V3 chain and current
                // mode head. Persist that authenticated history before retiring
                // A. This path intentionally returns no serving state.
                self.persist_history(&history.v2_entries, &history.v3_entries)?;
                clear_pending_v3(&self.directory)?;
                Err(FormalError::Conflict(
                    "a competing V3 migration committed first; the original migration is retired and must be re-approved at the new V2 head".into(),
                ))
            }
            FullSyncIssuerMigrationResumeResultV3::NotCommittedV2 {
                current_state,
                current_anchor,
                migration_head_conflict,
            } => {
                if current_state.anchor() != &current_anchor {
                    return Err(invalid(
                        "migration resume V2 head differs from reconstructed history",
                    ));
                }
                // A V2-mode response may repair a missing local prefix, but it
                // must never overwrite a local fork or a schema-3 history.
                verify_local_v2_prefix(
                    read_local_ledger(&self.directory)?.as_ref(),
                    current_state.records(),
                )?;
                if migration_head_conflict {
                    // The protocol proves the original head is a prefix and
                    // this exact migration request is absent from V2 mode.
                    let pending = read_pending_any(&self.directory)?;
                    if let Some(ReadPendingFile::V3(pending)) = pending
                        && let PendingIntentV3::Migration { record, .. } = *pending
                    {
                        if expected_record.is_some_and(|expected| expected != record.as_ref()) {
                            return Err(FormalError::Conflict(
                                "local migration pending candidate differs from requested migration".into(),
                            ));
                        }
                        self.verify_migration_record_at_prefix(&current_state, record.as_ref())?;
                        self.persist_v2_entries(current_state.records())?;
                        clear_pending_v3(&self.directory)?;
                    }
                }
                Ok(FullSyncIssuerMigrationResumeV3::NotCommittedV2 {
                    current_anchor,
                    migration_head_conflict,
                })
            }
        }
    }

    fn original_migration_material(
        &self,
        migration: &SignedFullSyncIssuerMigrationV3,
    ) -> Result<(SignedFullSyncIssuerQueryV2, Option<FullSyncIssuerRecordV3>), FormalError> {
        match read_pending_any(&self.directory)? {
            Some(ReadPendingFile::V3(pending)) => match *pending {
                PendingIntentV3::Migration {
                    original_v2_query,
                    original_v2_observation,
                    migration: stored,
                    record,
                    ..
                } => {
                    if stored.as_ref() != migration {
                        return Err(FormalError::Conflict(
                            "pending migration identity changed".into(),
                        ));
                    }
                    verify_migration_pending(
                        &self.v2_policy,
                        &self.policy,
                        &original_v2_query,
                        &original_v2_observation,
                        &stored,
                        &record,
                    )?;
                    return Ok((*original_v2_query, Some(*record)));
                }
                PendingIntentV3::Management { .. } | PendingIntentV3::Lease { .. } => {
                    return Err(FormalError::Conflict(
                        "another issuer intent blocks migration recovery".into(),
                    ));
                }
            },
            Some(ReadPendingFile::V2(_)) => {
                return Err(FormalError::Conflict(
                    "another issuer intent blocks migration recovery".into(),
                ));
            }
            None => {}
        }
        if let Some(ReadLedgerFile::V3(ledger)) = read_local_ledger(&self.directory)?
            && let Some(entry) = ledger.v3_entries.iter().find(|entry| {
                entry.record.event == FullSyncIssuerEventV3::Migration(Box::new(migration.clone()))
            })
        {
            entry
                .receipt
                .verify_for_record(&entry.record, &self.v2_policy, &self.policy)?;
            if let FullSyncIssuerReceiptObservationV3::Migration(observation) =
                &entry.receipt.body.observation
            {
                return Ok((observation.body.query.clone(), Some(entry.record.clone())));
            }
        }
        Err(FormalError::Rollback(
            "original V2 migration query unavailable; preserve the original pending/history".into(),
        ))
    }

    fn observe_v2_and_reconstruct(
        &self,
        witness: &mut dyn FullSyncIssuerWitnessV3,
        query_minimum: FullSyncIssuerAnchorV2,
    ) -> Result<
        (
            SignedFullSyncIssuerQueryV2,
            SignedFullSyncIssuerObservationV2,
            VerifiedFullSyncIssuerStateV2,
        ),
        FormalError,
    > {
        let query = SignedFullSyncIssuerQueryV2::sign(
            &self.v2_policy,
            query_minimum,
            random_nonce()?,
            &self.v2_issuer_key,
        )?;
        let observation = witness.observe(&query)?;
        if observation.body.query != query {
            return Err(invalid(
                "V2 witness response does not bind fresh migration query",
            ));
        }
        observation.verify(&self.v2_policy)?;
        let state = self.reconstruct_v2_at_observation(&observation, witness)?;
        Ok((query, observation, state))
    }

    fn reconstruct_v2_at_observation(
        &self,
        observation: &SignedFullSyncIssuerObservationV2,
        witness: &mut dyn FullSyncIssuerWitnessV3,
    ) -> Result<VerifiedFullSyncIssuerStateV2, FormalError> {
        observation.verify(&self.v2_policy)?;
        let genesis =
            VerifiedFullSyncIssuerStateV2::from_committed_records(self.v2_policy.clone(), vec![])?;
        let entries = witness.records_after(genesis.anchor(), &observation.body.current_anchor)?;
        enforce_v2_record_bounds(&entries)?;
        VerifiedFullSyncIssuerStateV2::from_witness_observation(
            self.v2_policy.clone(),
            entries,
            observation,
        )
    }

    fn resolve_legacy_pending_before_migration(
        &self,
        pending: super::PendingIntent,
        witness: &mut dyn FullSyncIssuerWitnessV3,
    ) -> Result<FullSyncIssuerCommittedEntryV3, FormalError> {
        let (_, observation, remote) =
            self.observe_v2_and_reconstruct(witness, self.minimum_v2_anchor.clone())?;
        verify_local_v2_prefix(
            read_local_ledger(&self.directory)?.as_ref(),
            remote.records(),
        )?;
        if pending.schema_version != 2 || pending.command != pending.record.command {
            return Err(invalid("V2 pending intent schema/command mismatch"));
        }
        let found = remote.records().iter().any(|entry| {
            entry.record.command.body.request_id == pending.command.body.request_id
                && entry.record.command == pending.command
                && entry.record == pending.record
        });
        if found {
            self.persist_v2_entries(remote.records())?;
            clear_pending_v3(&self.directory)?;
            return Err(FormalError::Conflict(
                "original V2 command committed; retry migration with a fresh manager decision"
                    .into(),
            ));
        }
        if remote
            .records()
            .iter()
            .any(|entry| entry.record.command.body.request_id == pending.command.body.request_id)
        {
            return Err(FormalError::Conflict(
                "witness has a conflicting V2 decision for the pending request identity".into(),
            ));
        }
        let prefix_len = usize::try_from(pending.record.previous_anchor.revision)
            .map_err(|_| invalid("V2 pending predecessor revision conversion"))?;
        if prefix_len > remote.records().len() {
            return Err(FormalError::Rollback(
                "V2 pending predecessor is ahead of witness".into(),
            ));
        }
        let prefix = VerifiedFullSyncIssuerStateV2::from_committed_records(
            self.v2_policy.clone(),
            remote.records()[..prefix_len].to_vec(),
        )?;
        let prepared = prefix.prepare(pending.command.clone())?;
        if prefix.anchor() != &pending.record.previous_anchor
            || prepared.record() != Some(&pending.record)
        {
            return Err(FormalError::Conflict(
                "V2 pending candidate is not valid at its exact predecessor".into(),
            ));
        }
        if remote.anchor() == prefix.anchor() {
            return Err(FormalError::Conflict(
                "uncommitted V2 command must be resolved before migration".into(),
            ));
        }
        // A complete advancing V2 history proves this CAS never committed.
        let _ = observation;
        self.persist_v2_entries(remote.records())?;
        clear_pending_v3(&self.directory)?;
        Err(FormalError::Conflict(
            "V2 advanced before migration; submit a freshly approved migration request".into(),
        ))
    }

    fn persist_v2_entries(
        &self,
        entries: &[FullSyncIssuerCommittedEntryV2],
    ) -> Result<(), FormalError> {
        enforce_v2_record_bounds(entries)?;
        let ledger = super::LedgerFile {
            schema_version: 2,
            entries: entries.to_vec(),
        };
        let bytes = serialize_bounded(&ledger, MAX_LEDGER_BYTES)?;
        atomic_bounded(
            &self.directory,
            &self.directory.join(LEDGER_FILE),
            &bytes,
            MAX_LEDGER_BYTES,
        )
    }

    fn persist_history(
        &self,
        v2_entries: &[FullSyncIssuerCommittedEntryV2],
        v3_entries: &[FullSyncIssuerCommittedEntryV3],
    ) -> Result<(), FormalError> {
        let ledger = LedgerFileV3 {
            schema_version: LEDGER_SCHEMA_V3,
            v2_entries: v2_entries.to_vec(),
            v3_entries: v3_entries.to_vec(),
        };
        enforce_count(v2_entries.len(), v3_entries.len())?;
        let bytes = serialize_bounded(&ledger, self.ledger_byte_limit())?;
        #[cfg(test)]
        if self.fail_next_local_sync.replace(false) {
            return Err(FormalError::Io(
                "simulated V3 migration ledger fsync failure after witness decision".into(),
            ));
        }
        atomic_bounded(
            &self.directory,
            &self.directory.join(LEDGER_FILE),
            &bytes,
            MAX_LEDGER_BYTES,
        )
    }

    fn persist_pending(&self, pending: PendingIntentV3) -> Result<(), FormalError> {
        validate_pending_schema(&pending)?;
        let bytes = serialize_bounded(&pending, MAX_LEDGER_BYTES)?;
        atomic_bounded(
            &self.directory,
            &self.directory.join(PENDING_FILE),
            &bytes,
            MAX_LEDGER_BYTES,
        )
    }

    fn verify_migration_record_at_prefix(
        &self,
        state: &VerifiedFullSyncIssuerStateV2,
        record: &FullSyncIssuerRecordV3,
    ) -> Result<(), FormalError> {
        let FullSyncIssuerPreviousAnchorV3::V2(previous) = &record.previous_anchor else {
            return Err(invalid("migration record predecessor is not V2"));
        };
        let prefix_len =
            usize::try_from(previous.revision).map_err(|_| invalid("migration prefix revision"))?;
        if prefix_len > state.records().len() {
            return Err(FormalError::Rollback(
                "migration pending head is ahead of witness".into(),
            ));
        }
        let prefix = VerifiedFullSyncIssuerStateV2::from_committed_records(
            self.v2_policy.clone(),
            state.records()[..prefix_len].to_vec(),
        )?;
        if prefix.anchor() != previous {
            return Err(FormalError::Rollback(
                "migration pending head is not an exact V2 prefix".into(),
            ));
        }
        let FullSyncIssuerEventV3::Migration(migration) = &record.event else {
            return Err(invalid("migration pending record has wrong event kind"));
        };
        let observation = match read_pending_v3(&self.directory)? {
            Some(PendingIntentV3::Migration {
                original_v2_observation,
                ..
            }) => original_v2_observation,
            _ => return Err(invalid("migration pending original observation missing")),
        };
        let prepared = prefix.prepare_migration_v3(
            self.policy.clone(),
            migration.as_ref().clone(),
            &observation,
        )?;
        if prepared.record() != Some(record) {
            return Err(FormalError::Conflict(
                "migration pending does not reproduce at its signed V2 prefix".into(),
            ));
        }
        Ok(())
    }

    fn verify_migration_candidate_in_history(
        &self,
        history: &FullSyncIssuerHistoryV3,
        query: &SignedFullSyncIssuerQueryV2,
        observation: &SignedFullSyncIssuerObservationV2,
        migration: &SignedFullSyncIssuerMigrationV3,
        expected_record: &FullSyncIssuerRecordV3,
    ) -> Result<(), FormalError> {
        let prefix_len = usize::try_from(migration.body.expected_v2_head.revision)
            .map_err(|_| invalid("migration expected V2 prefix revision"))?;
        if prefix_len > history.v2_entries.len() {
            return Err(FormalError::Rollback(
                "pending migration head is beyond the authenticated V2 prefix".into(),
            ));
        }
        let prefix = VerifiedFullSyncIssuerStateV2::from_committed_records(
            self.v2_policy.clone(),
            history.v2_entries[..prefix_len].to_vec(),
        )?;
        if prefix.anchor() != &migration.body.expected_v2_head
            || query.body.minimum_anchor != migration.body.expected_v2_head
            || observation.body.current_anchor != migration.body.expected_v2_head
        {
            return Err(FormalError::Rollback(
                "pending migration query and candidate do not name an exact authenticated V2 prefix".into(),
            ));
        }
        let prepared =
            prefix.prepare_migration_v3(self.policy.clone(), migration.clone(), observation)?;
        if prepared.record() != Some(expected_record) {
            return Err(FormalError::Conflict(
                "pending migration candidate does not reproduce from the authenticated V2 prefix"
                    .into(),
            ));
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
                "V3 migration requires reopen and original-request reconciliation".into(),
            ))
        } else {
            Ok(())
        }
    }
}

pub(super) fn verify_migration_pending(
    old_policy: &VerifiedFullSyncIssuerPolicyV2,
    new_policy: &VerifiedFullSyncIssuerPolicyV3,
    query: &SignedFullSyncIssuerQueryV2,
    observation: &SignedFullSyncIssuerObservationV2,
    migration: &SignedFullSyncIssuerMigrationV3,
    record: &FullSyncIssuerRecordV3,
) -> Result<(), FormalError> {
    query.verify(old_policy)?;
    observation.verify(old_policy)?;
    migration.verify(old_policy, new_policy)?;
    let expected_head = &migration.body.expected_v2_head;
    if observation.body.query != *query
        || query.body.minimum_anchor != *expected_head
        || observation.body.current_anchor != *expected_head
        || record.schema_version != 3
        || record.policy != *new_policy.signed()
        || record.previous_anchor != FullSyncIssuerPreviousAnchorV3::V2(expected_head.clone())
        || record.event != FullSyncIssuerEventV3::Migration(Box::new(migration.clone()))
        || record.next_anchor.policy_digest != new_policy.policy_digest()
        || record.next_anchor.signer_generation != new_policy.body().signer_generation
    {
        return Err(FormalError::Conflict(
            "persisted migration intent does not bind its original signed query, policy, or candidate".into(),
        ));
    }
    Ok(())
}
