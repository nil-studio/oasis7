//! Bounded raw/bootstrap qualification of both complete Factory field families.
use super::key_read::OuterKeyDecoder;
use super::*;
use crate::incremental_inventory_value_read::InventoryValueReader;
use oasis7_formal_protocol::{
    incremental_execution::RootSetV1,
    incremental_record_root::{RECORD_ROOT_BYTES, RecordRootRefV1},
    incremental_state::{MerkleNodeReader, StateScanBudget, StateScanContext, StateScanError},
    incremental_value::{ValueNodeReader, ValueReadContext},
};
use oasis7_proto::memory_budget::MemoryClass;
use std::io;

/// A successful raw scan is integrity/schema qualification, never business or
/// chain authority. No public constructor, Clone or mutable binding is exposed.
#[derive(Debug)]
pub struct FactoryFamilyQualificationV1 {
    pub(super) roots: [u8; 32],
    pub(super) records: [u64; 2],
}
impl FactoryFamilyQualificationV1 {
    pub fn validate_binding(&self, roots: &[u8; 32]) -> Result<(), ValueTreeError> {
        if &self.roots != roots {
            return Err(ValueTreeError::Invalid("factory family root binding"));
        }
        Ok(())
    }
    pub fn record_counts(&self) -> [u64; 2] {
        self.records
    }
}
/// Caller-owned continuous budgets cover all six roots and every child. The
/// state/value sources must share the admitted pool and declared read class.
pub struct FactoryFamilyReadContextV1<'a, M, R, E, C> {
    pub state: &'a M,
    pub values: ValueReadContext<'a, R>,
    pub key_work: &'a mut SemanticKeyBudget,
    pub semantic_bytes: u64,
    pub record_bytes: u64,
    pub max_payload_bytes: u64,
    pub outer_scan: &'a mut StateScanBudget,
    pub take_state_error: E,
    pub cancelled: C,
}
fn invalid() -> ValueTreeError {
    ValueTreeError::Invalid("factory family closure")
}

