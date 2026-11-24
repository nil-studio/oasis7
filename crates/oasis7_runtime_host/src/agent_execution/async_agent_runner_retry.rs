//! Inactive L4 Host retry-lineage guard for an already awaiting Runtime turn.
//!
//! This is a pure control state: it cannot dispatch a provider call, re-admit
//! an action, mint a receipt, or update the authoritative World. The live
//! legacy runner remains unchanged until the full actor/cold-resume cutover.

use oasis7_agent_api::{ContinuousAgentRequestContextV1, Digest32};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RetryLineageError {
    InvalidProductionContext,
    InvalidIdentity,
    ProviderStillInFlight,
    AwaitingIdentityMismatch,
    TransportAttemptMismatch,
    TransportAttemptOverflow,
}

/// A snapshot of the *accepted* logical request, not a new request identity.
/// Only transport_attempt is allowed to advance on an explicit provider retry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AwaitingRetryLineage {
    pub agent_subject: String,
    pub agent_turn_id: String,
    pub decision_request_id: String,
    pub request_digest: Digest32,
    pub retry_seq: u64,
    pub transport_attempt: u64,
}

impl AwaitingRetryLineage {
    /// Construct only from the original accepted production request.
    pub fn from_production_request(
        request: &ContinuousAgentRequestContextV1,
    ) -> Result<Self, RetryLineageError> {
        request
            .validate_production_lane()
            .map_err(|_| RetryLineageError::InvalidProductionContext)?;
        Ok(Self {
            agent_subject: request.agent_subject.clone(),
            agent_turn_id: request.agent_turn_id.clone(),
            decision_request_id: request.decision_request_id.clone(),
            request_digest: request.request_digest.clone(),
            retry_seq: request.retry_seq,
            transport_attempt: request.transport_attempt,
        })
    }

    /// Admit the next wire attempt against the exact awaiting snapshot.
    ///
    /// This does not reset the logical request budget, alter retry_seq,
    /// consume a cognition lease, or remove awaiting Runtime correlation.
    /// Callers must perform the side-effecting provider dispatch separately.
    pub fn next_transport_attempt(
        &self,
        supplied: &Self,
        provider_still_in_flight: bool,
    ) -> Result<Self, RetryLineageError> {
        if self.agent_subject.is_empty()
            || self.agent_turn_id.is_empty()
            || self.decision_request_id.is_empty()
            || !self.request_digest.is_canonical_blake3()
            || self.retry_seq == 0
            || self.transport_attempt == 0
        {
            return Err(RetryLineageError::InvalidIdentity);
        }
        if provider_still_in_flight {
            return Err(RetryLineageError::ProviderStillInFlight);
        }
        if self.agent_subject != supplied.agent_subject
            || self.agent_turn_id != supplied.agent_turn_id
            || self.decision_request_id != supplied.decision_request_id
            || self.request_digest != supplied.request_digest
            || self.retry_seq != supplied.retry_seq
        {
            return Err(RetryLineageError::AwaitingIdentityMismatch);
        }
        if self.transport_attempt != supplied.transport_attempt {
            return Err(RetryLineageError::TransportAttemptMismatch);
        }
        let transport_attempt = self
            .transport_attempt
            .checked_add(1)
            .ok_or(RetryLineageError::TransportAttemptOverflow)?;
        Ok(Self {
            transport_attempt,
            ..self.clone()
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn awaiting() -> AwaitingRetryLineage {
        AwaitingRetryLineage {
            agent_subject: "actor-a".into(),
            agent_turn_id: "turn-a".into(),
            decision_request_id: "request-a".into(),
            request_digest: Digest32(format!("blake3:{}", "a".repeat(64))),
            retry_seq: 1,
            transport_attempt: 1,
        }
    }

    #[test]
    fn retry_only_advances_transport_attempt_and_keeps_logical_identity() {
        let accepted = awaiting();
        let next = accepted.next_transport_attempt(&accepted, false).unwrap();
        assert_eq!(next.transport_attempt, 2);
        assert_eq!(next.retry_seq, accepted.retry_seq);
        assert_eq!(next.request_digest, accepted.request_digest);
        assert_eq!(next.agent_subject, accepted.agent_subject);
        assert_eq!(next.agent_turn_id, accepted.agent_turn_id);
        assert_eq!(next.decision_request_id, accepted.decision_request_id);
        assert_eq!(
            next.next_transport_attempt(&next, false)
                .unwrap()
                .transport_attempt,
            3
        );
    }

    #[test]
    fn retry_rejects_cross_actor_turn_request_digest_and_retry_seq() {
        let accepted = awaiting();
        let mut forged = accepted.clone();
        forged.agent_subject = "actor-b".into();
        assert_eq!(
            accepted.next_transport_attempt(&forged, false),
            Err(RetryLineageError::AwaitingIdentityMismatch)
        );
        forged = accepted.clone();
        forged.agent_turn_id = "turn-b".into();
        assert_eq!(
            accepted.next_transport_attempt(&forged, false),
            Err(RetryLineageError::AwaitingIdentityMismatch)
        );
        forged = accepted.clone();
        forged.decision_request_id = "request-b".into();
        assert_eq!(
            accepted.next_transport_attempt(&forged, false),
            Err(RetryLineageError::AwaitingIdentityMismatch)
        );
        forged = accepted.clone();
        forged.request_digest = Digest32(format!("blake3:{}", "b".repeat(64)));
        assert_eq!(
            accepted.next_transport_attempt(&forged, false),
            Err(RetryLineageError::AwaitingIdentityMismatch)
        );
        forged = accepted.clone();
        forged.retry_seq = 2;
        assert_eq!(
            accepted.next_transport_attempt(&forged, false),
            Err(RetryLineageError::AwaitingIdentityMismatch)
        );
    }

    #[test]
    fn retry_rejects_stale_attempt_and_concurrent_provider_dispatch() {
        let accepted = awaiting();
        assert_eq!(
            accepted.next_transport_attempt(&accepted, true),
            Err(RetryLineageError::ProviderStillInFlight)
        );
        let mut stale = accepted.clone();
        stale.transport_attempt += 1;
        assert_eq!(
            accepted.next_transport_attempt(&stale, false),
            Err(RetryLineageError::TransportAttemptMismatch)
        );
    }

    #[test]
    fn retry_rejects_invalid_identity_and_transport_overflow() {
        let mut accepted = awaiting();
        accepted.retry_seq = 0;
        assert_eq!(
            accepted.next_transport_attempt(&accepted, false),
            Err(RetryLineageError::InvalidIdentity)
        );
        accepted = awaiting();
        accepted.request_digest = Digest32("not-canonical".into());
        assert_eq!(
            accepted.next_transport_attempt(&accepted, false),
            Err(RetryLineageError::InvalidIdentity)
        );
        accepted = awaiting();
        accepted.transport_attempt = u64::MAX;
        assert_eq!(
            accepted.next_transport_attempt(&accepted, false),
            Err(RetryLineageError::TransportAttemptOverflow)
        );
    }
}
