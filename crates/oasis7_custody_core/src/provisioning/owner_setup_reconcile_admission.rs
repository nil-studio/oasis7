use super::*;
impl AccountProvisionApprovalState {
    pub fn owner_setup_reconciles(&self) -> &BTreeMap<String, OwnerSetupReconcileApprovalV1> {
        &self.owner_setup_reconciles
    }
    pub fn owner_setup_reconcile_original(&self, id: &str) -> Option<&ApprovalAdmissionRecord> {
        self.owner_setup_reconciles
            .get(id)
            .and_then(|v| v.book.original(id))
    }
    fn reconcile_scope(
        &self,
        plan: &OwnerSetupReconcilePlanV1,
        current: &ProvisionApprovalContext<'_>,
        now: u64,
    ) -> Result<()> {
        self.validate_reconcile_plan_scope(plan)?;
        let c = current.approval;
        if self.frozen()
            || self.cancellation_operation_id.is_some()
            || c.frozen
            || self.provision.security_epoch().0 == 0
            || c.authority.security_version != self.provision.security_epoch()
            || c.authority.environment != self.draft.entry.environment
            || c.authority.custody_account_id != self.draft.entry.custody_account_id
            || c.identity_namespace != self.draft.entry.manager.identity_namespace
            || c.source.manager_account_id != self.draft.entry.manager.native_account_id
            || c.fence.check_epoch(c.authority.service_epoch.0).is_err()
            || now >= plan.deadline.0
            || now >= c.source.session_expires_at.0
            || now >= c.source.session_idle_expires_at.0
            || current.policy.version != self.draft.entry.bootstrap_config_version.0
            || current.world_identity != &plan.world_identity
            || current.native_identity_namespace
                != self
                    .participant_provisions
                    .values()
                    .find(|v| v.result.is_some())
                    .ok_or_else(invalid)?
                    .descriptor
                    .native_binding
                    .identity_namespace
            || !current.policy.managers.iter().any(|m| {
                m.principal == self.draft.entry.manager
                    && m.environments.contains(&self.draft.entry.environment)
            })
        {
            return Err(invalid());
        }
        Ok(())
    }
    pub(super) fn validate_reconcile_plan_scope(
        &self,
        p: &OwnerSetupReconcilePlanV1,
    ) -> Result<()> {
        p.validate()?;
        let setup = self.owner_setups.get(&p.setup_id).ok_or_else(invalid)?;
        let o = setup.stages.get(&p.stage).ok_or_else(invalid)?;
        let original = o.original.as_ref().ok_or_else(invalid)?;
        let correlation = match original {
            OwnerSetupOriginalV1::Canonical { submit, .. } => digest(&submit.correlation)?,
            _ => digest(original)?,
        };
        if p.provision_id != self.draft.entry.provision_id
            || p.setup_plan_digest != setup.plan.digest()?
            || p.original_digest != digest(original)?
            || p.correlation_digest != correlation
            || p.world_identity != setup.plan.world_identity
            || p.owner_key_ref != setup.plan.owner_key_ref
            || p.executor_key_ref != setup.plan.executor_key_ref
            || p.original_deadline != setup.plan.deadline
            || o.claimed_at.is_none()
        {
            return Err(invalid());
        }
        Ok(())
    }
    pub fn begin_owner_setup_reconcile_checks(
        &self,
        a: &OwnerSetupReconcileAdmissionV1,
        current: &ProvisionApprovalContext<'_>,
        now: u64,
    ) -> Result<ExactSignatureChecks> {
        self.reconcile_scope(&a.plan, current, now)?;
        if self
            .owner_setup_outbox(&a.plan.setup_id, a.plan.stage)
            .and_then(|o| o.result.as_ref())
            .is_some_and(|r| r.terminal())
        {
            return Err(invalid());
        }
        self.check_owner_setup_proof_unique(&a.input)?;
        if a.input.operation_id != a.plan.reconcile_id
            || a.input.request.intent != a.plan.intent()?
            || a.input.request.context != current.approval.authority
        {
            return Err(invalid());
        }
        let empty = ApprovalAdmissionBook::default();
        Ok(self
            .owner_setup_reconciles
            .get(&a.plan.reconcile_id)
            .map_or(&empty, |v| &v.book)
            .begin_checks(&a.input, &a.verify_nonce, current.approval, now)?)
    }
    pub fn begin_owner_setup_reconcile(
        &mut self,
        a: OwnerSetupReconcileAdmissionV1,
        current: &ProvisionApprovalContext<'_>,
        now: u64,
        verifier: &mut impl ApprovalSignatureVerifier,
    ) -> Result<ApprovalAdmissionRecord> {
        verifier.verify(&self.begin_owner_setup_reconcile_checks(&a, current, now)?)?;
        let mut next = self.clone();
        let v = next
            .owner_setup_reconciles
            .entry(a.plan.reconcile_id.clone())
            .or_insert_with(|| OwnerSetupReconcileApprovalV1 {
                plan: a.plan.clone(),
                book: ApprovalAdmissionBook::default(),
                transport: None,
                source: None,
                session: None,
                attempts: BTreeMap::new(),
            });
        if v.plan != a.plan {
            return Err(ProvisionApprovalError::Conflict);
        }
        let result = v
            .book
            .begin(a.input, a.verify_nonce, current.approval, now, verifier)?;
        next.validate_persistence()?;
        *self = next;
        Ok(result)
    }
    pub fn commit_owner_setup_reconcile_checks(
        &self,
        id: &str,
        signed: &SignedApprovalVerificationV1,
        current: &ProvisionApprovalContext<'_>,
        now: u64,
    ) -> Result<ExactSignatureChecks> {
        let v = self.owner_setup_reconciles.get(id).ok_or_else(invalid)?;
        self.reconcile_scope(&v.plan, current, now)?;
        Ok(v.book.commit_checks(id, signed, current.approval, now)?)
    }
    pub fn commit_owner_setup_reconcile(
        &mut self,
        commit: CommitOwnerSetupV1,
        scope: &NativeStageScope<'_>,
        current: &ProvisionApprovalContext<'_>,
        now: u64,
        verifier: &mut impl ApprovalSignatureVerifier,
    ) -> Result<OwnerSetupReconcileApprovalV1> {
        verifier.verify(&self.commit_owner_setup_reconcile_checks(
            &commit.id,
            &commit.signed,
            current,
            now,
        )?)?;
        let id = commit.id.as_str();
        if let Some(v) = self
            .owner_setup_reconciles
            .get(id)
            .filter(|v| v.transport.is_some())
        {
            return Ok(v.clone());
        }
        if !hex64(&commit.source_nonce) {
            return Err(invalid());
        }
        let (session, status) = self.participant_session(scope)?;
        let binding = &self
            .participant_provisions
            .values()
            .find(|v| v.result.is_some())
            .ok_or_else(invalid)?
            .descriptor
            .native_binding;
        if &status.status.receipt != binding {
            return Err(invalid());
        }
        let mut next = self.clone();
        let v = next
            .owner_setup_reconciles
            .get_mut(id)
            .ok_or_else(invalid)?;
        v.book
            .commit(id, commit.signed, current.approval, now, verifier)?;
        let mut budget = BudgetLedger::new(v.plan.transport_limit.0);
        budget.charge(8).map_err(|_| invalid())?;
        v.transport = Some(budget);
        v.session = Some(session);
        v.source = Some(new_source(
            id,
            "initial",
            commit.source_nonce,
            v.plan.deadline,
            current.approval,
            now,
        )?);
        let result = v.clone();
        next.validate_persistence()?;
        *self = next;
        Ok(result)
    }
    pub(super) fn reconcile_live(
        &self,
        id: &str,
        current: &ProvisionApprovalContext<'_>,
        now: u64,
    ) -> Result<()> {
        let v = self.owner_setup_reconciles.get(id).ok_or_else(invalid)?;
        self.reconcile_scope(&v.plan, current, now)?;
        let r = v
            .book
            .original(id)
            .filter(|r| r.consumed_at().is_some_and(|t| t <= now))
            .ok_or_else(invalid)?;
        r.check_recorded_context(current.approval)?;
        Ok(())
    }
}
pub(super) fn new_source(
    id: &str,
    attempt: &str,
    nonce: String,
    deadline: DecimalU64,
    current: &ApprovalAdmissionContext,
    now: u64,
) -> Result<ProvisionOriginInspectionV1> {
    if !hex64(&nonce)
        || now >= deadline.0
        || now >= current.source.session_expires_at.0
        || now >= current.source.session_idle_expires_at.0
    {
        return Err(invalid());
    }
    Ok(ProvisionOriginInspectionV1 {
        operation_id: digest(&("oasis7.custody.setup-reconcile-source.v1", id, attempt))?,
        verify_nonce: nonce,
        created_at: DecimalU64(now),
        expires_at: DecimalU64(
            now.checked_add(30_000)
                .ok_or_else(invalid)?
                .min(deadline.0)
                .min(current.source.session_expires_at.0)
                .min(current.source.session_idle_expires_at.0),
        ),
        claimed_at: None,
        request_bytes: None,
        verification: None,
        verified_at: None,
    })
}
