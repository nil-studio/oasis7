use super::*;

impl RuntimeLlmSidecar {
    pub(in crate::viewer::runtime_live) fn validate_scheduler_checkpoint_integrity(
        world: &oasis7_client_api::world_service::WorldIdentity,
        request_id: &str,
        pending: &lineage_persistence::PendingProviderSchedulerIntent,
    ) -> Result<(), String> {
        let crate::world_service::wire::WorldServicePayloadV1::Scheduler(signed) = &pending.payload
        else {
            return Err("pending scheduler checkpoint operation mismatch".into());
        };
        crate::world_service::verify_read_request("scheduler", signed)?;
        if signed.request.request_id != request_id {
            return Err("pending scheduler original request ID mismatch; fenced".into());
        }
        if crate::world_service::derive_correlation(world.clone(), &pending.payload)?
            != pending.correlation
        {
            return Err("pending scheduler signed payload and correlation mismatch; fenced".into());
        }
        Ok(())
    }
    /// Freeze the exact signed operation durably before any transport.
    pub(in crate::viewer::runtime_live) fn prepare_service_scheduler_checkpoint(
        &mut self,
        request: &crate::simulator::ContinuousAgentRequestContextV1,
        phase: &str,
        operation: crate::world_service::wire::SchedulerOperationV1,
        prepared: Option<(
            cognition_context::ProviderContextState,
            crate::simulator::ContinuationCurrentContextV1,
        )>,
    ) -> Result<(lineage_persistence::PendingProviderSchedulerIntent, bool), String> {
        use crate::world_service::{authority::sign_read_request, wire::*};
        let config = self
            .provider_service_config
            .clone()
            .ok_or("canonical provider service missing")?;
        let signer = self
            .provider_service_signer
            .as_ref()
            .ok_or("explicit canonical Agent signer missing")?;
        let id = format!("{}:{phase}", request.provider_invocation_key());
        let existing = self.provider_scheduler_pending.get(&id).cloned();
        let pending = if let Some(pending) = existing.as_ref() {
            Self::validate_scheduler_checkpoint_integrity(&config.expected_world, &id, pending)?;
            if phase.starts_with("resume:") {
                let (context, current) = prepared.as_ref().ok_or(
                    "canonical ResumeWake retry requires complete original prepared context",
                )?;
                let original = pending.resume_context.as_ref().ok_or(
                    "canonical ResumeWake checkpoint lacks original prepared context; fenced",
                )?;
                if serde_json::to_value(context).map_err(|error| error.to_string())?
                    != serde_json::to_value(original).map_err(|error| error.to_string())?
                    || pending.resume_current_context.as_ref() != Some(current)
                {
                    return Err(
                        "canonical ResumeWake prepared checkpoint identity conflict; fenced".into(),
                    );
                }
            }
            let WorldServicePayloadV1::Scheduler(signed) = &pending.payload else {
                return Err("pending scheduler checkpoint operation mismatch".into());
            };
            let key_bytes: [u8; 32] = hex::decode(&signer.private_key_hex)
                .map_err(|_| "invalid Agent signer key")?
                .try_into()
                .map_err(|_| "invalid Agent signer key")?;
            let public_key = hex::encode(
                ed25519_dalek::SigningKey::from_bytes(&key_bytes)
                    .verifying_key()
                    .to_bytes(),
            );
            if signed.subject_public_key != public_key
                || signed.request.agent_id != request.agent_subject
                || signed.request.delegation_generation != signer.delegation_generation
                || signed.request.operation != operation
            {
                return Err("pending scheduler request identity or generation changed; original intent fenced".into());
            }
            pending.clone()
        } else {
            if phase == "reserve" || phase.starts_with("prefix:") || phase.starts_with("resume:") {
                self.ensure_canonical_agent_durable_admission()?;
            }
            let b = if phase == "reserve" {
                &request.runtime_binding
            } else {
                self.provider_service_projection
                    .as_ref()
                    .and_then(|view| view.runtime_binding.as_ref())
                    .ok_or("canonical scheduler settlement requires verified current binding")?
            };
            let signed = sign_read_request(
                "scheduler",
                SchedulerIntentV1 {
                    agent_id: request.agent_subject.clone(),
                    request_id: id.clone(),
                    delegation_generation: signer.delegation_generation,
                    captured_base_binding: crate::runtime::RuntimeCognitionBaseBindingV1 {
                        world_id: b.world_id.clone(),
                        branch_id: b.branch_id.clone(),
                        finality_epoch: b.finality_epoch,
                        finality_block_hash: b
                            .finality_block_hash
                            .as_ref()
                            .map(ToString::to_string),
                        finality_status: b.finality_status.clone(),
                        base_tick: b.base_tick,
                        base_world_hash: b.base_world_hash.to_string(),
                        reorg_epoch: b.reorg_epoch,
                        runtime_manifest_hash: b.runtime_manifest_hash.to_string(),
                    },
                    operation,
                },
                &signer.private_key_hex,
            )?;
            let payload = WorldServicePayloadV1::Scheduler(signed);
            let correlation =
                crate::world_service::derive_correlation(config.expected_world.clone(), &payload)?;
            let pending = lineage_persistence::PendingProviderSchedulerIntent {
                resume_context: prepared.as_ref().map(|(context, _)| context.clone()),
                resume_current_context: prepared.map(|(_, current)| current),
                correlation,
                payload,
            };
            self.provider_scheduler_pending.insert(id, pending.clone());
            self.persist_provider_lineage()?;
            pending
        };
        Ok((pending, existing.is_some()))
    }
}

impl RuntimeLlmSidecar {
    pub(in crate::viewer::runtime_live) fn ensure_canonical_agent_durable_admission(
        &self,
    ) -> Result<(), String> {
        if self.provider_service_required
            && (!self.provider_service_lineage_store_explicit
                || self.provider_lineage_store.is_none())
        {
            return Err("canonical Agent admission requires explicit durable App-private --provider-lineage-store".into());
        }
        Ok(())
    }
}

#[cfg(test)]
#[path = "llm_sidecar_scheduler_checkpoint_tests.rs"]
mod tests;
