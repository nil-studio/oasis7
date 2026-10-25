use super::*;
use std::fs;

const MAX_LEDGER_RECORDS_V3: usize = MAX_FULL_SYNC_ISSUER_LEDGER_RECORDS_V3;

pub(super) fn validate_v3_open_inputs(
    v2_policy: &VerifiedFullSyncIssuerPolicyV2,
    policy: &VerifiedFullSyncIssuerPolicyV3,
    lease_signer: &SigningKey,
    minimum: &FullSyncIssuerAnchorV3,
) -> Result<(), FormalError> {
    minimum.validate()?;
    if hex::encode(lease_signer.verifying_key().as_bytes()) != policy.body().lease_signer_public_key
    {
        return Err(invalid(
            "V3 lease signer does not match the verified policy",
        ));
    }
    if policy.body().world != v2_policy.body().world
        || policy.body().authority_epoch != v2_policy.body().authority_epoch
        || policy.scope_digest() != v2_policy.scope_digest()
        || policy.body().witness_public_key != v2_policy.body().witness_public_key
        || policy.body().signer_generation <= v2_policy.body().signer_generation
        || minimum.scope_digest != policy.scope_digest()
        || minimum.policy_digest != policy.policy_digest()
        || minimum.signer_generation != policy.body().signer_generation
    {
        return Err(FormalError::Rollback(
            "V3 policy or externally pinned minimum does not match the original witness scope"
                .into(),
        ));
    }
    Ok(())
}

pub(super) fn validate_v2_migration_inputs(
    v2_policy: &VerifiedFullSyncIssuerPolicyV2,
    policy: &VerifiedFullSyncIssuerPolicyV3,
    v2_issuer_key: &SigningKey,
    lease_signer: &SigningKey,
    minimum: &FullSyncIssuerAnchorV2,
    migration: &SignedFullSyncIssuerMigrationV3,
) -> Result<(), FormalError> {
    minimum.validate()?;
    if hex::encode(v2_issuer_key.verifying_key().as_bytes()) != v2_policy.body().issuer_public_key {
        return Err(invalid(
            "V2 migration observer key does not match old issuer policy",
        ));
    }
    if hex::encode(lease_signer.verifying_key().as_bytes()) != policy.body().lease_signer_public_key
    {
        return Err(invalid("V3 lease signer does not match new issuer policy"));
    }
    if minimum.scope_digest != v2_policy.scope_digest()
        || minimum.signer_generation > v2_policy.body().signer_generation
    {
        return Err(FormalError::Rollback(
            "V2 migration minimum scope/generation".into(),
        ));
    }
    if migration.body.new_policy != *policy.signed()
        || migration.body.previous_v2_policy_digest != v2_policy.policy_digest()
        || migration.body.previous_v2_signer_generation != v2_policy.body().signer_generation
        || migration.body.expected_v2_head.scope_digest != v2_policy.scope_digest()
    {
        return Err(FormalError::Conflict(
            "migration request does not bind the configured V2 and V3 policies".into(),
        ));
    }
    Ok(())
}

pub(super) fn validate_resume_inputs(
    v2_policy: &VerifiedFullSyncIssuerPolicyV2,
    policy: &VerifiedFullSyncIssuerPolicyV3,
    lease_signer: &SigningKey,
    migration: &SignedFullSyncIssuerMigrationV3,
) -> Result<(), FormalError> {
    if hex::encode(lease_signer.verifying_key().as_bytes()) != policy.body().lease_signer_public_key
        || migration.body.new_policy != *policy.signed()
        || migration.body.previous_v2_policy_digest != v2_policy.policy_digest()
        || migration.body.previous_v2_signer_generation != v2_policy.body().signer_generation
        || migration.body.expected_v2_head.scope_digest != v2_policy.scope_digest()
    {
        return Err(FormalError::Conflict(
            "migration resume policy/request binding".into(),
        ));
    }
    Ok(())
}

