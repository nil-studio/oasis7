//! Pure advisory recovery policy. Inputs must come from the adapter's current
//! authority checks and verified responses. This is not an admission proof.
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum DeliveryPhase {
    PreparedUnsent,
    DispatchMayHaveStarted,
    #[default]
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RecoveryIntent {
    QueryOnly,
    ContinueAuthorizedEntry,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OperationPolicy {
    QueryOnly,
    EntryFirstSend,
    PrivatePreparationReplay,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DomainDisposition {
    Received,
    Pending,
    Unknown,
    HistoryUnavailable,
    Committed,
    Rejected,
    Expired,
}

impl DomainDisposition {
    pub fn is_terminal(self) -> bool {
        matches!(self, Self::Committed | Self::Rejected | Self::Expired)
    }
}

#[derive(Debug, Clone, Copy, Deserialize)]
pub struct RecoveryInput {
    pub intent: RecoveryIntent,
    pub policy: OperationPolicy,
    #[serde(default)]
    pub phase: DeliveryPhase,
    pub disposition: DomainDisposition,
    pub proof_verified: bool,
    pub original_present: bool,
    pub scope_current: bool,
    pub read_allowed: bool,
    pub write_allowed: bool,
    pub stopped: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RecoveryAction {
    QueryOriginal,
    ContinueOriginal,
    ReadTerminal,
    Retain,
}

pub fn next_action(input: RecoveryInput) -> RecoveryAction {
    if !input.original_present || !input.scope_current {
        return RecoveryAction::Retain;
    }
    if input.proof_verified && input.disposition.is_terminal() {
        return RecoveryAction::ReadTerminal;
    }
    if input.intent == RecoveryIntent::ContinueAuthorizedEntry
        && !input.stopped
        && input.write_allowed
    {
        let allowed = match input.policy {
            OperationPolicy::QueryOnly => false,
            OperationPolicy::EntryFirstSend => input.phase == DeliveryPhase::PreparedUnsent,
            // This existing operation is idempotent by its exact signed proof;
            // it is not an intent submit or an action-recovery permission.
            OperationPolicy::PrivatePreparationReplay => true,
        };
        if allowed {
            return RecoveryAction::ContinueOriginal;
        }
    }
    if input.read_allowed {
        RecoveryAction::QueryOriginal
    } else {
        RecoveryAction::Retain
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn shared_recovery_vectors() {
        let vectors: Vec<serde_json::Value> =
            serde_json::from_str(include_str!("operation_lifecycle_vectors.json")).unwrap();
        for vector in vectors {
            let input: RecoveryInput = serde_json::from_value(vector["input"].clone()).unwrap();
            assert_eq!(
                serde_json::to_value(next_action(input)).unwrap(),
                vector["expected"],
                "{}",
                vector["name"]
            );
        }
    }
}
