//! Native signed facts are observations only; callers still need fresh Account authority.
use oasis7_custody_core::{DecimalU64, native_binding::*};
use wasm_bindgen::JsValue;
type Result<T> = std::result::Result<T, JsValue>;
fn rejected() -> JsValue {
    JsValue::from_str("native provision fact rejected")
}
fn now() -> Result<u64> {
    let now = js_sys::Date::now();
    if !now.is_finite() || now < 0.0 || now.fract() != 0.0 || now > 9_007_199_254_740_991.0 {
        return Err(rejected());
    }
    Ok(now as u64)
}
/// Pins come from fixed deployment config, never from a fact or RPC argument.
pub(crate) struct NativeFactPin {
    pub key_id: String,
    pub public_key: String,
}
impl NativeFactPin {
    async fn verify(&self, key_id: &str, signature: &str, bytes: &[u8]) -> Result<()> {
        if key_id != self.key_id {
            return Err(rejected());
        }
        crate::approval_crypto::verify_ed25519(&self.public_key, signature, bytes)
            .await
            .map_err(|_| rejected())
    }
    pub(crate) async fn registration_challenge(
        &self,
        signed: &SignedNativeBindingChallengeV1,
        registration: &NativeRegistrationRequestV1,
        binding_operation_id: &str,
    ) -> Result<()> {
        signed.validate().map_err(|_| rejected())?;
        let c = &signed.challenge;
        c.validate_at(now()?).map_err(|_| rejected())?;
        if c.challenge_id != binding_operation_id
            || c.identity_namespace != registration.identity_namespace
            || c.identity_instance_epoch != registration.identity_instance_epoch
            || c.registration_operation_id != registration.registration_operation_id
            || c.registration_request_digest != registration.digest().map_err(|_| rejected())?
            || c.login_public_key != registration.login_public_key
        {
            return Err(rejected());
        }
        self.verify(
            &signed.key_id,
            &signed.signature,
            &c.signing_bytes().map_err(|_| rejected())?,
        )
        .await?;
        c.validate_at(now()?).map_err(|_| rejected())
    }
    pub(crate) async fn binding_receipt(
        &self,
        signed: &SignedNativeBindingReceiptV1,
        expected: &OwnerNativeBindingStatementV1,
    ) -> Result<()> {
        signed.validate().map_err(|_| rejected())?;
        signed
            .receipt
            .validate_for(expected)
            .map_err(|_| rejected())?;
        self.verify(
            &signed.key_id,
            &signed.signature,
            &signed.receipt.signing_bytes().map_err(|_| rejected())?,
        )
        .await
    }
    /// Must use this managed account's encrypted original SID/CSRF on transport.
    /// This never accepts the manager browser's session or a cached earlier status.
    pub(crate) async fn session_status(
        &self,
        signed: &SignedManagedNativeSessionStatusV1,
        receipt: &NativeBindingReceiptV1,
        nonce: &str,
        generation: DecimalU64,
        service_epoch: DecimalU64,
    ) -> Result<()> {
        signed.validate().map_err(|_| rejected())?;
        signed
            .status
            .validate_for(receipt, nonce, generation, service_epoch, now()?)
            .map_err(|_| rejected())?;
        if !signed.status.active || signed.status.frozen || signed.status.pending {
            return Err(rejected());
        }
        self.verify(
            &signed.key_id,
            &signed.signature,
            &signed.status.signing_bytes().map_err(|_| rejected())?,
        )
        .await?;
        signed
            .status
            .validate_for(receipt, nonce, generation, service_epoch, now()?)
            .map_err(|_| rejected())
    }
}
