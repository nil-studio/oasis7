//! Shared resource admission for immutable, closed record/family metadata.
//! No public constructor or arbitrary mint is exposed by this module.
use oasis7_formal_protocol::{incremental_execution::RootSetV1, incremental_value::ValueTreeError};
use oasis7_proto::memory_budget::{MemoryBudget, MemoryClass, MemoryReservation, ReservedValue};
fn invalid() -> ValueTreeError {
    ValueTreeError::Invalid("qualification phase/pool/class/binding")
}
#[derive(Clone, Copy)]
pub(crate) enum FamilyBindingV1<'a> {
    Roots(&'a RootSetV1),
    Digest(&'a [u8; 32]),
}
impl FamilyBindingV1<'_> {
    pub(crate) fn digest(self) -> Result<[u8; 32], ValueTreeError> {
        match self {
            Self::Roots(r) => r.digest().map_err(|_| invalid()),
            Self::Digest(r) => Ok(*r),
        }
    }
}
pub(crate) trait PhaseQualificationV1: Sized {
    const RETAINED_BYTES: usize;
    type Binding<'a>;
    fn phase_class(&self) -> MemoryClass;
    fn validate_phase_binding(&self, binding: Self::Binding<'_>) -> Result<(), ValueTreeError>;
    fn from_readmission(permit: ReadmissionPermitV1<'_, Self>) -> Self;
}
/// The type parameter and private exact-carrier borrow prevent using a permit
/// issued for another qualification to construct or reclassify this token.
pub(crate) struct ReadmissionPermitV1<'a, T> {
    source: &'a ReservedValue<T>,
}
impl<T> ReadmissionPermitV1<'_, T> {
    pub(crate) fn source(&self) -> &T {
        self.source
    }
    pub(crate) fn class(&self) -> MemoryClass {
        MemoryClass::Ordinary
    }
}
pub(crate) fn check_class_v1<T: PhaseQualificationV1>(
    source: &ReservedValue<T>,
    memory: &MemoryBudget,
    class: MemoryClass,
) -> Result<(), ValueTreeError> {
    if T::RETAINED_BYTES < std::mem::size_of::<T>()
        || !source.uses_budget(memory)
        || source.reserved_bytes() < T::RETAINED_BYTES
        || source.phase_class() != class
    {
        return Err(invalid());
    }
    Ok(())
}
/// Reserve the complete bounded working phase before evaluating a RootSet
/// digest, root comparison, or caller-visible qualification operation.
pub(crate) fn bind_phase_v1<T: PhaseQualificationV1>(
    source: &ReservedValue<T>,
    binding: T::Binding<'_>,
    memory: &MemoryBudget,
    class: MemoryClass,
    mut cancelled: impl FnMut() -> bool,
) -> Result<MemoryReservation, ValueTreeError> {
    if cancelled() {
        return Err(ValueTreeError::Cancelled);
    }
    check_class_v1(source, memory, class)?;
    let phase = memory.reserve(128 << 10, class)?;
    source.validate_phase_binding(binding)?;
    if cancelled() {
        return Err(ValueTreeError::Cancelled);
    }
    Ok(phase)
}
pub(crate) fn readmit_ordinary_v1<T: PhaseQualificationV1>(
    source: &ReservedValue<T>,
    binding: T::Binding<'_>,
    memory: &MemoryBudget,
    mut cancelled: impl FnMut() -> bool,
) -> Result<ReservedValue<T>, ValueTreeError> {
    if cancelled() {
        return Err(ValueTreeError::Cancelled);
    }
    check_class_v1(source, memory, MemoryClass::Recovery)?;
    let _phase = memory.reserve(128 << 10, MemoryClass::Ordinary)?;
    source.validate_phase_binding(binding)?;
    let retained = memory.reserve(T::RETAINED_BYTES, MemoryClass::Ordinary)?;
    let output = T::from_readmission(ReadmissionPermitV1 { source });
    if cancelled() {
        return Err(ValueTreeError::Cancelled);
    }
    Ok(retained.retain(output))
}