/// Explicit raw/genesis/recovery scope. This intentionally scans the requested
/// complete families; ordinary point acceptance must use qualified-parent diff.
pub fn qualify_factory_family_v1<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    roots: &RootSetV1,
    context: &mut FactoryFamilyReadContextV1<'_, M, R, E, C>,
    work: &mut FactoryWorkBudgetV1,
) -> Result<ReservedValue<FactoryFamilyQualificationV1>, ValueTreeError> {
    if (context.cancelled)() {
        return Err(ValueTreeError::Cancelled);
    }
    let memory = context.values.memory;
    let class = context.values.class;
    let _workspace = memory.reserve(24 << 10, class)?;
    roots.validate().map_err(|_| invalid())?;
    if context.state.allocation_class() != Some(class)
        || context
            .state
            .allocation_budget()
            .is_none_or(|pool| !pool.same_pool(memory))
    {
        return Err(ValueTreeError::Invalid("factory family state pool/class"));
    }
    let mut metadata = [false; 2];
    let mut format = [false; 2];
    let mut counts = [0u64; 2];
    // StateScan cannot borrow the same FnMut twice. The RefCell only serializes
    // checks in traversal and its synchronous visitor; no callback is retained.
    let cancelled = std::cell::RefCell::new(&mut context.cancelled);
    for root in &roots.state {
        let result = StateScanContext {
            source: context.state,
            memory,
            class,
            io: &mut *context.outer_scan,
        }
        .visit_entries(
            root,
            |entry| {
                if !entry.key.starts_with(b"oasis7.iws.world-field-key.v1\0") {
                    return Ok(());
                }
                let binding =
                    WorldFieldKeyV1::from_bytes(&entry.key).map_err(ValueTreeError::Invalid)?;
                let (kind, index) = match binding.name {
                    "factories" => (FactoryRecordKindV1::Factory, 0),
                    "pending_factory_builds" => (FactoryRecordKindV1::Build, 1),
                    _ => return Ok(()),
                };
                work.cell()?;
                let field = kind.descriptor()?;
                let key = WorldFieldKeyV1::new(field, binding.kind, binding.semantic_key)
                    .map_err(ValueTreeError::Invalid)?;
                if root.namespace != kind.namespace()
                    || entry.namespace != kind.namespace()
                    || key.as_bytes() != entry.key
                {
                    return Err(invalid());
                }
                let mut cancellation = cancelled.borrow_mut();
                let mut records = InventoryValueReader {
                    values: &mut context.values,
                    key_work: &mut *context.key_work,
                    semantic_bytes: context.semantic_bytes,
                    record_bytes: context.record_bytes,
                    cancelled: &mut **cancellation,
                };
                records.values.verify_closure(
                    &entry.value,
                    records.record_bytes,
                    &mut *records.cancelled,
                )?;
                if binding.kind == WorldFieldRecordKindV1::Metadata {
                    if metadata[index] {
                        return Err(invalid());
                    }
                    records.presence(&entry.value, &key, 0)?;
                    metadata[index] = true;
                    return Ok(());
                }
                if binding.kind != WorldFieldRecordKindV1::Entry {
                    return Err(invalid());
                }
                let format_key = records.key(
                    field,
                    WorldFieldRecordKindV1::Entry,
                    &(RECORD_FORMAT_KEY, ()),
                )?;
                if key == format_key.0 {
                    if format[index] {
                        return Err(invalid());
                    }
                    read::value(
                        FactoryCellV1::Record,
                        &entry.value,
                        &format_key,
                        &mut records,
                        0,
                        &mut io::sink(),
                    )?;
                    format[index] = true;
                    return Ok(());
                }
                let child = reference(kind, &key, &entry.value, &mut records)?;
                closure::verify_child(kind, &child, &mut records, context.max_payload_bytes, work)?;
                counts[index] = counts[index].checked_add(1).ok_or(invalid())?;
                Ok(())
            },
            || (cancelled.borrow_mut())(),
        );
        if let Some(error) = (context.take_state_error)() {
            return Err(error);
        }
        match result {
            Ok(_) => {}
            Err(StateScanError::Memory(e)) => return Err(ValueTreeError::Memory(e)),
            Err(StateScanError::ResourceLimited) => {
                return Err(ValueTreeError::ResourceLimited("factory outer scan"));
            }
            Err(StateScanError::Cancelled) => return Err(ValueTreeError::Cancelled),
            Err(StateScanError::Visitor(e)) => return Err(e),
            Err(StateScanError::Invalid(_) | StateScanError::Tree(_)) => return Err(invalid()),
        }
    }
    if metadata != [true; 2] || format != [true; 2] {
        return Err(invalid());
    }
    let digest = roots.digest().map_err(|_| invalid())?;
    let charge = memory.reserve(
        std::mem::size_of::<FactoryFamilyQualificationV1>(),
        MemoryClass::Ordinary,
    )?;
    if (cancelled.borrow_mut())() {
        return Err(ValueTreeError::Cancelled);
    }
    Ok(charge.retain(FactoryFamilyQualificationV1 {
        roots: digest,
        records: counts,
    }))
}

pub(super) fn reference<R: ValueNodeReader, C: FnMut() -> bool>(
    kind: FactoryRecordKindV1,
    key: &WorldFieldKeyV1,
    value: &[u8; 32],
    records: &mut InventoryValueReader<'_, '_, R, C>,
) -> Result<oasis7_formal_protocol::incremental_state::StateRootV1, ValueTreeError> {
    let _scratch = records.values.memory.reserve(8192, records.values.class)?;
    records
        .values
        .verify_closure(value, records.record_bytes, &mut *records.cancelled)?;
    let mut scratch = [0u8; RECORD_ROOT_BYTES];
    let mut sink = io::sink();
    let mut decoder = OuterKeyDecoder::new(kind, &mut sink);
    let result = read_member_record_v1(
        value,
        key,
        records.values,
        MemberRecordLimitsV1 {
            semantic_bytes: records.semantic_bytes,
            payload_bytes: RECORD_ROOT_BYTES as u64,
            record_bytes: records.record_bytes,
        },
        &mut decoder,
        &mut &mut scratch[..],
        &mut *records.cancelled,
    );
    if let Some(reason) = decoder.bad {
        return Err(ValueTreeError::Invalid(reason));
    }
    let info = result?;
    decoder.finish()?;
    if info.codec != MemberPayloadCodecV1::RecordRoot
        || info.payload_bytes != RECORD_ROOT_BYTES as u64
    {
        return Err(invalid());
    }
    let reference = RecordRootRefV1::from_canonical_bytes(&scratch).map_err(|_| invalid())?;
    let binding = WorldFieldKeyV1::from_bytes(key.as_bytes()).map_err(ValueTreeError::Invalid)?;
    Ok(reference
        .validate_binding(
            &kind.schema(),
            &binding.semantic_key.ok_or(invalid())?,
            kind.namespace(),
        )
        .map_err(|_| invalid())?
        .clone())
}
