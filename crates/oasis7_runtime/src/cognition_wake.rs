//! Runtime continuation lifecycle policy.
pub use oasis7_world_state::cognition_wake::*;
use serde::Serialize;
pub struct ContinuationTransition;

impl ContinuationTransition {
    pub fn validate(
        from: ContinuationStatusV1,
        to: ContinuationStatusV1,
    ) -> Result<(), WakeConditionError> {
        let allowed = match from {
            ContinuationStatusV1::Scheduled => matches!(
                to,
                ContinuationStatusV1::Pending
                    | ContinuationStatusV1::Waking
                    | ContinuationStatusV1::Cancelled
                    | ContinuationStatusV1::Invalidated
                    | ContinuationStatusV1::Expired
                    | ContinuationStatusV1::Rejected
            ),
            ContinuationStatusV1::Pending => matches!(
                to,
                ContinuationStatusV1::Scheduled
                    | ContinuationStatusV1::Waking
                    | ContinuationStatusV1::Cancelled
                    | ContinuationStatusV1::Invalidated
                    | ContinuationStatusV1::Expired
                    | ContinuationStatusV1::Rejected
            ),
            ContinuationStatusV1::Waking => matches!(
                to,
                ContinuationStatusV1::Consumed
                    | ContinuationStatusV1::Cancelled
                    | ContinuationStatusV1::Invalidated
                    | ContinuationStatusV1::Expired
                    | ContinuationStatusV1::Rejected
            ),
            ContinuationStatusV1::Consumed => matches!(
                to,
                ContinuationStatusV1::Scheduled
                    | ContinuationStatusV1::Completed
                    | ContinuationStatusV1::Cancelled
                    | ContinuationStatusV1::Invalidated
                    | ContinuationStatusV1::Expired
                    | ContinuationStatusV1::Rejected
            ),
            ContinuationStatusV1::Completed
            | ContinuationStatusV1::Cancelled
            | ContinuationStatusV1::Invalidated
            | ContinuationStatusV1::Expired
            | ContinuationStatusV1::Rejected => false,
        };
        if allowed {
            Ok(())
        } else {
            Err(WakeConditionError::new("recovery_pending"))
        }
    }

    pub fn apply(
        continuation: &mut AgentContinuation,
        to: ContinuationStatusV1,
    ) -> Result<(), WakeConditionError> {
        Self::validate(continuation.status, to)?;
        continuation.status = to;
        if !matches!(
            to,
            ContinuationStatusV1::Scheduled
                | ContinuationStatusV1::Pending
                | ContinuationStatusV1::Waking
                | ContinuationStatusV1::Consumed
        ) {
            continuation.terminal_disposition = Some(
                match to {
                    ContinuationStatusV1::Cancelled => "cancelled",
                    ContinuationStatusV1::Invalidated => "reorg_invalidated",
                    ContinuationStatusV1::Expired => "expired",
                    ContinuationStatusV1::Rejected => "rejected",
                    ContinuationStatusV1::Completed => "completed",
                    _ => "",
                }
                .to_string(),
            );
        }
        continuation.refresh_status_digest();
        Ok(())
    }

    pub fn apply_at_tick(
        continuation: &mut AgentContinuation,
        to: ContinuationStatusV1,
        logical_tick: u64,
    ) -> Result<(), WakeConditionError> {
        Self::validate(continuation.status, to)?;
        continuation.logical_tick = logical_tick;
        continuation.status = to;
        if !matches!(
            to,
            ContinuationStatusV1::Scheduled
                | ContinuationStatusV1::Pending
                | ContinuationStatusV1::Waking
                | ContinuationStatusV1::Consumed
        ) {
            continuation.terminal_disposition = Some(
                match to {
                    ContinuationStatusV1::Cancelled => "cancelled",
                    ContinuationStatusV1::Invalidated => "reorg_invalidated",
                    ContinuationStatusV1::Expired => "expired",
                    ContinuationStatusV1::Rejected => "rejected",
                    ContinuationStatusV1::Completed => "completed",
                    _ => "",
                }
                .to_string(),
            );
        }
        continuation.refresh_status_digest();
        Ok(())
    }

    pub fn invalidate_for_reorg(
        continuation: &mut AgentContinuation,
        reorg_epoch: u64,
    ) -> Result<ContinuationReorgReport, WakeConditionError> {
        Self::validate(continuation.status, ContinuationStatusV1::Invalidated)?;
        continuation.reorg_epoch = reorg_epoch;
        continuation.status = ContinuationStatusV1::Invalidated;
        continuation.terminal_disposition = Some("reorg_invalidated".to_string());
        continuation.refresh_status_digest();
        Ok(ContinuationReorgReport {
            terminal_disposition: "reorg_invalidated".to_string(),
            provider_invocation_count: 0,
            effect_count: 0,
            receipt_count: 0,
        })
    }

    pub fn invalidate_for_reorg_at_tick(
        continuation: &mut AgentContinuation,
        reorg_epoch: u64,
        logical_tick: u64,
    ) -> Result<ContinuationReorgReport, WakeConditionError> {
        Self::validate(continuation.status, ContinuationStatusV1::Invalidated)?;
        continuation.reorg_epoch = reorg_epoch;
        continuation.logical_tick = logical_tick;
        continuation.status = ContinuationStatusV1::Invalidated;
        continuation.terminal_disposition = Some("reorg_invalidated".to_string());
        continuation.refresh_status_digest();
        Ok(ContinuationReorgReport {
            terminal_disposition: "reorg_invalidated".to_string(),
            provider_invocation_count: 0,
            effect_count: 0,
            receipt_count: 0,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ContinuationReorgReport {
    pub terminal_disposition: String,
    pub provider_invocation_count: u64,
    pub effect_count: u64,
    pub receipt_count: u64,
}
