//! Local control outcomes are carried separately from provider diagnostics.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) enum LocalBlocker {
    AttemptExpired,
    BudgetExhausted,
    #[default]
    RecoveryRequired,
}
impl LocalBlocker {
    pub(super) fn as_str(self) -> &'static str {
        match self {
            Self::AttemptExpired => "attempt_expired",
            Self::BudgetExhausted => "operation_budget_exhausted",
            Self::RecoveryRequired => "operation_recovery_required",
        }
    }
}

#[derive(Debug)]
pub(super) struct OperationFailure {
    pub(super) blocker: LocalBlocker,
    pub(super) message: String,
    pub(super) retry_installation: bool,
}
impl OperationFailure {
    pub(super) fn new(blocker: LocalBlocker, message: impl Into<String>) -> Self {
        Self {
            blocker,
            message: message.into(),
            retry_installation: false,
        }
    }
    pub(super) fn repair_attempt(mut self) -> Self {
        self.retry_installation = true;
        self
    }
}
impl From<String> for OperationFailure {
    fn from(message: String) -> Self {
        Self::new(LocalBlocker::RecoveryRequired, message)
    }
}
impl From<&str> for OperationFailure {
    fn from(message: &str) -> Self {
        message.to_owned().into()
    }
}
impl From<OperationFailure> for String {
    fn from(error: OperationFailure) -> Self {
        error.message
    }
}
impl std::fmt::Display for OperationFailure {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.message)
    }
}
impl std::error::Error for OperationFailure {}
