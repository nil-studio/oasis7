//! Provisional identities decoded from the actual `pending_recipe_jobs` root
//! and the Current Scheduler write set.
//!
//! Parent and pending emissions are provisional until their respective visit
//! returns `Ok`; callers discard all emitted values on an error. These APIs
//! bind the parent industry-family proof and do not mint a qualification.
use super::key_read::OuterKeyDecoder;
use super::*;
use crate::{
    incremental_field_overlay::WorldFieldOverlayErrorV1,
    incremental_inventory_value_read::InventoryValueReader,
};
use oasis7_formal_protocol::{
    incremental_execution::{ExecutionTransaction, RootSetV1},
    incremental_record_root::{RECORD_ROOT_BYTES, RecordRootRefV1},
    incremental_state::{
        MerkleNodeReader, StateNamespaceV1, StateRootV1, StateScanContext, StateScanError,
    },
    incremental_value::{ValueNodeReader, ValueTreeError},
};
use oasis7_proto::memory_budget::{MemoryClass, ReservedValue};
use std::{cell::RefCell, io};

/// One outer value and its typed record root.
#[derive(Debug, PartialEq, Eq)]
pub struct IndustryOuterRecordRootV1 {
    pub value_root: [u8; 32],
    pub record_root: StateRootV1,
}

/// An entry from the actual, fully scanned parent `pending_recipe_jobs` root.
#[derive(Debug, PartialEq, Eq)]
pub enum IndustryQualifiedOuterIdentityV1 {
    FormatMarker {
        value_root: [u8; 32],
    },
    Record {
        id: u64,
        value_root: [u8; 32],
        record_root: StateRootV1,
    },
}

/// A changed recipe-job outer record or the family's format marker.
#[derive(Debug, PartialEq, Eq)]
pub enum IndustryCurrentOuterChangeV1 {
    FormatMarker {
        before_value_root: [u8; 32],
        after_value_root: [u8; 32],
    },
    Record {
        id: u64,
        before: Option<IndustryOuterRecordRootV1>,
        after: Option<IndustryOuterRecordRootV1>,
    },
}

fn invalid() -> ValueTreeError {
    ValueTreeError::Invalid("industry Current recipe outer identity")
}

fn marker_key<R: ValueNodeReader, C: FnMut() -> bool>(
    records: &mut InventoryValueReader<'_, '_, R, C>,
) -> Result<(WorldFieldKeyV1, u64, WorldFieldKeyV1), ValueTreeError> {
    let field = IndustryRecordKindV1::Recipe.descriptor()?;
    let (marker, marker_bytes) = records.key(
        field,
        WorldFieldRecordKindV1::Entry,
        &(RECORD_FORMAT_KEY, ()),
    )?;
    let metadata = WorldFieldKeyV1::new(field, WorldFieldRecordKindV1::Metadata, None)
        .map_err(ValueTreeError::Invalid)?;
    Ok((marker, marker_bytes, metadata))
}

fn decode_recipe_root<R: ValueNodeReader, C: FnMut() -> bool>(
    value_root: &[u8; 32],
    key: &WorldFieldKeyV1,
    records: &mut InventoryValueReader<'_, '_, R, C>,
) -> Result<(u64, StateRootV1), ValueTreeError> {
    records
        .values
        .verify_closure(value_root, records.record_bytes, &mut *records.cancelled)?;
    let _scratch = records
        .values
        .memory
        .reserve(8192, records.values.class)
        .map_err(ValueTreeError::Memory)?;
    let mut payload = [0; RECORD_ROOT_BYTES];
    let mut decoder = OuterKeyDecoder::new(IndustryRecordKindV1::Recipe, &mut io::sink());
    let result = read_member_record_v1(
        value_root,
        key,
        records.values,
        MemberRecordLimitsV1 {
            semantic_bytes: records.semantic_bytes,
            payload_bytes: RECORD_ROOT_BYTES as u64,
            record_bytes: records.record_bytes,
        },
        &mut decoder,
        &mut &mut payload[..],
        &mut *records.cancelled,
    );
    if let Some(reason) = decoder.bad {
        return Err(ValueTreeError::Invalid(reason));
    }
    let info = result?;
    if info.codec != MemberPayloadCodecV1::RecordRoot
        || info.payload_bytes != RECORD_ROOT_BYTES as u64
    {
        return Err(invalid());
    }
    let id = decoder.finish()?;
    let reference = RecordRootRefV1::from_canonical_bytes(&payload).map_err(|_| invalid())?;
    let binding = WorldFieldKeyV1::from_bytes(key.as_bytes()).map_err(ValueTreeError::Invalid)?;
    let record_root = reference
        .validate_binding(
            &IndustryRecordKindV1::Recipe.schema(),
            &binding.semantic_key.ok_or_else(invalid)?,
            StateNamespaceV1::Scheduler,
        )
        .map_err(|_| invalid())?
        .clone();
    if record_root.node_root.capacity() > 64 {
        return Err(ValueTreeError::ResourceLimited(
            "industry child root capacity",
        ));
    }
    Ok((id, record_root))
}

