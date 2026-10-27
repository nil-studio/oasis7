use super::*;
impl OwnerSetupPlanV1 {
    pub fn intent(&self) -> Result<crate::approval::CustodyIntentV1> {
        self.validate()?;
        Ok(crate::approval::CustodyIntentV1::OwnerSetup {
            setup_id: self.setup_id.clone(),
            provision_id: self.provision_id.clone(),
            participant_receipt_digest: self.participant_receipt_digest.clone(),
            world_identity: Box::new(self.world_identity.clone()),
            owner_key_ref: self.owner_key_ref.clone(),
            executor_key_ref: self.executor_key_ref.clone(),
            setup_plan_digest: self.digest()?,
            bootstrap_config_version: self.bootstrap_config_version,
            deadline: self.deadline,
            request_limit: self.request_limit,
            transport_limit: self.transport_limit,
            write_limit: self.write_limit,
        })
    }
}
fn same_proof(a: &ApprovalAdmissionInput, b: &ApprovalAdmissionInput) -> bool {
    let (a, b) = (&a.approval.proof, &b.approval.proof);
    a.nonce == b.nonce
        || (a.identity_namespace == b.identity_namespace
            && a.identity_instance_epoch == b.identity_instance_epoch
            && (a.approval_id == b.approval_id || a.approval_request_id == b.approval_request_id))
}
impl AccountProvisionApprovalState {
    pub fn owner_setups(&self) -> &BTreeMap<String, OwnerSetupApprovalV1> {
        &self.owner_setups
    }
    pub fn owner_setup_plan(&self, id: &str) -> Option<&OwnerSetupPlanV1> {
        self.owner_setups.get(id).map(|v| &v.plan)
    }
    pub fn owner_setup_source(&self, id: &str) -> Option<&ProvisionOriginInspectionV1> {
        self.owner_setups.get(id).and_then(|v| v.source.as_ref())
    }
    pub fn owner_setup_original(&self, id: &str) -> Option<&ApprovalAdmissionRecord> {
        self.owner_setups.get(id).and_then(|v| v.book.original(id))
    }
    pub(in crate::provisioning) fn check_owner_setup_proof_unique(
        &self,
        input: &ApprovalAdmissionInput,
    ) -> Result<()> {
        for r in self.book.records().chain(
            self.managed_logins
                .keys()
                .filter_map(|id| self.managed_login_original(id)),
        ) {
            if same_proof(r.input(), input) {
                return Err(ApprovalAdmissionError::NonceReused.into());
            }
        }
        for (id, v) in &self.owner_setups {
            for r in v.book.records() {
                if id == &input.operation_id && r.input() == input {
                    continue;
                }
                if id == &input.operation_id {
                    return Err(ProvisionApprovalError::Conflict);
                }
                if same_proof(r.input(), input) {
                    return Err(ApprovalAdmissionError::NonceReused.into());
                }
            }
        }
        Ok(())
    }
    fn owner_setup_scope(
        &self,
        input: &ApprovalAdmissionInput,
        plan: &OwnerSetupPlanV1,
        current: &ProvisionApprovalContext<'_>,
        now: u64,
    ) -> Result<()> {
        plan.validate()?;
        input.request.validate(now).map_err(|_| invalid())?;
        let c = current.approval;
        let participant = self
            .participant_provisions
            .values()
            .find(|v| v.result.is_some())
            .ok_or_else(invalid)?;
        let result = participant.result.as_ref().ok_or_else(invalid)?;
        if input.operation_id != plan.setup_id
            || self.frozen()
            || !matches!(
                self.provision.stage(),
                ProvisionStage::ParticipantBound
                    | ProvisionStage::ReadyForSetup
                    | ProvisionStage::Prepared
            )
            || self.provision.security_epoch().0 == 0
            || input.request.context != c.authority
            || c.authority.security_version != self.provision.security_epoch()
            || c.frozen
            || c.fence.check_epoch(c.authority.service_epoch.0).is_err()
            || c.authority.environment != self.draft.entry.environment
            || c.authority.custody_account_id != self.draft.entry.custody_account_id
            || c.identity_namespace != self.draft.entry.manager.identity_namespace
            || c.source.manager_account_id != self.draft.entry.manager.native_account_id
            || current.policy.version != self.draft.entry.bootstrap_config_version.0
            || !current.policy.managers.iter().any(|v| {
                v.principal == self.draft.entry.manager
                    && v.environments.contains(&self.draft.entry.environment)
            })
            || current.native_identity_namespace
                != participant.descriptor.native_binding.identity_namespace
            || plan.provision_id != self.draft.entry.provision_id
            || plan.world_identity != self.draft.world_identity
            || plan.bootstrap_config_version != self.draft.entry.bootstrap_config_version
            || plan.participant_receipt_digest != digest(result)?
            || plan.owner_key_ref != participant.descriptor.owner_key.key_id
            || plan.executor_key_ref
                != self
                    .provision
                    .role_key(Role::Executor)
                    .ok_or_else(invalid)?
                    .key_id
            || plan.executor_public_key
                != self
                    .provision
                    .role_key(Role::Executor)
                    .ok_or_else(invalid)?
                    .public_key
            || input.request.intent != plan.intent()?
        {
            return Err(invalid());
        }
        Ok(())
    }
    pub fn begin_owner_setup_checks(
        &self,
        admission: &OwnerSetupAdmissionV1,
        current: &ProvisionApprovalContext<'_>,
        now: u64,
    ) -> Result<ExactSignatureChecks> {
        self.check_owner_setup_proof_unique(&admission.input)?;
        self.owner_setup_scope(&admission.input, &admission.plan, current, now)?;
        if self
            .owner_setups
            .iter()
            .any(|(id, _)| id != &admission.input.operation_id)
        {
            return Err(ProvisionApprovalError::Conflict);
        }
        let empty = ApprovalAdmissionBook::default();
        Ok(self
            .owner_setups
            .get(&admission.input.operation_id)
            .map_or(&empty, |v| &v.book)
            .begin_checks(
                &admission.input,
                &admission.verify_nonce,
                current.approval,
                now,
            )?)
    }
    pub fn begin_owner_setup(
        &mut self,
        admission: OwnerSetupAdmissionV1,
        current: &ProvisionApprovalContext<'_>,
        now: u64,
        verifier: &mut impl ApprovalSignatureVerifier,
    ) -> Result<ApprovalAdmissionRecord> {
        verifier.verify(&self.begin_owner_setup_checks(&admission, current, now)?)?;
        let mut next = self.clone();
        let id = admission.input.operation_id.clone();
        let v = next
            .owner_setups
            .entry(id)
            .or_insert_with(|| OwnerSetupApprovalV1 {
                plan: admission.plan.clone(),
                book: ApprovalAdmissionBook::default(),
                transport: None,
                source: None,
                session: None,
                stages: BTreeMap::new(),
            });
        if v.plan != admission.plan {
            return Err(ProvisionApprovalError::Conflict);
        }
        let record = v.book.begin(
            admission.input,
            admission.verify_nonce,
            current.approval,
            now,
            verifier,
        )?;
        next.validate_persistence()?;
        *self = next;
        Ok(record)
    }
    pub fn commit_owner_setup_checks(
        &self,
        id: &str,
        signed: &SignedApprovalVerificationV1,
        current: &ProvisionApprovalContext<'_>,
        now: u64,
    ) -> Result<ExactSignatureChecks> {
        let v = self.owner_setups.get(id).ok_or_else(invalid)?;
        let r = v.book.original(id).ok_or_else(invalid)?;
        self.owner_setup_scope(r.input(), &v.plan, current, now)?;
        Ok(v.book.commit_checks(id, signed, current.approval, now)?)
    }
    pub fn commit_owner_setup(
        &mut self,
        commit: CommitOwnerSetupV1,
        scope: &NativeStageScope<'_>,
        current: &ProvisionApprovalContext<'_>,
        now: u64,
        verifier: &mut impl ApprovalSignatureVerifier,
    ) -> Result<OwnerSetupApprovalV1> {
        let CommitOwnerSetupV1 {
            id,
            signed,
            source_nonce,
        } = commit;
        let id = id.as_str();
        verifier.verify(&self.commit_owner_setup_checks(id, &signed, current, now)?)?;
        if let Some(v) = self.owner_setups.get(id).filter(|v| v.transport.is_some()) {
            return Ok(v.clone());
        }
        if !hex64(&source_nonce) {
            return Err(invalid());
        }
        let (session, status) = self.participant_session(scope)?;
        let participant = self
            .participant_provisions
            .values()
            .find(|v| v.result.is_some())
            .ok_or_else(invalid)?;
        if status.status.receipt != participant.descriptor.native_binding {
            return Err(invalid());
        }
        let mut next = self.clone();
        let v = next.owner_setups.get_mut(id).ok_or_else(invalid)?;
        v.session = Some(session);
        v.book.commit(id, signed, current.approval, now, verifier)?;
        let mut transport = BudgetLedger::new(v.plan.transport_limit.0);
        transport.charge(8).map_err(|_| invalid())?;
        v.transport = Some(transport);
        v.source = Some(ProvisionOriginInspectionV1 {
            operation_id: digest(&("oasis7.custody.owner-setup-source.v1", id, &v.plan.setup_id))?,
            verify_nonce: source_nonce,
            created_at: DecimalU64(now),
            expires_at: DecimalU64(
                v.plan
                    .deadline
                    .0
                    .min(current.approval.source.session_expires_at.0)
                    .min(current.approval.source.session_idle_expires_at.0)
                    .min(now.checked_add(30_000).ok_or_else(invalid)?),
            ),
            claimed_at: None,
            request_bytes: None,
            verification: None,
            verified_at: None,
        });
        let result = v.clone();
        next.validate_persistence()?;
        *self = next;
        Ok(result)
    }
}
