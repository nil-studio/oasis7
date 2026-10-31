//! Qualified-parent creation into a fresh isolated relative record tree.
use super::*;
use crate::{
    incremental_field_overlay::{WorldFieldOverlayContextV1, WorldFieldOverlayErrorV1, lookup},
    incremental_inventory_value_read::InventoryValueReader,
};
use oasis7_formal_protocol::{
    incremental_execution::{ExecutionFence, PreparedGuard},
    incremental_state::{StateDeltaV1, StatePageWriter, StateWriteV1},
};
pub struct OrganizationEmptyRootCreateV1<'a> {
    pub owner: OrganizationRootKeyV1<'a>,
    pub source: super::super::WorldRuleOrganizationScalarsRefV1<'a>,
    pub parent: &'a oasis7_proto::memory_budget::ReservedValue<OrganizationFamilyQualificationV1>,
    pub max_pending_node_bytes: usize,
}
struct FreshTree;
impl MerkleNodeReader for FreshTree {
    type Node = Vec<u8>;
    fn read_node(&self, _: &str, _: usize) -> Result<Option<Self::Node>, String> {
        Err("fresh organization creation attempted an old-node read".into())
    }
}
/// Guard, exact qualified parent and empty borrowed source precede all writes.
/// This creates a new tree from the empty sentinel; legacy flat descendants
/// can never be adopted. Only the outer original key is written to the Tx.
pub fn create_qualified_organization_empty_v1<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    W: StatePageWriter,
    CW: FnMut() -> bool,
>(
    change: OrganizationEmptyRootCreateV1<'_>,
    context: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    write: &mut MemberRecordWriteContext<'_, W, CW>,
    guard: &PreparedGuard,
    live: &ExecutionFence,
    cells: &mut u64,
) -> Result<
    oasis7_proto::memory_budget::ReservedValue<OrganizationRecordQualificationV1>,
    WorldFieldOverlayErrorV1,
> {
    let cancel = &mut context.cancelled;
    let error = &mut context.take_state_error;
    context.transaction.contribute(|tx| {
        tx.validate_guard(guard, live)?;
        change
            .parent
            .validate_binding(&tx.parent_fence().parent_roots)?;
        if !matches!(
            (change.owner, change.source),
            (
                OrganizationRootKeyV1::Case(_),
                super::super::WorldRuleOrganizationScalarsRefV1::Case(_)
            ) | (
                OrganizationRootKeyV1::Estate(_),
                super::super::WorldRuleOrganizationScalarsRefV1::Estate(_)
            )
        ) || !change.source.empty()
        {
            return Err(ValueTreeError::Invalid("organization qualified empty source").into());
        }
        if context.values.class != MemoryClass::Ordinary
            || !tx.uses_budget(context.values.memory)
            || !tx.uses_budget(write.memory)
            || !change.parent.uses_budget(context.values.memory)
        {
            return Err(ValueTreeError::Invalid("organization create pool/class").into());
        }
        let key = {
            let mut combined = || cancel() || (write.cancelled)();
            let mut records = InventoryValueReader {
                values: &mut context.values,
                key_work: context.key_work,
                semantic_bytes: context.semantic_bytes,
                record_bytes: context.record_bytes,
                cancelled: &mut combined,
            };
            let (_, present) =
                crate::incremental_world_rule_parent::read_parent_overlay(tx, &mut records, error)?;
            if !present {
                return Err(ValueTreeError::Invalid("organization create requires rules").into());
            }
            let key = change
                .owner
                .outer()
                .overlay_path(RecordPathV1::Record, &mut records)?;
            if lookup(tx, &key.0, &mut records, error)?.is_some() {
                return Err(ValueTreeError::Invalid(
                    "organization create requires absent outer record",
                )
                .into());
            }
            key.0
        };
        if cancel() || (write.cancelled)() {
            return Err(ValueTreeError::Cancelled.into());
        }
        let _rows = write
            .memory
            .reserve(16 << 10, MemoryClass::Ordinary)
            .map_err(ValueTreeError::Memory)?;
        let mut mutations = Vec::with_capacity(17);
        {
            let mut combined = MemberRecordWriteContext {
                sink: &mut *write.sink,
                memory: write.memory,
                io: &mut *write.io,
                key_work: &mut *write.key_work,
                limits: write.limits,
                cancelled: || cancel() || (write.cancelled)(),
            };
            prepare_organization_relative_body_v1(
                change.source,
                &mut combined,
                cells,
                &mut |row| {
                    if mutations.len() == 17 {
                        return Err(ValueTreeError::Invalid(
                            "organization empty body cardinality",
                        ));
                    }
                    mutations.push(StateWriteV1 {
                        key: row.mutation.key.clone(),
                        before: None,
                        after: row.mutation.after,
                    });
                    Ok(())
                },
            )?;
        }
        mutations.sort_by(|a, b| a.key.cmp(&b.key));
        let _tree = inner_tree::workspace(write.memory, mutations.len())?;
        let empty = StateRootV1::empty(StateNamespaceV1::Authorization)
            .map_err(|_| ValueTreeError::Invalid("organization empty tree"))?;
        let delta = _tree.retain(
            StateDeltaV1::build(empty, mutations, &FreshTree, change.max_pending_node_bytes)
                .map_err(|message| {
                    if message.starts_with("ResourceLimited:") {
                        ValueTreeError::ResourceLimited("organization create tree")
                    } else {
                        ValueTreeError::Invalid("organization create tree")
                    }
                })?,
        );
        if cancel() || (write.cancelled)() {
            return Err(ValueTreeError::Cancelled.into());
        }
        let reference = {
            let mut combined = MemberRecordWriteContext {
                sink: &mut *write.sink,
                memory: write.memory,
                io: &mut *write.io,
                key_work: &mut *write.key_work,
                limits: write.limits,
                cancelled: || cancel() || (write.cancelled)(),
            };
            inner_tree::persist(&delta, &mut combined)?;
            write_organization_root_reference_v1(change.owner, &delta.after, &mut combined)?
        };
        if reference.field_key != key {
            return Err(ValueTreeError::Invalid("organization create outer identity").into());
        }
        if cancel() || (write.cancelled)() {
            return Err(ValueTreeError::Cancelled.into());
        }
        tx.write(
            StateNamespaceV1::Authorization,
            key.as_bytes(),
            Some(reference.value_root),
        )?;
        let digest = WorldFieldKeyV1::from_bytes(key.as_bytes())
            .map_err(ValueTreeError::Invalid)?
            .semantic_key
            .ok_or(ValueTreeError::Invalid("organization owner digest"))?;
        let proof = closure::from_typed(change.owner, digest, &delta.after, write.memory)?;
        if cancel() || (write.cancelled)() {
            return Err(ValueTreeError::Cancelled.into());
        }
        Ok(proof)
    })
}
