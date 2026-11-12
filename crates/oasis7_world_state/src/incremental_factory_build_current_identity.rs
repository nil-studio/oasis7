//! Provisional identities from the authenticated pending factory-build map.
//!
//! The semantic map key is a hash and cannot be inverted to recover its
//! ActionId. This visitor decodes the original typed outer key, re-derives the
//! registered semantic key, and checks the child record-root binding before
//! exposing an identity. Results remain provisional until the full visit
//! succeeds.

use super::{
    FactoryFamilyQualificationV1, FactoryFamilyReadContextV1, FactoryRecordKindV1,
    FactoryTransitionReadContextV1, FactoryWorkBudgetV1, RECORD_FORMAT_KEY, WorldFieldKeyV1,
    WorldFieldRecordKindV1,
    key_read::{FactoryOuterKeyInfoV1, OuterKeyDecoder},
};
use crate::{
    incremental_field_overlay::WorldFieldOverlayErrorV1,
    incremental_inventory_value_read::InventoryValueReader,
};
use oasis7_formal_protocol::{
    incremental_execution::{ExecutionTransaction, RootSetV1},
    incremental_record_root::{RECORD_ROOT_BYTES, RecordRootRefV1},
    incremental_state::{MerkleNodeReader, StateNamespaceV1, StateScanContext, StateScanError},
    incremental_value::{ValueNodeReader, ValueTreeError},
};
use oasis7_proto::memory_budget::{MemoryClass, ReservedValue};
use std::{cell::RefCell, io};

/// One pending build from the actual Scheduler root, after verifying its
/// original integer key, typed record root, namespace and family binding.
#[derive(Debug, PartialEq, Eq)]
pub struct FactoryBuildParentIdentityV1 {
    pub id: u64,
    pub value_root: [u8; 32],
    pub record_root: oasis7_formal_protocol::incremental_state::StateRootV1,
}

/// One changed pending-build outer record from the transaction's coalesced
/// Scheduler write set. The callback output remains provisional until the
/// complete visitor succeeds.
#[derive(Debug, PartialEq, Eq)]
pub struct FactoryBuildCurrentOuterRecordV1 {
    pub value_root: [u8; 32],
    pub record_root: oasis7_formal_protocol::incremental_state::StateRootV1,
}

#[derive(Debug, PartialEq, Eq)]
pub enum FactoryBuildCurrentOuterChangeV1 {
    FormatMarker {
        before_value_root: [u8; 32],
        after_value_root: [u8; 32],
    },
    Record {
        id: u64,
        before: Option<FactoryBuildCurrentOuterRecordV1>,
        after: Option<FactoryBuildCurrentOuterRecordV1>,
    },
}

fn invalid() -> ValueTreeError {
    ValueTreeError::Invalid("factory build parent identity")
}

