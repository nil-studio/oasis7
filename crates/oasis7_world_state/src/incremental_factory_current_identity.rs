//! Provisional identities decoded from the real Current `factories` write set.
//!
//! Emissions stay provisional until the complete visit returns `Ok`; callers
//! must discard every emitted value on any error. This reader binds the parent
//! family proof and reads only `ExecutionTransaction::visit_pending_writes`.
//! It does not mint a new family token or replace transition acceptance.
use super::key_read::{FactoryOuterKeyInfoV1, OuterKeyDecoder};
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
use std::{
    cell::RefCell,
    io::{self, Write},
};

/// The exact outer record value and its typed child root. The key identity is
/// carried separately, so embedded `factory_id` fields cannot replace it.
#[derive(Debug, PartialEq, Eq)]
pub struct FactoryOuterRecordRootV1 {
    pub value_root: [u8; 32],
    pub record_root: StateRootV1,
}

/// One identity emitted from an actual root-bound parent `factories` scan.
#[derive(Debug, PartialEq, Eq)]
pub enum FactoryQualifiedOuterIdentityV1 {
    FormatMarker {
        value_root: [u8; 32],
    },
    Record {
        id: String,
        value_root: [u8; 32],
        record_root: StateRootV1,
    },
}

/// A changed Current outer record or the family's format marker.
#[derive(Debug, PartialEq, Eq)]
pub enum FactoryCurrentOuterChangeV1 {
    FormatMarker {
        before_value_root: [u8; 32],
        after_value_root: [u8; 32],
    },
    Record {
        /// Original UTF-8 map key decoded from the authenticated semantic key.
        id: String,
        before: Option<FactoryOuterRecordRootV1>,
        after: Option<FactoryOuterRecordRootV1>,
    },
}

fn invalid() -> ValueTreeError {
    ValueTreeError::Invalid("factory Current outer identity")
}

fn root_header<R: ValueNodeReader, C: FnMut() -> bool>(
    value_root: &[u8; 32],
    key: &WorldFieldKeyV1,
    records: &mut InventoryValueReader<'_, '_, R, C>,
) -> Result<crate::incremental_field_member::MemberRecordInfoV1, ValueTreeError> {
    let info = crate::incremental_field_member::read_member_record_header_v1(
        value_root,
        key,
        records.values,
        MemberRecordLimitsV1 {
            semantic_bytes: records.semantic_bytes,
            payload_bytes: RECORD_ROOT_BYTES as u64,
            record_bytes: records.record_bytes,
        },
        &mut *records.cancelled,
    )?;
    if info.codec != MemberPayloadCodecV1::RecordRoot
        || info.payload_bytes != RECORD_ROOT_BYTES as u64
    {
        return Err(invalid());
    }
    Ok(info)
}

