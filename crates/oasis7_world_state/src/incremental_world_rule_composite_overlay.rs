//! One guarded transaction path for all six fixed rule branches.
use crate::{
    incremental_field_overlay::{WorldFieldOverlayContextV1, WorldFieldOverlayErrorV1, lookup},
    incremental_typed_record_codec::*,
    incremental_typed_record_diff::{ChangeDecoder, accept_body},
    incremental_typed_record_patch::patch_typed_path_v1,
    incremental_world_rule_composite_family::{
        CompositeBranchCodecV1, WorldRuleCompositeQualificationV1, read_body,
    },
};
use oasis7_formal_protocol::incremental_execution::{ExecutionFence, PreparedGuard, RootSetV1};
use oasis7_proto::memory_budget::{MemoryClass, ReservedValue};
use std::{cell::RefCell, marker::PhantomData};
fn invalid() -> ValueTreeError {
    ValueTreeError::Invalid("world rule composite typed overlay")
}
pub(crate) struct CompositePathPatchV1<'a> {
    pub(crate) path: &'a [&'a [u8]],
    pub(crate) expected: Option<[u8; 32]>,
    pub(crate) after: Option<Encoded<'a>>,
}
/// Private structural proof of this branch's current body in the transaction.
/// It does not accept the transaction or qualify any other original branch.
pub struct WorldRuleCompositeOverlayV1<T> {
    parent: [u8; 32],
    body: ReservedValue<StateRootV1>,
    guard: PreparedGuard,
    class: MemoryClass,
    _type: PhantomData<fn() -> T>,
}
impl<T> WorldRuleCompositeOverlayV1<T> {
    pub fn body(&self) -> &StateRootV1 {
        &self.body
    }
}
pub(crate) fn patch_world_rule_branch_v1<
    'a,
    T: CompositeBranchCodecV1,
    M: MerkleNodeReader,
    S: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    W: ValueNodeReader + StatePageWriter,
    CW: FnMut() -> bool,
    I: IntoIterator<Item = CompositePathPatchV1<'a>>,
