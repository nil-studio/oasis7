//! Terminal Current identities bound only to the actual sealed after roots.
//! This is structural DATA; durable acceptance and host installation are later.
use super::*;
use crate::incremental_field_overlay::{WorldFieldOverlayContextV1, WorldFieldOverlayErrorV1};
use oasis7_formal_protocol::incremental_execution::{
    ExecutionError, ExecutionFence, PreparedGuard, SealedExecutionBindingV1,
};
pub struct FrozenInventoryIdentityDirectoryV1 {
    tree: Option<tree::NodeRef>,
    guard: PreparedGuard,
    memory: MemoryBudget,
}
impl FrozenInventoryIdentityDirectoryV1 {
    pub fn validate_guard<M: MerkleNodeReader>(
        &self,
        tx: &oasis7_formal_protocol::incremental_execution::ExecutionTransaction<'_, M>,
        memory: &MemoryBudget,
        live: &ExecutionFence,
    ) -> Result<(), WorldFieldOverlayErrorV1> {
        tx.validate_guard(&self.guard, live)?;
        if !self.memory.same_pool(memory)
            || !tx.uses_budget(memory)
            || self.tree.as_ref().is_some_and(|n| !n.uses_budget(memory))
        {
            return Err(invalid().into());
        }
        Ok(())
    }
    pub fn validate_sealed(
        &self,
        binding: &SealedExecutionBindingV1,
        roots: &RootSetV1,
        memory: &MemoryBudget,
    ) -> Result<(), WorldFieldOverlayErrorV1> {
        if !self.memory.same_pool(memory)
            || self.tree.as_ref().is_some_and(|n| !n.uses_budget(memory))
        {
            return Err(invalid().into());
        }
        binding.validate_source_guard(&self.guard, memory)?;
        binding.validate_after_roots(roots, memory)?;
        Ok(())
    }
    pub fn rebind_after_seal_v1(
        &self,
        binding: &SealedExecutionBindingV1,
        roots: &RootSetV1,
        memory: &MemoryBudget,
        work: &mut InventoryIdentityDirectoryWorkV1,
        mut cancelled: impl FnMut() -> bool,
    ) -> Result<ReservedValue<InventoryIdentityDirectoryV1>, WorldFieldOverlayErrorV1> {
        // Fixed RootSet shape bounds the original digest/validation temporaries;
        // this does not turn a caller-supplied root into a seal witness.
        if roots.world_id.len() > 256
            || roots.state.len() != 6
            || roots.state.iter().any(|r| r.node_root.len() != 64)
        {
            return Err(invalid().into());
        }
        let wire = roots
            .world_id
            .len()
            .checked_add(6 * oasis7_formal_protocol::incremental_state::STATE_ROOT_BYTES + 256)
            .ok_or_else(invalid)?;
        work.step(wire.checked_mul(4).ok_or_else(invalid)?, &mut cancelled)?;
        let _phase = memory
            .reserve(
                wire.checked_mul(4)
                    .and_then(|n| n.checked_add(4096))
                    .ok_or_else(invalid)?,
                MemoryClass::Ordinary,
            )
            .map_err(ValueTreeError::Memory)?;
        self.validate_sealed(binding, roots, memory)?;
        let fee = memory
            .reserve(
                size_of::<InventoryIdentityDirectoryV1>() + 128,
                MemoryClass::Ordinary,
            )
            .map_err(ValueTreeError::Memory)?;
        work.step(0, &mut cancelled)?;
        // Every immutable AVL node/identity keeps its actual original fee. The
        // after-root owner adds only its O(1) carrier; no seven-map/World copy.
        Ok(fee.retain(InventoryIdentityDirectoryV1 {
            roots: binding.after_roots_digest(),
            tree: self.tree.clone(),
        }))
    }
}
#[allow(clippy::too_many_arguments)]
pub fn freeze_current_inventory_identity_directory_v1<M, R, E, C, P>(
    directory: &ReservedValue<InventoryIdentityDirectoryV1>,
    family: &ReservedValue<InventoryFamilyQualificationV1>,
    roots: &RootSetV1,
    state: &P,
    overlay: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    family_work: &mut InventoryFamilyWorkV1,
    work: &mut InventoryIdentityDirectoryWorkV1,
    guard: &PreparedGuard,
    live: &ExecutionFence,
) -> Result<ReservedValue<FrozenInventoryIdentityDirectoryV1>, WorldFieldOverlayErrorV1>
where
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    P: MerkleNodeReader,
{
    let result = (|| {
        overlay.transaction.validate_guard(guard, live)?;
        let tree = current::fold_with_state(
            directory,
            family,
            roots,
            overlay,
            state,
            &mut || false,
            family_work,
            work,
        )?;
        let memory = overlay.values.memory;
        let fee = memory
            .reserve(
                size_of::<FrozenInventoryIdentityDirectoryV1>() + 128,
                MemoryClass::Ordinary,
            )
            .map_err(ValueTreeError::Memory)?;
        work.step(0, &mut overlay.cancelled)?;
        let guard = overlay.transaction.guard();
        overlay.transaction.validate_guard(&guard, live)?;
        Ok(fee.retain(FrozenInventoryIdentityDirectoryV1 {
            tree,
            guard,
            memory: memory.clone(),
        }))
    })();
    if result.is_err() {
        let _: Result<(), ExecutionError> = overlay
            .transaction
            .contribute(|_| Err(ExecutionError::Aborted));
    }
    result
}
