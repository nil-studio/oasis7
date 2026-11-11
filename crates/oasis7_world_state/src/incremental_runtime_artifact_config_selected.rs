//! Selected-body bootstrap from the actual shared transaction overlay.
use super::*;
use crate::{
    incremental_field_member::*,
    incremental_field_overlay::{WorldFieldOverlayContextV1, WorldFieldOverlayErrorV1},
    incremental_field_read::WorldFieldReadContextV1,
    incremental_inventory_value_read::InventoryValueReader,
};
use oasis7_formal_protocol::{
    incremental_state::{MerkleNodeReader, StateScanBudget},
    incremental_value::{ValueNodeReader, ValueReadContext, ValueTreeError},
};
use oasis7_proto::memory_budget::{MemoryClass, ReservedValue};
pub struct ArtifactConfigSelectedBodyRequestV1<'a> {
    pub field: ArtifactConfigFieldV1,
    pub key: ArtifactConfigRecordKeyV1<'a>,
    pub family: &'a ReservedValue<ArtifactConfigFamilyQualificationV1>,
    pub parent_roots: &'a oasis7_formal_protocol::incremental_execution::RootSetV1,
    /// A real typed-after token may avoid re-reading unchanged large columns.
    pub current: Option<&'a ReservedValue<ArtifactConfigBodyQualificationV1>>,
}
/// Reads all original outer siblings, then fully qualifies the selected body.
/// New or edited same-TX records are visible; no materialized registry is used.
pub fn open_artifact_config_body_overlay_v1<M, R, E, C>(
    request: ArtifactConfigSelectedBodyRequestV1<'_>,
    state: &M,
    ctx: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    scan: &mut StateScanBudget,
    work: &mut ArtifactConfigReadWorkV1,
) -> Result<Option<ReservedValue<ArtifactConfigBodyQualificationV1>>, WorldFieldOverlayErrorV1>
where
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
{
    ctx.transaction.contribute(|tx| {
        if ctx.values.class != MemoryClass::Ordinary
            || !tx.uses_budget(ctx.values.memory)
            || !request.family.uses_budget(ctx.values.memory)
            || request.family.allocation_class() != ctx.values.class
            || request.family.reserved_bytes()
                < std::mem::size_of::<ArtifactConfigFamilyQualificationV1>()
        {
            return Err(read::invalid().into());
        }
        request
            .family
            .validate_binding(&tx.parent_fence().parent_roots)?;
        let _scratch = ctx
            .values
            .memory
            .reserve(32 << 10, ctx.values.class)
            .map_err(ValueTreeError::Memory)?;
        if request.parent_roots.digest()? != tx.parent_fence().parent_roots {
            return Err(read::invalid().into());
        }
        let fields = if request.field.body_kind() == ArtifactConfigBodyKindV1::Module {
            family::RECORD_FIELDS
        } else {
            [request.field; 4]
        };
        let count = if request.field.body_kind() == ArtifactConfigBodyKindV1::Module {
            4
        } else {
            1
        };
        let mut child = None;
        let mut present = None;
        {
            let mut records = InventoryValueReader {
                values: &mut ctx.values,
                key_work: ctx.key_work,
                semantic_bytes: ctx.semantic_bytes,
                record_bytes: ctx.record_bytes,
                cancelled: &mut ctx.cancelled,
            };
            for field in fields.into_iter().take(count) {
                let meta = WorldFieldKeyV1::new(
                    field.descriptor(),
                    WorldFieldRecordKindV1::Metadata,
                    None,
                )
                .map_err(ValueTreeError::Invalid)?;
                let id = crate::incremental_field_overlay::lookup(
                    tx,
                    &meta,
                    &mut records,
                    &mut ctx.take_state_error,
                )?
                .ok_or_else(read::invalid)?;
                records.presence(&id, &meta, 0)?;
                let key = family::record_key(field, request.key, &mut records)?;
                let id = crate::incremental_field_overlay::lookup(
                    tx,
                    &key.0,
                    &mut records,
                    &mut ctx.take_state_error,
                )?;
                if present.is_some_and(|old| old != id.is_some()) {
                    return Err(read::invalid().into());
                }
                present = Some(id.is_some());
                if let Some(id) = id {
                    let root = family::outer(field, &key.0, &id, &mut records)?;
                    if child.as_ref().is_some_and(|old| old != &*root) {
                        return Err(read::invalid().into());
                    }
                    child = Some((*root).clone());
                }
            }
        }
        let Some(root) = child else {
            if request.field.global() || request.current.is_some() {
                return Err(read::invalid().into());
            }
            if (ctx.cancelled)() {
                return Err(ValueTreeError::Cancelled.into());
            }
            return Ok(None);
        };
        if let Some(current) = request.current {
            if !current.uses_budget(ctx.values.memory)
                || current.allocation_class() != ctx.values.class
                || current.reserved_bytes() < 512
            {
                return Err(read::invalid().into());
            }
            current.validate_binding(request.field.body_kind(), &root)?;
            let token = closure::mint(
                request.field.body_kind(),
                &root,
                ctx.values.memory,
                ctx.values.class,
            )?;
            if (ctx.cancelled)() {
                return Err(ValueTreeError::Cancelled.into());
            }
            return Ok(Some(token));
        }
        let parent = open_qualified_artifact_config_body_v1(
            request.field,
            request.key,
            request.parent_roots,
            request.family,
            &mut WorldFieldReadContextV1 {
                root: request.parent_roots.state_root(
                    oasis7_formal_protocol::incremental_state::StateNamespaceV1::Artifacts,
                ),
                state,
                values: ValueReadContext {
                    source: ctx.values.source,
                    memory: ctx.values.memory,
                    class: ctx.values.class,
                    io: ctx.values.io,
                },
                key_work: ctx.key_work,
                semantic_bytes: ctx.semantic_bytes,
                record_bytes: ctx.record_bytes,
                take_state_error: &mut ctx.take_state_error,
                cancelled: &mut ctx.cancelled,
            },
        )?;
        if let Some(parent) = parent.filter(|proof| proof.root() == &root) {
            if (ctx.cancelled)() {
                return Err(ValueTreeError::Cancelled.into());
            }
            return Ok(Some(parent));
        }
        let result = qualify_artifact_config_body_v1(
            request.field.body_kind(),
            &mut WorldFieldReadContextV1 {
                root: &root,
                state,
                values: ValueReadContext {
                    source: ctx.values.source,
                    memory: ctx.values.memory,
                    class: ctx.values.class,
                    io: ctx.values.io,
                },
                key_work: ctx.key_work,
                semantic_bytes: ctx.semantic_bytes,
                record_bytes: ctx.record_bytes,
                take_state_error: &mut ctx.take_state_error,
                cancelled: &mut ctx.cancelled,
            },
            scan,
            work,
        )?;
        if (ctx.cancelled)() {
            return Err(ValueTreeError::Cancelled.into());
        }
        Ok(Some(result))
    })
}

