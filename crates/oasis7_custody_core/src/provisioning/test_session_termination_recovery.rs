//! Readonly recovery of an already published and claimed signed original.
use super::*;
impl AccountProvisionApprovalState {
    pub fn known_test_session_termination_lookup_checks(
        &self,
        id: &str,
        kind: TerminationOperationV1,
        current: &TestSessionLiveCurrent<'_, '_>,
        now: UnixMillis,
    ) -> Result<ExactSignatureChecks> {
        self.require_session_source_funding(
            id,
            current.source_operation_id,
            SessionSourceFundingV1::TerminationPlan,
        )?;
        let s = self.test_session_local(id, SessionPurposeV1::TerminationPlan, now)?;
        let slot = s.termination_slots.get(&kind).ok_or_else(invalid)?;
        if s.closing.is_none() || slot.claimed_at.is_none() || slot.result.is_some() {
            return Err(invalid());
        }
        let original = slot.signed.as_ref().ok_or_else(invalid)?;
        self.validate_termination_signed(s, slot, original)?;
        let checks =
            self.test_session_live_checks(id, SessionPurposeV1::TerminationPlan, current, now)?;
        Ok(append_owner_setup_checks(
            checks,
            OwnerSetupSignatureFact::Original(
                original,
                &s.termination.initial_fence.owner_public_key,
            ),
        )?)
    }
    pub fn claim_known_test_session_termination_lookup(
        &mut self,
        id: &str,
        kind: TerminationOperationV1,
        attempt: &str,
        current: &TestSessionLiveCurrent<'_, '_>,
        now: UnixMillis,
        verifier: &mut impl ApprovalSignatureVerifier,
    ) -> Result<oasis7_client_api::world_service_wire::AuthenticatedLookup> {
        verifier
            .verify(&self.known_test_session_termination_lookup_checks(id, kind, current, now)?)?;
        let s = self.test_sessions.get(id).ok_or_else(invalid)?;
        let o = s.termination_slots.get(&kind).ok_or_else(invalid)?;
        if !id_nonempty(attempt)
            || o.lookup_claimed_at.is_some()
            || o.lookup_paid_claim.is_some()
            || Self::session_transport_attempt_occupied(s, attempt)
        {
            return Err(ProvisionApprovalError::Conflict);
        }
        let original = o.signed.as_ref().ok_or_else(invalid)?.as_ref().clone();
        let binding = digest(&(
            "oasis7.custody.termination-known-lookup.v1",
            id,
            kind,
            &original,
        ))?;
        let mut next = self.clone();
        let s = next.test_sessions.get_mut(id).ok_or_else(invalid)?;
        s.roots
            .get_mut(&SessionPurposeV1::TerminationPlan)
            .ok_or_else(invalid)?
            .budgets
            .transport
            .charge(1)
            .map_err(|_| invalid())?;
        let o = s.termination_slots.get_mut(&kind).ok_or_else(invalid)?;
        let paid_transport_index =
            u32::try_from(o.transport_attempts.len()).map_err(|_| invalid())?;
        o.transport_attempts.push(attempt.into());
        o.lookup_claimed_at = Some(now);
        o.lookup_paid_claim = Some(TerminationLookupClaimV1 {
            attempt_id: attempt.into(),
            claimed_at: now,
            original_digest: binding,
            paid_transport_index,
        });
        let OwnerSetupOriginalV1::Canonical { lookup, .. } = original else {
            return Err(invalid());
        };
        next.validate_persistence()?;
        *self = next;
        Ok(*lookup)
    }
    pub fn test_session_termination_archive_checks(
        &self,
        id: &str,
        kind: TerminationOperationV1,
        result: &OwnerSetupResultV1,
        world_public_key: &str,
    ) -> Result<ExactSignatureChecks> {
        let s = self.test_sessions.get(id).ok_or_else(invalid)?;
        if world_public_key != s.termination.world.service_public_key
            || world_public_key != self.draft.world_identity.service_public_key
        {
            return Err(invalid());
        }
        let o = s.termination_slots.get(&kind).ok_or_else(invalid)?;
        self.validate_termination_result(s, o, result)?;
        Ok(termination_archive_signature_checks(
            id,
            kind,
            o.signed.as_ref().ok_or_else(invalid)?,
            result,
            world_public_key,
        )?)
    }
    pub fn archive_test_session_termination_fact(
        &mut self,
        id: &str,
        kind: TerminationOperationV1,
        result: OwnerSetupResultV1,
        world_public_key: &str,
        verifier: &mut impl ApprovalSignatureVerifier,
    ) -> Result<OwnerSetupResultV1> {
        verifier.verify(&self.test_session_termination_archive_checks(
            id,
            kind,
            &result,
            world_public_key,
        )?)?;
        let source = result.canonical_fact().ok_or_else(invalid)?.source;
        let o = self
            .test_sessions
            .get(id)
            .and_then(|s| s.termination_slots.get(&kind))
            .ok_or_else(invalid)?;
        if let Some(old) = o.observations.get(&source) {
            return if old == &result {
                Ok(old.clone())
            } else {
                Err(ProvisionApprovalError::Conflict)
            };
        }
        let mut next = self.clone();
        next.test_sessions
            .get_mut(id)
            .and_then(|s| s.termination_slots.get_mut(&kind))
            .ok_or_else(invalid)?
            .observations
            .insert(source, result.clone());
        next.validate_persistence()?;
        *self = next;
        Ok(result)
    }
    pub(in crate::provisioning) fn termination_attempt_occupied(
        s: &TestSessionAuthorityV1,
        attempt: &str,
    ) -> bool {
        s.termination_slots
            .values()
            .any(|o| o.transport_attempts.iter().any(|v| v == attempt))
    }
    pub(in crate::provisioning) fn session_transport_attempt_occupied(
        s: &TestSessionAuthorityV1,
        attempt: &str,
    ) -> bool {
        Self::termination_attempt_occupied(s, attempt)
            || s.sources.contains_key(attempt)
            || s.source_funding.contains_key(attempt)
            || s.actions
                .values()
                .any(|a| a.transport_attempts.iter().any(|v| v == attempt))
            || s.purpose_transports.contains_key(attempt)
            || s.purpose_http_admissions.contains_key(attempt)
            || s.control_transports.contains_key(attempt)
            || s.client_http_admissions.contains_key(attempt)
    }
}
