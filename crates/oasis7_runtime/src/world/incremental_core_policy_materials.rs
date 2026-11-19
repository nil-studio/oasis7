//! Concrete CorePolicy material owner context. Parent discovery is GEN/cold;
//! each event consumes only its genuine Current pending projection.
use super::super::incremental_body::{BodyMutationContext, BodyMutationError};
use super::super::incremental_material_normalization::{
    MaterialNormalizationErrorV1, MaterialNormalizationWorkV1, normalize_current_materials_v1,
};
use oasis7_formal_protocol::{
    incremental_execution::{
        ExecutionError, ExecutionFence, ExecutionTransaction, PreparedGuard, RootSetV1,
    },
    incremental_state::MerkleNodeReader,
    incremental_value::{
        ValueIoBudget, ValueNodeReader, ValueNodeWriter, ValueReadContext, ValueTreeError,
    },
};
use oasis7_proto::memory_budget::{MemoryClass, ReservedValue};
use oasis7_world_state::{
    incremental_field_member::{MemberRecordError, MemberRecordLimitsV1, MemberRecordWriteContext},
    incremental_field_overlay::{WorldFieldOverlayContextV1, WorldFieldOverlayErrorV1},
    incremental_inventory::{
        InventoryFamilyWorkV1, InventoryMaterialScopeWorkV1, InventoryWorldGroupWorkV1,
        ParentInventoryMaterialScopeV1, open_current_inventory_material_scope_v1,
    },
    incremental_inventory_change::InventoryChangeError,
    incremental_inventory_overlay_change::InventoryOverlayChangeError,
    incremental_inventory_transaction::InventoryContributionError,
    incremental_semantic_key::{SemanticKeyBudget, SemanticKeyError},
};
use std::cell::RefCell;

