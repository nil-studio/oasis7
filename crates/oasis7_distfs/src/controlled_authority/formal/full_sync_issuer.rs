//! Durable local coordination for the V2 full-sync issuer management ledger.
//!
//! This is a local adapter only. The required witness trait must be backed by
//! an independently protected monotonic service in production; a local file
//! or a test double cannot establish that property.
use super::{
    FormalError,
    issuer::{atomic_bounded, private_directory, private_options, private_regular},
};
use ed25519_dalek::SigningKey;
pub use oasis7_formal_protocol::controlled_authority::formal::full_sync_issuer::*;
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use std::{
    fs::{self, File},
    io::{Read, Write},
    path::{Path, PathBuf},
};

const MAX_LEDGER_BYTES: usize = 64 * 1024 * 1024;
const MAX_LEDGER_RECORDS: usize = MAX_FULL_SYNC_ISSUER_LEDGER_RECORDS_V2;
const LEDGER_FILE: &str = "full-sync-issuer-v2.json";
const PENDING_FILE: &str = "full-sync-issuer-v2.pending.json";
const LOCK_FILE: &str = "full-sync-issuer-v2.lock";

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct LedgerFile {
    schema_version: u32,
    entries: Vec<FullSyncIssuerCommittedEntryV2>,
}

/// The exact signed command and its deterministic candidate are persisted
/// before attempting witness CAS. The transport observation nonce is absent.
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct PendingIntent {
    schema_version: u32,
    command: SignedFullSyncIssuerCommandV2,
    record: FullSyncIssuerRecordV2,
}

/// A file-backed cache and coordinator around a required V2 witness.
///
/// The caller supplies the root-verified policy, matching current issuer key,
/// and externally pinned minimum anchor on every open. No provider grant or
/// V1 output can be produced by this API.
pub struct DurableFullSyncIssuerV2 {
    directory: PathBuf,
    policy: VerifiedFullSyncIssuerPolicyV2,
    key: SigningKey,
    minimum_anchor: FullSyncIssuerAnchorV2,
    _lock: File,
    poisoned: bool,
    #[cfg(test)]
    ledger_byte_limit: usize,
    #[cfg(test)]
    fail_next_local_sync: std::cell::Cell<bool>,
}

impl DurableFullSyncIssuerV2 {
    /// Opens and synchronizes the cache against the required witness.
    ///
    /// This call fails closed when the witness is unavailable, its complete
    /// log cannot be validated, or local state is corrupt/ahead/forked. A
    /// valid local prefix may be repaired from the authenticated witness log.
    pub fn open(
        directory: &Path,
        policy: VerifiedFullSyncIssuerPolicyV2,
        key: SigningKey,
        minimum_anchor: FullSyncIssuerAnchorV2,
        witness: &mut dyn FullSyncIssuerWitnessV2,
    ) -> Result<Self, FormalError> {
        validate_open_inputs(&policy, &key, &minimum_anchor)?;

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
            // SAFETY: this owned descriptor is held until the store is dropped.
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

        let mut issuer = Self {
            directory: directory.into(),
            policy,
            key,
            minimum_anchor,
            _lock: lock,
            poisoned: false,
            #[cfg(test)]
            ledger_byte_limit: MAX_LEDGER_BYTES,
            #[cfg(test)]
            fail_next_local_sync: std::cell::Cell::new(false),
        };
        issuer.synchronize(witness)?;
        Ok(issuer)
    }

