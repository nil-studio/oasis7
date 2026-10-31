//! Metadata/options producers do not consume MFA or create roots.
use super::*;
#[cfg_attr(test, derive(schemars::JsonSchema))]
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TestSessionBasisRequestV2 {
    pub api_version: u8,
    pub request_id: String,
    pub enrollment_id: String,
    pub setup_id: String,
    pub cleanup_until: UnixMillis,
}
impl TestSessionBasisRequestV2 {
    pub fn validate(&self) -> Result<()> {
        if self.api_version != 2
            || !id(&self.request_id)
            || !id(&self.enrollment_id)
            || !id(&self.setup_id)
            || self.cleanup_until.get() == 0
        {
            return Err(invalid());
        }
        Ok(())
    }
}
#[cfg_attr(test, derive(schemars::JsonSchema))]
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TestSessionBasisV2 {
    pub api_version: u8,
    pub request_id: String,
    pub custody_account_id: String,
    pub provision_id: String,
    pub scope: ResourceTestSessionScopeV2,
    pub owner_setup_plan_digest: String,
    pub termination: TerminationPlanV2,
    pub basis_digest: String,
}
#[cfg_attr(test, derive(schemars::JsonSchema))]
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TestSessionPrepareRequestV2 {
    pub api_version: u8,
    pub approval_request_id: String,
    pub plan: TestSessionPlanV2,
    pub scope: ResourceTestSessionScopeV2,
}
impl TestSessionPrepareRequestV2 {
    pub fn validate(&self) -> Result<()> {
        if self.api_version != 2 || !id(&self.approval_request_id) {
            return Err(invalid());
        }
        self.plan.validate().map_err(|_| invalid())
    }
}
impl core::fmt::Debug for TestSessionBasisRequestV2 {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str("TestSessionBasisRequestV2(<private>)")
    }
}
impl core::fmt::Debug for TestSessionBasisV2 {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str("TestSessionBasisV2(<private>)")
    }
}
impl core::fmt::Debug for TestSessionPrepareRequestV2 {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str("TestSessionPrepareRequestV2(<private>)")
    }
}
impl AccountProvisionApprovalState {
    pub fn test_session_basis_v2(
        &self,
        request: &TestSessionBasisRequestV2,
        current: &TestSessionManagementCurrent<'_, '_>,
        now: UnixMillis,
    ) -> Result<TestSessionBasisV2> {
        request.validate()?;
        let actual = self.owner_setup_basis(current.management, now.get())?;
        if self.provision.stage() != ProvisionStage::Prepared
            || self.session_inventory != SessionInventoryV1::Known
        {
            return Err(invalid());
        }
        current.enrollment.validate_physical()?;
        let fact = current.enrollment.fact;
        let r = &fact.registration;
        let d = &r.request.descriptor;
        let invite = self.pairing_invite(&r.request.target)?;
        if request.enrollment_id != r.enrollment_id
            || r.request.target.custody_account_id != self.draft.entry.custody_account_id
            || r.request.target.provision_id != self.draft.entry.provision_id
            || now.get() >= r.expires_at.get()
            || now.get() >= invite.original.expires_at.get()
            || invite.original.service_epoch != current.management.authority.service_epoch
            || invite.original.transport_profile != d.transport_profile
            || invite.original.tool_profile != d.tool_profile
            || d.environment != self.draft.entry.environment
            || d.transport_profile != HostTransportProfileV1::NativeBrokerCustodyV1
            || current
                .deployments
                .configured_metadata(d)
                .map_err(|_| invalid())?
                .is_none()
        {
            return Err(invalid());
        }
        let session_id = digest(&(
            "oasis7.custody.test-session-id.v2",
            &self.draft.entry.environment,
            &self.draft.entry.custody_account_id,
            &self.draft.entry.provision_id,
            &r.enrollment_id,
            fact.digest().map_err(|_| invalid())?,
            &request.request_id,
        ))?;
        if self.test_sessions.contains_key(&session_id)
            || invite
                .consumed_session
                .as_ref()
                .is_some_and(|old| old != &session_id)
        {
            return Err(ProvisionApprovalError::Conflict);
        }
        let (_, status) = self.participant_session(current.native_scope)?;
        if status.status.receipt.native_account_id != actual.native_account_id {
            return Err(invalid());
        }
        let prepared = self.owner_setup_prepared_v2(&request.setup_id)?;
        let termination = self.test_session_termination_basis_v2(
            &request.setup_id,
            &session_id,
            request.cleanup_until,
        )?;
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
        let scope = ResourceTestSessionScopeV2 {
            session_id,
            native_account_id: actual.native_account_id,
            world_identity: prepared.plan.world_identity.clone(),
            task_fence: GameTaskFenceV1::from_control(prepared.control),
            executor_key_ref: prepared.plan.executor_key_ref.clone(),
            client_binding: crate::approval::ApprovedClientBindingV1::NativeBrokerCustodyV1 {
                client_key: pairing.client_public_key.clone(),
            },
            client_pairing: pairing,
            runtime_ref: d.restricted_runtime_identity.clone(),
            skill_ref: d.skill_manifest_digest.clone(),
            source_ref: d.build_digest.clone(),
            termination_plan_digest: termination.digest().map_err(|_| invalid())?,
        };
        Ok(TestSessionBasisV2 {
            api_version: 2,
            request_id: request.request_id.clone(),
            custody_account_id: self.draft.entry.custody_account_id.clone(),
            provision_id: self.draft.entry.provision_id.clone(),
            owner_setup_plan_digest: prepared.plan.digest().map_err(|_| invalid())?,
            basis_digest: self
                .test_session_v2_basis_digest(&scope, current.management.authority)?,
            scope,
            termination,
        })
    }
    pub fn test_session_prepare_request_v2(
        &self,
        request: &TestSessionPrepareRequestV2,
        current: &TestSessionManagementCurrent<'_, '_>,
        now: UnixMillis,
    ) -> Result<crate::approval::ApprovalOptionsRequestV1> {
        request.validate()?;
        let expected = self.test_session_basis_v2(
            &TestSessionBasisRequestV2 {
                api_version: 2,
                request_id: request.plan.request_id.clone(),
                enrollment_id: request.plan.enrollment_id.clone(),
                setup_id: request.plan.termination.setup_id.clone(),
                cleanup_until: request.plan.termination.cleanup_until,
            },
            current,
            now,
        )?;
        if request.scope != expected.scope
            || request.plan.termination.as_ref() != &expected.termination
            || request.plan.basis_digest != expected.basis_digest
            || request.plan.owner_setup_plan_digest != expected.owner_setup_plan_digest
            || !request.plan.selection.is_exact_move0()
        {
            return Err(invalid());
        }
        request
            .plan
            .validate_scope(&request.scope, now.get())
            .map_err(|_| invalid())?;
        let prepared = self.owner_setup_prepared_v2(&request.plan.termination.setup_id)?;
        if request.plan.selection != prepared.plan.selection {
            return Err(invalid());
        }
        Ok(crate::approval::ApprovalOptionsRequestV1 {
            approval_request_id: request.approval_request_id.clone(),
            context: current.management.authority.clone(),
            intent: request
                .plan
                .intent(request.scope.clone(), now.get())
                .map_err(|_| invalid())?,
        })
    }
}