/// Empty registry implies no registry-derived subscriptions. Refuse all pending
/// Artifacts writes conservatively; changed state needs the full dispatcher.
pub fn require_empty_artifact_registry_overlay_v1<M, R, E, C>(
    family: &ReservedValue<ArtifactConfigFamilyQualificationV1>,
    ctx: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    remaining_entries: &mut usize,
) -> Result<(), WorldFieldOverlayErrorV1>
where
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
{
    use oasis7_formal_protocol::incremental_state::StateNamespaceV1;
    ctx.transaction.contribute(|tx| {
        if ctx.values.class != MemoryClass::Ordinary
            || !tx.uses_budget(ctx.values.memory)
            || !family.uses_budget(ctx.values.memory)
            || family.allocation_class() != MemoryClass::Ordinary
            || family.reserved_bytes() < 512
        {
            return Err(read::invalid().into());
        }
        family.validate_binding(&tx.parent_fence().parent_roots)?;
        tx.visit_pending_writes::<WorldFieldOverlayErrorV1>(
            StateNamespaceV1::Artifacts,
            remaining_entries,
            || {
                if (ctx.cancelled)() {
                    Err(ValueTreeError::Cancelled.into())
                } else {
                    Ok(())
                }
            },
            |_, _, _| {
                Err(ValueTreeError::Invalid(
                    "unsupported bootstrap action: pending artifact changes",
                )
                .into())
            },
        )?;
        let mut records = InventoryValueReader {
            values: &mut ctx.values,
            key_work: ctx.key_work,
            semantic_bytes: ctx.semantic_bytes,
            record_bytes: ctx.record_bytes,
            cancelled: &mut ctx.cancelled,
        };
        for field in family::RECORD_FIELDS {
            let key =
                WorldFieldKeyV1::new(field.descriptor(), WorldFieldRecordKindV1::Metadata, None)
                    .map_err(ValueTreeError::Invalid)?;
            let id = crate::incremental_field_overlay::lookup(
                tx,
                &key,
                &mut records,
                &mut ctx.take_state_error,
            )?
            .ok_or_else(read::invalid)?;
            records.presence(&id, &key, 0)?;
        }
        if family.registry_records() != 0 {
            return Err(ValueTreeError::Invalid(
                "unsupported bootstrap action: nonempty module registry",
            )
            .into());
        }
        if (records.cancelled)() {
            return Err(ValueTreeError::Cancelled.into());
        }
        Ok(())
    })
}
