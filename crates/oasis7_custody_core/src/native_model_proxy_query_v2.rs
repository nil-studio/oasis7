//! N-signed first-reserve possession. This is never a Current or provider grant.
use crate::{
    DecimalU64, approval_admission::ApprovalAdmissionInput,
    native_model_proxy_v2::NativeModelCallKindV2,
};
use serde::{Deserialize, Serialize};
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum NativeModelRequestCommitmentV2 {
    EncodedRequestV2 { provider_request_digest: String },
    AwaitingCapturedV2 { context_policy_digest: String },
}
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NativeModelReservationQueryV2 {
    pub api_version: u8,
    pub request_id: String,
    pub reservation_id: String,
    pub provision_id: String,
    pub custody_account_id: String,
    pub session_id: String,
    pub approved_input_digest: String,
    pub budget_digest: String,
    pub basis_digest: String,
    pub request_commitment: NativeModelRequestCommitmentV2,
    pub physical_graph: crate::native_model_graph_v2::NativeModelPhysicalGraphV2,
    pub call_kind: NativeModelCallKindV2,
    pub ordinal: DecimalU64,
    pub expected_floor: DecimalU64,
    pub query_nonce: String,
    pub issued_at: DecimalU64,
    pub admit_until: DecimalU64,
}
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SignedNativeModelReservationQueryV2 {
    pub query: NativeModelReservationQueryV2,
    pub signature_hex: String,
}
fn invalid() -> String {
    "invalid Native model reserve query".into()
}
fn hex32(v: &str) -> bool {
    v.len() == 64
        && v.bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}
impl NativeModelReservationQueryV2 {
    pub fn signing_bytes(&self) -> Result<Vec<u8>, String> {
        self.physical_graph.validate()?;
        if self.request_id == self.reservation_id
            || self.request_id == self.query_nonce
            || self.reservation_id == self.query_nonce
        {
            return Err(invalid());
        }
        if self.api_version != 2
            || [
                &self.request_id,
                &self.reservation_id,
                &self.provision_id,
                &self.custody_account_id,
                &self.session_id,
            ]
            .iter()
            .any(|v| v.is_empty() || v.len() > 256)
            || [
                &self.approved_input_digest,
                &self.budget_digest,
                &self.basis_digest,
            ]
            .iter()
            .any(|v| !hex32(v))
            || self.issued_at.0 >= self.admit_until.0
            || self.admit_until.0 > self.issued_at.0.checked_add(5000).ok_or_else(invalid)?
            || match self.call_kind {
                NativeModelCallKindV2::ProbeFirst => self.ordinal.0 != 1,
                NativeModelCallKindV2::ProbeSecond => self.ordinal.0 != 2,
                NativeModelCallKindV2::Inference => self.ordinal.0 < 3,
            }
        {
            return Err(invalid());
        }
        if !crate::http_pop::nonce_valid(&self.query_nonce) {
            return Err(invalid());
        }
        let commitment = match &self.request_commitment {
            NativeModelRequestCommitmentV2::EncodedRequestV2 {
                provider_request_digest,
            } => provider_request_digest,
            NativeModelRequestCommitmentV2::AwaitingCapturedV2 {
                context_policy_digest,
            } => {
                self.physical_graph.validate_awaiting_captured()?;
                if self.call_kind != NativeModelCallKindV2::Inference {
                    return Err(invalid());
                }
                context_policy_digest
            }
        };
        if !hex32(commitment) {
            return Err(invalid());
        }
        crate::native_cleanup_v2::bytes("oasis7.custody.native-model-reserve-query.v2", self)
    }
}
impl SignedNativeModelReservationQueryV2 {
    /// Cryptographic check for this closed N query only. The caller supplies a
    /// configured/approved instance key; this grants no Current, input or floor.
    pub fn verify_instance_key(&self, expected_instance_key: &str) -> Result<(), String> {
        self.query.signing_bytes()?;
        crate::native_cleanup_v2::verify(
            "oasis7.custody.native-model-reserve-query.v2",
            &self.query,
            &self.signature_hex,
            expected_instance_key,
        )
    }
    /// Only the real consumed/activated Account record supplies `input` here.
    /// Strong N proof precedes using its nonce in H RFC validation; successful
    /// verification alone consumes neither nonce nor budget and grants no IO.
    pub fn verify_input_binding(
        &self,
        input: &ApprovalAdmissionInput,
        now: u64,
    ) -> Result<(), String> {
        use crate::{
            approval::CustodyIntentV1, approval::NativeCustodySummaryContractV1,
            provisioning::ModelBudgetApprovalContractV1,
        };
        let CustodyIntentV1::TestSessionV2 { plan, scope } = &input.request.intent else {
            return Err(invalid());
        };
        let Some(budget) = &plan.model_budget else {
            return Err(invalid());
        };
        let Some(NativeCustodySummaryContractV1::FullTestSessionProxyV2(contract)) =
            &input.approval.proof.model_budget_summary_contract
        else {
            return Err(invalid());
        };
        self.query.signing_bytes()?;
        self.query.physical_graph.validate_budget(budget)?;
        if budget.approval_contract != ModelBudgetApprovalContractV1::NativeCustodyModelProxyV2
            || self.query.session_id != scope.session_id
            || input.operation_id != scope.session_id
            || self.query.provision_id != scope.client_pairing.target.provision_id
            || self.query.custody_account_id != scope.client_pairing.target.custody_account_id
            || self.query.approved_input_digest != input.approved_test_session_v2_digest()?
            || self.query.budget_digest != budget.digest(plan.write_deadline.0)?
            || self.query.basis_digest != contract.basis.digest()?
            || self.query.ordinal.0 > budget.max_calls.0
            || self.query.issued_at.0 > now
            || now >= self.query.admit_until.0
            || self.query.admit_until.0 > budget.deadline.0
            || self.query.admit_until.0 > plan.write_deadline.0
        {
            return Err(invalid());
        }
        if matches!(&self.query.request_commitment,
            NativeModelRequestCommitmentV2::AwaitingCapturedV2{context_policy_digest}
            if context_policy_digest!=&budget.context_policy_digest)
        {
            return Err(invalid());
        }
        self.verify_instance_key(&budget.native_instance_public_key)
    }
}
