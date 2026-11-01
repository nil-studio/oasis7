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
    pub(in crate::provisioning) calls_reserved: u64,
    pub(in crate::provisioning) tokens_reserved: u64,
    pub(in crate::provisioning) micro_units_reserved: u64,
    pub(in crate::provisioning) reservations: BTreeMap<String, NativeModelReservationRecordV2>,
    pub(in crate::provisioning) http_admissions: BTreeMap<String, NativeModelHttpAdmissionV2>,
    pub(in crate::provisioning) graph_ports: BTreeMap<String, NativeModelReservedPortV2>,
    pub(in crate::provisioning) entry_ports: BTreeMap<String, NativeModelEntryPortRecordV2>,
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
            reservations: BTreeMap::new(),
            http_admissions: BTreeMap::new(),
            entry_ports: BTreeMap::new(),
            graph_ports: BTreeMap::new(),
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
        match (&s.native_model_budget, expected) {
            (None, None) => {}
            (Some(actual), Some(expected)) => {
                let mut binding = actual.clone();
                binding.calls_reserved = 0;
                binding.tokens_reserved = 0;
                binding.micro_units_reserved = 0;
                binding.reservations.clear();
                binding.http_admissions.clear();
                binding.entry_ports.clear();
                binding.graph_ports.clear();
                if binding != expected {
                    return Err(invalid());
                }
                super::native_model_entry_fee_v2::validate_entry_fees(s, actual)?;
                super::native_model_ports_v2::validate_ports(s, actual)?;
                let (plan, _) = s.header()?;
                let budget = plan.model_budget.as_ref().ok_or_else(invalid)?;
                // This mounted prefix admits only the first, still-pending
                // reservation. Later phases require actual authenticated usage
                // and the complete floor/Current consumer, never caller counters.
                if actual.reservations.len() > 1 {
                    return Err(invalid());
                }
                let mut calls = 0u64;
                let mut tokens = 0u64;
                let mut money = 0u64;
                for (key, record) in &actual.reservations {
                    let q = &record.query.query;
                    let paid = actual
                        .http_admissions
                        .get(&record.host_ingress_attempt_id)
                        .ok_or_else(invalid)?;
                    if paid.query != record.query
                        || paid.signed_attempt_digest != record.host_signed_attempt_digest
                        || paid.credential_generation != record.credential_generation
                        || paid.charged_at.get() > record.claimed_at.get()
                    {
                        return Err(invalid());
                    }
                    record
                        .query
                        .verify_input_binding(&s.input, record.claimed_at.get())
                        .map_err(|_| invalid())?;
                    let expected_units = budget.probe_physical_http[0]
                        .0
                        .checked_sub(super::native_model_entry_fee_v2::entry_units()?)
                        .ok_or_else(invalid)?;
                    let maximum_tokens = budget
                        .max_input_tokens
                        .0
                        .checked_add(budget.max_output_tokens.0)
                        .ok_or_else(invalid)?;
                    let maximum_money = budget.maximum_call_price().map_err(|_| invalid())?;
                    if key != &q.reservation_id
                        || q.call_kind
                            != crate::native_model_proxy_v2::NativeModelCallKindV2::ProbeFirst
                        || q.ordinal.0 != 1
                        || q.expected_floor.0 != 0
                        || record.floor != 1
                        || record.physical_units != expected_units
                        || expected_units == 0
                        || record.maximum_tokens != maximum_tokens
                        || record.maximum_micro_units != maximum_money
                        || record.credential_generation.0 == 0
                        || record.claimed_at.get() < actual.consumed_at.get()
                        || s.approved_at
                            .is_none_or(|at| record.claimed_at.get() < at.get())
                        || record.host_ingress_attempt_id.is_empty()
                        || record.host_ingress_attempt_id.len() > 256
                        || record.host_ingress_attempt_id.chars().any(char::is_control)
                        || !hex64(&record.host_signed_attempt_digest)
                    {
                        return Err(invalid());
                    }
                    calls = calls.checked_add(1).ok_or_else(invalid)?;
                    tokens = tokens.checked_add(maximum_tokens).ok_or_else(invalid)?;
                    money = money.checked_add(maximum_money).ok_or_else(invalid)?;
                }
                if actual.calls_reserved != calls
                    || actual.tokens_reserved != tokens
                    || actual.micro_units_reserved != money
                    || calls > budget.max_calls.0
                    || tokens > budget.max_total_tokens.0
                    || money > budget.price_policy.max_total_micro_units.0
                {
                    return Err(invalid());
                }
            }
            _ => return Err(invalid()),
        }
        Ok(())
    }
}

impl SessionNativeModelBudgetV2 {
    pub(in crate::provisioning) fn id_in_use(&self, id: &str) -> bool {
        self.graph_ports.contains_key(id)
            || self.http_admissions.contains_key(id)
            || self.entry_ports.contains_key(id)
            || self
                .http_admissions
                .values()
                .any(|r| r.query.query.query_nonce == id || r.query.query.request_id == id)
            || self.reservations.iter().any(|(key, r)| {
                key == id
                    || r.host_ingress_attempt_id == id
                    || r.query.query.request_id == id
                    || r.query.query.query_nonce == id
            })
    }
    pub(in crate::provisioning) fn reserved_transport_units(&self) -> Result<u64> {
        let entry = u64::try_from(self.http_admissions.len())
            .map_err(|_| invalid())?
            .checked_mul(super::native_model_entry_fee_v2::entry_units()?)
            .ok_or_else(invalid)?;
        self.reservations.values().try_fold(entry, |sum, r| {
            sum.checked_add(r.physical_units).ok_or_else(invalid)
        })
    }
}
