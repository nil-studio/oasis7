//! Complete all-intent max over the qualified parent plus actual coalesced
//! Current group changes. No agent list, caller epoch or cold normal scan.
use super::*;
use crate::incremental_field_overlay::{WorldFieldOverlayContextV1, WorldFieldOverlayErrorV1};
use oasis7_formal_protocol::incremental_execution::{
    ExecutionFence, ExecutionTransaction, PreparedGuard, SealedExecutionBindingV1,
};
use oasis7_proto::memory_budget::{MemoryBudget, MemoryClass};
pub struct CurrentIntentReorgAggregateV2 {
    tree: intent_epochs::Link,
    cover: Option<Arc<ReservedValue<()>>>,
    guard: PreparedGuard,
    parent_roots: [u8; 32],
    _fee: ReservedValue<()>,
}
pub struct FrozenIntentReorgAggregateV2 {
    tree: intent_epochs::Link,
    cover: Option<Arc<ReservedValue<()>>>,
    guard: PreparedGuard,
    _fee: ReservedValue<()>,
}
pub struct SealedIntentReorgAggregateV2 {
    tree: intent_epochs::Link,
    cover: Option<Arc<ReservedValue<()>>>,
    roots: [u8; 32],
    _fee: ReservedValue<()>,
}
impl CurrentIntentReorgAggregateV2 {
    pub fn validate<M: MerkleNodeReader>(
        &self,
        tx: &ExecutionTransaction<'_, M>,
        memory: &MemoryBudget,
        live: &ExecutionFence,
    ) -> Result<(), ValueTreeError> {
        tx.validate_guard(&self.guard, live)
            .map_err(|e| ValueTreeError::Io(std::io::Error::other(e)))?;
        if !self._fee.uses_budget(memory)
            || self.parent_roots != tx.parent_fence().parent_roots
            || self
                .cover
                .as_ref()
                .is_some_and(|fee| !fee.uses_budget(memory))
            || (intent_epochs::required_ordinary_cover(&self.tree) > 0
                && self.cover.as_ref().is_none_or(|fee| {
                    fee.reserved_bytes() < intent_epochs::required_ordinary_cover(&self.tree)
                }))
        {
            return Err(invalid());
        }
        Ok(())
    }
    pub fn max_reorg_epoch(&self) -> u64 {
        intent_epochs::max_epoch(&self.tree)
    }
    pub fn freeze<M: MerkleNodeReader>(
        self,
        tx: &ExecutionTransaction<'_, M>,
        memory: &MemoryBudget,
        live: &ExecutionFence,
    ) -> Result<FrozenIntentReorgAggregateV2, ValueTreeError> {
        self.validate(tx, memory, live)?;
        Ok(FrozenIntentReorgAggregateV2 {
            tree: self.tree,
            cover: self.cover,
            guard: tx.guard(),
            _fee: self._fee,
        })
    }
}
impl FrozenIntentReorgAggregateV2 {
    /// The actual after-family receiver independently carries this same metadata
    /// into the next accepted parent; this witness cannot mint that qualification.
    pub fn rebind_after_seal_v2(
        self,
        sealed: &SealedExecutionBindingV1,
        after: &RootSetV1,
        qualified: &ReservedValue<AgentAuthorityFamilyQualificationV1>,
        memory: &MemoryBudget,
        mut cancelled: impl FnMut() -> bool,
    ) -> Result<SealedIntentReorgAggregateV2, ValueTreeError> {
        if cancelled() {
            return Err(ValueTreeError::Cancelled);
        }
        sealed
            .validate_source_guard(&self.guard, memory)
            .map_err(|e| ValueTreeError::Io(std::io::Error::other(e)))?;
        sealed
            .validate_after_roots(after, memory)
            .map_err(|e| ValueTreeError::Io(std::io::Error::other(e)))?;
        let _phase = memory.reserve(4096, MemoryClass::Ordinary)?;
        qualified.validate_binding(&sealed.after_roots_digest())?;
        qualified.check_intent_epoch_pool(memory)?;
        if !qualified.uses_budget(memory)
            || !self._fee.uses_budget(memory)
            || qualified.class != MemoryClass::Ordinary
            || qualified.record_counts()[0] != intent_epochs::record_count(&self.tree)
            || qualified.max_intent_reorg_epoch_v2() != intent_epochs::max_epoch(&self.tree)
        {
            return Err(invalid());
        }
        if cancelled() {
            return Err(ValueTreeError::Cancelled);
        }
        Ok(SealedIntentReorgAggregateV2 {
            tree: self.tree,
            cover: self.cover,
            roots: sealed.after_roots_digest(),
            _fee: self._fee,
        })
    }
}
impl SealedIntentReorgAggregateV2 {
    pub fn max_reorg_epoch(&self) -> u64 {
        intent_epochs::max_epoch(&self.tree)
    }
    pub fn validate_binding(
        &self,
        roots: &[u8; 32],
        memory: &MemoryBudget,
    ) -> Result<(), ValueTreeError> {
        if &self.roots != roots
            || !self._fee.uses_budget(memory)
            || self
                .cover
                .as_ref()
                .is_some_and(|fee| !fee.uses_budget(memory))
        {
            return Err(invalid());
        }
        Ok(())
    }
}
#[allow(clippy::too_many_arguments)]
pub fn open_current_intent_reorg_aggregate_v2<M, P, R, E, C>(
    parent: &ReservedValue<AgentAuthorityFamilyQualificationV1>,
    roots: &RootSetV1,
    state: &P,
    ctx: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    scan: &mut StateScanBudget,
    cells: &mut u64,
    max_payload: u64,
    pending_entries: &mut usize,
    guard: &PreparedGuard,
    live: &ExecutionFence,
) -> Result<CurrentIntentReorgAggregateV2, WorldFieldOverlayErrorV1>
where
    M: MerkleNodeReader,
    P: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
{
    ctx.transaction.validate_guard(guard, live)?;
    let _phase = crate::incremental_record_qualification_phase::bind_phase_v1(
        parent,
        crate::incremental_record_qualification_phase::FamilyBindingV1::Roots(roots),
        ctx.values.memory,
        MemoryClass::Ordinary,
        &mut ctx.cancelled,
    )?;
    parent.check_intent_epoch_pool(ctx.values.memory)?;
    if ctx.values.class != MemoryClass::Ordinary
        || !ctx.transaction.uses_budget(ctx.values.memory)
        || state.allocation_class() != Some(MemoryClass::Ordinary)
        || state
            .allocation_budget()
            .is_none_or(|memory| !memory.same_pool(ctx.values.memory))
        || ctx.transaction.parent_fence().parent_roots != roots.digest().map_err(|_| invalid())?
    {
        return Err(invalid().into());
    }
    let fee = ctx
        .values
        .memory
        .reserve(
            std::mem::size_of::<CurrentIntentReorgAggregateV2>(),
            MemoryClass::Ordinary,
        )
        .map_err(ValueTreeError::Memory)?;
    let mut tree = intent_epochs::share_tree(&parent.intent_epochs);
    let cancel = RefCell::new(&mut ctx.cancelled);
    let mut local = WorldFieldReadContextV1 {
        root: roots
            .state_root(oasis7_formal_protocol::incremental_state::StateNamespaceV1::Authorization),
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
        cancelled: || (cancel.borrow_mut())(),
    };
    ctx.transaction
        .visit_pending_writes::<WorldFieldOverlayErrorV1>(
            AgentAuthorityRecordKindV1::IntentLedger.namespace(),
            pending_entries,
            || {
                if (cancel.borrow_mut())() {
                    Err(ValueTreeError::Cancelled.into())
                } else {
                    Ok(())
                }
            },
            |key, before, after| {
                *cells = cells.checked_sub(1).ok_or(ValueTreeError::ResourceLimited(
                    "Current intent epoch changes",
                ))?;
                if !key.starts_with(b"oasis7.iws.world-field-key.v1\0") {
                    return Ok(());
                }
                let binding = WorldFieldKeyV1::from_bytes(key).map_err(ValueTreeError::Invalid)?;
                if binding.name != AgentAuthorityRecordKindV1::IntentLedger.field_name() {
                    return Ok(());
                }
                let old = before
                    .map(|value| {
                        outer_parts(
                            AgentAuthorityRecordKindV1::IntentLedger.namespace(),
                            key,
                            value,
                            &mut local,
                        )
                    })
                    .transpose()?
                    .flatten();
                let new = after
                    .map(|value| {
                        outer_parts(
                            AgentAuthorityRecordKindV1::IntentLedger.namespace(),
                            key,
                            value,
                            &mut local,
                        )
                    })
                    .transpose()?
                    .flatten();
                if old.is_none() && new.is_none() {
                    if before.is_none() || after.is_none() {
                        return Err(invalid().into());
                    }
                    return Ok(());
                }
                match (&old, &new) {
                    (None, Some((kind, owner, root))) => {
                        let mut body = WorldFieldReadContextV1 {
                            root,
                            state: local.state,
                            values: ValueReadContext {
                                source: local.values.source,
                                memory: local.values.memory,
                                class: local.values.class,
                                io: &mut *local.values.io,
                            },
                            key_work: &mut *local.key_work,
                            semantic_bytes: local.semantic_bytes,
                            record_bytes: local.record_bytes,
                            take_state_error: &mut local.take_state_error,
                            cancelled: &mut local.cancelled,
                        };
                        qualify_agent_authority_body_v1(
                            *kind,
                            *owner,
                            &mut body,
                            scan,
                            cells,
                            max_payload,
                        )?;
                    }
                    (Some((old_kind, old_owner, old)), Some((new_kind, new_owner, new))) => {
                        if old_kind != new_kind || old_owner != new_owner {
                            return Err(invalid().into());
                        }
                        super::super::receiver::child(
                            AgentAuthorityRecordKindV1::IntentLedger,
                            old,
                            new,
                            &mut local.records(),
                            cells,
                            max_payload,
                        )?;
                    }
                    (Some(_), None) => {}
                    _ => return Err(invalid().into()),
                }
                tree = fold_verified_intent_epoch_change(
                    &tree,
                    key,
                    old.as_ref().map(|row| &row.2),
                    new.as_ref().map(|row| &row.2),
                    &mut local,
                    cells,
                )?;
                Ok(())
            },
        )?;
    local.check()?;
    let value = CurrentIntentReorgAggregateV2 {
        tree,
        cover: parent.intent_ordinary_cover.as_ref().map(Arc::clone),
        guard: ctx.transaction.guard(),
        parent_roots: ctx.transaction.parent_fence().parent_roots,
        _fee: fee.retain(()),
    };
    value
        .validate(ctx.transaction, ctx.values.memory, live)
        .map_err(WorldFieldOverlayErrorV1::Value)?;
    Ok(value)
}
