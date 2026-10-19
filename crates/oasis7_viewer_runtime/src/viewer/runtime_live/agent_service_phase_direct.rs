//! Bounded synchronous driver for the exact durable hosted-service checkpoint.
use super::super::agent_service_io::AgentServiceProgress;
use super::super::control_plane::llm_sidecar::RuntimeProviderActionContext;
use super::super::control_plane::llm_sidecar::lineage_persistence::PendingProviderServiceIntent;
use super::super::control_plane::provider_action_commit::{
    ProviderRuntimeActionCommitError, ProviderServiceCognitionReceipt,
};
use super::HostedServicePhase;
use crate::viewer::ViewerRuntimeLiveServer;

impl HostedServicePhase {
    #[cfg(any(test, feature = "test_tier_required"))]
    pub(in crate::viewer::runtime_live) fn test_phase_label(&self) -> &'static str {
        match self {
            Self::Act { .. } => "act",
            Self::ActView { settled: false, .. } => "act_view",
            Self::ActView { settled: true, .. } => "act_settled_view",
            Self::ActSettle { .. } => "act_settle",
            Self::HandoffWake { .. } => "handoff_wake",
            Self::HandoffView { .. } => "handoff_view",
            Self::Feedback { .. } => "feedback",
            Self::FeedbackAck { .. } => "feedback_ack",
            Self::FeedbackAckView { .. } => "feedback_ack_view",
            Self::Finalize { .. } => "finalize",
        }
    }

    pub(in crate::viewer::runtime_live) fn identity(&self) -> Result<String, String> {
        let (label, pending, receipt, details) = match self {
            Self::Act { pending, .. } => ("act", pending, None, serde_json::Value::Null),
            Self::ActView {
                pending,
                receipt,
                commit,
                settled,
            } => (
                if *settled {
                    "act_settled_view"
                } else {
                    "act_view"
                },
                pending,
                Some(receipt),
                serde_json::json!({"commit": commit, "settled": settled}),
            ),
            Self::ActSettle {
                pending,
                receipt,
                replay_authorized,
                ..
            } => (
                "act_settle",
                pending,
                Some(receipt),
                serde_json::json!({"replay_authorized": replay_authorized}),
            ),
            Self::HandoffWake {
                pending,
                receipt,
                target_wake,
                replay_authorized,
                ..
            } => (
                "handoff_wake",
                pending,
                Some(receipt),
                serde_json::json!({
                    "target_wake": target_wake,
                    "replay_authorized": replay_authorized
                }),
            ),
            Self::HandoffView {
                pending,
                receipt,
                target_wake,
                commit,
            } => (
                "handoff_view",
                pending,
                Some(receipt),
                serde_json::json!({"target_wake": target_wake, "commit": commit}),
            ),
            Self::Feedback { pending, receipt } => {
                ("feedback", pending, Some(receipt), serde_json::Value::Null)
            }
            Self::FeedbackAck {
                pending, receipt, ..
            } => (
                "feedback_ack",
                pending,
                Some(receipt),
                serde_json::Value::Null,
            ),
            Self::FeedbackAckView {
                pending,
                receipt,
                commit,
            } => (
                "feedback_ack_view",
                pending,
                Some(receipt),
                serde_json::json!({"commit": commit}),
            ),
            Self::Finalize {
                pending,
                receipt,
                feedback_acked,
            } => (
                "finalize",
                pending,
                Some(receipt),
                serde_json::json!({"feedback_acked": feedback_acked}),
            ),
        };
        let material = serde_json::to_vec(&(label, pending, receipt, details))
            .map_err(|error| format!("hosted service phase identity encoding failed: {error}"))?;
        Ok(format!(
            "{label}:{}:{}",
            pending
                .cognition
                .request
                .request_context
                .provider_invocation_key(),
            blake3::hash(&material).to_hex()
        ))
    }

    fn pending(&self) -> &PendingProviderServiceIntent {
        match self {
            Self::Act { pending, .. }
            | Self::ActView { pending, .. }
            | Self::ActSettle { pending, .. }
            | Self::HandoffWake { pending, .. }
            | Self::HandoffView { pending, .. }
            | Self::Feedback { pending, .. }
            | Self::FeedbackAck { pending, .. }
            | Self::FeedbackAckView { pending, .. }
            | Self::Finalize { pending, .. } => pending,
        }
    }

    fn may_submit_original(&self) -> bool {
        match self {
            Self::Act { submit, .. }
            | Self::ActSettle { submit, .. }
            | Self::HandoffWake { submit, .. }
            | Self::FeedbackAck { submit, .. } => *submit,
            _ => false,
        }
    }

    fn same_pending(&self, candidate: &PendingProviderServiceIntent) -> Result<bool, String> {
        Ok(
            serde_json::to_value(self.pending()).map_err(|error| error.to_string())?
                == serde_json::to_value(candidate).map_err(|error| error.to_string())?,
        )
    }
}

