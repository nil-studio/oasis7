use super::*;
impl AccountProvisionApprovalState {
    pub fn publish_test_session_termination_checks(
        &self,
        id: &str,
        kind: TerminationOperationV1,
        original: &OwnerSetupOriginalV1,
        current: &TestSessionTerminationCurrent<'_, '_, '_>,
        now: UnixMillis,
    ) -> Result<ExactSignatureChecks> {
        let checks = self.termination_current_checks(id, kind, current, now)?;
        let s = self.test_sessions.get(id).ok_or_else(invalid)?;
        let slot = s.termination_slots.get(&kind).ok_or_else(invalid)?;
        self.validate_termination_signed(s, slot, original)?;
        Ok(append_owner_setup_checks(
            checks,
            OwnerSetupSignatureFact::Original(
                original,
                &s.termination.initial_fence.owner_public_key,
            ),
        )?)
    }
    pub(in crate::provisioning) fn validate_termination_signed(
        &self,
        s: &TestSessionAuthorityV1,
        slot: &TerminationOriginalV1,
        original: &OwnerSetupOriginalV1,
    ) -> Result<()> {
        let OwnerSetupOriginalV1::Canonical { submit, lookup } = original else {
            return Err(invalid());
        };
        let key = &s.termination.initial_fence.owner_public_key;
        let matched = match (&slot.payload, &submit.signed_payload) {
            (
                TerminationPayloadV1::Stop(expected),
                WorldServicePayloadV1::GameExecutorControl(actual),
            ) => expected.as_ref() == &actual.request && &actual.subject_public_key == key,
            (TerminationPayloadV1::Revoke(expected), WorldServicePayloadV1::Delegation(actual)) => {
                expected == &actual.request && &actual.subject_public_key == key
            }
            _ => false,
        };
        submit.validate().map_err(|_| invalid())?;
        let correlation = oasis7_client_api::world_service_correlation::derive_correlation(
            ws::WorldIdentity {
                world_id: s.termination.world.world_id.clone(),
                genesis_digest: s.termination.world.genesis_digest.clone(),
            },
            &submit.signed_payload,
        )
        .map_err(|_| invalid())?;
        if !matched
            || submit.correlation != correlation
            || submit.deadline_unix_ms != Some(s.termination.cleanup_until.get())
            || lookup.original != submit.signed_payload
            || lookup.request.contract_version != 1
            || lookup.request.key != correlation.key
            || slot.signed.as_ref().is_some_and(|v| v.as_ref() != original)
        {
            return Err(invalid());
        }
        Ok(())
    }
    pub fn publish_test_session_termination(
        &mut self,
        id: &str,
        kind: TerminationOperationV1,
        original: OwnerSetupOriginalV1,
        current: &TestSessionTerminationCurrent<'_, '_, '_>,
        now: UnixMillis,
        verifier: &mut impl ApprovalSignatureVerifier,
    ) -> Result<()> {
        verifier.verify(
            &self.publish_test_session_termination_checks(id, kind, &original, current, now)?,
        )?;
        let mut next = self.clone();
        next.test_sessions
            .get_mut(id)
            .and_then(|s| s.termination_slots.get_mut(&kind))
            .ok_or_else(invalid)?
            .signed = Some(Box::new(original));
        next.validate_persistence()?;
        *self = next;
        Ok(())
    }
    /// Charge each real private/HTTP transport before any I/O. No implicit retries.
    pub fn reserve_test_session_termination_transport(
        &mut self,
        id: &str,
        kind: TerminationOperationV1,
        attempt: &str,
        now: UnixMillis,
    ) -> Result<()> {
        self.test_session_local(id, SessionPurposeV1::TerminationPlan, now)?;
        if !id_nonempty(attempt) {
            return Err(invalid());
        }
        if Self::session_transport_attempt_occupied(
            self.test_sessions.get(id).ok_or_else(invalid)?,
            attempt,
        ) {
            return Err(ProvisionApprovalError::Conflict);
        }
        let mut next = self.clone();
        let s = next.test_sessions.get_mut(id).ok_or_else(invalid)?;
        if s.closing.is_none() {
            return Err(invalid());
        }
        let slot = s.termination_slots.get_mut(&kind).ok_or_else(invalid)?;
        if slot.transport_attempts.iter().any(|a| a == attempt) {
            return Err(ProvisionApprovalError::Conflict);
        }
        slot.transport_attempts.push(attempt.into());
        s.roots
            .get_mut(&SessionPurposeV1::TerminationPlan)
            .ok_or_else(invalid)?
            .budgets
            .transport
            .charge(1)
            .map_err(|_| invalid())?;
        next.validate_persistence()?;
        *self = next;
        Ok(())
    }
    pub fn claim_test_session_termination(
        &mut self,
        id: &str,
        kind: TerminationOperationV1,
        lookup: bool,
        current: &TestSessionTerminationCurrent<'_, '_, '_>,
        now: UnixMillis,
        verifier: &mut impl ApprovalSignatureVerifier,
    ) -> Result<OwnerSetupOriginalV1> {
        // Legacy API can claim a write only; readonly recovery requires the
        // atomic, specifically linked paid lookup API below.
        if lookup {
            return Err(invalid());
        }
        verifier.verify(&self.termination_current_checks(id, kind, current, now)?)?;
        let mut next = self.clone();
        let slot = next
            .test_sessions
            .get_mut(id)
            .and_then(|s| s.termination_slots.get_mut(&kind))
            .ok_or_else(invalid)?;
        if slot.transport_attempts.is_empty() || slot.result.is_some() || slot.claimed_at.is_some()
        {
            return Err(invalid());
        }
        let original = slot.signed.as_ref().ok_or_else(invalid)?.as_ref().clone();
        slot.claimed_at = Some(now);
        next.validate_persistence()?;
        *self = next;
        Ok(original)
    }
}
fn id_nonempty(s: &str) -> bool {
    !s.is_empty() && s.len() <= 256 && !s.chars().any(char::is_control)
}

