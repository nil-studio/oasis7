//! Recovery authorization is reconstructed from a fixed cut and checked signed
//! evidence. No JSON authority, boolean or local seal promotes a runtime token.
use super::*;
use oasis7_distfs::controlled_authority::formal::{
    self,
    endpoint::{
        FileFormalArchiveSourceV1, FormalArchiveChunkV1, FormalArchiveReadOwnerV1,
        FormalArchiveReadWorkV1, FormalArchiveSourceV1, SignedFormalArchiveTransferV1,
        VerifiedFormalArchiveTransferV1, verify_formal_archive_transfer,
    },
    issuer::{IssuerAnchorV1, SignedIssuanceWitnessV1},
};
use std::sync::Arc;
#[path = "recovery_permission.rs"]
mod permission;
use permission::{PermissionView, RecoveryPermissionBootstrap, verify_permission};

/// Object locations carry no authority. Prefer the new owner's hash-checked
/// originals; only a genuine local absence may open the predecessor directory.
/// In particular, a damaged local object cannot be hidden by a remote fallback.
pub(super) struct RecoveryArchiveSource {
    target: PathBuf,
    predecessor: PathBuf,
}
impl RecoveryArchiveSource {
    pub(super) fn new(target: &Path, predecessor: PathBuf) -> Self {
        Self {
            target: target.to_path_buf(),
            predecessor,
        }
    }
    fn local(&self) -> Result<Option<FileFormalArchiveSourceV1>, formal::FormalError> {
        private_data_dir(&self.target).map_err(formal::FormalError::Invalid)?;
        match std::fs::symlink_metadata(self.target.join("formal-objects-v1")) {
            Ok(_) => FileFormalArchiveSourceV1::new(self.target.clone()).map(Some),
            // A fresh registered target has no object directory until bootstrap.
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(error) => Err(formal::FormalError::Io(error.to_string())),
        }
    }
    fn read<T>(
        &self,
        mut read: impl FnMut(&FileFormalArchiveSourceV1) -> Result<Option<T>, formal::FormalError>,
    ) -> Result<Option<T>, formal::FormalError> {
        if let Some(local) = self.local()?
            && let Some(value) = read(&local)?
        {
            return Ok(Some(value));
        }
        read(&FileFormalArchiveSourceV1::new(self.predecessor.clone())?)
    }
}
impl FormalArchiveSourceV1 for RecoveryArchiveSource {
    fn read_object(&self, id: &str) -> Result<Option<Vec<u8>>, formal::FormalError> {
        self.read(|source| source.read_object(id))
    }
    fn read_range(
        &self,
        id: &str,
        offset: u64,
        limit: u32,
    ) -> Result<Option<FormalArchiveChunkV1>, formal::FormalError> {
        self.read(|source| source.read_range(id, offset, limit))
    }
    fn read_object_bounded(
        &self,
        id: &str,
        max: usize,
        memory: &oasis7_proto::memory_budget::MemoryBudget,
        work: &FormalArchiveReadWorkV1,
        cancelled: &mut dyn FnMut() -> bool,
    ) -> Result<Option<FormalArchiveReadOwnerV1>, formal::FormalError> {
        work.charge(1, 0, cancelled)?;
        if let Some(local) = self.local()?
            && let Some(owner) = local.read_object_bounded(id, max, memory, work, cancelled)?
        {
            return Ok(Some(owner));
        }
        // Keep the same paid read session, memory owner and cancellation state.
        work.charge(1, 0, cancelled)?;
        FileFormalArchiveSourceV1::new(self.predecessor.clone())?
            .read_object_bounded(id, max, memory, work, cancelled)
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct RecoveryBootstrap {
    pub predecessor_checkpoint: formal::FormalReadCheckpointV1,
    /// Deployment pin supplied independently of the checkpoint being checked.
    pub predecessor_checkpoint_digest: String,
    #[serde(default)]
    pub predecessor_context_version: Option<u32>,
    pub certificate: formal::SignedRecoveryCertificateV1,
    pub fencing: FencingBootstrap,
    pub tail: formal::RecoveryTailWitnessV1,
    pub issuer_witness: SignedIssuanceWitnessV1,
    /// Optional externally retained witness observation floor, not a local seal.
    pub minimum_issuer_anchor: IssuerAnchorV1,
    pub archive_manifest: SignedFormalArchiveTransferV1,
    pub archive_directory: PathBuf,
    /// Per-attempt local materialization bound; mandatory for new schema3.
    #[serde(default)]
    pub recovery_copy_max_bytes: Option<u64>,
    #[serde(default)]
    prior_permission: Option<Box<RecoveryPermissionBootstrap>>,
}
pub(super) use formal::FencingBootstrapV1 as FencingBootstrap;
pub(super) struct RecoveryAuthorization {
    pub authorization: formal::endpoint::FormalEndpointAuthorizationV1,
    pub archive: VerifiedFormalArchiveTransferV1,
}
impl RecoveryBootstrap {
    pub(super) fn verify(
        &mut self,
        initial_trust: &formal::InitialActivationTrustV1,
        initial_certificate: &formal::SignedInitialActivationV1,
        config_parent: &Path,
        target: &Path,
    ) -> Result<RecoveryAuthorization, String> {
        let verified = verify_permission(
            PermissionView {
                checkpoint: &self.predecessor_checkpoint,
                checkpoint_digest: &self.predecessor_checkpoint_digest,
                predecessor_context_version: self.predecessor_context_version,
                certificate: &self.certificate,
                fencing: &self.fencing,
                tail: &self.tail,
                witness: &self.issuer_witness,
                minimum: &self.minimum_issuer_anchor,
                prior: self.prior_permission.as_deref(),
            },
            initial_trust,
            initial_certificate,
        )?;
        let recovery = verified.recovery;
        if !self.archive_directory.is_absolute() {
            self.archive_directory = config_parent.join(&self.archive_directory);
        }
        let source = Arc::new(RecoveryArchiveSource::new(
            target,
            self.archive_directory.clone(),
        ));
        let mut archive = verify_formal_archive_transfer(
            &self.archive_manifest,
            &verified.active_predecessor,
            source,
        )
        .map_err(|e| {
            format!(
                "recovery archive verification source={:?}: {e}",
                self.archive_directory
            )
        })?;
        if recovery.body().schema_version == 3 {
            archive = archive
                .with_recovery_copy_budget(self.recovery_copy_max_bytes.ok_or_else(|| {
                    "schema3 recovery requires explicit recovery_copy_max_bytes".to_string()
                })?)
                .map_err(|error| error.to_string())?;
        }
        Ok(RecoveryAuthorization {
            authorization: formal::endpoint::FormalEndpointAuthorizationV1 {
                authority: recovery.successor().clone(),
                original: recovery.original_authority().clone(),
                recovery: Some(recovery),
                parent: Some(verified.cut),
            },
            archive,
        })
    }
}