impl ViewerRuntimeLiveServer {
    pub(in crate::viewer::runtime_live) fn ensure_direct_hosted_service_owner(
        &self,
        cognition: &RuntimeProviderActionContext,
    ) -> Result<(), String> {
        let agent_id = &cognition.request.request_context.agent_subject;
        if self.agent_service_pump_started
            || self.llm_sidecar.hosted_service_inflight.is_some()
            || self.llm_sidecar.hosted_wait.is_some()
            || self.llm_sidecar.hosted_resume.is_some()
            || self.llm_sidecar.hosted_admission.is_some()
        {
            return Err(
                "canonical Agent service pump or phase owns hosted work; defer direct action"
                    .into(),
            );
        }
        if self.llm_sidecar.provider_service_pending.len() > 1
            || self
                .llm_sidecar
                .provider_service_pending
                .keys()
                .any(|pending_agent| pending_agent != agent_id)
        {
            return Err("another canonical Agent intent is pending; defer direct action".into());
        }
        if let Some(phase) = self.llm_sidecar.hosted_service_phase.as_ref() {
            if serde_json::to_value(&phase.pending().cognition)
                .map_err(|error| error.to_string())?
                != serde_json::to_value(cognition).map_err(|error| error.to_string())?
            {
                return Err("another hosted service phase owns work; defer direct action".into());
            }
            let persisted = self
                .llm_sidecar
                .provider_service_pending
                .get(agent_id)
                .ok_or("hosted phase has no durable original action; defer direct action")?;
            if !phase.same_pending(persisted)? {
                return Err("hosted phase differs from durable original action; fenced".into());
            }
        }
        Ok(())
    }

    pub(in crate::viewer::runtime_live) fn classify_direct_hosted_service_error(
        &self,
        error: String,
    ) -> ProviderRuntimeActionCommitError {
        match self.llm_sidecar.hosted_service_phase.as_ref() {
            Some(
                HostedServicePhase::HandoffWake { .. } | HostedServicePhase::HandoffView { .. },
            ) => ProviderRuntimeActionCommitError::WakeHandoff(error),
            Some(HostedServicePhase::Act { .. }) | None => {
                ProviderRuntimeActionCommitError::Message(error)
            }
            _ => ProviderRuntimeActionCommitError::PostCommit(error),
        }
    }

