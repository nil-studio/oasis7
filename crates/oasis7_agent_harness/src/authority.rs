//! Narrow application host callbacks for data that the Harness cannot verify
//! or own: Runtime leases, durable receipt readback, and continuation state.

use oasis7_agent_api::{
    CognitionError, CognitionLeaseConsumptionViewV1, ContinuousAgentRequestContextV1,
    FeedbackEnvelopeV1, ResponseArtifactIdentityV1, RuntimeReceiptLineageV1,
};

use crate::cognition::FeedbackHistoryProjection;
use crate::{ContinuationHostProjection, MemoryWritePolicyContextV1};

/// Application-side authority boundary. Associated values remain opaque to
/// the Harness; returning a correlation view does not create an admission or
/// settlement right. Implementations must perform full Runtime validation
/// before projecting any read-only DTO.
pub trait RuntimeAuthority {
    type Lease;
    type Receipt;
    type Continuation;

    /// Perform the complete Runtime-owned lease admission checks and return
    /// only its read-only API view. Harness rechecks request correlation.
    fn validate_lease(
        &self,
        lease: &Self::Lease,
        request: &ContinuousAgentRequestContextV1,
    ) -> Result<CognitionLeaseConsumptionViewV1, CognitionError>;

    /// Verify durable World readback for this committed feedback and response
    /// identity, then return the Runtime-owned lineage projection. A DTO by
    /// itself is never readback proof.
    fn verify_receipt_readback(
        &self,
        feedback: &FeedbackEnvelopeV1,
        receipt: &Self::Receipt,
        response_identity: &ResponseArtifactIdentityV1,
    ) -> Result<RuntimeReceiptLineageV1, CognitionError>;

    /// Verify an authoritative receipt for a memory commit. Runtime must
    /// establish the committed readback before this returns a lineage DTO.
    fn verify_memory_receipt(
        &self,
        receipt: &Self::Receipt,
        context: Option<&MemoryWritePolicyContextV1>,
    ) -> Result<RuntimeReceiptLineageV1, CognitionError>;

    /// Return only records whose durable outbox identity/payload was already
    /// validated by the host. Recovery imports replay metadata, never receipt
    /// or memory effects.
    fn validated_feedback_history(&self) -> Result<Vec<FeedbackHistoryProjection>, CognitionError>;

    /// Validate and project a complete Runtime continuation after its own
    /// authoritative validation (the Oasis7 adapter must call the existing
    /// `RuntimeAgentContinuation::validate_authoritative()` path). The
    /// returned internal view is not serialized or persisted and carries no
    /// authority beyond this callback boundary.
    fn continuation_projection(
        &self,
        continuation: &Self::Continuation,
    ) -> Result<ContinuationHostProjection, CognitionError>;
}

/// History-only `RuntimeAuthority` adapter for a scope-authenticated view.
///
/// Callers can hydrate validated feedback replay history through this adapter
/// without obtaining lease, receipt, memory, or continuation mutation authority.
#[derive(Debug, Clone, Default)]
pub struct ScopedFeedbackHistoryAuthority {
    history: Vec<FeedbackHistoryProjection>,
}

impl ScopedFeedbackHistoryAuthority {
    pub fn new(history: Vec<FeedbackHistoryProjection>) -> Self {
        Self { history }
    }
}

impl RuntimeAuthority for ScopedFeedbackHistoryAuthority {
    type Lease = ();
    type Receipt = ();
    type Continuation = ();

    fn validate_lease(
        &self,
        _lease: &Self::Lease,
        _request: &ContinuousAgentRequestContextV1,
    ) -> Result<CognitionLeaseConsumptionViewV1, CognitionError> {
        Err(scoped_history_only())
    }

    fn verify_receipt_readback(
        &self,
        _feedback: &FeedbackEnvelopeV1,
        _receipt: &Self::Receipt,
        _response_identity: &ResponseArtifactIdentityV1,
    ) -> Result<RuntimeReceiptLineageV1, CognitionError> {
        Err(scoped_history_only())
    }

    fn verify_memory_receipt(
        &self,
        _receipt: &Self::Receipt,
        _context: Option<&MemoryWritePolicyContextV1>,
    ) -> Result<RuntimeReceiptLineageV1, CognitionError> {
        Err(scoped_history_only())
    }

    fn validated_feedback_history(&self) -> Result<Vec<FeedbackHistoryProjection>, CognitionError> {
        Ok(self.history.clone())
    }

    fn continuation_projection(
        &self,
        _continuation: &Self::Continuation,
    ) -> Result<ContinuationHostProjection, CognitionError> {
        Err(scoped_history_only())
    }
}

fn scoped_history_only() -> CognitionError {
    CognitionError::new(
        "scoped_feedback_history_only",
        "authenticated feedback scope does not authorize Runtime mutations or projections",
    )
}

#[cfg(test)]
pub(crate) mod test_support {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};

    use oasis7_agent_api::{
        CognitionError, CognitionLeaseConsumptionViewV1, ContinuousAgentRequestContextV1,
        FeedbackEnvelopeV1, ResponseArtifactIdentityV1, RuntimeReceiptLineageV1,
    };

    use crate::cognition::FeedbackHistoryProjection;
    use crate::{ContinuationHostProjection, MemoryWritePolicyContextV1};

    use super::RuntimeAuthority;

    #[derive(Default)]
    pub(crate) struct FixtureAuthority {
        pub feedback_history: Vec<FeedbackHistoryProjection>,
        pub feedback_history_error: Option<CognitionError>,
        pub history_reads: Arc<AtomicUsize>,
    }

    impl RuntimeAuthority for FixtureAuthority {
        type Lease = ();
        type Receipt = ();
        type Continuation = ();

        fn validate_lease(
            &self,
            _lease: &Self::Lease,
            _request: &ContinuousAgentRequestContextV1,
        ) -> Result<CognitionLeaseConsumptionViewV1, CognitionError> {
            Err(CognitionError::new(
                "fixture_unused",
                "lease validation unused",
            ))
        }

        fn verify_receipt_readback(
            &self,
            _feedback: &FeedbackEnvelopeV1,
            _receipt: &Self::Receipt,
            _response_identity: &ResponseArtifactIdentityV1,
        ) -> Result<RuntimeReceiptLineageV1, CognitionError> {
            Err(CognitionError::new(
                "fixture_unused",
                "receipt readback unused",
            ))
        }

        fn verify_memory_receipt(
            &self,
            _receipt: &Self::Receipt,
            _context: Option<&MemoryWritePolicyContextV1>,
        ) -> Result<RuntimeReceiptLineageV1, CognitionError> {
            Err(CognitionError::new(
                "fixture_unused",
                "memory verification unused",
            ))
        }

        fn validated_feedback_history(
            &self,
        ) -> Result<Vec<FeedbackHistoryProjection>, CognitionError> {
            self.history_reads.fetch_add(1, Ordering::SeqCst);
            if let Some(error) = &self.feedback_history_error {
                return Err(error.clone());
            }
            Ok(self.feedback_history.clone())
        }

        fn continuation_projection(
            &self,
            _continuation: &Self::Continuation,
        ) -> Result<ContinuationHostProjection, CognitionError> {
            Err(CognitionError::new(
                "fixture_unused",
                "continuation projection unused",
            ))
        }
    }
}
