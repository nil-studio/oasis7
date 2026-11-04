//! Qualified status-only mutation preserves original manifest and patch graphs.
use super::*;
use crate::{
    ProposalStatus,
    incremental_field_overlay::{WorldFieldOverlayErrorV1, lookup},
    incremental_inventory_value_read::InventoryValueReader,
    incremental_record_tree::{self as tree, RecordTreeWritePolicyV1},
};
use oasis7_formal_protocol::incremental_execution::{
    ExecutionFence, ExecutionTransaction, PreparedGuard,
};
use oasis7_formal_protocol::incremental_state::StatePageWriter;
pub struct RuntimeProposalBodyQualificationV1 {
    parent: [u8; 32],
    original: u64,
    root: StateRootV1,
    class: MemoryClass,
}
impl RuntimeProposalBodyQualificationV1 {
    pub fn root(&self) -> &StateRootV1 {
        &self.root
    }
    pub fn original_key(&self) -> u64 {
        self.original
    }
}
pub fn read_runtime_proposal_body_qualification_v1<M, R, E, C>(
    parent: &ReservedValue<RuntimeProposalsQualificationV1>,
    roots: &RootSetV1,
    original: u64,
    ctx: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
) -> Result<Option<ReservedValue<RuntimeProposalBodyQualificationV1>>, ValueTreeError>
where
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
{
    ctx.check()?;
    parent.validate_binding(roots)?;
    if !parent.uses_budget(ctx.values.memory)
        || parent.reserved_bytes() < 512
        || parent.class != ctx.values.class
        || ctx.root != roots.state_root(StateNamespaceV1::Authorization)
    {
        return Err(invalid());
    }
    let retained = ctx.values.memory.reserve(512, ctx.values.class)?;
    let _scratch = ctx.values.memory.reserve(16 << 10, ctx.values.class)?;
    let metadata = WorldFieldKeyV1::new(
        runtime_proposal_descriptor_v1(),
        WorldFieldRecordKindV1::Metadata,
        None,
    )
    .map_err(ValueTreeError::Invalid)?;
    let meta = ctx.lookup(&metadata)?.ok_or_else(invalid)?;
    ctx.records().presence(&meta, &metadata, 0)?;
    let (key, _) = ctx.key(
        runtime_proposal_descriptor_v1(),
        WorldFieldRecordKindV1::Entry,
        &original,
    )?;
    let Some(value) = ctx.lookup(&key)? else {
        ctx.check()?;
        return Ok(None);
    };
    let (actual, root) = outer_child(&key, &value, ctx)?;
    if actual != original {
        return Err(invalid());
    }
    ctx.check()?;
    Ok(Some(retained.retain(RuntimeProposalBodyQualificationV1 {
        parent: parent.roots,
        original,
        root,
        class: ctx.values.class,
    })))
}
pub struct RuntimeProposalStatusChangeV1<'a> {
    pub expected: &'a ProposalStatus,
    pub after: &'a ProposalStatus,
    pub max_pending_node_bytes: usize,
}
fn combined<C: FnMut() -> bool, CW: FnMut() -> bool>(cancel: &RefCell<(&mut C, &mut CW)>) -> bool {
    let mut c = cancel.borrow_mut();
    (c.0)() || (c.1)()
}
#[allow(clippy::too_many_arguments)]
pub fn change_runtime_proposal_status_v1<M, R, E, C, W, CW>(
    change: RuntimeProposalStatusChangeV1<'_>,
    body: &ReservedValue<RuntimeProposalBodyQualificationV1>,
    transaction: &mut ExecutionTransaction<'_, M>,
    read: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
    write: &mut MemberRecordWriteContext<'_, W, CW>,
    guard: &PreparedGuard,
    live: &ExecutionFence,
    work: &mut RuntimeProposalReadWorkV1,
) -> Result<Option<ReservedValue<RuntimeProposalBodyQualificationV1>>, WorldFieldOverlayErrorV1>
where
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    W: StatePageWriter + ValueNodeReader,
    CW: FnMut() -> bool,
{
    transaction.contribute(|tx| {
        tx.validate_guard(guard, live)?;
        if body.class != MemoryClass::Ordinary
            || body.reserved_bytes() < 512
            || !body.uses_budget(read.values.memory)
            || !tx.uses_budget(read.values.memory)
            || !tx.uses_budget(write.memory)
            || read.values.class != MemoryClass::Ordinary
            || body.parent != tx.parent_fence().parent_roots
        {
            return Err(invalid().into());
        }
        let retained = read
            .values
            .memory
            .reserve(512, MemoryClass::Ordinary)
            .map_err(ValueTreeError::Memory)?;
        let _scratch = read
            .values
            .memory
            .reserve(32 << 10, MemoryClass::Ordinary)
            .map_err(ValueTreeError::Memory)?;
        let cancel = RefCell::new((&mut read.cancelled, &mut write.cancelled));
        let mut local = WorldFieldReadContextV1 {
            root: read.root,
            state: read.state,
            values: ValueReadContext {
                source: read.values.source,
                memory: read.values.memory,
                class: read.values.class,
                io: &mut *read.values.io,
            },
            key_work: &mut *read.key_work,
            semantic_bytes: read.semantic_bytes,
            record_bytes: read.record_bytes,
            take_state_error: &mut read.take_state_error,
            cancelled: || combined(&cancel),
        };
        let (entry, current) = {
            let mut records = InventoryValueReader {
                values: &mut local.values,
                key_work: &mut *local.key_work,
                semantic_bytes: local.semantic_bytes,
                record_bytes: local.record_bytes,
                cancelled: &mut local.cancelled,
            };
            let metadata = WorldFieldKeyV1::new(
                runtime_proposal_descriptor_v1(),
                WorldFieldRecordKindV1::Metadata,
                None,
            )
            .map_err(ValueTreeError::Invalid)?;
            let meta = lookup(tx, &metadata, &mut records, &mut local.take_state_error)?
                .ok_or_else(invalid)?;
            records.presence(&meta, &metadata, 0)?;
            let (key, _) = records.key(
                runtime_proposal_descriptor_v1(),
                WorldFieldRecordKindV1::Entry,
                &body.original,
            )?;
            let current =
                lookup(tx, &key, &mut records, &mut local.take_state_error)?.ok_or_else(invalid)?;
            (key, current)
        };
        let (original, current_root) = outer_child(&entry, &current, &mut local)?;
        if original != body.original || current_root != body.root {
            return Err(invalid().into());
        }
        let before = header::validate_status(
            &current_root,
            Some(change.expected),
            &mut local.records(),
            work,
        )?;
        if combined(&cancel) {
            return Err(ValueTreeError::Cancelled.into());
        }
        let unchanged = change.expected == change.after;
        if combined(&cancel) {
            return Err(ValueTreeError::Cancelled.into());
        }
        if unchanged {
            return Ok(None);
        }
        let (tag, texts) = header::view(change.after);
        let mut root = current_root;
        for index in 0..4 {
            if combined(&cancel) {
                return Err(ValueTreeError::Cancelled.into());
            }
            work.cells_remaining =
                work.cells_remaining
                    .checked_sub(1)
                    .ok_or(ValueTreeError::ResourceLimited(
                        "proposal status mutation cells",
                    ))?;
            let (key, _, old) = &before.cells[index];
            let payload = if index == 0 {
                Some(MemberPayloadRefV1::U64(tag))
            } else {
                texts[index - 1].map(MemberPayloadRefV1::Text)
            };
            let next = if let Some(payload) = payload {
                let row = write_member_record_v1(
                    columns(),
                    WorldFieldRecordKindV1::Entry,
                    &header::STATUS_COLUMNS[index],
                    payload,
                    &mut MemberRecordWriteContext {
                        sink: &mut *write.sink,
                        memory: write.memory,
                        io: &mut *write.io,
                        key_work: &mut *write.key_work,
                        limits: write.limits,
                        cancelled: || combined(&cancel),
                    },
                )?;
                if row.field_key != *key {
                    return Err(invalid().into());
                }
                Some(row.value_root)
            } else {
                None
            };
            if *old != next {
                root = tree::edit(
                    root,
                    key,
                    (*old, next),
                    &mut local.values,
                    &mut MemberRecordWriteContext {
                        sink: &mut *write.sink,
                        memory: write.memory,
                        io: &mut *write.io,
                        key_work: &mut *write.key_work,
                        limits: write.limits,
                        cancelled: || combined(&cancel),
                    },
                    &mut || combined(&cancel),
                    RecordTreeWritePolicyV1 {
                        max_pending_node_bytes: change.max_pending_node_bytes,
                        labels: genesis::LABELS,
                    },
                )?;
            }
        }
        header::validate_status(&root, Some(change.after), &mut local.records(), work)?;
        let owner = crate::incremental_semantic_key::hash_semantic_key_v1(
            &body.original,
            write.memory,
            MemoryClass::Ordinary,
            write.key_work,
            &mut || combined(&cancel),
        )
        .map_err(MemberRecordError::Key)?;
        let reference =
            RecordRootRefV1::new(schema(), owner.digest, root.clone()).map_err(|_| invalid())?;
        let row = write_member_record_v1(
            runtime_proposal_descriptor_v1(),
            WorldFieldRecordKindV1::Entry,
            &body.original,
            MemberPayloadRefV1::RecordRoot(&reference),
            &mut MemberRecordWriteContext {
                sink: &mut *write.sink,
                memory: write.memory,
                io: &mut *write.io,
                key_work: &mut *write.key_work,
                limits: write.limits,
                cancelled: || combined(&cancel),
            },
        )?;
        if row.field_key != entry {
            return Err(invalid().into());
        }
        if combined(&cancel) {
            return Err(ValueTreeError::Cancelled.into());
        }
        let ready = tx.guard();
        tx.validate_guard(&ready, live)?;
        tx.write(
            StateNamespaceV1::Authorization,
            entry.as_bytes(),
            Some(row.value_root),
        )?;
        if combined(&cancel) {
            return Err(ValueTreeError::Cancelled.into());
        }
        Ok(Some(retained.retain(RuntimeProposalBodyQualificationV1 {
            parent: body.parent,
            original: body.original,
            root,
            class: MemoryClass::Ordinary,
        })))
    })
}

#[path = "incremental_runtime_proposal_create.rs"]
mod create;
pub use create::create_runtime_proposal_overlay_v1;
