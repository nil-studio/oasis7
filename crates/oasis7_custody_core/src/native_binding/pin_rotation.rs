//! Owner service pin rotation facts. Admission requires external fixed-root and new-key verification.
use super::*;
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OwnerPinRotationManifestV1 {
    pub api_version: u8,
    pub from_ledger_digest: String,
    pub identity_namespace: String,
    pub identity_instance_epoch: DecimalU64,
    pub old_pin: NativeBindingPinV1,
    pub new_pin: NativeBindingPinV1,
    pub operation_id: String,
    pub issued_at: DecimalU64,
    pub expires_at: DecimalU64,
}
impl OwnerPinRotationManifestV1 {
    pub fn validate(&self) -> Result<()> {
        self.old_pin.validate()?;
        self.new_pin.validate()?;
        check(
            self.api_version == 1
                && hex(&self.from_ledger_digest, 64)
                && id(&self.identity_namespace)
                && self.identity_instance_epoch.0 > 0
                && id(&self.operation_id)
                && self.old_pin.environment == self.new_pin.environment
                && self.old_pin.service_identity == self.new_pin.service_identity
                && self.new_pin.service_epoch.0 > self.old_pin.service_epoch.0
                && self.new_pin.key_id != self.old_pin.key_id
                && self.new_pin.public_key != self.old_pin.public_key
                && self.expires_at.0 > self.issued_at.0
                && self.expires_at.0 - self.issued_at.0 <= 300_000,
        )
    }
    pub fn validate_at(&self, now: u64) -> Result<()> {
        self.validate()?;
        check(self.issued_at.0 <= now && now < self.expires_at.0)
    }
    /// Every expected value is obtained from the existing native ledger, never the manifest itself.
    pub fn validate_for(
        &self,
        from_ledger_digest: &str,
        current_pin: &NativeBindingPinV1,
        identity_namespace: &str,
        identity_instance_epoch: DecimalU64,
        now: u64,
    ) -> Result<()> {
        self.validate_at(now)?;
        check(
            self.from_ledger_digest == from_ledger_digest
                && &self.old_pin == current_pin
                && self.identity_namespace == identity_namespace
                && self.identity_instance_epoch == identity_instance_epoch,
        )
    }
    pub fn signing_bytes(&self) -> Result<Vec<u8>> {
        self.validate()?;
        canonical("oasis7-custody-owner-pin-rotation-v1", self)
    }
    pub fn digest(&self) -> Result<String> {
        Ok(bytes_digest(&self.signing_bytes()?))
    }
    pub fn possession_signing_bytes(&self) -> Result<Vec<u8>> {
        canonical(
            "oasis7-custody-owner-pin-rotation-possession-v1",
            &self.digest()?,
        )
    }
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SignedOwnerPinRotationManifestV1 {
    pub manifest: OwnerPinRotationManifestV1,
    pub native_root_key_id: String,
    pub native_root_signature: String,
    pub new_owner_signature: String,
}
impl SignedOwnerPinRotationManifestV1 {
    /// Structural validation only. Neither signature nor a caller's public key is authority here.
    pub fn validate(&self) -> Result<()> {
        self.manifest.validate()?;
        check(
            id(&self.native_root_key_id)
                && signature(&self.native_root_signature)
                && signature(&self.new_owner_signature),
        )
    }
    pub fn validate_root_key_id(&self, expected: &str) -> Result<()> {
        self.validate()?;
        check(id(expected) && self.native_root_key_id == expected)
    }
}
