use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use ed25519_dalek::SigningKey;
use serde::{Deserialize, Serialize};

use super::proof::sign_local_decision;
use super::storage::{self, FaultInjection};
use super::*;

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Snapshot {
    schema_version: u32,
    authority: TrustedLocalAuthority,
    decisions: Vec<ControlledAuthorityLocalDecisionProofV1>,
}

type RequestIndex = BTreeMap<LocalRequestIdentity, usize>;

/// Single local storage path writer. Never grants distributed authority.
pub struct ControlledAuthorityLocalJournal {
    directory: PathBuf,
    _writer_lock: super::writer_lock::WriterLock,
    signer: SigningKey,
    snapshot: Snapshot,
    head: LocalHeadAnchor,
    requests: RequestIndex,
    poisoned: bool,
    #[cfg(test)]
    pub(super) fault: FaultInjection,
}

impl ControlledAuthorityLocalJournal {
    /// Parent directory must already exist and be operator-controlled.
    /// A minimum anchor is mandatory. Explicit genesis position 0 permits first
    /// initialization, but cannot detect rollback of a previously used journal.
    pub fn open(
        directory: &Path,
        trusted: TrustedLocalAuthority,
        signer: SigningKey,
        minimum_head_anchor: LocalHeadAnchor,
    ) -> Result<Self, LocalJournalError> {
        if !cfg!(unix) {
            return Err(LocalJournalError::UnsupportedPlatform);
        }
        trusted.validate()?;
        if hex::encode(signer.verifying_key().to_bytes()) != trusted.signer_public_key_hex {
            return Err(invalid("private signer differs from trusted signer"));
        }
        let writer_lock = storage::lock_directory(directory)?;
        let snapshot = match storage::read_snapshot(directory)? {
            Some(bytes) => decode_snapshot(&bytes)?,
            None => {
                if minimum_head_anchor != trusted.genesis_anchor()? {
                    return Err(invalid(
                        "missing journal cannot satisfy trusted history anchor",
                    ));
                }
                let initial = Snapshot {
                    schema_version: 1,
                    authority: trusted.clone(),
                    decisions: Vec::new(),
                };
                storage::persist_snapshot(
                    directory,
                    &encode_snapshot(&initial)?,
                    FaultInjection::None,
                )
                .map_err(storage::io_error)?;
                initial
            }
        };
        let (head, requests) = validate_snapshot(&snapshot, &trusted, &minimum_head_anchor)?;
        // Resolve prior process-crash/rename uncertainty before claiming local durability.
        storage::sync_existing_snapshot(directory)?;
        Ok(Self {
            directory: directory.to_path_buf(),
            _writer_lock: writer_lock,
            signer,
            snapshot,
            head,
            requests,
            poisoned: false,
            #[cfg(test)]
            fault: FaultInjection::None,
        })
    }

    /// Caller must retain this outside the journal storage to detect later rollback.
    pub fn head_anchor(&self) -> LocalHeadAnchor {
        self.head.clone()
    }

