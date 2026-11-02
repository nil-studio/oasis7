//! Wire compaction reconstructs the *same* full certificate bytes from Native's
//! exact saved completed consent. It is not another authority or signature domain.
use super::*;
use crate::approval_admission::ApprovalAdmissionInput;
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AccountConsumedPendingReferenceV2 {
    pub api_version: u8,
    pub issuer: AccountMachineIssuerV2,
    pub certificate_id: String,
    pub approval_id: String,
    pub session_id: String,
    pub approved_input_digest: String,
    pub intent_digest: String,
    pub plan_digest: String,
    pub participant_session: crate::provisioning::ParticipantSessionRefV1,
    pub native_lifetime: crate::native_binding::SignedManagedNativeSessionLifetimeV1,
    pub preflight_read_claim: NativeRegistrationReadClaimV2,
    pub consumed_at: DecimalU64,
    pub issued_at: DecimalU64,
    pub expires_at: DecimalU64,
    pub signature_hex: String,
}
impl SignedAccountConsumedPendingV2 {
    pub fn compact_reference(&self) -> Result<AccountConsumedPendingReferenceV2, String> {
        let c = &self.certificate;
        c.validate_shape()?;
        Ok(AccountConsumedPendingReferenceV2 {
            api_version: c.api_version,
            issuer: c.issuer.clone(),
            certificate_id: c.certificate_id.clone(),
            approval_id: c.input.approval.proof.approval_id.clone(),
            session_id: c.input.operation_id.clone(),
            approved_input_digest: c.approved_input_digest.clone(),
            intent_digest: c.intent_digest.clone(),
            plan_digest: c.plan_digest.clone(),
            participant_session: c.participant_session.clone(),
            native_lifetime: c.native_lifetime.clone(),
            preflight_read_claim: c.preflight_read_claim.clone(),
            consumed_at: c.consumed_at,
            issued_at: c.issued_at,
            expires_at: c.expires_at,
            signature_hex: self.signature_hex.clone(),
        })
    }
}
impl AccountConsumedPendingReferenceV2 {
    /// `input` must be reconstructed from Native's exact saved completed record.
    /// The caller then verifies issuer signature against independently configured pins.
    pub fn expand(
        &self,
        input: ApprovalAdmissionInput,
    ) -> Result<SignedAccountConsumedPendingV2, String> {
        if self.api_version != 2
            || input.operation_id != self.session_id
            || input.approval.proof.approval_id != self.approval_id
        {
            return Err(invalid());
        }
        let expanded = SignedAccountConsumedPendingV2 {
            certificate: AccountConsumedPendingV2 {
                api_version: self.api_version,
                issuer: self.issuer.clone(),
                certificate_id: self.certificate_id.clone(),
                input: Box::new(input),
                approved_input_digest: self.approved_input_digest.clone(),
                intent_digest: self.intent_digest.clone(),
                plan_digest: self.plan_digest.clone(),
                participant_session: self.participant_session.clone(),
                native_lifetime: self.native_lifetime.clone(),
                preflight_read_claim: self.preflight_read_claim.clone(),
                consumed_at: self.consumed_at,
                issued_at: self.issued_at,
                expires_at: self.expires_at,
            },
            signature_hex: self.signature_hex.clone(),
        };
        expanded.certificate.validate_shape()?;
        Ok(expanded)
    }
}
impl core::fmt::Debug for AccountConsumedPendingReferenceV2 {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str("AccountConsumedPendingReferenceV2(<private>)")
    }
}
