//! Bounded borrowed serde_json nodes with owner-selected addressing.
//! A subtree receipt proves this JSON schema only. It never grants a World,
//! Runtime, Cognition, RootSet, dispatch or business-authority capability.
use crate::{
    incremental_field_member::*,
    incremental_field_presence::{
        PresenceRecordLimitsV1, PresenceRecordV1, PresenceRecordWriteContext,
        write_field_metadata_v1,
    },
    incremental_record_tree::{self as tree, RecordTreeLabelsV1, RecordTreeWritePolicyV1},
    incremental_semantic_key::SemanticKeyV1,
};
use oasis7_formal_protocol::{
    incremental_record_root::RecordRootRefV1,
    incremental_state::{StateNamespaceV1, StatePageWriter, StateRootV1},
    incremental_value::{ValueNodeReader, ValueNodeWriter, ValueTreeError},
};
use oasis7_proto::memory_budget::MemoryClass;
/// Owners must bind their own constant profile in their full family gate.
/// Caller-selected descriptors or schema bytes do not grant owner qualification.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct JsonRecordProfileV1 {
    pub field: FieldDescriptor,
    pub schema: [u8; 32],
}
impl JsonRecordProfileV1 {
    fn namespace(self) -> Result<StateNamespaceV1, ValueTreeError> {
        match self.field.destination {
            FieldDestination::State(namespace) if self.field.shape == FieldShape::Map => {
                Ok(namespace)
            }
            _ => Err(ValueTreeError::Invalid("JSON node profile")),
        }
    }
}
pub struct JsonWriteWorkV1 {
    pub cells_remaining: u64,
    pub json_nodes_remaining: u64,
    /// Root depth is zero; this bounds edges beneath it.
    pub max_json_depth: usize,
    pub max_pending_node_bytes: usize,
}
impl JsonWriteWorkV1 {
    fn cell(&mut self) -> Result<(), MemberRecordError> {
        self.cells_remaining = self
            .cells_remaining
            .checked_sub(1)
            .ok_or(ValueTreeError::ResourceLimited("JSON cells"))?;
        Ok(())
    }
    fn json_node(&mut self) -> Result<(), MemberRecordError> {
        self.json_nodes_remaining = self
            .json_nodes_remaining
            .checked_sub(1)
            .ok_or(ValueTreeError::ResourceLimited("JSON nodes"))?;
        Ok(())
    }
}
const LABELS: RecordTreeLabelsV1 = RecordTreeLabelsV1 {
    read: "JSON node read",
    pending: "JSON node pending",
    tree: "JSON node tree",
    digest: "JSON node digest",
    read_allowance: "JSON node read allowance",
    admission: "JSON node admission",
    bound: "JSON node bound",
    producer_pool: "JSON node pool",
    page: "JSON node page",
    write_allowance: "JSON node write allowance",
};
fn metadata<W: ValueNodeWriter, C: FnMut() -> bool>(
    profile: JsonRecordProfileV1,
    ctx: &mut MemberRecordWriteContext<'_, W, C>,
) -> Result<PresenceRecordV1, MemberRecordError> {
    write_field_metadata_v1(
        profile.field,
        &mut PresenceRecordWriteContext {
            sink: &mut *ctx.sink,
            memory: ctx.memory,
            io: &mut *ctx.io,
            key_work: &mut *ctx.key_work,
            limits: PresenceRecordLimitsV1 {
                semantic_bytes: ctx.limits.semantic_bytes,
                record_bytes: ctx.limits.record_bytes,
            },
            cancelled: &mut ctx.cancelled,
        },
    )
}
fn row<K: SemanticKeyV1 + ?Sized, W: ValueNodeWriter, C: FnMut() -> bool>(
    profile: JsonRecordProfileV1,
    key: &K,
    payload: MemberPayloadRefV1<'_>,
    ctx: &mut MemberRecordWriteContext<'_, W, C>,
) -> Result<MemberRecordV1, MemberRecordError> {
    write_member_record_v1(
        profile.field,
        WorldFieldRecordKindV1::Entry,
        key,
        payload,
        ctx,
    )
}
fn root_row<K: SemanticKeyV1 + ?Sized, W: ValueNodeWriter, C: FnMut() -> bool>(
    profile: JsonRecordProfileV1,
    key: &K,
    child: StateRootV1,
    ctx: &mut MemberRecordWriteContext<'_, W, C>,
) -> Result<MemberRecordV1, MemberRecordError> {
    let _scratch = ctx
        .memory
        .reserve(8192, MemoryClass::Ordinary)
        .map_err(ValueTreeError::Memory)?;
    let semantic = crate::incremental_semantic_key::hash_semantic_key_v1(
        key,
        ctx.memory,
        MemoryClass::Ordinary,
        ctx.key_work,
        &mut ctx.cancelled,
    )
    .map_err(MemberRecordError::Key)?;
    if child.namespace != profile.namespace()? {
        return Err(MemberRecordError::Invalid("JSON child namespace"));
    }
    let reference = RecordRootRefV1::new(profile.schema, semantic.digest, child)
        .map_err(|_| MemberRecordError::Invalid("JSON root binding"))?;
    row(
        profile,
        key,
        MemberPayloadRefV1::RecordRoot(&reference),
        ctx,
    )
}
fn append<W: StatePageWriter + ValueNodeReader, C: FnMut() -> bool>(
    root: StateRootV1,
    row: &MemberRecordV1,
    ctx: &mut MemberRecordWriteContext<'_, W, C>,
    work: &JsonWriteWorkV1,
) -> Result<StateRootV1, MemberRecordError> {
    Ok(tree::append(
        root,
        &row.field_key,
        row.value_root,
        ctx,
        RecordTreeWritePolicyV1 {
            max_pending_node_bytes: work.max_pending_node_bytes,
            labels: LABELS,
        },
    )?)
}
#[path = "incremental_json_records_genesis.rs"]
mod genesis;
pub use genesis::prepare_json_subtree_v1;
#[path = "incremental_json_records_read.rs"]
mod read;
pub use read::{JsonReadWorkV1, JsonSubtreeQualificationV1, qualify_json_subtree_v1};
#[path = "incremental_json_records_cursor.rs"]
pub(crate) mod cursor;

#[path = "incremental_json_records_point.rs"]
mod point;
pub use point::{
    JsonPathSegmentV1, JsonScalarPointWriteV1, JsonScalarRefV1, prepare_json_scalar_path_change_v1,
};

#[path = "incremental_json_records_change.rs"]
mod change;
pub use change::{JsonScalarChangeWorkV1, advance_json_scalar_subtree_v1};
