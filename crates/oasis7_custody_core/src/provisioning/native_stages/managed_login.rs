use super::*;
use crate::budget::BudgetLedger;
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ManagedLoginTaskV1 {
    pub operation_id: String,
    pub login_operation_id: String,
    pub blocked_operation_ref: String,
    pub native_account_id: String,
    pub login_key: PublicRoleKey,
    pub native_identity_namespace: String,
    pub target_security_epoch: DecimalU64,
    pub deadline: DecimalU64,
    pub transport: BudgetLedger,
    pub status_nonce: String,
    pub claimed_at: Option<DecimalU64>,
    pub reads: u8,
    pub request_bytes: String,
    pub result: Option<NativeStageResultV1>,
    pub private_sid_generation: Option<DecimalU64>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ManagedLoginApprovalV1 {
    pub(super) book: ApprovalAdmissionBook,
    pub(super) task: Option<ManagedLoginTaskV1>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ManagedLoginPermitViewV1 {
    pub task: ManagedLoginTaskV1,
    pub initial_login_operation_id: String,
    pub identity_scope: RoleKeySlotScopeV1,
    pub authority_revision: String,
    pub source_revision: String,
    pub service_epoch: DecimalU64,
    pub observed_at: DecimalU64,
    pub expires_at: DecimalU64,
    pub http_origin: String,
    pub source_audience: String,
    pub binding: NativeBindingReceiptV1,
}
impl AccountProvisionApprovalState {
    pub fn managed_login_original(&self, id: &str) -> Option<&ApprovalAdmissionRecord> {
        self.managed_logins
            .get(id)
            .and_then(|v| v.book.original(id))
    }
    pub fn managed_logins(&self) -> &BTreeMap<String, ManagedLoginApprovalV1> {
        &self.managed_logins
    }
    pub fn managed_login_task(&self, id: &str) -> Option<&ManagedLoginTaskV1> {
        self.managed_logins.get(id).and_then(|v| v.task.as_ref())
    }
    pub(super) fn managed_login_scope(
        &self,
        input: &ApprovalAdmissionInput,
        current: &ProvisionApprovalContext<'_>,
        now: u64,
    ) -> Result<()> {
        let CustodyIntentV1::ManagedLogin {
            native_account_id,
            login_key_ref,
            native_identity_namespace,
            blocked_operation_ref,
            ..
        } = &input.request.intent
        else {
            return Err(ProvisionApprovalError::BindingMismatch);
        };
        let receipt = self.native_receipt()?;
        let target = &current.approval.authority;
        if self.provision.frozen()
            || self.provision.security_epoch().0 != 1
            || self.bootstrap_activation.is_none()
            || target.security_version.0 != 1
            || input.request.context != *target
            || current.approval.frozen
            || current
                .approval
                .fence
                .check_epoch(target.service_epoch.0)
                .is_err()
            || target.environment != self.draft.entry.environment
            || target.custody_account_id != self.draft.entry.custody_account_id
            || current.policy.version != self.draft.entry.bootstrap_config_version.0
            || !current.policy.managers.iter().any(|v| {
                v.principal == self.draft.entry.manager
                    && v.environments.contains(&self.draft.entry.environment)
            })
            || current.approval.source.manager_account_id
                != self.draft.entry.manager.native_account_id
            || native_account_id != &receipt.native_account_id
            || login_key_ref != &receipt.login_key_ref
            || native_identity_namespace != &receipt.identity_namespace
            || self
                .native_stages
                .get(blocked_operation_ref)
                .is_none_or(|v| {
                    !matches!(v.result, Some(NativeStageResultV1::ExplicitLoginRequired))
                })
        {
            return Err(ProvisionApprovalError::BindingMismatch);
        }
        input
            .request
            .validate(now)
            .map_err(|_| ProvisionApprovalError::BindingMismatch)?;
        Ok(())
    }
    fn check_managed_proof_unique(&self, input: &ApprovalAdmissionInput) -> Result<()> {
        for record in self
            .book
            .records()
            .chain(self.owner_setup_approval_records())
        {
            if same_approval_proof_identity(record.input(), input) {
                return Err(ApprovalAdmissionError::NonceReused.into());
            }
        }
        for (id, approval) in &self.managed_logins {
            for record in approval.book.records() {
                if id == &input.operation_id && record.input() == input {
                    continue;
                }
                if id == &input.operation_id {
                    return Err(ProvisionApprovalError::Conflict);
                }
                if same_approval_proof_identity(record.input(), input) {
                    return Err(ApprovalAdmissionError::NonceReused.into());
                }
            }
        }
        Ok(())
    }
    pub fn begin_managed_login_checks(
        &self,
        input: &ApprovalAdmissionInput,
        nonce: &str,
        current: &ProvisionApprovalContext<'_>,
        now: u64,
    ) -> Result<ExactSignatureChecks> {
        self.check_managed_proof_unique(input)?;
        self.managed_login_scope(input, current, now)?;
        let empty = ApprovalAdmissionBook::default();
        Ok(self
            .managed_logins
            .get(&input.operation_id)
            .map_or(&empty, |v| &v.book)
            .begin_checks(input, nonce, current.approval, now)?)
    }
    pub fn begin_managed_login(
        &mut self,
        input: ApprovalAdmissionInput,
        nonce: String,
        current: &ProvisionApprovalContext<'_>,
        now: u64,
        verifier: &mut impl ApprovalSignatureVerifier,
    ) -> Result<ApprovalAdmissionRecord> {
        self.check_managed_proof_unique(&input)?;
        self.managed_login_scope(&input, current, now)?;
        if self.managed_logins.len() >= 16 && !self.managed_logins.contains_key(&input.operation_id)
        {
            return Err(ProvisionApprovalError::Conflict);
        }
        let mut next = self.clone();
        let record = next
            .managed_logins
            .entry(input.operation_id.clone())
            .or_insert_with(|| ManagedLoginApprovalV1 {
                book: ApprovalAdmissionBook::default(),
                task: None,
            })
            .book
            .begin(input, nonce, current.approval, now, verifier)?;
        next.validate_persistence()?;
        *self = next;
        Ok(record)
    }
    pub fn commit_managed_login_checks(
        &self,
        id: &str,
        signed: &SignedApprovalVerificationV1,
        current: &ProvisionApprovalContext<'_>,
        now: u64,
    ) -> Result<ExactSignatureChecks> {
        let r = self
            .managed_logins
            .get(id)
            .ok_or(ApprovalAdmissionError::Missing)?;
        let original = r.book.original(id).ok_or(ApprovalAdmissionError::Missing)?;
        self.managed_login_scope(original.input(), current, now)?;
        Ok(r.book.commit_checks(id, signed, current.approval, now)?)
    }
    pub fn commit_managed_login(
        &mut self,
        id: &str,
        signed: SignedApprovalVerificationV1,
        current: &ProvisionApprovalContext<'_>,
        now: u64,
        status_nonce: &str,
        verifier: &mut impl ApprovalSignatureVerifier,
    ) -> Result<ManagedLoginTaskV1> {
        let checks = self.commit_managed_login_checks(id, &signed, current, now)?;
        verifier.verify(&checks)?;
        if let Some(task) = self.managed_login_task(id) {
            return Ok(task.clone());
        }
        let mut next = self.clone();
        let original = next
            .managed_logins
            .get_mut(id)
            .ok_or(ApprovalAdmissionError::Missing)?
            .book
            .commit(id, signed, current.approval, now, verifier)?;
        let CustodyIntentV1::ManagedLogin {
            native_account_id,
            login_key_ref: _,
            native_identity_namespace,
            blocked_operation_ref,
            login_request_id,
            transport_limit,
            deadline,
        } = &original.input().request.intent
        else {
            return Err(ProvisionApprovalError::BindingMismatch);
        };
        let login_key = next
            .provision
            .role_key(Role::Identity)
            .ok_or(ApprovalAdmissionError::Missing)?
            .clone();
        let operation_scope = super::super::super::types::digest(&(
            "oasis7-custody-managed-login-operation-v1",
            &next.draft.entry.environment,
            &next.draft.entry.custody_account_id,
            &next.draft.entry.provision_id,
            native_identity_namespace,
            original.input().approval.proof.identity_instance_epoch,
            &login_key.public_key,
            id,
        ))?;
        let mut transport = BudgetLedger::new(transport_limit.0);
        transport
            .charge(64)
            .map_err(|_| ProvisionError::Exhausted)?;
        if status_nonce.len() != 64
            || !status_nonce
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        {
            return Err(ProvisionApprovalError::BindingMismatch);
        }
        let wire = serde_json::to_string(&(
            "oasis7-custody-managed-login-v1",
            &operation_scope,
            login_request_id,
            native_account_id,
            &login_key,
            native_identity_namespace,
            status_nonce,
            deadline,
        ))
        .map_err(|_| ProvisionApprovalError::InvalidPersistence)?;
        let task = ManagedLoginTaskV1 {
            status_nonce: status_nonce.into(),
            claimed_at: None,
            reads: 0,
            request_bytes: wire,
            result: None,
            private_sid_generation: None,
            operation_id: id.into(),
            login_operation_id: derive_native_operation_id(&operation_scope, login_request_id)?,
            blocked_operation_ref: blocked_operation_ref.clone(),
            native_account_id: native_account_id.clone(),
            login_key,
            native_identity_namespace: native_identity_namespace.clone(),
            target_security_epoch: current.approval.authority.security_version,
            deadline: *deadline,
            transport,
        };
        next.managed_logins
            .get_mut(id)
            .ok_or(ApprovalAdmissionError::Missing)?
            .task = Some(task.clone());
        next.validate_persistence()?;
        *self = next;
        Ok(task)
    }
    pub(in crate::provisioning) fn validate_managed_login_persistence(&self) -> Result<()> {
        if self.managed_logins.len() > 16 {
            return Err(ProvisionApprovalError::InvalidPersistence);
        }
        for (id, r) in &self.managed_logins {
            let original = r
                .book
                .original(id)
                .ok_or(ProvisionApprovalError::InvalidPersistence)?;
            if r.book.records().count() != 1 {
                return Err(ProvisionApprovalError::InvalidPersistence);
            }
            self.check_managed_proof_unique(original.input())
                .map_err(|_| ProvisionApprovalError::InvalidPersistence)?;
            if original.input().request.context.security_version.0 != 1
                || original.input().request.context.custody_account_id
                    != self.draft.entry.custody_account_id
                || original.input().request.context.environment != self.draft.entry.environment
            {
                return Err(ProvisionApprovalError::InvalidPersistence);
            }
            if let Some(task) = &r.task {
                let CustodyIntentV1::ManagedLogin {
                    native_account_id,
                    login_key_ref,
                    native_identity_namespace,
                    blocked_operation_ref,
                    login_request_id,
                    transport_limit,
                    deadline,
                } = &original.input().request.intent
                else {
                    return Err(ProvisionApprovalError::InvalidPersistence);
                };
                let operation_scope = super::super::super::types::digest(&(
                    "oasis7-custody-managed-login-operation-v1",
                    &self.draft.entry.environment,
                    &self.draft.entry.custody_account_id,
                    &self.draft.entry.provision_id,
                    native_identity_namespace,
                    original.input().approval.proof.identity_instance_epoch,
                    &task.login_key.public_key,
                    id,
                ))?;
                let expected_wire = serde_json::to_string(&(
                    "oasis7-custody-managed-login-v1",
                    &operation_scope,
                    login_request_id,
                    native_account_id,
                    &task.login_key,
                    native_identity_namespace,
                    &task.status_nonce,
                    deadline,
                ))
                .map_err(|_| ProvisionApprovalError::InvalidPersistence)?;
                if task.request_bytes != expected_wire
                    || task.login_operation_id
                        != derive_native_operation_id(&operation_scope, login_request_id)?
                    || !matches!(
                        &task.result,
                        None | Some(NativeStageResultV1::SessionStatus(_))
                            | Some(NativeStageResultV1::ExplicitLoginRequired)
                    )
                {
                    return Err(ProvisionApprovalError::InvalidPersistence);
                }
                let mut expected = BudgetLedger::new(transport_limit.0);
                expected
                    .charge(64 + self.managed_recovery_transport_cost(id))
                    .map_err(|_| ProvisionError::Exhausted)?;
                if task.reads > 8
                    || task.result.is_some() && task.claimed_at.is_none()
                    || task.status_nonce.len() != 64
                    || task.request_bytes.is_empty()
                    || original.stage() != ApprovalAdmissionStage::Consumed
                    || task.operation_id != *id
                    || task.transport != expected
                    || task.target_security_epoch.0 != 1
                    || task.native_account_id != *native_account_id
                    || task.native_identity_namespace != *native_identity_namespace
                    || task.blocked_operation_ref != *blocked_operation_ref
                    || task.deadline != *deadline
                    || task.login_key.key_id != *login_key_ref
                    || self.provision.role_key(Role::Identity) != Some(&task.login_key)
                    || task.native_account_id != self.native_receipt()?.native_account_id
                {
                    return Err(ProvisionApprovalError::InvalidPersistence);
                }
            } else if original.stage() == ApprovalAdmissionStage::Consumed {
                return Err(ProvisionApprovalError::InvalidPersistence);
            }
        }
        Ok(())
    }
}

fn same_approval_proof_identity(a: &ApprovalAdmissionInput, b: &ApprovalAdmissionInput) -> bool {
    let a = &a.approval.proof;
    let b = &b.approval.proof;
    a.nonce == b.nonce
        || (a.identity_namespace == b.identity_namespace
            && a.identity_instance_epoch == b.identity_instance_epoch
            && (a.approval_id == b.approval_id || a.approval_request_id == b.approval_request_id))
}
