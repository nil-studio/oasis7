//! Trusted host conversions and receipt-backed memory projection.
pub use crate::agent_execution::agent_api_compat::{feedback_to_api, request_context_to_api};
pub use crate::agent_execution::{CommittedFeedbackMemoryInput, project_receipt_memory};
pub use crate::agent_execution::world_service_feedback_host::{check_fresh_session, restore_preverified};

/// Commit memory only after the Runner validates the actual host receipt.
pub fn with_committed_feedback_memory_transaction<A, F>(
    runner: &mut crate::agent_execution::AsyncAgentRunner,
    input: CommittedFeedbackMemoryInput<'_>,
    authority_receipt: &A::Receipt,
    authority: &A,
    memory: &mut crate::agent_execution::MemoryWriteStore,
    persist: F,
) -> Result<(), crate::agent_execution::AsyncAgentRunnerError>
where
    A: oasis7_agent_harness::RuntimeAuthority,
    F: FnOnce(&mut crate::agent_execution::MemoryWriteStore) -> Result<(), String>,
{
    runner.with_committed_feedback_memory_transaction(
        input,
        authority_receipt,
        authority,
        memory,
        persist,
    )
}
