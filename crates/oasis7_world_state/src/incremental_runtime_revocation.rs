//! Closed typed persistence for the complete original revocation DTO.
use crate::{
    incremental_field_member::*, incremental_field_presence::*,
    incremental_field_read::WorldFieldReadContextV1, incremental_record_columns_read::*,
    incremental_record_tree::RecordTreeLabelsV1, incremental_semantic_key::hash_semantic_key_v1,
};
use oasis7_formal_protocol::{
    incremental_record_root::RecordRootRefV1,
    incremental_state::{
        MerkleNodeReader, StateNamespaceV1, StatePageWriter, StateRootV1, StateScanBudget,
    },
    incremental_value::{ValueNodeReader, ValueNodeWriter, ValueTreeError},
};
use oasis7_proto::memory_budget::MemoryClass;
fn invalid() -> ValueTreeError {
    ValueTreeError::Invalid("closed runtime revocation columns")
}
fn field() -> FieldDescriptor {
    FieldDescriptor {
        owner: FieldOwner::Runtime,
        name: "capability_revocation_state.columns",
        shape: FieldShape::Map,
        destination: FieldDestination::State(StateNamespaceV1::Authorization),
    }
}
fn schema() -> [u8; 32] {
    *blake3::hash(b"oasis7.iws.runtime-revocation.columns.v1\0").as_bytes()
}
const LABELS: RecordTreeLabelsV1 = RecordTreeLabelsV1 {
    read: "runtime revocation child state read",
    pending: "runtime revocation pending node allowance",
    tree: "runtime revocation typed child state tree",
    digest: "runtime revocation state node digest",
    read_allowance: "runtime revocation child read allowance",
    admission: "runtime revocation child source admission",
    bound: "runtime revocation child source bound",
    producer_pool: "runtime revocation producer pool",
    page: "runtime revocation state page",
    write_allowance: "runtime revocation write allowance",
};

use crate::incremental_typed_record_codec::{
    Codec, Decoder, Encoded, TypedRecordProfileV1, TypedTreeBuilderV1, dto,
};
pub use crate::incremental_typed_record_codec::{
    RecoveredTypedRecordV1 as RevocationRecoveredV1, TypedRecordReadWorkV1 as RevocationReadWorkV1,
    TypedRecordWriteWorkV1 as RevocationWriteWorkV1,
};
fn profile() -> TypedRecordProfileV1 {
    TypedRecordProfileV1 {
        field: field(),
        schema: schema(),
        labels: LABELS,
        reference_schema: Some(bytes::reference_schema),
    }
}
#[path = "incremental_runtime_revocation_dto.rs"]
mod dto;
pub fn prepare_runtime_revocation_body_v1<
    W: ValueNodeWriter + ValueNodeReader + StatePageWriter,
    C: FnMut() -> bool,
>(
    value: &crate::CapabilityRevocationState,
    ctx: &mut MemberRecordWriteContext<'_, W, C>,
    work: &mut RevocationWriteWorkV1,
) -> Result<oasis7_proto::memory_budget::ReservedValue<StateRootV1>, MemberRecordError> {
    crate::incremental_typed_record_codec::prepare_typed_record_body_v1(
        value,
        &profile(),
        ctx,
        work,
    )
}
pub fn recover_runtime_revocation_body_v1<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    root: &StateRootV1,
    ctx: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
    scan: &mut StateScanBudget,
    work: &mut RevocationReadWorkV1,
) -> Result<RevocationRecoveredV1<crate::CapabilityRevocationState>, ValueTreeError> {
    crate::incremental_typed_record_codec::recover_typed_record_body_v1(
        root,
        &profile(),
        ctx,
        scan,
        work,
    )
}
#[path = "incremental_runtime_revocation_action_index.rs"]
mod action_index;
#[path = "incremental_runtime_revocation_family.rs"]
mod family;
pub use family::*;
#[path = "incremental_runtime_revocation_overlay.rs"]
mod overlay;
pub use overlay::*;

#[cfg(test)]
#[path = "incremental_runtime_revocation_tests.rs"]
mod tests;

#[path = "incremental_runtime_revocation_bytes.rs"]
mod bytes;

#[path = "incremental_runtime_revocation_current_identity.rs"]
mod current_identity;
pub use current_identity::{
    CurrentRevocationAgentIdentityV1, read_current_revocation_agent_identity_v1,
    resolve_manufactured_identity_generation_v1,
};
#[path = "incremental_runtime_revocation_current_alternative.rs"]
mod current_alternative;
pub use current_alternative::{
    CurrentSafeAlternativeRecordV1, read_current_safe_alternative_for_action_v1,
    replace_current_safe_alternative_record_v1,
};

#[path = "incremental_runtime_revocation_participant_current.rs"]
mod participant_current;
pub use participant_current::{
    CurrentParticipantOwnerDataV1, read_current_participant_owner_data_v1,
};
