//! Independent signed retention ownership. These storage capabilities preserve
//! original JSON; they never authorize terminal, replay, GC or pin operations.
use crate::{
    incremental_field_member::*,
    incremental_field_overlay::WorldFieldOverlayErrorV1,
    incremental_field_read::WorldFieldReadContextV1,
    incremental_json_records::{
        JsonChangeWorkV1, JsonPathSegmentV1, JsonReadWorkV1, JsonRecordProfileV1,
        JsonSubtreeEditV1, JsonSubtreeQualificationV1, JsonWriteWorkV1, advance_json_subtree_v1,
        copy_json_subtree_metadata_ordinary_v1, prepare_json_subtree_change_v1,
        prepare_json_subtree_v1, qualify_json_subtree_v1,
    },
    incremental_projection::{FieldDescriptor, FieldDestination, FieldOwner, FieldShape},
};
use oasis7_formal_protocol::{
    controlled_authority::formal::{
        FormalError, VerifiedFormalCommitV1,
        bounded_context::{FormalBoundedContextV1, WorkIdentityV1},
    },
    incremental_execution::{
        ExecutionError, ExecutionFence, ExecutionTransaction, PreparedGuard, RootSetV1,
        SealedExecutionBindingV1,
    },
    incremental_material::CheckedCommitCore,
    incremental_retention::RetentionLedgerManifestV1,
    incremental_state::{MerkleNodeReader, StateNamespaceV1, StatePageWriter, StateRootV1},
    incremental_value::{ValueNodeReader, ValueTreeError},
};
use oasis7_proto::memory_budget::{MemoryBudget, MemoryClass, MemoryReservation, ReservedValue};
use std::mem::size_of;
/// All fine work is supplied continuously by the caller; no Default/refill.
pub struct CognitionRetentionWorkV1<'a> {
    pub json: &'a mut JsonReadWorkV1,
    pub producer: &'a mut JsonWriteWorkV1,
    pub changed: &'a mut JsonChangeWorkV1,
    pub processing: &'a mut crate::incremental_runtime_revocation::BusinessDiffProcessingWorkV1,
}
#[path = "incremental_cognition_retention_ledger_io.rs"]
mod io;
fn invalid() -> ValueTreeError {
    ValueTreeError::Invalid("independent Cognition retention ledger")
}
fn formal(e: FormalError) -> ValueTreeError {
    match e {
        FormalError::Invalid(ref s) if s == "bounded archive read cancelled" => {
            ValueTreeError::Cancelled
        }
        FormalError::Invalid(ref s)
            if s == "bounded archive read steps" || s == "bounded archive read bytes" =>
        {
            ValueTreeError::ResourceLimited("retention Common work")
        }
        _ => invalid(),
    }
}
fn execution(e: ExecutionError) -> ValueTreeError {
    match e {
        ExecutionError::Memory(e) => ValueTreeError::Memory(e),
        ExecutionError::ResourceLimited(s) => ValueTreeError::ResourceLimited(s),
        _ => invalid(),
    }
}
fn profile() -> JsonRecordProfileV1 {
    JsonRecordProfileV1 {
        field: FieldDescriptor {
            owner: FieldOwner::Cognition,
            name: "retention_state.json.v1",
            shape: FieldShape::Value,
            destination: FieldDestination::State(StateNamespaceV1::Artifacts),
        },
        schema: [0xbd; 32],
    }
}
/// No constructor: issued only by a genuine signed-role recovery/promotion.
pub struct AcceptedCognitionRetentionLedgerV1 {
    class: MemoryClass,
    manifest: ReservedValue<RetentionLedgerManifestV1>,
    payload: Option<ReservedValue<JsonSubtreeQualificationV1>>,
    record: [u8; 32],
    body: [u8; 32],
    artifact: [u8; 32],
}
impl AcceptedCognitionRetentionLedgerV1 {
    pub fn allocation_class(&self) -> MemoryClass {
        self.class
    }
    pub fn manifest(&self) -> &RetentionLedgerManifestV1 {
        &self.manifest
    }
    pub fn head_record_id(&self) -> [u8; 32] {
        self.record
    }
    pub fn head_body_id(&self) -> [u8; 32] {
        self.body
    }
    pub fn artifact_id(&self) -> [u8; 32] {
        self.artifact
    }
    pub fn validate_binding_v1(
        &self,
        head: &VerifiedFormalCommitV1,
        roots: &RootSetV1,
        common: &mut FormalBoundedContextV1<'_>,
    ) -> Result<(), ValueTreeError> {
        common.charge(8192, 8192).map_err(formal)?;
        self.check(common.memory(), MemoryClass::Ordinary)?;
        let _phase = common.memory().reserve(8192, MemoryClass::Ordinary)?;
        let role =
            oasis7_formal_protocol::incremental_retention::retention_root_id_v1(head.record())
                .map_err(|_| invalid())?;
        let text=head.record().artifact_roots.get(&oasis7_formal_protocol::controlled_authority::replicated_protocol::ArtifactRole::IncrementalCommitBody).ok_or_else(invalid)?;
        let mut body = [0; 32];
        hex::decode_to_slice(text, &mut body).map_err(|_| invalid())?;
        if self.record != head.verified_record_id()
            || self.body != body
            || self.artifact != role
            || roots.digest().map_err(|_| invalid())? != self.manifest.roots
        {
            return Err(invalid());
        }
        Ok(())
    }
    fn check(&self, memory: &MemoryBudget, class: MemoryClass) -> Result<(), ValueTreeError> {
        if !self.manifest.uses_budget(memory)
            || self.class != class
            || self
                .payload
                .as_ref()
                .is_some_and(|q| !q.uses_budget(memory))
        {
            return Err(invalid());
        }
        Ok(())
    }
}
#[path = "incremental_cognition_retention_ledger_producer.rs"]
mod producer;
pub use producer::*;
#[path = "incremental_cognition_retention_ledger_current.rs"]
mod current;
pub use current::*;
#[path = "incremental_cognition_retention_ledger_source.rs"]
mod source;
pub use source::*;
#[path = "incremental_cognition_retention_ledger_projection.rs"]
mod projection;
pub use projection::*;
fn member(error: MemberRecordError) -> ValueTreeError {
    match error {
        MemberRecordError::Value(e) => e,
        other => ValueTreeError::Io(std::io::Error::other(other)),
    }
}

#[cfg(test)]
#[path = "incremental_cognition_retention_ledger_tests.rs"]
mod tests;
