//! One-time full selected-source recovery for the original Runtime DTO.
//! Ordinary transitions use projection's changed-path reader instead.
use super::*;
use oasis7_proto::memory_budget::{MemoryBudget, MemoryReservation, ReservedValue};
pub struct QualifiedRuntimeRevocationProjectionColumnsV1 {
    value: RevocationRecoveredV1<crate::CapabilityRevocationState>,
    body: StateRootV1,
    roots: [u8; 32],
    memory: MemoryBudget,
    _fee: MemoryReservation,
}
impl QualifiedRuntimeRevocationProjectionColumnsV1 {
    /// Original DATA only. This getter is not an authorization or status grant.
    pub fn value(&self) -> &crate::CapabilityRevocationState {
        self.value.value()
    }
    pub fn body(&self) -> &StateRootV1 {
        &self.body
    }
    pub fn validate_binding(
        &self,
        qualification: &ReservedValue<RuntimeRevocationQualificationV1>,
        roots: &oasis7_formal_protocol::incremental_execution::RootSetV1,
        memory: &MemoryBudget,
    ) -> Result<(), ValueTreeError> {
        qualification.validate_binding(roots)?;
        if !self.memory.same_pool(memory)
            || !self._fee.uses_budget(memory)
            || !qualification.uses_budget(memory)
            || qualification.allocation_class() != MemoryClass::Ordinary
            || self.body != *qualification.body()
            || self.roots != roots.digest().map_err(|_| invalid())?
        {
            return Err(invalid());
        }
        Ok(())
    }
}
/// Genuine Q and original RootSet select the complete original typed body.
/// The caller lends finite scan/codec/IO resources; no default or new meter is
/// introduced, and no projection/accepted cache is minted by this reader.
pub fn read_qualified_runtime_revocation_projection_columns_v1<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    qualification: &ReservedValue<RuntimeRevocationQualificationV1>,
    roots: &oasis7_formal_protocol::incremental_execution::RootSetV1,
    read: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
    scan: &mut StateScanBudget,
    work: &mut RevocationReadWorkV1,
) -> Result<QualifiedRuntimeRevocationProjectionColumnsV1, ValueTreeError> {
    qualification.validate_binding(roots)?;
    if qualification.allocation_class() != MemoryClass::Ordinary
        || read.values.class != MemoryClass::Ordinary
        || !qualification.uses_budget(read.values.memory)
        || read.root != roots.state_root(StateNamespaceV1::Authorization)
    {
        return Err(invalid());
    }
    read.check()?;
    let fee = read.values.memory.reserve(
        std::mem::size_of::<QualifiedRuntimeRevocationProjectionColumnsV1>() + 8192,
        MemoryClass::Ordinary,
    )?;
    let value = recover_runtime_revocation_body_v1(qualification.body(), read, scan, work)?;
    read.check()?;
    if let Some(error) = (read.take_state_error)() {
        return Err(error);
    }
    Ok(QualifiedRuntimeRevocationProjectionColumnsV1 {
        value,
        body: qualification.body().clone(),
        roots: roots.digest().map_err(|_| invalid())?,
        memory: read.values.memory.clone(),
        _fee: fee,
    })
}