/// Every field is mandatory. A caller cannot substitute a map, empty flag or
/// naked root for the private-constructor authenticated parent scope. The same
/// counters are borrowed for the complete step and are never reset per event.
pub struct CorePolicyMaterialContextV1<'a, M, E> {
    pub parent: &'a ReservedValue<ParentInventoryMaterialScopeV1>,
    pub roots: &'a RootSetV1,
    pub pages: &'a M,
    pub read_io: &'a mut ValueIoBudget,
    pub read_keys: &'a mut SemanticKeyBudget,
    pub write_keys: &'a mut SemanticKeyBudget,
    pub semantic_bytes: u64,
    pub record_bytes: u64,
    pub write_limits: MemberRecordLimitsV1,
    pub take_state_error: E,
    pub family_work: &'a mut InventoryFamilyWorkV1,
    pub scope_work: &'a mut InventoryMaterialScopeWorkV1,
    pub group_work: &'a mut InventoryWorldGroupWorkV1,
    pub normalization_max_union_entries: usize,
    pub normalization_cells_remaining: &'a mut u64,
    pub normalization_comparison_bytes_remaining: &'a mut u64,
}
impl<M: MerkleNodeReader, E: FnMut() -> Option<ValueTreeError>>
    CorePolicyMaterialContextV1<'_, M, E>
{
    /// Contribute the original two-map normalization before the CorePolicy
    /// event owner publishes. This performs no event/allocator/mailbox action.
    pub(in crate::world) fn contribute<
        V: ValueNodeReader,
        W: ValueNodeWriter,
        C: FnMut() -> bool,
    >(
        &mut self,
        tx: &mut ExecutionTransaction<'_, M>,
        values: &V,
        mutation: &mut BodyMutationContext<'_, W, C>,
        guard: &PreparedGuard,
        live: &ExecutionFence,
    ) -> Result<(), BodyMutationError> {
        let result = (|| {
            tx.validate_guard(guard, live)?;
            if !tx.uses_budget(mutation.memory) || !self.parent.uses_budget(mutation.memory) {
                return Err(BodyMutationError::Invalid(
                    "CorePolicy material context pool",
                ));
            }
            if (mutation.cancelled)() {
                return Err(ValueTreeError::Cancelled.into());
            }
            // Cover the bounded RootSet binding encoding and local context
            // metadata independently of the real scope's retained allocations.
            let _phase = mutation
                .memory
                .reserve(32 << 10, MemoryClass::Ordinary)
                .map_err(ExecutionError::Memory)?;
            let stop = RefCell::new(&mut mutation.cancelled);
            let mut overlay = WorldFieldOverlayContextV1 {
                transaction: &mut *tx,
                values: ValueReadContext {
                    source: values,
                    memory: mutation.memory,
                    class: MemoryClass::Ordinary,
                    io: &mut *self.read_io,
                },
                key_work: &mut *self.read_keys,
                semantic_bytes: self.semantic_bytes,
                record_bytes: self.record_bytes,
                take_state_error: &mut self.take_state_error,
                cancelled: || (stop.borrow_mut())(),
            };
            let source = open_current_inventory_material_scope_v1(
                self.parent,
                self.roots,
                self.pages,
                &mut overlay,
                self.family_work,
                self.scope_work,
                guard,
                live,
            )
            .map_err(overlay_error)?;
            // The scope opening performs real transaction reads. Use its new
            // exact snapshot, never the caller's now-old read-set revision.
            let guard = overlay.transaction.guard();
            let mut write = MemberRecordWriteContext {
                sink: &mut *mutation.sink,
                memory: mutation.memory,
                io: &mut *mutation.io,
                key_work: &mut *self.write_keys,
                limits: self.write_limits,
                cancelled: || (stop.borrow_mut())(),
            };
            normalize_current_materials_v1(
                &source,
                &mut overlay,
                &mut write,
                self.group_work,
                &mut MaterialNormalizationWorkV1 {
                    max_union_entries: self.normalization_max_union_entries,
                    cells_remaining: &mut *self.normalization_cells_remaining,
                    comparison_bytes_remaining: &mut *self.normalization_comparison_bytes_remaining,
                },
                &guard,
                live,
            )
            .map_err(normalization_error)?;
            if (stop.borrow_mut())() {
                return Err(ValueTreeError::Cancelled.into());
            }
            Ok(())
        })();
        if result.is_err() {
            let _: Result<(), ExecutionError> = tx.contribute(|_| Err(ExecutionError::Aborted));
        }
        result
    }
}
fn member_error(error: MemberRecordError) -> BodyMutationError {
    match error {
        MemberRecordError::Value(error) => BodyMutationError::Value(error),
        MemberRecordError::Invalid(reason) => BodyMutationError::Invalid(reason),
        MemberRecordError::Key(error) => match error {
            SemanticKeyError::Memory(error) => {
                BodyMutationError::Value(ValueTreeError::Memory(error))
            }
            SemanticKeyError::ResourceLimited => BodyMutationError::Value(
                ValueTreeError::ResourceLimited("semantic key byte allowance"),
            ),
            SemanticKeyError::Cancelled => BodyMutationError::Value(ValueTreeError::Cancelled),
            SemanticKeyError::Invalid(reason) => BodyMutationError::Invalid(reason),
        },
        MemberRecordError::Io(error) => BodyMutationError::Original(Box::new(error)),
    }
}
fn overlay_error(error: WorldFieldOverlayErrorV1) -> BodyMutationError {
    match error {
        WorldFieldOverlayErrorV1::Execution(error) => BodyMutationError::Execution(error),
        WorldFieldOverlayErrorV1::Value(error) => BodyMutationError::Value(error),
        WorldFieldOverlayErrorV1::Member(error) => member_error(error),
    }
}
fn normalization_error(error: MaterialNormalizationErrorV1) -> BodyMutationError {
    match error {
        MaterialNormalizationErrorV1::Execution(error) => BodyMutationError::Execution(error),
        MaterialNormalizationErrorV1::Value(error) => BodyMutationError::Value(error),
        MaterialNormalizationErrorV1::Overlay(error) => overlay_error(error),
        MaterialNormalizationErrorV1::Inventory(error) => match error {
            InventoryOverlayChangeError::Execution(error) => BodyMutationError::Execution(error),
            InventoryOverlayChangeError::Read(error) => overlay_error(error),
            InventoryOverlayChangeError::Change(error) => match error {
                InventoryChangeError::Precondition => {
                    BodyMutationError::Invalid("inventory precondition")
                }
                InventoryChangeError::Invalid(reason) => BodyMutationError::Invalid(reason),
                InventoryChangeError::Value(error) => BodyMutationError::Value(error),
                InventoryChangeError::Record(error) => member_error(error),
            },
            InventoryOverlayChangeError::Contribution(error) => match error {
                InventoryContributionError::Execution(error) => BodyMutationError::Execution(error),
                InventoryContributionError::Invalid(reason) => BodyMutationError::Invalid(reason),
                InventoryContributionError::DependencyConflict => {
                    BodyMutationError::Invalid("inventory transaction dependency conflict")
                }
                InventoryContributionError::Cancelled => {
                    BodyMutationError::Value(ValueTreeError::Cancelled)
                }
            },
        },
    }
}

#[cfg(any(test, feature = "test-support"))]
#[path = "incremental_core_policy_materials_test_support.rs"]
pub(in crate::world) mod test_support;
