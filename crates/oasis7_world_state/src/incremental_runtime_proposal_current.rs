//! Complete selected Current proposal records. These are storage observations,
//! never governance, finality, or Action authorization.
use super::*;
use crate::{
    Proposal,
    incremental_field_overlay::{WorldFieldOverlayContextV1, WorldFieldOverlayErrorV1, lookup},
    incremental_inventory_value_read::InventoryValueReader,
    incremental_runtime_proposal::{
        RecoveredRuntimeProposalV1, prepare_runtime_proposal_member_v1,
        recover_runtime_proposal_body_v1,
    },
};
use oasis7_formal_protocol::{
    incremental_execution::{ExecutionFence, ExecutionTransaction, PreparedGuard},
    incremental_state::StatePageWriter,
};

/// The private expected outer value remains authoritative even after unrelated
/// reads advance the transaction revision. The full recovered graph retains all
/// original codec allocations until this owner is dropped.
pub struct CurrentRuntimeProposalV1 {
    original: u64,
    key: WorldFieldKeyV1,
    before: Option<[u8; 32]>,
    body: Option<RecoveredRuntimeProposalV1>,
    guard: PreparedGuard,
    roots: [u8; 32],
}
impl CurrentRuntimeProposalV1 {
    pub fn value(&self) -> Option<&Proposal> {
        self.body.as_ref().map(RecoveredRuntimeProposalV1::value)
    }
    pub fn original_id(&self) -> u64 {
        self.original
    }
    pub fn output_reserved_bytes_v1(&self) -> Result<usize, ValueTreeError> {
        self.body
            .as_ref()
            .map(RecoveredRuntimeProposalV1::output_reserved_bytes_v1)
            .transpose()
            .map(|n| n.unwrap_or(0))
    }
    pub fn output_reservation_count_v1(&self) -> Result<usize, ValueTreeError> {
        self.body
            .as_ref()
            .map(RecoveredRuntimeProposalV1::output_reservation_count_v1)
            .transpose()
            .map(|n| n.unwrap_or(0))
    }

    pub fn validate_provenance<M: MerkleNodeReader>(
        &self,
        tx: &ExecutionTransaction<'_, M>,
        memory: &MemoryBudget,
        live: &ExecutionFence,
    ) -> Result<(), WorldFieldOverlayErrorV1> {
        tx.validate_transaction_identity(&self.guard)?;
        tx.validate_guard(&tx.guard(), live)?;
        if !tx.uses_budget(memory) || tx.parent_fence().parent_roots != self.roots {
            return Err(invalid().into());
        }
        Ok(())
    }
    pub fn validate_strict<M: MerkleNodeReader>(
        &self,
        tx: &ExecutionTransaction<'_, M>,
        memory: &MemoryBudget,
        live: &ExecutionFence,
    ) -> Result<(), WorldFieldOverlayErrorV1> {
        self.validate_provenance(tx, memory, live)?;
        tx.validate_guard(&self.guard, live)?;
        Ok(())
    }
}

/// Open the actual transaction outer row before recovering its closed child.
/// In particular a changed proposal is never reopened at its parent outer root.
#[allow(clippy::too_many_arguments)]
pub fn read_current_runtime_proposal_v1<M, P, R, E, C>(
    original: u64,
    parent: &ReservedValue<RuntimeProposalsQualificationV1>,
    roots: &RootSetV1,
    state: &P,
    ctx: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    scan: &mut StateScanBudget,
    work: &mut RuntimeProposalReadWorkV1,
    guard: &PreparedGuard,
    live: &ExecutionFence,
) -> Result<ReservedValue<CurrentRuntimeProposalV1>, WorldFieldOverlayErrorV1>
where
    M: MerkleNodeReader,
    P: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
{
    let caught = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        ctx.transaction.validate_guard(guard, live)?;
        parent.validate_binding(roots)?;
        let digest = roots.digest().map_err(|_| invalid())?;
        if parent.class != MemoryClass::Ordinary
            || parent.reserved_bytes() < 512
            || !parent.uses_budget(ctx.values.memory)
            || ctx.values.class != MemoryClass::Ordinary
            || !ctx.transaction.uses_budget(ctx.values.memory)
            || digest != ctx.transaction.parent_fence().parent_roots
        {
            return Err(invalid().into());
        }
        let fee = ctx
            .values
            .memory
            .reserve(
                std::mem::size_of::<CurrentRuntimeProposalV1>() + 1024,
                MemoryClass::Ordinary,
            )
            .map_err(ValueTreeError::Memory)?;
        work.cells_remaining =
            work.cells_remaining
                .checked_sub(2)
                .ok_or(ValueTreeError::ResourceLimited(
                    "Current proposal outer reads",
                ))?;
        let (key, before) = {
            let mut records = InventoryValueReader {
                values: &mut ctx.values,
                key_work: &mut *ctx.key_work,
                semantic_bytes: ctx.semantic_bytes,
                record_bytes: ctx.record_bytes,
                cancelled: &mut ctx.cancelled,
            };
            let metadata = WorldFieldKeyV1::new(
                runtime_proposal_descriptor_v1(),
                WorldFieldRecordKindV1::Metadata,
                None,
            )
            .map_err(ValueTreeError::Invalid)?;
            let meta = lookup(
                ctx.transaction,
                &metadata,
                &mut records,
                &mut ctx.take_state_error,
            )?
            .ok_or_else(invalid)?;
            records.presence(&meta, &metadata, 0)?;
            let (key, _) = records.key(
                runtime_proposal_descriptor_v1(),
                WorldFieldRecordKindV1::Entry,
                &original,
            )?;
            let before = lookup(
                ctx.transaction,
                &key,
                &mut records,
                &mut ctx.take_state_error,
            )?;
            (key, before)
        };
        let body = if let Some(value) = before {
            // outer_child only reads the supplied actual member bytes. Its
            // local root is not used as a fallback outer lookup.
            let fallback = roots
                .state
                .iter()
                .find(|r| r.namespace == StateNamespaceV1::Authorization)
                .ok_or_else(invalid)?;
            let mut read = WorldFieldReadContextV1 {
                root: fallback,
                state,
                values: ValueReadContext {
                    source: ctx.values.source,
                    memory: ctx.values.memory,
                    class: ctx.values.class,
                    io: &mut *ctx.values.io,
                },
                key_work: &mut *ctx.key_work,
                semantic_bytes: ctx.semantic_bytes,
                record_bytes: ctx.record_bytes,
                take_state_error: &mut ctx.take_state_error,
                cancelled: &mut ctx.cancelled,
            };
            let (actual, child) = outer_child(&key, &value, &mut read)?;
            if actual != original {
                return Err(invalid().into());
            }
            // Preserve the existing distinct outer-id / Proposal.id semantics.
            let recovered = recover_runtime_proposal_body_v1(
                &child,
                &mut read,
                scan,
                &mut work.json,
                work.max_selected_heap,
            )?;
            Some(recovered)
        } else {
            None
        };
        ctx.transaction
            .validate_guard(&ctx.transaction.guard(), live)?;
        if (ctx.cancelled)() {
            return Err(ValueTreeError::Cancelled.into());
        }
        Ok(fee.retain(CurrentRuntimeProposalV1 {
            original,
            key,
            before,
            body,
            guard: ctx.transaction.guard(),
            roots: digest,
        }))
    }));
    let result = match caught {
        Ok(result) => result,
        Err(payload) => {
            let _: Result<(), oasis7_formal_protocol::incremental_execution::ExecutionError> =
                ctx.transaction.contribute(|_| {
                    Err(oasis7_formal_protocol::incremental_execution::ExecutionError::Aborted)
                });
            std::panic::resume_unwind(payload)
        }
    };
    if result.is_err() {
        let _: Result<(), oasis7_formal_protocol::incremental_execution::ExecutionError> =
            ctx.transaction.contribute(|_| {
                Err(oasis7_formal_protocol::incremental_execution::ExecutionError::Aborted)
            });
    }
    result
}

