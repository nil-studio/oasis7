//! Closed Native summary contracts; expected values do not prove DOM execution.
use super::*;
#[cfg_attr(test, derive(schemars::JsonSchema))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NativeModelBudgetSummaryContractV1 {
    pub api_version: u8,
    pub summary_digest: String,
}
#[cfg_attr(test, derive(schemars::JsonSchema))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NativeFullSessionSummaryProfileV2 {
    FullTestSessionV2,
}
#[cfg_attr(test, derive(schemars::JsonSchema))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NativeFullSessionSummaryContractV2 {
    pub api_version: u8,
    pub contract: NativeFullSessionSummaryProfileV2,
    pub summary_digest: String,
    #[serde(deserialize_with = "required_nullable")]
    #[cfg_attr(test, schemars(with = "RequiredNullableBudgetContract"))]
    pub model_budget_contract: Option<NativeModelBudgetSummaryContractV1>,
}
// A schema-only non-Option type preserves both required presence and null.
#[cfg(test)]
struct RequiredNullableBudgetContract;
#[cfg(test)]
impl schemars::JsonSchema for RequiredNullableBudgetContract {
    fn schema_name() -> String {
        "RequiredNullableBudgetContract".into()
    }
    fn is_referenceable() -> bool {
        false
    }
    fn json_schema(g: &mut schemars::r#gen::SchemaGenerator) -> schemars::schema::Schema {
        <Option<NativeModelBudgetSummaryContractV1> as schemars::JsonSchema>::json_schema(g)
    }
}
fn required_nullable<'de, D, T>(d: D) -> Result<Option<T>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: Deserialize<'de>,
{
    Option::<T>::deserialize(d)
}
/// Legacy owner-budget wire remains unchanged; the new V2 variant has a fixed
/// required contract tag and cannot deserialize as the legacy deny-unknown DTO.
#[cfg_attr(test, derive(schemars::JsonSchema))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum NativeCustodySummaryContractV1 {
    ModelBudgetV1(NativeModelBudgetSummaryContractV1),
    FullTestSessionV2(NativeFullSessionSummaryContractV2),
}
#[cfg_attr(test, derive(schemars::JsonSchema))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NativeSessionApprovalSummaryV2 {
    pub api_version: u8,
    pub plan: Box<crate::provisioning::TestSessionPlanV2>,
    pub scope: Box<crate::provisioning::ResourceTestSessionScopeV2>,
}
impl ApprovalOptionsRequestV1 {
    fn expected_budget_contract(
        &self,
        budget: &crate::provisioning::ModelInferenceBudgetV1,
    ) -> Result<NativeModelBudgetSummaryContractV1, String> {
        encode(
            "oasis7.custody.native-model-budget-summary.v1",
            &(self.intent_digest()?, &self.context, budget),
        )
        .map(|bytes| NativeModelBudgetSummaryContractV1 {
            api_version: 1,
            summary_digest: bytes_digest(&bytes),
        })
    }
    /// Expected content only. Only the trusted Native signer can attest it.
    pub fn expected_model_budget_summary_contract(
        &self,
    ) -> Result<Option<NativeCustodySummaryContractV1>, String> {
        match &self.intent {
            CustodyIntentV1::OwnerSetupV2 { plan } => plan
                .model_budget
                .as_ref()
                .map(|b| {
                    self.expected_budget_contract(b)
                        .map(NativeCustodySummaryContractV1::ModelBudgetV1)
                })
                .transpose(),
            CustodyIntentV1::TestSessionV2 { plan, scope } => {
                plan.validate()?;
                let nested = plan
                    .model_budget
                    .as_ref()
                    .map(|b| self.expected_budget_contract(b))
                    .transpose()?;
                let bytes = encode(
                    "oasis7.custody.native-full-test-session-summary.v2",
                    &(self.intent_digest()?, &self.context, plan, scope),
                )?;
                Ok(Some(NativeCustodySummaryContractV1::FullTestSessionV2(
                    NativeFullSessionSummaryContractV2 {
                        api_version: 2,
                        contract: NativeFullSessionSummaryProfileV2::FullTestSessionV2,
                        summary_digest: bytes_digest(&bytes),
                        model_budget_contract: nested,
                    },
                )))
            }
            _ => Ok(None),
        }
    }
    pub fn native_session_summary_v2(&self) -> Option<Box<NativeSessionApprovalSummaryV2>> {
        let CustodyIntentV1::TestSessionV2 { plan, scope } = &self.intent else {
            return None;
        };
        Some(Box::new(NativeSessionApprovalSummaryV2 {
            api_version: 2,
            plan: plan.clone(),
            scope: scope.clone(),
        }))
    }
}

#[cfg(test)]
mod schema_tests {
    use super::*;
    #[test]
    fn full_summary_nested_budget_is_required_and_nullable_in_schema() {
        let schema = schemars::schema_for!(NativeFullSessionSummaryContractV2);
        let object = schema.schema.object.unwrap();
        assert!(object.required.contains("model_budget_contract"));
        let property = serde_json::to_value(&object.properties["model_budget_contract"]).unwrap();
        assert!(property.to_string().contains("null"));
    }
}
