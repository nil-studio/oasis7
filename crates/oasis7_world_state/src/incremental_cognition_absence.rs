//! Explicit bounded audit of current Cognition absence. Enabled current-state
//! qualification keeps its required-column contract; this type is separate.
use crate::{
    incremental_field_read::WorldFieldReadContextV1, incremental_projection::FieldOwner,
    incremental_world_field_key::WorldFieldKeyV1,
};
use oasis7_formal_protocol::{
    incremental_execution::RootSetV1,
    incremental_state::{MerkleNodeReader, StateScanBudget, StateScanContext, StateScanError},
    incremental_value::{ValueNodeReader, ValueTreeError},
};
use oasis7_proto::memory_budget::{MemoryClass, ReservedValue};

#[derive(Debug)]
pub struct CognitionCurrentAbsenceQualificationV1 {
    roots: [u8; 32],
    class: MemoryClass,
}
impl CognitionCurrentAbsenceQualificationV1 {
    pub(crate) fn class(&self) -> MemoryClass {
        self.class
    }
    pub fn validate_binding(&self, roots: &[u8; 32]) -> Result<(), ValueTreeError> {
        if &self.roots == roots {
            Ok(())
        } else {
            Err(ValueTreeError::Invalid("Cognition absence RootSet binding"))
        }
    }
}
pub(crate) fn copy_cognition_absence_ordinary_v1(
    source: &ReservedValue<CognitionCurrentAbsenceQualificationV1>,
    roots: &[u8; 32],
    memory: &oasis7_proto::memory_budget::MemoryBudget,
    mut cancelled: impl FnMut() -> bool,
) -> Result<ReservedValue<CognitionCurrentAbsenceQualificationV1>, ValueTreeError> {
    if cancelled() {
        return Err(ValueTreeError::Cancelled);
    }
    if !source.uses_budget(memory)
        || source.class != MemoryClass::Recovery
        || source.reserved_bytes() < std::mem::size_of::<CognitionCurrentAbsenceQualificationV1>()
    {
        return Err(ValueTreeError::Invalid(
            "Cognition absence readmission source",
        ));
    }
    let retained = memory.reserve(1024, MemoryClass::Ordinary)?;
    source.validate_binding(roots)?;
    if cancelled() {
        return Err(ValueTreeError::Cancelled);
    }
    Ok(retained.retain(CognitionCurrentAbsenceQualificationV1 {
        roots: source.roots,
        class: MemoryClass::Ordinary,
    }))
}
/// This explicit audit scans all six authenticated top trees. It is not a
/// mandatory startup/ordinary-write entrypoint or a full-World qualification.
pub fn qualify_cognition_current_absence_v1<M, R, E, C>(
    roots: &RootSetV1,
    context: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
    scan: &mut StateScanBudget,
    cells_remaining: &mut u64,
) -> Result<ReservedValue<CognitionCurrentAbsenceQualificationV1>, ValueTreeError>
where
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
{
    context.check()?;
    if let Some(error) = (context.take_state_error)() {
        return Err(error);
    }
    let charge = context
        .values
        .memory
        .reserve(16 << 10, context.values.class)?;
    roots
        .validate()
        .map_err(|_| ValueTreeError::Invalid("Cognition absence RootSet"))?;
    if !roots.state.iter().any(|root| root == context.root) {
        return Err(ValueTreeError::Invalid("Cognition absence context root"));
    }
    for root in &roots.state {
        let result = StateScanContext {
            source: context.state,
            memory: context.values.memory,
            class: context.values.class,
            io: &mut *scan,
        }
        .visit_entries(
            root,
            |entry| -> Result<(), ValueTreeError> {
                *cells_remaining = cells_remaining
                    .checked_sub(1)
                    .ok_or(ValueTreeError::ResourceLimited("Cognition absence cells"))?;
                if entry.key.starts_with(b"oasis7.iws.world-field-key.v1\0") {
                    let binding =
                        WorldFieldKeyV1::from_bytes(&entry.key).map_err(ValueTreeError::Invalid)?;
                    // Any owned leaf rejects absence, not just a known schema or
                    // Metadata marker. Unknown/orphan/wrong-namespace rows count.
                    if binding.owner == FieldOwner::Cognition {
                        return Err(ValueTreeError::Invalid(
                            "Cognition cell present during absence audit",
                        ));
                    }
                }
                Ok(())
            },
            &mut context.cancelled,
        );
        if let Some(error) = (context.take_state_error)() {
            return Err(error);
        }
        match result {
            Ok(_) => {}
            Err(StateScanError::Visitor(error)) => return Err(error),
            Err(StateScanError::Memory(error)) => return Err(ValueTreeError::Memory(error)),
            Err(StateScanError::Cancelled) => return Err(ValueTreeError::Cancelled),
            Err(StateScanError::ResourceLimited) => {
                return Err(ValueTreeError::ResourceLimited(
                    "Cognition absence State scan",
                ));
            }
            Err(_) => return Err(ValueTreeError::Invalid("Cognition absence State integrity")),
        }
    }
    let roots = roots
        .digest()
        .map_err(|_| ValueTreeError::Invalid("Cognition absence root digest"))?;
    if (context.cancelled)() {
        return Err(ValueTreeError::Cancelled);
    }
    Ok(charge.retain(CognitionCurrentAbsenceQualificationV1 {
        roots,
        class: context.values.class,
    }))
}