/// Exact full-row CAS using the sole original proposal column/JSON producer.
/// The caller proves business policy separately; a bare after DTO cannot supply
/// the private expected row, transaction identity, or absence observation.
#[allow(clippy::too_many_arguments)]
pub fn replace_current_runtime_proposal_v1<M, R, E, C, W, CW>(
    before: &ReservedValue<CurrentRuntimeProposalV1>,
    after: &Proposal,
    ctx: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    write: &mut MemberRecordWriteContext<'_, W, CW>,
    work: &mut RuntimeProposalWriteWorkV1,
    guard: &PreparedGuard,
    live: &ExecutionFence,
) -> Result<(), WorldFieldOverlayErrorV1>
where
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    W: StatePageWriter + ValueNodeReader,
    CW: FnMut() -> bool,
{
    let caught = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        ctx.transaction.validate_guard(guard, live)?;
        before.validate_provenance(ctx.transaction, ctx.values.memory, live)?;
        if !before.uses_budget(ctx.values.memory)
            || !ctx.values.memory.same_pool(write.memory)
            || ctx.values.class != MemoryClass::Ordinary
        {
            return Err(invalid().into());
        }
        work.cells_remaining = work
            .cells_remaining
            .checked_sub(1)
            .ok_or(ValueTreeError::ResourceLimited("Current proposal CAS"))?;
        let actual = {
            let mut records = InventoryValueReader {
                values: &mut ctx.values,
                key_work: &mut *ctx.key_work,
                semantic_bytes: ctx.semantic_bytes,
                record_bytes: ctx.record_bytes,
                cancelled: &mut ctx.cancelled,
            };
            lookup(
                ctx.transaction,
                &before.key,
                &mut records,
                &mut ctx.take_state_error,
            )?
        };
        if actual != before.before {
            return Err(ValueTreeError::Invalid("stale Current Runtime proposal").into());
        }
        let row = prepare_runtime_proposal_member_v1(before.original, after, write, work)
            .map_err(WorldFieldOverlayErrorV1::Member)?;
        if row.field_key != before.key {
            return Err(invalid().into());
        }
        if (ctx.cancelled)() || (write.cancelled)() {
            return Err(ValueTreeError::Cancelled.into());
        }
        ctx.transaction
            .validate_guard(&ctx.transaction.guard(), live)?;
        ctx.transaction.write(
            StateNamespaceV1::Authorization,
            before.key.as_bytes(),
            Some(row.value_root),
        )?;
        if (ctx.cancelled)() || (write.cancelled)() {
            return Err(ValueTreeError::Cancelled.into());
        }
        Ok(())
    }));
    let result = match caught {
        Ok(result) => result,
        Err(payload) => {
            let _: Result<(), oasis7_formal_protocol::incremental_execution::ExecutionError> =
                ctx.transaction.contribute(|_| {
                    Err(oasis7_formal_protocol::incremental_execution::ExecutionError::Aborted)
                });
            std::panic::resume_unwind(payload)
        }
    };
    if result.is_err() {
        let _: Result<(), oasis7_formal_protocol::incremental_execution::ExecutionError> =
            ctx.transaction.contribute(|_| {
                Err(oasis7_formal_protocol::incremental_execution::ExecutionError::Aborted)
            });
    }
    result
}
