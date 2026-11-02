//! Explicit full-signed-response storage. Metadata is never a live grant.
use crate::{
    DecimalU64,
    approval::{ApprovedWorldV1, bytes_digest},
    context_body::{
        CONTEXT_BODY_MAX_WIRE_BYTES, ContextBodyError, ContextBodyScopeV1, ContextBodySlotKindV1,
    },
    envelope::*,
    role_key_slot::RoleKeySlotScopeV1,
};
use oasis7_agent_api::game::{TaskContextAuthorityModeV1, TaskContextIdentityV1};
use oasis7_client_api::world_service::task_context_projection::ContextBodyCommitmentV2;
use serde::{Deserialize, Serialize};
pub type ContextBodyV2Result<T> = core::result::Result<T, ContextBodyError>;
#[derive(Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ContextBodyFormatV2 {
    FullSignedResponseV2,
}
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ContextBodyScopeV2 {
    pub api_version: u8,
    pub format: ContextBodyFormatV2,
    pub identity_scope: RoleKeySlotScopeV1,
    pub world: ApprovedWorldV1,
    pub session_id: String,
    pub context_operation_id: String,
    pub prepare_operation_id: String,
    pub session_intent_digest: String,
    pub native_binding_digest: String,
    pub slot: ContextBodySlotKindV1,
    pub version: DecimalU64,
    pub write_deadline: DecimalU64,
    pub context: TaskContextIdentityV1,
    pub public_projection_digest: String,
    pub body: ContextBodyCommitmentV2,
}
impl core::fmt::Debug for ContextBodyScopeV2 {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str("ContextBodyScopeV2(<private>)")
    }
}
fn hash(s: &str) -> bool {
    s.len() == 64
        && s.bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}
