//! Cross-binding only: selected History view and the exact current-state heads.
//! Neither this token nor a current-state proof certifies historical originals.
use super::*;
use crate::{
    incremental_cognition_absence::CognitionCurrentAbsenceQualificationV1,
    incremental_cognition_family::CognitionCurrentStateQualificationV1,
    incremental_cognition_fields::CognitionControlFieldV1 as Field,
    incremental_cognition_read::{CognitionControlReadV1, read_cognition_control_v1},
    incremental_field_read::WorldFieldReadContextV1,
};
use oasis7_formal_protocol::{
    incremental_execution::RootSetV1,
    incremental_state::{MerkleNodeReader, StateNamespaceV1},
    incremental_value::{ValueNodeReader, ValueTreeError},
};
use oasis7_proto::memory_budget::{MemoryClass, ReservedValue};
use std::io::{self, Write};

pub enum CognitionHistoryCurrentSourceV1<'a> {
    Enabled(&'a ReservedValue<CognitionCurrentStateQualificationV1>),
    Disabled(&'a ReservedValue<CognitionCurrentAbsenceQualificationV1>),
}
pub struct CognitionHistoryCurrentBindingV1 {
    roots: [u8; 32],
    head: [u8; 32],
    enabled: bool,
    class: MemoryClass,
}
impl CognitionHistoryCurrentBindingV1 {
    pub(crate) fn class(&self) -> MemoryClass {
        self.class
    }
    pub fn roots_id(&self) -> [u8; 32] {
        self.roots
    }
    pub fn head_record_id(&self) -> [u8; 32] {
        self.head
    }
    pub fn enabled(&self) -> bool {
        self.enabled
    }
}
enum OrdinaryCurrent {
    Enabled(ReservedValue<CognitionCurrentStateQualificationV1>),
    Disabled(ReservedValue<CognitionCurrentAbsenceQualificationV1>),
}
pub struct CognitionHistoryCurrentReadmissionV1 {
    binding: ReservedValue<CognitionHistoryCurrentBindingV1>,
    current: OrdinaryCurrent,
}
impl CognitionHistoryCurrentReadmissionV1 {
    pub fn binding(&self) -> &ReservedValue<CognitionHistoryCurrentBindingV1> {
        &self.binding
    }
    pub fn current(&self) -> CognitionHistoryCurrentSourceV1<'_> {
        match &self.current {
            OrdinaryCurrent::Enabled(proof) => CognitionHistoryCurrentSourceV1::Enabled(proof),
            OrdinaryCurrent::Disabled(proof) => CognitionHistoryCurrentSourceV1::Disabled(proof),
        }
    }
}
/// Resource-only migration of all fixed current-state proof metadata. The
/// accepted Ordinary view and exact old head-value binding are required; no
/// source, page reader, JSON traversal or historical authority is introduced.
pub fn readmit_cognition_history_current_ordinary_v1(
    source: &ReservedValue<CognitionHistoryCurrentBindingV1>,
    current: CognitionHistoryCurrentSourceV1<'_>,
    view: &ReservedValue<AcceptedCognitionHistoryViewV1>,
    roots: &RootSetV1,
    memory: &oasis7_proto::memory_budget::MemoryBudget,
    mut cancelled: impl FnMut() -> bool,
) -> Result<ReservedValue<CognitionHistoryCurrentReadmissionV1>, ValueTreeError> {
    if cancelled() {
        return Err(ValueTreeError::Cancelled);
    }
    if !source.uses_budget(memory)
        || source.class != MemoryClass::Recovery
        || source.reserved_bytes() < std::mem::size_of::<CognitionHistoryCurrentBindingV1>()
        || !view.uses_budget(memory)
        || view.class() != MemoryClass::Ordinary
        || view.occurrence().allocation_class() != MemoryClass::Ordinary
    {
        return Err(ValueTreeError::Invalid("Cognition current resource source"));
    }
    let retained = memory.reserve(8192, MemoryClass::Ordinary)?;
    let binding_charge = memory.reserve(1024, MemoryClass::Ordinary)?;
    let digest = roots
        .digest()
        .map_err(|_| ValueTreeError::Invalid("Cognition current resource roots"))?;
    if source.roots != digest
        || source.head != view.occurrence().head_record_id()
        || view.manifest().roots != digest
        || view.occurrence().roots_id() != digest
        || source.enabled != view.manifest().enabled
    {
        return Err(ValueTreeError::Invalid(
            "Cognition current resource binding",
        ));
    }
    let current = match current {
        CognitionHistoryCurrentSourceV1::Enabled(proof) if source.enabled => {
            OrdinaryCurrent::Enabled(
                crate::incremental_cognition_family::copy_cognition_current_ordinary_v1(
                    proof,
                    roots,
                    memory,
                    &mut cancelled,
                )?,
            )
        }
        CognitionHistoryCurrentSourceV1::Disabled(proof) if !source.enabled => {
            OrdinaryCurrent::Disabled(
                crate::incremental_cognition_absence::copy_cognition_absence_ordinary_v1(
                    proof,
                    &digest,
                    memory,
                    &mut cancelled,
                )?,
            )
        }
        _ => {
            return Err(ValueTreeError::Invalid(
                "Cognition current resource presence",
            ));
        }
    };
    let binding = binding_charge.retain(CognitionHistoryCurrentBindingV1 {
        roots: source.roots,
        head: source.head,
        enabled: source.enabled,
        class: MemoryClass::Ordinary,
    });
    if cancelled() {
        return Err(ValueTreeError::Cancelled);
    }
    Ok(retained.retain(CognitionHistoryCurrentReadmissionV1 { binding, current }))
}
pub fn bind_cognition_history_current_v1<M, R, E, C>(
    view: &ReservedValue<AcceptedCognitionHistoryViewV1>,
    roots: &RootSetV1,
    current: CognitionHistoryCurrentSourceV1<'_>,
    context: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
) -> Result<ReservedValue<CognitionHistoryCurrentBindingV1>, ValueTreeError>
where
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
{
    context.check()?;
    let memory = context.values.memory;
    let charge = memory.reserve(8192, context.values.class)?;
    let digest = roots
        .digest()
        .map_err(|_| ValueTreeError::Invalid("Cognition History current roots"))?;
    if !view.uses_budget(memory)
        || view.class() != context.values.class
        || view.occurrence().allocation_class() != context.values.class
        || view.manifest().roots != digest
        || view.occurrence().roots_id() != digest
        || view.occurrence().head_body_id() != view.occurrence().fact_body_id()
        || context.root != roots.state_root(StateNamespaceV1::Allocator)
    {
        return Err(ValueTreeError::Invalid("Cognition History current binding"));
    }
    match current {
        CognitionHistoryCurrentSourceV1::Disabled(proof) => {
            if !proof.uses_budget(memory)
                || proof.class() != context.values.class
                || view.manifest().enabled
            {
                return Err(ValueTreeError::Invalid("Cognition History disabled source"));
            }
            proof.validate_binding(&digest)?;
        }
        CognitionHistoryCurrentSourceV1::Enabled(proof) => {
            if !proof.uses_budget(memory)
                || proof.class() != context.values.class
                || !view.manifest().enabled
            {
                return Err(ValueTreeError::Invalid("Cognition History enabled source"));
            }
            proof.validate_binding(&digest)?;
            let economy = proof.control_present(Field::EconomySchema);
            if economy != view.manifest().indexes[2].present {
                return Err(ValueTreeError::Invalid("Cognition History economy cohort"));
            }
            for (index, sequence, text) in [
                (0, Field::JournalHeadSeq, Field::JournalHeadDigest),
                (1, Field::EconomyHeadSeq, Field::EconomyHeadDigest),
                (
                    2,
                    Field::EconomyProvisionHeadSeq,
                    Field::EconomyProvisionHeadDigest,
                ),
            ] {
                if index != 0 && !economy {
                    continue;
                }
                let expected = &view.manifest().heads[index];
                if read_cognition_control_v1(sequence, context, &mut io::sink())?
                    != CognitionControlReadV1::Unsigned(expected.sequence)
                {
                    return Err(ValueTreeError::Invalid(
                        "Cognition History current head sequence",
                    ));
                }
                let mut bytes = [0; 71];
                let length = if let Some(digest) = expected.digest {
                    bytes[..7].copy_from_slice(b"blake3:");
                    bytes[7..].copy_from_slice(blake3::Hash::from(digest).to_hex().as_bytes());
                    71
                } else {
                    0
                };
                let mut output = Compare {
                    expected: &bytes[..length],
                    at: 0,
                    equal: true,
                };
                let result = read_cognition_control_v1(text, context, &mut output)?;
                if result
                    != (CognitionControlReadV1::Text {
                        bytes: length as u64,
                    })
                    || !output.equal
                    || output.at != length
                {
                    return Err(ValueTreeError::Invalid(
                        "Cognition History current head digest",
                    ));
                }
            }
        }
    }
    context.check()?;
    Ok(charge.retain(CognitionHistoryCurrentBindingV1 {
        class: context.values.class,
        roots: digest,
        head: view.occurrence().head_record_id(),
        enabled: view.manifest().enabled,
    }))
}
struct Compare<'a> {
    expected: &'a [u8],
    at: usize,
    equal: bool,
}
impl Write for Compare<'_> {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        let end = self.at.checked_add(bytes.len());
        self.equal &= end.and_then(|end| self.expected.get(self.at..end)) == Some(bytes);
        self.at = end.unwrap_or(usize::MAX);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
