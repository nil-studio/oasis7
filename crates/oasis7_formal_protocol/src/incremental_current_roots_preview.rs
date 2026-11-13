//! Paid same-revision Current roots. This is DATA, not a transition/seal,
//! final-fact result, accepted head, durable receipt or execution permission.
use super::*;
use crate::controlled_authority::formal::{
    FormalError,
    bounded_context::{FormalBoundedContextV1, WorkIdentityV1},
};
use std::{cell::RefCell, fmt};
#[derive(Debug)]
pub enum CurrentRootsPreviewErrorV1 {
    Execution(ExecutionError),
    Common(FormalError),
}
impl From<ExecutionError> for CurrentRootsPreviewErrorV1 {
    fn from(e: ExecutionError) -> Self {
        Self::Execution(e)
    }
}
impl From<FormalError> for CurrentRootsPreviewErrorV1 {
    fn from(e: FormalError) -> Self {
        Self::Common(e)
    }
}
impl fmt::Display for CurrentRootsPreviewErrorV1 {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Execution(e) => write!(f, "{e}"),
            Self::Common(e) => write!(f, "{e:?}"),
        }
    }
}
impl std::error::Error for CurrentRootsPreviewErrorV1 {}
/// No constructor, Clone, detached roots or guard-refresh API. Every consumer
/// rechecks the actual transaction revision and continuing Common identity.
pub struct CurrentRootsPreviewV1 {
    roots: RootSetV1,
    guard: PreparedGuard,
    core: WorkIdentityV1,
    memory: MemoryBudget,
}
impl CurrentRootsPreviewV1 {
    pub fn roots(&self) -> &RootSetV1 {
        &self.roots
    }
    pub fn validate<R: MerkleNodeReader>(
        &self,
        tx: &ExecutionTransaction<'_, R>,
        common: &mut FormalBoundedContextV1<'_>,
        live: &ExecutionFence,
    ) -> Result<(), CurrentRootsPreviewErrorV1> {
        common.charge(1, 128)?;
        tx.validate_guard(&self.guard, live)?;
        if self.core != common.work().identity()
            || !self.memory.same_pool(common.memory())
            || !tx.uses_budget(common.memory())
        {
            return Err(ExecutionError::Invalid("Current roots preview Core/pool").into());
        }
        Ok(())
    }
}
impl<R: MerkleNodeReader> ExecutionTransaction<'_, R> {
    /// Reuse the exact seal root-building kernel at the actual Current tick.
    /// Height is the original seal's parent+1 convention, never a future grant.
    /// No final-fact capacity/producer or transition predicate runs for DATA.
    pub fn preview_current_roots_v1(
        &mut self,
        now: u64,
        common: &mut FormalBoundedContextV1<'_>,
        live: &ExecutionFence,
    ) -> Result<ReservedValue<CurrentRootsPreviewV1>, CurrentRootsPreviewErrorV1> {
        self.contribute(|tx| {
            common.charge(1, 128)?;
            tx.validate_guard(&tx.guard(), live)?;
            if !tx.memory.same_pool(common.memory()) {
                return Err(ExecutionError::Invalid("Current roots preview Common pool").into());
            }
            validate_source_budget(tx.source, tx.memory, MemoryClass::Ordinary)?;
            if now < tx.parent.roots.simulation_tick {
                return Err(ExecutionError::Invalid("tick regression").into());
            }
            let height = tx
                .parent
                .roots
                .height
                .checked_add(1)
                .ok_or(ExecutionError::Invalid("height overflow"))?;
            let mut workspace = tx
                .memory
                .reserve(
                    transition::phase_bound(tx.dirty, tx.reads, 0)?,
                    MemoryClass::Ordinary,
                )
                .map_err(ExecutionError::Memory)?;
            let (built, first) = {
                let context = RefCell::new(&mut *common);
                let first = RefCell::new(None);
                let meter = |bytes: usize| -> Result<(), String> {
                    // A stopped traversal keeps its original typed cause and admits no more work.
                    if first.borrow().is_some() {
                        return Err("Current preview Common stopped".into());
                    }
                    let bytes = match u64::try_from(bytes) {
                        Ok(n) => n,
                        Err(_) => {
                            *first.borrow_mut() = Some(CurrentRootsPreviewErrorV1::Execution(
                                ExecutionError::ResourceLimited("Current preview work width"),
                            ));
                            return Err("Current preview work width".into());
                        }
                    };
                    if let Err(e) = context.borrow_mut().charge(1, bytes) {
                        *first.borrow_mut() = Some(CurrentRootsPreviewErrorV1::Common(e));
                        return Err("Current preview Common stopped".into());
                    }
                    Ok(())
                };
                let built = tx.build_current_delta_v1(now, height, &mut || false, Some(&meter));
                (built, first.into_inner())
            };
            if let Some(error) = first {
                return Err(error);
            }
            let CurrentDeltaV1 {
                after,
                writes: _,
                new_nodes: _,
            } = built.map_err(|e| match e {
                DeltaBuildError::Execution(e) => CurrentRootsPreviewErrorV1::Execution(e),
                DeltaBuildError::Cancelled => {
                    CurrentRootsPreviewErrorV1::Execution(ExecutionError::Aborted)
                }
            })?;
            let metadata = after.state.iter().try_fold(after.world_id.len(), |n, r| {
                n.checked_add(r.node_root.len())
                    .and_then(|n| n.checked_add(128))
                    .ok_or(ExecutionError::ResourceLimited(
                        "Current preview roots metadata",
                    ))
            })?;
            common.charge(1, metadata as u64)?;
            after.validate()?;
            let bytes = after.state.iter().try_fold(
                std::mem::size_of::<CurrentRootsPreviewV1>()
                    + std::mem::size_of::<MemoryReservation>()
                    + after.world_id.capacity(),
                |n, r| {
                    n.checked_add(r.node_root.capacity())
                        .ok_or(ExecutionError::ResourceLimited(
                            "Current preview retained layout",
                        ))
                },
            )?;
            let fee = workspace
                .split(bytes)
                .map_err(|_| ExecutionError::Invalid("Current preview exceeded workspace"))?;
            common.consume_steps(0)?;
            let guard = tx.guard();
            tx.validate_guard(&guard, live)?;
            Ok(fee.retain(CurrentRootsPreviewV1 {
                roots: after,
                guard,
                core: common.work().identity(),
                memory: tx.memory.clone(),
            }))
        })
    }
}
#[cfg(test)]
#[path = "incremental_current_roots_preview_tests.rs"]
mod tests;