struct BoundedBytes<'a> {
    bytes: &'a mut Vec<u8>,
    limit: usize,
    failed: bool,
}
impl Write for BoundedBytes<'_> {
    fn write(&mut self, input: &[u8]) -> io::Result<usize> {
        let Some(end) = self.bytes.len().checked_add(input.len()) else {
            self.failed = true;
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "factory identity length",
            ));
        };
        if end > self.limit || end > self.bytes.capacity() {
            self.failed = true;
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "factory identity capacity",
            ));
        }
        self.bytes.extend_from_slice(input);
        Ok(input.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

struct ExactBytes<'a> {
    expected: &'a [u8],
    used: usize,
    failed: bool,
}
impl Write for ExactBytes<'_> {
    fn write(&mut self, input: &[u8]) -> io::Result<usize> {
        let Some(end) = self.used.checked_add(input.len()) else {
            self.failed = true;
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "factory identity mismatch",
            ));
        };
        if self.expected.get(self.used..end) != Some(input) {
            self.failed = true;
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "factory identity mismatch",
            ));
        }
        self.used = end;
        Ok(input.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

fn decode_factory_root<R: ValueNodeReader, C: FnMut() -> bool>(
    value_root: &[u8; 32],
    key: &WorldFieldKeyV1,
    records: &mut InventoryValueReader<'_, '_, R, C>,
    identity_output: &mut impl Write,
) -> Result<(FactoryOuterKeyInfoV1, StateRootV1), ValueTreeError> {
    records
        .values
        .verify_closure(value_root, records.record_bytes, &mut *records.cancelled)?;
    let _scratch = records
        .values
        .memory
        .reserve(8192, records.values.class)
        .map_err(ValueTreeError::Memory)?;
    let mut payload = [0; RECORD_ROOT_BYTES];
    let mut decoder = OuterKeyDecoder::new(FactoryRecordKindV1::Factory, identity_output);
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
    let identity = decoder.finish()?;
    if !matches!(identity, FactoryOuterKeyInfoV1::Text { .. }) {
        return Err(invalid());
    }
    let reference = RecordRootRefV1::from_canonical_bytes(&payload).map_err(|_| invalid())?;
    let binding = WorldFieldKeyV1::from_bytes(key.as_bytes()).map_err(ValueTreeError::Invalid)?;
    let child = reference
        .validate_binding(
            &FactoryRecordKindV1::Factory.schema(),
            &binding.semantic_key.ok_or_else(invalid)?,
            StateNamespaceV1::Business,
        )
        .map_err(|_| invalid())?
        .clone();
    if child.node_root.capacity() > 64 {
        return Err(ValueTreeError::ResourceLimited(
            "factory child root capacity",
        ));
    }
    Ok((identity, child))
}

pub(super) fn capture_factory_id<R: ValueNodeReader, C: FnMut() -> bool>(
    value_root: &[u8; 32],
    key: &WorldFieldKeyV1,
    records: &mut InventoryValueReader<'_, '_, R, C>,
) -> Result<
    (
        String,
        StateRootV1,
        oasis7_proto::memory_budget::MemoryReservation,
    ),
    ValueTreeError,
> {
    let header = root_header(value_root, key, records)?;
    let capacity = usize::try_from(header.semantic_bytes)
        .map_err(|_| ValueTreeError::ResourceLimited("factory identity capacity"))?;
    let charge_bytes = std::mem::size_of::<FactoryQualifiedOuterIdentityV1>()
        .checked_add(capacity)
        .and_then(|n| n.checked_add(std::mem::size_of::<StateRootV1>() + 64))
        .ok_or_else(invalid)?;
    let charge = records
        .values
        .memory
        .reserve(charge_bytes, records.values.class)
        .map_err(ValueTreeError::Memory)?;
    let mut bytes = Vec::new();
    if bytes.try_reserve_exact(capacity).is_err() || bytes.capacity() > capacity {
        return Err(ValueTreeError::ResourceLimited(
            "factory identity allocation",
        ));
    }
    let (identity, record_root, output_failed) = {
        let mut output = BoundedBytes {
            bytes: &mut bytes,
            limit: capacity,
            failed: false,
        };
        let (identity, record_root) = decode_factory_root(value_root, key, records, &mut output)?;
        (identity, record_root, output.failed)
    };
    let FactoryOuterKeyInfoV1::Text { utf8_bytes } = identity else {
        return Err(invalid());
    };
    if output_failed || usize::try_from(utf8_bytes).ok() != Some(bytes.len()) {
        return Err(invalid());
    }
    let id = String::from_utf8(bytes).map_err(|_| invalid())?;
    if id.capacity() > capacity || record_root.node_root.capacity() > 64 {
        return Err(ValueTreeError::ResourceLimited("factory identity capacity"));
    }
    Ok((id, record_root, charge))
}

fn verify_factory_marker<R: ValueNodeReader, C: FnMut() -> bool>(
    value_root: &[u8; 32],
    key: &WorldFieldKeyV1,
    semantic_bytes: u64,
    records: &mut InventoryValueReader<'_, '_, R, C>,
) -> Result<(), ValueTreeError> {
    records
        .values
        .verify_closure(value_root, records.record_bytes, &mut *records.cancelled)?;
    if read::value(
        FactoryCellV1::Record,
        value_root,
        &(key.clone(), semantic_bytes),
        records,
        0,
        &mut io::sink(),
    )? == FactoryValueV1::Unit
    {
        Ok(())
    } else {
        Err(invalid())
    }
}

/// Visit the complete `factories` parent directory from the authenticated
/// Business root. The `factories` inventory is read from that actual root; the
/// family token binds the accompanying full family RootSet. Emissions are
/// provisional until this scan returns `Ok`, and do not create qualification.
pub fn visit_qualified_factory_parent_identities_v1<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    roots: &RootSetV1,
    qualification: &ReservedValue<FactoryFamilyQualificationV1>,
    context: &mut FactoryFamilyReadContextV1<'_, M, R, E, C>,
    work: &mut FactoryWorkBudgetV1,
    mut visit: impl FnMut(ReservedValue<FactoryQualifiedOuterIdentityV1>) -> Result<(), ValueTreeError>,
) -> Result<(), ValueTreeError> {
    if (context.cancelled)() {
        return Err(ValueTreeError::Cancelled);
    }
    let memory = context.values.memory;
    let class = context.values.class;
    if qualification.class != class
        || !qualification.uses_budget(memory)
        || qualification.reserved_bytes() < std::mem::size_of::<FactoryFamilyQualificationV1>()
        || context.state.allocation_class() != Some(class)
        || context
            .state
            .allocation_budget()
            .is_none_or(|pool| !pool.same_pool(memory))
    {
        return Err(ValueTreeError::Invalid(
            "factory parent identity pool/class",
        ));
    }
    let _workspace = memory
        .reserve(24 << 10, class)
        .map_err(ValueTreeError::Memory)?;
    roots.validate().map_err(|_| invalid())?;
    qualification.validate_binding(&roots.digest().map_err(|_| invalid())?)?;
    let root = roots.state_root(StateNamespaceV1::Business);
    if root.namespace != StateNamespaceV1::Business {
        return Err(invalid());
    }
    let kind = FactoryRecordKindV1::Factory;
    let field = kind.descriptor()?;
    let cancelled = RefCell::new(&mut context.cancelled);
    let mut records_cancelled = || (cancelled.borrow_mut())();
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
    let metadata_key = WorldFieldKeyV1::new(field, WorldFieldRecordKindV1::Metadata, None)
        .map_err(ValueTreeError::Invalid)?;
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
            if binding.name != "factories" {
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
                verify_factory_marker(&entry.value, &key, marker_bytes, &mut records)?;
                marker = true;
                let charge = memory
                    .reserve(
                        std::mem::size_of::<FactoryQualifiedOuterIdentityV1>(),
                        class,
                    )
                    .map_err(ValueTreeError::Memory)?;
                return visit(
                    charge.retain(FactoryQualifiedOuterIdentityV1::FormatMarker {
                        value_root: entry.value,
                    }),
                );
            }
            let (id, record_root, charge) = capture_factory_id(&entry.value, &key, &mut records)?;
            record_count = record_count.checked_add(1).ok_or_else(invalid)?;
            visit(charge.retain(FactoryQualifiedOuterIdentityV1::Record {
                id,
                value_root: entry.value,
                record_root,
            }))
        },
        || (cancelled.borrow_mut())(),
    );
    if let Some(error) = (context.take_state_error)() {
        return Err(error);
    }
    match result {
        Ok(_) => {}
        Err(StateScanError::Memory(error)) => return Err(ValueTreeError::Memory(error)),
        Err(StateScanError::ResourceLimited) => {
            return Err(ValueTreeError::ResourceLimited(
                "factory parent identity scan",
            ));
        }
        Err(StateScanError::Cancelled) => return Err(ValueTreeError::Cancelled),
        Err(StateScanError::Visitor(error)) => return Err(error),
        Err(StateScanError::Invalid(_) | StateScanError::Tree(_)) => return Err(invalid()),
    }
    if !metadata
        || !marker
        || qualification.record_counts()[0] != record_count
        || (cancelled.borrow_mut())()
    {
        return Err(invalid());
    }
    Ok(())
}