fn verify_recipe_marker<R: ValueNodeReader, C: FnMut() -> bool>(
    value_root: &[u8; 32],
    key: &WorldFieldKeyV1,
    semantic_bytes: u64,
    records: &mut InventoryValueReader<'_, '_, R, C>,
) -> Result<(), ValueTreeError> {
    records
        .values
        .verify_closure(value_root, records.record_bytes, &mut *records.cancelled)?;
    if read::value(
        IndustryCellV1::Record,
        value_root,
        &(key.clone(), semantic_bytes),
        records,
        0,
        &mut io::sink(),
    )? == IndustryValueV1::Unit
    {
        Ok(())
    } else {
        Err(invalid())
    }
}

/// Visit the complete `pending_recipe_jobs` parent directory from its actual
/// Scheduler root. The family qualification binds the full industry RootSet;
/// this scan proves membership and completeness for the selected root itself.
pub fn visit_qualified_recipe_parent_identities_v1<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    roots: &RootSetV1,
    qualification: &ReservedValue<IndustryFamilyQualificationV1>,
    context: &mut IndustryFamilyReadContextV1<'_, M, R, E, C>,
    work: &mut IndustryWorkBudgetV1,
    mut visit: impl FnMut(ReservedValue<IndustryQualifiedOuterIdentityV1>) -> Result<(), ValueTreeError>,
) -> Result<(), ValueTreeError> {
    if (context.cancelled)() {
        return Err(ValueTreeError::Cancelled);
    }
    let memory = context.values.memory;
    let class = context.values.class;
    if qualification.class != class
        || !qualification.uses_budget(memory)
        || qualification.reserved_bytes() < std::mem::size_of::<IndustryFamilyQualificationV1>()
        || context.state.allocation_class() != Some(class)
        || context
            .state
            .allocation_budget()
            .is_none_or(|pool| !pool.same_pool(memory))
    {
        return Err(ValueTreeError::Invalid(
            "industry parent identity pool/class",
        ));
    }
    let _workspace = memory
        .reserve(24 << 10, class)
        .map_err(ValueTreeError::Memory)?;
    roots.validate().map_err(|_| invalid())?;
    qualification.validate_binding(&roots.digest().map_err(|_| invalid())?)?;
    let root = roots.state_root(StateNamespaceV1::Scheduler);
    if root.namespace != StateNamespaceV1::Scheduler {
        return Err(invalid());
    }
    let kind = IndustryRecordKindV1::Recipe;
    let field = kind.descriptor()?;
    let cancellation = RefCell::new(&mut context.cancelled);
    let mut records_cancelled = || (cancellation.borrow_mut())();
    let mut records = InventoryValueReader {
        values: &mut context.values,
        key_work: &mut *context.key_work,
        semantic_bytes: context.semantic_bytes,
        record_bytes: context.record_bytes,
        cancelled: &mut records_cancelled,
    };
    let (marker_key, marker_bytes, metadata_key) = marker_key(&mut records)?;
    let mut metadata = false;
    let mut marker = false;
    let mut record_count = 0u64;
    let result = StateScanContext {
        source: context.state,
        memory,
        class,
        io: &mut *context.outer_scan,
    }
    .visit_entries(
        root,
        |entry| {
            work.cell()?;
            if !entry.key.starts_with(b"oasis7.iws.world-field-key.v1\0") {
                return Ok(());
            }
            let binding =
                WorldFieldKeyV1::from_bytes(&entry.key).map_err(ValueTreeError::Invalid)?;
            if binding.name != "pending_recipe_jobs" {
                return Ok(());
            }
            let key = WorldFieldKeyV1::new(field, binding.kind, binding.semantic_key)
                .map_err(ValueTreeError::Invalid)?;
            if entry.namespace != kind.namespace() || key.as_bytes() != entry.key {
                return Err(invalid());
            }
            if binding.kind == WorldFieldRecordKindV1::Metadata {
                if metadata || key != metadata_key {
                    return Err(invalid());
                }
                records.presence(&entry.value, &key, 0)?;
                metadata = true;
                return Ok(());
            }
            if binding.kind != WorldFieldRecordKindV1::Entry {
                return Err(invalid());
            }
            if key == marker_key {
                if marker {
                    return Err(invalid());
                }
                verify_recipe_marker(&entry.value, &key, marker_bytes, &mut records)?;
                marker = true;
                let charge = memory
                    .reserve(
                        std::mem::size_of::<IndustryQualifiedOuterIdentityV1>(),
                        class,
                    )
                    .map_err(ValueTreeError::Memory)?;
                return visit(
                    charge.retain(IndustryQualifiedOuterIdentityV1::FormatMarker {
                        value_root: entry.value,
                    }),
                );
            }
            let (id, record_root) = decode_recipe_root(&entry.value, &key, &mut records)?;
            record_count = record_count.checked_add(1).ok_or_else(invalid)?;
            let retained = std::mem::size_of::<IndustryQualifiedOuterIdentityV1>()
                .checked_add(std::mem::size_of::<StateRootV1>() + 64)
                .ok_or_else(invalid)?;
            let charge = memory
                .reserve(retained, class)
                .map_err(ValueTreeError::Memory)?;
            visit(charge.retain(IndustryQualifiedOuterIdentityV1::Record {
                id,
                value_root: entry.value,
                record_root,
            }))
        },
        || (cancellation.borrow_mut())(),
    );
    if let Some(error) = (context.take_state_error)() {
        return Err(error);
    }
    match result {
        Ok(_) => {}
        Err(StateScanError::Memory(error)) => return Err(ValueTreeError::Memory(error)),
        Err(StateScanError::ResourceLimited) => {
            return Err(ValueTreeError::ResourceLimited(
                "recipe parent identity scan",
            ));
        }
        Err(StateScanError::Cancelled) => return Err(ValueTreeError::Cancelled),
        Err(StateScanError::Visitor(error)) => return Err(error),
        Err(StateScanError::Invalid(_) | StateScanError::Tree(_)) => return Err(invalid()),
    }
    if !metadata
        || !marker
        || qualification.record_counts()[kind.index()] != record_count
        || (cancellation.borrow_mut())()
    {
        return Err(invalid());
    }
    Ok(())
}

