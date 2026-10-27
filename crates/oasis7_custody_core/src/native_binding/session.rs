use super::*;
/// Signed online observation, never an authorization or a replacement for a current session.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ManagedNativeSessionStatusV1 {
    pub api_version: u8,
    pub receipt: NativeBindingReceiptV1,
    pub service_epoch: DecimalU64,
    pub native_account_security_epoch: DecimalU64,
    pub native_credential_version: DecimalU64,
    pub session_ref: String,
    pub nonce: String,
    pub generation: DecimalU64,
    pub active: bool,
    pub frozen: bool,
    pub pending: bool,
    pub observed_at: DecimalU64,
    pub expires_at: DecimalU64,
}
impl ManagedNativeSessionStatusV1 {
    pub fn validate(&self) -> Result<()> {
        self.receipt.validate()?;
        check(
            self.api_version == 1
                && self.native_account_security_epoch.0
                    >= self.receipt.native_account_security_epoch.0
                && self.native_credential_version.0 >= self.receipt.native_credential_version.0
                && self.service_epoch.0 > 0
                && id(&self.session_ref)
                && hex(&self.nonce, 64)
                && self.generation.0 > 0
                && !(self.active && (self.frozen || self.pending))
                && self.observed_at.0 >= self.receipt.committed_at.0
                && self.expires_at.0 > self.observed_at.0
                && self.expires_at.0 - self.observed_at.0 <= 5_000,
        )
    }
    pub fn validate_for(
        &self,
        r: &NativeBindingReceiptV1,
        nonce: &str,
        generation: DecimalU64,
        epoch: DecimalU64,
        now: u64,
    ) -> Result<()> {
        self.validate()?;
        check(
            &self.receipt == r
                && self.nonce == nonce
                && self.generation == generation
                && self.service_epoch == epoch
                && self.observed_at.0 <= now
                && now < self.expires_at.0,
        )
    }
    pub fn signing_bytes(&self) -> Result<Vec<u8>> {
        self.validate()?;
        canonical("oasis7-custody-managed-native-session-status-v1", self)
    }
    pub fn digest(&self) -> Result<String> {
        Ok(bytes_digest(&self.signing_bytes()?))
    }
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SignedManagedNativeSessionStatusV1 {
    pub status: ManagedNativeSessionStatusV1,
    pub key_id: String,
    pub signature: String,
}
impl SignedManagedNativeSessionStatusV1 {
    pub fn validate(&self) -> Result<()> {
        self.status.validate()?;
        check(id(&self.key_id) && signature(&self.signature))
    }
}
/// Permanent native fact. Sequence comparison is against the persisted receiver watermark.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ManagedNativeInvalidationV1 {
    pub api_version: u8,
    pub receipt: NativeBindingReceiptV1,
    pub service_epoch: DecimalU64,
    pub native_account_security_epoch: DecimalU64,
    pub native_credential_version: DecimalU64,
    pub session_ref: Option<String>,
    pub generation: DecimalU64,
    pub reason: String,
    pub sequence: DecimalU64,
    pub changed_at: DecimalU64,
}
impl ManagedNativeInvalidationV1 {
    pub fn validate(&self) -> Result<()> {
        self.receipt.validate()?;
        check(
            self.api_version == 1
                && self.native_account_security_epoch.0
                    >= self.receipt.native_account_security_epoch.0
                && self.native_credential_version.0 >= self.receipt.native_credential_version.0
                && self.service_epoch.0 > 0
                && self.session_ref.as_ref().is_none_or(|v| id(v))
                && self.generation.0 > 0
                && [
                    "account_invalidated",
                    "credential_invalidated",
                    "session_invalidated",
                    "namespace_changed",
                ]
                .contains(&self.reason.as_str())
                && self.sequence.0 > 0
                && self.changed_at.0 >= self.receipt.committed_at.0,
        )
    }
    pub fn validate_for(
        &self,
        r: &NativeBindingReceiptV1,
        epoch: DecimalU64,
        previous_sequence: DecimalU64,
    ) -> Result<()> {
        self.validate()?;
        check(
            &self.receipt == r
                && self.service_epoch == epoch
                && self.sequence.0 > previous_sequence.0,
        )
    }
    pub fn signing_bytes(&self) -> Result<Vec<u8>> {
        self.validate()?;
        canonical("oasis7-custody-managed-native-invalidation-v1", self)
    }
    pub fn digest(&self) -> Result<String> {
        Ok(bytes_digest(&self.signing_bytes()?))
    }
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SignedManagedNativeInvalidationV1 {
    pub invalidation: ManagedNativeInvalidationV1,
    pub key_id: String,
    pub signature: String,
}
impl SignedManagedNativeInvalidationV1 {
    pub fn validate(&self) -> Result<()> {
        self.invalidation.validate()?;
        check(id(&self.key_id) && signature(&self.signature))
    }
}
