use super::*;
use oasis7_agent_api::game::GameTaskFenceV1;
#[cfg_attr(test, derive(schemars::JsonSchema))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ResourceTestSessionScopeV2 {
    pub session_id: String,
    pub native_account_id: String,
    pub world_identity: ApprovedWorldV1,
    pub task_fence: GameTaskFenceV1,
    pub executor_key_ref: String,
    pub client_binding: ApprovedClientBindingV1,
    pub client_pairing: crate::client_enrollment::ClientPairingBindingV1,
    pub runtime_ref: String,
    pub skill_ref: String,
    pub source_ref: String,
    pub termination_plan_digest: String,
}
#[cfg_attr(test, derive(schemars::JsonSchema))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TestSessionPlanV2 {
    pub api_version: u8,
    #[serde(deserialize_with = "required_option")]
    pub model_budget: Option<ModelInferenceBudgetV1>,
    pub request_id: String,
    pub enrollment_id: String,
    pub basis_digest: String,
    pub owner_setup_plan_digest: String,
    #[serde(deserialize_with = "required_option")]
    pub prepared_resource_basis_digest: Option<String>,
    #[cfg_attr(test, schemars(with = "super::schema::ActionSelectionSchemaV2"))]
    pub selection: ApprovedActionSelectionV2,
    pub actions: Vec<ApprovedActionV1>,
    pub allowed_resource_refs: Vec<String>,
    pub write_deadline: DecimalU64,
    pub read_until: DecimalU64,
    pub cleanup_until: DecimalU64,
    pub request_limit: DecimalU64,
    pub transport_limit: DecimalU64,
    pub business_limit: DecimalU64,
    pub read_budget: PurposeBudgetV1,
    pub cleanup_budget: PurposeBudgetV1,
    pub control_budget: ControlBudgetV1,
}
impl TestSessionPlanV2 {
    pub fn validate(&self) -> Result<(), String> {
        if let Some(budget) = &self.model_budget {
            budget.validate(self.write_deadline.0)?;
            if budget.maximum_physical_http()? > self.transport_limit.0 {
                return Err(invalid());
            }
        }
        self.selection.validate()?;
        let required = self.selection.required_resource_refs();
        if required
            .iter()
            .any(|v| !self.allowed_resource_refs.contains(v))
            || self.api_version != 2
            || !id(&self.request_id)
            || !id(&self.enrollment_id)
            || [&self.basis_digest, &self.owner_setup_plan_digest]
                .iter()
                .any(|v| !hex(v))
            || self.actions.len() != 1
            || self.actions[0] != self.selection.approved_action()
            || if self.selection.is_exact_move0() {
                !self.allowed_resource_refs.is_empty()
                    || self.business_limit.0 != 0
                    || self.prepared_resource_basis_digest.is_some()
            } else {
                self.allowed_resource_refs.is_empty()
                    || self
                        .prepared_resource_basis_digest
                        .as_ref()
                        .is_none_or(|v| !hex(v))
            }
            || self.allowed_resource_refs.len() > 256
            || self.allowed_resource_refs.iter().any(|v| !id(v))
            || self
                .allowed_resource_refs
                .iter()
                .collect::<std::collections::BTreeSet<_>>()
                .len()
                != self.allowed_resource_refs.len()
            || !(1..=100).contains(&self.request_limit.0)
            || !(1..=400).contains(&self.transport_limit.0)
            || !(1..=100).contains(&self.read_budget.requests.0)
            || !(1..=300).contains(&self.read_budget.transport.0)
            || self.read_budget.business.0 != 0
            || self.cleanup_budget.requests.0 != 2
            || self.cleanup_budget.transport.0 != 24
            || self.cleanup_budget.business.0 != 0
            || !(1..=80).contains(&self.control_budget.requests.0)
            || !(1..=240).contains(&self.control_budget.transport_per_hop.0)
            || !(1..=160).contains(&self.control_budget.identity_transport.0)
            || !(1..=2).contains(&self.control_budget.transport_hops)
            || self.read_until.0 < self.write_deadline.0
            || self.read_until.0 > self.write_deadline.0.saturating_add(1_800_000)
            || self.cleanup_until.0 < self.write_deadline.0
            || self.cleanup_until.0 > self.write_deadline.0.saturating_add(300_000)
        {
            return Err(invalid());
        }
        Ok(())
    }
    pub fn digest(&self) -> Result<String, String> {
        self.validate()?;
        digest("oasis7.custody.test-session-plan.v2", self)
    }
    pub fn validate_scope(&self, s: &ResourceTestSessionScopeV2, now: u64) -> Result<(), String> {
        if self
            .model_budget
            .as_ref()
            .is_some_and(|b| b.deadline.0 <= now)
        {
            return Err(invalid());
        }
        self.validate()?;
        s.client_pairing.validate().map_err(|_| invalid())?;
        s.task_fence.validate()?;
        let f = &s.task_fence;
        let w = &s.world_identity;
        let client = match &s.client_binding {
            ApprovedClientBindingV1::NativeBrokerCustodyV1 { client_key } => {
                self.control_budget.transport_hops == 1
                    && client_key == &s.client_pairing.client_public_key
                    && s.client_pairing.connector_public_key.is_none()
            }
            ApprovedClientBindingV1::DotMcpCustodyV1 {
                client_key,
                connector_key,
            } => {
                self.control_budget.transport_hops == 2
                    && client_key == &s.client_pairing.client_public_key
                    && s.client_pairing.connector_public_key.as_ref() == Some(connector_key)
            }
        };
        if [
            &s.session_id,
            &s.native_account_id,
            &s.executor_key_ref,
            &s.runtime_ref,
            &s.skill_ref,
            &s.source_ref,
        ]
        .iter()
        .any(|v| !id(v))
            || !hex(&s.termination_plan_digest)
            || !w.valid()
            || f.world_id != w.world_id
            || f.genesis_digest != w.genesis_digest
            || f.branch_id != w.branch_id
            || f.reorg_epoch.0 != w.reorg_epoch.0
            || f.task_status != oasis7_agent_api::game::GameTaskStatusV1::Active
            || !client
            || self.write_deadline.0 <= now
            || self.write_deadline.0 > now.saturating_add(1_800_000)
            || s.client_pairing.enrollment_id != self.enrollment_id
        {
            return Err(invalid());
        }
        Ok(())
    }
    pub fn intent(
        &self,
        s: ResourceTestSessionScopeV2,
        now: u64,
    ) -> Result<CustodyIntentV1, String> {
        self.validate_scope(&s, now)?;
        Ok(CustodyIntentV1::TestSessionV2 {
            plan: Box::new(self.clone()),
            scope: Box::new(s),
        })
    }
}
