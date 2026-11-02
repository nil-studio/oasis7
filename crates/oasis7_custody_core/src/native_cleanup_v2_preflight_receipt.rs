//! Native facts show that a particular fixed World substep actually completed.
//! A historical receipt is not a new Current, allowance or retry permission.
use super::*;
pub const NATIVE_PREFLIGHT_RECEIPT_DOMAIN_V2: &str =
    "oasis7.custody.native-preflight-world-receipt.v2";
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NativePreflightWorldReceiptV2 {
    pub api_version: u8,
    pub native_key_id: String,
    pub certificate_digest: String,
    pub operation: NativePreflightWorldOperationV2,
    pub physical_attempt_id: String,
    pub body_digest: String,
    pub result_digest: String,
    pub completed_at: DecimalU64,
}
impl SignedAccountPreflightAccessV2 {
    pub fn digest(&self) -> Result<String, String> {
        hash(&(PREFLIGHT_ACCESS_DOMAIN_V2, self))
    }
}
impl NativePreflightWorldReceiptV2 {
    /// Unsigned fact metadata; only the independently fixed Native key can authenticate it.
    pub fn policy_result_digest(
        value:&oasis7_client_api::world_service::participant_discovery::ParticipantPolicyOriginalV1,
    ) -> Result<String, String> {
        hash(value)
    }
    pub fn admission_result_digest(
        value: &oasis7_client_api::world_service::SignedReadRequest<
            oasis7_agent_api::game::GameParticipantAdmissionV1,
        >,
    ) -> Result<String, String> {
        hash(value)
    }
    pub fn validate_relative(
        &self,
        original: &SignedAccountPreflightAccessV2,
        result_digest: &str,
    ) -> Result<(), String> {
        let c = &original.certificate;
        c.validate_shape()?;
        let port = c
            .ports
            .iter()
            .find(|p| p.operation == self.operation)
            .ok_or_else(invalid)?;
        if self.api_version != 2
            || !id(&self.native_key_id)
            || self.certificate_digest != original.digest()?
            || self.physical_attempt_id != port.physical_attempt_id
            || self.body_digest != port.body_digest
            || !hex64(&self.result_digest)
            || self.result_digest != result_digest
            || self.completed_at.0 < c.issued_at.0
            || self.completed_at.0 >= c.expires_at.0
        {
            return Err(invalid());
        }
        Ok(())
    }
    pub fn signing_bytes(&self) -> Result<Vec<u8>, String> {
        bytes(NATIVE_PREFLIGHT_RECEIPT_DOMAIN_V2, self)
    }
}
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SignedNativePreflightWorldReceiptV2 {
    pub receipt: NativePreflightWorldReceiptV2,
    pub signature_hex: String,
}
impl SignedNativePreflightWorldReceiptV2 {
    pub fn verify_native(
        &self,
        original: &SignedAccountPreflightAccessV2,
        result_digest: &str,
        key_id: &str,
        public_key: &str,
    ) -> Result<(), String> {
        self.receipt.validate_relative(original, result_digest)?;
        if self.receipt.native_key_id != key_id {
            return Err(invalid());
        }
        verify(
            NATIVE_PREFLIGHT_RECEIPT_DOMAIN_V2,
            &self.receipt,
            &self.signature_hex,
            public_key,
        )
    }
}
#[derive(Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NativePreflightPolicyReplyV2 {
    pub receipt: SignedNativePreflightWorldReceiptV2,
    pub policy:
        oasis7_client_api::world_service::participant_discovery::ParticipantPolicyOriginalV1,
}
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NativePreflightAuthorizeReplyV2 {
    pub receipt: SignedNativePreflightWorldReceiptV2,
    pub admission: oasis7_client_api::world_service::SignedReadRequest<
        oasis7_agent_api::game::GameParticipantAdmissionV1,
    >,
    pub new_registration_enabled: bool,
}
impl core::fmt::Debug for SignedNativePreflightWorldReceiptV2 {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str("SignedNativePreflightWorldReceiptV2(<private>)")
    }
}
