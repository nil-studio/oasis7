//! Native application entry points for Runtime-backed runner recovery and finalization.
//! The concrete authority borrows the real Runtime World; these calls preserve
//! receipt readback, exact awaiting-outcome identity, and transactional rollback.
use super::harness_authority::RuntimeWorldAuthority;
use crate::runtime::RuntimeReceiptLineageV1;
use crate::simulator::{
    AsyncAgentRunner, AsyncAgentRunnerError, FeedbackEnvelopeV1, MemoryWriteStore,
};

pub trait RuntimeRunnerHostExt {
    fn host_restore_feedback_history(
        &mut self,
        authority: &RuntimeWorldAuthority<'_>,
    ) -> Result<(), AsyncAgentRunnerError>;
    fn host_needs_feedback_recovery(&self) -> bool;
    fn host_feedback_history_loaded(&self) -> bool;
    fn host_feedback_recovery_blocked(&self, agent: &str, session: &str) -> bool;
    fn host_agent_feedback_recovery_blocked(&self, agent: &str) -> bool;
    fn host_release_turn_for_continuation<F>(
        &mut self,
        agent: &str,
        session: &str,
        turn: &str,
        request: &str,
        digest: &str,
        persist: F,
    ) -> Result<(), AsyncAgentRunnerError>
    where
        F: FnOnce(&AsyncAgentRunner) -> Result<(), String>;
    fn host_consume_feedback_from_outcome(
        &mut self,
        agent: &str,
        feedback: FeedbackEnvelopeV1,
        receipt: &RuntimeReceiptLineageV1,
        authority: &RuntimeWorldAuthority<'_>,
        store: &mut MemoryWriteStore,
    ) -> Result<(), AsyncAgentRunnerError>;
}

impl RuntimeRunnerHostExt for AsyncAgentRunner {
    fn host_restore_feedback_history(
        &mut self,
        authority: &RuntimeWorldAuthority<'_>,
    ) -> Result<(), AsyncAgentRunnerError> {
        self.restore_host_validated_feedback_history(authority)
    }
    fn host_needs_feedback_recovery(&self) -> bool {
        self.needs_runtime_feedback_recovery()
    }
    fn host_feedback_history_loaded(&self) -> bool {
        self.runtime_feedback_history_loaded()
    }
    fn host_feedback_recovery_blocked(&self, agent: &str, session: &str) -> bool {
        self.feedback_recovery_blocked(agent, session)
    }
    fn host_agent_feedback_recovery_blocked(&self, agent: &str) -> bool {
        self.agent_feedback_recovery_blocked(agent)
    }
    fn host_release_turn_for_continuation<F>(
        &mut self,
        agent: &str,
        session: &str,
        turn: &str,
        request: &str,
        digest: &str,
        persist: F,
    ) -> Result<(), AsyncAgentRunnerError>
    where
        F: FnOnce(&AsyncAgentRunner) -> Result<(), String>,
    {
        self.with_runtime_turn_release_for_continuation(
            agent, session, turn, request, digest, persist,
        )
    }
    fn host_consume_feedback_from_outcome(
        &mut self,
        agent: &str,
        feedback: FeedbackEnvelopeV1,
        receipt: &RuntimeReceiptLineageV1,
        authority: &RuntimeWorldAuthority<'_>,
        store: &mut MemoryWriteStore,
    ) -> Result<(), AsyncAgentRunnerError> {
        self.consume_runtime_feedback_with_world_readback_from_outcome(
            agent, feedback, receipt, authority, store,
        )
    }
}