>(
    parent: &ReservedValue<WorldRuleCompositeQualificationV1<T>>,
    roots: &RootSetV1,
    current: Option<&ReservedValue<WorldRuleCompositeOverlayV1<T>>>,
    changes: I,
    state: &S,
    context: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    write: &mut MemberRecordWriteContext<'_, W, CW>,
    scan: &mut StateScanBudget,
    read_work: &mut TypedRecordReadWorkV1,
    write_work: &mut TypedRecordWriteWorkV1,
    guard: &PreparedGuard,
    live: &ExecutionFence,
) -> Result<Option<ReservedValue<WorldRuleCompositeOverlayV1<T>>>, WorldFieldOverlayErrorV1> {
    let cancel = RefCell::new(&mut context.cancelled);
    let write_cancel = RefCell::new(&mut write.cancelled);
    context.transaction.contribute(|tx| {
        tx.validate_guard(guard, live)?;
        parent.check_budget(write.memory, MemoryClass::Ordinary)?;
        if !parent.uses_budget(write.memory)
            || parent.reserved_bytes() < 512
            || !tx.uses_budget(write.memory)
            || !context.values.memory.same_pool(write.memory)
            || context.values.class != MemoryClass::Ordinary
        {
            return Err(invalid().into());
        }
        let mut phase = write
            .memory
            .reserve(128 << 10, MemoryClass::Ordinary)
            .map_err(ValueTreeError::Memory)?;
        parent.validate_binding(roots)?;
        let digest = roots.digest().map_err(|_| invalid())?;
        if tx.parent_fence().parent_roots != digest
            || !parent.parent_present()
            || !parent.uses_budget(write.memory)
            || parent.reserved_bytes() < 512
            || !tx.uses_budget(write.memory)
            || !context.values.memory.same_pool(write.memory)
            || context.values.class != MemoryClass::Ordinary
        {
            return Err(invalid().into());
        }
        if let Some(current) = current {
            tx.validate_transaction_identity(&current.guard)?;
            if current.parent != digest
                || current.class != MemoryClass::Ordinary
                || !current.uses_budget(write.memory)
                || current.reserved_bytes() < 512
                || !current.body.uses_budget(write.memory)
                || current.body.reserved_bytes() < 512
            {
                return Err(invalid().into());
            }
        }
        let mut combined = || (*cancel.borrow_mut())() || (*write_cancel.borrow_mut())();
        if combined() {
            return Err(ValueTreeError::Cancelled.into());
        }
        let (outer, semantic_bytes, initial) = {
            let mut records = crate::incremental_inventory_value_read::InventoryValueReader {
                values: &mut context.values,
                key_work: &mut *context.key_work,
                semantic_bytes: context.semantic_bytes,
                record_bytes: context.record_bytes,
                cancelled: &mut combined,
            };
            let (field, present) = crate::incremental_world_rule_parent::read_parent_overlay(
                tx,
                &mut records,
                &mut context.take_state_error,
            )?;
            if !present {
                return Err(invalid().into());
            }
            let (outer, semantic_bytes) = records.key(
                field,
                WorldFieldRecordKindV1::Entry,
                &(T::BRANCH.name(), ()),
            )?;
            let id = lookup(tx, &outer, &mut records, &mut context.take_state_error)?
                .ok_or_else(invalid)?;
            let initial = read_body::<T, _, _>(&outer, semantic_bytes, &id, &mut records)?;
            (outer, semantic_bytes, initial)
        };
        let expected = current
            .map(|c| c.body())
            .or_else(|| parent.body())
            .ok_or_else(invalid)?;
        if expected != &*initial {
            return Err(invalid().into());
        }
        let profile = T::BRANCH.profile();
        let mut body = TypedTreeBuilderV1::from_root(&initial, write.memory)?;
        {
            let mut read = WorldFieldReadContextV1 {
                root: &initial,
                state,
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
                cancelled: &mut combined,
            };
            crate::incremental_world_rule_composite_family::check(&mut read)?;
            let mut output = MemberRecordWriteContext {
                sink: &mut *write.sink,
                memory: write.memory,
                io: &mut *write.io,
                key_work: &mut *write.key_work,
                limits: write.limits,
                cancelled: || (*cancel.borrow_mut())() || (*write_cancel.borrow_mut())(),
            };
            for change in changes {
                patch_typed_path_v1(
                    &profile,
                    &mut body,
                    change.path,
                    change.expected,
                    change.after,
                    &mut read,
                    &mut output,
                    write_work,
                )?;
            }
            crate::incremental_world_rule_composite_family::check(&mut read)?;
            if body.root() == &*initial {
                return Ok(None);
            }
            let trees_remaining = read_work.max_retained_trees;
            accept_body::<T, _, _, _, _>(
                &profile,
                Some(&initial),
                Some(body.root()),
                &mut ChangeDecoder {
                    ctx: &mut read,
                    scan,
                    work: read_work,
                    profile,
                    trees_remaining,
                    tree_depth: 0,
                },
            )?;
            crate::incremental_world_rule_composite_family::check(&mut read)?;
        }
        if combined() {
            return Err(ValueTreeError::Cancelled.into());
        }
        let semantic = WorldFieldKeyV1::from_bytes(outer.as_bytes())
            .map_err(ValueTreeError::Invalid)?
            .semantic_key
            .ok_or_else(invalid)?;
        let reference = RecordRootRefV1::new(profile.schema, semantic, body.root().clone())
            .map_err(|_| invalid())?;
        let mut output = MemberRecordWriteContext {
            sink: &mut *write.sink,
            memory: write.memory,
            io: &mut *write.io,
            key_work: &mut *write.key_work,
            limits: write.limits,
            cancelled: || (*cancel.borrow_mut())() || (*write_cancel.borrow_mut())(),
        };
        let row = write_member_record_v1(
            crate::incremental_world_rule_parent::descriptor()?,
            WorldFieldRecordKindV1::Entry,
            &(T::BRANCH.name(), ()),
            MemberPayloadRefV1::RecordRoot(&reference),
            &mut output,
        )?;
        if row.field_key != outer || semantic_bytes == 0 {
            return Err(invalid().into());
        }
        if (output.cancelled)() {
            return Err(ValueTreeError::Cancelled.into());
        }
        if let Some(error) = (context.take_state_error)() {
            return Err(error.into());
        }
        tx.write(
            StateNamespaceV1::Authorization,
            outer.as_bytes(),
            Some(row.value_root),
        )?;
        let proof = phase.split(512).map_err(ValueTreeError::Invalid)?.retain(
            WorldRuleCompositeOverlayV1 {
                parent: digest,
                body: body.into_reserved(),
                guard: tx.guard(),
                class: MemoryClass::Ordinary,
                _type: PhantomData,
            },
        );
        if (output.cancelled)() {
            return Err(ValueTreeError::Cancelled.into());
        }
        Ok(Some(proof))
    })
}
