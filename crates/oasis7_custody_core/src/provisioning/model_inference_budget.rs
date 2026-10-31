//! Signed V2 budget shape. Validation never creates a network dispatch permit.
use super::*;
#[cfg_attr(test, derive(schemars::JsonSchema))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ModelBudgetApprovalContractV1 {
    NativeCustodyModelBudgetV1,
}
#[cfg_attr(test, derive(schemars::JsonSchema))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ModelPricePolicyV1 {
    pub currency: String,
    pub input_micro_units_per_million: DecimalU64,
    pub output_micro_units_per_million: DecimalU64,
    pub max_total_micro_units: DecimalU64,
    pub quote_digest: String,
    pub valid_until: DecimalU64,
}
#[cfg_attr(test, derive(schemars::JsonSchema))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ModelInferenceBudgetV1 {
    pub api_version: u8,
    pub approval_contract: ModelBudgetApprovalContractV1,
    pub native_account_id: String,
    pub native_public_key: String,
    pub native_instance_public_key: String,
    pub ledger_id: String,
    pub preset_id: String,
    pub preset_revision: DecimalU64,
    pub model: String,
    pub runtime_digest: String,
    pub skill_digest: String,
    pub tools_digest: String,
    pub context_policy_digest: String,
    pub physical_graph_digest: String,
    /// Includes this phase's H->Account reserve and every authorization edge.
    pub probe_physical_http: [DecimalU64; 2],
    pub inference_physical_http: DecimalU64,
    pub max_calls: DecimalU64,
    pub max_input_tokens: DecimalU64,
    pub max_output_tokens: DecimalU64,
    pub max_total_tokens: DecimalU64,
    pub max_concurrent: DecimalU64,
    pub deadline: DecimalU64,
    pub price_policy: ModelPricePolicyV1,
}
impl ModelInferenceBudgetV1 {
    /// Shape and approved ceilings only; trusted preset/graph/quote verification
    /// belongs to the actual issuer and cannot be asserted by this DTO.
    pub fn validate(&self, parent_deadline: u64) -> Result<(), String> {
        let price = &self.price_policy;
        if self.api_version != 1
            || [
                &self.native_account_id,
                &self.ledger_id,
                &self.preset_id,
                &self.model,
            ]
            .iter()
            .any(|v| !id(v))
            || [
                &self.native_public_key,
                &self.native_instance_public_key,
                &self.runtime_digest,
                &self.skill_digest,
                &self.tools_digest,
                &self.context_policy_digest,
                &self.physical_graph_digest,
                &price.quote_digest,
            ]
            .iter()
            .any(|v| !hex(v))
            || self.native_public_key == self.native_instance_public_key
            || self.preset_revision.0 == 0
            || !(3..=400).contains(&self.max_calls.0)
            || self.max_input_tokens.0 == 0
            || self.max_output_tokens.0 == 0
            || self.max_total_tokens.0 == 0
            || self.max_concurrent.0 != 1
            || self.deadline.0 == 0
            || self.deadline.0 > parent_deadline
            || price.valid_until.0 < self.deadline.0
            || price.currency.len() != 3
            || !price.currency.bytes().all(|v| v.is_ascii_uppercase())
            || price.max_total_micro_units.0 == 0
        {
            return Err(invalid());
        }
        let max_tokens = self
            .max_input_tokens
            .0
            .checked_add(self.max_output_tokens.0)
            .and_then(|v| v.checked_mul(self.max_calls.0))
            .ok_or_else(invalid)?;
        if self.max_total_tokens.0 > max_tokens {
            return Err(invalid());
        }
        // Probes consume the same per-call maximum; no free readiness calls.
        self.maximum_call_price()?;
        if self.maximum_physical_http()? > 400 {
            return Err(invalid());
        }
        Ok(())
    }
    pub fn maximum_physical_http(&self) -> Result<u64, String> {
        if self.probe_physical_http.iter().any(|v| v.0 == 0) || self.inference_physical_http.0 == 0
        {
            return Err(invalid());
        }
        let infer_calls = self.max_calls.0.checked_sub(2).ok_or_else(invalid)?;
        self.inference_physical_http
            .0
            .checked_mul(infer_calls)
            .and_then(|v| v.checked_add(self.probe_physical_http[0].0))
            .and_then(|v| v.checked_add(self.probe_physical_http[1].0))
            .ok_or_else(invalid)
    }
    pub fn maximum_call_price(&self) -> Result<u64, String> {
        let p = &self.price_policy;
        let cost = u128::from(self.max_input_tokens.0)
            .checked_mul(u128::from(p.input_micro_units_per_million.0))
            .and_then(|v| {
                u128::from(self.max_output_tokens.0)
                    .checked_mul(u128::from(p.output_micro_units_per_million.0))
                    .and_then(|o| v.checked_add(o))
            })
            .and_then(|v| v.checked_add(999_999))
            .ok_or_else(invalid)?
            / 1_000_000;
        let cost = u64::try_from(cost).map_err(|_| invalid())?;
        if cost > p.max_total_micro_units.0 {
            return Err(invalid());
        }
        Ok(cost)
    }
    pub fn digest(&self, parent_deadline: u64) -> Result<String, String> {
        self.validate(parent_deadline)?;
        digest("oasis7.custody.model-inference-budget.v1", self)
    }
}