impl ContextBodyScopeV2 {
    pub fn validate(&self) -> ContextBodyV2Result<()> {
        // Reuse only the existing mechanical metadata bounds. This temporary
        // value never addresses a legacy actor or enters a V1 crypto method.
        ContextBodyScopeV1 {
            api_version: 1,
            identity_scope: self.identity_scope.clone(),
            world: self.world.clone(),
            session_id: self.session_id.clone(),
            context_operation_id: self.context_operation_id.clone(),
            session_intent_digest: self.session_intent_digest.clone(),
            native_binding_digest: self.native_binding_digest.clone(),
            slot: self.slot,
            version: self.version,
            write_deadline: self.write_deadline,
        }
        .validate()?;
        self.context
            .validate()
            .map_err(|_| ContextBodyError::Invalid)?;
        self.body
            .validate()
            .map_err(|_| ContextBodyError::Invalid)?;
        let f = &self.context.fence;
        if self.api_version != 2
            || self.format != ContextBodyFormatV2::FullSignedResponseV2
            || self.context.authority_mode != Some(TaskContextAuthorityModeV1::NativeExecutorV1)
            || self.prepare_operation_id.is_empty()
            || self.prepare_operation_id.len() > 256
            || self.prepare_operation_id.chars().any(char::is_control)
            || !hash(&self.public_projection_digest)
            || f.world_id != self.world.world_id
            || f.genesis_digest != self.world.genesis_digest
            || f.branch_id != self.world.branch_id
            || f.reorg_epoch.0 != self.world.reorg_epoch.0
        {
            return Err(ContextBodyError::Invalid);
        }
        Ok(())
    }
    fn scope_bytes(&self) -> ContextBodyV2Result<Vec<u8>> {
        self.validate()?;
        oasis7_wasm_abi::encode_canonical_cbor(&("oasis7.custody.context-body.scope.v2", self))
            .map_err(|_| ContextBodyError::Encoding)
    }
    pub fn derived_actor_name(&self) -> ContextBodyV2Result<String> {
        Ok(format!(
            "context-body-v2:{}",
            bytes_digest(&self.scope_bytes()?)
        ))
    }
    pub fn envelope_context(&self) -> ContextBodyV2Result<EnvelopeContextV1> {
        let mut c = self.identity_scope.context.clone();
        c.identity.key_id = bytes_digest(&self.scope_bytes()?);
        c.identity.key_version = self.version;
        c.validate().map_err(|_| ContextBodyError::Invalid)?;
        Ok(c)
    }
}
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ContextBodyRecordV2 {
    pub scope: ContextBodyScopeV2,
    pub envelope: KeyEnvelopeV1,
}
impl core::fmt::Debug for ContextBodyRecordV2 {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str("ContextBodyRecordV2(<private>)")
    }
}
impl ContextBodyRecordV2 {
    pub fn validate(&self) -> ContextBodyV2Result<()> {
        self.scope.validate()?;
        self.envelope
            .validate_for(&self.scope.envelope_context()?)
            .map_err(|_| ContextBodyError::Invalid)?;
        let expected = self
            .scope
            .body
            .canonical_bytes
            .0
            .checked_add(crate::envelope::AES_GCM_TAG_BYTES as u64)
            .and_then(|n| n.checked_mul(2))
            .ok_or(ContextBodyError::Invalid)?;
        if self.envelope.ciphertext.len() as u64 != expected {
            return Err(ContextBodyError::Conflict);
        }
        if serde_json::to_vec(self)
            .map_err(|_| ContextBodyError::Encoding)?
            .len()
            > CONTEXT_BODY_MAX_WIRE_BYTES
        {
            return Err(ContextBodyError::ContextTooLarge);
        }
        Ok(())
    }
    pub fn validate_for(&self, expected: &ContextBodyScopeV2) -> ContextBodyV2Result<()> {
        self.validate()?;
        if &self.scope != expected {
            return Err(ContextBodyError::Conflict);
        }
        Ok(())
    }
    pub fn wire_bytes(&self) -> ContextBodyV2Result<Vec<u8>> {
        self.validate()?;
        serde_json::to_vec(self).map_err(|_| ContextBodyError::Encoding)
    }
    pub fn envelope_digest(&self) -> ContextBodyV2Result<String> {
        self.validate()?;
        Ok(bytes_digest(
            &oasis7_wasm_abi::encode_canonical_cbor(&(
                "oasis7.custody.context-body.envelope.v2",
                &self.envelope,
            ))
            .map_err(|_| ContextBodyError::Encoding)?,
        ))
    }
}

#[path = "context_body_v2_factory.rs"]
mod factory;
pub use factory::*;

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ContextBodyStorageFactV2 {
    pub api_version: u8,
    pub scope: ContextBodyScopeV2,
    pub stored: Option<crate::context_body::ContextBodyStoredMetadataV1>,
}
impl core::fmt::Debug for ContextBodyStorageFactV2 {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str("ContextBodyStorageFactV2(<metadata-not-authority>)")
    }
}
impl ContextBodyStorageFactV2 {
    pub fn validate_for(&self, scope: &ContextBodyScopeV2) -> ContextBodyV2Result<()> {
        self.scope.validate()?;
        if self.api_version != 2
            || &self.scope != scope
            || self.stored.as_ref().is_some_and(|s| {
                !hash(&s.envelope_digest)
                    || s.record_wire_bytes.0 == 0
                    || s.record_wire_bytes.0 > CONTEXT_BODY_MAX_WIRE_BYTES as u64
            })
        {
            return Err(ContextBodyError::Invalid);
        }
        Ok(())
    }
}
impl ContextBodyRecordV2 {
    pub fn storage_fact(&self) -> ContextBodyV2Result<ContextBodyStorageFactV2> {
        Ok(ContextBodyStorageFactV2 {
            api_version: 2,
            scope: self.scope.clone(),
            stored: Some(crate::context_body::ContextBodyStoredMetadataV1 {
                envelope_digest: self.envelope_digest()?,
                record_wire_bytes: DecimalU64(self.wire_bytes()?.len() as u64),
            }),
        })
    }
}
