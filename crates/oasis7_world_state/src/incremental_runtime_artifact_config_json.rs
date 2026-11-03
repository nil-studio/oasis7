//! Actual JsonValue columns use the shared neutral engine, never a DTO blob.
use super::*;
use crate::{
    incremental_field_member::*,
    incremental_field_overlay::{WorldFieldOverlayContextV1, WorldFieldOverlayErrorV1},
    incremental_field_read::WorldFieldReadContextV1,
    incremental_inventory_value_read::InventoryValueReader,
    incremental_json_records::{
        JsonChangeWorkV1, JsonPathSegmentV1, JsonReadWorkV1, JsonSubtreeEditV1,
        JsonSubtreeQualificationV1, JsonWriteWorkV1, advance_json_subtree_records_v1,
        prepare_json_subtree_change_records_v1, qualify_json_subtree_records_v1,
    },
    incremental_record_tree::{self as tree, RecordTreeWritePolicyV1},
};
use oasis7_formal_protocol::{
    incremental_execution::{ExecutionFence, PreparedGuard},
    incremental_state::{MerkleNodeReader, StatePageWriter},
    incremental_value::{ValueNodeReader, ValueTreeError},
};
use oasis7_proto::memory_budget::{MemoryClass, ReservedValue};

/// Owner-local JSON scope; the neutral graph token alone does not bind the
/// memory class used by this owner's ordinary or recovery operations.
#[derive(Debug)]
pub struct ArtifactConfigJsonQualificationV1 {
    inner: ReservedValue<JsonSubtreeQualificationV1>,
    class: MemoryClass,
}
impl ArtifactConfigJsonQualificationV1 {
    pub fn root(&self) -> &oasis7_formal_protocol::incremental_state::StateRootV1 {
        self.inner.root()
    }
    pub fn allocation_class(&self) -> MemoryClass {
        self.class
    }
}
/// Owner-class-checked selected DTO read. The retained result is data, not an
/// invocation permission or a new parent/RootSet qualification.
pub fn materialize_artifact_config_json_v1<M, R, E, C>(
    proof: &ReservedValue<ArtifactConfigJsonQualificationV1>,
    ctx: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
    work: &mut JsonReadWorkV1,
    max_heap_bytes: usize,
) -> Result<ReservedValue<serde_json::Value>, ValueTreeError>
where
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
{
    ctx.check()?;
    let (value, charge) = materialize_parts(proof, &mut ctx.records(), work, max_heap_bytes)?;
    ctx.check()?;
    Ok(charge.retain(value))
}
pub(super) fn materialize_parts<R: ValueNodeReader, C: FnMut() -> bool>(
    proof: &ReservedValue<ArtifactConfigJsonQualificationV1>,
    records: &mut InventoryValueReader<'_, '_, R, C>,
    work: &mut JsonReadWorkV1,
    max_heap_bytes: usize,
) -> Result<
    (
        serde_json::Value,
        oasis7_proto::memory_budget::MemoryReservation,
    ),
    ValueTreeError,
> {
    if !proof.uses_budget(records.values.memory)
        || proof.reserved_bytes() < 512
        || proof.class != records.values.class
    {
        return Err(read::invalid());
    }
    crate::incremental_json_records::materialize_records(
        genesis::json_profile(),
        &proof.inner,
        records,
        work,
        max_heap_bytes,
    )
}
fn retain_json(
    inner: ReservedValue<JsonSubtreeQualificationV1>,
    memory: &oasis7_proto::memory_budget::MemoryBudget,
    class: MemoryClass,
) -> Result<ReservedValue<ArtifactConfigJsonQualificationV1>, ValueTreeError> {
    Ok(memory
        .reserve(512, class)?
        .retain(ArtifactConfigJsonQualificationV1 { inner, class }))
}

