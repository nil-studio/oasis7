//! Recovery authorization is reconstructed from a fixed cut and checked signed
//! evidence. No JSON authority, boolean or local seal promotes a runtime token.
use super::*;
use oasis7_distfs::controlled_authority::formal::{
    self,
    endpoint::{
        FileFormalArchiveSourceV1, SignedFormalArchiveTransferV1, VerifiedFormalArchiveTransferV1,
        verify_formal_archive_transfer,
    },
    issuer::{IssuerAnchorV1, SignedIssuanceWitnessV1},
};
use std::sync::Arc;
#[path = "recovery_permission.rs"]
mod permission;
use permission::{PermissionView, RecoveryPermissionBootstrap, verify_permission};
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
        let source = Arc::new(
            FileFormalArchiveSourceV1::new(self.archive_directory.clone())
                .map_err(|e| e.to_string())?,
        );
        let archive = verify_formal_archive_transfer(
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
