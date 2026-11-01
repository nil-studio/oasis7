use super::*;
use crate::{
    approval::CustodyIntentV1, approval_admission::ApprovalAdmissionInput,
    native_binding::SignedManagedNativeSessionLifetimeV1, provisioning::ParticipantSessionRefV1,
};
pub const CONSUMED_PENDING_V2_DOMAIN: &str = "oasis7.custody.account-consumed-pending.v2";
pub const ACTIVE_THREE_ROOTS_V2_DOMAIN: &str = "oasis7.custody.account-active-three-roots.v2";
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AccountConsumedPendingV2 {
    pub api_version: u8,
    pub issuer: AccountMachineIssuerV2,
    pub certificate_id: String,
    pub input: Box<ApprovalAdmissionInput>,
    pub approved_input_digest: String,
    pub intent_digest: String,
    pub plan_digest: String,
    pub participant_session: ParticipantSessionRefV1,
    pub native_lifetime: SignedManagedNativeSessionLifetimeV1,
    pub preflight_read_claim: NativeRegistrationReadClaimV2,
    pub consumed_at: DecimalU64,
    pub issued_at: DecimalU64,
    pub expires_at: DecimalU64,
}
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SignedAccountConsumedPendingV2 {
    pub certificate: AccountConsumedPendingV2,
    pub signature_hex: String,
}
impl AccountConsumedPendingV2 {
    pub fn validate_shape(&self) -> Result<(), String> {
        let CustodyIntentV1::TestSessionV2 { plan, scope } = &self.input.request.intent else {
            return Err(invalid());
        };
        plan.validate_scope(scope, self.consumed_at.0)?;
        self.input.request.validate(self.consumed_at.0)?;
        self.native_lifetime.validate().map_err(|_| invalid())?;
        self.preflight_read_claim.validate()?;
        self.participant_session
            .scope
            .validate()
            .map_err(|_| invalid())?;
        // Shape consistency uses the approved declared N pin only. The actual
        // Account factory and Native saved-consent consumer repeat this with
        // independently configured Native public keys before accepting effects.
        self.input.request.validate_native_summary_contract(
            self.input
                .approval
                .proof
                .model_budget_summary_contract
                .as_ref(),
            &self.input.approval.proof.manager_account_id,
            plan.model_budget
                .as_ref()
                .map(|v| v.native_public_key.as_str()),
            self.input.approval.proof.issued_at.0,
        )?;
        if self.input.approval.proof.approval_request_id != self.input.request.approval_request_id
            || self.input.approval.proof.context != self.input.request.context
            || self.api_version != 2
            || !id(&self.certificate_id)
            || self.input.operation_id != scope.session_id
            || self.approved_input_digest
                != hash(&(
                    "oasis7.custody.test-session-approved-input.v2",
                    self.input.as_ref(),
                ))?
            || self.intent_digest != self.input.request.intent_digest()?
            || self.intent_digest != self.input.approval.proof.intent_digest
            || self.plan_digest != plan.digest()?
            || self
                .participant_session
                .scope
                .role_scope
                .context
                .identity
                .account_id
                != plan.termination.custody_account_id
            || self.participant_session.scope.role_scope.provision_id
                != plan.termination.provision_id
            || self
                .participant_session
                .scope
                .role_scope
                .context
                .identity
                .environment
                != self.input.request.context.environment
            || self.participant_session.scope.identity_namespace
                != self
                    .native_lifetime
                    .lifetime
                    .status
                    .receipt
                    .identity_namespace
            || self.participant_session.scope.identity_instance_epoch
                != self
                    .native_lifetime
                    .lifetime
                    .status
                    .receipt
                    .identity_instance_epoch
            || self
                .native_lifetime
                .lifetime
                .status
                .receipt
                .custody_account_id
                != plan.termination.custody_account_id
            || self.native_lifetime.lifetime.status.receipt.provision_id
                != plan.termination.provision_id
            || !matches!(
                self.participant_session.phase,
                crate::native_history::NativeHistoryPhaseV1::InitialLogin
                    | crate::native_history::NativeHistoryPhaseV1::ManagedLogin
            )
            || !id(&self.participant_session.source_permit_id)
            || !id(&self.participant_session.native_operation_id)
            || self.participant_session.private_sid_generation.0 == 0
            || !plan.selection.is_exact_move0()
            || self.preflight_read_claim.claimed_at.0 > self.issued_at.0
            || self.preflight_read_claim.claimed_at.0 < self.consumed_at.0
            || self.consumed_at.0 == 0
            || self.consumed_at.0 > self.issued_at.0
            || self.issued_at.0 >= self.expires_at.0
            || self.expires_at.0 > self.input.approval.proof.expires_at.0
            || self.expires_at.0 > plan.write_deadline.0
            || self.participant_session.session_ref
                != self.native_lifetime.lifetime.status.session_ref
            || self.participant_session.native_binding_generation
                != self.native_lifetime.lifetime.status.generation
            || self.expires_at.0 > self.native_lifetime.lifetime.status.expires_at.0
            || !hex64(&self.approved_input_digest)
            || !hex64(&self.intent_digest)
            || !hex64(&self.plan_digest)
        {
            return Err(invalid());
        }
        Ok(())
    }
    pub fn signing_bytes(&self) -> Result<Vec<u8>, String> {
        self.validate_shape()?;
        bytes(CONSUMED_PENDING_V2_DOMAIN, self)
    }
}
impl SignedAccountConsumedPendingV2 {
    pub fn digest(&self) -> Result<String, String> {
        hash(&(CONSUMED_PENDING_V2_DOMAIN, self))
    }
    pub fn verify_issuer(
        &self,
        pin: &AccountMachineIssuerPinV2,
        floor: &AccountMachineIssuerFloorV2,
        now: u64,
    ) -> Result<(), String> {
        let c = &self.certificate;
        c.validate_shape()?;
        c.issuer.verify_pin(
            pin,
            floor,
            AccountMachinePurposeV2::ConsumedPending,
            c.issued_at.0,
            c.expires_at.0,
            now,
        )?;
        verify(
            CONSUMED_PENDING_V2_DOMAIN,
            c,
            &self.signature_hex,
            &pin.public_key,
        )
    }
}
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AccountActiveThreeRootsV2 {
    pub api_version: u8,
    pub issuer: AccountMachineIssuerV2,
    pub certificate_id: String,
    pub session_id: String,
    pub approved_input_digest: String,
    pub pending_certificate_digest: String,
    pub native_registration_ack_digest: String,
    pub opaque_registration_ref: String,
    pub root_birth_digest: String,
    pub activation_transport: NativeActivationPaidClaimV2,
    pub issued_at: DecimalU64,
    pub expires_at: DecimalU64,
}
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SignedAccountActiveThreeRootsV2 {
    pub certificate: AccountActiveThreeRootsV2,
    pub signature_hex: String,
}
impl AccountActiveThreeRootsV2 {
    pub fn validate_relative(
        &self,
        pending: &SignedAccountConsumedPendingV2,
    ) -> Result<(), String> {
        pending.certificate.validate_shape()?;
        let CustodyIntentV1::TestSessionV2 { plan, scope } =
            &pending.certificate.input.request.intent
        else {
            return Err(invalid());
        };
        self.activation_transport.validate()?;
        if self.api_version != 2
            || self.activation_transport.claimed_at.0 != self.issued_at.0
            || ![
                &self.certificate_id,
                &self.session_id,
                &self.opaque_registration_ref,
            ]
            .iter()
            .all(|s| id(s))
            || ![
                &self.approved_input_digest,
                &self.pending_certificate_digest,
                &self.native_registration_ack_digest,
                &self.root_birth_digest,
            ]
            .iter()
            .all(|s| hex64(s))
            || self.session_id != scope.session_id
            || self.approved_input_digest != pending.certificate.approved_input_digest
            || self.pending_certificate_digest != pending.digest()?
            || self.issued_at.0 < pending.certificate.issued_at.0
            || self.issued_at.0 >= plan.write_deadline.0
            || self.expires_at.0 <= self.issued_at.0
            || self.expires_at.0 > plan.termination.cleanup_until.get()
            || self.expires_at.0
                > pending
                    .certificate
                    .native_lifetime
                    .lifetime
                    .session_expires_at
                    .0
            || self.expires_at.0
                > pending
                    .certificate
                    .native_lifetime
                    .lifetime
                    .session_idle_expires_at
                    .0
            || self.issuer != pending.certificate.issuer
        {
            return Err(invalid());
        }
        Ok(())
    }
}
impl SignedAccountActiveThreeRootsV2 {
    pub fn digest(&self) -> Result<String, String> {
        hash(&(ACTIVE_THREE_ROOTS_V2_DOMAIN, self))
    }
    pub fn verify_issuer(
        &self,
        pending: &SignedAccountConsumedPendingV2,
        pin: &AccountMachineIssuerPinV2,
        floor: &AccountMachineIssuerFloorV2,
        now: u64,
    ) -> Result<(), String> {
        let c = &self.certificate;
        c.validate_relative(pending)?;
        c.issuer.verify_pin(
            pin,
            floor,
            AccountMachinePurposeV2::ActiveThreeRoots,
            c.issued_at.0,
            c.expires_at.0,
            now,
        )?;
        verify(
            ACTIVE_THREE_ROOTS_V2_DOMAIN,
            c,
            &self.signature_hex,
            &pin.public_key,
        )
    }
}
impl core::fmt::Debug for AccountConsumedPendingV2 {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str("AccountConsumedPendingV2(<private>)")
    }
}
impl core::fmt::Debug for SignedAccountConsumedPendingV2 {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str("SignedAccountConsumedPendingV2(<private>)")
    }
}
impl core::fmt::Debug for AccountActiveThreeRootsV2 {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str("AccountActiveThreeRootsV2(<private>)")
    }
}
impl core::fmt::Debug for SignedAccountActiveThreeRootsV2 {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str("SignedAccountActiveThreeRootsV2(<private>)")
    }
}

/// The Account must persist the one Native160 prepayment before issuing the
/// Active certificate. This is a physical request claim, not another root cap.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NativeActivationPaidClaimV2 {
    pub native_transport_attempt_id: String,
    pub native_paid_record_digest: String,
    pub activation_nonce: String,
    pub native_http_units: u8,
    pub claimed_at: DecimalU64,
}
impl NativeActivationPaidClaimV2 {
    pub fn validate(&self) -> Result<(), String> {
        if !id(&self.native_transport_attempt_id)
            || !hex64(&self.native_paid_record_digest)
            || !hex64(&self.activation_nonce)
            || self.native_http_units != 1
        {
            return Err(invalid());
        }
        Ok(())
    }
}