pub(crate) fn copy_checked_empty_cognition_group_v3(
    source: &ReservedValue<crate::incremental_cognition_family::CheckedCognitionCurrentEmptyV3>,
    memory: &oasis7_proto::memory_budget::MemoryBudget,
    mut cancelled: impl FnMut() -> bool,
) -> Result<ReservedValue<CognitionCurrentAbsenceQualificationV1>, ValueTreeError> {
    if cancelled() {
        return Err(ValueTreeError::Cancelled);
    }
    if !source.uses_budget(memory)
        || source.reserved_bytes() < 32 << 10
        || source.class() != MemoryClass::Ordinary
    {
        return Err(ValueTreeError::Invalid("checked empty Cognition group"));
    }
    let charge = memory.reserve(16 << 10, MemoryClass::Ordinary)?;
    let output = CognitionCurrentAbsenceQualificationV1 {
        roots: *source.roots(),
        class: MemoryClass::Ordinary,
    };
    if cancelled() {
        return Err(ValueTreeError::Cancelled);
    }
    Ok(charge.retain(output))
}
pub(crate) fn advance_cognition_absence_no_changes_v3(
    source: &ReservedValue<CognitionCurrentAbsenceQualificationV1>,
    witness: &crate::incremental_verified_world_transition::VerifiedWorldTransitionV1,
    memory: &oasis7_proto::memory_budget::MemoryBudget,
    mut cancelled: impl FnMut() -> bool,
) -> Result<ReservedValue<CognitionCurrentAbsenceQualificationV1>, ValueTreeError> {
    if cancelled() {
        return Err(ValueTreeError::Cancelled);
    }
    witness.check_pool_class(memory, MemoryClass::Ordinary)?;
    if !source.uses_budget(memory) || source.class() != MemoryClass::Ordinary {
        return Err(ValueTreeError::Invalid("Cognition absence predecessor"));
    }
    source.validate_binding(witness.before_digest())?;
    for write in &witness.record().writes {
        if cancelled() {
            return Err(ValueTreeError::Cancelled);
        }
        if write
            .mutation
            .key
            .starts_with(b"oasis7.iws.world-field-key.v1\0")
            && WorldFieldKeyV1::from_bytes(&write.mutation.key)
                .map_err(ValueTreeError::Invalid)?
                .owner
                == FieldOwner::Cognition
        {
            return Err(ValueTreeError::Invalid("Cognition absent changed entry"));
        }
    }
    let charge = memory.reserve(16 << 10, MemoryClass::Ordinary)?;
    let output = CognitionCurrentAbsenceQualificationV1 {
        roots: *witness.after_digest(),
        class: MemoryClass::Ordinary,
    };
    if cancelled() {
        return Err(ValueTreeError::Cancelled);
    }
    Ok(charge.retain(output))
}