/// Visit changed `factories` entries from the transaction's actual final
/// pending write set. Parent-side roots are covered by `qualification`; every
/// emitted identity includes only the outer semantic key and typed child-root
/// binding. The downstream complete family transition validates changed child
/// closure, so a small outer change does not reopen an unrelated large child.
/// Callback output is provisional until the whole traversal succeeds. This
/// borrows the transaction, so callers must run it inside
/// `ExecutionTransaction::contribute`; on any returned error they must discard
/// all emissions and let the contribution boundary poison the transaction.
pub fn visit_factory_current_outer_changes_v1<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    C: FnMut() -> bool,
>(
    transaction: &ExecutionTransaction<'_, M>,
    qualification: &ReservedValue<FactoryFamilyQualificationV1>,
    context: &mut FactoryTransitionReadContextV1<'_, R, C>,
    work: &mut FactoryWorkBudgetV1,
    mut visit: impl FnMut(
        ReservedValue<FactoryCurrentOuterChangeV1>,
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
        || qualification.reserved_bytes() < std::mem::size_of::<FactoryFamilyQualificationV1>()
    {
        return Err(ValueTreeError::Invalid("factory Current identity pool/class").into());
    }
    qualification.validate_binding(&transaction.parent_fence().parent_roots)?;
    let _workspace = memory
        .reserve(24 << 10, MemoryClass::Ordinary)
        .map_err(ValueTreeError::Memory)?;
    let namespace = FactoryRecordKindV1::Factory.namespace();
    let field = FactoryRecordKindV1::Factory.descriptor()?;

    let cancellation = RefCell::new(&mut context.cancelled);
    let shared_work = RefCell::new(work);
    let mut cancelled = || (cancellation.borrow_mut())();
    let mut records = InventoryValueReader {
        values: &mut context.values,
        key_work: &mut *context.key_work,
        semantic_bytes: context.semantic_bytes,
        record_bytes: context.record_bytes,
        cancelled: &mut cancelled,
    };
    let (marker_key, marker_semantic_bytes) = records.key(
        field,
        WorldFieldRecordKindV1::Entry,
        &(RECORD_FORMAT_KEY, ()),
    )?;
    let mut remaining = usize::try_from(shared_work.borrow().cells_remaining)
        .map_err(|_| ValueTreeError::ResourceLimited("factory pending work"))?;
    let check = || -> Result<(), WorldFieldOverlayErrorV1> {
        if (cancellation.borrow_mut())() {
            return Err(ValueTreeError::Cancelled.into());
        }
        shared_work.borrow_mut().cell()?;
        Ok(())
    };

    transaction.visit_pending_writes(
        namespace,
        &mut remaining,
        check,
        |raw_key, before, after| {
            if !raw_key.starts_with(b"oasis7.iws.world-field-key.v1\0") {
                return Ok(());
            }
            let binding = WorldFieldKeyV1::from_bytes(raw_key).map_err(ValueTreeError::Invalid)?;
            if binding.name != "factories" {
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
                    records.values.verify_closure(
                        root,
                        records.record_bytes,
                        &mut *records.cancelled,
                    )?;
                    if read::value(
                        FactoryCellV1::Record,
                        root,
                        &(key.clone(), marker_semantic_bytes),
                        &mut records,
                        0,
                        &mut io::sink(),
                    )? != FactoryValueV1::Unit
                    {
                        return Err(invalid().into());
                    }
                }
                let charge = memory
                    .reserve(
                        std::mem::size_of::<FactoryCurrentOuterChangeV1>(),
                        MemoryClass::Ordinary,
                    )
                    .map_err(ValueTreeError::Memory)?;
                return visit(charge.retain(FactoryCurrentOuterChangeV1::FormatMarker {
                    before_value_root: before_root,
                    after_value_root: after_root,
                }));
            }
            if before.is_none() && after.is_none() {
                return Err(invalid().into());
            }
            // Full family qualification authenticates the before-side outer
            // records. Decode one original key, then compare any second side
            // byte-for-byte rather than trusting the outer digest alone.
            let first_root = before.as_ref().or(after.as_ref()).ok_or_else(invalid)?;
            let header = root_header(first_root, &key, &mut records)?;
            let semantic_capacity = usize::try_from(header.semantic_bytes)
                .map_err(|_| ValueTreeError::ResourceLimited("factory identity capacity"))?;
            let root_count = usize::from(before.is_some()) + usize::from(after.is_some());
            let charge_bytes = std::mem::size_of::<FactoryCurrentOuterChangeV1>()
                .checked_add(semantic_capacity)
                .and_then(|n| {
                    root_count
                        .checked_mul(std::mem::size_of::<FactoryOuterRecordRootV1>() + 64)
                        .and_then(|roots| n.checked_add(roots))
                })
                .ok_or_else(invalid)?;
            let charge = memory
                .reserve(charge_bytes, MemoryClass::Ordinary)
                .map_err(ValueTreeError::Memory)?;
            let mut id = Vec::new();
            if id.try_reserve_exact(semantic_capacity).is_err() || id.capacity() > semantic_capacity
            {
                return Err(ValueTreeError::ResourceLimited("factory identity allocation").into());
            }
            let mut first_value = None;
            let mut second_value = None;
            let mut captured = false;
            for (slot, root) in [(0usize, before.as_ref()), (1usize, after.as_ref())] {
                let Some(root) = root else { continue };
                let value = if !captured {
                    let (identity, child, output_failed) = {
                        let mut output = BoundedBytes {
                            bytes: &mut id,
                            limit: semantic_capacity,
                            failed: false,
                        };
                        let (identity, child) =
                            decode_factory_root(root, &key, &mut records, &mut output)?;
                        (identity, child, output.failed)
                    };
                    let FactoryOuterKeyInfoV1::Text { utf8_bytes } = identity else {
                        return Err(invalid().into());
                    };
                    if output_failed || usize::try_from(utf8_bytes).ok() != Some(id.len()) {
                        return Err(invalid().into());
                    }
                    captured = true;
                    Some(FactoryOuterRecordRootV1 {
                        value_root: *root,
                        record_root: child,
                    })
                } else {
                    let expected = id.as_slice();
                    let (identity, child, output_failed, output_used) = {
                        let mut output = ExactBytes {
                            expected,
                            used: 0,
                            failed: false,
                        };
                        let (identity, child) =
                            decode_factory_root(root, &key, &mut records, &mut output)?;
                        (identity, child, output.failed, output.used)
                    };
                    let FactoryOuterKeyInfoV1::Text { utf8_bytes } = identity else {
                        return Err(invalid().into());
                    };
                    if output_failed
                        || output_used != expected.len()
                        || usize::try_from(utf8_bytes).ok() != Some(expected.len())
                    {
                        return Err(invalid().into());
                    }
                    Some(FactoryOuterRecordRootV1 {
                        value_root: *root,
                        record_root: child,
                    })
                };
                if slot == 0 {
                    first_value = value;
                } else {
                    second_value = value;
                }
            }
            if !captured {
                return Err(invalid().into());
            }
            let id = String::from_utf8(id).map_err(|_| invalid())?;
            if id.capacity() > semantic_capacity {
                return Err(ValueTreeError::ResourceLimited("factory identity capacity").into());
            }
            visit(charge.retain(FactoryCurrentOuterChangeV1::Record {
                id,
                before: first_value,
                after: second_value,
            }))
        },
    )?;
    Ok(())
}