/// Explicit full-target bootstrap, O(target JSON), for retaining a hot path
/// scope after a cold root-only open. No family/RootSet authority is minted.
pub fn open_artifact_config_json_v1<M, R, E, C>(
    body: &ReservedValue<ArtifactConfigBodyQualificationV1>,
    address: ArtifactConfigAddressV1<'_>,
    ctx: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
    work: &mut JsonReadWorkV1,
) -> Result<ReservedValue<ArtifactConfigJsonQualificationV1>, ValueTreeError>
where
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
{
    ctx.check()?;
    if !body.uses_budget(ctx.values.memory)
        || body.allocation_class() != ctx.values.class
        || body.reserved_bytes()
            < std::mem::size_of::<ArtifactConfigBodyQualificationV1>()
                + body.root().node_root.capacity()
    {
        return Err(read::invalid());
    }
    body.validate_binding(body.kind(), ctx.root)?;
    if schema::spec(body.kind(), address)
        .map_err(|_| read::invalid())?
        .codec
        != MemberPayloadCodecV1::RecordRoot
    {
        return Err(read::invalid());
    }
    let _scratch = ctx.values.memory.reserve(32 << 10, ctx.values.class)?;
    let key = read::key(address, &mut ctx.records())?;
    let id = ctx.lookup(&key.0)?.ok_or_else(read::invalid)?;
    let pointer = read::value(
        body.kind(),
        address,
        &id,
        &key,
        165,
        &mut ctx.records(),
        &mut std::io::sink(),
    )?;
    let ArtifactConfigValueV1::JsonRoot(root) = pointer else {
        return Err(read::invalid());
    };
    let result =
        qualify_json_subtree_records_v1(genesis::json_profile(), &root, &mut ctx.records(), work)?;
    let result = retain_json(result, ctx.values.memory, ctx.values.class)?;
    ctx.check()?;
    Ok(result)
}
pub struct ArtifactConfigJsonChangeV1<'a> {
    pub request: ArtifactConfigCellRequestV1<'a>,
    pub json: &'a ReservedValue<ArtifactConfigJsonQualificationV1>,
    pub path: &'a [JsonPathSegmentV1<'a>],
    pub edit: JsonSubtreeEditV1<'a>,
}
pub struct ArtifactConfigJsonChangeResultV1 {
    pub changed: bool,
    pub body: Option<ReservedValue<ArtifactConfigBodyQualificationV1>>,
    pub json: Option<ReservedValue<ArtifactConfigJsonQualificationV1>>,
}
/// Bounded member/append/truncate/explicit-replace operations inherit the
/// qualified parent graph. Any new subgraph is completely checked by the
/// neutral producer and independent changed-path verifier before publication.
pub fn change_qualified_artifact_config_json_v1<M, R, E, C, W, CW>(
    change: ArtifactConfigJsonChangeV1<'_>,
    context: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    write: &mut MemberRecordWriteContext<'_, W, CW>,
    work: &mut JsonWriteWorkV1,
    delta: &mut JsonChangeWorkV1,
    guard: &PreparedGuard,
    live: &ExecutionFence,
) -> Result<ArtifactConfigJsonChangeResultV1, WorldFieldOverlayErrorV1>
where
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    W: StatePageWriter + ValueNodeReader,
    CW: FnMut() -> bool,
{
    let cancel = &mut context.cancelled;
    context.transaction.contribute(|tx| {
        tx.validate_guard(guard, live)?;
        let request = change.request;
        if !tx.uses_budget(write.memory)
            || !change.json.uses_budget(write.memory)
            || change.json.allocation_class() != MemoryClass::Ordinary
            || !change.json.inner.uses_budget(write.memory)
            || change.json.reserved_bytes()
                < std::mem::size_of::<ArtifactConfigJsonQualificationV1>()
            || schema::spec(request.field.body_kind(), request.address)
                .map_err(|_| read::invalid())?
                .codec
                != MemberPayloadCodecV1::RecordRoot
        {
            return Err(read::invalid().into());
        }
        let _operation = write
            .memory
            .reserve(64 << 10, MemoryClass::Ordinary)
            .map_err(ValueTreeError::Memory)?;
        let resolved = {
            let mut combined = || cancel() || (write.cancelled)();
            let mut records = InventoryValueReader {
                values: &mut context.values,
                key_work: &mut *context.key_work,
                semantic_bytes: context.semantic_bytes,
                record_bytes: context.record_bytes,
                cancelled: &mut combined,
            };
            overlay::resolve(
                request,
                tx,
                &mut records,
                &mut context.take_state_error,
                &mut std::io::sink(),
            )?
        };
        let ArtifactConfigValueV1::JsonRoot(root) = &resolved.value else {
            return Err(read::invalid().into());
        };
        change
            .json
            .inner
            .validate_binding(genesis::json_profile(), root)?;
        let prepared = {
            let mut records = InventoryValueReader {
                values: &mut context.values,
                key_work: &mut *context.key_work,
                semantic_bytes: context.semantic_bytes,
                record_bytes: context.record_bytes,
                cancelled: cancel,
            };
            prepare_json_subtree_change_records_v1(
                genesis::json_profile(),
                &change.json.inner,
                change.path,
                change.edit,
                &mut records,
                write,
                work,
            )?
        };
        if cancel() || (write.cancelled)() {
            return Err(ValueTreeError::Cancelled.into());
        }
        if !prepared.changed() {
            return Ok(ArtifactConfigJsonChangeResultV1 {
                changed: false,
                body: None,
                json: None,
            });
        }
        let json = {
            let mut combined = || cancel() || (write.cancelled)();
            let mut records = InventoryValueReader {
                values: &mut context.values,
                key_work: &mut *context.key_work,
                semantic_bytes: context.semantic_bytes,
                record_bytes: context.record_bytes,
                cancelled: &mut combined,
            };
            advance_json_subtree_records_v1(
                genesis::json_profile(),
                &change.json.inner,
                prepared.after(),
                &mut records,
                delta,
                None,
            )?
        };
        let row = {
            let mut combined = MemberRecordWriteContext {
                sink: &mut *write.sink,
                memory: write.memory,
                io: &mut *write.io,
                key_work: &mut *write.key_work,
                limits: write.limits,
                cancelled: || cancel() || (write.cancelled)(),
            };
            genesis::root_row(
                genesis::body_descriptor(),
                &request.address.semantic(),
                genesis::json_profile().schema,
                json.root().clone(),
                &mut combined,
            )?
        };
        if row.field_key != resolved.cell_key.0 {
            return Err(read::invalid().into());
        }
        let child = tree::edit(
            resolved.child,
            &resolved.cell_key.0,
            (Some(resolved.cell_root), Some(row.value_root)),
            &mut context.values,
            write,
            cancel,
            RecordTreeWritePolicyV1 {
                max_pending_node_bytes: work.max_pending_node_bytes,
                labels: genesis::LABELS,
            },
        )?;
        let body = overlay::publish(
            overlay::Publication {
                field: request.field,
                key: request.key,
                outer: resolved.outer,
                child: Some(child),
            },
            tx,
            write,
            cancel,
        )?
        .ok_or_else(read::invalid)?;
        let json = retain_json(json, write.memory, MemoryClass::Ordinary)?;
        if cancel() || (write.cancelled)() {
            return Err(ValueTreeError::Cancelled.into());
        }
        Ok(ArtifactConfigJsonChangeResultV1 {
            changed: true,
            body: Some(body),
            json: Some(json),
        })
    })
}

