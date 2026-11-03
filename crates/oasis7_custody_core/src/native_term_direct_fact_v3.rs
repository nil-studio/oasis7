//! New Root fact domain. The nested common factual payload has no V2 signature.
use crate::{DecimalU64, native_cleanup_v2::*};
use serde::{Deserialize, Serialize};
const DOMAIN: &str = "oasis7.custody.native-term-direct-current-fact.v3";
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NativeTermDirectFactV3 {
    pub api_version: u8,
    pub frame_digest: String,
    pub local: NativeTermCurrentFactV2,
}
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SignedNativeTermDirectFactV3 {
    pub fact: NativeTermDirectFactV3,
    pub signature_hex: String,
}
/// Fixed recipient and actual retained registration, not a caller approval flag.
pub struct TermDirectFactContextV3<'a> {
    pub pending: &'a SignedAccountConsumedPendingV2,
    pub active: &'a SignedAccountActiveThreeRootsV2,
    pub registration: &'a SignedNativeRegistrationAckV2,
    pub native_key_id: &'a str,
    pub native_public_key: &'a str,
    pub now: u64,
}
impl NativeTermDirectFactV3 {
    pub fn validate_frame(&self, frame: &NativeTermDirectFrameV3) -> Result<(), String> {
        self.local.validate_shape()?;
        let f = &self.local;
        let b = &frame.actor.binding;
        let q = &frame.body;
        if self.api_version != 3
            || self.frame_digest != frame.digest()?
            || f.native_request_digest != self.frame_digest
            || f.session_id != q.session_id
            || f.owned_invocation_id != q.owned_invocation_id
            || f.source_operation_id != q.source_operation_id
            || f.stop_operation_id != q.stop_operation_id
            || f.close_operation_id != q.close_operation_id
            || f.nonce != q.nonce
            || f.lifetime_nonce != q.lifetime_nonce
            || f.origin_nonce != q.origin_nonce
            || f.approved_input_digest != b.approved_input_digest
            || f.active_certificate_digest != b.active_certificate_digest
            || f.opaque_registration_ref != b.opaque_registration_ref
            || f.native_key_id != b.native_key_id
            || f.identity_namespace != b.identity_namespace
            || f.identity_epoch != b.identity_epoch
            || f.native_transport_attempt_id != b.native_attempt_id
            || f.observation_started_at != b.observation_started_at
            || f.issued_at.0 < b.issued_at.0
            || f.expires_at.0 > b.expires_at.0
        {
            return Err(invalid());
        }
        Ok(())
    }
    /// Validate the retained V3 approval independently of the wire signature.
    /// This does not admit Current; the fixed Root and live time are checked by verify_for.
    pub fn validate_approval(
        &self,
        pending: &SignedAccountConsumedPendingV2,
        active: &SignedAccountActiveThreeRootsV2,
        registration: &SignedNativeRegistrationAckV2,
    ) -> Result<(), String> {
        let crate::approval::CustodyIntentV1::TestSessionV2 { plan, .. } =
            &pending.certificate.input.request.intent
        else {
            return Err(invalid());
        };
        if self.api_version != 3
            || plan.termination.cleanup_transport_policy
                != crate::provisioning::CleanupTransportPolicyV2::TermWideFiveSecondOwnedDirectCurrent5V3
        {
            return Err(invalid());
        }
        self.local
            .validate_approval_binding(pending, active, registration)
    }
    pub fn signing_bytes(&self) -> Result<Vec<u8>, String> {
        self.local.validate_shape()?;
        if self.api_version != 3
            || !hex64(&self.frame_digest)
            || self.local.native_request_digest != self.frame_digest
        {
            return Err(invalid());
        }
        bytes(DOMAIN, self)
    }
}
impl SignedNativeTermDirectFactV3 {
    pub fn digest(&self) -> Result<String, String> {
        hash(&(DOMAIN, self))
    }
    pub fn verify_for(
        &self,
        frame: &NativeTermDirectFrameV3,
        context: TermDirectFactContextV3<'_>,
    ) -> Result<(), String> {
        self.fact.validate_frame(frame)?;
        frame.actor.binding.validate_relative(
            &frame.body,
            context.pending,
            context.active,
            context.registration,
        )?;
        self.fact
            .validate_approval(context.pending, context.active, context.registration)?;
        self.verify_frame_root(
            frame,
            context.native_key_id,
            context.native_public_key,
            context.now,
        )
    }
    /// Fixed Root verification at the direct Actor; full approval is rechecked by Core.
    pub fn verify_frame_root(
        &self,
        frame: &NativeTermDirectFrameV3,
        key_id: &str,
        public_key: &str,
        now: u64,
    ) -> Result<(), String> {
        self.fact.validate_frame(frame)?;
        let f = &self.fact.local;
        if f.native_key_id != key_id || f.issued_at.0 > now || now >= f.expires_at.0 {
            return Err(invalid());
        }
        f.lifetime
            .verify_root(key_id, public_key)
            .map_err(|_| invalid())?;
        use base64::Engine as _;
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
        verify(DOMAIN, &self.fact, &self.signature_hex, public_key)
    }
}

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TermDirectPrivateCurrentReplyV3 {
    pub api_version: u8,
    pub frame_digest: String,
    pub actor_object_id: String,
    pub session_ref: String,
    pub private_sid_generation: DecimalU64,
    pub lifetime_digest: String,
    pub actor_capacity: DecimalU64,
    pub actor_claim_count: DecimalU64,
    pub native: SignedNativeTermDirectFactV3,
}

impl TermDirectPrivateCurrentReplyV3 {
    /// The direct Actor reports only its own local metadata. It cannot replace
    /// the fixed Native Root or turn this archived fact into live Current.
    pub fn verify_for(
        &self,
        frame: &NativeTermDirectFrameV3,
        context: TermDirectFactContextV3<'_>,
    ) -> Result<(), String> {
        let original = &frame.body.metadata_session;
        if self.api_version != 3
            || self.frame_digest != frame.digest()?
            || self.actor_object_id != frame.actor.binding.actor_object_id
            || self.session_ref != original.session_ref
            || self.private_sid_generation != original.private_sid_generation
            || self.lifetime_digest
                != self
                    .native
                    .fact
                    .local
                    .lifetime
                    .lifetime
                    .digest()
                    .map_err(|_| invalid())?
            || self.actor_capacity.0 == 0
            || self.actor_capacity.0 > u64::from(u32::MAX)
            || self.actor_claim_count.0 == 0
            || self.actor_claim_count.0 > self.actor_capacity.0
        {
            return Err(invalid());
        }
        self.native.verify_for(frame, context)
    }
}