/// Visit changed recipe-job outer entries from the transaction's actual final
/// pending Scheduler writes. Parent-side values are bound by the qualified
/// family. This resolves only the outer semantic ID and typed child-root
/// binding; the downstream complete family transition performs child closure
/// validation, so an unrelated large child is not reopened here.
/// This borrows the transaction, so callers must run it inside
/// `ExecutionTransaction::contribute`; on any returned error they must discard
/// every emission and let that contribution boundary poison the transaction.
pub fn visit_recipe_current_outer_changes_v1<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    C: FnMut() -> bool,
>(
    transaction: &ExecutionTransaction<'_, M>,
    qualification: &ReservedValue<IndustryFamilyQualificationV1>,
    context: &mut IndustryTransitionReadContextV1<'_, R, C>,
    work: &mut IndustryWorkBudgetV1,
    mut visit: impl FnMut(
        ReservedValue<IndustryCurrentOuterChangeV1>,
    ) -> Result<(), WorldFieldOverlayErrorV1>,
) -> Result<(), WorldFieldOverlayErrorV1> {
    if (context.cancelled)() {
        return Err(ValueTreeError::Cancelled.into());
    }
    let memory = context.values.memory;
    if context.values.class != MemoryClass::Ordinary
        || qualification.class != MemoryClass::Ordinary
        || !transaction.uses_budget(memory)
        || !qualification.uses_budget(memory)
        || qualification.reserved_bytes() < std::mem::size_of::<IndustryFamilyQualificationV1>()
    {
        return Err(ValueTreeError::Invalid("industry Current identity pool/class").into());
    }
    qualification.validate_binding(&transaction.parent_fence().parent_roots)?;
    let _workspace = memory
        .reserve(24 << 10, MemoryClass::Ordinary)
        .map_err(ValueTreeError::Memory)?;
    let kind = IndustryRecordKindV1::Recipe;
    let field = kind.descriptor()?;
    let cancellation = RefCell::new(&mut context.cancelled);
    let shared_work = RefCell::new(work);
    let mut records_cancelled = || (cancellation.borrow_mut())();
    let mut records = InventoryValueReader {
        values: &mut context.values,
        key_work: &mut *context.key_work,
        semantic_bytes: context.semantic_bytes,
        record_bytes: context.record_bytes,
        cancelled: &mut records_cancelled,
    };
    let (format_key, format_bytes, _) = marker_key(&mut records)?;
    let mut remaining = usize::try_from(shared_work.borrow().cells_remaining)
        .map_err(|_| ValueTreeError::ResourceLimited("industry pending work"))?;
    let check = || -> Result<(), WorldFieldOverlayErrorV1> {
        if (cancellation.borrow_mut())() {
            return Err(ValueTreeError::Cancelled.into());
        }
        shared_work.borrow_mut().cell()?;
        Ok(())
    };
    transaction.visit_pending_writes(
        StateNamespaceV1::Scheduler,
        &mut remaining,
        check,
        |raw_key, before, after| {
            if !raw_key.starts_with(b"oasis7.iws.world-field-key.v1\0") {
                return Ok(());
            }
            let binding = WorldFieldKeyV1::from_bytes(raw_key).map_err(ValueTreeError::Invalid)?;
            if binding.name != "pending_recipe_jobs" {
                return Ok(());
            }
            let key = WorldFieldKeyV1::new(field, binding.kind, binding.semantic_key)
                .map_err(ValueTreeError::Invalid)?;
            if key.as_bytes() != raw_key || binding.kind != WorldFieldRecordKindV1::Entry {
                return Err(invalid().into());
            }
            if (cancellation.borrow_mut())() {
                return Err(ValueTreeError::Cancelled.into());
            }
            if key == format_key {
                let (Some(before_root), Some(after_root)) = (before, after) else {
                    return Err(invalid().into());
                };
                for root in [&before_root, &after_root] {
                    verify_recipe_marker(root, &key, format_bytes, &mut records)?;
                }
                let charge = memory
                    .reserve(
                        std::mem::size_of::<IndustryCurrentOuterChangeV1>(),
                        MemoryClass::Ordinary,
                    )
                    .map_err(ValueTreeError::Memory)?;
                return visit(charge.retain(IndustryCurrentOuterChangeV1::FormatMarker {
                    before_value_root: before_root,
                    after_value_root: after_root,
                }));
            }
            if before.is_none() && after.is_none() {
                return Err(invalid().into());
            }
            let retained = std::mem::size_of::<IndustryCurrentOuterChangeV1>()
                .checked_add(
                    (usize::from(before.is_some()) + usize::from(after.is_some()))
                        .checked_mul(std::mem::size_of::<IndustryOuterRecordRootV1>() + 64)
                        .ok_or_else(invalid)?,
                )
                .ok_or_else(invalid)?;
            let charge = memory
                .reserve(retained, MemoryClass::Ordinary)
                .map_err(ValueTreeError::Memory)?;
            let mut before_value = None;
            let mut after_value = None;
            let mut identity = None;
            for (slot, root) in [(0usize, before.as_ref()), (1usize, after.as_ref())] {
                let Some(root) = root else { continue };
                let (id, record_root) = decode_recipe_root(root, &key, &mut records)?;
                if identity.is_some_and(|existing| existing != id) {
                    return Err(invalid().into());
                }
                identity = Some(id);
                let value = IndustryOuterRecordRootV1 {
                    value_root: *root,
                    record_root,
                };
                if slot == 0 {
                    before_value = Some(value);
                } else {
                    records.values.verify_closure(
                        root,
                        records.record_bytes,
                        &mut *records.cancelled,
                    )?;
                    after_value = Some(value);
                }
            }
            visit(charge.retain(IndustryCurrentOuterChangeV1::Record {
                id: identity.ok_or_else(invalid)?,
                before: before_value,
                after: after_value,
            }))
        },
    )?;
    Ok(())
}
