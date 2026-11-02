//! Original ordered pending and String-keyed inflight effect storage.
//! GEN outputs are provisional; this module does not confer dispatch authority.
use crate::incremental_field_presence::{
    PresenceRecordLimitsV1, PresenceRecordV1, PresenceRecordWriteContext, write_field_metadata_v1,
};
use crate::{
    EffectIntent, EffectOrigin,
    incremental_field_member::*,
    incremental_record_tree::{self as tree, RecordTreeLabelsV1, RecordTreeWritePolicyV1},
    incremental_semantic_key::SemanticKeyV1,
};
use oasis7_formal_protocol::{
    incremental_record_root::RecordRootRefV1,
    incremental_state::{StateNamespaceV1, StatePageWriter, StateRootV1},
    incremental_value::{ValueNodeReader, ValueNodeWriter, ValueTreeError},
};
use oasis7_proto::memory_budget::MemoryClass;
use std::collections::{BTreeMap, VecDeque};
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuntimeEffectKindV1 {
    Pending,
    Inflight,
}
impl RuntimeEffectKindV1 {
    pub const ALL: [Self; 2] = [Self::Pending, Self::Inflight];
    pub fn name(self) -> &'static str {
        match self {
            Self::Pending => "pending_effects",
            Self::Inflight => "inflight_effects",
        }
    }
    pub(super) fn descriptor(self) -> FieldDescriptor {
        FieldDescriptor {
            owner: FieldOwner::Runtime,
            name: self.name(),
            shape: match self {
                Self::Pending => FieldShape::Ordered,
                Self::Inflight => FieldShape::Map,
            },
            destination: FieldDestination::State(StateNamespaceV1::Effects),
        }
    }
    pub fn schema(self) -> [u8; 32] {
        match self {
            Self::Pending => pending_schema(),
            Self::Inflight => shared_schema(false),
        }
    }
    pub(super) fn json_schema(self) -> [u8; 32] {
        shared_schema(true)
    }
}
#[derive(Clone, Copy)]
pub(super) enum Space {
    Outer(RuntimeEffectKindV1),
    Body,
    PendingItem,
    Json,
}
impl Space {
    pub(super) fn descriptor(self) -> FieldDescriptor {
        match self {
            Self::Outer(kind) => kind.descriptor(),
            Self::Body | Self::PendingItem | Self::Json => FieldDescriptor {
                owner: FieldOwner::Runtime,
                name: match self {
                    Self::Body => "effect_intent_body_v1",
                    Self::PendingItem => "effect_pending_item_v1",
                    _ => "effect_json_node_v1",
                },
                shape: FieldShape::Map,
                destination: FieldDestination::State(StateNamespaceV1::Effects),
            },
        }
    }
}
pub(super) fn pending_schema() -> [u8; 32] {
    *blake3::hash(b"oasis7.iws.runtime-effect-pending-item.v1\0").as_bytes()
}
pub(super) fn shared_schema(json: bool) -> [u8; 32] {
    *blake3::hash(if json {
        b"oasis7.iws.runtime-effect-json-node.v1\0"
    } else {
        b"oasis7.iws.runtime-effect-intent-body.v1\0"
    })
    .as_bytes()
}

