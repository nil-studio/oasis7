//! Durable issuer operations over the shared checked issuance protocol.
use super::{
    FormalError, RecoveryPolicyV1, SignedRecoveryCertificateV1, VerifiedRecoveryIntent,
    recovery_certificate_digest, recovery_signing_bytes,
};
use ed25519_dalek::{Signer, SigningKey};
pub use oasis7_formal_protocol::controlled_authority::formal::issuer::*;
use serde::{Deserialize, Serialize};
use std::{
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
};
const MAX_BYTES: usize = 256 * 1024;
fn io(e: std::io::Error) -> FormalError {
    FormalError::Io(e.to_string())
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Intent {
    policy: RecoveryPolicyV1,
    certificate: SignedRecoveryCertificateV1,
}
/// The store holds one immutable intent per predecessor. Its lock prevents
/// local races; all cross-instance uniqueness comes from IssuanceWitness.
pub struct RecoveryIssuer {
    directory: PathBuf,
    policy: RecoveryPolicyV1,
    key: SigningKey,
    minimum: IssuerAnchorV1,
    _lock: File,
    poisoned: bool,
}
impl RecoveryIssuer {
    pub fn open(
        directory: &Path,
        policy: RecoveryPolicyV1,
        key: SigningKey,
        minimum: IssuerAnchorV1,
    ) -> Result<Self, FormalError> {
        if hex::encode(key.verifying_key().to_bytes()) != policy.recovery_issuer_public_key {
            return Err(invalid("recovery issuer signer mismatch"));
        }
        minimum.validate()?;
        if minimum.revision == 0 && minimum != IssuerAnchorV1::initial(&policy)? {
            return Err(FormalError::Rollback(
                "issuer initial anchor differs from policy".into(),
            ));
        }
        canonical::<32>(&policy.issuer_anchor_public_key)?;
        if policy.issuer_anchor_public_key == policy.recovery_issuer_public_key {
            return Err(invalid("issuer witness must be independently keyed"));
        }
        private_directory(directory)?;
        let lock_path = directory.join("issuer.lock");
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
            // SAFETY: owns this open file descriptor for the store lifetime.
            if unsafe { libc::flock(lock.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) } != 0 {
                return Err(FormalError::Conflict("issuer store is locked".into()));
            }
        }
        #[cfg(not(unix))]
        {
            let _ = lock;
            return Err(FormalError::IssuerUnproven);
        }
        Ok(Self {
            directory: directory.into(),
            policy,
            key,
            minimum,
            _lock: lock,
            poisoned: false,
        })
    }
    pub fn issue(
        &mut self,
        intent: &VerifiedRecoveryIntent,
        witness: &mut dyn IssuanceWitness,
    ) -> Result<
        (
            SignedRecoveryCertificateV1,
            SignedIssuanceWitnessV1,
            VerifiedIssuance,
        ),
        FormalError,
    > {
        if self.poisoned {
            return Err(FormalError::Io(
                "issuer requires reopen and original reconciliation".into(),
            ));
        }
        if intent.policy() != &self.policy {
            return Err(invalid("issuer original policy changed"));
        }
        let body = intent.body().clone();
        canonical::<32>(&body.predecessor_authority_id)?;
        let signature_hex = hex::encode(self.key.sign(&recovery_signing_bytes(&body)?).to_bytes());
        let certificate = SignedRecoveryCertificateV1 {
            body,
            signature_hex,
        };
        let path = self.directory.join(format!(
            "{}.intent.json",
            certificate.body.predecessor_authority_id
        ));
        let query = fresh_query(
            &self.policy,
            &self.key,
            certificate.body.predecessor_authority_id.clone(),
            self.minimum.clone(),
        )?;
        let existing = witness.query_predecessor(&query)?;
        existing.verify(&query, &self.policy)?;
        self.minimum = existing.body.current_anchor.clone();
        if let Some(grant) = existing.body.grant {
            // An external original grant can repair a stale or absent local
            // intent. Preserve a conflicting ungranted intent as diagnostics.
            if path.exists() {
                let old: Intent = read_bounded(&path)?;
                if old.certificate != grant.certificate {
                    let diagnostic = self.directory.join(format!(
                        "{}.orphan-intent.json",
                        recovery_certificate_digest(&old.certificate)?
                    ));
                    let bytes = serde_json::to_vec(&old).map_err(|e| invalid(e.to_string()))?;
                    if let Err(error) = atomic(&self.directory, &diagnostic, &bytes) {
                        self.poisoned = true;
                        return Err(error);
                    }
                }
            }
            let bytes = serde_json::to_vec(&Intent {
                policy: self.policy.clone(),
                certificate: grant.certificate.clone(),
            })
            .map_err(|e| invalid(e.to_string()))?;
            if let Err(error) = atomic(&self.directory, &path, &bytes) {
                self.poisoned = true;
                return Err(error);
            }
            if grant.certificate != certificate {
                return Err(FormalError::Conflict("external witness retains the original recovery certificate; query and continue that operation".into()));
            }
        }
        if path.exists() {
            let old: Intent = read_bounded(&path)?;
            if old.policy != self.policy || old.certificate != certificate {
                return Err(FormalError::Conflict(
                    "predecessor already has another immutable recovery intent".into(),
                ));
            }
        } else {
            let bytes = serde_json::to_vec(&Intent {
                policy: self.policy.clone(),
                certificate: certificate.clone(),
            })
            .map_err(|e| invalid(e.to_string()))?;
            if let Err(error) = atomic(&self.directory, &path, &bytes) {
                self.poisoned = true;
                return Err(error);
            }
        }
        let mut nonce = [0; 32];
        #[cfg(unix)]
        {
            File::open("/dev/urandom")
                .and_then(|mut f| f.read_exact(&mut nonce))
                .map_err(io)?;
        }
        #[cfg(not(unix))]
        {
            return Err(FormalError::IssuerUnproven);
        }
        let request_body = IssuanceRequestBodyV1 {
            schema_version: 1,
            issuer_public_key: self.policy.recovery_issuer_public_key.clone(),
            policy_binding: policy_binding(&self.policy)?,
            predecessor_authority_id: certificate.body.predecessor_authority_id.clone(),
            certificate_digest: recovery_certificate_digest(&certificate)?,
            observation_nonce: hex::encode(nonce),
            minimum_anchor: self.minimum.clone(),
        };
        let request = SignedIssuanceRequestV1 {
            signature_hex: hex::encode(
                self.key
                    .sign(&encoded(REQUEST_DOMAIN, &request_body)?)
                    .to_bytes(),
            ),
            body: request_body,
        };
        // Any lost reply is reconciled by the same immutable intent; a new nonce
        // is a fresh read challenge, never a new recovery operation identity.
        let receipt = witness.authorize_or_query(&request, &certificate)?;
        if receipt.body.request != request.body {
            return Err(invalid("stale or wrong issuer witness response"));
        }
        let verified = verify_issuance_witness(&receipt, &certificate, &self.policy)?;
        let result_path = self.directory.join(format!(
            "{}.result.json",
            certificate.body.predecessor_authority_id
        ));
        let bytes =
            serde_json::to_vec(&(&certificate, &receipt)).map_err(|e| invalid(e.to_string()))?;
        if let Err(error) = atomic(&self.directory, &result_path, &bytes) {
            self.poisoned = true;
            return Err(error);
        }
        self.minimum = receipt.body.current_anchor.clone();
        Ok((certificate, receipt, verified))
    }
    pub fn minimum_anchor(&self) -> &IssuerAnchorV1 {
        &self.minimum
    }
}
pub(super) fn private_options() -> OpenOptions {
    let mut options = OpenOptions::new();
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600).custom_flags(libc::O_NOFOLLOW);
    }
    options
}
pub(super) fn private_regular(path: &Path) -> Result<(), FormalError> {
    let meta = fs::symlink_metadata(path).map_err(io)?;
    if !meta.is_file() || meta.file_type().is_symlink() {
        return Err(invalid("issuer regular private file"));
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::{MetadataExt, PermissionsExt};
        // SAFETY: geteuid only reads this process identity.
        if meta.uid() != unsafe { libc::geteuid() } || meta.permissions().mode() & 0o777 != 0o600 {
            return Err(invalid("issuer private file owner/mode"));
        }
    }
    Ok(())
}
pub(super) fn private_directory(path: &Path) -> Result<(), FormalError> {
    if !path.is_absolute() {
        return Err(invalid("issuer directory must be absolute"));
    }
    for component in path.ancestors() {
        match fs::symlink_metadata(component) {
            Ok(m) if m.file_type().is_symlink() => return Err(invalid("issuer directory symlink")),
            Ok(_) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(io(e)),
        }
    }
    if !path.exists() {
        let mut builder = fs::DirBuilder::new();
        #[cfg(unix)]
        {
            use std::os::unix::fs::DirBuilderExt;
            builder.mode(0o700);
        }
        builder.create(path).map_err(io)?;
        if let Some(parent) = path.parent() {
            File::open(parent).and_then(|f| f.sync_all()).map_err(io)?;
        }
    }
    let meta = fs::symlink_metadata(path).map_err(io)?;
    if !meta.is_dir() {
        return Err(invalid("issuer directory"));
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::{MetadataExt, PermissionsExt};
        // SAFETY: geteuid only reads this process identity.
        if meta.uid() != unsafe { libc::geteuid() } || meta.permissions().mode() & 0o777 != 0o700 {
            return Err(invalid("issuer directory owner/mode"));
        }
    }
    Ok(())
}
fn read_bounded<T: serde::de::DeserializeOwned>(path: &Path) -> Result<T, FormalError> {
    private_regular(path)?;
    let mut bytes = Vec::new();
    private_options()
        .read(true)
        .open(path)
        .map_err(io)?
        .take((MAX_BYTES + 1) as u64)
        .read_to_end(&mut bytes)
        .map_err(io)?;
    if bytes.len() > MAX_BYTES {
        return Err(invalid("issuer file byte bound"));
    }
    serde_json::from_slice(&bytes).map_err(|e| invalid(e.to_string()))
}
fn atomic(directory: &Path, path: &Path, bytes: &[u8]) -> Result<(), FormalError> {
    atomic_bounded(directory, path, bytes, MAX_BYTES)
}
pub(super) fn atomic_bounded(
    directory: &Path,
    path: &Path,
    bytes: &[u8],
    max_bytes: usize,
) -> Result<(), FormalError> {
    if bytes.len() > max_bytes {
        return Err(invalid("issuer file byte bound"));
    }
    if path.exists() {
        private_regular(path)?;
    }
    let mut nonce = [0u8; 16];
    #[cfg(unix)]
    {
        File::open("/dev/urandom")
            .and_then(|mut f| f.read_exact(&mut nonce))
            .map_err(io)?;
    }
    #[cfg(not(unix))]
    {
        return Err(FormalError::IssuerUnproven);
    }
    let tmp = directory.join(format!(
        ".issuer-{}-{}.tmp",
        std::process::id(),
        hex::encode(nonce)
    ));
    let mut file = private_options()
        .write(true)
        .create_new(true)
        .open(&tmp)
        .map_err(io)?;
    let result = (|| {
        file.write_all(bytes).map_err(io)?;
        file.sync_all().map_err(io)?;
        fs::rename(&tmp, path).map_err(io)?;
        File::open(directory).and_then(|f| f.sync_all()).map_err(io)
    })();
    if result.is_err() {
        let _ = fs::remove_file(&tmp);
    }
    result
}

#[cfg(test)]
#[path = "issuer_tests.rs"]
mod tests;

fn fresh_query(
    policy: &RecoveryPolicyV1,
    key: &SigningKey,
    predecessor: String,
    minimum: IssuerAnchorV1,
) -> Result<SignedIssuerGrantQueryV1, FormalError> {
    let mut nonce = [0u8; 32];
    #[cfg(unix)]
    {
        File::open("/dev/urandom")
            .and_then(|mut f| f.read_exact(&mut nonce))
            .map_err(io)?;
    }
    #[cfg(not(unix))]
    {
        return Err(FormalError::IssuerUnproven);
    }
    SignedIssuerGrantQueryV1::new_with_nonce(policy, key, predecessor, minimum, nonce)
}