pub(super) fn acquire_store_lock(directory: &Path) -> Result<File, FormalError> {
    private_directory(directory)?;
    let lock_path = directory.join(LOCK_FILE);
    if lock_path.exists() {
        private_regular(&lock_path)?;
    }
    let lock = private_options()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(&lock_path)
        .map_err(io)?;
    #[cfg(unix)]
    {
        use std::os::fd::AsRawFd;
        // SAFETY: the owned file remains alive as long as the coordinator/store.
        if unsafe { libc::flock(lock.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) } != 0 {
            return Err(FormalError::Conflict(
                "full-sync issuer store is locked".into(),
            ));
        }
    }
    #[cfg(not(unix))]
    {
        let _ = lock;
        return Err(FormalError::IssuerUnproven);
    }
    Ok(lock)
}

pub(super) fn read_local_ledger(directory: &Path) -> Result<Option<ReadLedgerFile>, FormalError> {
    read_optional_bounded(&directory.join(LEDGER_FILE), MAX_LEDGER_BYTES)
}

pub(super) fn read_pending_v3(directory: &Path) -> Result<Option<PendingIntentV3>, FormalError> {
    read_optional_bounded(&directory.join(PENDING_FILE), MAX_LEDGER_BYTES)
}

pub(super) fn read_pending_any(directory: &Path) -> Result<Option<ReadPendingFile>, FormalError> {
    read_optional_bounded(&directory.join(PENDING_FILE), MAX_LEDGER_BYTES)
}

