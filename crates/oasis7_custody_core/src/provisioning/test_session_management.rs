use super::*;
/// Metadata-only, fixed private ports; no approval/passkey source is invented.
pub struct TestSessionManagementCurrent<'a, 'b> {
    pub management: &'a OwnerSetupManagementCurrent<'b>,
    pub native_scope: &'a NativeStageScope<'b>,
    pub enrollment: &'a ProvedEnrollmentObservation<'a>,
    pub deployments: &'a HostDeploymentAllowlist,
}
#[cfg_attr(test, derive(schemars::JsonSchema))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TestSessionBasisRequestV1 {
    pub api_version: u8,
    pub request_id: String,
    pub enrollment_id: String,
    pub cleanup_until: UnixMillis,
}
impl TestSessionBasisRequestV1 {
    pub fn validate(&self) -> Result<()> {
        if self.api_version != 1
            || !id(&self.request_id)
            || !id(&self.enrollment_id)
            || self.cleanup_until.get() == 0
        {
            return Err(invalid());
        }
        Ok(())
    }
}
#[cfg_attr(test, derive(schemars::JsonSchema))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TestSessionBasisV1 {
    pub api_version: u8,
    pub request_id: String,
    pub session_id: String,
    pub custody_account_id: String,
    pub provision_id: String,
    pub authority: crate::approval::ApprovalContextV1,
    pub termination: TerminationPlanV1,
    pub native_account_id: String,
    pub owner_key: PublicRoleKey,
    pub executor_key: PublicRoleKey,
    pub client_pairing: ClientPairingBindingV1,
    pub runtime_ref: String,
    pub skill_ref: String,
    pub source_ref: String,
    pub allowed_resource_refs: Vec<String>,
}
impl TestSessionBasisV1 {
    pub fn digest(&self) -> Result<String> {
        digest(&("oasis7.custody.test-session-basis.v1", self))
    }
}
#[cfg_attr(test, derive(schemars::JsonSchema))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TestSessionPlanV1 {
    pub api_version: u8,
    pub request_id: String,
    pub enrollment_id: String,
    pub basis_digest: String,
    pub actions: Vec<crate::approval::ApprovedActionV1>,
    pub allowed_resource_refs: Vec<String>,
    pub write_deadline: UnixMillis,
    pub read_until: UnixMillis,
    pub cleanup_until: UnixMillis,
    pub request_limit: DecimalU64,
    pub transport_limit: DecimalU64,
    pub business_limit: DecimalU64,
    pub read_budget: crate::approval::PurposeBudgetV1,
    pub cleanup_budget: crate::approval::PurposeBudgetV1,
    pub control_budget: crate::approval::ControlBudgetV1,
}
impl TestSessionPlanV1 {
    pub fn validate(&self) -> Result<()> {
        if self.api_version != 1
            || !id(&self.request_id)
            || !id(&self.enrollment_id)
            || !hex64(&self.basis_digest)
        {
            return Err(invalid());
        }
        Ok(())
    }
}
#[cfg_attr(test, derive(schemars::JsonSchema))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TestSessionPrepareRequestV1 {
    pub api_version: u8,
    pub approval_request_id: String,
    pub plan: TestSessionPlanV1,
}
impl TestSessionPrepareRequestV1 {
    pub fn validate(&self) -> Result<()> {
        self.plan.validate()?;
        if self.api_version != 1 || !id(&self.approval_request_id) {
            return Err(invalid());
        }
        Ok(())
    }
}
pub type TestSessionPrepareResponseV1 = crate::approval::ApprovalOptionsRequestV1;
#[cfg_attr(test, derive(schemars::JsonSchema))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TestSessionApproveRequestV1 {
    pub api_version: u8,
    pub input: ApprovalAdmissionInput,
    pub plan: TestSessionPlanV1,
}
impl TestSessionApproveRequestV1 {
    pub fn validate(&self) -> Result<()> {
        self.plan.validate()?;
        if self.api_version != 1
            || !matches!(
                self.input.request.intent,
                CustodyIntentV1::TestSession { .. }
            )
        {
            return Err(invalid());
        }
        Ok(())
    }
}
#[cfg_attr(test, derive(schemars::JsonSchema))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TestSessionContinueRequestV1 {
    pub api_version: u8,
    pub session_id: String,
}
impl TestSessionContinueRequestV1 {
    pub fn validate(&self) -> Result<()> {
        if self.api_version != 1 || !id(&self.session_id) {
            return Err(invalid());
        }
        Ok(())
    }
}
impl AccountProvisionApprovalState {
    pub fn test_session_basis(
        &self,
        request: &TestSessionBasisRequestV1,
        current: &TestSessionManagementCurrent<'_, '_>,
        now: UnixMillis,
    ) -> Result<TestSessionBasisV1> {
        request.validate()?;
        let actual = self.owner_setup_basis(current.management, now.get())?;
        if self.session_inventory != SessionInventoryV1::Known
            || self.provision.stage() != ProvisionStage::Prepared
            || request.cleanup_until.get() <= now.get()
            || request.cleanup_until.get() > now.get().checked_add(2_100_000).ok_or_else(invalid)?
        {
            return Err(invalid());
        }
        current.enrollment.validate_physical()?;
        let fact = current.enrollment.fact;
        let r = &fact.registration;
        let d = &r.request.descriptor;
        let invite = self.pairing_invite(&r.request.target)?;
        if request.enrollment_id != r.enrollment_id
            || now.get() >= r.expires_at.get()
            || now.get() >= invite.original.expires_at.get()
            || invite.original.service_epoch != current.management.authority.service_epoch
            || invite.original.transport_profile != d.transport_profile
            || invite.original.tool_profile != d.tool_profile
            || d.environment != self.draft.entry.environment
            || current
                .deployments
                .configured_metadata(d)
                .map_err(|_| invalid())?
                .is_none()
        {
            return Err(invalid());
        }
        let session_id = digest(&(
            "oasis7.custody.test-session-id.v1",
            &self.draft.entry.environment,
            &self.draft.entry.custody_account_id,
            &self.draft.entry.provision_id,
            &r.enrollment_id,
            fact.digest().map_err(|_| invalid())?,
            &request.request_id,
        ))?;
        if invite
            .consumed_session
            .as_ref()
            .is_some_and(|old| old != &session_id)
            || self.test_sessions.iter().any(|(id, s)| {
                id != &session_id && s.enrollment.registration.enrollment_id == r.enrollment_id
            })
        {
            return Err(ProvisionApprovalError::Conflict);
        }
        let (_, status) = self.participant_session(current.native_scope)?;
        if status.status.receipt.native_account_id != actual.native_account_id {
            return Err(invalid());
        }
        let pairing = ClientPairingBindingV1 {
            api_version: 1,
            target: r.request.target.clone(),
            enrollment_id: r.enrollment_id.clone(),
            enrollment_digest: fact.digest().map_err(|_| invalid())?,
            host_instance_id: d.host_instance_id.clone(),
            descriptor_digest: r.descriptor_digest.clone(),
            client_key_id: r.key_id.clone(),
            client_public_key: d.client_public_key.clone(),
            connector_public_key: None,
        };
        // A connector is a separately proved key. Never infer one from the Host descriptor.
        if d.transport_profile != HostTransportProfileV1::NativeBrokerCustodyV1 {
            return Err(invalid());
        }
        pairing.validate().map_err(|_| invalid())?;
        let termination =
            self.test_session_termination_basis(&session_id, request.cleanup_until)?;
        let resources = self
            .owner_setups
            .get(&termination.setup_id)
            .ok_or_else(invalid)?
            .plan
            .resource_refs
            .clone();
        Ok(TestSessionBasisV1 {
            api_version: 1,
            request_id: request.request_id.clone(),
            session_id,
            custody_account_id: self.draft.entry.custody_account_id.clone(),
            provision_id: self.draft.entry.provision_id.clone(),
            authority: current.management.authority.clone(),
            termination,
            native_account_id: actual.native_account_id,
            owner_key: actual.owner_key,
            executor_key: actual.executor_key,
            client_pairing: pairing,
            runtime_ref: d.restricted_runtime_identity.clone(),
            skill_ref: d.skill_manifest_digest.clone(),
            source_ref: d.build_digest.clone(),
            allowed_resource_refs: resources,
        })
    }
    pub fn test_session_prepare_request(
        &self,
        request: &TestSessionPrepareRequestV1,
        current: &TestSessionManagementCurrent<'_, '_>,
        now: UnixMillis,
    ) -> Result<TestSessionPrepareResponseV1> {
        request.validate()?;
        let plan = &request.plan;
        let basis = self.test_session_basis(
            &TestSessionBasisRequestV1 {
                api_version: 1,
                request_id: plan.request_id.clone(),
                enrollment_id: plan.enrollment_id.clone(),
                cleanup_until: plan.cleanup_until,
            },
            current,
            now,
        )?;
        if plan.basis_digest != basis.digest()?
            || plan.allowed_resource_refs != basis.allowed_resource_refs
        {
            return Err(invalid());
        }
        let client_binding = crate::approval::ApprovedClientBindingV1::NativeBrokerCustodyV1 {
            client_key: basis.client_pairing.client_public_key.clone(),
        };
        let request = crate::approval::ApprovalOptionsRequestV1 {
            approval_request_id: request.approval_request_id.clone(),
            context: basis.authority,
            intent: CustodyIntentV1::TestSession {
                session_id: basis.session_id,
                native_account_id: basis.native_account_id,
                world_identity: Box::new(basis.termination.world.clone()),
                task_fence: Box::new(basis.termination.initial_fence.clone()),
                executor_key_ref: basis.executor_key.key_id,
                client_binding,
                client_pairing: Some(Box::new(basis.client_pairing)),
                runtime_ref: basis.runtime_ref,
                skill_ref: basis.skill_ref,
                source_ref: basis.source_ref,
                actions: plan.actions.clone(),
                allowed_resource_refs: plan.allowed_resource_refs.clone(),
                read_budget: plan.read_budget.clone(),
                cleanup_budget: plan.cleanup_budget.clone(),
                control_budget: plan.control_budget.clone(),
                write_deadline: DecimalU64(plan.write_deadline.get()),
                read_until: DecimalU64(plan.read_until.get()),
                cleanup_until: DecimalU64(plan.cleanup_until.get()),
                request_limit: plan.request_limit,
                transport_limit: plan.transport_limit,
                business_limit: plan.business_limit,
                termination_plan_digest: basis.termination.digest()?,
            },
        };
        request.validate(now.get()).map_err(|_| invalid())?;
        Ok(request)
    }
    /// Read-only codec. The resulting admission still needs strong begin/consume and live preflight.
    pub fn test_session_management_admission(
        &self,
        request: &TestSessionApproveRequestV1,
        server_verify_nonce: &str,
        current: &TestSessionManagementCurrent<'_, '_>,
        now: UnixMillis,
    ) -> Result<TestSessionAdmissionV1> {
        request.validate()?;
        if !hex64(server_verify_nonce) {
            return Err(invalid());
        }
        let expected = self.test_session_prepare_request(
            &TestSessionPrepareRequestV1 {
                api_version: 1,
                approval_request_id: request.input.request.approval_request_id.clone(),
                plan: request.plan.clone(),
            },
            current,
            now,
        )?;
        let CustodyIntentV1::TestSession { session_id, .. } = &expected.intent else {
            return Err(invalid());
        };
        if request.input.request != expected || request.input.operation_id != *session_id {
            return Err(invalid());
        }
        Ok(TestSessionAdmissionV1 {
            input: request.input.clone(),
            termination: self
                .test_session_termination_basis(session_id, request.plan.cleanup_until)?,
            verify_nonce: server_verify_nonce.into(),
        })
    }
}