#[cfg_attr(test, derive(schemars::JsonSchema))]
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TestSessionApproveRequestV2 {
    pub api_version: u8,
    pub input: ApprovalAdmissionInput,
}
impl TestSessionApproveRequestV2 {
    pub fn validate(&self) -> Result<()> {
        let CustodyIntentV1::TestSessionV2 { plan, scope } = &self.input.request.intent else {
            return Err(invalid());
        };
        if self.api_version != 2 || self.input.operation_id != scope.session_id {
            return Err(invalid());
        }
        plan.validate().map_err(|_| invalid())
    }
}
impl core::fmt::Debug for TestSessionApproveRequestV2 {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str("TestSessionApproveRequestV2(<private>)")
    }
}
#[cfg_attr(test, derive(schemars::JsonSchema))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TestSessionContinueRequestV2 {
    pub api_version: u8,
    pub session_id: String,
}
impl TestSessionContinueRequestV2 {
    pub fn validate(&self) -> Result<()> {
        if self.api_version != 2 || !id(&self.session_id) {
            return Err(invalid());
        }
        Ok(())
    }
}
#[cfg_attr(test, derive(schemars::JsonSchema))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TestSessionApprovalProgressV2 {
    pub api_version: u8,
    pub custody_account_id: String,
    pub provision_id: String,
    pub session_id: String,
    pub phase: crate::approval::ApprovalPhaseV1,
}
impl AccountProvisionApprovalState {
    /// An approval completion receipt is not activation or an execution permit.
    pub fn test_session_v2_approval_progress(
        &self,
        id: &str,
    ) -> Result<TestSessionApprovalProgressV2> {
        let session = self.test_session_v2_record(id)?;
        let phase = if session.consumed_at.is_some() {
            crate::approval::ApprovalPhaseV1::Completed
        } else {
            crate::approval::ApprovalPhaseV1::Pending
        };
        Ok(TestSessionApprovalProgressV2 {
            api_version: 2,
            custody_account_id: self.draft.entry.custody_account_id.clone(),
            provision_id: self.draft.entry.provision_id.clone(),
            session_id: id.into(),
            phase,
        })
    }
}

