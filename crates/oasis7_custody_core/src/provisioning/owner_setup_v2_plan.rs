use super::*;
use oasis7_agent_api::game::GoalContentV1;
#[cfg_attr(test, derive(schemars::JsonSchema))]
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OwnerSetupPlanV2 {
    pub api_version: u8,
    #[serde(deserialize_with = "required_option")]
    pub model_budget: Option<ModelInferenceBudgetV1>,
    pub setup_id: String,
    pub provision_id: String,
    pub participant_receipt_digest: String,
    pub world_identity: crate::approval::ApprovedWorldV1,
    pub owner_key_ref: String,
    pub executor_key_ref: String,
    pub bootstrap_config_version: DecimalU64,
    pub goal_content: GoalContentV1,
    pub create_operation_id: String,
    pub prepare_operation_id: String,
    pub prepare_nonce: String,
    pub publish_operation_id: String,
    pub delegation_operation_id: String,
    pub bind_operation_id: String,
    pub executor_id: String,
    pub executor_public_key: String,
    pub delegation_generation: DecimalU64,
    pub delegation_nonce: DecimalU64,
    #[cfg_attr(test, schemars(with = "super::schema::ActionSelectionSchemaV2"))]
    pub selection: ApprovedActionSelectionV2,
    pub deadline: DecimalU64,
    pub request_limit: DecimalU64,
    pub transport_limit: DecimalU64,
    pub write_limit: DecimalU64,
}
impl core::fmt::Debug for OwnerSetupPlanV2 {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str("OwnerSetupPlanV2(<private>)")
    }
}
impl OwnerSetupPlanV2 {
    pub fn validate(&self) -> Result<(), String> {
        if let Some(budget) = &self.model_budget {
            budget.validate(self.deadline.0)?;
        }
        let ops = [
            &self.create_operation_id,
            &self.prepare_operation_id,
            &self.publish_operation_id,
            &self.delegation_operation_id,
            &self.bind_operation_id,
        ];
        if self.api_version != 2
            || ops.iter().any(|v| !id(v))
            || ops.iter().collect::<std::collections::BTreeSet<_>>().len() != 5
            || [
                &self.setup_id,
                &self.provision_id,
                &self.owner_key_ref,
                &self.executor_key_ref,
                &self.executor_id,
            ]
            .iter()
            .any(|v| !id(v))
            || !hex(&self.participant_receipt_digest)
            || !hex(&self.prepare_nonce)
            || !hex(&self.executor_public_key)
            || !self.world_identity.valid()
            || self.bootstrap_config_version.0 == 0
            || self.delegation_generation.0 != 1
            || self.delegation_nonce.0 != 1
            || self.deadline.0 == 0
            || self.request_limit.0 != 6
            || !(584..=600).contains(&self.transport_limit.0)
            || self.write_limit.0 != 4
        {
            return Err(invalid());
        }
        self.goal_content.validate()?;
        self.selection.validate()?;
        Ok(())
    }
    pub fn digest(&self) -> Result<String, String> {
        self.validate()?;
        digest("oasis7.custody.owner-setup-plan.v2", self)
    }
    pub fn intent(&self) -> Result<CustodyIntentV1, String> {
        self.validate()?;
        Ok(CustodyIntentV1::OwnerSetupV2 {
            plan: Box::new(self.clone()),
        })
    }
}

/// Actual management prepare input. Preparing a UV request does not accept
/// phases, issue a model capability or grant any network dispatch permission.
#[cfg_attr(test, derive(schemars::JsonSchema))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OwnerSetupPrepareRequestV2 {
    pub api_version: u8,
    pub approval_request_id: String,
    pub plan: OwnerSetupPlanV2,
}
impl OwnerSetupPrepareRequestV2 {
    pub fn validate(&self) -> Result<(), String> {
        self.plan.validate()?;
        if self.api_version != 2 || !id(&self.approval_request_id) {
            return Err(invalid());
        }
        Ok(())
    }
}

#[cfg_attr(test, derive(schemars::JsonSchema))]
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OwnerSetupApproveRequestV2 {
    pub api_version: u8,
    pub input: crate::approval_admission::ApprovalAdmissionInput,
    pub plan: OwnerSetupPlanV2,
}
impl OwnerSetupApproveRequestV2 {
    pub fn validate(&self) -> Result<(), String> {
        self.plan.validate()?;
        if self.api_version != 2
            || self.input.operation_id != self.plan.setup_id
            || self.input.request.intent != self.plan.intent()?
        {
            return Err(invalid());
        }
        Ok(())
    }
}
#[cfg_attr(test, derive(schemars::JsonSchema))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OwnerSetupApprovalProgressV2 {
    pub api_version: u8,
    pub custody_account_id: String,
    pub provision_id: String,
    pub setup_id: String,
    pub phase: crate::approval::ApprovalPhaseV1,
    /// This approval result is not a paid inference graph capability.
    pub model_budget_digest: Option<String>,
}
