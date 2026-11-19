//! Current typed branch observations derived from the actual transaction.
//! A branch pointer alone is never a proof of its closed typed contents.
use super::*;
use crate::incremental_inventory_value_read::InventoryValueReader;
use oasis7_formal_protocol::incremental_execution::{ExecutionError, ExecutionTransaction};
use oasis7_formal_protocol::incremental_value::ValueReadContext;

/// Shared by observation and mutation: authenticate the actual parent marker,
/// semantic outer key, record-root schema and branch namespace in that order.
pub(super) fn read_current_branch<T, M, R, E, C>(
    tx: &mut ExecutionTransaction<'_, M>,
    records: &mut InventoryValueReader<'_, '_, R, C>,
    take_error: &mut E,
) -> Result<(WorldFieldKeyV1, u64, ReservedValue<StateRootV1>), WorldFieldOverlayErrorV1>
where
    T: CompositeBranchCodecV1,
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
{
    let (field, present) =
        crate::incremental_world_rule_parent::read_parent_overlay(tx, records, take_error)?;
    if !present {
        return Err(invalid().into());
    }
    let (outer, semantic_bytes) = records.key(
        field,
        WorldFieldRecordKindV1::Entry,
        &(T::BRANCH.name(), ()),
    )?;
    let id = lookup(tx, &outer, records, take_error)?.ok_or_else(invalid)?;
    let body = read_body::<T, _, _>(&outer, semantic_bytes, &id, records)?;
    Ok((outer, semantic_bytes, body))
}

/// Observe a fixed branch's actual Current closure against its genuine parent.
/// The sealed T implementations choose the schema and key; consumers cannot
/// substitute a path, root or decoder. No business mutation is granted here.
#[allow(clippy::too_many_arguments)]
pub(crate) fn bind_current_world_rule_branch_v1<T, M, S, R, E, C>(
    parent: &ReservedValue<WorldRuleCompositeQualificationV1<T>>,
    roots: &RootSetV1,
    state: &S,
    context: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    scan: &mut StateScanBudget,
    work: &mut TypedRecordReadWorkV1,
    guard: &PreparedGuard,
    live: &ExecutionFence,
) -> Result<ReservedValue<WorldRuleCompositeOverlayV1<T>>, WorldFieldOverlayErrorV1>
where
    T: CompositeBranchCodecV1,
    M: MerkleNodeReader,
    S: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
{
    let result = (|| {
        context.transaction.validate_guard(guard, live)?;
        parent.check_budget(context.values.memory, MemoryClass::Ordinary)?;
        if !parent.uses_budget(context.values.memory)
            || parent.reserved_bytes() < 512
            || !context.transaction.uses_budget(context.values.memory)
            || context.values.class != MemoryClass::Ordinary
        {
            return Err(invalid().into());
        }
        let mut phase = context
            .values
            .memory
            .reserve(128 << 10, MemoryClass::Ordinary)
            .map_err(ValueTreeError::Memory)?;
        // Validate fixed RootSet limits before computing its bounded digest.
        roots.validate().map_err(|_| invalid())?;
        parent.validate_binding(roots)?;
        let digest = roots.digest().map_err(|_| invalid())?;
        if context.transaction.parent_fence().parent_roots != digest {
            return Err(invalid().into());
        }
        let body = {
            let mut records = InventoryValueReader {
                values: &mut context.values,
                key_work: &mut *context.key_work,
                semantic_bytes: context.semantic_bytes,
                record_bytes: context.record_bytes,
                cancelled: &mut context.cancelled,
            };
            let (_, _, body) = read_current_branch::<T, _, _, _, _>(
                context.transaction,
                &mut records,
                &mut context.take_state_error,
            )?;
            body
        };
        {
            let mut read = WorldFieldReadContextV1 {
                root: &body,
                state,
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
                cancelled: &mut context.cancelled,
            };
            crate::incremental_world_rule_composite_family::check(&mut read)?;
            let profile = T::BRANCH.profile();
            let trees_remaining = work.max_retained_trees;
            // An absent parent plus a newly-created Current branch validates
            // the entire insertion. Existing parents validate only changes.
            accept_body::<T, _, _, _, _>(
                &profile,
                parent.body(),
                Some(&body),
                &mut ChangeDecoder {
                    ctx: &mut read,
                    scan,
                    work,
                    profile,
                    trees_remaining,
                    tree_depth: 0,
                },
            )?;
            crate::incremental_world_rule_composite_family::check(&mut read)?;
        }
        Ok(phase
            .split(512)
            .map_err(ValueTreeError::Invalid)?
            .retain(WorldRuleCompositeOverlayV1 {
                parent: digest,
                body,
                guard: context.transaction.guard(),
                class: MemoryClass::Ordinary,
                _type: PhantomData,
            }))
    })();
    if result.is_err() {
        let _: Result<(), ExecutionError> = context
            .transaction
            .contribute(|_| Err(ExecutionError::Aborted));
    }
    result
}
