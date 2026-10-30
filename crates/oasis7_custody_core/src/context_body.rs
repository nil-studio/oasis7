//! Closed private storage metadata. These wire values never confer context authority.
use crate::{
    DecimalU64,
    approval::{ApprovedWorldV1, bytes_digest},
    envelope::*,
    http::Role,
    role_key_slot::RoleKeySlotScopeV1,
};
use serde::{Deserialize, Serialize};
pub const CONTEXT_BODY_MAX_BYTES: usize = MAX_ENVELOPE_VALUE_BYTES;
pub const CONTEXT_BODY_MAX_WIRE_BYTES: usize = 256 * 1024;
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContextBodyError {
    Invalid,
    Encoding,
    ContextTooLarge,
    Conflict,
}
type Result<T> = core::result::Result<T, ContextBodyError>;
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ContextBodySlotKindV1 {
    Prepared,
    Captured,
}
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ContextBodyScopeV1 {
    pub api_version: u8,
    pub identity_scope: RoleKeySlotScopeV1,
    pub world: ApprovedWorldV1,
    pub session_id: String,
    pub context_operation_id: String,
    pub session_intent_digest: String,
    pub native_binding_digest: String,
    pub slot: ContextBodySlotKindV1,
    pub version: DecimalU64,
    pub write_deadline: DecimalU64,
}
impl core::fmt::Debug for ContextBodyScopeV1 {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str("ContextBodyScopeV1(<private>)")
    }
}
fn identifier(s: &str) -> bool {
    !s.is_empty() && s.len() <= 256 && !s.chars().any(char::is_control)
}
fn hash(s: &str) -> bool {
    s.len() == 64
        && s.bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}
impl ContextBodyScopeV1 {
    pub fn validate(&self) -> Result<()> {
        self.identity_scope
            .validate()
            .map_err(|_| ContextBodyError::Invalid)?;
        if !self.world.valid() {
            return Err(ContextBodyError::Invalid);
        }
        if self.api_version != 1
            || self.version.0 != 1
            || self.write_deadline.0 == 0
            || self.identity_scope.context.identity.role != Role::Identity
            || !identifier(&self.session_id)
            || !identifier(&self.context_operation_id)
            || !hash(&self.session_intent_digest)
            || !hash(&self.native_binding_digest)
        {
            return Err(ContextBodyError::Invalid);
        }
        Ok(())
    }
    fn scope_bytes(&self) -> Result<Vec<u8>> {
        self.validate()?;
        oasis7_wasm_abi::encode_canonical_cbor(&("oasis7.custody.context-body.scope.v1", self))
            .map_err(|_| ContextBodyError::Encoding)
    }
    pub fn derived_actor_name(&self) -> Result<String> {
        Ok(format!(
            "context-body:{}",
            bytes_digest(&self.scope_bytes()?)
        ))
    }
    pub fn envelope_context(&self) -> Result<EnvelopeContextV1> {
        let mut context = self.identity_scope.context.clone();
        context.identity.key_id = bytes_digest(&self.scope_bytes()?);
        context.identity.key_version = self.version;
        context.validate().map_err(|_| ContextBodyError::Invalid)?;
        Ok(context)
    }
}
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ContextBodyRecordV1 {
    pub scope: ContextBodyScopeV1,
    pub envelope: KeyEnvelopeV1,
}
impl core::fmt::Debug for ContextBodyRecordV1 {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str("ContextBodyRecordV1(<private>)")
    }
}
impl ContextBodyRecordV1 {
    pub fn validate(&self) -> Result<()> {
        self.scope.validate()?;
        self.envelope
            .validate_for(&self.scope.envelope_context()?)
            .map_err(|_| ContextBodyError::Invalid)?;
        if serde_json::to_vec(self)
            .map_err(|_| ContextBodyError::Encoding)?
            .len()
            > CONTEXT_BODY_MAX_WIRE_BYTES
        {
            return Err(ContextBodyError::ContextTooLarge);
        }
        Ok(())
    }
    pub fn validate_for(&self, scope: &ContextBodyScopeV1) -> Result<()> {
        self.validate()?;
        if &self.scope != scope {
            return Err(ContextBodyError::Conflict);
        }
        Ok(())
    }
    pub fn wire_bytes(&self) -> Result<Vec<u8>> {
        self.validate()?;
        serde_json::to_vec(self).map_err(|_| ContextBodyError::Encoding)
    }
    pub fn envelope_digest(&self) -> Result<String> {
        self.validate()?;
        Ok(bytes_digest(
            &oasis7_wasm_abi::encode_canonical_cbor(&(
                "oasis7.custody.context-body.envelope.v1",
                &self.envelope,
            ))
            .map_err(|_| ContextBodyError::Encoding)?,
        ))
    }
    pub fn storage_fact(&self) -> Result<ContextBodyStorageFactV1> {
        Ok(ContextBodyStorageFactV1 {
            scope: self.scope.clone(),
            stored: Some(ContextBodyStoredMetadataV1 {
                envelope_digest: self.envelope_digest()?,
                record_wire_bytes: DecimalU64(self.wire_bytes()?.len() as u64),
            }),
        })
    }
}
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ContextBodyStoredMetadataV1 {
    pub envelope_digest: String,
    pub record_wire_bytes: DecimalU64,
}
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ContextBodyStorageFactV1 {
    pub scope: ContextBodyScopeV1,
    pub stored: Option<ContextBodyStoredMetadataV1>,
}
impl core::fmt::Debug for ContextBodyStorageFactV1 {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str("ContextBodyStorageFactV1(<metadata-not-authority>)")
    }
}
impl ContextBodyStorageFactV1 {
    pub fn validate(&self) -> Result<()> {
        self.scope.validate()?;
        if self.stored.as_ref().is_some_and(|s| {
            !hash(&s.envelope_digest)
                || s.record_wire_bytes.0 == 0
                || s.record_wire_bytes.0 > CONTEXT_BODY_MAX_WIRE_BYTES as u64
        }) {
            return Err(ContextBodyError::Invalid);
        }
        Ok(())
    }
    pub fn validate_for(&self, scope: &ContextBodyScopeV1) -> Result<()> {
        self.validate()?;
        if &self.scope != scope {
            return Err(ContextBodyError::Conflict);
        }
        Ok(())
    }
}

mod artifact;
pub use artifact::{
    ContextBodyEvidenceV1, ContextBodyResponseMaterialV1, ContextPrefixEvidenceV1,
    ContextPrefixProofV1, ValidatedContextBody,
};

mod storage_factory;
pub use storage_factory::{
    CapturedContextStorageInput, ContextStorageError, PreparedContextStorageInput,
    captured_context_storage_checks, prepared_context_storage_checks,
    validate_captured_context_storage, validate_prepared_context_storage,
};

#[cfg(test)]
mod storage_fixture_tests;
