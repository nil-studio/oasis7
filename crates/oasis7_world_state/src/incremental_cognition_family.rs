//! Complete current-state Cognition routing over all six namespace roots.
//! History/ProofLedger/RetentionLedger and semantic authority remain separate.
use crate::{
    incremental_cognition_fields::CognitionControlFieldV1,
    incremental_cognition_json::CognitionJsonFieldV1,
    incremental_cognition_json_read::{self as json_read, CognitionJsonEntryQualificationV1},
    incremental_field_member::*,
    incremental_field_read::WorldFieldReadContextV1,
    incremental_inventory_value_read::InventoryValueReader,
    incremental_json_records::JsonReadWorkV1,
    incremental_semantic_key::hash_semantic_key_v1,
    incremental_utf8_output::read_text_value,
};
use oasis7_formal_protocol::{
    incremental_execution::RootSetV1,
    incremental_state::{MerkleNodeReader, StateScanBudget, StateScanContext, StateScanError},
    incremental_value::{ValueNodeReader, ValueReadContext, ValueTreeError},
};
use oasis7_proto::memory_budget::ReservedValue;
use std::{cell::RefCell, sync::Arc};
fn invalid() -> ValueTreeError {
    ValueTreeError::Invalid("cognition current-state family closure")
}
#[derive(Debug)]
pub struct CognitionCurrentStateQualificationV1 {
    roots: [u8; 32],
    controls: u16,
    entries: [Option<Arc<ReservedValue<CognitionJsonEntryQualificationV1>>>; 20],
}
impl CognitionCurrentStateQualificationV1 {
    pub fn validate_binding(&self, roots: &[u8; 32]) -> Result<(), ValueTreeError> {
        if &self.roots == roots {
            Ok(())
        } else {
            Err(invalid())
        }
    }
    pub fn json_entry(
        &self,
        field: CognitionJsonFieldV1,
        roots: &[u8; 32],
    ) -> Result<Option<&ReservedValue<CognitionJsonEntryQualificationV1>>, ValueTreeError> {
        self.validate_binding(roots)?;
        let index = CognitionJsonFieldV1::ALL
            .iter()
            .position(|value| *value == field)
            .ok_or_else(invalid)?;
        Ok(self.entries[index].as_ref().map(|proof| proof.as_ref()))
    }
    pub fn control_present(&self, field: CognitionControlFieldV1) -> bool {
        CognitionControlFieldV1::ALL
            .iter()
            .position(|value| *value == field)
            .is_some_and(|index| self.controls & (1 << index) != 0)
    }
}
pub(crate) fn read_control<R: ValueNodeReader, C: FnMut() -> bool>(
    field: CognitionControlFieldV1,
    key: &(WorldFieldKeyV1, u64),
    root: &[u8; 32],
    records: &mut InventoryValueReader<'_, '_, R, C>,
) -> Result<(), ValueTreeError> {
    if field.unsigned() {
        records.unsigned(root, &key.0, key.1)?;
    } else {
        let schema = match field {
            CognitionControlFieldV1::Schema => Some("cognition-persistence.v1"),
            CognitionControlFieldV1::JournalSchema => Some("cognition-journal.v1"),
            CognitionControlFieldV1::EconomySchema => Some("cognition-economy.v1"),
            _ => None,
        };
        let mut output =
            crate::incremental_expected_bytes::Compare::new(schema.unwrap_or("").as_bytes());
        let profile = MemberReadProfileV1 {
            codec: MemberPayloadCodecV1::Text,
            semantic_bytes: key.1,
            limits: MemberRecordLimitsV1 {
                semantic_bytes: records.semantic_bytes,
                payload_bytes: records.record_bytes,
                record_bytes: records.record_bytes,
            },
        };
        if let Some(schema) = schema {
            let bytes = read_text_value(
                root,
                &key.0,
                records.values,
                profile,
                &mut output,
                "cognition control UTF-8",
                &mut *records.cancelled,
            )?;
            if bytes != schema.len() as u64 || !output.is_equal() {
                return Err(invalid());
            }
        } else {
            read_text_value(
                root,
                &key.0,
                records.values,
                profile,
                &mut std::io::sink(),
                "cognition control UTF-8",
                &mut *records.cancelled,
            )?;
        }
    }
    Ok(())
}
pub fn qualify_cognition_current_state_v1<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    roots: &RootSetV1,
    context: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
    scan: &mut StateScanBudget,
    work: &mut JsonReadWorkV1,
    outer_cells_remaining: &mut u64,
) -> Result<ReservedValue<CognitionCurrentStateQualificationV1>, ValueTreeError> {
    context.check()?;
    roots.validate().map_err(|_| invalid())?;
    if !roots.state.iter().any(|root| root == context.root) {
        return Err(invalid());
    }
    // Covers all Arc allocations/metadata, including retained ownership after
    // parent qualifiers drop. Bodies retain their own separately admitted charge.
    let retained = context
        .values
        .memory
        .reserve(32 << 10, context.values.class)?;
    let unit = hash_semantic_key_v1(
        &(),
        context.values.memory,
        context.values.class,
        context.key_work,
        &mut context.cancelled,
    )
    .map_err(|error| match error {
        crate::incremental_semantic_key::SemanticKeyError::Memory(error) => {
            ValueTreeError::Memory(error)
        }
        crate::incremental_semantic_key::SemanticKeyError::ResourceLimited => {
            ValueTreeError::ResourceLimited("cognition family Unit key")
        }
        crate::incremental_semantic_key::SemanticKeyError::Cancelled => ValueTreeError::Cancelled,
        crate::incremental_semantic_key::SemanticKeyError::Invalid(error) => {
            ValueTreeError::Invalid(error)
        }
    })?;
    let cancel = RefCell::new(&mut context.cancelled);
    let mut seen = [[false; 2]; 30];
    let mut entries = std::array::from_fn(|_| None);
    for root in &roots.state {
        let mut local = WorldFieldReadContextV1 {
            root,
            state: context.state,
            values: ValueReadContext {
                source: context.values.source,
                memory: context.values.memory,
                class: context.values.class,
                io: &mut *context.values.io,
            },
            key_work: &mut *context.key_work,
            semantic_bytes: context.semantic_bytes,
            record_bytes: context.record_bytes,
            take_state_error: &mut context.take_state_error,
            cancelled: || (cancel.borrow_mut())(),
        };
        let result = StateScanContext {
            source: local.state,
            memory: local.values.memory,
            class: local.values.class,
            io: &mut *scan,
        }
        .visit_entries(
            root,
            |entry| {
                *outer_cells_remaining = outer_cells_remaining
                    .checked_sub(1)
                    .ok_or(ValueTreeError::ResourceLimited("cognition outer cells"))?;
                if !entry.key.starts_with(b"oasis7.iws.world-field-key.v1\0") {
                    return Ok(());
                }
                let binding =
                    WorldFieldKeyV1::from_bytes(&entry.key).map_err(ValueTreeError::Invalid)?;
                if binding.owner != FieldOwner::Cognition {
                    return Ok(());
                }
                let control = CognitionControlFieldV1::ALL
                    .into_iter()
                    .enumerate()
                    .find(|(_, field)| field.descriptor().name == binding.name);
                let json = CognitionJsonFieldV1::ALL
                    .into_iter()
                    .enumerate()
                    .find(|(_, field)| field.name() == binding.name);
                let (index, descriptor) = if let Some((index, field)) = control {
                    (index, field.descriptor())
                } else if let Some((index, field)) = json {
                    (10 + index, field.descriptor())
                } else {
                    return Err(invalid());
                };
                if descriptor.destination != FieldDestination::State(entry.namespace) {
                    return Err(invalid());
                }
                let slot = match binding.kind {
                    WorldFieldRecordKindV1::Metadata => 0,
                    WorldFieldRecordKindV1::Entry => 1,
                    _ => return Err(invalid()),
                };
                let key = WorldFieldKeyV1::new(
                    descriptor,
                    binding.kind,
                    (slot == 1).then_some(unit.digest),
                )
                .map_err(ValueTreeError::Invalid)?;
                if entry.key != key.as_bytes() || seen[index][slot] {
                    return Err(invalid());
                }
                seen[index][slot] = true;
                if slot == 0 {
                    local.records().presence(&entry.value, &key, 0)?;
                } else if let Some((_, field)) = control {
                    read_control(
                        field,
                        &(key, unit.encoded_bytes),
                        &entry.value,
                        &mut local.records(),
                    )?;
                } else if let Some((json_index, field)) = json {
                    let child = json_read::outer(
                        field,
                        &(key, unit.encoded_bytes),
                        &entry.value,
                        &mut local.records(),
                    )?;
                    let body = json_read::qualify_body(field, &child, &mut local, work)?;
                    entries[json_index] = Some(Arc::new(body));
                }
                Ok(())
            },
            || (cancel.borrow_mut())(),
        );
        let error = (local.take_state_error)();
        if (cancel.borrow_mut())() {
            return Err(ValueTreeError::Cancelled);
        }
        if let Some(error) = error {
            return Err(error);
        }
        match result {
            Ok(_) => {}
            Err(StateScanError::Visitor(error)) => return Err(error),
            Err(StateScanError::Memory(error)) => return Err(error.into()),
            Err(StateScanError::Cancelled) => return Err(ValueTreeError::Cancelled),
            Err(StateScanError::ResourceLimited) => {
                return Err(ValueTreeError::ResourceLimited(
                    "cognition family State scan",
                ));
            }
            Err(_) => return Err(invalid()),
        }
    }
    for pair in seen {
        if pair[0] != pair[1] {
            return Err(invalid());
        }
    }
    let mut controls = 0u16;
    for (index, pair) in seen[..10].iter().enumerate() {
        if pair[1] {
            controls |= 1 << index;
        }
    }
    check_layout(
        controls,
        &std::array::from_fn(|index| entries[index].is_some()),
    )?;
    let roots = roots.digest().map_err(|_| invalid())?;
    if (cancel.borrow_mut())() {
        return Err(ValueTreeError::Cancelled);
    }
    Ok(retained.retain(CognitionCurrentStateQualificationV1 {
        roots,
        controls,
        entries,
    }))
}

