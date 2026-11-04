//! Closed original receipt/order mutations within one guarded shared transaction.
use super::edits::{OwnerEditV1, PreparedEdit};
use super::*;
use crate::{
    incremental_field_overlay::{WorldFieldOverlayContextV1, WorldFieldOverlayErrorV1},
    incremental_world_rule_composite_family::{
        CompositeBranchCodecV1, WorldRuleCompositeQualificationV1,
    },
    incremental_world_rule_composite_overlay::{
        CompositePathPatchV1, WorldRuleCompositeOverlayV1, patch_world_rule_branch_v1,
    },
};
use oasis7_formal_protocol::incremental_execution::{ExecutionFence, PreparedGuard, RootSetV1};
use oasis7_proto::memory_budget::MemoryClass;
use std::collections::BTreeMap;
pub type WorldRuleRequestsOverlayV1 =
    WorldRuleCompositeOverlayV1<BTreeMap<String, crate::world_rules::WorldRuleReceiptV1>>;
pub type AgentOrdersOverlayV1 =
    WorldRuleCompositeOverlayV1<BTreeMap<String, crate::world_rules::AgentIndustrialOrderV1>>;
fn invalid() -> ValueTreeError {
    ValueTreeError::Invalid("closed receipt order owner batch")
}
fn batch<
    Edit: OwnerEditV1,
    M: MerkleNodeReader,
    S: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    W: ValueNodeReader + StatePageWriter,
    CW: FnMut() -> bool,
>(
    parent: &ReservedValue<WorldRuleCompositeQualificationV1<Edit::Branch>>,
    roots: &RootSetV1,
    current: Option<&ReservedValue<WorldRuleCompositeOverlayV1<Edit::Branch>>>,
    changes: &[Edit],
    state: &S,
    context: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    write: &mut MemberRecordWriteContext<'_, W, CW>,
    scan: &mut StateScanBudget,
    read_work: &mut WorldRuleCompositeReadWorkV1,
    write_work: &mut WorldRuleCompositeWriteWorkV1,
    guard: &PreparedGuard,
    live: &ExecutionFence,
) -> Result<
    Option<ReservedValue<WorldRuleCompositeOverlayV1<Edit::Branch>>>,
    WorldFieldOverlayErrorV1,
