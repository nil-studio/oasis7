//! Recovery cut derivation from the unchanged externally provisioned origin.
//! A page cursor or the target checkpoint digest is never a new trust pin.
use super::formal_read::{FormalReadHistoryPageV1, FormalReadVerifier};
use oasis7_distfs::controlled_authority::formal::{
    FormalReadCheckpointV1, InitialActivationTrustV1, VerifiedFormalReadAnchorV1,
};

/// Replays bounded pages independently from `origin`, retaining the verifier's
/// bounded cache. There is no total history length cap. The returned anchor is
/// exactly the requested checkpoint, including authority and finalizer context.
/// No bytes after that exact cut are used to authorize the recovery.
pub fn verify_recovery_predecessor_lineage(
    trust: &InitialActivationTrustV1,
    origin: &VerifiedFormalReadAnchorV1,
    expected: &FormalReadCheckpointV1,
    pages: impl IntoIterator<Item = Result<FormalReadHistoryPageV1, String>>,
) -> Result<VerifiedFormalReadAnchorV1, String> {
    let mut verifier = FormalReadVerifier::new(trust, origin.clone())?;
    if verifier.current_anchor().checkpoint() == *expected {
        return Ok(verifier.current_anchor().clone());
    }
    for page in pages {
        verifier.replay_history_page(&page?)?;
        let current = verifier.current_anchor();
        if current.checkpoint() == *expected {
            return Ok(current.clone());
        }
        if current.head().record().position > expected.head_record.position {
            return Err("recovery lineage overshot exact predecessor cut".into());
        }
    }
    Err("HistoryUnavailable: missing exact recovery predecessor lineage".into())
}
#[cfg(test)]
#[path = "formal_recovery_lineage_tests.rs"]
mod tests;
