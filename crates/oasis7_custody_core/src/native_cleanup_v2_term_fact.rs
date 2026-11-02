//! Whole-term Native-local facts. New domain and request type, no slot-bound projection.
use super::*;
use crate::native_binding::SignedManagedNativeSessionLifetimeV1;
pub const NATIVE_TERM_CURRENT_FACT_V2_DOMAIN: &str = "oasis7.custody.native-term-current-fact.v2";
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NativeTermCurrentFactV2 {
    pub api_version: u8,
    pub native_key_id: String,
    pub identity_namespace: String,
    pub identity_epoch: DecimalU64,
    pub custody_account_id: String,
    pub session_id: String,
    pub approved_input_digest: String,
    pub active_certificate_digest: String,
    pub opaque_registration_ref: String,
    pub owned_invocation_id: String,
    pub stop_operation_id: String,
    pub close_operation_id: String,
    pub source_operation_id: String,
    pub native_transport_attempt_id: String,
    pub native_request_digest: String,
    pub nonce: String,
    pub origin_nonce: String,
    pub lifetime_nonce: String,
    pub storage_head: NativeStorageHeadV2,
    pub registry_cursor: DecimalU64,
    pub lifetime: SignedManagedNativeSessionLifetimeV1,
    pub origin: crate::approval::SignedApprovalVerificationV1,
    pub observation_started_at: DecimalU64,
    pub issued_at: DecimalU64,
    pub expires_at: DecimalU64,
}
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SignedNativeTermCurrentFactV2 {
    pub fact: NativeTermCurrentFactV2,
    pub signature_hex: String,
}
impl NativeTermCurrentFactV2 {
    pub fn validate_shape(&self) -> Result<(), String> {
        self.storage_head.validate()?;
        self.lifetime.validate().map_err(|_| invalid())?;
        if self.api_version != 2
            || ![
                &self.native_key_id,
                &self.identity_namespace,
                &self.custody_account_id,
                &self.session_id,
                &self.opaque_registration_ref,
                &self.owned_invocation_id,
                &self.stop_operation_id,
                &self.close_operation_id,
                &self.source_operation_id,
                &self.native_transport_attempt_id,
            ]
            .iter()
            .all(|s| id(s))
            || ![
                &self.approved_input_digest,
                &self.active_certificate_digest,
                &self.native_request_digest,
                &self.nonce,
                &self.origin_nonce,
                &self.lifetime_nonce,
            ]
            .iter()
            .all(|s| hex64(s))
            || self.origin.verification.key_id != self.native_key_id
            || self.origin.verification.identity_namespace != self.identity_namespace
            || self.origin.verification.identity_instance_epoch != self.identity_epoch
            || self.origin.verification.purpose
                != crate::approval::VerificationPurposeV1::InspectOrigin
            || self.origin.verification.origin_state != crate::approval::OriginStateV1::Active
            || self.origin.verification.verify_nonce != self.origin_nonce
            || self.lifetime.lifetime.status.nonce != self.lifetime_nonce
            || self.origin.verification.observed_at != self.issued_at
            || self.expires_at.0 > self.origin.verification.source_session_expires_at.0
            || self.expires_at.0 > self.origin.verification.source_session_idle_expires_at.0
            || !self.lifetime.lifetime.status.active
            || self.lifetime.lifetime.status.frozen
            || self.lifetime.lifetime.status.pending
            || self.identity_epoch.0 == 0
            || self.registry_cursor.0 == 0
            || self.identity_namespace != self.lifetime.lifetime.status.receipt.identity_namespace
            || self.identity_epoch
                != self
                    .lifetime
                    .lifetime
                    .status
                    .receipt
                    .identity_instance_epoch
            || self.stop_operation_id == self.close_operation_id
            || self.observation_started_at.0 > self.issued_at.0
            || self.expires_at.0 <= self.issued_at.0
            || self.expires_at.0
                > self
                    .observation_started_at
                    .0
                    .checked_add(5000)
                    .ok_or_else(invalid)?
            || self.lifetime.lifetime.status.observed_at.0 != self.issued_at.0
            || self.expires_at.0 > self.lifetime.lifetime.status.expires_at.0
            || self.expires_at.0 > self.lifetime.lifetime.session_expires_at.0
            || self.expires_at.0 > self.lifetime.lifetime.session_idle_expires_at.0
        {
            return Err(invalid());
        }
        Ok(())
    }
    pub fn validate_envelope(&self, request: &NativeTermCurrentEnvelopeV2) -> Result<(), String> {
        self.validate_shape()?;
        let body = &request.body;
        let paid = &request.paid.edge;
        if self.session_id != body.session_id
            || self.owned_invocation_id != body.owned_invocation_id
            || self.stop_operation_id != body.stop_operation_id
            || self.close_operation_id != body.close_operation_id
            || self.observation_started_at != paid.observation_started_at
            || self.source_operation_id != body.source_operation_id
            || self.nonce != body.nonce
            || self.origin_nonce != body.origin_nonce
            || self.lifetime_nonce != body.lifetime_nonce
            || self.native_request_digest != request.request_digest()?
            || self.native_transport_attempt_id != paid.native_transport_attempt_id
            || self.approved_input_digest != paid.approved_input_digest
            || self.active_certificate_digest != paid.active_certificate_digest
            || self.opaque_registration_ref != paid.opaque_registration_ref
            || self.native_key_id != paid.native_key_id
            || self.identity_namespace != paid.identity_namespace
            || self.identity_epoch != paid.identity_epoch
            || self.issued_at.0 < paid.issued_at.0
            || self.expires_at.0 > paid.expires_at.0
        {
            return Err(invalid());
        }
        Ok(())
    }
    pub fn validate_approval(
        &self,
        pending: &SignedAccountConsumedPendingV2,
        active: &SignedAccountActiveThreeRootsV2,
        ack: &SignedNativeRegistrationAckV2,
    ) -> Result<(), String> {
        self.validate_shape()?;
        active.validate_registration(pending, ack)?;
        let c = &pending.certificate;
        let crate::approval::CustodyIntentV1::TestSessionV2 { plan, .. } = &c.input.request.intent
        else {
            return Err(invalid());
        };
        if !matches!(
            plan.termination.cleanup_transport_policy,
            crate::provisioning::CleanupTransportPolicyV2::TermWideFiveSecondOwnedV2
                | crate::provisioning::CleanupTransportPolicyV2::TermWideFiveSecondOwnedWorld3V2
        ) || self.stop_operation_id != plan.termination.stop_operation_id
            || self.close_operation_id != plan.termination.close_operation_id
        {
            return Err(invalid());
        }
        let p = &c.input.approval.proof;
        let v = &self.origin.verification;
        let expired = self.issued_at.0 >= p.expires_at.0;
        if self.active_certificate_digest != active.digest()?
            || self.opaque_registration_ref != ack.ack.opaque_registration_ref
            || self.identity_namespace != c.input.approval.proof.identity_namespace
            || self.identity_epoch != c.input.approval.proof.identity_instance_epoch
            || self.lifetime.lifetime.status.receipt != c.native_lifetime.lifetime.status.receipt
            || self.approved_input_digest != c.approved_input_digest
            || self.session_id != active.certificate.session_id
            || self.custody_account_id != c.input.request.context.custody_account_id
            || self.issued_at.0 < active.certificate.issued_at.0
            || self.expires_at.0 > active.certificate.expires_at.0
            || self.lifetime.lifetime.status.session_ref != c.participant_session.session_ref
            || self.lifetime.lifetime.status.generation
                != c.participant_session.native_binding_generation
            || v.proof_digest != c.input.approval.digest()?
            || v.approval_request_id != p.approval_request_id
            || v.audience != c.input.request.context.audience
            || v.intent_digest != c.intent_digest
            || v.manager_policy_version != p.manager_policy_version
            || v.manager_security_version != p.manager_security_version
            || v.origin_credential_version != p.origin_credential_version
            || v.proof_expires_at != p.expires_at
            || v.proof_state
                != if expired {
                    crate::approval::ProofStateV1::Expired
                } else {
                    crate::approval::ProofStateV1::Active
                }
        {
            return Err(invalid());
        }
        Ok(())
    }
    pub fn signing_bytes(&self) -> Result<Vec<u8>, String> {
        self.validate_shape()?;
        bytes(NATIVE_TERM_CURRENT_FACT_V2_DOMAIN, self)
    }
}
impl SignedNativeTermCurrentFactV2 {
    pub fn digest(&self) -> Result<String, String> {
        hash(&(NATIVE_TERM_CURRENT_FACT_V2_DOMAIN, self))
    }
    pub fn verify_native(&self, key_id: &str, public_key: &str, now: u64) -> Result<(), String> {
        let f = &self.fact;
        f.validate_shape()?;
        if f.native_key_id != key_id || f.issued_at.0 > now || now >= f.expires_at.0 {
            return Err(invalid());
        }
        f.lifetime
            .verify_root(key_id, public_key)
            .map_err(|_| invalid())?;
        use base64::Engine;
        let signature: [u8; 64] = base64::engine::general_purpose::URL_SAFE_NO_PAD
            .decode(&f.origin.signature)
            .map_err(|_| invalid())?
            .try_into()
            .map_err(|_| invalid())?;
        ed25519_dalek::VerifyingKey::from_bytes(&decode::<32>(public_key)?)
            .map_err(|_| invalid())?
            .verify_strict(
                &f.origin.verification.signing_bytes()?,
                &ed25519_dalek::Signature::from_bytes(&signature),
            )
            .map_err(|_| invalid())?;
        verify(
            NATIVE_TERM_CURRENT_FACT_V2_DOMAIN,
            f,
            &self.signature_hex,
            public_key,
        )
    }
}
impl core::fmt::Debug for NativeTermCurrentFactV2 {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str("NativeTermCurrentFactV2(<private>)")
    }
}
impl core::fmt::Debug for SignedNativeTermCurrentFactV2 {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str("SignedNativeTermCurrentFactV2(<private>)")
    }
}