> {
    context.transaction.contribute(|tx| {
        tx.validate_guard(guard, live)?;
        parent.check_budget(write.memory, MemoryClass::Ordinary)?;
        parent.validate_binding(roots)?;
        if tx.parent_fence().parent_roots != roots.digest().map_err(|_| invalid())?
            || !tx.uses_budget(write.memory)
            || !context.values.memory.same_pool(write.memory)
            || context.values.class != MemoryClass::Ordinary
        {
            return Err(invalid().into());
        }
        if u64::try_from(changes.len()).map_err(|_| invalid())? > write_work.cells_remaining {
            return Err(ValueTreeError::ResourceLimited("owner batch cells").into());
        }
        if (context.cancelled)() || (write.cancelled)() {
            return Err(ValueTreeError::Cancelled.into());
        }
        if let Some(error) = (context.take_state_error)() {
            return Err(error.into());
        }
        // Covers prepared enums, all three per-edit key carriers, Vec headers,
        // reference slices and allocator rounding before any metadata allocation.
        let bytes = changes
            .len()
            .checked_mul(2048)
            .and_then(|n| n.checked_add(4096))
            .ok_or_else(invalid)?;
        let _metadata = write
            .memory
            .reserve(bytes, MemoryClass::Ordinary)
            .map_err(ValueTreeError::Memory)?;
        let profile = <Edit::Branch as CompositeBranchCodecV1>::BRANCH.profile();
        let mut prepared = Vec::<PreparedEdit<'_>>::with_capacity(changes.len());
        {
            let mut combined = || (context.cancelled)() || (write.cancelled)();
            let mut output = MemberRecordWriteContext {
                sink: &mut *write.sink,
                memory: write.memory,
                io: &mut *write.io,
                key_work: &mut *write.key_work,
                limits: write.limits,
                cancelled: &mut combined,
            };
            for change in changes {
                if (output.cancelled)() {
                    return Err(ValueTreeError::Cancelled.into());
                }
                prepared.push(change.prepare(&profile, &mut output, write_work)?);
            }
        }
        let mut values = Vec::with_capacity(prepared.len());
        for item in &mut prepared {
            values.push(item.after.take());
        }
        let mut paths = Vec::<Vec<&[u8]>>::with_capacity(prepared.len());
        for item in &prepared {
            if item.keys.len() > 3 {
                return Err(invalid().into());
            }
            let mut path = Vec::with_capacity(item.keys.len());
            for key in &item.keys {
                path.push(key.as_slice());
            }
            paths.push(path);
        }
        let actual = prepared
            .capacity()
            .checked_mul(std::mem::size_of::<PreparedEdit<'_>>())
            .and_then(|n| n.checked_add(paths.capacity() * std::mem::size_of::<Vec<&[u8]>>()))
            .and_then(|n| {
                prepared.iter().try_fold(n, |n, p| {
                    n.checked_add(p.keys.capacity() * std::mem::size_of::<ReservedValue<Vec<u8>>>())
                })
            })
            .and_then(|n| {
                paths.iter().try_fold(n, |n, p| {
                    n.checked_add(p.capacity() * std::mem::size_of::<&[u8]>())
                })
            })
            .and_then(|n| {
                n.checked_add(values.capacity() * std::mem::size_of::<Option<Encoded<'_>>>())
            })
            .ok_or_else(invalid)?;
        if actual > bytes {
            return Err(invalid().into());
        }
        let changes = prepared
            .iter()
            .zip(paths.iter())
            .zip(values)
            .map(|((p, path), after)| CompositePathPatchV1 {
                path,
                expected: p.expected,
                after,
            });
        let mut local = WorldFieldOverlayContextV1 {
            transaction: tx,
            values: oasis7_formal_protocol::incremental_value::ValueReadContext {
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
        patch_world_rule_branch_v1(
            parent, roots, current, changes, state, &mut local, write, scan, read_work, write_work,
            guard, live,
        )
    })
}
macro_rules! owner {
    ($name:ident,$edit:ident,$record:ty) => {
        /// Exact old leaf IDs, original map keys and complete container counts
        /// are checked in one batch. Errors poison any successful shared prefix.
        /// This supplies storage typing; business authority remains with caller.
        pub fn $name<
            M: MerkleNodeReader,
            S: MerkleNodeReader,
            R: ValueNodeReader,
            E: FnMut() -> Option<ValueTreeError>,
            C: FnMut() -> bool,
            W: ValueNodeReader + StatePageWriter,
            CW: FnMut() -> bool,
        >(
            parent: &ReservedValue<WorldRuleCompositeQualificationV1<BTreeMap<String, $record>>>,
            roots: &RootSetV1,
            current: Option<&ReservedValue<WorldRuleCompositeOverlayV1<BTreeMap<String, $record>>>>,
            changes: &[$edit<'_>],
            state: &S,
            context: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
            write: &mut MemberRecordWriteContext<'_, W, CW>,
            scan: &mut StateScanBudget,
            read_work: &mut WorldRuleCompositeReadWorkV1,
            write_work: &mut WorldRuleCompositeWriteWorkV1,
            guard: &PreparedGuard,
            live: &ExecutionFence,
        ) -> Result<
            Option<ReservedValue<WorldRuleCompositeOverlayV1<BTreeMap<String, $record>>>>,
            WorldFieldOverlayErrorV1,
        > {
            batch(
                parent, roots, current, changes, state, context, write, scan, read_work,
                write_work, guard, live,
            )
        }
    };
}
owner!(
    patch_world_rule_requests_v1,
    WorldRuleRequestEditV1,
    crate::world_rules::WorldRuleReceiptV1
);
owner!(
    patch_world_rule_agent_orders_v1,
    AgentOrderEditV1,
    crate::world_rules::AgentIndustrialOrderV1
);
