//! NativeRoot paid-response envelope. Ordinary InspectOrigin alone is never this result.
use crate::{
    DecimalU64,
    approval::{OriginStateV1, ProofStateV1, SignedApprovalVerificationV1, VerificationPurposeV1},
    execution_pending_source_inspect_v2::{PendingSourceNativeRequestV2, bounded, decode},
    native_cleanup_v2::{bytes, hash, hex64, verify},
};
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use serde::{Deserialize, Serialize};
pub const PENDING_SOURCE_INSPECT_RESULT_DOMAIN_V2: &str =
    "oasis7.custody.native-pending-source-inspect-result.v2";
fn rejected() -> String {
    "paid source inspect result rejected".into()
}
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PendingSourceInspectResultV2 {
    pub api_version: u8,
    pub key_id: String,
    pub dispatch_digest: String,
    pub native_leaf_digest: String,
    pub native_request_digest: String,
    pub inner: SignedApprovalVerificationV1,
    pub completed_at: DecimalU64,
}
impl PendingSourceInspectResultV2 {
    pub fn validate_shape(&self) -> Result<(), String> {
        let fact = &self.inner.verification;
        if self.api_version != 2
            || self.key_id.is_empty()
            || self.key_id.len() > 256
            || self.key_id.chars().any(char::is_control)
            || fact.key_id != self.key_id
            || !hex64(&self.dispatch_digest)
            || !hex64(&self.native_leaf_digest)
            || !hex64(&self.native_request_digest)
            || self.completed_at.0 == 0
            || fact.purpose != VerificationPurposeV1::InspectOrigin
            || fact.proof_state != ProofStateV1::Active
            || fact.origin_state != OriginStateV1::Active
            || fact.observed_at.0 == 0
            || fact.observed_at.0 > self.completed_at.0
            || fact.proof_expires_at.0 <= self.completed_at.0
            || fact.source_session_expires_at.0 <= self.completed_at.0
            || fact.source_session_idle_expires_at.0 <= self.completed_at.0
        {
            return Err(rejected());
        }
        Ok(())
    }
    pub fn check_request(&self, q: &PendingSourceNativeRequestV2) -> Result<(), String> {
        self.validate_shape()?;
        q.wire_bytes()?;
        let fact = &self.inner.verification;
        let leaf = &q.leaf;
        if self.dispatch_digest != leaf.claim.dispatch.digest()?
            || self.native_leaf_digest != leaf.digest()?
            || self.native_request_digest != q.digest()?
            || fact.verify_nonce != q.request.verify_nonce
            || fact.proof_digest != q.request.proof_digest
            || fact.intent_digest != q.request.expected_intent_digest
            || fact.audience != q.request.expected_audience
            || fact.observed_at.0 < leaf.claim.claimed_at.0
            || self.completed_at.0 >= leaf.claim.expires_at.0
            || self.completed_at.0 >= q.request.approval.proof.expires_at.0
        {
            return Err(rejected());
        }
        Ok(())
    }
}
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SignedPendingSourceInspectResultV2 {
    pub result: PendingSourceInspectResultV2,
    pub signature_hex: String,
}
impl SignedPendingSourceInspectResultV2 {
    pub fn signing_bytes(result: &PendingSourceInspectResultV2) -> Result<Vec<u8>, String> {
        result.validate_shape()?;
        bytes(PENDING_SOURCE_INSPECT_RESULT_DOMAIN_V2, result)
    }
    /// Verify both preserved layers with the fixed Native root. Account still
    /// runs original Source/current semantics before any fresh admission.
    pub fn verify_for(
        &self,
        q: &PendingSourceNativeRequestV2,
        key_id: &str,
        public_key: &str,
    ) -> Result<(), String> {
        self.result.check_request(q)?;
        if self.result.key_id != key_id {
            return Err(rejected());
        }
        verify(
            PENDING_SOURCE_INSPECT_RESULT_DOMAIN_V2,
            &self.result,
            &self.signature_hex,
            public_key,
        )?;
        let signature = URL_SAFE_NO_PAD
            .decode(&self.result.inner.signature)
            .map_err(|_| rejected())?;
        if URL_SAFE_NO_PAD.encode(&signature) != self.result.inner.signature {
            return Err(rejected());
        }
        let signature = ed25519_dalek::Signature::from_slice(&signature).map_err(|_| rejected())?;
        let key = ed25519_dalek::VerifyingKey::from_bytes(&crate::native_cleanup_v2::decode::<32>(
            public_key,
        )?)
        .map_err(|_| rejected())?;
        key.verify_strict(
            &self
                .result
                .inner
                .verification
                .signing_bytes()
                .map_err(|_| rejected())?,
            &signature,
        )
        .map_err(|_| rejected())
    }
    pub fn wire_bytes(&self) -> Result<Vec<u8>, String> {
        self.result.validate_shape()?;
        bounded(self)
    }
    pub fn decode(raw: &[u8]) -> Result<Self, String> {
        let r: Self = decode(raw)?;
        r.wire_bytes()?;
        Ok(r)
    }
    pub fn digest(&self) -> Result<String, String> {
        self.wire_bytes()?;
        hash(self)
    }
}
