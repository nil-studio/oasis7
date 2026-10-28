//! Closed native binding facts. Signature verification uses caller-owned fixed pins.
use crate::{DecimalU64, approval::bytes_digest, provisioning::PublicRoleKey};
use serde::{Deserialize, Serialize};
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InvalidNativeBinding;
type Result<T> = std::result::Result<T, InvalidNativeBinding>;
fn id(v: &str) -> bool {
    !v.is_empty() && v.len() <= 256 && !v.chars().any(char::is_control) && v.trim() == v
}
fn hex(v: &str, n: usize) -> bool {
    v.len() == n
        && v.bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}
fn check(v: bool) -> Result<()> {
    if v { Ok(()) } else { Err(InvalidNativeBinding) }
}
fn canonical<T: Serialize>(domain: &str, value: &T) -> Result<Vec<u8>> {
    oasis7_wasm_abi::encode_canonical_cbor(&(domain, value)).map_err(|_| InvalidNativeBinding)
}
fn signature(v: &str) -> bool {
    v.len() == 86
        && v.bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
        && matches!(v.as_bytes().last(), Some(b'A' | b'Q' | b'g' | b'w'))
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NativeBindingChallengeV1 {
    pub api_version: u8,
    pub challenge_id: String,
    pub nonce: String,
    pub identity_namespace: String,
    pub identity_instance_epoch: DecimalU64,
    pub registration_operation_id: String,
    pub registration_request_digest: String,
    pub registration_receipt_digest: String,
    pub native_account_id: String,
    pub native_credential_ref: String,
    pub login_public_key: String,
    pub owner_context_ref: String,
    pub issued_at: DecimalU64,
    pub expires_at: DecimalU64,
}
impl NativeBindingChallengeV1 {
    pub fn validate(&self) -> Result<()> {
        check(
            self.api_version == 1
                && [
                    &self.challenge_id,
                    &self.identity_namespace,
                    &self.registration_operation_id,
                    &self.native_account_id,
                    &self.native_credential_ref,
                ]
                .iter()
                .all(|v| id(v))
                && [
                    &self.registration_request_digest,
                    &self.registration_receipt_digest,
                    &self.login_public_key,
                    &self.owner_context_ref,
                ]
                .iter()
                .all(|v| hex(v, 64))
                && hex(&self.nonce, 64)
                && self.identity_instance_epoch.0 > 0
                && self.expires_at.0 > self.issued_at.0
                && self.expires_at.0 - self.issued_at.0 <= 30_000,
        )
    }
    pub fn validate_at(&self, now: u64) -> Result<()> {
        self.validate()?;
        check(self.issued_at.0 <= now && now < self.expires_at.0)
    }
    pub fn signing_bytes(&self) -> Result<Vec<u8>> {
        self.validate()?;
        canonical("oasis7-custody-native-binding-challenge-v1", self)
    }
    pub fn digest(&self) -> Result<String> {
        Ok(bytes_digest(&self.signing_bytes()?))
    }
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SignedNativeBindingChallengeV1 {
    pub challenge: NativeBindingChallengeV1,
    pub key_id: String,
    pub signature: String,
}
impl SignedNativeBindingChallengeV1 {
    pub fn validate(&self) -> Result<()> {
        self.challenge.validate()?;
        check(id(&self.key_id) && signature(&self.signature))
    }
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OwnerNativeBindingStatementV1 {
    pub challenge: NativeBindingChallengeV1,
    pub environment: String,
    pub custody_account_id: String,
    pub provision_id: String,
    pub approved_plan_digest: String,
    pub login_key_ref: String,
    pub binding_operation_id: String,
    pub permit_id: String,
    pub source_revision: String,
    pub account_authority_revision: String,
    pub service_epoch: DecimalU64,
    pub service_identity: String,
    pub signer_key_id: String,
    pub authority_observed_at: DecimalU64,
    pub expires_at: DecimalU64,
}
/// Expected facts must come from the registered Account plan and Identity role result.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NativeBindingScopeV1 {
    pub environment: String,
    pub custody_account_id: String,
    pub provision_id: String,
    pub approved_plan_digest: String,
    pub identity_namespace: String,
    pub identity_instance_epoch: DecimalU64,
    pub registration_operation_id: String,
    pub registration_request_digest: String,
    pub login_key: PublicRoleKey,
    pub plan_expires_at: DecimalU64,
    pub source_expires_at: DecimalU64,
}
impl OwnerNativeBindingStatementV1 {
    pub fn validate(&self) -> Result<()> {
        self.challenge.validate()?;
        check(
            [
                &self.environment,
                &self.custody_account_id,
                &self.provision_id,
                &self.login_key_ref,
                &self.binding_operation_id,
                &self.permit_id,
                &self.service_identity,
                &self.signer_key_id,
            ]
            .iter()
            .all(|v| id(v))
                && [
                    &self.approved_plan_digest,
                    &self.source_revision,
                    &self.account_authority_revision,
                ]
                .iter()
                .all(|v| hex(v, 64))
                && self.challenge.challenge_id == self.binding_operation_id
                && self.service_epoch.0 > 0
                && self.authority_observed_at.0 >= self.challenge.issued_at.0
                && self.expires_at.0 > self.authority_observed_at.0
                && self.expires_at.0 - self.authority_observed_at.0 <= 5_000
                && self.expires_at.0 <= self.challenge.expires_at.0,
        )
    }
    pub fn validate_at(&self, now: u64) -> Result<()> {
        self.validate()?;
        self.challenge.validate_at(now)?;
        check(self.authority_observed_at.0 <= now && now < self.expires_at.0)
    }
    pub fn validate_for(&self, s: &NativeBindingScopeV1, now: u64) -> Result<()> {
        self.validate_at(now)?;
        check(
            self.environment == s.environment
                && self.custody_account_id == s.custody_account_id
                && self.provision_id == s.provision_id
                && self.approved_plan_digest == s.approved_plan_digest
                && self.login_key_ref == s.login_key.key_id
                && s.login_key.key_version.0 > 0
                && self.challenge.login_public_key == s.login_key.public_key
                && self.challenge.identity_namespace == s.identity_namespace
                && self.challenge.identity_instance_epoch == s.identity_instance_epoch
                && self.challenge.registration_operation_id == s.registration_operation_id
                && self.challenge.registration_request_digest == s.registration_request_digest
                && self.expires_at.0 <= s.plan_expires_at.0
                && self.expires_at.0 <= s.source_expires_at.0,
        )
    }
    pub fn signing_bytes(&self) -> Result<Vec<u8>> {
        self.validate()?;
        canonical("oasis7-custody-native-binding-statement-v1", self)
    }
    pub fn digest(&self) -> Result<String> {
        Ok(bytes_digest(&self.signing_bytes()?))
    }
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SignedOwnerNativeBindingStatementV1 {
    pub statement: OwnerNativeBindingStatementV1,
    pub signature: String,
}
impl SignedOwnerNativeBindingStatementV1 {
    pub fn validate(&self) -> Result<()> {
        self.statement.validate()?;
        check(signature(&self.signature))
    }
}
/// A configured current pin; historical pins must never be used to admit new work.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NativeBindingPinV1 {
    pub key_id: String,
    pub public_key: String,
    pub environment: String,
    pub service_identity: String,
    pub service_epoch: DecimalU64,
}
impl NativeBindingPinV1 {
    pub fn validate(&self) -> Result<()> {
        check(
            [&self.key_id, &self.environment, &self.service_identity]
                .iter()
                .all(|v| id(v))
                && hex(&self.public_key, 64)
                && self.service_epoch.0 > 0,
        )
    }
    pub fn validate_statement(&self, v: &OwnerNativeBindingStatementV1, now: u64) -> Result<()> {
        self.validate()?;
        v.validate_at(now)?;
        check(
            v.signer_key_id == self.key_id
                && v.environment == self.environment
                && v.service_identity == self.service_identity
                && v.service_epoch == self.service_epoch,
        )
    }
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NativeBindingReceiptV1 {
    pub api_version: u8,
    pub binding_operation_id: String,
    pub statement_digest: String,
    pub environment: String,
    pub custody_account_id: String,
    pub provision_id: String,
    pub identity_namespace: String,
    pub identity_instance_epoch: DecimalU64,
    pub native_account_id: String,
    pub native_credential_ref: String,
    pub login_public_key: String,
    pub login_key_ref: String,
    pub registration_operation_id: String,
    pub registration_receipt_digest: String,
    pub native_account_security_epoch: DecimalU64,
    pub native_credential_version: DecimalU64,
    pub committed_at: DecimalU64,
}
impl NativeBindingReceiptV1 {
    pub fn validate(&self) -> Result<()> {
        check(
            self.api_version == 1
                && [
                    &self.binding_operation_id,
                    &self.environment,
                    &self.custody_account_id,
                    &self.provision_id,
                    &self.identity_namespace,
                    &self.native_account_id,
                    &self.native_credential_ref,
                    &self.login_key_ref,
                    &self.registration_operation_id,
                ]
                .iter()
                .all(|v| id(v))
                && [
                    &self.statement_digest,
                    &self.login_public_key,
                    &self.registration_receipt_digest,
                ]
                .iter()
                .all(|v| hex(v, 64))
                && self.identity_instance_epoch.0 > 0
                && self.native_account_security_epoch.0 > 0
                && self.native_credential_version.0 > 0,
        )
    }
    pub fn validate_for(&self, s: &OwnerNativeBindingStatementV1) -> Result<()> {
        self.validate()?;
        s.validate_at(self.committed_at.0)?;
        let c = &s.challenge;
        check(
            self.statement_digest == s.digest()?
                && self.binding_operation_id == s.binding_operation_id
                && self.environment == s.environment
                && self.custody_account_id == s.custody_account_id
                && self.provision_id == s.provision_id
                && self.identity_namespace == c.identity_namespace
                && self.identity_instance_epoch == c.identity_instance_epoch
                && self.native_account_id == c.native_account_id
                && self.native_credential_ref == c.native_credential_ref
                && self.login_public_key == c.login_public_key
                && self.login_key_ref == s.login_key_ref
                && self.registration_operation_id == c.registration_operation_id
                && self.registration_receipt_digest == c.registration_receipt_digest,
        )
    }
    pub fn signing_bytes(&self) -> Result<Vec<u8>> {
        self.validate()?;
        canonical("oasis7-custody-native-binding-receipt-v1", self)
    }
    pub fn digest(&self) -> Result<String> {
        Ok(bytes_digest(&self.signing_bytes()?))
    }
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SignedNativeBindingReceiptV1 {
    pub receipt: NativeBindingReceiptV1,
    pub key_id: String,
    pub signature: String,
}
impl SignedNativeBindingReceiptV1 {
    pub fn validate(&self) -> Result<()> {
        self.receipt.validate()?;
        check(id(&self.key_id) && signature(&self.signature))
    }
}

/// Public canonical registration intent, independent of native private HMAC keys.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NativeRegistrationRequestV1 {
    pub identity_namespace: String,
    pub identity_instance_epoch: DecimalU64,
    pub registration_operation_id: String,
    pub login_public_key: String,
    pub display_name: String,
    pub audience: String,
}
impl NativeRegistrationRequestV1 {
    pub fn validate(&self) -> Result<()> {
        check(
            id(&self.identity_namespace)
                && self.identity_instance_epoch.0 > 0
                && id(&self.registration_operation_id)
                && hex(&self.login_public_key, 64)
                && id(&self.display_name)
                && self.display_name.len() <= 128
                && id(&self.audience),
        )
    }
    pub fn signing_bytes(&self) -> Result<Vec<u8>> {
        self.validate()?;
        canonical("oasis7-custody-native-registration-request-v1", self)
    }
    pub fn digest(&self) -> Result<String> {
        Ok(bytes_digest(&self.signing_bytes()?))
    }
}
impl OwnerNativeBindingStatementV1 {
    pub fn possession_signing_bytes(&self) -> Result<Vec<u8>> {
        canonical(
            "oasis7-custody-native-binding-possession-v1",
            &self.digest()?,
        )
    }
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NativeBindingCompleteV1 {
    pub api_version: u8,
    pub statement: SignedOwnerNativeBindingStatementV1,
    pub login_key_signature: String,
}
impl NativeBindingCompleteV1 {
    pub fn validate(&self) -> Result<()> {
        self.statement.validate()?;
        check(self.api_version == 1 && signature(&self.login_key_signature))
    }
}
mod session;
pub use session::*;

mod pin_rotation;
pub use pin_rotation::*;

mod lifetime;
pub use lifetime::*;
