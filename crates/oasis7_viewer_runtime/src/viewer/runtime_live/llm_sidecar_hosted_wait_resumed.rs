//! A resumed Wait retains Runtime's already admitted positive successor.
use super::*;
use crate::world_service::wire::WorldServicePayloadV1;
impl RuntimeLlmSidecar {
    pub(super) fn validate_resumed_wait_checkpoint(
        &mut self,
        wait: &HostedWait,
    ) -> Result<lineage_persistence::PendingProviderSchedulerIntent, String> {
        wait.validate_original()?;
        let cognition = wait
            .decision
            .cognition
            .as_ref()
            .ok_or("resumed Wait cognition missing")?;
        let request = &cognition.request.request_context;
        let context = &cognition.request;
        let prefix = format!("{}:resume:", request.provider_invocation_key());
        let candidates = self
            .provider_scheduler_pending
            .iter()
            .filter(|(id, pending)| {
                id.starts_with(&prefix)
                    && pending.resume_context.as_ref().is_some_and(|original| {
                        serde_json::to_value(original).ok() == serde_json::to_value(context).ok()
                    })
            })
            .map(|(id, pending)| (id.clone(), pending.clone()))
            .collect::<Vec<_>>();
        if candidates.len() != 1 {
            return Err("resumed Wait unique original Resume checkpoint missing".into());
        }
        let (id, pending) = &candidates[0];
        let WorldServicePayloadV1::Scheduler(signed) = &pending.payload else {
            return Err("resumed Wait original signed Resume missing".into());
        };
        let SchedulerOperationV1::ResumeWake {
            proposal, resume, ..
        } = &signed.request.operation
        else {
            return Err("resumed Wait checkpoint operation mismatch".into());
        };
        if serde_json::to_value(proposal).map_err(|error| error.to_string())?
            != serde_json::to_value(&wait.runtime).map_err(|error| error.to_string())?
            || resume.agent_session_id != request.agent_session_id
            || resume.agent_turn_id != request.agent_turn_id
            || resume.decision_request_id != request.decision_request_id
            || resume.request_digest != request.request_digest.to_string()
            || resume.context_digest != async_support::runtime_provider_context_digest(request)
        {
            return Err("resumed Wait original Resume identity mismatch".into());
        }
        let current = pending
            .resume_current_context
            .as_ref()
            .ok_or("resumed Wait original current context missing")?;
        if current.authority != wait.current.authority {
            return Err("resumed Wait original authority changed".into());
        }
        let phase = id
            .strip_prefix(&format!("{}:", request.provider_invocation_key()))
            .ok_or("resumed Wait phase invalid")?;
        let (checked, existed) = self.prepare_service_scheduler_checkpoint(
            request,
            phase,
            signed.request.operation.clone(),
            Some((context.clone(), current.clone())),
        )?;
        if !existed
            || serde_json::to_value(&checked).map_err(|error| error.to_string())?
                != serde_json::to_value(pending).map_err(|error| error.to_string())?
        {
            return Err("resumed Wait original checkpoint changed".into());
        }
        Ok(checked)
    }
}