    pub(in crate::viewer::runtime_live) fn drive_direct_hosted_service_action(
        &mut self,
        pending: PendingProviderServiceIntent,
        submit: bool,
    ) -> Result<(), String> {
        const MAX_DIRECT_IO: usize = 24;
        const MAX_DIRECT_LOCAL_ADVANCES: usize = 16;

        let agent_id = &pending.cognition.request.request_context.agent_subject;
        self.ensure_direct_hosted_service_owner(&pending.cognition)?;
        let persisted = self
            .llm_sidecar
            .provider_service_pending
            .get(agent_id)
            .ok_or("canonical Agent action checkpoint missing")?;
        if serde_json::to_value(persisted).map_err(|error| error.to_string())?
            != serde_json::to_value(&pending).map_err(|error| error.to_string())?
        {
            return Err("canonical Agent action checkpoint identity mismatch; fenced".into());
        }
        if self.llm_sidecar.hosted_service_phase.is_none() {
            self.llm_sidecar.hosted_service_phase = Some(HostedServicePhase::Act {
                pending: pending.clone(),
                submit,
            });
        }
        let phase = self
            .llm_sidecar
            .hosted_service_phase
            .as_ref()
            .ok_or("canonical Agent hosted phase missing")?;
        if !phase.same_pending(&pending)? {
            return Err("another canonical Agent phase owns this action; defer".into());
        }

        let mut io_count = 0usize;
        let mut local_advances = 0usize;
        let mut same_phase_replays = 0usize;
        loop {
            self.ensure_direct_hosted_service_owner(&pending.cognition)?;
            if !self
                .llm_sidecar
                .provider_service_pending
                .contains_key(agent_id)
            {
                if self.llm_sidecar.hosted_service_phase.is_none() {
                    return Ok(());
                }
                return Err(
                    "canonical Agent phase remains after pending action disappeared".into(),
                );
            }
            let before = self
                .llm_sidecar
                .hosted_service_phase
                .as_ref()
                .ok_or("canonical Agent service phase disappeared before finalization")?
                .identity()?;
            match self.prepare_hosted_service_io(false)? {
                AgentServiceProgress::Idle => {
                    return Err(
                        "canonical Agent action remains pending without an owned phase advance"
                            .into(),
                    );
                }
                AgentServiceProgress::Advanced => {
                    if !self
                        .llm_sidecar
                        .provider_service_pending
                        .contains_key(agent_id)
                        && self.llm_sidecar.hosted_service_phase.is_none()
                    {
                        return Ok(());
                    }
                    let after = self
                        .llm_sidecar
                        .hosted_service_phase
                        .as_ref()
                        .ok_or("canonical Agent phase disappeared with pending action retained")?
                        .identity()?;
                    if after == before {
                        return Err(
                            "canonical Agent action remains unresolved; exact checkpoint retained"
                                .into(),
                        );
                    }
                    local_advances += 1;
                    if local_advances > MAX_DIRECT_LOCAL_ADVANCES {
                        return Err("canonical Agent action exceeded bounded local phase advances; checkpoint retained".into());
                    }
                    same_phase_replays = 0;
                }
                AgentServiceProgress::NeedsIo(job) => {
                    io_count += 1;
                    if io_count > MAX_DIRECT_IO {
                        self.llm_sidecar.hosted_service_inflight = None;
                        return Err("canonical Agent action exceeded bounded direct I/O; checkpoint retained".into());
                    }
                    self.apply_hosted_service_io(job.execute())?;
                    if !self
                        .llm_sidecar
                        .provider_service_pending
                        .contains_key(agent_id)
                        && self.llm_sidecar.hosted_service_phase.is_none()
                    {
                        return Ok(());
                    }
                    let phase =
                        self.llm_sidecar.hosted_service_phase.as_ref().ok_or(
                            "canonical Agent phase disappeared with pending action retained",
                        )?;
                    let after = phase.identity()?;
                    if after != before {
                        local_advances += 1;
                        if local_advances > MAX_DIRECT_LOCAL_ADVANCES {
                            return Err("canonical Agent action exceeded bounded local phase advances; checkpoint retained".into());
                        }
                        same_phase_replays = 0;
                    } else if phase.may_submit_original() && same_phase_replays == 0 {
                        same_phase_replays += 1;
                    } else {
                        return Err(
                            "canonical Agent action remains unresolved; exact checkpoint retained"
                                .into(),
                        );
                    }
                }
            }
        }
    }

    pub(super) fn prepare_hosted_service_wake_handoff(
        &mut self,
        pending: PendingProviderServiceIntent,
        receipt: ProviderServiceCognitionReceipt,
    ) -> Result<Option<HostedServicePhase>, String> {
        let Some((target_wake, operation)) = self
            .llm_sidecar
            .committed_service_wake_handoff(&pending.cognition)?
        else {
            return Ok(None);
        };
        let request = &pending.cognition.request.request_context;
        let phase = format!("handoff:{}", target_wake.wake_id);
        let (_, existed) = self
            .llm_sidecar
            .prepare_service_scheduler_checkpoint(request, &phase, operation, None)?;
        Ok(Some(HostedServicePhase::HandoffWake {
            pending,
            receipt,
            target_wake: Box::new(target_wake),
            submit: !existed,
            replay_authorized: false,
        }))
    }
}
