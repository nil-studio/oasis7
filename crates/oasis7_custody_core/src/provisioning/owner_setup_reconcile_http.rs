use super::*;
#[cfg_attr(test, derive(schemars::JsonSchema))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OwnerSetupReconcileBasisRequestV1 {
    pub api_version: u8,
    pub setup_id: String,
    pub stage: OwnerSetupStageV1,
}
#[cfg_attr(test, derive(schemars::JsonSchema))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OwnerSetupReconcileBasisV1 {
    pub api_version: u8,
    pub custody_account_id: String,
    pub provision_id: String,
    pub setup_id: String,
    pub stage: OwnerSetupStageV1,
    pub setup_plan_digest: String,
    pub original_digest: String,
    pub correlation_digest: String,
    pub world_identity: crate::approval::ApprovedWorldV1,
    pub owner_key_ref: String,
    pub executor_key_ref: String,
    pub original_deadline: DecimalU64,
    pub original_transport_remaining: DecimalU64,
    pub security_epoch: DecimalU64,
    pub service_epoch: DecimalU64,
}
#[cfg_attr(test, derive(schemars::JsonSchema))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OwnerSetupReconcilePrepareRequestV1 {
    pub api_version: u8,
    pub approval_request_id: String,
    pub plan: OwnerSetupReconcilePlanV1,
}
pub type OwnerSetupReconcilePrepareResponseV1 = ApprovalOptionsRequestV1;
#[cfg_attr(test, derive(schemars::JsonSchema))]
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OwnerSetupReconcileApproveRequestV1 {
    pub api_version: u8,
    pub input: ApprovalAdmissionInput,
    pub plan: OwnerSetupReconcilePlanV1,
}
impl core::fmt::Debug for OwnerSetupReconcileApproveRequestV1 {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str("OwnerSetupReconcileApproveRequestV1(<private>)")
    }
}
#[cfg_attr(test, derive(schemars::JsonSchema))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OwnerSetupReconcileAttemptRequestV1 {
    pub api_version: u8,
    pub reconcile_id: String,
    pub attempt_id: String,
}
#[cfg_attr(test, derive(schemars::JsonSchema))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OwnerSetupReconcileProgressV1 {
    pub api_version: u8,
    pub custody_account_id: String,
    pub provision_id: String,
    pub setup_id: String,
    pub reconcile_id: String,
    pub stage: OwnerSetupStageV1,
    pub pending_attempts: Vec<String>,
    pub completed_attempts: Vec<String>,
    pub transport_remaining: DecimalU64,
    pub original_transport_remaining: DecimalU64,
}
impl OwnerSetupReconcileBasisRequestV1 {
    pub fn validate(&self) -> Result<()> {
        if self.api_version != 1 || !id(&self.setup_id) {
            return Err(invalid());
        }
        Ok(())
    }
}
impl OwnerSetupReconcilePrepareRequestV1 {
    pub fn validate(&self) -> Result<()> {
        self.plan.validate()?;
        if self.api_version != 1 || !id(&self.approval_request_id) {
            return Err(invalid());
        }
        Ok(())
    }
}
impl OwnerSetupReconcileApproveRequestV1 {
    pub fn validate(&self) -> Result<()> {
        self.plan.validate()?;
        if self.api_version != 1
            || self.input.operation_id != self.plan.reconcile_id
            || self.input.request.intent != self.plan.intent()?
        {
            return Err(invalid());
        }
        Ok(())
    }
}
impl OwnerSetupReconcileAttemptRequestV1 {
    pub fn validate(&self) -> Result<()> {
        if self.api_version != 1 || !id(&self.reconcile_id) || !id(&self.attempt_id) {
            return Err(invalid());
        }
        Ok(())
    }
}
impl AccountProvisionApprovalState {
    pub fn owner_setup_reconcile_basis(
        &self,
        request: &OwnerSetupReconcileBasisRequestV1,
        current: &OwnerSetupManagementCurrent<'_>,
        now: u64,
    ) -> Result<OwnerSetupReconcileBasisV1> {
        request.validate()?;
        self.owner_setup_basis(current, now)?;
        let setup = self
            .owner_setups
            .get(&request.setup_id)
            .ok_or_else(invalid)?;
        let o = setup.stages.get(&request.stage).ok_or_else(invalid)?;
        let original = o.original.as_ref().ok_or_else(invalid)?;
        if o.claimed_at.is_none()
            || o.result.as_ref().is_some_and(|r| r.terminal())
            || now >= setup.plan.deadline.0
        {
            return Err(invalid());
        }
        Ok(OwnerSetupReconcileBasisV1 {
            api_version: 1,
            custody_account_id: self.draft.entry.custody_account_id.clone(),
            provision_id: self.draft.entry.provision_id.clone(),
            setup_id: request.setup_id.clone(),
            stage: request.stage,
            setup_plan_digest: setup.plan.digest()?,
            original_digest: digest(original)?,
            correlation_digest: match original {
                OwnerSetupOriginalV1::Canonical { submit, .. } => digest(&submit.correlation)?,
                _ => digest(original)?,
            },
            world_identity: setup.plan.world_identity.clone(),
            owner_key_ref: setup.plan.owner_key_ref.clone(),
            executor_key_ref: setup.plan.executor_key_ref.clone(),
            original_deadline: setup.plan.deadline,
            original_transport_remaining: DecimalU64(
                setup
                    .transport
                    .as_ref()
                    .ok_or_else(invalid)?
                    .remaining()
                    .map_err(|_| invalid())?,
            ),
            security_epoch: self.provision.security_epoch(),
            service_epoch: current.authority.service_epoch,
        })
    }
    pub fn owner_setup_reconcile_prepare_request(
        &self,
        request: &OwnerSetupReconcilePrepareRequestV1,
        current: &OwnerSetupManagementCurrent<'_>,
        now: u64,
    ) -> Result<OwnerSetupReconcilePrepareResponseV1> {
        request.validate()?;
        self.owner_setup_reconcile_basis(
            &OwnerSetupReconcileBasisRequestV1 {
                api_version: 1,
                setup_id: request.plan.setup_id.clone(),
                stage: request.plan.stage,
            },
            current,
            now,
        )?;
        self.validate_reconcile_plan_scope(&request.plan)?;
        let result = ApprovalOptionsRequestV1 {
            approval_request_id: request.approval_request_id.clone(),
            context: current.authority.clone(),
            intent: request.plan.intent()?,
        };
        result.validate(now).map_err(|_| invalid())?;
        Ok(result)
    }
    pub fn owner_setup_reconcile_progress(
        &self,
        id: &str,
    ) -> Result<OwnerSetupReconcileProgressV1> {
        let v = self.owner_setup_reconciles.get(id).ok_or_else(invalid)?;
        let setup = self
            .owner_setups
            .get(&v.plan.setup_id)
            .ok_or_else(invalid)?;
        Ok(OwnerSetupReconcileProgressV1 {
            api_version: 1,
            custody_account_id: self.draft.entry.custody_account_id.clone(),
            provision_id: self.draft.entry.provision_id.clone(),
            setup_id: v.plan.setup_id.clone(),
            reconcile_id: id.into(),
            stage: v.plan.stage,
            pending_attempts: v
                .attempts
                .values()
                .filter(|a| a.result.as_ref().is_none_or(|r| !r.terminal()))
                .map(|a| a.attempt_id.clone())
                .collect(),
            completed_attempts: v
                .attempts
                .values()
                .filter(|a| a.result.as_ref().is_some_and(|r| r.terminal()))
                .map(|a| a.attempt_id.clone())
                .collect(),
            transport_remaining: DecimalU64(
                v.transport
                    .as_ref()
                    .map_or(Ok(0), |b| b.remaining())
                    .map_err(|_| invalid())?,
            ),
            original_transport_remaining: DecimalU64(
                setup
                    .transport
                    .as_ref()
                    .ok_or_else(invalid)?
                    .remaining()
                    .map_err(|_| invalid())?,
            ),
        })
    }
}