pub(super) fn clear_pending_v3(directory: &Path) -> Result<(), FormalError> {
    let path = directory.join(PENDING_FILE);
    match fs::symlink_metadata(&path) {
        Ok(_) => {
            private_regular(&path)?;
            fs::remove_file(&path).map_err(io)?;
            File::open(directory)
                .and_then(|directory| directory.sync_all())
                .map_err(io)
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(io(error)),
    }
}

pub(super) fn verify_local_prefix(
    local: Option<&ReadLedgerFile>,
    remote: &VerifiedFullSyncIssuerStateV3,
) -> Result<(), FormalError> {
    let Some(local) = local else {
        return Ok(());
    };
    let (local_v2, local_v3) = match local {
        ReadLedgerFile::V2(ledger) => {
            if ledger.schema_version != 2 {
                return Err(invalid("local V2 ledger schema version"));
            }
            (&ledger.entries[..], &[][..])
        }
        ReadLedgerFile::V3(ledger) => {
            if ledger.schema_version != LEDGER_SCHEMA_V3 {
                return Err(invalid("local V3 ledger schema version"));
            }
            if ledger.v3_entries.is_empty() {
                return Err(invalid("local V3 ledger is missing its migration entry"));
            }
            (&ledger.v2_entries[..], &ledger.v3_entries[..])
        }
    };
    if local_v2.len() > remote.v2_prefix().len()
        || local_v2
            .iter()
            .zip(remote.v2_prefix())
            .any(|(left, right)| left != right)
    {
        return Err(FormalError::Rollback(
            "local V2 issuer history is ahead of or forks the complete V3 witness history".into(),
        ));
    }
    if !local_v3.is_empty() && local_v2.len() != remote.v2_prefix().len() {
        return Err(FormalError::Rollback(
            "local V3 ledger omits part of the immutable V2 prefix".into(),
        ));
    }
    if local_v3.len() > remote.entries().len()
        || local_v3
            .iter()
            .zip(remote.entries())
            .any(|(left, right)| left != right)
    {
        return Err(FormalError::Rollback(
            "local V3 issuer history is ahead of or forks the witness".into(),
        ));
    }
    Ok(())
}

pub(super) fn verify_local_v2_prefix(
    local: Option<&ReadLedgerFile>,
    remote: &[FullSyncIssuerCommittedEntryV2],
) -> Result<(), FormalError> {
    match local {
        None => Ok(()),
        Some(ReadLedgerFile::V2(ledger)) if ledger.schema_version == 2 => {
            if ledger.entries.len() > remote.len()
                || ledger
                    .entries
                    .iter()
                    .zip(remote)
                    .any(|(left, right)| left != right)
            {
                return Err(FormalError::Rollback(
                    "local V2 ledger forks or exceeds V2 witness history".into(),
                ));
            }
            Ok(())
        }
        Some(ReadLedgerFile::V3(_)) => Err(FormalError::Rollback(
            "V3 local ledger cannot be overwritten while witness reports V2 mode".into(),
        )),
        Some(ReadLedgerFile::V2(_)) => Err(invalid("local V2 ledger schema version")),
    }
}

pub(super) fn verify_local_prefix_of_history(
    local: Option<&ReadLedgerFile>,
    history: &FullSyncIssuerHistoryV3,
) -> Result<(), FormalError> {
    let Some(local) = local else {
        return Ok(());
    };
    let (v2_entries, v3_entries) = match local {
        ReadLedgerFile::V2(ledger) if ledger.schema_version == 2 => (&ledger.entries[..], &[][..]),
        ReadLedgerFile::V2(_) => return Err(invalid("local V2 ledger schema version")),
        ReadLedgerFile::V3(ledger) if ledger.schema_version == LEDGER_SCHEMA_V3 => {
            if ledger.v3_entries.is_empty() {
                return Err(invalid("local V3 ledger is missing its migration entry"));
            }
            (&ledger.v2_entries[..], &ledger.v3_entries[..])
        }
        ReadLedgerFile::V3(_) => return Err(invalid("local V3 ledger schema version")),
    };
    if v2_entries.len() > history.v2_entries.len()
        || v2_entries
            .iter()
            .zip(&history.v2_entries)
            .any(|(left, right)| left != right)
        || (!v3_entries.is_empty() && v2_entries.len() != history.v2_entries.len())
        || v3_entries.len() > history.v3_entries.len()
        || v3_entries
            .iter()
            .zip(&history.v3_entries)
            .any(|(left, right)| left != right)
    {
        return Err(FormalError::Rollback(
            "local issuer ledger is not an exact prefix of the authenticated migration history"
                .into(),
        ));
    }
    Ok(())
}

pub(super) fn ledger_equals_state(
    local: &ReadLedgerFile,
    remote: &VerifiedFullSyncIssuerStateV3,
) -> bool {
    match local {
        ReadLedgerFile::V2(ledger) => {
            ledger.schema_version == 2
                && remote.entries().is_empty()
                && ledger.entries == remote.v2_prefix()
        }
        ReadLedgerFile::V3(ledger) => {
            ledger.schema_version == LEDGER_SCHEMA_V3
                && ledger.v2_entries == remote.v2_prefix()
                && ledger.v3_entries == remote.entries()
        }
    }
}

pub(super) fn verify_minimum_prefix(
    state: &VerifiedFullSyncIssuerStateV3,
    minimum: &FullSyncIssuerAnchorV3,
) -> Result<(), FormalError> {
    minimum.validate()?;
    if minimum.scope_digest != state.anchor().scope_digest
        || minimum.policy_digest != state.policy().policy_digest()
        || minimum.signer_generation != state.policy().body().signer_generation
        || minimum.revision > state.anchor().revision
    {
        return Err(FormalError::Rollback(
            "configured V3 minimum is not at or below witness head".into(),
        ));
    }
    if minimum == state.anchor() {
        return Ok(());
    }
    if !state
        .entries()
        .iter()
        .any(|entry| entry.record.next_anchor == *minimum)
    {
        return Err(FormalError::Rollback(
            "configured V3 minimum is not an exact history prefix".into(),
        ));
    }
    Ok(())
}

/// Confirms the externally pinned V2 floor is an exact prefix of the complete
/// authenticated V2 history. Migration queries use the exact CAS head as their
/// minimum so that a lost response can be resumed with the original query;
/// this independent check retains the older external rollback floor.
pub(super) fn verify_v2_minimum_prefix(
    policy: &VerifiedFullSyncIssuerPolicyV2,
    state: &VerifiedFullSyncIssuerStateV2,
    minimum: &FullSyncIssuerAnchorV2,
) -> Result<(), FormalError> {
    minimum.validate()?;
    if minimum.scope_digest != policy.scope_digest()
        || minimum.signer_generation > policy.body().signer_generation
        || minimum.revision > state.anchor().revision
    {
        return Err(FormalError::Rollback(
            "configured V2 migration minimum is outside or ahead of the witness history".into(),
        ));
    }
    if minimum.revision == 0 {
        if VerifiedFullSyncIssuerStateV2::genesis_anchor(policy)? == *minimum {
            return Ok(());
        }
    } else if usize::try_from(minimum.revision)
        .ok()
        .and_then(|revision| revision.checked_sub(1))
        .and_then(|index| state.records().get(index))
        .is_some_and(|entry| entry.record.next_anchor == *minimum)
    {
        return Ok(());
    }
    Err(FormalError::Rollback(
        "configured V2 migration minimum is not an exact witness-history prefix".into(),
    ))
}

pub(super) fn enforce_remote_bounds(history: &FullSyncIssuerHistoryV3) -> Result<(), FormalError> {
    enforce_count(history.v2_entries.len(), history.v3_entries.len())?;
    let ledger = LedgerFileV3 {
        schema_version: LEDGER_SCHEMA_V3,
        v2_entries: history.v2_entries.clone(),
        v3_entries: history.v3_entries.clone(),
    };
    let _ = serialize_bounded(&ledger, MAX_LEDGER_BYTES)?;
    Ok(())
}

pub(super) fn enforce_count(v2_count: usize, v3_count: usize) -> Result<(), FormalError> {
    if v2_count.saturating_add(v3_count) > MAX_LEDGER_RECORDS_V3 {
        return Err(invalid(
            "full-sync V3 complete history exceeds 1,024-event lifetime bound",
        ));
    }
    Ok(())
}

pub(super) fn enforce_v2_record_bounds(
    entries: &[FullSyncIssuerCommittedEntryV2],
) -> Result<(), FormalError> {
    if entries.len() > MAX_LEDGER_RECORDS {
        return Err(invalid(
            "full-sync V2 prefix exceeds shared event count bound",
        ));
    }
    let _ = serialize_bounded(&entries, MAX_LEDGER_BYTES)?;
    Ok(())
}

pub(super) fn preflight_migration(
    state: &VerifiedFullSyncIssuerStateV2,
    record: &FullSyncIssuerRecordV3,
    observation: &SignedFullSyncIssuerObservationV2,
    byte_limit: usize,
) -> Result<(), FormalError> {
    let mut v3_entries = vec![FullSyncIssuerCommittedEntryV3 {
        record: record.clone(),
        receipt: placeholder_migration_receipt(record, observation)?,
    }];
    enforce_count(state.records().len(), v3_entries.len())?;
    let ledger = LedgerFileV3 {
        schema_version: LEDGER_SCHEMA_V3,
        v2_entries: state.records().to_vec(),
        v3_entries: std::mem::take(&mut v3_entries),
    };
    let _ = serialize_bounded(&ledger, byte_limit)?;
    Ok(())
}

pub(super) fn placeholder_migration_receipt(
    record: &FullSyncIssuerRecordV3,
    observation: &SignedFullSyncIssuerObservationV2,
) -> Result<SignedFullSyncIssuerReceiptV3, FormalError> {
    let request_id = match &record.event {
        FullSyncIssuerEventV3::Migration(migration) => migration.body.request_id.clone(),
        _ => {
            return Err(invalid(
                "migration receipt placeholder for non-migration event",
            ));
        }
    };
    Ok(SignedFullSyncIssuerReceiptV3 {
        body: FullSyncIssuerReceiptBodyV3 {
            schema_version: 3,
            observation: FullSyncIssuerReceiptObservationV3::Migration(observation.clone()),
            record_digest: record.digest()?,
            request_id,
            previous_anchor: record.previous_anchor.clone(),
            next_anchor: record.next_anchor.clone(),
        },
        signature_hex: "0".repeat(128),
    })
}

pub(super) fn placeholder_receipt(
    record: &FullSyncIssuerRecordV3,
    observation: &SignedFullSyncIssuerObservationV3,
) -> Result<SignedFullSyncIssuerReceiptV3, FormalError> {
    let request_id = event_request_id(&record.event).to_owned();
    Ok(SignedFullSyncIssuerReceiptV3 {
        body: FullSyncIssuerReceiptBodyV3 {
            schema_version: 3,
            observation: FullSyncIssuerReceiptObservationV3::V3(observation.clone()),
            record_digest: record.digest()?,
            request_id,
            previous_anchor: record.previous_anchor.clone(),
            next_anchor: record.next_anchor.clone(),
        },
        signature_hex: "0".repeat(128),
    })
}

pub(super) fn pending_record(pending: &PendingIntentV3) -> Option<&FullSyncIssuerRecordV3> {
    match pending {
        PendingIntentV3::Migration { record, .. }
        | PendingIntentV3::Management { record, .. }
        | PendingIntentV3::Lease { record, .. } => Some(record.as_ref()),
    }
}

pub(super) fn validate_pending_schema(pending: &PendingIntentV3) -> Result<(), FormalError> {
    let (schema, record, valid_kind) = match pending {
        PendingIntentV3::Migration {
            schema_version,
            original_v2_query,
            original_v2_observation,
            migration,
            record,
        } => (
            *schema_version,
            record,
            original_v2_query.as_ref() == &original_v2_observation.body.query
                && record.event
                    == FullSyncIssuerEventV3::Migration(Box::new(migration.as_ref().clone()))
                && record.previous_anchor
                    == FullSyncIssuerPreviousAnchorV3::V2(migration.body.expected_v2_head.clone())
                && original_v2_query.body.minimum_anchor == migration.body.expected_v2_head
                && original_v2_observation.body.current_anchor == migration.body.expected_v2_head,
        ),
        PendingIntentV3::Management {
            schema_version,
            observation,
            command,
            record,
        } => (
            *schema_version,
            record,
            record.event == FullSyncIssuerEventV3::Management(Box::new(command.as_ref().clone()))
                && observation.body.current_anchor == *previous_anchor_v3(record)?,
        ),
        PendingIntentV3::Lease {
            schema_version,
            observation,
            request,
            record,
        } => (
            *schema_version,
            record,
            matches!(&record.event, FullSyncIssuerEventV3::Lease { request: candidate, .. } if candidate.as_ref() == request.as_ref())
                && observation.body.current_anchor == *previous_anchor_v3(record)?,
        ),
    };
    if schema != PENDING_SCHEMA_V3 || !valid_kind || record.schema_version != 3 {
        return Err(invalid("full-sync V3 pending schema or candidate mismatch"));
    }
    Ok(())
}

pub(super) fn pending_matches_management(
    pending: &PendingIntentV3,
    command: &SignedFullSyncIssuerCommandV3,
) -> bool {
    matches!(pending, PendingIntentV3::Management { command: old, .. } if old.as_ref() == command)
}

pub(super) fn pending_matches_lease(
    pending: &PendingIntentV3,
    request: &SignedFullSyncProviderLeaseRequestV3,
) -> bool {
    matches!(pending, PendingIntentV3::Lease { request: old, .. } if old.as_ref() == request)
}

pub(super) fn event_request_id(event: &FullSyncIssuerEventV3) -> &str {
    match event {
        FullSyncIssuerEventV3::Migration(migration) => &migration.body.request_id,
        FullSyncIssuerEventV3::Management(command) => &command.body.request_id,
        FullSyncIssuerEventV3::Lease { request, .. } => &request.body.request_id,
    }
}

pub(super) fn previous_anchor_v2(
    record: &FullSyncIssuerRecordV3,
) -> Result<&FullSyncIssuerAnchorV2, FormalError> {
    match &record.previous_anchor {
        FullSyncIssuerPreviousAnchorV3::V2(anchor) => Ok(anchor),
        FullSyncIssuerPreviousAnchorV3::V3(_) => {
            Err(invalid("record predecessor is not a V2 anchor"))
        }
    }
}

pub(super) fn previous_anchor_v3(
    record: &FullSyncIssuerRecordV3,
) -> Result<&FullSyncIssuerAnchorV3, FormalError> {
    match &record.previous_anchor {
        FullSyncIssuerPreviousAnchorV3::V3(anchor) => Ok(anchor),
        FullSyncIssuerPreviousAnchorV3::V2(_) => {
            Err(invalid("record predecessor is not a V3 anchor"))
        }
    }
}

pub(super) fn find_event_by_request_id<'a>(
    state: &'a VerifiedFullSyncIssuerStateV3,
    request_id: &str,
) -> Result<Option<&'a FullSyncIssuerCommittedEntryV3>, FormalError> {
    let mut matches = state
        .entries()
        .iter()
        .filter(|entry| event_request_id(&entry.record.event) == request_id);
    let first = matches.next();
    if matches.next().is_some() {
        return Err(invalid("V3 witness repeats an event request identity"));
    }
    Ok(first)
}