impl AccountProvisionApprovalState {
    /// Reconstruct the exact server-owned V2 options before any admission.
    /// This codec creates neither a Book record nor roots.
    pub fn test_session_management_admission_v2(
        &self,
        request: &TestSessionApproveRequestV2,
        server_verify_nonce: &str,
        current: &TestSessionManagementCurrent<'_, '_>,
        now: UnixMillis,
    ) -> Result<TestSessionAdmissionV2> {
        request.validate()?;
        if !hex64(server_verify_nonce) {
            return Err(invalid());
        }
        let CustodyIntentV1::TestSessionV2 { plan, scope } = &request.input.request.intent else {
            return Err(invalid());
        };
        let expected = self.test_session_prepare_request_v2(
            &TestSessionPrepareRequestV2 {
                api_version: 2,
                approval_request_id: request.input.request.approval_request_id.clone(),
                plan: plan.as_ref().clone(),
                scope: scope.as_ref().clone(),
            },
            current,
            now,
        )?;
        if request.input.request != expected || request.input.operation_id != scope.session_id {
            return Err(invalid());
        }
        Ok(TestSessionAdmissionV2 {
            input: request.input.clone(),
            verify_nonce: server_verify_nonce.into(),
        })
    }
}

/// Readonly operation receipt. It cannot be used as a Native certificate or
/// substitute for the actual three-root Account record.
#[cfg_attr(test, derive(schemars::JsonSchema))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TestSessionActivationProgressV2 {
    pub api_version: u8,
    pub custody_account_id: String,
    pub provision_id: String,
    pub session_id: String,
    #[serde(deserialize_with = "required_activation_timestamp")]
    #[cfg_attr(test, schemars(with = "RequiredActivationTimestampSchema"))]
    pub activated_at: Option<UnixMillis>,
    pub preflight_pending: bool,
    pub closing: bool,
}
fn required_activation_timestamp<'de, D: serde::Deserializer<'de>>(
    d: D,
) -> std::result::Result<Option<UnixMillis>, D::Error> {
    Option::<UnixMillis>::deserialize(d)
}

#[cfg(test)]
struct RequiredActivationTimestampSchema;
#[cfg(test)]
impl schemars::JsonSchema for RequiredActivationTimestampSchema {
    fn schema_name() -> std::borrow::Cow<'static, str> {
        "RequiredActivationTimestampSchema".into()
    }
    fn inline_schema() -> bool {
        true
    }
    fn json_schema(g: &mut schemars::SchemaGenerator) -> schemars::Schema {
        <Option<UnixMillis> as schemars::JsonSchema>::json_schema(g)
    }
}