impl AccountProvisionApprovalState {
    pub fn test_session_termination_result_checks(
        &self,
        id: &str,
        kind: TerminationOperationV1,
        result: &OwnerSetupResultV1,
        current: &TestSessionLiveCurrent<'_, '_>,
        now: UnixMillis,
    ) -> Result<ExactSignatureChecks> {
        self.require_session_source_funding(
            id,
            current.source_operation_id,
            SessionSourceFundingV1::TerminationPlan,
        )?;
        let checks =
            self.test_session_live_checks(id, SessionPurposeV1::TerminationPlan, current, now)?;
        let s = self.test_sessions.get(id).ok_or_else(invalid)?;
        let slot = s.termination_slots.get(&kind).ok_or_else(invalid)?;
        self.validate_termination_result(s, slot, result)?;
        let source = result.canonical_fact().ok_or_else(invalid)?.source;
        if slot.observations.get(&source) != Some(result) {
            return Err(invalid());
        }
        Ok(append_owner_setup_checks(
            checks,
            OwnerSetupSignatureFact::Result(result, &s.termination.world.service_public_key),
        )?)
    }
    pub(in crate::provisioning) fn validate_termination_result(
        &self,
        s: &TestSessionAuthorityV1,
        slot: &TerminationOriginalV1,
        result: &OwnerSetupResultV1,
    ) -> Result<()> {
        let OwnerSetupOriginalV1::Canonical { submit, lookup } =
            slot.signed.as_ref().ok_or_else(invalid)?.as_ref()
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
        if slot.claimed_at.is_none()
            || (*from_lookup && slot.lookup_claimed_at.is_none())
            || slot
                .observations
                .get(&if *from_lookup {
                    OwnerSetupCanonicalSourceV1::Lookup
                } else {
                    OwnerSetupCanonicalSourceV1::Submit
                })
                .is_some_and(|old| old != result)
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
        validate_owner_setup_signature_fact(OwnerSetupSignatureFact::Result(
            result,
            &s.termination.world.service_public_key,
        ))?;
        if let ws::IntentOutcome::Committed { commit, receipt } = &response.payload.outcome {
            if commit.binding.branch_id != s.termination.world.branch_id
                || commit.binding.reorg_generation != s.termination.world.reorg_epoch.0
            {
                return Err(invalid());
            }
            let expected = match &slot.payload {
                TerminationPayloadV1::Stop(_) => {
                    let legacy: ws::SubmitIntentRequest<
                        oasis7_client_api::legacy_world_service::WorldServicePayloadV1,
                    > = serde_json::from_value(
                        serde_json::to_value(submit).map_err(|_| invalid())?,
                    )
                    .map_err(|_| invalid())?;
                    serde_json::to_value(
                        oasis7_client_api::legacy_task_results::executor_receipt(&legacy)
                            .map_err(|_| invalid())?,
                    )
                    .map_err(|_| invalid())?
                }
                TerminationPayloadV1::Revoke(d) => serde_json::json!({"delegation":d}),
            };
            if receipt != &expected {
                return Err(invalid());
            }
        }
        Ok(())
    }
    pub fn record_test_session_termination_result(
        &mut self,
        id: &str,
        kind: TerminationOperationV1,
        result: OwnerSetupResultV1,
        current: &TestSessionLiveCurrent<'_, '_>,
        now: UnixMillis,
        verifier: &mut impl ApprovalSignatureVerifier,
    ) -> Result<()> {
        let mut next = self.clone();
        let world_public_key = next
            .test_sessions
            .get(id)
            .ok_or_else(invalid)?
            .termination
            .world
            .service_public_key
            .clone();
        next.archive_test_session_termination_fact(
            id,
            kind,
            result.clone(),
            &world_public_key,
            verifier,
        )?;
        verifier.verify(
            &next.test_session_termination_result_checks(id, kind, &result, current, now)?,
        )?;
        let slot = next
            .test_sessions
            .get_mut(id)
            .and_then(|s| s.termination_slots.get_mut(&kind))
            .ok_or_else(invalid)?;
        let OwnerSetupResultV1::Canonical { from_lookup, .. } = &result else {
            return Err(invalid());
        };
        slot.observations.insert(
            if *from_lookup {
                OwnerSetupCanonicalSourceV1::Lookup
            } else {
                OwnerSetupCanonicalSourceV1::Submit
            },
            result.clone(),
        );
        if result.terminal() {
            slot.result = Some(Box::new(result));
        }
        next.validate_persistence()?;
        *self = next;
        Ok(())
    }
}

#[path = "test_session_termination_recovery.rs"]
mod recovery;
