use super::*;
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
/// Authenticated natural lifetime, distinct from status.expires_at (the five
/// second observation deadline). This read never extends a session or idle TTL.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ManagedNativeSessionLifetimeV1 {
    pub api_version: u8,
    pub status: ManagedNativeSessionStatusV1,
    pub session_expires_at: DecimalU64,
    pub session_idle_expires_at: DecimalU64,
}
impl ManagedNativeSessionLifetimeV1 {
    pub fn validate(&self) -> Result<()> {
        self.status.validate()?;
        check(
            self.api_version == 1
                && self.session_expires_at.0 > self.status.observed_at.0
                && self.session_idle_expires_at.0 > self.status.observed_at.0,
        )
    }
    pub fn validate_for(
        &self,
        receipt: &NativeBindingReceiptV1,
        nonce: &str,
        generation: DecimalU64,
        epoch: DecimalU64,
        now: u64,
    ) -> Result<()> {
        self.validate()?;
        self.status
            .validate_for(receipt, nonce, generation, epoch, now)?;
        check(
            self.status.active
                && !self.status.frozen
                && !self.status.pending
                && now < self.session_expires_at.0
                && now < self.session_idle_expires_at.0,
        )
    }
    pub fn covers(&self, deadline: crate::time_units::UnixMillis) -> Result<()> {
        self.validate()?;
        check(
            deadline.get() <= self.session_expires_at.0
                && deadline.get() <= self.session_idle_expires_at.0,
        )
    }
    pub fn signing_bytes(&self) -> Result<Vec<u8>> {
        self.validate()?;
        canonical("oasis7-custody-managed-native-session-lifetime-v1", self)
    }
    pub fn digest(&self) -> Result<String> {
        Ok(bytes_digest(&self.signing_bytes()?))
    }
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SignedManagedNativeSessionLifetimeV1 {
    pub lifetime: ManagedNativeSessionLifetimeV1,
    pub key_id: String,
    pub signature: String,
}
impl SignedManagedNativeSessionLifetimeV1 {
    pub fn validate(&self) -> Result<()> {
        self.lifetime.validate()?;
        check(id(&self.key_id) && signature(&self.signature))
    }
    /// Fixed configuration supplies both Root key ID and public key.
    pub fn verify_root(&self, expected_key_id: &str, public_key_hex: &str) -> Result<()> {
        self.validate()?;
        check(self.key_id == expected_key_id && hex(public_key_hex, 64))?;
        let mut bytes = [0u8; 32];
        for (i, pair) in public_key_hex.as_bytes().chunks_exact(2).enumerate() {
            bytes[i] = u8::from_str_radix(
                std::str::from_utf8(pair).map_err(|_| InvalidNativeBinding)?,
                16,
            )
            .map_err(|_| InvalidNativeBinding)?;
        }
        let key =
            ed25519_dalek::VerifyingKey::from_bytes(&bytes).map_err(|_| InvalidNativeBinding)?;
        let signature_bytes: [u8; 64] = URL_SAFE_NO_PAD
            .decode(&self.signature)
            .map_err(|_| InvalidNativeBinding)?
            .try_into()
            .map_err(|_| InvalidNativeBinding)?;
        check(!key.is_weak())?;
        key.verify_strict(
            &self.lifetime.signing_bytes()?,
            &ed25519_dalek::Signature::from_bytes(&signature_bytes),
        )
        .map_err(|_| InvalidNativeBinding)
    }
}