pub(super) fn pending_needs_clear(
    pending: Option<&ReadPendingFile>,
    state: &VerifiedFullSyncIssuerStateV3,
) -> Result<bool, FormalError> {
    let Some(pending) = pending else {
        return Ok(false);
    };
    let pending = match pending {
        ReadPendingFile::V2(_) => return Ok(true),
        ReadPendingFile::V3(pending) => pending.as_ref(),
    };
    let record = pending_record(pending).ok_or_else(|| invalid("V3 pending candidate missing"))?;
    let committed = match pending {
        PendingIntentV3::Migration { migration, .. } => state.entries().iter().any(|entry| {
            entry.record.event
                == FullSyncIssuerEventV3::Migration(Box::new(migration.as_ref().clone()))
                && entry.record == *record
        }),
        PendingIntentV3::Management { command, .. } => {
            find_event_by_request_id(state, &command.body.request_id)?.is_some_and(|entry| {
                entry.record.event
                    == FullSyncIssuerEventV3::Management(Box::new(command.as_ref().clone()))
                    && entry.record == *record
            })
        }
        PendingIntentV3::Lease { request, .. } => {
            find_event_by_request_id(state, &request.body.request_id)?
                .is_some_and(|entry| entry.record == *record)
        }
    };
    if committed {
        return Ok(true);
    }
    let previous = previous_anchor_v3(record)?;
    if previous == state.anchor() {
        return Ok(false);
    }
    if previous.revision < state.anchor().revision {
        return Ok(true);
    }
    Err(FormalError::Rollback(
        "pending V3 predecessor is not a witness prefix".into(),
    ))
}

