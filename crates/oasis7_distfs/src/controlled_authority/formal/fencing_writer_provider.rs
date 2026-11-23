//! Provider-owned writer keys with one durable, exclusive software lease.
//!
//! This is CONTROLLED_ONLY storage: it proves local crash/reopen and final
//! service fencing, not physical nonexportability or privileged rollback safety.
use super::{atomic_bounded, no_symlink, private_dir, read_bounded};
use crate::controlled_authority::formal::{writer_signing::*, *};
use ed25519_dalek::{Signer, SigningKey};
use oasis7_formal_protocol::controlled_authority::formal::crypto::{self, invalid};
use serde::{Deserialize, Serialize};
use std::fs::{self, File, OpenOptions};
#[cfg(unix)]
use std::os::unix::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};

const LEDGER_DOMAIN: &str = "oasis7.controlled.writer_custody.ledger.v1";
const LEDGER_LIMIT: usize = 64 * 1024 * 1024;
const LEDGER_FILE: &str = "writer-custody.cbor";
const LOCK_FILE: &str = "writer-custody.lock";

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Binding {
    authority: AuthorityDefinitionV1,
    authority_id: String,
    activation_ref: String,
    normal_credential: String,
    recovery_credential: String,
    recovery_certificate_digest: Option<String>,
    directory_fingerprint: String,
}

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct PlannedRecovery {
    plan: SignedRecoveryPlanV1,
    pending: RecoveryPendingHandoffV1,
    fixed: RecoveryTailWitnessV1,
    fixed_material_digest: Option<String>,
    native_environment: Option<native_environment::NativeExecutionEnvironmentV1>,
    candidate: Option<SignedCanonicalCandidateV1>,
    candidate_material_digest: Option<String>,
    acknowledgement: SignedWriterRecoveryPlanAckV1,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct WriterActivation {
    candidate: SignedCanonicalCandidateV1,
    material_digest: Option<String>,
    qualified: Option<FormalQualifiedCommitProofV1>,
}

/// A signed handoff may still have an unknown endpoint outcome. Retain the
/// exact candidate, allow its completion, and never reopen fresh controlled work.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ClosingCandidate {
    record: CanonicalRecordV1,
    frozen: Option<FormalFrozenCertificateV1>,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct WriterLedger {
    schema_version: u32,
    binding: Binding,
    revision: u64,
    planned: Option<PlannedRecovery>,
    seal: Option<SignedWriterEpochSealV1>,
    drained: Option<RecoveryDrainedProofV1>,
    closing: Option<ClosingCandidate>,
    activation: Option<WriterActivation>,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct SignedWriterLedger {
    body: WriterLedger,
    signature_hex: String,
}

/// The writer secret is owned here, never returned to a caller. All service
/// commands execute through `&mut self` while the original native file lock is
/// held. The network server serializes calls on this same provider instance.
pub struct ControlledFileWriterProvider {
    _lock: File,
    path: PathBuf,
    authority: VerifiedAuthorityV1,
    recovery: Option<VerifiedRecovery>,
    key: SigningKey,
    ledger: WriterLedger,
    poisoned: bool,
    persisted: bool,
}

impl ControlledFileWriterProvider {
    /// Check the independently provisioned lower bound before publishing a
    /// listener. This never mutates or manufactures provider progress.
    pub fn require_minimum_revision(&self, minimum: u64) -> Result<(), FormalError> {
        if self.poisoned || self.ledger.revision < minimum {
            return Err(FormalError::Rollback(
                "writer custody is below its independent revision floor or requires reopen".into(),
            ));
        }
        self.check_storage()
    }

    /// Explicit provisioning in a fresh directory. Existing ownership or ledger
    /// files are never reconciled by this constructor; use `open_existing`.
    pub fn initialize(
        directory: impl AsRef<Path>,
        authority: VerifiedAuthorityV1,
        key: SigningKey,
        normal_credential: String,
        recovery_credential: String,
        recovery: Option<VerifiedRecovery>,
    ) -> Result<Self, FormalError> {
        let root = directory.as_ref();
        private_dir(root).map_err(storage)?;
        private_directory(root)?;
        if fs::read_dir(root).map_err(io)?.next().is_some() {
            return Err(FormalError::Conflict(
                "writer custody initialization requires an empty directory".into(),
            ));
        }
        let root = fs::canonicalize(root).map_err(io)?;
        let binding = checked_binding(
            &root,
            &authority,
            &key,
            normal_credential,
            recovery_credential,
            recovery.as_ref(),
        )?;
        let lock = acquire_lock(&root, true)?;
        let ledger = WriterLedger {
            schema_version: 1,
            binding,
            revision: 0,
            planned: None,
            seal: None,
            drained: None,
            closing: None,
            activation: None,
        };
        let mut provider = Self {
            _lock: lock,
            path: root.join(LEDGER_FILE),
            authority,
            recovery,
            key,
            ledger,
            poisoned: false,
            persisted: false,
        };
        provider.commit(provider.ledger.clone())?;
        Ok(provider)
    }

    /// Reopen only the original signed ledger and original lease. A missing,
    /// truncated, moved, or differently bound original is an error, never a
    /// request to initialize a new epoch.
    pub fn open_existing(
        directory: impl AsRef<Path>,
        authority: VerifiedAuthorityV1,
        key: SigningKey,
        normal_credential: String,
        recovery_credential: String,
        recovery: Option<VerifiedRecovery>,
    ) -> Result<Self, FormalError> {
        let root = directory.as_ref();
        private_directory(root)?;
        let root = fs::canonicalize(root).map_err(io)?;
        let binding = checked_binding(
            &root,
            &authority,
            &key,
            normal_credential,
            recovery_credential,
            recovery.as_ref(),
        )?;
        let lock = acquire_lock(&root, false)?;
        let path = root.join(LEDGER_FILE);
        private_regular(&path)?;
        let raw = read_bounded(&path, LEDGER_LIMIT).map_err(storage)?;
        let signed: SignedWriterLedger =
            serde_cbor::from_slice(&raw).map_err(|e| invalid(e.to_string()))?;
        crypto::verify(
            LEDGER_DOMAIN,
            &signed.body,
            &signed.signature_hex,
            &binding.authority.writer_public_key,
        )?;
        if signed.body.schema_version != 1 || signed.body.binding != binding {
            return Err(invalid(
                "writer custody original authority, credentials, recovery, or directory changed",
            ));
        }
        let provider = Self {
            _lock: lock,
            path,
            authority,
            recovery,
            key,
            ledger: signed.body,
            poisoned: false,
            persisted: true,
        };
        provider.validate_original()?;
        Ok(provider)
    }

    fn validate_original(&self) -> Result<(), FormalError> {
        self.validate_recovery_original()?;
        self.validate_activation_original()?;
        if let Some(closing) = &self.ledger.closing {
            if closing.record.handoff.is_none() {
                return Err(invalid("writer custody closing candidate has no handoff"));
            }
            self.validate_candidate(&closing.record, closing.frozen.as_ref())?;
        }
        Ok(())
    }

    fn commit(&mut self, next: WriterLedger) -> Result<(), FormalError> {
        if self.poisoned {
            return Err(invalid(
                "writer custody outcome unknown; strict reopen required",
            ));
        }
        if self.persisted
            && let Err(error) = self.check_storage()
        {
            self.poisoned = true;
            return Err(error);
        }
        let encoded = crypto::bytes(LEDGER_DOMAIN, &next)?;
        let signed = SignedWriterLedger {
            signature_hex: hex::encode(self.key.sign(&encoded).to_bytes()),
            body: next,
        };
        let bytes = serde_cbor::to_vec(&signed).map_err(|e| invalid(e.to_string()))?;
        if let Err(error) = atomic_bounded(&self.path, &bytes, LEDGER_LIMIT) {
            self.poisoned = true;
            return Err(storage(error));
        }
        self.ledger = signed.body;
        self.persisted = true;
        Ok(())
    }

    fn check_storage(&self) -> Result<(), FormalError> {
        let root = self
            .path
            .parent()
            .ok_or_else(|| invalid("writer custody original parent missing"))?;
        private_directory(root)?;
        private_regular(&self.path)?;
        let named = private_regular(&root.join(LOCK_FILE))?;
        let held = self._lock.metadata().map_err(io)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::MetadataExt;
            if named.dev() != held.dev() || named.ino() != held.ino() {
                return Err(invalid(
                    "writer custody original lease pathname was replaced",
                ));
            }
        }
        Ok(())
    }

    fn next_revision(&self) -> Result<u64, FormalError> {
        self.ledger
            .revision
            .checked_add(1)
            .ok_or_else(|| invalid("writer custody revision exhausted"))
    }

    fn snapshot(&self) -> Result<WriterCustodySnapshotV1, FormalError> {
        Ok(WriterCustodySnapshotV1 {
            writer_public_key: self.ledger.binding.authority.writer_public_key.clone(),
            authority_epoch: self.ledger.binding.authority.epoch,
            revision: self.ledger.revision,
            plan_digest: self
                .ledger
                .planned
                .as_ref()
                .map(|p| p.plan.digest())
                .transpose()?,
            drained_digest: self
                .ledger
                .drained
                .as_ref()
                .map(RecoveryDrainedProofV1::digest)
                .transpose()?,
            seal_digest: self
                .ledger
                .seal
                .as_ref()
                .map(SignedWriterEpochSealV1::digest)
                .transpose()?,
        })
    }

    fn execute(
        &mut self,
        request: &WriterCustodyRequestBodyV1,
    ) -> Result<WriterCustodyValueV1, FormalError> {
        if self.poisoned {
            return Err(FormalError::Io(
                "writer custody outcome unknown; strict reopen required".into(),
            ));
        }
        if let Err(error) = self.check_storage() {
            self.poisoned = true;
            return Err(error);
        }
        if request
            .expected_revision
            .is_some_and(|minimum| minimum > self.ledger.revision)
        {
            return Err(FormalError::Rollback(
                "writer custody is below the authenticated revision floor".into(),
            ));
        }
        let recovering = request.credential_public_key == self.ledger.binding.recovery_credential;
        match &request.command {
            WriterCustodyCommandV1::Query => Ok(WriterCustodyValueV1::State(self.snapshot()?)),
            WriterCustodyCommandV1::InstallPlan {
                plan,
                fixed,
                fixed_material,
                pending,
            } if recovering => self.install_plan(
                plan,
                fixed,
                fixed_material.as_deref(),
                pending,
                request.expected_revision,
            ),
            WriterCustodyCommandV1::DrainComplete(drained) if recovering => {
                self.complete_drain(drained, request.expected_revision)
            }
            WriterCustodyCommandV1::Frozen(body) if recovering => self.sign_original_frozen(body),
            WriterCustodyCommandV1::Candidate { record, frozen } if recovering => {
                self.sign_original_candidate(record, frozen.as_deref())
            }
            WriterCustodyCommandV1::Seal { plan_digest, cut } if recovering => {
                self.seal(plan_digest, cut, request.expected_revision)
            }
            WriterCustodyCommandV1::Frozen(body) if !recovering => {
                self.require_normal()?;
                self.require_activated()?;
                if self.ledger.closing.is_some() {
                    return Err(invalid("writer already signed a profile-closing candidate"));
                }
                self.validate_frozen(body)?;
                Ok(WriterCustodyValueV1::Signature(hex::encode(
                    self.key.sign(&frozen_signing_bytes(body)?).to_bytes(),
                )))
            }
            WriterCustodyCommandV1::Candidate { record, frozen } if !recovering => {
                self.require_normal()?;
                if matches!(record.kind, FormalRecordKindV1::RecoveryActivation { .. }) {
                    return self.sign_activation(record, frozen.as_deref());
                }
                self.require_activated()?;
                self.validate_candidate(record, frozen.as_deref())?;
                let candidate = ClosingCandidate {
                    record: *record.clone(),
                    frozen: frozen.as_deref().cloned(),
                };
                if let Some(original) = &self.ledger.closing {
                    if original != &candidate {
                        return Err(invalid(
                            "writer may only retry the exact original closing candidate",
                        ));
                    }
                } else if record.handoff.is_some() {
                    let mut next = self.ledger.clone();
                    next.revision = self.next_revision()?;
                    next.closing = Some(candidate);
                    self.commit(next)?;
                }
                Ok(WriterCustodyValueV1::Signature(hex::encode(
                    self.key
                        .sign(&canonical_candidate_signing_bytes(record)?)
                        .to_bytes(),
                )))
            }
            WriterCustodyCommandV1::Endpoint(endpoint) => {
                self.validate_endpoint(endpoint, recovering)?;
                Ok(WriterCustodyValueV1::Signature(hex::encode(
                    self.key.sign(&endpoint.signing_bytes()?).to_bytes(),
                )))
            }
            _ => Err(invalid(
                "writer credential does not authorize this custody command",
            )),
        }
    }

    fn require_normal(&self) -> Result<(), FormalError> {
        if self.ledger.planned.is_some() || self.ledger.seal.is_some() {
            return Err(invalid(
                "writer normal credential is fenced by the original recovery plan",
            ));
        }
        Ok(())
    }
}

impl WriterCustodyServiceV1 for ControlledFileWriterProvider {
    fn handle_signed(
        &mut self,
        request: SignedWriterCustodyRequestV1,
    ) -> Result<SignedWriterCustodyResponseV1, FormalError> {
        verify_writer_custody_request(&request)?;
        if request.body.authority_id != self.ledger.binding.authority_id
            || (request.body.credential_public_key != self.ledger.binding.normal_credential
                && request.body.credential_public_key != self.ledger.binding.recovery_credential)
        {
            return Err(invalid(
                "writer custody original authority/credential binding",
            ));
        }
        let result = self.execute(&request.body);
        let body = WriterCustodyResponseBodyV1 {
            schema_version: 1,
            authority_id: self.ledger.binding.authority_id.clone(),
            challenge: request.body.challenge.clone(),
            request_digest: writer_custody_request_digest(&request)?,
            revision: self.ledger.revision,
            result,
        };
        Ok(SignedWriterCustodyResponseV1 {
            signature_hex: hex::encode(
                self.key
                    .sign(&writer_custody_response_signing_bytes(&body)?)
                    .to_bytes(),
            ),
            body,
        })
    }
}

fn checked_binding(
    root: &Path,
    authority: &VerifiedAuthorityV1,
    key: &SigningKey,
    normal_credential: String,
    recovery_credential: String,
    recovery: Option<&VerifiedRecovery>,
) -> Result<Binding, FormalError> {
    let writer = hex::encode(key.verifying_key().to_bytes());
    crypto::public_key(&normal_credential)?;
    crypto::public_key(&recovery_credential)?;
    let definition = authority.definition();
    let policy = authority.policy();
    let mut reserved = vec![
        definition.writer_public_key.as_str(),
        definition.primary.instance.endpoint_public_key.as_str(),
        definition.replica.instance.endpoint_public_key.as_str(),
        policy.fencing_provider_public_key.as_str(),
        policy.recovery_issuer_public_key.as_str(),
        policy.issuer_anchor_public_key.as_str(),
    ];
    if let Some(recovery) = recovery {
        for original in [recovery.predecessor(), recovery.original_authority()] {
            reserved.extend([
                original.definition().writer_public_key.as_str(),
                original
                    .definition()
                    .primary
                    .instance
                    .endpoint_public_key
                    .as_str(),
                original
                    .definition()
                    .replica
                    .instance
                    .endpoint_public_key
                    .as_str(),
                original.policy().fencing_provider_public_key.as_str(),
                original.policy().recovery_issuer_public_key.as_str(),
                original.policy().issuer_anchor_public_key.as_str(),
            ]);
        }
    }
    if authority.definition().profile_version != 2
        || writer != authority.definition().writer_public_key
        || normal_credential == writer
        || recovery_credential == writer
        || normal_credential == recovery_credential
        || reserved.contains(&normal_credential.as_str())
        || reserved.contains(&recovery_credential.as_str())
    {
        return Err(invalid(
            "writer custody native authority or distinct credential binding",
        ));
    }
    if let Some(recovery) = recovery
        && (recovery.successor().authority_id()? != authority.authority_id()?
            || recovery.successor().activation_ref() != authority.activation_ref())
    {
        return Err(invalid(
            "writer custody recovery does not authorize this exact successor",
        ));
    }
    Ok(Binding {
        authority: authority.definition().clone(),
        authority_id: authority.authority_id()?,
        activation_ref: authority.activation_ref().into(),
        normal_credential,
        recovery_credential,
        recovery_certificate_digest: recovery.map(|r| r.certificate_digest().into()),
        directory_fingerprint: super::super::directory_fingerprint(root).map_err(storage)?,
    })
}

fn acquire_lock(root: &Path, initialize: bool) -> Result<File, FormalError> {
    let path = root.join(LOCK_FILE);
    no_symlink(&path).map_err(storage)?;
    if !initialize {
        private_regular(&path)?;
    }
    let mut options = OpenOptions::new();
    options.read(true).write(true);
    if initialize {
        options.create_new(true);
    }
    #[cfg(unix)]
    options.mode(0o600).custom_flags(libc::O_NOFOLLOW);
    let lock = options.open(path).map_err(io)?;
    if !lock.metadata().map_err(io)?.is_file() {
        return Err(invalid(
            "writer custody original lease is not a regular file",
        ));
    }
    private_metadata(&lock.metadata().map_err(io)?)?;
    lock.try_lock()
        .map_err(|_| FormalError::Conflict("writer custody original lease is held".into()))?;
    Ok(lock)
}
fn private_directory(path: &Path) -> Result<(), FormalError> {
    let metadata = fs::symlink_metadata(path).map_err(io)?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Err(invalid(
            "writer custody directory must be an original private directory",
        ));
    }
    private_metadata(&metadata)
}
fn private_regular(path: &Path) -> Result<fs::Metadata, FormalError> {
    let metadata = fs::symlink_metadata(path).map_err(io)?;
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err(invalid(
            "writer custody original must be a private regular file",
        ));
    }
    private_metadata(&metadata)?;
    Ok(metadata)
}
fn private_metadata(metadata: &fs::Metadata) -> Result<(), FormalError> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        // SAFETY: this getter has no arguments or memory-safety preconditions.
        let uid = unsafe { libc::geteuid() };
        if metadata.uid() != uid || metadata.mode() & 0o077 != 0 {
            return Err(invalid(
                "writer custody original ownership/private permissions changed",
            ));
        }
    }
    Ok(())
}
fn io(error: std::io::Error) -> FormalError {
    FormalError::Io(error.to_string())
}
fn storage(error: crate::controlled_authority::fencing::FencingError) -> FormalError {
    FormalError::Io(error.to_string())
}

#[path = "fencing_writer_provider_activation.rs"]
mod activation;
#[path = "fencing_writer_provider_checks.rs"]
mod checks;
#[path = "fencing_writer_provider_recovery.rs"]
mod recovery;
#[cfg(test)]
#[path = "fencing_writer_provider_tests.rs"]
mod tests;
