//! Integrity replay plus Logistics-specific incremental closure qualification.
use super::*;
use crate::{
    incremental_field_overlay::WorldFieldOverlayErrorV1,
    incremental_inventory_value_read::InventoryValueReader,
};
use oasis7_formal_protocol::{
    incremental_execution::{
        ExecutionParent, PreparedTransition, TransitionRecordV1, TransitionVerifier,
    },
    incremental_value::{ValueNodeReader, ValueReadContext},
};
use oasis7_proto::memory_budget::MemoryClass;
use std::cell::RefCell;

pub struct LogisticsTransitionReadContextV1<'a, R, C> {
    pub values: ValueReadContext<'a, R>,
    pub key_work: &'a mut SemanticKeyBudget,
    pub semantic_bytes: u64,
    pub record_bytes: u64,
    pub max_payload_bytes: u64,
    pub cancelled: C,
}
/// Immutable, independently replayed transition and its exact after-family token.
pub struct LogisticsPreparedTransitionV1 {
    transition: ReservedValue<PreparedTransition>,
    qualification: ReservedValue<LogisticsFamilyQualificationV1>,
}
impl LogisticsPreparedTransitionV1 {
    pub fn transition(&self) -> &PreparedTransition {
        &self.transition
    }
    pub fn qualification(&self) -> &ReservedValue<LogisticsFamilyQualificationV1> {
        &self.qualification
    }
}
fn invalid() -> ValueTreeError {
    ValueTreeError::Invalid("logistics typed transition")
}
/// Raw writes cannot bypass this receiver: generic replay authenticates every
/// actual outer mutation; the typed pass qualifies all Logistics child changes.
/// Equal child subtrees inherit parent qualification without scanning payloads.
pub fn verify_logistics_transition_v1<R: ValueNodeReader, C: FnMut() -> bool>(
    parent: &ExecutionParent,
    operation: [u8; 32],
    record: &TransitionRecordV1,
    qualification: &ReservedValue<LogisticsFamilyQualificationV1>,
    context: &mut LogisticsTransitionReadContextV1<'_, R, C>,
    work: &mut LogisticsWorkBudgetV1,
) -> Result<LogisticsPreparedTransitionV1, WorldFieldOverlayErrorV1> {
    if (context.cancelled)() {
        return Err(ValueTreeError::Cancelled.into());
    }
    let memory = context.values.memory;
    if context.values.class != MemoryClass::Ordinary || !qualification.uses_budget(memory) {
        return Err(invalid().into());
    }
    let _workspace = memory
        .reserve(24 << 10, MemoryClass::Ordinary)
        .map_err(ValueTreeError::Memory)?;
    qualification.validate_binding(&record.before.digest()?)?;
    let prepared = {
        let source = tree::Source {
            source: context.values.source,
            memory,
            class: MemoryClass::Ordinary,
            io: RefCell::new(&mut *context.values.io),
            cancelled: RefCell::new(&mut context.cancelled),
            error: RefCell::new(None),
        };
        let result = TransitionVerifier {
            parent,
            operation,
            source: &source,
            memory,
            class: MemoryClass::Ordinary,
            max_pending_node_bytes: work.max_pending_node_bytes,
        }
        .verify(record);
        if let Some(error) = source.error.borrow_mut().take() {
            return Err(error.into());
        }
        if (source.cancelled.borrow_mut())() {
            return Err(ValueTreeError::Cancelled.into());
        }
        result?
    };
    let mut counts = qualification.record_counts();
    for write in &prepared.record.writes {
        if (context.cancelled)() {
            return Err(ValueTreeError::Cancelled.into());
        }
        if !write
            .mutation
            .key
            .starts_with(b"oasis7.iws.world-field-key.v1\0")
        {
            continue;
        }
        let binding =
            WorldFieldKeyV1::from_bytes(&write.mutation.key).map_err(ValueTreeError::Invalid)?;
        let Some(kind) = LogisticsRecordKindV1::ALL
            .into_iter()
            .find(|kind| kind.field_name() == binding.name)
        else {
            continue;
        };
        let index = kind.index();
        work.cell()?;
        let field = kind.descriptor()?;
        let key = WorldFieldKeyV1::new(field, binding.kind, binding.semantic_key)
            .map_err(ValueTreeError::Invalid)?;
        if write.namespace != kind.namespace()
            || key.as_bytes() != write.mutation.key
            || binding.kind != WorldFieldRecordKindV1::Entry
        {
            return Err(invalid().into());
        }
        let mut records = InventoryValueReader {
            values: &mut context.values,
            key_work: &mut *context.key_work,
            semantic_bytes: context.semantic_bytes,
            record_bytes: context.record_bytes,
            cancelled: &mut context.cancelled,
        };
        let format = records.key(
            field,
            WorldFieldRecordKindV1::Entry,
            &(RECORD_FORMAT_KEY, ()),
        )?;
        if key == format.0 {
            return Err(invalid().into());
        }
        let before = write
            .mutation
            .before
            .map(|value| family::reference(kind, &key, &value, &mut records))
            .transpose()?;
        let after = write
            .mutation
            .after
            .map(|value| family::reference(kind, &key, &value, &mut records))
            .transpose()?;
        match (&before, &after) {
            (None, Some(after)) => {
                closure::verify_child(kind, after, &mut records, context.max_payload_bytes, work)?;
                counts[index] = counts[index].checked_add(1).ok_or(invalid())?;
            }
            (Some(before), Some(after)) => advance::child(
                kind,
                before,
                after,
                &mut records,
                work,
                context.max_payload_bytes,
            )?,
            (Some(_), None) => {
                counts[index] = counts[index].checked_sub(1).ok_or(invalid())?;
            }
            (None, None) => return Err(invalid().into()),
        }
    }
    let digest = prepared.record.after.digest()?;
    let charge = memory
        .reserve(
            std::mem::size_of::<LogisticsFamilyQualificationV1>(),
            MemoryClass::Ordinary,
        )
        .map_err(ValueTreeError::Memory)?;
    if (context.cancelled)() {
        return Err(ValueTreeError::Cancelled.into());
    }
    Ok(LogisticsPreparedTransitionV1 {
        transition: prepared,
        qualification: charge.retain(LogisticsFamilyQualificationV1 {
            roots: digest,
            records: counts,
        }),
    })
}
