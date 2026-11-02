//! Fixed-world historical verification. No Current grant is inferred from facts.
use super::*;
impl TerminationStopAllocationV2 {
    pub(in crate::provisioning) fn validate_shape(&self) -> Result<()> {
        let r = &self.recovery;
        if self.payload.operation != oasis7_agent_api::game::GameExecutorOperationV1::Stop
            || self.allocated_at.get() == 0
            || self.allocated_at.get() >= r.original_until.get()
            || r.original_until.get() > r.source_until.get()
            || r.original_until.get() > r.native_until.get()
            || r.native_lifetime.validate().is_err()
        {
            return Err(invalid());
        }
        if self.signed.is_none()
            && (r.submit_claim.is_some()
                || r.lookup_claim.is_some()
                || !r.observations.is_empty()
                || r.admitted.is_some())
        {
            return Err(invalid());
        }
        if r.lookup_claim.is_some() && r.submit_claim.is_none() {
            return Err(invalid());
        }
        if let Some(signed) = &self.signed {
            let OwnerSetupOriginalV1::Canonical { submit, lookup } = signed.as_ref() else {
                return Err(invalid());
            };
            let WorldServicePayloadV1::GameExecutorControl(control) = &submit.signed_payload else {
                return Err(invalid());
            };
            if control.request != self.payload
                || control.subject_public_key != self.payload.fence.owner_public_key
                || submit.deadline_unix_ms != Some(r.original_until.get())
                || lookup.original != submit.signed_payload
                || lookup.request.contract_version != 1
                || lookup.request.key != submit.correlation.key
            {
                return Err(invalid());
            }
            submit.validate().map_err(|_| invalid())?;
            oasis7_client_api::world_service::private_goal::verify_owner_signature_strict(
                oasis7_agent_api::game::GAME_EXECUTOR_CONTROL_DOMAIN,
                &control.request,
                &control.subject_public_key,
                &control.signature_hex,
            )
            .map_err(|_| invalid())?;
            let correlation = oasis7_client_api::world_service_correlation::derive_correlation(
                ws::WorldIdentity {
                    world_id: self.payload.fence.world_id.clone(),
                    genesis_digest: self.payload.fence.genesis_digest.clone(),
                },
                &submit.signed_payload,
            )
            .map_err(|_| invalid())?;
            if submit.correlation != correlation {
                return Err(invalid());
            }
            let digest = digest(&("oasis7.custody.stop-original.v2", signed))?;
            let mut ids = BTreeSet::new();
            let mut indices = BTreeSet::new();
            for claim in [&r.submit_claim, &r.lookup_claim].into_iter().flatten() {
                if claim.original_digest != digest
                    || claim.claimed_at.get() < self.allocated_at.get()
                    || !id(&claim.attempt_id)
                    || !ids.insert(&claim.attempt_id)
                    || !indices.insert(claim.paid_index)
                    || claim.paid_index >= 24
                {
                    return Err(invalid());
                }
            }
        }
        if let Some(admitted) = &r.admitted {
            let source = admitted.result.canonical_fact().ok_or_else(invalid)?.source;
            if r.observations.get(&source) != Some(admitted.result.as_ref())
                || !admitted.result.terminal()
            {
                return Err(invalid());
            }
        }
        Ok(())
    }
    pub(in crate::provisioning) fn validate_fact(
        &self,
        term: &TerminationPlanV2,
        result: &OwnerSetupResultV1,
    ) -> Result<()> {
        self.validate_shape()?;
        let OwnerSetupOriginalV1::Canonical { submit, lookup } =
            self.signed.as_ref().ok_or_else(invalid)?.as_ref()
        else {
            return Err(invalid());
        };
        let OwnerSetupResultV1::Canonical {
            response,
            from_lookup,
        } = result
        else {
            return Err(invalid());
        };
        if self.recovery.submit_claim.is_none()
            || (*from_lookup && self.recovery.lookup_claim.is_none())
        {
            return Err(invalid());
        }
        let domain = if *from_lookup {
            ws::LOOKUP_PATH
        } else {
            ws::SUBMIT_PATH
        };
        let expected = if *from_lookup {
            ws::request_digest(domain, lookup)
        } else {
            ws::request_digest(domain, submit)
        }
        .map_err(|_| invalid())?;
        if response.request_digest != expected {
            return Err(invalid());
        }
        response
            .payload
            .validate(&submit.correlation)
            .map_err(|_| invalid())?;
        oasis7_client_api::world_service::private_goal::verify_owner_signature_strict(
            domain,
            &(&response.request_digest, &response.payload),
            &term.world.service_public_key,
            &response.signature_hex,
        )
        .map_err(|_| invalid())?;
        if let ws::IntentOutcome::Committed { commit, receipt } = &response.payload.outcome {
            if commit.binding.branch_id != term.world.branch_id
                || commit.binding.reorg_generation != term.world.reorg_epoch.0
            {
                return Err(invalid());
            }
            let legacy =
                serde_json::from_value(serde_json::to_value(submit).map_err(|_| invalid())?)
                    .map_err(|_| invalid())?;
            let expected = serde_json::to_value(
                oasis7_client_api::legacy_task_results::executor_receipt(&legacy)
                    .map_err(|_| invalid())?,
            )
            .map_err(|_| invalid())?;
            if receipt != &expected {
                return Err(invalid());
            }
        }
        Ok(())
    }
    pub(in crate::provisioning) fn validate_source(
        &self,
        sources: &BTreeMap<String, TestSessionSourceV2>,
        funding: &BTreeMap<String, SessionSourceFundingV1>,
        input: &str,
    ) -> Result<()> {
        let r = &self.recovery;
        let s = sources.get(&r.source_operation_id).ok_or_else(invalid)?;
        let v = &s
            .source
            .verification
            .as_ref()
            .ok_or_else(invalid)?
            .verification;
        let until = v
            .observed_at
            .0
            .checked_add(5000)
            .ok_or_else(invalid)?
            .min(s.source.expires_at.0)
            .min(v.proof_expires_at.0)
            .min(v.source_session_expires_at.0)
            .min(v.source_session_idle_expires_at.0);
        if s.api_version != 2
            || s.approved_input_digest != input
            || funding.get(&r.source_operation_id) != Some(&SessionSourceFundingV1::TerminationPlan)
            || digest(s)? != r.source_record_digest
            || r.source_until.get() != until
            || self.allocated_at.get() < v.observed_at.0
        {
            return Err(invalid());
        }
        if let Some(a) = &r.admitted {
            let source = sources.get(&a.source_operation_id).ok_or_else(invalid)?;
            let v = &source
                .source
                .verification
                .as_ref()
                .ok_or_else(invalid)?
                .verification;
            if source.api_version != 2
                || source.approved_input_digest != input
                || digest(source)? != a.source_record_digest
                || funding.get(&a.source_operation_id)
                    != Some(&SessionSourceFundingV1::TerminationPlan)
                || a.admitted_at.get() < v.observed_at.0
                || a.admitted_at.get().saturating_sub(v.observed_at.0) >= 5000
                || a.admitted_at.get() >= source.source.expires_at.0
                || a.native_lifetime.validate().is_err()
                || a.admitted_at.get() >= a.native_lifetime.lifetime.status.expires_at.0
            {
                return Err(invalid());
            }
        }
        for claim in [&r.submit_claim, &r.lookup_claim].into_iter().flatten() {
            let source = sources
                .get(&claim.source_operation_id)
                .ok_or_else(invalid)?;
            let v = &source
                .source
                .verification
                .as_ref()
                .ok_or_else(invalid)?
                .verification;
            if source.api_version != 2
                || source.approved_input_digest != input
                || digest(source)? != claim.source_record_digest
                || funding.get(&claim.source_operation_id)
                    != Some(&SessionSourceFundingV1::TerminationPlan)
                || claim.claimed_at.get() < v.observed_at.0
                || claim.claimed_at.get().saturating_sub(v.observed_at.0) >= 5000
                || claim.claimed_at.get() >= source.source.expires_at.0
                || claim.native_lifetime.validate().is_err()
                || claim.claimed_at.get() >= claim.native_lifetime.lifetime.status.expires_at.0
                || claim.claimed_at.get() >= claim.native_lifetime.lifetime.session_expires_at.0
                || claim.claimed_at.get()
                    >= claim.native_lifetime.lifetime.session_idle_expires_at.0
            {
                return Err(invalid());
            }
        }
        Ok(())
    }
    pub(in crate::provisioning) fn validate_admitted(
        &self,
        term: &TerminationPlanV2,
        input: &str,
    ) -> Result<()> {
        let Some(a) = &self.recovery.admitted else {
            return Ok(());
        };
        self.validate_fact(term, &a.result)?;
        let w = &a.world;
        w.response
            .payload
            .validate(&w.request)
            .map_err(|_| invalid())?;
        let read = &w.request.read;
        let expected_nonce = digest(&(
            "oasis7.custody.test-session-termination-read.v2",
            input,
            &a.source_operation_id,
        ))?;
        if read.subject_public_key != term.initial_fence.owner_public_key
            || read.request.observation_nonce.as_deref() != Some(expected_nonce.as_str())
            || read.request.deadline_unix_ms != Some(term.cleanup_until.get())
            || read.request.fixed_commit.is_some()
            || read.request.world.world_id != term.world.world_id
            || read.request.world.genesis_digest != term.world.genesis_digest
            || read.request.scope_id != format!("agent:{}", term.initial_fence.agent_id)
            || a.admitted_at.get() >= term.cleanup_until.get()
            || w.response.request_digest
                != ws::request_digest(
                    ws::participant_discovery::PARTICIPANT_OWNER_VIEW_PATH,
                    &w.request,
                )
                .map_err(|_| invalid())?
        {
            return Err(invalid());
        }
        oasis7_client_api::world_service::private_goal::verify_owner_signature_strict(
            ws::VIEW_JSON_PATH,
            &read.request,
            &read.subject_public_key,
            &read.signature_hex,
        )
        .map_err(|_| invalid())?;
        oasis7_client_api::world_service::private_goal::verify_owner_signature_strict(
            ws::participant_discovery::PARTICIPANT_OWNER_VIEW_PATH,
            &(&w.response.request_digest, &w.response.payload),
            &term.world.service_public_key,
            &w.response.signature_hex,
        )
        .map_err(|_| invalid())?;
        let mut expected = term.initial_fence.clone();
        if let OwnerSetupResultV1::Canonical { response, .. } = a.result.as_ref() {
            if let ws::IntentOutcome::Committed { receipt, .. } = &response.payload.outcome {
                let r: oasis7_client_api::legacy_task_results::ExecutorReceipt =
                    serde_json::from_value(receipt.clone()).map_err(|_| invalid())?;
                expected.task_revision = r.task_revision;
                expected.executor_epoch = r.executor_epoch;
                expected.executor_binding = r.executor_binding;
                expected.task_status = r.task_status;
            }
        }
        if GameTaskFenceV1::from_control(
            w.response
                .payload
                .control
                .view
                .game_task_control
                .as_ref()
                .ok_or_else(invalid)?,
        ) != expected
            || w.response.payload.signer_delegation.as_ref() != Some(&term.original_delegation)
        {
            return Err(invalid());
        }
        Ok(())
    }
}
