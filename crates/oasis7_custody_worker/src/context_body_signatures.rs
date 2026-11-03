//! Native ContextBody WebCrypto adapter. A cache authenticates one exact manifest,
//! while the final Core operation rebuilds all semantic and current checks.
use oasis7_custody_core::approval_admission::{
    ApprovalAdmissionError, ApprovalSignatureVerifier, ExactSignatureChecks,
};
use wasm_bindgen::JsValue;

/// Temporary in-process result only: never deserialize or persist this cache.
pub struct ContextBodySignatureCache(ExactSignatureChecks);
impl ApprovalSignatureVerifier for ContextBodySignatureCache {
    fn verify(&mut self, checks: &ExactSignatureChecks) -> Result<(), ApprovalAdmissionError> {
        if checks == &self.0 {
            Ok(())
        } else {
            Err(ApprovalAdmissionError::SignatureRejected)
        }
    }
}
pub async fn verify(checks: ExactSignatureChecks) -> Result<ContextBodySignatureCache, JsValue> {
    for job in checks.checks() {
        crate::approval_crypto::verify_ed25519(
            job.public_key_hex(),
            job.signature_base64url(),
            job.canonical_bytes(),
        )
        .await
        .map_err(|_| JsValue::from_str("private context signature rejected"))?;
    }
    Ok(ContextBodySignatureCache(checks))
}
