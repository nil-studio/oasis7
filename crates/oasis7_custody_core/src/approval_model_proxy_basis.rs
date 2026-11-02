//! Native-produced basis is consent evidence only until actual graph prepayment.
use super::*;
#[cfg_attr(test, derive(schemars::JsonSchema))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NativeModelProxyBasisV2 {
    pub api_version: u8,
    pub approval_request_id: String,
    pub intent_digest: String,
    pub context_digest: String,
    pub budget_digest: String,
    pub ledger_id: String,
    pub native_account_id: String,
    pub native_instance_public_key: String,
    /// Opaque actual N-owned connection binding, never provider key or SID.
    pub connection_ref_digest: String,
    pub connection_generation: DecimalU64,
    pub preset_id: String,
    pub preset_revision: DecimalU64,
    pub model: String,
    pub endpoint_policy_digest: String,
    pub physical_graph_digest: String,
    pub price_policy_digest: String,
    pub issued_at: DecimalU64,
    pub expires_at: DecimalU64,
}
fn hex_digest(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c))
}
/// Content input from the actual Native connection owner. This is not a grant
/// and is intentionally not Deserializable from an HTTP request.
pub struct NativeModelProxyConnectionBindingV2 {
    pub native_account_id: String,
    pub native_instance_public_key: String,
    pub connection_ref_digest: String,
    pub connection_generation: DecimalU64,
    pub endpoint_policy_digest: String,
}
impl NativeModelProxyBasisV2 {
    /// Construct public content; only an actual Native Root proof attests it.
    pub fn for_request(
        request: &ApprovalOptionsRequestV1,
        connection: &NativeModelProxyConnectionBindingV2,
        issued_at: u64,
        expires_at: u64,
    ) -> Result<Self, String> {
        let CustodyIntentV1::TestSessionV2 { plan, .. } = &request.intent else {
            return Err("unsupported Native model proxy intent".into());
        };
        let budget = plan.model_budget.as_ref().ok_or("model budget required")?;
        let basis = Self {
            api_version: 2,
            approval_request_id: request.approval_request_id.clone(),
            intent_digest: request.intent_digest()?,
            context_digest: bytes_digest(&encode(
                "oasis7.custody.approval-context.v1",
                &request.context,
            )?),
            budget_digest: budget.digest(plan.write_deadline.0)?,
            ledger_id: budget.ledger_id.clone(),
            native_account_id: connection.native_account_id.clone(),
            native_instance_public_key: connection.native_instance_public_key.clone(),
            connection_ref_digest: connection.connection_ref_digest.clone(),
            connection_generation: connection.connection_generation,
            preset_id: budget.preset_id.clone(),
            preset_revision: budget.preset_revision,
            model: budget.model.clone(),
            endpoint_policy_digest: connection.endpoint_policy_digest.clone(),
            physical_graph_digest: budget.physical_graph_digest.clone(),
            price_policy_digest: bytes_digest(&encode(
                "oasis7.custody.model-price-policy.v1",
                &budget.price_policy,
            )?),
            issued_at: DecimalU64(issued_at),
            expires_at: DecimalU64(expires_at),
        };
        basis.validate_request(request, &connection.native_account_id, issued_at)?;
        Ok(basis)
    }
    pub fn digest(&self) -> Result<String, String> {
        encode("oasis7.custody.native-model-proxy-basis.v2", self).map(|v| bytes_digest(&v))
    }
    pub fn validate_request(
        &self,
        request: &ApprovalOptionsRequestV1,
        manager_account_id: &str,
        now: u64,
    ) -> Result<(), String> {
        use crate::provisioning::ModelBudgetApprovalContractV1;
        let CustodyIntentV1::TestSessionV2 { plan, .. } = &request.intent else {
            return Err("unsupported Native model proxy intent".into());
        };
        let budget = plan.model_budget.as_ref().ok_or("model budget required")?;
        budget.validate(plan.write_deadline.0)?;
        let context_digest = bytes_digest(&encode(
            "oasis7.custody.approval-context.v1",
            &request.context,
        )?);
        let price_digest = bytes_digest(&encode(
            "oasis7.custody.model-price-policy.v1",
            &budget.price_policy,
        )?);
        if self.api_version != 2
            || budget.approval_contract != ModelBudgetApprovalContractV1::NativeCustodyModelProxyV2
            || self.approval_request_id != request.approval_request_id
            || self.intent_digest != request.intent_digest()?
            || self.context_digest != context_digest
            || self.budget_digest != budget.digest(plan.write_deadline.0)?
            || self.ledger_id != budget.ledger_id
            || self.native_account_id != budget.native_account_id
            || self.native_account_id != manager_account_id
            || self.native_instance_public_key != budget.native_instance_public_key
            || self.preset_id != budget.preset_id
            || self.preset_revision != budget.preset_revision
            || self.model != budget.model
            || self.physical_graph_digest != budget.physical_graph_digest
            || self.price_policy_digest != price_digest
            || !hex_digest(&self.connection_ref_digest)
            || !hex_digest(&self.endpoint_policy_digest)
            || self.issued_at.0 > now
            || self.expires_at.0 <= now
            || self.expires_at.0 > budget.deadline.0
            || self.expires_at.0 > budget.price_policy.valid_until.0
        {
            return Err("Native model proxy basis mismatch".into());
        }
        Ok(())
    }
}
