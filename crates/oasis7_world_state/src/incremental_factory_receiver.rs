//! Integrity replay plus Factory-specific incremental closure qualification.
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

pub struct FactoryTransitionReadContextV1<'a, R, C> {
    pub values: ValueReadContext<'a, R>,
    pub key_work: &'a mut SemanticKeyBudget,
    pub semantic_bytes: u64,
    pub record_bytes: u64,
    pub max_payload_bytes: u64,
    pub cancelled: C,
}
/// Immutable, independently replayed transition and its exact after-family token.
pub struct FactoryPreparedTransitionV1 {
    transition: ReservedValue<PreparedTransition>,
    qualification: ReservedValue<FactoryFamilyQualificationV1>,
}
impl FactoryPreparedTransitionV1 {
    pub fn transition(&self) -> &PreparedTransition {
        &self.transition
    }
    pub fn qualification(&self) -> &ReservedValue<FactoryFamilyQualificationV1> {
        &self.qualification
    }
}
fn invalid() -> ValueTreeError {
    ValueTreeError::Invalid("factory typed transition")
}
/// Raw writes cannot bypass this receiver: generic replay authenticates every
/// actual outer mutation; the typed pass qualifies all Factory child changes.
/// Equal child subtrees inherit parent qualification without scanning payloads.
pub fn verify_factory_transition_v1<R: ValueNodeReader, C: FnMut() -> bool>(
    parent: &ExecutionParent,
    operation: [u8; 32],
    record: &TransitionRecordV1,
    qualification: &ReservedValue<FactoryFamilyQualificationV1>,
    context: &mut FactoryTransitionReadContextV1<'_, R, C>,
    work: &mut FactoryWorkBudgetV1,
) -> Result<FactoryPreparedTransitionV1, WorldFieldOverlayErrorV1> {
    {
        if (context.cancelled)() {
            return Err(ValueTreeError::Cancelled.into());
        }
        let memory = context.values.memory;
        if context.values.class != MemoryClass::Ordinary
            || qualification.class != MemoryClass::Ordinary
            || !qualification.uses_budget(memory)
            || qualification.reserved_bytes() < std::mem::size_of::<FactoryFamilyQualificationV1>()
        {
            return Err(invalid().into());
        }
        let _workspace = memory
            .reserve(24 << 10, MemoryClass::Ordinary)
            .map_err(ValueTreeError::Memory)?;
        qualification.validate_binding(&record.before.digest()?)?;
    }
    let memory = context.values.memory;
    let witness = {
        let source = tree::Source {
            source: context.values.source,
            memory,
            class: MemoryClass::Ordinary,
            io: RefCell::new(&mut *context.values.io),
            cancelled: RefCell::new(&mut context.cancelled),
            error: RefCell::new(None),
        };
        let verifier = TransitionVerifier {
            parent,
            operation,
            source: &source,
            memory,
            class: MemoryClass::Ordinary,
            max_pending_node_bytes: work.max_pending_node_bytes,
        };
        let mut verify =
            crate::incremental_verified_world_transition::WorldTransitionVerificationContextV1 {
                after_root: record.after.state_root(
                    oasis7_formal_protocol::incremental_state::StateNamespaceV1::Business,
                ),
                memory,
                class: MemoryClass::Ordinary,
                take_state_error: || source.error.borrow_mut().take(),
                cancelled: || (source.cancelled.borrow_mut())(),
            };
        crate::incremental_verified_world_transition::verify_world_transition_in_v1(
            &verifier,
            record,
            &mut verify,
        )?
    };
    let qualification = accept_factory_verified_v1(&witness, qualification, context, work)?;
    Ok(FactoryPreparedTransitionV1 {
        transition: witness.into_prepared(),
        qualification,
    })
}

pub(crate) fn accept_factory_verified_v1<R: ValueNodeReader, C: FnMut() -> bool>(
    witness: &crate::incremental_verified_world_transition::VerifiedWorldTransitionV1,
    qualification: &ReservedValue<FactoryFamilyQualificationV1>,
    context: &mut FactoryTransitionReadContextV1<'_, R, C>,
    work: &mut FactoryWorkBudgetV1,
) -> Result<ReservedValue<FactoryFamilyQualificationV1>, WorldFieldOverlayErrorV1> {
    let record = witness.record();
    witness.check_pool_class(context.values.memory, context.values.class)?;

    if (context.cancelled)() {
        return Err(ValueTreeError::Cancelled.into());
    }
    let memory = context.values.memory;
    if context.values.class != MemoryClass::Ordinary
        || qualification.class != MemoryClass::Ordinary
        || !qualification.uses_budget(memory)
        || qualification.reserved_bytes() < std::mem::size_of::<FactoryFamilyQualificationV1>()
    {
        return Err(invalid().into());
    }
    let _workspace = memory
        .reserve(24 << 10, MemoryClass::Ordinary)
        .map_err(ValueTreeError::Memory)?;
    qualification.validate_binding(&record.before.digest()?)?;
    let prepared = witness.prepared();
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
        let (kind, index) = match binding.name {
            "factories" => (FactoryRecordKindV1::Factory, 0),
            "pending_factory_builds" => (FactoryRecordKindV1::Build, 1),
            _ => continue,
        };
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
            std::mem::size_of::<FactoryFamilyQualificationV1>(),
            MemoryClass::Ordinary,
        )
        .map_err(ValueTreeError::Memory)?;
    if (context.cancelled)() {
        return Err(ValueTreeError::Cancelled.into());
    }
    Ok(charge.retain(FactoryFamilyQualificationV1 {
        roots: digest,
        records: counts,
        class: MemoryClass::Ordinary,
    }))
}