    pub fn append(
        &mut self,
        expected_epoch: u64,
        expected_parent: &LocalHeadAnchor,
        request: LocalRequestIdentity,
        prepared: PreparedLocalDecision,
    ) -> Result<LocalAppendOutcome, LocalJournalError> {
        if self.poisoned {
            return Err(LocalJournalError::Poisoned);
        }
        request.validate()?;
        prepared.validate()?;
        // A retry resolves the original request before evaluating a stale parent.
        if let Some(index) = self.requests.get(&request) {
            let existing = &self.snapshot.decisions[*index];
            if existing.decision.prepared.payload_digest != prepared.payload_digest {
                return Err(invalid(
                    "request identity reused with a different payload digest",
                ));
            }
            return Ok(LocalAppendOutcome::LocallyRecorded(Box::new(
                existing.clone(),
            )));
        }
        if expected_epoch != self.snapshot.authority.authority_epoch
            || expected_parent != &self.head
        {
            return Err(invalid(
                "expected authority epoch/parent/head fence mismatch",
            ));
        }
        let position = self
            .head
            .position
            .checked_add(1)
            .ok_or_else(|| invalid("position exhausted"))?;
        let proof = sign_local_decision(
            LocalDecisionBodyV1 {
                schema_version: LOCAL_DECISION_SCHEMA,
                profile: LOCAL_AUTHORITY_PROFILE.to_string(),
                scope: LOCAL_DECISION_SCOPE.to_string(),
                authority: self.snapshot.authority.clone(),
                position,
                parent_hash: self.head.proof_hash.clone(),
                request: request.clone(),
                prepared,
            },
            &self.signer,
        )?;
        let next_head = verify_local_decision_proof(&proof, &self.snapshot.authority, &self.head)?;
        let mut decisions = self.snapshot.decisions.clone();
        decisions.push(proof.clone());
        let next = Snapshot {
            schema_version: 1,
            authority: self.snapshot.authority.clone(),
            decisions,
        };
        let bytes = encode_snapshot(&next)?;
        #[cfg(test)]
        let fault = self.fault;
        #[cfg(not(test))]
        let fault = FaultInjection::None;
        if storage::persist_snapshot(&self.directory, &bytes, fault).is_err() {
            self.poisoned = true;
            return Ok(LocalAppendOutcome::Unknown { request });
        }
        self.requests.insert(request, next.decisions.len() - 1);
        self.snapshot = next;
        self.head = next_head;
        Ok(LocalAppendOutcome::LocallyRecorded(Box::new(proof)))
    }

    /// Revalidate and sync the canonical snapshot under the retained exclusive lock.
    /// On I/O or verification failure there is no definitive local answer.
    /// Querying a poisoned writer does not unpoison it; drop and reopen explicitly.
    pub fn lookup(
        &mut self,
        request: &LocalRequestIdentity,
    ) -> Result<LocalLookupOutcome, LocalJournalError> {
        request.validate()?;
        let bytes = storage::read_snapshot(&self.directory)?
            .ok_or_else(|| invalid("journal snapshot disappeared"))?;
        let snapshot = decode_snapshot(&bytes)?;
        let (head, requests) = validate_snapshot(&snapshot, &self.snapshot.authority, &self.head)?;
        storage::sync_existing_snapshot(&self.directory)?;
        self.snapshot = snapshot;
        self.head = head;
        self.requests = requests;
        Ok(match self.requests.get(request) {
            Some(index) => LocalLookupOutcome::LocallyRecorded(Box::new(
                self.snapshot.decisions[*index].clone(),
            )),
            None => LocalLookupOutcome::NotLocallyRecorded,
        })
    }
}

fn encode_snapshot(snapshot: &Snapshot) -> Result<Vec<u8>, LocalJournalError> {
    let bytes = serde_json::to_vec(snapshot).map_err(|error| invalid(error.to_string()))?;
    if bytes.len() > storage::MAX_SNAPSHOT_BYTES {
        return Err(invalid("local snapshot size limit exceeded"));
    }
    Ok(bytes)
}

fn decode_snapshot(bytes: &[u8]) -> Result<Snapshot, LocalJournalError> {
    serde_json::from_slice(bytes).map_err(|error| invalid(format!("snapshot decode: {error}")))
}

fn validate_snapshot(
    snapshot: &Snapshot,
    trusted: &TrustedLocalAuthority,
    minimum: &LocalHeadAnchor,
) -> Result<(LocalHeadAnchor, RequestIndex), LocalJournalError> {
    if snapshot.schema_version != 1 || &snapshot.authority != trusted {
        return Err(invalid("snapshot schema/trusted identity mismatch"));
    }
    let mut head = trusted.genesis_anchor()?;
    let mut anchor_found = &head == minimum;
    let mut requests = BTreeMap::new();
    for (index, proof) in snapshot.decisions.iter().enumerate() {
        head = verify_local_decision_proof(proof, trusted, &head)?;
        if &head == minimum {
            anchor_found = true;
        }
        if requests
            .insert(proof.decision.request.clone(), index)
            .is_some()
        {
            return Err(invalid(
                "duplicate request identity in local decision history",
            ));
        }
    }
    if !anchor_found {
        return Err(invalid(
            "snapshot does not contain externally trusted minimum head anchor",
        ));
    }
    Ok((head, requests))
}
