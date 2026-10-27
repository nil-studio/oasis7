use super::*;
impl AccountProvisionApprovalState {
    pub fn reserve_owner_setup_reconcile_attempt(
        &mut self,
        id: &str,
        attempt_id: &str,
        nonce: &str,
        origin_nonce: &str,
        now: u64,
    ) -> Result<OwnerSetupReconcileAttemptV1> {
        if !id_valid(attempt_id)
            || !hex64(nonce)
            || !hex64(origin_nonce)
            || nonce == origin_nonce
            || self.frozen()
            || self.cancellation_operation_id.is_some()
        {
            return Err(invalid());
        }
        let v = self.owner_setup_reconciles.get(id).ok_or_else(invalid)?;
        let r = v
            .book
            .original(id)
            .filter(|r| r.consumed_at().is_some_and(|t| t <= now))
            .ok_or_else(invalid)?;
        if now >= v.plan.deadline.0
            || v.source.as_ref().is_none_or(|s| s.verification.is_none())
            || self.provision.security_epoch() != r.input().request.context.security_version
        {
            return Err(invalid());
        }
        if let Some(old) = v.attempts.get(attempt_id) {
            return if old.nonce == nonce && old.source.verify_nonce == origin_nonce {
                Ok(old.clone())
            } else {
                Err(ProvisionApprovalError::Conflict)
            };
        }
        if v.attempts.len() >= usize::from(v.plan.attempt_limit)
            || v.attempts
                .values()
                .any(|a| a.nonce == nonce || a.source.verify_nonce == origin_nonce)
            || self
                .owner_setup_outbox(&v.plan.setup_id, v.plan.stage)
                .and_then(|o| o.result.as_ref())
                .is_some_and(|r| r.terminal())
        {
            return Err(invalid());
        }
        let lookup = match self
            .owner_setup_outbox(&v.plan.setup_id, v.plan.stage)
            .and_then(|o| o.original.as_ref())
        {
            Some(OwnerSetupOriginalV1::Canonical { lookup, .. }) => {
                Some(serde_json::to_string(lookup).map_err(|_| invalid())?)
            }
            _ => None,
        };
        let mut next = self.clone();
        let v = next
            .owner_setup_reconciles
            .get_mut(id)
            .ok_or_else(invalid)?;
        v.transport
            .as_mut()
            .ok_or_else(invalid)?
            .charge(OWNER_SETUP_RECONCILE_ATTEMPT_TRANSPORT_COST)
            .map_err(|_| invalid())?;
        let context = v.book.original(id).ok_or_else(invalid)?.recorded_context();
        let source = admission::new_source(
            id,
            attempt_id,
            origin_nonce.into(),
            v.plan.deadline,
            context,
            now,
        )?;
        let a = OwnerSetupReconcileAttemptV1 {
            attempt_id: attempt_id.into(),
            nonce: nonce.into(),
            original_lookup_bytes: lookup,
            reserved_at: DecimalU64(now),
            source,
            oracle_reads: 0,
            access_claimed_at: None,
            access: None,
            access_recorded_at: None,
            observation: None,
            goal_request: None,
            published_at: None,
            admission_reserved_at: None,
            admission: None,
            admitted_at: None,
            lookup_reserved_at: None,
            lookup_claimed_at: None,
            fact: None,
            completion_reserved_at: None,
            result: None,
            result_session: None,
            result_lookup_session: None,
            final_world: None,
            completed_at: None,
        };
        v.attempts.insert(attempt_id.into(), a.clone());
        next.validate_persistence()?;
        *self = next;
        Ok(a)
    }
    pub fn owner_setup_reconcile_attempt(
        &self,
        id: &str,
        attempt: &str,
    ) -> Option<&OwnerSetupReconcileAttemptV1> {
        self.owner_setup_reconciles
            .get(id)
            .and_then(|v| v.attempts.get(attempt))
    }
    pub fn claim_owner_setup_reconcile_source(
        &mut self,
        id: &str,
        attempt: Option<&str>,
        current: &ProvisionApprovalContext<'_>,
        now: u64,
    ) -> Result<ApprovalVerifyRequestV1> {
        self.reconcile_live(id, current, now)?;
        let mut next = self.clone();
        let v = next
            .owner_setup_reconciles
            .get_mut(id)
            .ok_or_else(invalid)?;
        let r = v.book.original(id).ok_or_else(invalid)?;
        let source = match attempt {
            Some(a) => &mut v.attempts.get_mut(a).ok_or_else(invalid)?.source,
            None => v.source.as_mut().ok_or_else(invalid)?,
        };
        if source.claimed_at.is_some() || now >= source.expires_at.0 {
            return Err(invalid());
        }
        let mut request = r.native_verify_request()?;
        request.purpose = VerificationPurposeV1::InspectOrigin;
        request.verify_nonce = source.verify_nonce.clone();
        source.claimed_at = Some(DecimalU64(now));
        source.request_bytes = Some(serde_json::to_string(&request).map_err(|_| invalid())?);
        next.validate_persistence()?;
        *self = next;
        Ok(request)
    }
    pub fn owner_setup_reconcile_source_checks(
        &self,
        id: &str,
        attempt: Option<&str>,
        signed: &SignedApprovalVerificationV1,
        current: &ProvisionApprovalContext<'_>,
        now: u64,
    ) -> Result<ExactSignatureChecks> {
        self.reconcile_live(id, current, now)?;
        let v = self.owner_setup_reconciles.get(id).ok_or_else(invalid)?;
        let source = match attempt {
            Some(a) => &v.attempts.get(a).ok_or_else(invalid)?.source,
            None => v.source.as_ref().ok_or_else(invalid)?,
        };
        super::super::source::validate_inspection_record(
            v.book.original(id).ok_or_else(invalid)?,
            source,
            signed,
            current.approval,
            now,
        )
    }
    pub fn complete_owner_setup_reconcile_source(
        &mut self,
        id: &str,
        attempt: Option<&str>,
        signed: SignedApprovalVerificationV1,
        current: &ProvisionApprovalContext<'_>,
        now: u64,
        verifier: &mut impl ApprovalSignatureVerifier,
    ) -> Result<()> {
        verifier.verify(
            &self.owner_setup_reconcile_source_checks(id, attempt, &signed, current, now)?,
        )?;
        let mut next = self.clone();
        let v = next
            .owner_setup_reconciles
            .get_mut(id)
            .ok_or_else(invalid)?;
        let s = match attempt {
            Some(a) => &mut v.attempts.get_mut(a).ok_or_else(invalid)?.source,
            None => v.source.as_mut().ok_or_else(invalid)?,
        };
        s.verification = Some(signed);
        s.verified_at = Some(DecimalU64(now));
        next.validate_persistence()?;
        *self = next;
        Ok(())
    }
    pub fn owner_setup_reconcile_checks(
        &self,
        id: &str,
        attempt: &str,
        current: &ProvisionApprovalContext<'_>,
        now: u64,
    ) -> Result<ExactSignatureChecks> {
        let s = &self
            .owner_setup_reconcile_attempt(id, attempt)
            .ok_or_else(invalid)?
            .source;
        self.owner_setup_reconcile_source_checks(
            id,
            Some(attempt),
            s.verification.as_ref().ok_or_else(invalid)?,
            current,
            now,
        )
    }
}
fn id_valid(value: &str) -> bool {
    super::id(value)
}
