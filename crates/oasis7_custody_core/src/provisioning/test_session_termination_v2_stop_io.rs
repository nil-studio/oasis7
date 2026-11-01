//! Each claim below pays one explicitly named World HTTP, never a whole Native RPC.
//! Callers still need prepaid Native/authentication/VIEW/Actor ports; dispatch stays off.
use super::*;
impl AccountProvisionApprovalState {
    pub fn publish_test_session_stop_v2_original(
        &mut self,
        id: &str,
        original: OwnerSetupOriginalV1,
        current: &TestSessionTerminationCurrent<'_, '_, '_>,
        now: UnixMillis,
        verifier: &mut impl ApprovalSignatureVerifier,
    ) -> Result<()> {
        verifier.verify(&self.test_session_v2_stop_current_checks(id, current, now, None)?)?;
        let s = self.test_session_v2_record(id)?;
        let slot = s.termination_state.stop.as_ref().ok_or_else(invalid)?;
        if now.get() >= slot.recovery.original_until.get()
            || current.live.source_operation_id != slot.recovery.source_operation_id
        {
            return Err(invalid());
        }
        if let Some(old) = &slot.signed {
            return if old.as_ref() == &original {
                Ok(())
            } else {
                Err(ProvisionApprovalError::Conflict)
            };
        }
        let mut check = slot.clone();
        check.signed = Some(Box::new(original.clone()));
        check.validate_shape()?;
        let mut next = self.clone();
        next.test_session_v2_record_mut(id)?
            .termination_state
            .stop
            .as_mut()
            .ok_or_else(invalid)?
            .signed = Some(Box::new(original));
        next.validate_persistence()?;
        *self = next;
        Ok(())
    }
    /// Canonical Stop write. The second invocation is rejected even if the first reply was lost.
    pub fn claim_test_session_stop_v2_world_submit(
        &mut self,
        id: &str,
        attempt: &str,
        current: &TestSessionTerminationCurrent<'_, '_, '_>,
        now: UnixMillis,
        verifier: &mut impl ApprovalSignatureVerifier,
    ) -> Result<OwnerSetupOriginalV1> {
        verifier.verify(&self.test_session_v2_stop_current_checks(id, current, now, None)?)?;
        self.claim_stop_v2_world(id, attempt, current.live, now, false)
    }
    /// Original lookup only: retain all live Account/Source/Native checks, but no initialActive World predicate.
    pub fn claim_known_test_session_stop_v2_world_lookup(
        &mut self,
        id: &str,
        attempt: &str,
        current: &TestSessionLiveCurrent<'_, '_>,
        now: UnixMillis,
        verifier: &mut impl ApprovalSignatureVerifier,
    ) -> Result<oasis7_client_api::world_service_wire::AuthenticatedLookup> {
        verifier.verify(&self.test_session_v2_live_scope(
            id,
            SessionPurposeV1::TerminationPlan,
            current,
            now,
        )?)?;
        let original = self.claim_stop_v2_world(id, attempt, current, now, true)?;
        let OwnerSetupOriginalV1::Canonical { lookup, .. } = original else {
            return Err(invalid());
        };
        Ok(*lookup)
    }
    fn claim_stop_v2_world(
        &mut self,
        id: &str,
        attempt: &str,
        current: &TestSessionLiveCurrent<'_, '_>,
        now: UnixMillis,
        lookup: bool,
    ) -> Result<OwnerSetupOriginalV1> {
        let s = self.test_session_v2_local(id, SessionPurposeV1::TerminationPlan, now)?;
        if s.closing.is_none()
            || s.source_funding.get(current.source_operation_id)
                != Some(&SessionSourceFundingV1::TerminationPlan)
        {
            return Err(invalid());
        }
        self.test_session_v2_transport_id_available(id, attempt)?;
        if !id_nonempty(attempt) {
            return Err(invalid());
        }
        let slot = s.termination_state.stop.as_ref().ok_or_else(invalid)?;
        if slot.recovery.admitted.is_some()
            || if lookup {
                slot.recovery.submit_claim.is_none() || slot.recovery.lookup_claim.is_some()
            } else {
                slot.recovery.submit_claim.is_some()
                    || now.get() >= slot.recovery.original_until.get()
            }
        {
            return Err(ProvisionApprovalError::Conflict);
        }
        slot.validate_shape()?;
        let original = slot.signed.as_ref().ok_or_else(invalid)?.as_ref().clone();
        let root = s
            .roots
            .get(&SessionPurposeV1::TerminationPlan)
            .ok_or_else(invalid)?;
        let index = 24u64
            .checked_sub(root.budgets.transport.remaining().map_err(|_| invalid())?)
            .ok_or_else(invalid)?;
        let claim = TerminationStopWorldClaimV2 {
            attempt_id: attempt.into(),
            claimed_at: now,
            paid_index: u32::try_from(index).map_err(|_| invalid())?,
            original_digest: digest(&("oasis7.custody.stop-original.v2", &original))?,
            source_operation_id: current.source_operation_id.into(),
            source_record_digest: digest(
                s.sources
                    .get(current.source_operation_id)
                    .ok_or_else(invalid)?,
            )?,
            native_lifetime: current.lifetime.clone(),
        };
        let mut next = self.clone();
        let s = next.test_session_v2_record_mut(id)?;
        s.roots
            .get_mut(&SessionPurposeV1::TerminationPlan)
            .ok_or_else(invalid)?
            .budgets
            .transport
            .charge(1)
            .map_err(|_| invalid())?;
        let r = &mut s
            .termination_state
            .stop
            .as_mut()
            .ok_or_else(invalid)?
            .recovery;
        if lookup {
            r.lookup_claim = Some(claim);
        } else {
            r.submit_claim = Some(claim);
        }
        next.validate_persistence()?;
        *self = next;
        Ok(original)
    }
    /// Immutable facts have no fresh-current clock, no state transition to admitted, and no grant.
    pub fn archive_test_session_stop_v2_fact(
        &mut self,
        id: &str,
        result: OwnerSetupResultV1,
    ) -> Result<OwnerSetupResultV1> {
        let s = self.test_session_v2_record(id)?;
        let (header, _) = s.header()?;
        let stop = s.termination_state.stop.as_ref().ok_or_else(invalid)?;
        stop.validate_fact(&header.termination, &result)?;
        let source = result.canonical_fact().ok_or_else(invalid)?.source;
        if let Some(old) = stop.recovery.observations.get(&source) {
            return if old == &result {
                Ok(old.clone())
            } else {
                Err(ProvisionApprovalError::Conflict)
            };
        }
        let mut next = self.clone();
        next.test_session_v2_record_mut(id)?
            .termination_state
            .stop
            .as_mut()
            .ok_or_else(invalid)?
            .recovery
            .observations
            .insert(source, result.clone());
        next.validate_persistence()?;
        *self = next;
        Ok(result)
    }
    /// Historical exact signed fact plus independently fresh full Current. Rejection does not assert Suspended.
    pub fn admit_test_session_stop_v2_fact(
        &mut self,
        id: &str,
        result: &OwnerSetupResultV1,
        current: &TestSessionTerminationCurrent<'_, '_, '_>,
        now: UnixMillis,
        verifier: &mut impl ApprovalSignatureVerifier,
    ) -> Result<()> {
        let s = self.test_session_v2_record(id)?;
        let (header, _) = s.header()?;
        let stop = s.termination_state.stop.as_ref().ok_or_else(invalid)?;
        stop.validate_fact(&header.termination, result)?;
        let source = result.canonical_fact().ok_or_else(invalid)?.source;
        if !result.terminal() || stop.recovery.observations.get(&source) != Some(result) {
            return Err(invalid());
        }
        if let Some(old) = &stop.recovery.admitted {
            if old.result.as_ref() != result {
                return Err(ProvisionApprovalError::Conflict);
            }
        }
        verifier.verify(&self.test_session_v2_stop_current_checks(
            id,
            current,
            now,
            Some(result),
        )?)?;
        let source_record_digest = digest(
            s.sources
                .get(current.live.source_operation_id)
                .ok_or_else(invalid)?,
        )?;
        let admitted = TerminationStopAdmissionV2 {
            result: Box::new(result.clone()),
            world: current.world.clone(),
            source_operation_id: current.live.source_operation_id.into(),
            source_record_digest,
            native_lifetime: current.live.lifetime.clone(),
            admitted_at: now,
        };
        let mut next = self.clone();
        next.test_session_v2_record_mut(id)?
            .termination_state
            .stop
            .as_mut()
            .ok_or_else(invalid)?
            .recovery
            .admitted = Some(Box::new(admitted));
        next.validate_persistence()?;
        *self = next;
        Ok(())
    }
}
fn id_nonempty(v: &str) -> bool {
    !v.is_empty() && v.len() <= 256 && !v.chars().any(char::is_control)
}