pub(super) fn history_at_previous(
    remote: &VerifiedFullSyncIssuerStateV3,
    previous: &FullSyncIssuerPreviousAnchorV3,
) -> Result<FullSyncIssuerHistoryV3, FormalError> {
    match previous {
        FullSyncIssuerPreviousAnchorV3::V2(anchor) => {
            if remote.v2_prefix().len()
                != usize::try_from(anchor.revision)
                    .map_err(|_| invalid("V2 predecessor revision"))?
            {
                return Err(FormalError::Rollback(
                    "V3 pending expects a noncurrent V2 predecessor".into(),
                ));
            }
            let state = VerifiedFullSyncIssuerStateV2::from_committed_records(
                remote.v2_policy().clone(),
                remote.v2_prefix().to_vec(),
            )?;
            if state.anchor() != anchor {
                return Err(FormalError::Rollback(
                    "V3 pending V2 predecessor is not exact".into(),
                ));
            }
            Ok(FullSyncIssuerHistoryV3 {
                v2_entries: remote.v2_prefix().to_vec(),
                v3_entries: vec![],
            })
        }
        FullSyncIssuerPreviousAnchorV3::V3(anchor) => {
            let v2_len = remote.v2_prefix().len();
            let total =
                usize::try_from(anchor.revision).map_err(|_| invalid("V3 predecessor revision"))?;
            let v3_len = total
                .checked_sub(v2_len)
                .ok_or_else(|| FormalError::Rollback("V3 predecessor precedes migration".into()))?;
            if v3_len == 0
                || v3_len > remote.entries().len()
                || remote
                    .entries()
                    .get(v3_len - 1)
                    .map(|entry| &entry.record.next_anchor)
                    != Some(anchor)
            {
                return Err(FormalError::Rollback(
                    "pending V3 predecessor is not an exact history prefix".into(),
                ));
            }
            Ok(FullSyncIssuerHistoryV3 {
                v2_entries: remote.v2_prefix().to_vec(),
                v3_entries: remote.entries()[..v3_len].to_vec(),
            })
        }
    }
}
