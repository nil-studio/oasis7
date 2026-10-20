//! Read-only recovery permission loading. All authorities are derived from the
//! shared signature/fencing/issuer verifier, never operator JSON definitions.
#[path = "formal_live_recovery_pages.rs"]
mod pages;
use super::*;
use oasis7_distfs::controlled_authority::formal::{
    RecoveryPermissionBootstrapV1, VerifiedRecoveryBootstrapV1,
};
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct FormalRecoveryIssuerMinimumFileV1 {
    pub schema_version: u32,
    pub world: oasis7_distfs::controlled_authority::formal::FormalWorldIdentityV1,
    pub full_policy_digest: String,
    pub recovery_issuer_public_key: String,
    pub issuer_anchor_public_key: String,
    pub anchor: oasis7_distfs::controlled_authority::formal::issuer::IssuerAnchorV1,
}
impl FormalRecoveryIssuerMinimumFileV1 {
    pub(super) fn verify(&self, external: &VerifiedFormalReadAnchorV1) -> Result<(), String> {
        let authority = external.authority();
        if self.schema_version != 1
            || self.world != authority.definition().world
            || self.full_policy_digest != authority.policy().digest().map_err(|e| e.to_string())?
            || self.recovery_issuer_public_key != authority.policy().recovery_issuer_public_key
            || self.issuer_anchor_public_key != authority.policy().issuer_anchor_public_key
        {
            return Err("ISSUER_UNPROVEN: independent issuer minimum namespace mismatch".into());
        }
        Ok(())
    }
}
pub(super) fn load_recovery_permission(
    path: &std::path::Path,
    issuer_minimum_path: &std::path::Path,
    origin_history_directory: Option<&std::path::Path>,
    trust: &FormalReadTrustConfigV1,
    external: &VerifiedFormalReadAnchorV1,
) -> Result<VerifiedRecoveryBootstrapV1, String> {
    super::super::controlled_live_config::require_private_owned(path, false)?;
    let bytes = super::super::controlled_history_cli::read_bounded(path, 4 * 1024 * 1024)?;
    let raw: RecoveryPermissionBootstrapV1 = serde_json::from_slice(&bytes)
        .map_err(|error| format!("formal recovery permission: {error}"))?;
    super::super::controlled_live_config::require_private_owned(issuer_minimum_path, false)?;
    let bytes = super::super::controlled_history_cli::read_bounded(issuer_minimum_path, 16 * 1024)?;
    let minimum: FormalRecoveryIssuerMinimumFileV1 = serde_json::from_slice(&bytes)
        .map_err(|e| format!("formal recovery independent issuer minimum: {e}"))?;
    minimum.verify(external)?;
    let verified = raw.verify_at_minimum(
        &trust.initial_trust,
        &trust.checkpoint.initial_certificate,
        &minimum.anchor,
    )?;
    // Only the earliest ordinary cut needs a public qualification lineage.
    // Every later same-cut retained active context is derived by the checked
    // bootstrap chain, never published by the ordinary reader before completion.
    let mut earliest = &raw;
    while let Some(previous) = earliest.prior_permission.as_deref() {
        earliest = previous;
    }
    let expected = earliest.predecessor_checkpoint.clone();
    let derived = if expected == external.checkpoint() {
        external.clone()
    } else {
        let directory = origin_history_directory.ok_or(
            "HistoryUnavailable: recovery cut requires independently replayed origin history",
        )?;
        oasis7::world_service::formal_recovery_lineage::verify_recovery_predecessor_lineage(
            &trust.initial_trust,
            external,
            &expected,
            pages::RecoveryPages::open(directory)?,
        )?
    };
    if derived.checkpoint() != expected
        || verified.cut().head() != derived.head().head()
        || verified.cut().record() != derived.head().record()
        || verified.external_anchor().checkpoint() != raw.predecessor_checkpoint
    {
        return Err(
            "formal recovery permission differs from independently replayed predecessor".into(),
        );
    }
    Ok(verified)
}