    /// Applies one already-signed management command through the witness CAS.
    ///
    /// A transport or local durability error poisons this instance. Reopen it
    /// and submit the same signed command to reconcile; a different command is
    /// never automatically rebased or substituted.
    pub fn manage(
        &mut self,
        command: SignedFullSyncIssuerCommandV2,
        witness: &mut dyn FullSyncIssuerWitnessV2,
    ) -> Result<FullSyncIssuerCommittedEntryV2, FormalError> {
        if self.poisoned {
            return Err(FormalError::Io(
                "full-sync issuer requires reopen and original-request reconciliation".into(),
            ));
        }

        let state = self.synchronize(witness)?;
        let pending = self.read_pending()?;
        if let Some(pending) = &pending
            && pending.command != command
        {
            return Err(FormalError::Conflict(
                "unresolved full-sync intent only accepts the original signed command".into(),
            ));
        }

        let prepared = state.prepare(command.clone())?;
        if let Some(existing) = prepared.existing() {
            return Ok(existing.clone());
        }
        let record = prepared
            .record()
            .cloned()
            .ok_or_else(|| invalid("full-sync prepared command has no deterministic record"))?;

        let observation = self.observe_for_head(witness, &record.previous_anchor)?;
        self.preflight_projected_ledger(state.records(), &record, &observation)?;

        if let Some(pending) = pending {
            if pending.record != record || pending.command != record.command {
                return Err(FormalError::Conflict(
                    "pending full-sync command no longer matches its original candidate".into(),
                ));
            }
        } else {
            let intent = PendingIntent {
                schema_version: 2,
                command: command.clone(),
                record: record.clone(),
            };
            let bytes = serialize_bounded(&intent, MAX_LEDGER_BYTES)?;
            if let Err(error) = atomic_bounded(
                &self.directory,
                &self.directory.join(PENDING_FILE),
                &bytes,
                MAX_LEDGER_BYTES,
            ) {
                self.poisoned = true;
                return Err(error);
            }
        }

        // The exact-head query is fresh and is written into the durable receipt.
        // The witness CAS must still reject it if the head moved meanwhile.
        let receipt = match witness.commit(&observation, &record) {
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

        if let Err(error) = self.persist_entries(decision.state().records()) {
            self.poisoned = true;
            return Err(error);
        }
        if let Err(error) = self.clear_pending() {
            self.poisoned = true;
            return Err(error);
        }
        Ok(decision.entry().clone())
    }

    /// Returns the operator-pinned floor used by every witness observation.
    pub fn minimum_anchor(&self) -> &FullSyncIssuerAnchorV2 {
        &self.minimum_anchor
    }

    fn synchronize(
        &mut self,
        witness: &mut dyn FullSyncIssuerWitnessV2,
    ) -> Result<VerifiedFullSyncIssuerStateV2, FormalError> {
        if self.poisoned {
            return Err(FormalError::Io(
                "full-sync issuer requires reopen and original-request reconciliation".into(),
            ));
        }
        let local = self.read_local_ledger()?;
        let pending = self.read_pending()?;
        let (_, remote) = self.observe_and_reconstruct(witness)?;

        if let Some(local) = &local {
            if local.schema_version != 2 {
                return Err(invalid("full-sync local ledger schema version"));
            }
            let local_state = VerifiedFullSyncIssuerStateV2::from_committed_records(
                self.policy.clone(),
                local.entries.clone(),
            )?;
            if local.entries.len() > remote.records().len()
                || local
                    .entries
                    .iter()
                    .zip(remote.records())
                    .any(|(left, right)| left != right)
                || local_state.anchor().revision > remote.anchor().revision
            {
                return Err(FormalError::Rollback(
                    "local full-sync issuer ledger is ahead of or forks the witness".into(),
                ));
            }
        }

        let mut clear_pending = false;
        let mut must_persist_current = false;
        if let Some(pending) = &pending {
            if pending.schema_version != 2 || pending.command != pending.record.command {
                return Err(invalid("full-sync pending intent schema/command mismatch"));
            }
            if pending.record.previous_anchor != pending.command.body.expected_head {
                return Err(invalid(
                    "full-sync pending predecessor/command head mismatch",
                ));
            }
            let matches: Vec<_> = remote
                .records()
                .iter()
                .filter(|entry| {
                    entry.record.command.body.request_id == pending.command.body.request_id
                })
                .collect();
            match matches.as_slice() {
                [entry]
                    if entry.record.command == pending.command
                        && entry.record == pending.record =>
                {
                    clear_pending = true;
                }
                [_] => {
                    return Err(FormalError::Conflict(
                        "witness has a conflicting command for pending request identity".into(),
                    ));
                }
                [] => {
                    if remote.anchor().revision > pending.record.previous_anchor.revision
                        && anchor_is_exact_prefix(
                            &self.policy,
                            &remote,
                            &pending.record.previous_anchor,
                        )?
                    {
                        let pending_policy = VerifiedFullSyncIssuerPolicyV2::verify(
                            &pending.record.policy,
                            self.policy.trust(),
                        )?;
                        if pending_policy.scope_digest() != self.policy.scope_digest() {
                            return Err(FormalError::Conflict(
                                "pending full-sync candidate policy is outside configured root scope".into(),
                            ));
                        }
                        let prefix_len = usize::try_from(pending.record.previous_anchor.revision)
                            .map_err(|_| {
                            invalid("pending full-sync prefix revision conversion")
                        })?;
                        let prefix_state = VerifiedFullSyncIssuerStateV2::from_committed_records(
                            pending_policy,
                            remote.records()[..prefix_len].to_vec(),
                        )?;
                        if prefix_state.anchor() != &pending.record.previous_anchor
                            || prefix_state.prepare(pending.command.clone())?.record()
                                != Some(&pending.record)
                        {
                            return Err(FormalError::Conflict(
                                "pending full-sync command does not reproduce its candidate at the proven prefix".into(),
                            ));
                        }
                        // A CAS bound to this exact old head cannot have
                        // committed after the witness advanced without its
                        // request ID appearing in the complete signed log.
                        clear_pending = true;
                        must_persist_current = true;
                    } else if remote.anchor() == &pending.record.previous_anchor {
                        let prepared = remote.prepare(pending.command.clone())?;
                        if prepared.record() != Some(&pending.record) {
                            return Err(FormalError::Conflict(
                                "pending full-sync record differs from original deterministic candidate".into(),
                            ));
                        }
                    } else {
                        return Err(FormalError::Conflict(
                            "pending full-sync predecessor is not a proven witness prefix".into(),
                        ));
                    }
                }
                _ => return Err(invalid("witness repeats full-sync request identity")),
            }
        }

        let repair = local
            .as_ref()
            .is_none_or(|ledger| ledger.entries.len() != remote.records().len());
        if repair || must_persist_current {
            self.persist_entries(remote.records())?;
        }
        if clear_pending {
            self.clear_pending()?;
        }
        Ok(remote)
    }

    fn observe_and_reconstruct(
        &self,
        witness: &mut dyn FullSyncIssuerWitnessV2,
    ) -> Result<
        (
            SignedFullSyncIssuerObservationV2,
            VerifiedFullSyncIssuerStateV2,
        ),
        FormalError,
    > {
        let query = SignedFullSyncIssuerQueryV2::sign(
            &self.policy,
            self.minimum_anchor.clone(),
            random_nonce()?,
            &self.key,
        )?;
        let observation = witness.observe(&query)?;
        if observation.body.query != query {
            return Err(invalid(
                "full-sync witness response does not bind the fresh query",
            ));
        }
        observation.verify(&self.policy)?;
        let genesis =
            VerifiedFullSyncIssuerStateV2::from_committed_records(self.policy.clone(), vec![])?;
        let entries = witness.records_after(genesis.anchor(), &observation.body.current_anchor)?;
        if entries.len() > MAX_LEDGER_RECORDS {
            return Err(invalid("full-sync witness record count exceeds bound"));
        }
        let _ = serialize_bounded(&entries, MAX_LEDGER_BYTES)?;
        let state = VerifiedFullSyncIssuerStateV2::from_witness_observation(
            self.policy.clone(),
            entries,
            &observation,
        )?;
        Ok((observation, state))
    }

    fn observe_for_head(
        &self,
        witness: &mut dyn FullSyncIssuerWitnessV2,
        expected_head: &FullSyncIssuerAnchorV2,
    ) -> Result<SignedFullSyncIssuerObservationV2, FormalError> {
        let query = SignedFullSyncIssuerQueryV2::sign(
            &self.policy,
            expected_head.clone(),
            random_nonce()?,
            &self.key,
        )?;
        let observation = witness.observe(&query)?;
        if observation.body.query != query {
            return Err(invalid(
                "full-sync witness response does not bind the fresh query",
            ));
        }
        observation.verify(&self.policy)?;
        if observation.body.current_anchor != *expected_head {
            return Err(FormalError::Conflict(
                "full-sync witness head changed before intent CAS".into(),
            ));
        }
        Ok(observation)
    }

    fn preflight_projected_ledger(
        &self,
        entries: &[FullSyncIssuerCommittedEntryV2],
        record: &FullSyncIssuerRecordV2,
        observation: &SignedFullSyncIssuerObservationV2,
    ) -> Result<(), FormalError> {
        let placeholder_receipt = SignedFullSyncIssuerReceiptV2 {
            body: FullSyncIssuerReceiptBodyV2 {
                schema_version: 2,
                observation: observation.clone(),
                record_digest: record.digest()?,
                request_id: record.command.body.request_id.clone(),
                previous_anchor: record.previous_anchor.clone(),
                next_anchor: record.next_anchor.clone(),
            },
            // Ed25519 signatures are fixed at 64 bytes; a valid hex signature
            // therefore always occupies exactly 128 JSON characters.
            signature_hex: "0".repeat(128),
        };
        let mut projected = entries.to_vec();
        projected.push(FullSyncIssuerCommittedEntryV2 {
            record: record.clone(),
            receipt: placeholder_receipt,
        });
        let ledger = LedgerFile {
            schema_version: 2,
            entries: projected,
        };
        let _ = serialize_bounded(&ledger, self.ledger_byte_limit())?;
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

    fn read_local_ledger(&self) -> Result<Option<LedgerFile>, FormalError> {
        let path = self.directory.join(LEDGER_FILE);
        read_optional_bounded(&path, MAX_LEDGER_BYTES)
    }

    fn read_pending(&self) -> Result<Option<PendingIntent>, FormalError> {
        let path = self.directory.join(PENDING_FILE);
        read_optional_bounded(&path, MAX_LEDGER_BYTES)
    }

    fn persist_entries(
        &self,
        entries: &[FullSyncIssuerCommittedEntryV2],
    ) -> Result<(), FormalError> {
        let ledger = LedgerFile {
            schema_version: 2,
            entries: entries.to_vec(),
        };
        let bytes = serialize_bounded(&ledger, self.ledger_byte_limit())?;
        #[cfg(test)]
        if self.fail_next_local_sync.replace(false) {
            return Err(FormalError::Io(
                "simulated local ledger fsync failure after witness decision".into(),
            ));
        }
        atomic_bounded(
            &self.directory,
            &self.directory.join(LEDGER_FILE),
            &bytes,
            MAX_LEDGER_BYTES,
        )
    }

    fn clear_pending(&self) -> Result<(), FormalError> {
        let path = self.directory.join(PENDING_FILE);
        match fs::symlink_metadata(&path) {
            Ok(_) => {
                private_regular(&path)?;
                fs::remove_file(&path).map_err(io)?;
                File::open(&self.directory)
                    .and_then(|directory| directory.sync_all())
                    .map_err(io)
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(error) => Err(io(error)),
        }
    }
}

fn validate_open_inputs(
    policy: &VerifiedFullSyncIssuerPolicyV2,
    key: &SigningKey,
    minimum: &FullSyncIssuerAnchorV2,
) -> Result<(), FormalError> {
    minimum.validate()?;
    if hex::encode(key.verifying_key().as_bytes()) != policy.body().issuer_public_key {
        return Err(invalid(
            "full-sync issuer signer does not match root-verified policy",
        ));
    }
    if minimum.scope_digest != policy.scope_digest()
        || minimum.signer_generation > policy.body().signer_generation
    {
        return Err(FormalError::Rollback(
            "full-sync external minimum anchor scope/generation".into(),
        ));
    }
    Ok(())
}

fn anchor_is_exact_prefix(
    policy: &VerifiedFullSyncIssuerPolicyV2,
    state: &VerifiedFullSyncIssuerStateV2,
    expected: &FullSyncIssuerAnchorV2,
) -> Result<bool, FormalError> {
    expected.validate()?;
    if expected.scope_digest != state.anchor().scope_digest
        || expected.revision > state.anchor().revision
    {
        return Ok(false);
    }
    if expected.revision == 0 {
        return Ok(VerifiedFullSyncIssuerStateV2::genesis_anchor(policy)? == *expected);
    }
    let index = usize::try_from(expected.revision - 1)
        .map_err(|_| invalid("pending full-sync anchor revision conversion"))?;
    Ok(state
        .records()
        .get(index)
        .is_some_and(|entry| entry.record.next_anchor == *expected))
}

fn read_optional_bounded<T: DeserializeOwned>(
    path: &Path,
    max_bytes: usize,
) -> Result<Option<T>, FormalError> {
    match fs::symlink_metadata(path) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(io(error)),
        Ok(_) => {}
    }
    private_regular(path)?;
    let mut bytes = Vec::new();
    private_options()
        .read(true)
        .open(path)
        .map_err(io)?
        .take((max_bytes as u64).saturating_add(1))
        .read_to_end(&mut bytes)
        .map_err(io)?;
    if bytes.len() > max_bytes {
        return Err(invalid("full-sync issuer local file byte bound"));
    }
    serde_json::from_slice(&bytes)
        .map(Some)
        .map_err(|error| invalid(error.to_string()))
}

fn serialize_bounded<T: Serialize>(value: &T, max_bytes: usize) -> Result<Vec<u8>, FormalError> {
    let mut writer = BoundedWriter {
        bytes: Vec::with_capacity(64 * 1024),
        max_bytes,
    };
    serde_json::to_writer(&mut writer, value).map_err(|error| invalid(error.to_string()))?;
    Ok(writer.bytes)
}

struct BoundedWriter {
    bytes: Vec<u8>,
    max_bytes: usize,
}

impl Write for BoundedWriter {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        if self.bytes.len().saturating_add(bytes.len()) > self.max_bytes {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "full-sync issuer serialization exceeds byte bound",
            ));
        }
        self.bytes.extend_from_slice(bytes);
        Ok(bytes.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

fn random_nonce() -> Result<[u8; 32], FormalError> {
    let mut nonce = [0u8; 32];
    #[cfg(unix)]
    File::open("/dev/urandom")
        .and_then(|mut random| random.read_exact(&mut nonce))
        .map_err(io)?;
    #[cfg(not(unix))]
    return Err(FormalError::IssuerUnproven);
    Ok(nonce)
}

fn io(error: std::io::Error) -> FormalError {
    FormalError::Io(error.to_string())
}

fn invalid(message: impl Into<String>) -> FormalError {
    FormalError::Invalid(message.into())
}

#[cfg(test)]
#[path = "full_sync_issuer_v2_tests.rs"]
mod tests;

#[path = "full_sync_issuer_v3.rs"]
mod durable_v3;
pub use durable_v3::{
    DurableFullSyncIssuerV3, FullSyncIssuerMigrationResumeV3, FullSyncIssuerV2MigrationConfig,
};
