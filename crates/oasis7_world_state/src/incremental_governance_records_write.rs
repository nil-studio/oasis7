//! Publish one completely prepared governance child; every sink result is provisional.
use super::*;
use crate::incremental_field_overlay::WorldFieldOverlayErrorV1;
use oasis7_formal_protocol::{
    incremental_execution::ExecutionTransaction,
    incremental_state::{MerkleNodeReader, StatePageWriter, StateRootV1},
};
use oasis7_proto::memory_budget::{MemoryClass, ReservedValue};
fn invalid() -> ValueTreeError {
    ValueTreeError::Invalid("governance child publication")
}
pub(super) fn publish<
    M: MerkleNodeReader,
    W: StatePageWriter,
    CW: FnMut() -> bool,
    C: FnMut() -> bool,
>(
    key: GovernanceRecordKeyV1<'_>,
    child: StateRootV1,
    outer: WorldFieldKeyV1,
    tx: &mut ExecutionTransaction<'_, M>,
    write: &mut MemberRecordWriteContext<'_, W, CW>,
    cancel: &mut C,
) -> Result<ReservedValue<GovernanceRecordQualificationV1>, WorldFieldOverlayErrorV1> {
    let row = {
        let mut combined = MemberRecordWriteContext {
            sink: &mut *write.sink,
            memory: write.memory,
            io: &mut *write.io,
            key_work: &mut *write.key_work,
            limits: write.limits,
            cancelled: || cancel() || (write.cancelled)(),
        };
        genesis::write_root(key, child.clone(), &mut combined)?
    };
    if row.field_key != outer {
        return Err(invalid().into());
    }
    if cancel() || (write.cancelled)() {
        return Err(ValueTreeError::Cancelled.into());
    }
    tx.write(key.kind.namespace(), outer.as_bytes(), Some(row.value_root))?;
    let owner = WorldFieldKeyV1::from_bytes(outer.as_bytes())
        .map_err(ValueTreeError::Invalid)?
        .semantic_key
        .ok_or_else(invalid)?;
    let proof = closure::mint(key.kind, owner, child, write.memory, MemoryClass::Ordinary)?;
    if cancel() || (write.cancelled)() {
        return Err(ValueTreeError::Cancelled.into());
    }
    Ok(proof)
}