/// Optional retained, privately qualified parent graphs for incremental JSON
/// receiving. Search and delta work are continuous explicit allowances. Missing
/// scopes fall back to complete new-graph qualification, not guessed closure.
pub struct ArtifactConfigJsonParentsV1<'a> {
    pub scopes: &'a [&'a ReservedValue<ArtifactConfigJsonQualificationV1>],
    pub probes_remaining: u64,
    pub delta: JsonChangeWorkV1,
}
impl ArtifactConfigJsonParentsV1<'_> {
    pub(super) fn advance<R: ValueNodeReader, C: FnMut() -> bool>(
        &mut self,
        before: &oasis7_formal_protocol::incremental_state::StateRootV1,
        after: &oasis7_formal_protocol::incremental_state::StateRootV1,
        records: &mut InventoryValueReader<'_, '_, R, C>,
    ) -> Result<bool, ValueTreeError> {
        for proof in self.scopes {
            self.probes_remaining =
                self.probes_remaining
                    .checked_sub(1)
                    .ok_or(ValueTreeError::ResourceLimited(
                        "artifact config JSON parent probes",
                    ))?;
            if proof.root() == before {
                if !proof.uses_budget(records.values.memory)
                    || proof.allocation_class() != records.values.class
                    || !proof.inner.uses_budget(records.values.memory)
                    || proof.reserved_bytes()
                        < std::mem::size_of::<ArtifactConfigJsonQualificationV1>()
                {
                    return Err(read::invalid());
                }
                advance_json_subtree_records_v1(
                    genesis::json_profile(),
                    &proof.inner,
                    after,
                    records,
                    &mut self.delta,
                    None,
                )?;
                return Ok(true);
            }
        }
        Ok(false)
    }
}
