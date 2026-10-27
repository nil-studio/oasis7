use super::*;
impl AccountProvisionApprovalState {
    pub(in crate::provisioning) fn validate_owner_setup_reconcile_persistence(&self) -> Result<()> {
        if self.owner_setup_reconciles.len() > 32 {
            return Err(invalid());
        }
        for (id, v) in &self.owner_setup_reconciles {
            self.validate_reconcile_plan_scope(&v.plan)?;
            let r = v.book.original(id).ok_or_else(invalid)?;
            self.check_owner_setup_proof_unique(r.input())?;
            if id != &v.plan.reconcile_id
                || v.book.records().count() != 1
                || r.input().operation_id != *id
                || r.input().request.intent != v.plan.intent()?
                || r.input().request.context.security_version.0 == 0
                || r.input().request.context.custody_account_id
                    != self.draft.entry.custody_account_id
                || r.input().request.context.environment != self.draft.entry.environment
            {
                return Err(invalid());
            }
            if r.consumed_at().is_none() {
                if v.transport.is_some()
                    || v.source.is_some()
                    || v.session.is_some()
                    || !v.attempts.is_empty()
                {
                    return Err(invalid());
                }
                continue;
            }
            let context = r.recorded_context();
            let source = v.source.as_ref().ok_or_else(invalid)?;
            if v.session.is_none() || source.created_at.0 != r.consumed_at().ok_or_else(invalid)? {
                return Err(invalid());
            }
            validate_source(id, "initial", r, source, v.plan.deadline)?;
            let mut budget = BudgetLedger::new(v.plan.transport_limit.0);
            budget.charge(8).map_err(|_| invalid())?;
            if v.attempts.len() > usize::from(v.plan.attempt_limit)
                || !v.attempts.is_empty() && source.verification.is_none()
            {
                return Err(invalid());
            }
            let mut nonces = BTreeSet::new();
            nonces.insert(source.verify_nonce.clone());
            for (attempt, a) in &v.attempts {
                budget
                    .charge(OWNER_SETUP_RECONCILE_ATTEMPT_TRANSPORT_COST)
                    .map_err(|_| invalid())?;
                if attempt != &a.attempt_id
                    || !id_valid(attempt)
                    || !hex64(&a.nonce)
                    || !nonces.insert(a.nonce.clone())
                    || !nonces.insert(a.source.verify_nonce.clone())
                    || a.reserved_at.0 < source.created_at.0
                    || a.reserved_at.0 >= v.plan.deadline.0
                    || a.source.created_at != a.reserved_at
                    || a.oracle_reads > 5
                {
                    return Err(invalid());
                }
                validate_source(id, attempt, r, &a.source, v.plan.deadline)?;
                let expected_lookup = match self
                    .owner_setup_outbox(&v.plan.setup_id, v.plan.stage)
                    .and_then(|o| o.original.as_ref())
                {
                    Some(OwnerSetupOriginalV1::Canonical { lookup, .. }) => {
                        Some(serde_json::to_string(lookup).map_err(|_| invalid())?)
                    }
                    _ => None,
                };
                if a.original_lookup_bytes != expected_lookup
                    || a.oracle_reads > 0 && a.source.verification.is_none()
                    || a.access_claimed_at.is_some() && a.oracle_reads < 2
                    || a.access.is_some() != a.access_recorded_at.is_some()
                    || a.access.is_some() && a.oracle_reads < 3
                {
                    return Err(invalid());
                }
                if let Some(t) = a.access_claimed_at {
                    timed(a, t.0)?;
                }
                if let Some(e) = &a.access {
                    let at = a.access_recorded_at.ok_or_else(invalid)?.0;
                    timed(a, at)?;
                    let claimed = a.access_claimed_at.ok_or_else(invalid)?.0;
                    if claimed > at {
                        return Err(invalid());
                    }
                    self.validate_reconcile_access_history(
                        id,
                        attempt,
                        e,
                        at,
                        context.authority.service_epoch,
                    )?;
                }
                if a.observation.is_some() != a.published_at.is_some()
                    || a.goal_request.is_some()
                        != (a.observation.is_some()
                            && v.plan.stage == OwnerSetupStageV1::FinalObservation)
                {
                    return Err(invalid());
                }
                if let Some(o) = &a.observation {
                    let at = a.published_at.ok_or_else(invalid)?.0;
                    timed(a, at)?;
                    if a.oracle_reads != 5 || a.access_recorded_at.is_none_or(|t| t.0 > at) {
                        return Err(invalid());
                    }
                    self.validate_reconcile_published_history(id, attempt, o)?;
                    self.validate_reconcile_access_history(
                        id,
                        attempt,
                        a.access.as_ref().ok_or_else(invalid)?,
                        at,
                        context.authority.service_epoch,
                    )?;
                }
                for t in [
                    a.admission_reserved_at,
                    a.lookup_reserved_at,
                    a.completion_reserved_at,
                ]
                .into_iter()
                .flatten()
                {
                    timed(a, t.0)?;
                    if a.published_at.is_none_or(|p| p.0 > t.0) {
                        return Err(invalid());
                    }
                }
                if a.admission.is_some() != a.admitted_at.is_some() {
                    return Err(invalid());
                }
                if let Some(e) = &a.admission {
                    let at = a.admitted_at.ok_or_else(invalid)?.0;
                    timed(a, at)?;
                    if a.admission_reserved_at.is_none_or(|t| t.0 > at) {
                        return Err(invalid());
                    }
                    self.validate_reconcile_world(id, attempt, &e.world)?;
                    self.reconcile_session(
                        id,
                        &runtime::reconcile_nonce(id, attempt, &a.nonce, "preflight")?,
                        &e.session,
                        at,
                        context.authority.service_epoch,
                    )?;
                    if e.session.status.status.observed_at.0 < a.published_at.ok_or_else(invalid)?.0
                    {
                        return Err(invalid());
                    }
                }
                if let Some(at) = a.lookup_claimed_at {
                    timed(a, at.0)?;
                    if a.lookup_reserved_at.is_none_or(|t| t.0 > at.0)
                        || a.admitted_at
                            .is_none_or(|t| t.0 > at.0 || at.0 - t.0 > 5000)
                        || a.original_lookup_bytes.is_none()
                    {
                        return Err(invalid());
                    }
                    self.validate_reconcile_access_history(
                        id,
                        attempt,
                        a.access.as_ref().ok_or_else(invalid)?,
                        at.0,
                        context.authority.service_epoch,
                    )?;
                }
                if let Some(fact) = &a.fact {
                    self.owner_setup_reconcile_archive_checks(
                        id,
                        attempt,
                        fact,
                        &OwnerSetupArchiveContext {
                            world_public_key: &v.plan.world_identity.service_public_key,
                        },
                    )?;
                }
                if a.result.is_some() != a.completed_at.is_some()
                    || a.result.is_some() != a.result_session.is_some()
                    || a.result.is_some() != a.final_world.is_some()
                    || a.result.is_none() && a.result_lookup_session.is_some()
                {
                    return Err(invalid());
                }
                if let Some(result) = &a.result {
                    let at = a.completed_at.ok_or_else(invalid)?.0;
                    timed(a, at)?;
                    if a.completion_reserved_at.is_none_or(|t| t.0 > at)
                        || a.admitted_at.is_none_or(|t| t.0 > at || at - t.0 > 5000)
                    {
                        return Err(invalid());
                    }
                    let e = OwnerSetupReconcileCompletionV1 {
                        result: result.clone(),
                        session: a.result_session.clone().ok_or_else(invalid)?,
                        lookup_session: a.result_lookup_session.clone(),
                        world: a.final_world.clone().ok_or_else(invalid)?,
                    };
                    self.validate_reconcile_world(id, attempt, &e.world)?;
                    self.validate_reconcile_result(id, attempt, &e)?;
                    if e.session.status.status.observed_at.0
                        < a.completion_reserved_at.ok_or_else(invalid)?.0
                    {
                        return Err(invalid());
                    }
                    match &e.result {
                        OwnerSetupResultV1::Canonical { .. } => {
                            let lookup = e.lookup_session.as_ref().ok_or_else(invalid)?;
                            if lookup.status.status.observed_at.0
                                < a.lookup_claimed_at.ok_or_else(invalid)?.0
                            {
                                return Err(invalid());
                            }
                            self.reconcile_session(
                                id,
                                &runtime::reconcile_nonce(id, attempt, &a.nonce, "lookup")?,
                                lookup,
                                at,
                                context.authority.service_epoch,
                            )?;
                        }
                        _ => {
                            if e.lookup_session.is_some() {
                                return Err(invalid());
                            }
                        }
                    }
                    self.reconcile_session(
                        id,
                        &runtime::reconcile_nonce(id, attempt, &a.nonce, "poststate")?,
                        &e.session,
                        at,
                        context.authority.service_epoch,
                    )?;
                }
            }
            if v.transport.as_ref() != Some(&budget) {
                return Err(invalid());
            }
        }
        Ok(())
    }
    pub(in crate::provisioning) fn validate_reconciled_stage_result(
        &self,
        id: &str,
        stage: OwnerSetupStageV1,
        o: &OwnerSetupOutboxV1,
    ) -> Result<()> {
        let Some(OwnerSetupResultProvenanceV1::Reconcile {
            reconcile_id,
            attempt_id,
        }) = &o.result_provenance
        else {
            return Err(invalid());
        };
        let v = self
            .owner_setup_reconciles
            .get(reconcile_id)
            .ok_or_else(invalid)?;
        let a = v.attempts.get(attempt_id).ok_or_else(invalid)?;
        if v.plan.setup_id != id
            || v.plan.stage != stage
            || a.result != o.result
            || a.result_session != o.session
            || a.completed_at != o.completed_at
            || a.result.as_ref().is_none_or(|r| !r.terminal())
        {
            return Err(invalid());
        }
        self.validate_reconcile_result(
            reconcile_id,
            attempt_id,
            &OwnerSetupReconcileCompletionV1 {
                result: a.result.clone().ok_or_else(invalid)?,
                session: a.result_session.clone().ok_or_else(invalid)?,
                lookup_session: a.result_lookup_session.clone(),
                world: a.final_world.clone().ok_or_else(invalid)?,
            },
        )
    }
    fn validate_reconcile_access_history(
        &self,
        id: &str,
        attempt: &str,
        e: &OwnerSetupAccessEvidenceV1,
        now: u64,
        epoch: DecimalU64,
    ) -> Result<()> {
        use super::super::super::participant::validation::{
            admission_for_descriptor, policy_for_descriptor,
        };
        let a = self
            .owner_setup_reconcile_attempt(id, attempt)
            .ok_or_else(invalid)?;
        let d = &self
            .participant_provisions
            .values()
            .find(|p| p.result.is_some())
            .ok_or_else(invalid)?
            .descriptor;
        let n = self.owner_setup_reconcile_nonces(id, attempt)?;
        policy_for_descriptor(d, &n.policy, &e.policy)?;
        admission_for_descriptor(d, &e.policy, &e.admission)?;
        for (nonce, status) in [
            (&n.policy_session, &e.policy_session),
            (&n.authorize_session, &e.authorize_session),
        ] {
            if status.status.status.observed_at.0 < a.access_claimed_at.ok_or_else(invalid)?.0 {
                return Err(invalid());
            }
            self.reconcile_session(id, nonce, status, now, epoch)?;
        }
        Ok(())
    }
    fn validate_reconcile_published_history(
        &self,
        id: &str,
        attempt: &str,
        o: &ws::participant_discovery::ParticipantOwnerViewRequestV1,
    ) -> Result<()> {
        let v = self.owner_setup_reconciles.get(id).ok_or_else(invalid)?;
        let a = v.attempts.get(attempt).ok_or_else(invalid)?;
        let p = self
            .participant_provisions
            .values()
            .find(|p| p.result.is_some())
            .ok_or_else(invalid)?;
        let mut read = p.descriptor.owner_read.clone();
        read.observation_nonce = Some(runtime::reconcile_nonce(
            id,
            attempt,
            &a.nonce,
            "preflight",
        )?);
        read.deadline_unix_ms = Some(v.plan.deadline.0);
        read.fixed_commit = None;
        read.formal_anchor = None;
        read.min_commit = None;
        if o.owner != p.bundle.as_ref().ok_or_else(invalid)?.owner
            || o.admission != a.access.as_ref().ok_or_else(invalid)?.admission
            || o.read.request != read
            || o.read.subject_public_key != p.descriptor.owner_key.public_key
            || o.response_profile
                != Some(ws::participant_discovery::ParticipantOwnerViewProfileV1::OwnerSetupV1)
        {
            return Err(invalid());
        }
        validate_owner_setup_signature_fact(OwnerSetupSignatureFact::Read(
            o,
            &p.descriptor.owner_key.public_key,
        ))?;
        if let Some(goal) = &a.goal_request {
            let OwnerSetupSigningRequestV1::FinalObservation { goal: expected, .. } =
                self.owner_setup_expected(&v.plan.setup_id, v.plan.stage)?
            else {
                return Err(invalid());
            };
            if goal.request != expected
                || goal.subject_public_key != p.descriptor.owner_key.public_key
            {
                return Err(invalid());
            }
            validate_owner_setup_signature_fact(OwnerSetupSignatureFact::GoalRead(
                goal,
                &p.descriptor.owner_key.public_key,
            ))?;
        }
        Ok(())
    }
}
fn id_valid(value: &str) -> bool {
    super::id(value)
}
fn timed(a: &OwnerSetupReconcileAttemptV1, at: u64) -> Result<()> {
    let source = &a.source;
    let observed = source
        .verification
        .as_ref()
        .ok_or_else(invalid)?
        .verification
        .observed_at
        .0;
    if at < a.reserved_at.0 || at >= source.expires_at.0 || observed > at || at - observed > 5000 {
        return Err(invalid());
    }
    Ok(())
}
fn validate_source(
    id: &str,
    attempt: &str,
    r: &ApprovalAdmissionRecord,
    s: &ProvisionOriginInspectionV1,
    deadline: DecimalU64,
) -> Result<()> {
    let expected = admission::new_source(
        id,
        attempt,
        s.verify_nonce.clone(),
        deadline,
        r.recorded_context(),
        s.created_at.0,
    )?;
    if s.operation_id != expected.operation_id
        || s.expires_at != expected.expires_at
        || s.claimed_at.is_some() != s.request_bytes.is_some()
        || s.verification.is_some() != s.verified_at.is_some()
    {
        return Err(invalid());
    }
    if let Some(at) = s.claimed_at {
        if at.0 < s.created_at.0 || at.0 >= s.expires_at.0 {
            return Err(invalid());
        }
        let mut req = r.native_verify_request()?;
        req.purpose = VerificationPurposeV1::InspectOrigin;
        req.verify_nonce = s.verify_nonce.clone();
        if s.request_bytes.as_deref()
            != Some(serde_json::to_string(&req).map_err(|_| invalid())?.as_str())
        {
            return Err(invalid());
        }
    }
    if let (Some(signed), Some(at)) = (&s.verification, s.verified_at) {
        let mut context = r.recorded_context().clone();
        context.source.session_expires_at = signed.verification.source_session_expires_at;
        context.source.session_idle_expires_at = signed.verification.source_session_idle_expires_at;
        r.check_recorded_context(&context)?;
        super::super::source::validate_inspection_record(r, s, signed, &context, at.0)?;
    }
    Ok(())
}
