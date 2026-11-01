//! Whole-session model accounting is armed by the actual consumed new contract.
//! Arming this record grants no provider dispatch, graph or price qualification.
use super::*;
use crate::time_units::UnixMillis;
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(in crate::provisioning) struct SessionNativeModelBudgetV2 {
    approved_input_digest: String,
    budget_digest: String,
    basis_digest: String,
    consumed_at: UnixMillis,
    // Required empty initial state. Reservation/usage records are introduced
    // together with their actual Current/floor producer, never caller counters.
    calls_reserved: u64,
    tokens_reserved: u64,
    micro_units_reserved: u64,
}
impl SessionNativeModelBudgetV2 {
    fn approved_binding(input: &ApprovalAdmissionInput) -> Result<Option<(String, String)>> {
        let CustodyIntentV1::TestSessionV2 { plan, .. } = &input.request.intent else {
            return Err(invalid());
        };
        let Some(budget) = &plan.model_budget else {
            return Ok(None);
        };
        if budget.approval_contract != ModelBudgetApprovalContractV1::NativeCustodyModelProxyV2 {
            return Ok(None);
        }
        let Some(crate::approval::NativeCustodySummaryContractV1::FullTestSessionProxyV2(contract)) =
            &input.approval.proof.model_budget_summary_contract
        else {
            return Err(invalid());
        };
        // Book crypto is independently verified by the actual consume caller.
        // Shape/replay validation cannot turn this metadata into live authority.
        input
            .request
            .validate_native_summary_contract(
                Some(
                    &crate::approval::NativeCustodySummaryContractV1::FullTestSessionProxyV2(
                        contract.clone(),
                    ),
                ),
                &budget.native_account_id,
                Some(&budget.native_public_key),
                input.approval.proof.issued_at.0,
            )
            .map_err(|_| invalid())?;
        Ok(Some((
            budget
                .digest(plan.write_deadline.0)
                .map_err(|_| invalid())?,
            contract.basis.digest().map_err(|_| invalid())?,
        )))
    }
    pub(in crate::provisioning) fn from_consumed_input(
        input: &ApprovalAdmissionInput,
        now: UnixMillis,
    ) -> Result<Option<Self>> {
        let Some((budget_digest, basis_digest)) = Self::approved_binding(input)? else {
            return Ok(None);
        };
        Ok(Some(Self {
            approved_input_digest: digest(&(
                "oasis7.custody.test-session-approved-input.v2",
                input,
            ))?,
            budget_digest,
            basis_digest,
            consumed_at: now,
            calls_reserved: 0,
            tokens_reserved: 0,
            micro_units_reserved: 0,
        }))
    }
    pub(in crate::provisioning) fn validate_session_binding(
        s: &TestSessionAuthorityV2,
    ) -> Result<()> {
        let expected = if let Some(at) = s.consumed_at {
            Self::from_consumed_input(&s.input, at)?
        } else {
            None
        };
        // Until the producer/records are mounted atomically no arbitrary spent
        // counters, signed floor or resumed provider capability can be restored.
        if s.native_model_budget != expected {
            return Err(invalid());
        }
        Ok(())
    }
}