fn decode_build_root<R: ValueNodeReader, C: FnMut() -> bool>(
    value_root: &[u8; 32],
    key: &WorldFieldKeyV1,
    records: &mut InventoryValueReader<'_, '_, R, C>,
) -> Result<(u64, oasis7_formal_protocol::incremental_state::StateRootV1), ValueTreeError> {
    records
        .values
        .verify_closure(value_root, records.record_bytes, &mut *records.cancelled)?;
    let _scratch = records
        .values
        .memory
        .reserve(8192, records.values.class)
        .map_err(ValueTreeError::Memory)?;
    let mut payload = [0; RECORD_ROOT_BYTES];
    let mut sink = io::sink();
    let mut decoder = OuterKeyDecoder::new(FactoryRecordKindV1::Build, &mut sink);
    let result = crate::incremental_field_member::read_member_record_v1(
        value_root,
        key,
        records.values,
        crate::incremental_field_member::MemberRecordLimitsV1 {
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
    if info.codec != crate::incremental_field_member::MemberPayloadCodecV1::RecordRoot
        || info.payload_bytes != RECORD_ROOT_BYTES as u64
    {
        return Err(invalid());
    }
    let FactoryOuterKeyInfoV1::Unsigned(id) = decoder.finish()? else {
        return Err(invalid());
    };
    let field = FactoryRecordKindV1::Build.descriptor()?;
    let (expected_key, _) = records.key(field, WorldFieldRecordKindV1::Entry, &id)?;
    if expected_key.as_bytes() != key.as_bytes() {
        return Err(invalid());
    }
    let binding = WorldFieldKeyV1::from_bytes(key.as_bytes()).map_err(ValueTreeError::Invalid)?;
    let semantic_key = binding.semantic_key.ok_or_else(invalid)?;
    let reference = RecordRootRefV1::from_canonical_bytes(&payload).map_err(|_| invalid())?;
    let record_root = reference
        .validate_binding(
            &FactoryRecordKindV1::Build.schema(),
            &semantic_key,
            StateNamespaceV1::Scheduler,
        )
        .map_err(|_| invalid())?
        .clone();
    if record_root.node_root.capacity() > 64 {
        return Err(ValueTreeError::ResourceLimited(
            "factory build child root capacity",
        ));
    }
    Ok((id, record_root))
}

fn verify_build_marker<R: ValueNodeReader, C: FnMut() -> bool>(
    value_root: &[u8; 32],
    key: &WorldFieldKeyV1,
    semantic_bytes: u64,
    records: &mut InventoryValueReader<'_, '_, R, C>,
) -> Result<(), ValueTreeError> {
    records
        .values
        .verify_closure(value_root, records.record_bytes, &mut *records.cancelled)?;
    if super::read::value(
        super::FactoryCellV1::Record,
        value_root,
        &(key.clone(), semantic_bytes),
        records,
        0,
        &mut io::sink(),
    )? == super::FactoryValueV1::Unit
    {
        Ok(())
    } else {
        Err(invalid())
    }
}

/// Visit every pending factory-build entry from the exact qualified parent.
/// The caller's state scan, semantic-key budget, IO budget, factory cells and
/// cancellation closure are carried continuously; this helper creates no
/// replacement work allowance and does not qualify any source.
pub fn visit_qualified_build_parent_identities_v1<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    roots: &RootSetV1,
    qualification: &ReservedValue<FactoryFamilyQualificationV1>,
    context: &mut FactoryFamilyReadContextV1<'_, M, R, E, C>,
    work: &mut FactoryWorkBudgetV1,
    mut visit: impl FnMut(ReservedValue<FactoryBuildParentIdentityV1>) -> Result<(), ValueTreeError>,
) -> Result<(), ValueTreeError> {
    if (context.cancelled)() {
        return Err(ValueTreeError::Cancelled);
    }
    let memory = context.values.memory;
    let class = context.values.class;
    if qualification.allocation_class() != class
        || !qualification.uses_budget(memory)
        || qualification.reserved_bytes() < std::mem::size_of::<FactoryFamilyQualificationV1>()
        || context.state.allocation_class() != Some(class)
        || context
            .state
            .allocation_budget()
            .is_none_or(|pool| !pool.same_pool(memory))
    {
        return Err(invalid());
    }
    let _workspace = memory
        .reserve(24 << 10, class)
        .map_err(ValueTreeError::Memory)?;
    roots.validate().map_err(|_| invalid())?;
    let digest = roots.digest().map_err(|_| invalid())?;
    qualification.validate_binding(&digest)?;
    let root = roots.state_root(StateNamespaceV1::Scheduler);
    if root.namespace != StateNamespaceV1::Scheduler {
        return Err(invalid());
    }
    let kind = FactoryRecordKindV1::Build;
    let field = kind.descriptor()?;
    let (marker_key, marker_bytes) = {
        let mut records = InventoryValueReader {
            values: &mut context.values,
            key_work: &mut *context.key_work,
            semantic_bytes: context.semantic_bytes,
            record_bytes: context.record_bytes,
            cancelled: &mut context.cancelled,
        };
        records.key(
            field,
            WorldFieldRecordKindV1::Entry,
            &(RECORD_FORMAT_KEY, ()),
        )?
    };
    let metadata_key = WorldFieldKeyV1::new(field, WorldFieldRecordKindV1::Metadata, None)
        .map_err(ValueTreeError::Invalid)?;
    let mut metadata = false;
    let mut marker = false;
    let mut record_count = 0u64;
    let cancellation = RefCell::new(&mut context.cancelled);
    let mut records_cancelled = || (cancellation.borrow_mut())();
    let mut records = InventoryValueReader {
        values: &mut context.values,
        key_work: &mut *context.key_work,
        semantic_bytes: context.semantic_bytes,
        record_bytes: context.record_bytes,
        cancelled: &mut records_cancelled,
    };
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
            if binding.name != "pending_factory_builds" {
                return Ok(());
            }
            let key = WorldFieldKeyV1::new(field, binding.kind, binding.semantic_key)
                .map_err(ValueTreeError::Invalid)?;
            if entry.namespace != StateNamespaceV1::Scheduler || key.as_bytes() != entry.key {
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
                verify_build_marker(&entry.value, &key, marker_bytes, &mut records)?;
                marker = true;
                return Ok(());
            }
            let (id, record_root) = decode_build_root(&entry.value, &key, &mut records)?;
            let (expected_key, _) = records.key(field, WorldFieldRecordKindV1::Entry, &id)?;
            if expected_key.as_bytes() != entry.key {
                return Err(invalid());
            }
            record_count = record_count.checked_add(1).ok_or_else(invalid)?;
            let bytes = std::mem::size_of::<FactoryBuildParentIdentityV1>()
                .checked_add(std::mem::size_of::<
                    oasis7_formal_protocol::incremental_state::StateRootV1,
                >())
                .and_then(|value| value.checked_add(64))
                .ok_or_else(invalid)?;
            let charge = memory
                .reserve(bytes, class)
                .map_err(ValueTreeError::Memory)?;
            visit(charge.retain(FactoryBuildParentIdentityV1 {
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
            return Err(ValueTreeError::ResourceLimited("factory build parent scan"));
        }
        Err(StateScanError::Cancelled) => return Err(ValueTreeError::Cancelled),
        Err(StateScanError::Visitor(error)) => return Err(error),
        Err(StateScanError::Invalid(_) | StateScanError::Tree(_)) => return Err(invalid()),
    }
    if !metadata
        || !marker
        || qualification.record_counts()[1] != record_count
        || (cancellation.borrow_mut())()
    {
        return Err(invalid());
    }
    Ok(())
}

/// Visit coalesced Current changes to pending factory builds. This is a point
/// diff over the actual transaction write set; the parent completeness and
/// unchanged records are supplied by the separately checked family token.
/// Call inside `ExecutionTransaction::contribute` and discard every emission
/// unless the complete visit returns `Ok`.
pub fn visit_build_current_outer_changes_v1<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    C: FnMut() -> bool,
>(
    transaction: &ExecutionTransaction<'_, M>,
    qualification: &ReservedValue<FactoryFamilyQualificationV1>,
    context: &mut FactoryTransitionReadContextV1<'_, R, C>,
    work: &mut FactoryWorkBudgetV1,
    mut visit: impl FnMut(
        ReservedValue<FactoryBuildCurrentOuterChangeV1>,
    ) -> Result<(), WorldFieldOverlayErrorV1>,
) -> Result<(), WorldFieldOverlayErrorV1> {
    if (context.cancelled)() {
        return Err(ValueTreeError::Cancelled.into());
    }
    let memory = context.values.memory;
    if context.values.class != MemoryClass::Ordinary
        || qualification.allocation_class() != MemoryClass::Ordinary
        || !transaction.uses_budget(memory)
        || !qualification.uses_budget(memory)
        || qualification.reserved_bytes() < std::mem::size_of::<FactoryFamilyQualificationV1>()
    {
        return Err(ValueTreeError::Invalid("factory build Current identity pool/class").into());
    }
    qualification.validate_binding(&transaction.parent_fence().parent_roots)?;
    let _workspace = memory
        .reserve(24 << 10, MemoryClass::Ordinary)
        .map_err(ValueTreeError::Memory)?;
    let kind = FactoryRecordKindV1::Build;
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
    let (marker_key, marker_bytes) = records.key(
        field,
        WorldFieldRecordKindV1::Entry,
        &(RECORD_FORMAT_KEY, ()),
    )?;
    let mut remaining = usize::try_from(shared_work.borrow().cells_remaining)
        .map_err(|_| ValueTreeError::ResourceLimited("factory build pending work"))?;
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
            if binding.name != "pending_factory_builds" {
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
            if key == marker_key {
                let (Some(before_root), Some(after_root)) = (before, after) else {
                    return Err(invalid().into());
                };
                for root in [&before_root, &after_root] {
                    verify_build_marker(root, &key, marker_bytes, &mut records)?;
                }
                let charge = memory
                    .reserve(
                        std::mem::size_of::<FactoryBuildCurrentOuterChangeV1>(),
                        MemoryClass::Ordinary,
                    )
                    .map_err(ValueTreeError::Memory)?;
                return visit(
                    charge.retain(FactoryBuildCurrentOuterChangeV1::FormatMarker {
                        before_value_root: before_root,
                        after_value_root: after_root,
                    }),
                );
            }
            if before.is_none() && after.is_none() {
                return Err(invalid().into());
            }
            let roots = usize::from(before.is_some()) + usize::from(after.is_some());
            let charge_bytes = std::mem::size_of::<FactoryBuildCurrentOuterChangeV1>()
                .checked_add(
                    roots
                        .checked_mul(
                            std::mem::size_of::<FactoryBuildCurrentOuterRecordV1>()
                                + std::mem::size_of::<
                                    oasis7_formal_protocol::incremental_state::StateRootV1,
                                >()
                                + 64,
                        )
                        .ok_or_else(invalid)?,
                )
                .ok_or_else(invalid)?;
            let charge = memory
                .reserve(charge_bytes, MemoryClass::Ordinary)
                .map_err(ValueTreeError::Memory)?;
            let mut identity = None;
            let mut before_value = None;
            let mut after_value = None;
            for (slot, root) in [(0usize, before.as_ref()), (1usize, after.as_ref())] {
                let Some(root) = root else { continue };
                let (id, record_root) = decode_build_root(root, &key, &mut records)?;
                if identity.is_some_and(|prior| prior != id) {
                    return Err(invalid().into());
                }
                identity = Some(id);
                let value = FactoryBuildCurrentOuterRecordV1 {
                    value_root: *root,
                    record_root,
                };
                if slot == 0 {
                    before_value = Some(value);
                } else {
                    after_value = Some(value);
                }
            }
            visit(charge.retain(FactoryBuildCurrentOuterChangeV1::Record {
                id: identity.ok_or_else(invalid)?,
                before: before_value,
                after: after_value,
            }))
        },
    )?;
    Ok(())
}
