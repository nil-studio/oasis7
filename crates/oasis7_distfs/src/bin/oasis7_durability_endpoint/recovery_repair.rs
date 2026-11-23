//! The original protected provider authorizes a replacement directory. The
//! survivor supplies hash-checked originals and never supplies the lost role's
//! authority, key, monotonic floor or checkpoint signature.
use super::*;
use oasis7_distfs::controlled_authority::formal::{
    endpoint::ProtectedRecoveryRepairInputV1, issuer::IssuerAnchorV1,
};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct RecoveryRepairBootstrap {
    plan: formal::SignedRecoveryPlanV1,
    fixed: formal::RecoveryTailWitnessV1,
    writer_ack: formal::SignedWriterRecoveryPlanAckV1,
    pending_handoff_file: PathBuf,
    survivor_checkpoint_file: PathBuf,
    survivor_archive_directory: PathBuf,
    minimum_revision: u64,
    copy_max_bytes: u64,
    #[serde(default)]
    recovery_permission: Option<formal::RecoveryPermissionBootstrapV1>,
    #[serde(default)]
    minimum_issuer_anchor: Option<IssuerAnchorV1>,
}

#[cfg(unix)]
fn private_pending(path: &Path) -> Result<Vec<u8>, String> {
    use std::os::unix::fs::{MetadataExt, OpenOptionsExt};
    const LIMIT: u64 = formal::RECOVERY_PENDING_HANDOFF_ENCODED_LIMIT_V1 as u64;
    let file = std::fs::OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW)
        .open(path)
        .map_err(|error| error.to_string())?;
    let metadata = file.metadata().map_err(|error| error.to_string())?;
    // SAFETY: geteuid reads the calling process identity only.
    if !metadata.is_file()
        || metadata.uid() != unsafe { libc::geteuid() }
        || metadata.mode() & 0o077 != 0
        || metadata.len() > LIMIT
    {
        return Err("repair pending handoff must be bounded, owned and private".into());
    }
    let mut bytes = Vec::new();
    file.take(LIMIT + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| error.to_string())?;
    if bytes.len() as u64 > LIMIT {
        return Err("repair pending handoff byte bound".into());
    }
    Ok(bytes)
}

#[cfg(not(unix))]
fn private_pending(_: &Path) -> Result<Vec<u8>, String> {
    Err("protected recovery repair requires Unix private files".into())
}

impl RecoveryRepairBootstrap {
    pub(super) fn verify(
        mut self,
        initial: formal::VerifiedAuthorityV1,
        initial_trust: &formal::InitialActivationTrustV1,
        initial_certificate: &formal::SignedInitialActivationV1,
        config_parent: &Path,
        target: &Path,
    ) -> Result<ProtectedRecoveryRepairInputV1, String> {
        let authority = match (&self.recovery_permission, &self.minimum_issuer_anchor) {
            (None, None) => initial,
            (Some(permission), Some(minimum)) => permission
                .verify_at_minimum(initial_trust, initial_certificate, minimum)?
                .recovery().successor().clone(),
            _ => return Err("repair recovered authority requires its original permission and independent issuer floor".into()),
        };
        self.plan
            .verify(&authority)
            .map_err(|error| error.to_string())?;
        self.writer_ack
            .verify(&self.plan, &authority)
            .map_err(|error| error.to_string())?;
        if self.copy_max_bytes == 0 {
            return Err("repair requires a positive explicit copy_max_bytes".into());
        }
        for path in [
            &mut self.pending_handoff_file,
            &mut self.survivor_checkpoint_file,
            &mut self.survivor_archive_directory,
        ] {
            if !path.is_absolute() {
                *path = config_parent.join(&*path);
            }
        }
        let pending: formal::RecoveryPendingHandoffV1 =
            serde_cbor::from_slice(&private_pending(&self.pending_handoff_file)?)
                .map_err(|error| error.to_string())?;
        if pending.root().map_err(|error| error.to_string())? != self.plan.body.pending_handoff_root
        {
            return Err("repair original pending collection differs from signed plan".into());
        }
        Ok(ProtectedRecoveryRepairInputV1 {
            authority,
            plan: self.plan,
            fixed: self.fixed,
            pending,
            writer_ack: self.writer_ack,
            survivor_checkpoint_bytes: read(&self.survivor_checkpoint_file, 64 * 1024)?,
            source: Arc::new(recovery_bootstrap::RecoveryArchiveSource::new(
                target,
                self.survivor_archive_directory,
            )),
            minimum_revision: self.minimum_revision,
            copy_max_bytes: self.copy_max_bytes,
        })
    }
}