#[path = "incremental_cognition_receiver.rs"]
mod receiver;
pub use receiver::{
    accept_cognition_current_scalar_transition_v1, accept_cognition_current_transition_v1,
};

#[path = "incremental_cognition_pages.rs"]
mod pages;
pub use pages::*;

pub(super) fn check_layout(controls: u16, present: &[bool; 20]) -> Result<(), ValueTreeError> {
    let present_control = |field: CognitionControlFieldV1| {
        CognitionControlFieldV1::ALL
            .iter()
            .position(|value| *value == field)
            .is_some_and(|index| controls & (1 << index) != 0)
    };
    for field in CognitionControlFieldV1::REQUIRED {
        if !present_control(field) {
            return Err(invalid());
        }
    }
    for (index, field) in CognitionJsonFieldV1::ALL.into_iter().enumerate() {
        if !field.optional() && !present[index] {
            return Err(invalid());
        }
    }
    let economy = present_control(CognitionControlFieldV1::EconomySchema);
    for field in CognitionControlFieldV1::ALL {
        if field.descriptor().name.starts_with("economy.") && present_control(field) != economy {
            return Err(invalid());
        }
    }
    let delegation = present[16];
    for (index, field) in CognitionJsonFieldV1::ALL.into_iter().enumerate() {
        if field.name().starts_with("economy.") && present[index] != economy
            || field.name().starts_with("delegation.") && present[index] != delegation
        {
            return Err(invalid());
        }
    }
    Ok(())
}

#[path = "incremental_cognition_group_receiver.rs"]
mod group_receiver;
pub use group_receiver::accept_cognition_current_lifecycle_transition_v1;
