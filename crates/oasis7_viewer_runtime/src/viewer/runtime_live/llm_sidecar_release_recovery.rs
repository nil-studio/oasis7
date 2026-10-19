use super::*;
use serde_json::Value;

impl RuntimeLlmSidecar {
    pub(in crate::viewer::runtime_live) fn validate_service_release_receipt(
        request: &crate::simulator::ContinuousAgentRequestContextV1,
        lease: &crate::runtime::CognitionLeaseV1,
        value: Value,
    ) -> Result<(), String> {
        lineage_generation_recovery::validate_provider_lease_identity(
            &request.agent_subject,
            request,
            lease,
        )?;
        let receipt: crate::runtime::CognitionReceiptV1 = serde_json::from_value(value)
            .map_err(|error| format!("canonical release receipt decode failed: {error}"))?;
        receipt
            .validate()
            .map_err(|error| format!("canonical release receipt invalid: {error}"))?;
        if receipt.operation != "release"
            || receipt.status != crate::runtime::CognitionLeaseStatusV1::Released
            || receipt.lease_id != lease.lease_id
            || receipt.idempotency_key != lease.idempotency_key
            || receipt.account_id != lease.account_id
            || receipt.agent_id != lease.agent_id
            || receipt.agent_session_id != lease.agent_session_id
            || receipt.agent_turn_id != lease.agent_turn_id
            || receipt.decision_request_id != lease.decision_request_id
            || receipt.request_digest != lease.request_digest
            || receipt.quote != lease.quote
            || receipt.reserved_amount != lease.reserved_amount
            || receipt.released_amount != lease.reserved_amount
            || receipt.consumed_amount != 0
            || receipt.refunded_amount != 0
            || receipt.net_amount != 0
        {
            return Err("canonical release receipt identity or accounting mismatch".into());
        }
        Ok(())
    }

    pub(in crate::viewer::runtime_live) fn recover_service_release_checkpoints(
        &mut self,
        world: &mut RuntimeWorld,
    ) -> Result<bool, String> {
        if !self.provider_service_required {
            return Ok(false);
        }
        let mut recovered = false;
        for (agent_id, recovery) in self.provider_recovery_pending.clone() {
            let request = &recovery.active.request_context;
            let id = format!("{}:release", request.provider_invocation_key());
            let Some(pending) = self.provider_scheduler_pending.get(&id).cloned() else {
                continue;
            };
            let crate::world_service::wire::WorldServicePayloadV1::Scheduler(signed) =
                &pending.payload
            else {
                return Err("canonical release recovery checkpoint operation mismatch".into());
            };
            let crate::world_service::wire::SchedulerOperationV1::ReleaseLease { lease_id } =
                &signed.request.operation
            else {
                return Err("canonical release recovery checkpoint operation mismatch".into());
            };
            let lease = self
                .provider_cognition_leases
                .get(&agent_id)
                .cloned()
                .ok_or("canonical release recovery original mirror missing")?;
            request
                .validate_production_lane()
                .map_err(|error| error.to_string())?;
            lineage_generation_recovery::validate_provider_lease_identity(
                &agent_id, request, &lease,
            )?;
            let view = self
                .provider_service_projection
                .as_ref()
                .ok_or("canonical release recovery verified projection missing")?;
            let binding = view
                .runtime_binding
                .as_ref()
                .ok_or("canonical release recovery binding missing")?;
            let capability = view
                .agent_context
                .as_ref()
                .filter(|context| context.agent_id == agent_id)
                .ok_or("canonical release recovery Agent authority missing")?;
            let invocation = request
                .base_decision_request
                .capability_invocation_context
                .as_ref()
                .ok_or("canonical release recovery original capability missing")?;
            if lease_id != &lease.lease_id
                || signed.request.agent_id != agent_id
                || signed.request.request_id != id
                || invocation.subject != capability.capability_invocation_context.subject
                || invocation.grant_id != capability.capability_invocation_context.grant_id
                || request.runtime_binding.world_id != binding.world_id
                || request.runtime_binding.branch_id != binding.branch_id
                || request.runtime_binding.reorg_epoch != binding.reorg_epoch
                || request.runtime_binding.finality_epoch != binding.finality_epoch
                || crate::world_service::derive_correlation(
                    pending.correlation.key.world.clone(),
                    &pending.payload,
                )? != pending.correlation
            {
                return Err(
                    "canonical release recovery original identity or delegation mismatch; fenced"
                        .into(),
                );
            }
            // Existing checkpoint forces Lookup and validates the original
            // signer/generation/operation. A signed Released view is no receipt.
            self.release_provider_lease_at_authority(world, request, &lease)?;
            let backup_leases = self.provider_cognition_leases.clone();
            let backup_recovery = self.provider_recovery_pending.clone();
            let backup_exhausted = self.provider_transport_exhausted.clone();
            let backup_contexts = self.provider_contexts.clone();
            let backup_retry = self.provider_retry_contexts.clone();
            if self.provider_cognition_leases.get(&agent_id) != Some(&lease) {
                return Err(
                    "canonical release recovery mirror changed; newer identity retained".into(),
                );
            }
            for context in [
                self.provider_contexts.get(&agent_id),
                self.provider_retry_contexts.get(&agent_id),
                self.provider_active_turns.get(&agent_id),
            ]
            .into_iter()
            .flatten()
            {
                if serde_json::to_value(context).map_err(|error| error.to_string())?
                    != serde_json::to_value(&recovery.active).map_err(|error| error.to_string())?
                {
                    return Err(
                        "canonical release recovery context changed; newer identity retained"
                            .into(),
                    );
                }
            }
            self.provider_cognition_leases.remove(&agent_id);
            self.provider_recovery_pending.remove(&agent_id);
            self.provider_transport_exhausted.remove(&agent_id);
            self.provider_contexts.remove(&agent_id);
            self.provider_retry_contexts.remove(&agent_id);
            self.provider_scheduler_pending.remove(&id);
            if let Err(error) = self.persist_provider_lineage() {
                self.provider_cognition_leases = backup_leases;
                self.provider_recovery_pending = backup_recovery;
                self.provider_transport_exhausted = backup_exhausted;
                self.provider_contexts = backup_contexts;
                self.provider_retry_contexts = backup_retry;
                self.provider_scheduler_pending.insert(id, pending);
                return Err(error);
            }
            recovered = true;
        }
        Ok(recovered)
    }
}