fn json_profile() -> crate::incremental_json_records::JsonRecordProfileV1 {
    crate::incremental_json_records::JsonRecordProfileV1 {
        field: Space::Json.descriptor(),
        schema: shared_schema(true),
    }
}
pub struct RuntimeEffectRecordsRefV1<'a> {
    pub pending: &'a VecDeque<EffectIntent>,
    pub inflight: &'a BTreeMap<String, EffectIntent>,
}
pub struct RuntimeEffectRecordsWorkV1 {
    pub cells_remaining: u64,
    pub json_nodes_remaining: u64,
    /// Maximum edges below the JSON root; the root itself has depth zero.
    pub max_json_depth: usize,
    pub max_pending_node_bytes: usize,
}
impl RuntimeEffectRecordsWorkV1 {
    pub(super) fn cell(&mut self) -> Result<(), MemberRecordError> {
        self.cells_remaining = self
            .cells_remaining
            .checked_sub(1)
            .ok_or(ValueTreeError::ResourceLimited("effect record cells"))?;
        Ok(())
    }
}
pub(super) const FORMAT: &str = "runtime_effect_intent_closure_v1";
pub(super) const LABELS: RecordTreeLabelsV1 = RecordTreeLabelsV1 {
    read: "effect record read",
    pending: "effect record pending",
    tree: "effect record tree",
    digest: "effect record digest",
    read_allowance: "effect record read allowance",
    admission: "effect record admission",
    bound: "effect record bound",
    producer_pool: "effect record pool",
    page: "effect record page",
    write_allowance: "effect record write allowance",
};
pub(super) fn metadata<W: ValueNodeWriter, C: FnMut() -> bool>(
    space: Space,
    ctx: &mut MemberRecordWriteContext<'_, W, C>,
) -> Result<PresenceRecordV1, MemberRecordError> {
    write_field_metadata_v1(
        space.descriptor(),
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
pub(super) fn row<K: SemanticKeyV1 + ?Sized, W: ValueNodeWriter, C: FnMut() -> bool>(
    space: Space,
    key: &K,
    payload: MemberPayloadRefV1<'_>,
    ctx: &mut MemberRecordWriteContext<'_, W, C>,
) -> Result<MemberRecordV1, MemberRecordError> {
    write_member_record_v1(
        space.descriptor(),
        WorldFieldRecordKindV1::Entry,
        key,
        payload,
        ctx,
    )
}
pub(super) fn root_row<K: SemanticKeyV1 + ?Sized, W: ValueNodeWriter, C: FnMut() -> bool>(
    space: Space,
    key: &K,
    child: StateRootV1,
    json: bool,
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
    let reference = RecordRootRefV1::new(
        if matches!(space, Space::Outer(RuntimeEffectKindV1::Pending)) {
            pending_schema()
        } else {
            shared_schema(json)
        },
        semantic.digest,
        child,
    )
    .map_err(|_| MemberRecordError::Invalid("effect root namespace/profile"))?;
    row(space, key, MemberPayloadRefV1::RecordRoot(&reference), ctx)
}
pub(super) fn append<W: StatePageWriter + ValueNodeReader, C: FnMut() -> bool>(
    root: StateRootV1,
    row: &MemberRecordV1,
    ctx: &mut MemberRecordWriteContext<'_, W, C>,
    work: &RuntimeEffectRecordsWorkV1,
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
#[path = "incremental_runtime_effect_records_json_genesis.rs"]
mod json_genesis;
fn body<W: StatePageWriter + ValueNodeReader, C: FnMut() -> bool>(
    source: &EffectIntent,
    ctx: &mut MemberRecordWriteContext<'_, W, C>,
    work: &mut RuntimeEffectRecordsWorkV1,
) -> Result<StateRootV1, MemberRecordError> {
    let _scratch = ctx
        .memory
        .reserve(16 << 10, MemoryClass::Ordinary)
        .map_err(ValueTreeError::Memory)?;
    let EffectIntent {
        birth_era,
        intent_id,
        kind: intent_kind,
        params,
        cap_ref,
        origin,
    } = source;
    let json = json_genesis::body(params, ctx, work)?;
    work.cell()?;
    let meta = metadata(Space::Body, ctx)?;
    let mut root = StateRootV1::empty(StateNamespaceV1::Effects)
        .map_err(|_| MemberRecordError::Invalid("effect root namespace/profile"))?;
    root = tree::append(
        root,
        &meta.field_key,
        meta.value_root,
        ctx,
        RecordTreeWritePolicyV1 {
            max_pending_node_bytes: work.max_pending_node_bytes,
            labels: LABELS,
        },
    )?;
    work.cell()?;
    let marker = row(Space::Body, &(), MemberPayloadRefV1::Unit, ctx)?;
    root = append(root, &marker, ctx, work)?;
    let (origin_kind, name, module) = match origin {
        EffectOrigin::Reducer { name } => ("Reducer", Some(name.as_str()), None),
        EffectOrigin::Plan { name } => ("Plan", Some(name.as_str()), None),
        EffectOrigin::Module { module_id } => ("Module", None, Some(module_id.as_str())),
        EffectOrigin::System => ("System", None, None),
    };
    for (name, payload) in [
        (
            "birth_era",
            birth_era.map_or(MemberPayloadRefV1::Unit, MemberPayloadRefV1::U64),
        ),
        ("intent_id", MemberPayloadRefV1::Text(intent_id)),
        ("kind", MemberPayloadRefV1::Text(intent_kind)),
        ("cap_ref", MemberPayloadRefV1::Text(cap_ref)),
        ("origin.kind", MemberPayloadRefV1::Text(origin_kind)),
        (
            "origin.name",
            name.map_or(MemberPayloadRefV1::Unit, MemberPayloadRefV1::Text),
        ),
        (
            "origin.module_id",
            module.map_or(MemberPayloadRefV1::Unit, MemberPayloadRefV1::Text),
        ),
    ] {
        work.cell()?;
        let cell = row(Space::Body, &((), name), payload, ctx)?;
        root = append(root, &cell, ctx, work)?;
    }
    work.cell()?;
    let params = root_row(Space::Body, &((), "params"), json, true, ctx)?;
    root = append(root, &params, ctx, work)?;
    if root.leaf_count != 10 {
        return Err(MemberRecordError::Invalid(
            "effect record exhaustive columns",
        ));
    }
    if (ctx.cancelled)() {
        return Err(ValueTreeError::Cancelled.into());
    }
    Ok(root)
}
fn pending_item<W: StatePageWriter + ValueNodeReader, C: FnMut() -> bool>(
    body: StateRootV1,
    prev: Option<u64>,
    next: Option<u64>,
    ctx: &mut MemberRecordWriteContext<'_, W, C>,
    work: &mut RuntimeEffectRecordsWorkV1,
) -> Result<StateRootV1, MemberRecordError> {
    work.cell()?;
    let meta = metadata(Space::PendingItem, ctx)?;
    let mut root = StateRootV1::empty(StateNamespaceV1::Effects)
        .map_err(|_| MemberRecordError::Invalid("effect pending namespace"))?;
    root = tree::append(
        root,
        &meta.field_key,
        meta.value_root,
        ctx,
        RecordTreeWritePolicyV1 {
            max_pending_node_bytes: work.max_pending_node_bytes,
            labels: LABELS,
        },
    )?;
    for (name, payload) in [
        (None, MemberPayloadRefV1::Unit),
        (
            Some("prev"),
            prev.map_or(MemberPayloadRefV1::Unit, MemberPayloadRefV1::U64),
        ),
        (
            Some("next"),
            next.map_or(MemberPayloadRefV1::Unit, MemberPayloadRefV1::U64),
        ),
    ] {
        work.cell()?;
        let cell = match name {
            None => row(Space::PendingItem, &(), payload, ctx)?,
            Some(name) => row(Space::PendingItem, &((), name), payload, ctx)?,
        };
        root = append(root, &cell, ctx, work)?;
    }
    work.cell()?;
    let reference = root_row(Space::PendingItem, &((), "body"), body, false, ctx)?;
    root = append(root, &reference, ctx, work)?;
    if (ctx.cancelled)() {
        return Err(ValueTreeError::Cancelled.into());
    }
    Ok(root)
}
/// Both real collections share the same exhaustive borrowed Intent/JSON producer.
/// No raw callback or provisional output grants outbox/World readiness.
pub fn prepare_runtime_effect_records_genesis_v1<
    W: StatePageWriter + ValueNodeReader,
    C: FnMut() -> bool,
    S: FnMut(
        oasis7_proto::memory_budget::ReservedValue<
            oasis7_formal_protocol::incremental_execution::ExecutionWriteV1,
        >,
    ) -> Result<(), ValueTreeError>,
>(
    source: &RuntimeEffectRecordsRefV1<'_>,
    ctx: &mut MemberRecordWriteContext<'_, W, C>,
    work: &mut RuntimeEffectRecordsWorkV1,
    sink: &mut S,
) -> Result<u64, MemberRecordError> {
    if (ctx.cancelled)() {
        return Err(ValueTreeError::Cancelled.into());
    }
    let _scratch = ctx
        .memory
        .reserve(16 << 10, MemoryClass::Ordinary)
        .map_err(ValueTreeError::Memory)?;
    for kind in RuntimeEffectKindV1::ALL {
        work.cell()?;
        let meta = metadata(Space::Outer(kind), ctx)?;
        crate::incremental_field_write::emit(meta.field_key, meta.value_root, ctx, sink)?;
        work.cell()?;
        let marker = row(
            Space::Outer(kind),
            &(FORMAT, ()),
            MemberPayloadRefV1::Unit,
            ctx,
        )?;
        crate::incremental_field_write::emit(marker.field_key, marker.value_root, ctx, sink)?;
    }
    let len = u64::try_from(source.pending.len())
        .map_err(|_| MemberRecordError::Invalid("effect count"))?;
    for (name, payload) in [
        (
            "head",
            if len == 0 {
                MemberPayloadRefV1::Unit
            } else {
                MemberPayloadRefV1::U64(0)
            },
        ),
        (
            "tail",
            len.checked_sub(1)
                .map_or(MemberPayloadRefV1::Unit, MemberPayloadRefV1::U64),
        ),
        ("count", MemberPayloadRefV1::U64(len)),
        ("next_ordinal", MemberPayloadRefV1::U64(len)),
    ] {
        work.cell()?;
        let value = row(
            Space::Outer(RuntimeEffectKindV1::Pending),
            &((FORMAT, ()), name),
            payload,
            ctx,
        )?;
        crate::incremental_field_write::emit(value.field_key, value.value_root, ctx, sink)?;
    }
    let mut count = 0u64;
    for (ordinal, intent) in source.pending.iter().enumerate() {
        let child = body(intent, ctx, work)?;
        let child = pending_item(
            child,
            (ordinal as u64).checked_sub(1),
            (ordinal as u64).checked_add(1).filter(|v| *v < len),
            ctx,
            work,
        )?;
        work.cell()?;
        let row = root_row(
            Space::Outer(RuntimeEffectKindV1::Pending),
            &(ordinal as u64),
            child,
            false,
            ctx,
        )?;
        crate::incremental_field_write::emit(row.field_key, row.value_root, ctx, sink)?;
        count = count
            .checked_add(1)
            .ok_or(MemberRecordError::Invalid("effect count"))?;
    }
    for (key, intent) in source.inflight {
        let child = body(intent, ctx, work)?;
        work.cell()?;
        let row = root_row(
            Space::Outer(RuntimeEffectKindV1::Inflight),
            key,
            child,
            false,
            ctx,
        )?;
        crate::incremental_field_write::emit(row.field_key, row.value_root, ctx, sink)?;
        count = count
            .checked_add(1)
            .ok_or(MemberRecordError::Invalid("effect count"))?;
    }
    if (ctx.cancelled)() {
        return Err(ValueTreeError::Cancelled.into());
    }
    Ok(count)
}

#[path = "incremental_runtime_effect_records_json_read.rs"]
mod json_read;
pub use json_read::{
    RuntimeEffectJsonQualificationV1, RuntimeEffectJsonWorkV1, inspect_runtime_effect_json_body_v1,
};

#[path = "incremental_runtime_effect_records_family.rs"]
mod family;
pub use family::{
    RuntimeEffectFamilyQualificationV1, RuntimeEffectQueueFrontierV1,
    qualify_runtime_effect_records_v1,
};

#[path = "incremental_runtime_effect_records_overlay.rs"]
mod overlay;
pub use overlay::{RuntimeEffectMembershipChangeV1, change_runtime_effect_membership_v1};

#[path = "incremental_runtime_effect_records_receiver.rs"]
mod receiver;
pub use receiver::accept_runtime_effect_transition_v1;

#[path = "incremental_runtime_effect_records_read.rs"]
mod read;
pub use read::{
    RuntimeEffectFieldV1, RuntimeEffectFieldValueV1, RuntimeEffectRecordKeyV1,
    RuntimeEffectRecordQualificationV1, open_runtime_effect_record_v1,
    read_qualified_runtime_effect_field_v1, read_qualified_runtime_effect_params_root_v1,
};

#[path = "incremental_runtime_effect_records_pages.rs"]
mod pages;
pub use pages::{
    RuntimeEffectFamilyPagesV1, RuntimeEffectPagesWorkV1, mark_runtime_effect_family_pages_v1,
    transfer_runtime_effect_family_pages_v1,
};
