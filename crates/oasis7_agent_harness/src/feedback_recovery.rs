//! World-agnostic finality binding and preverified feedback replay recovery primitives.

use oasis7_agent_api::{CognitionError, Digest32, FeedbackEnvelopeV1, h_v1};
use serde::{Deserialize, Serialize};

use crate::authority::ScopedFeedbackHistoryAuthority;
use crate::cognition::{AgentCognitionStore, FeedbackHistoryLineage, FeedbackHistoryProjection};

fn default_schema_version() -> u16 {
    1
}

/// Shared branch/finality binding projection. Runtime is authoritative for its
/// values; the Harness only carries, validates, and hashes the verified projection.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FinalityBindingV1 {
    #[serde(default = "default_schema_version")]
    pub schema_version: u16,
    pub branch_id: String,
    pub finality_epoch: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub finality_block_hash: Option<Digest32>,
    pub finality_status: String,
    pub reorg_epoch: u64,
}

impl FinalityBindingV1 {
    /// Validate the Runtime-owned branch/finality projection. A block hash is
    /// optional for non-verified states, but every supplied hash must use the
    /// shared typed BLAKE3-256 rendering.
    pub fn validate(&self) -> Result<(), CognitionError> {
        if self.branch_id.trim().is_empty() || self.branch_id.len() > 128 {
            return Err(CognitionError::new(
                "recovery_pending",
                "finality binding branch identity is invalid",
            ));
        }
        if !matches!(
            self.finality_status.as_str(),
            "pending" | "verified" | "reorged" | "suspended"
        ) {
            return Err(CognitionError::new(
                "recovery_pending",
                "finality binding status is not in the v1 registry",
            ));
        }
        match self.finality_block_hash.as_ref() {
            Some(hash) if !hash.is_canonical_blake3() => Err(CognitionError::new(
                "recovery_pending",
                "finality block hash is not a canonical BLAKE3-256 digest",
            )),
            None if self.finality_status == "verified" => Err(CognitionError::new(
                "recovery_pending",
                "verified finality requires a block hash",
            )),
            _ => Ok(()),
        }
    }

    pub fn digest(&self) -> Digest32 {
        h_v1("oasis7.runtime.finality-binding.v1", self)
    }
}

/// Convert preverified scoped `(FeedbackEnvelopeV1, original_digest, delivery_state)`
/// tuples into `FeedbackHistoryProjection` records suitable for `AgentCognitionStore`.
pub fn project_preverified_scoped_feedback_history(
    records: &[(FeedbackEnvelopeV1, String, String)],
) -> Result<Vec<FeedbackHistoryProjection>, CognitionError> {
    let mut projected = Vec::with_capacity(records.len());
    for (feedback, original_digest, state) in records {
        feedback.validate()?;
        if !Digest32::from(original_digest.as_str()).is_canonical_blake3()
            || !matches!(state.as_str(), "pending" | "in_flight" | "acked")
        {
            return Err(CognitionError::new(
                "feedback_recovery_record_invalid",
                "invalid scoped replay metadata",
            ));
        }
        projected.push(FeedbackHistoryProjection {
            feedback: feedback.clone(),
            acknowledged: state == "acked",
            lineage: Some(FeedbackHistoryLineage {
                original_envelope_digest: original_digest.clone(),
                delivery_state: state.clone(),
                receipt_lineage: None,
            }),
        });
    }
    Ok(projected)
}

impl AgentCognitionStore {
    /// Hydrate replay/collision metadata from an authenticated scoped view whose
    /// durable records and receipt lineage were already validated by the host.
    pub fn restore_preverified_scoped_feedback_history(
        &mut self,
        records: &[(FeedbackEnvelopeV1, String, String)],
    ) -> Result<(), CognitionError> {
        let mut projections = Vec::with_capacity(records.len());
        for (feedback, original_digest, state) in records {
            feedback.validate()?;
            if !Digest32::from(original_digest.as_str()).is_canonical_blake3()
                || !matches!(state.as_str(), "pending" | "in_flight" | "acked")
            {
                return Err(CognitionError::new(
                    "feedback_recovery_record_invalid",
                    "invalid scoped replay metadata",
                ));
            }
            projections.push(FeedbackHistoryProjection {
                feedback: feedback.clone(),
                acknowledged: state == "acked",
                lineage: None,
            });
        }
        let authority = ScopedFeedbackHistoryAuthority::new(projections);
        self.restore_from_runtime(&authority)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finality_binding_validates_status_and_canonical_block_hash() {
        let valid = FinalityBindingV1 {
            schema_version: 1,
            branch_id: "main".into(),
            finality_epoch: 3,
            finality_block_hash: Some(Digest32::from(
                "blake3:1111111111111111111111111111111111111111111111111111111111111111",
            )),
            finality_status: "verified".into(),
            reorg_epoch: 0,
        };
        valid.validate().expect("valid finality binding");
        assert!(valid.digest().is_canonical_blake3());

        let missing_hash = FinalityBindingV1 {
            finality_block_hash: None,
            ..valid.clone()
        };
        assert!(missing_hash.validate().is_err());
    }

    #[test]
    fn preverified_scoped_feedback_history_restores_into_store() {
        let feedback = FeedbackEnvelopeV1 {
            feedback_id: "fb-1".into(),
            feedback_seq: 1,
            agent_subject: "agent-1".into(),
            agent_session_id: "session-1".into(),
            agent_turn_id: "turn-1".into(),
            decision_request_id: "req-1".into(),
            candidate_action_id: Some(7),
            runtime_receipt_id: Some("rcpt-1".into()),
            status: "committed".into(),
            request_digest: Digest32::from(
                "blake3:2222222222222222222222222222222222222222222222222222222222222222",
            ),
            reject_reason: None,
            provenance: "runtime_authoritative".into(),
        };
        let digest =
            "blake3:3333333333333333333333333333333333333333333333333333333333333333".to_string();
        let mut store = AgentCognitionStore::default();
        store
            .restore_preverified_scoped_feedback_history(&[(feedback, digest, "acked".to_string())])
            .expect("restore preverified scoped feedback");
    }
}
