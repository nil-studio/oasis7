//! The received package reaches pure Runtime admission before any roll action.
use super::{canonical_digest, error};
use crate::{
    NodeBftExecutionContextV3, NodeBftPrivateCandidatePackageV3,
    NodeBftProtectedCandidateAdmissionV3, NodeError,
};
use std::time::Instant;

#[derive(Clone, Copy)]
pub(super) struct Original<'a> {
    pub context: &'a NodeBftExecutionContextV3,
    pub package: &'a NodeBftPrivateCandidatePackageV3,
    pub deadline: Instant,
}

pub(super) trait Host {
    fn retain_original(
        &mut self,
        original: Original<'_>,
    ) -> Result<NodeBftProtectedCandidateAdmissionV3, NodeError>;
    fn roll_original(&mut self, original: Original<'_>) -> Result<(), NodeError>;
    fn refresh_original(&mut self, original: Original<'_>) -> Result<(), NodeError>;
}

pub(super) fn retain(host: &mut impl Host, original: Original<'_>) -> Result<bool, NodeError> {
    let digest = match host.retain_original(original)? {
        NodeBftProtectedCandidateAdmissionV3::Retained(digest) => digest,
        NodeBftProtectedCandidateAdmissionV3::Rejected(_) => return Ok(false),
        NodeBftProtectedCandidateAdmissionV3::NeedsRoll => {
            host.roll_original(original)?;
            match host.retain_original(original)? {
                NodeBftProtectedCandidateAdmissionV3::Retained(digest) => digest,
                NodeBftProtectedCandidateAdmissionV3::Rejected(_)
                | NodeBftProtectedCandidateAdmissionV3::NeedsRoll => {
                    return Err(error("wire-3 original received package blocked after roll"));
                }
            }
        }
    };
    if !canonical_digest(&digest) {
        return Err(error(
            "wire-3 received protected retention receipt malformed",
        ));
    }
    host.refresh_original(original)?;
    Ok(true)
}

#[cfg(test)]
#[path = "node_bft_v3_received_admission_tests.rs"]
mod tests;
