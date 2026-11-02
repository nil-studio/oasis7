//! Permanent activation acknowledgement; it never replaces fresh Native Current.
use super::*;
pub const NATIVE_ACTIVATION_ACK_V2_DOMAIN: &str = "oasis7.custody.native-activation-ack.v2";
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NativeActivationAckV2 {
    pub api_version: u8,
    pub native_key_id: String,
    pub identity_namespace: String,
    pub identity_epoch: DecimalU64,
    pub session_id: String,
    pub approved_input_digest: String,
    pub opaque_registration_ref: String,
    pub pending_certificate_digest: String,
    pub active_certificate_digest: String,
    pub root_birth_digest: String,
    pub native_transport_attempt_id: String,
    pub activation_nonce: String,
    pub storage_head: NativeStorageHeadV2,
    pub registry_cursor: DecimalU64,
    pub activated_at: DecimalU64,
}
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SignedNativeActivationAckV2 {
    pub ack: NativeActivationAckV2,
    pub signature_hex: String,
}
impl NativeActivationAckV2 {
    pub fn signing_bytes(
        &self,
        pending: &SignedAccountConsumedPendingV2,
        active: &SignedAccountActiveThreeRootsV2,
        registration: &SignedNativeRegistrationAckV2,
    ) -> Result<Vec<u8>, String> {
        self.validate_relative(pending, active, registration)?;
        bytes(NATIVE_ACTIVATION_ACK_V2_DOMAIN, self)
    }
    pub fn validate_relative(
        &self,
        pending: &SignedAccountConsumedPendingV2,
        active: &SignedAccountActiveThreeRootsV2,
        registration: &SignedNativeRegistrationAckV2,
    ) -> Result<(), String> {
        active.validate_registration(pending, registration)?;
        self.storage_head.validate()?;
        let reg = &registration.ack;
        let a = &active.certificate;
        if self.api_version != 2
            || self.native_key_id != reg.native_key_id
            || self.identity_namespace != reg.identity_namespace
            || self.identity_epoch != reg.identity_epoch
            || self.session_id != a.session_id
            || self.approved_input_digest != a.approved_input_digest
            || self.opaque_registration_ref != a.opaque_registration_ref
            || self.pending_certificate_digest != pending.digest()?
            || self.active_certificate_digest != active.digest()?
            || self.root_birth_digest != a.root_birth_digest
            || self.native_transport_attempt_id
                != a.activation_transport.native_transport_attempt_id
            || self.activation_nonce != a.activation_transport.activation_nonce
            || self.storage_head.instance_id != reg.storage_head.instance_id
            || self.storage_head.revision.0 <= reg.storage_head.revision.0
            || self.registry_cursor.0 != reg.registry_cursor.0.checked_add(1).ok_or_else(invalid)?
            || self.activated_at.0 < a.issued_at.0
            || self.activated_at.0 >= a.expires_at.0
        {
            return Err(invalid());
        }
        Ok(())
    }
}
impl SignedNativeActivationAckV2 {
    pub fn digest(&self) -> Result<String, String> {
        hash(&(NATIVE_ACTIVATION_ACK_V2_DOMAIN, self))
    }
    pub fn verify_native(
        &self,
        pending: &SignedAccountConsumedPendingV2,
        active: &SignedAccountActiveThreeRootsV2,
        registration: &SignedNativeRegistrationAckV2,
        key_id: &str,
        public_key: &str,
    ) -> Result<(), String> {
        self.ack.validate_relative(pending, active, registration)?;
        if self.ack.native_key_id != key_id {
            return Err(invalid());
        }
        verify(
            NATIVE_ACTIVATION_ACK_V2_DOMAIN,
            &self.ack,
            &self.signature_hex,
            public_key,
        )
    }
}
impl core::fmt::Debug for NativeActivationAckV2 {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str("NativeActivationAckV2(<private>)")
    }
}
impl core::fmt::Debug for SignedNativeActivationAckV2 {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str("SignedNativeActivationAckV2(<private>)")
    }
}

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NativeActivateRequestV2 {
    pub api_version: u8,
    pub opaque_registration_ref: String,
    pub active: SignedAccountActiveThreeRootsV2,
}
impl NativeActivateRequestV2 {
    pub fn validate_shape(&self) -> Result<(), String> {
        if self.api_version != 2
            || !id(&self.opaque_registration_ref)
            || self.opaque_registration_ref != self.active.certificate.opaque_registration_ref
        {
            return Err(invalid());
        }
        Ok(())
    }
}
impl core::fmt::Debug for NativeActivateRequestV2 {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str("NativeActivateRequestV2(<private>)")
    }
}
