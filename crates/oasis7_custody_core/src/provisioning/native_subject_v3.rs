//! Public non-bearer metadata only. Shape does not prove a live Native binding.
use crate::{DecimalU64, native_binding::ManagedNativeSessionStatusV1};
use serde::{Deserialize, Serialize};
#[cfg_attr(test, derive(schemars::JsonSchema))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NativeSubjectRefV3 {
    pub identity_namespace: String,
    pub identity_instance_epoch: DecimalU64,
    pub native_account_id: String,
    pub native_credential_ref: String,
    pub native_credential_version: DecimalU64,
    pub native_account_security_epoch: DecimalU64,
    pub native_session_ref: String,
    pub login_key_ref: String,
    pub login_public_key: String,
    pub native_binding_receipt_digest: String,
}
fn id(s: &str) -> bool {
    !s.trim().is_empty() && s.len() <= 256 && !s.chars().any(char::is_control)
}
fn hash(s: &str) -> bool {
    s.len() == 64
        && s.bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}
impl NativeSubjectRefV3 {
    pub fn validate(&self) -> Result<(), String> {
        if [
            &self.identity_namespace,
            &self.native_account_id,
            &self.native_credential_ref,
            &self.native_session_ref,
            &self.login_key_ref,
        ]
        .iter()
        .any(|s| !id(s))
            || self.identity_instance_epoch.0 == 0
            || self.native_credential_version.0 == 0
            || !hash(&self.login_public_key)
            || !hash(&self.native_binding_receipt_digest)
        {
            return Err("invalid Native subject reference shape".into());
        }
        let mut key = [0u8; 32];
        for (i, value) in key.iter_mut().enumerate() {
            *value = u8::from_str_radix(&self.login_public_key[i * 2..i * 2 + 2], 16)
                .map_err(|_| "invalid Native login key")?;
        }
        if ed25519_dalek::VerifyingKey::from_bytes(&key)
            .map_err(|_| "invalid Native login key")?
            .is_weak()
        {
            return Err("weak Native login key".into());
        }
        // Native account security epoch0 is not artificially ruled out here.
        Ok(())
    }
    /// Deterministic projection only. The adapter must separately verify the
    /// actual Native signature/pin and match the real V1 session before consume.
    pub fn from_status(status: &ManagedNativeSessionStatusV1) -> Result<Self, String> {
        let r = &status.receipt;
        let v = Self {
            identity_namespace: r.identity_namespace.clone(),
            identity_instance_epoch: r.identity_instance_epoch,
            native_account_id: r.native_account_id.clone(),
            native_credential_ref: r.native_credential_ref.clone(),
            native_credential_version: status.native_credential_version,
            native_account_security_epoch: status.native_account_security_epoch,
            native_session_ref: status.session_ref.clone(),
            login_key_ref: r.login_key_ref.clone(),
            login_public_key: r.login_public_key.clone(),
            native_binding_receipt_digest: r
                .digest()
                .map_err(|_| "invalid Native binding receipt".to_owned())?,
        };
        v.validate()?;
        Ok(v)
    }
}
