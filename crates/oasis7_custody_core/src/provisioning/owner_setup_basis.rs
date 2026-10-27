use super::*;
/// Trusted management metadata + fixed configuration. No approval/passkey proof
/// is fabricated to read a basis or prepare the future UV request.
pub struct OwnerSetupManagementCurrent<'a> {
    pub manager: &'a crate::approval::CustodySessionStatusV1,
    pub authority: &'a crate::approval::ApprovalContextV1,
    pub policy: ProvisionPolicy<'a>,
    pub world_identity: &'a crate::approval::ApprovedWorldV1,
    pub native_identity_namespace: &'a str,
    pub fence: &'a crate::recovery::RecoveryFence,
    pub observed_at: u64,
}
#[cfg_attr(test, derive(schemars::JsonSchema))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OwnerSetupBasisRequestV1 {
    pub api_version: u8,
}
impl OwnerSetupBasisRequestV1 {
    pub fn validate(&self) -> Result<()> {
        if self.api_version != 1 {
            return Err(invalid());
        }
        Ok(())
    }
}
#[cfg_attr(test, derive(schemars::JsonSchema))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OwnerSetupBasisV1 {
    pub api_version: u8,
    pub custody_account_id: String,
    pub provision_id: String,
    pub participant_receipt_digest: String,
    pub world_identity: crate::approval::ApprovedWorldV1,
    pub owner_key: PublicRoleKey,
    pub executor_key: PublicRoleKey,
    pub native_account_id: String,
    pub native_identity_namespace: String,
    pub identity_instance_epoch: DecimalU64,
    pub bootstrap_config_version: DecimalU64,
    pub security_epoch: DecimalU64,
    pub service_epoch: DecimalU64,
}
#[cfg_attr(test, derive(schemars::JsonSchema))]
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OwnerSetupPrepareRequestV1 {
    pub api_version: u8,
    pub approval_request_id: String,
    pub plan: OwnerSetupPlanV1,
}
impl core::fmt::Debug for OwnerSetupPrepareRequestV1 {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str("OwnerSetupPrepareRequestV1(<private>)")
    }
}
impl OwnerSetupPrepareRequestV1 {
    pub fn validate(&self) -> Result<()> {
        self.plan.validate()?;
        if self.api_version != 1 || !id(&self.approval_request_id) {
            return Err(invalid());
        }
        Ok(())
    }
}
/// The response is the actual native approval-options request, not a proof.
pub type OwnerSetupPrepareResponseV1 = crate::approval::ApprovalOptionsRequestV1;
impl AccountProvisionApprovalState {
    pub fn owner_setup_basis(
        &self,
        current: &OwnerSetupManagementCurrent<'_>,
        now: u64,
    ) -> Result<OwnerSetupBasisV1> {
        let m = current.manager;
        let a = current.authority;
        let participant = self
            .participant_provisions
            .values()
            .find(|p| p.result.is_some())
            .ok_or_else(invalid)?;
        let binding = &participant.descriptor.native_binding;
        if self.frozen()
            || !matches!(
                self.provision.stage(),
                ProvisionStage::ParticipantBound
                    | ProvisionStage::ReadyForSetup
                    | ProvisionStage::Prepared
            )
            || self.provision.security_epoch().0 == 0
            || a.security_version != self.provision.security_epoch()
            || a.environment != self.draft.entry.environment
            || a.custody_account_id != self.draft.entry.custody_account_id
            || a.policy_version.0 == 0
            || a.service_epoch.0 == 0
            || !id(&a.audience)
            || current.fence.check_epoch(a.service_epoch.0).is_err()
            || current.observed_at > now
            || now - current.observed_at > 5000
            || m.api_version != 1
            || m.identity_namespace != self.draft.entry.manager.identity_namespace
            || m.account_id != self.draft.entry.manager.native_account_id
            || m.identity_instance_epoch != binding.identity_instance_epoch
            || m.credential_version.0 == 0
            || m.security_version.0 == 0
            || m.policy_version.0 == 0
            || !id(&m.session_ref)
            || !id(&m.credential_ref)
            || now >= m.expires_at.0
            || now >= m.idle_expires_at.0
            || current.policy.version != self.draft.entry.bootstrap_config_version.0
            || current.world_identity != &self.draft.world_identity
            || current.native_identity_namespace != binding.identity_namespace
            || !current.policy.managers.iter().any(|v| {
                v.principal == self.draft.entry.manager
                    && v.environments.contains(&self.draft.entry.environment)
            })
        {
            return Err(invalid());
        }
        Ok(OwnerSetupBasisV1 {
            api_version: 1,
            custody_account_id: self.draft.entry.custody_account_id.clone(),
            provision_id: self.draft.entry.provision_id.clone(),
            participant_receipt_digest: digest(participant.result.as_ref().ok_or_else(invalid)?)?,
            world_identity: self.draft.world_identity.clone(),
            owner_key: self
                .provision
                .role_key(Role::Owner)
                .ok_or_else(invalid)?
                .clone(),
            executor_key: self
                .provision
                .role_key(Role::Executor)
                .ok_or_else(invalid)?
                .clone(),
            native_account_id: binding.native_account_id.clone(),
            native_identity_namespace: binding.identity_namespace.clone(),
            identity_instance_epoch: binding.identity_instance_epoch,
            bootstrap_config_version: self.draft.entry.bootstrap_config_version,
            security_epoch: self.provision.security_epoch(),
            service_epoch: a.service_epoch,
        })
    }
    pub fn owner_setup_prepare_request(
        &self,
        request: &OwnerSetupPrepareRequestV1,
        current: &OwnerSetupManagementCurrent<'_>,
        now: u64,
    ) -> Result<OwnerSetupPrepareResponseV1> {
        request.validate()?;
        let basis = self.owner_setup_basis(current, now)?;
        let p = &request.plan;
        if p.provision_id != basis.provision_id
            || p.participant_receipt_digest != basis.participant_receipt_digest
            || p.world_identity != basis.world_identity
            || p.owner_key_ref != basis.owner_key.key_id
            || p.executor_key_ref != basis.executor_key.key_id
            || p.executor_public_key != basis.executor_key.public_key
            || p.bootstrap_config_version != basis.bootstrap_config_version
            || self.owner_setups.keys().any(|id| id != &p.setup_id)
            || self
                .owner_setups
                .get(&p.setup_id)
                .is_some_and(|v| v.plan != *p)
        {
            return Err(invalid());
        }
        let options = crate::approval::ApprovalOptionsRequestV1 {
            approval_request_id: request.approval_request_id.clone(),
            context: current.authority.clone(),
            intent: p.intent()?,
        };
        options.validate(now).map_err(|_| invalid())?;
        Ok(options)
    }
}
