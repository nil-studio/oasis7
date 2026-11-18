//! Physical Cognition storage absence, distinct from config.enabled=false.
//! The complete parent router proves the role is absent; actual Current net
//! writes are audited through the existing paid lanes, never a cold World scan.
use super::*;
use oasis7_formal_protocol::controlled_authority::formal::bounded_context::WorkIdentityV1;
use oasis7_world_state::{
    incremental_cognition_current_complete_v3::CognitionCurrentPresenceV3,
    incremental_projection::FieldOwner, incremental_world_field_key::WorldFieldKeyV1,
};
pub(in crate::world) struct PaidDisabledNormalCognitionSourceV1 {
    guard: PreparedGuard,
    core: WorkIdentityV1,
    head: [u8; 32],
    _fee: ReservedValue<()>,
}
pub(in crate::world) enum NormalCognitionExpirySourceV1<'a> {
    Enabled(&'a CurrentCognitionProofLedgerV1),
    Disabled(&'a ReservedValue<PaidDisabledNormalCognitionSourceV1>),
}
impl PaidDisabledNormalCognitionSourceV1 {
    pub(in crate::world) fn validate<M: MerkleNodeReader>(
        &self,
        parent: &ReservedValue<BoundNormalTickParentV1<'_>>,
        current: &ReservedValue<CompleteCognitionCurrentQualificationV3>,
        tx: &ExecutionTransaction<'_, M>,
        common: &mut FormalBoundedContextV1<'_>,
        live: &ExecutionFence,
    ) -> Result<(), BodyMutationError> {
        tx.validate_transaction_identity(&self.guard)?;
        tx.validate_guard(&tx.guard(), live)?;
        if self.core != common.work().identity()
            || !self._fee.uses_budget(common.memory())
            || self.head != parent.refs().head.verified_record_id()
        {
            return Err(bad().into());
        }
        validate_parent(parent, current, tx, common)
    }
}
fn validate_parent<M: MerkleNodeReader>(
    parent: &ReservedValue<BoundNormalTickParentV1<'_>>,
    current: &ReservedValue<CompleteCognitionCurrentQualificationV3>,
    tx: &ExecutionTransaction<'_, M>,
    common: &mut FormalBoundedContextV1<'_>,
) -> Result<(), BodyMutationError> {
    parent.validate_context(common).map_err(formal)?;
    common
        .charge(12, parent.refs().roots.world_id.len() as u64 + 2048)
        .map_err(formal)?;
    let refs = parent.refs();
    let memory = common.memory();
    let routing = refs.storage.results();
    let cognition = refs.cognition;
    let selected = cognition.selected();
    let fence = tx.parent_fence();
    if fence != parent.fence()
        || !tx.uses_budget(memory)
        || !current.uses_budget(memory)
        || current.allocation_class() != MemoryClass::Ordinary
        || current.presence() != CognitionCurrentPresenceV3::Disabled
        || !routing.uses_budget(memory)
        || routing.proof_ledger().is_some()
        || selected.manifest().enabled
        || !cognition.uses_budget(memory)
        || cognition.current().enabled()
    {
        return Err(bad().into());
    }
    current.validate_binding(refs.roots)?;
    let disabled = cognition.disabled().ok_or_else(bad)?;
    let route = routing.cognition().occurrence();
    let selected = selected.occurrence();
    if !disabled.uses_budget(memory)
        || disabled.allocation_class() != MemoryClass::Ordinary
        || disabled.head_record_id() != refs.head.verified_record_id()
        || disabled.roots_id() != fence.parent_roots
        || route.candidate_record_id() != refs.head.verified_record_id()
        || route.body_id() != fence.parent_commit
        || route.roots_id() != fence.parent_roots
        || selected.head_record_id() != route.candidate_record_id()
        || selected.head_body_id() != route.body_id()
        || selected.location() != route.location()
    {
        return Err(bad().into());
    }
    Ok(())
}
// One transfer of an already-admitted fine lane, not a second allowance copied
// from Common. Drop reconciles both normal errors and unwinds into the IO bank.
struct Visit<'a> {
    target: &'a mut u64,
    remaining: usize,
}
impl Drop for Visit<'_> {
    fn drop(&mut self) {
        *self.target = self.remaining as u64;
    }
}
impl<'a, 't, 's, 'v, M, R, E, C, W, F, O>
    RootModuleExecutionContextV1<'a, 't, 's, 'v, M, R, E, C, W, F, O>
where
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    W: ValueNodeReader + ValueNodeWriter + StatePageWriter,
    F: FnMut(&mut dyn AgentEventOriginalV1) -> Result<(), BodyMutationError>,
    O: OriginalObjectStreamReader,
{
    fn check_current_disabled_cognition_v1(
        &mut self,
        parent: &ReservedValue<BoundNormalTickParentV1<'_>>,
        current: &ReservedValue<CompleteCognitionCurrentQualificationV3>,
        work: &mut CurrentCognitionExpiryWorkV1,
        common: &mut FormalBoundedContextV1<'_>,
        live: &ExecutionFence,
    ) -> Result<(), RootModuleExecutionErrorV1<O::Error>> {
        validate_parent(parent, current, self.ctx.transaction, common)
            .map_err(RootModuleExecutionErrorV1::Body)?;
        self.ctx
            .transaction
            .validate_guard(&self.ctx.transaction.guard(), live)?;
        common
            .charge(6, 0)
            .map_err(|e| RootModuleExecutionErrorV1::Body(formal(e)))?;
        with_current_cognition_io_v1(self.ctx, self.members, work, common, |ctx, _, work| {
            let remaining = usize::try_from(work.index.cells_remaining).map_err(|_| {
                ValueTreeError::ResourceLimited("disabled Cognition pending visits")
            })?;
            work.index.cells_remaining = 0;
            let mut quota = Visit {
                target: &mut work.index.cells_remaining,
                remaining,
            };
            for ns in [
                StateNamespaceV1::Business,
                StateNamespaceV1::Allocator,
                StateNamespaceV1::Scheduler,
                StateNamespaceV1::Authorization,
                StateNamespaceV1::Effects,
                StateNamespaceV1::Artifacts,
            ] {
                ctx.transaction.visit_pending_writes::<BodyMutationError>(
                    ns,
                    &mut quota.remaining,
                    || {
                        if (ctx.cancelled)() {
                            Err(ValueTreeError::Cancelled.into())
                        } else {
                            Ok(())
                        }
                    },
                    |key, _, _| {
                        ctx.key_work.bytes_remaining = ctx
                            .key_work
                            .bytes_remaining
                            .checked_sub(key.len() as u64)
                            .ok_or(ValueTreeError::ResourceLimited(
                                "disabled Cognition pending keys",
                            ))?;
                        if key.starts_with(b"oasis7.iws.world-field-key.v1\0") {
                            let parsed = WorldFieldKeyV1::from_bytes(key)
                                .map_err(ValueTreeError::Invalid)?;
                            if parsed.namespace != ns || parsed.owner == FieldOwner::Cognition {
                                return Err(bad().into());
                            }
                        }
                        Ok(())
                    },
                )?;
            }
            Ok(())
        })
        .map_err(RootModuleExecutionErrorV1::Body)
    }
    pub(in crate::world) fn prepare_disabled_normal_cognition_source_v1(
        &mut self,
        parent: &ReservedValue<BoundNormalTickParentV1<'_>>,
        current: &ReservedValue<CompleteCognitionCurrentQualificationV3>,
        work: &mut CurrentCognitionExpiryWorkV1,
        common: &mut FormalBoundedContextV1<'_>,
        live_owner: &mut dyn FnMut() -> ExecutionFence,
    ) -> Result<
        ReservedValue<PaidDisabledNormalCognitionSourceV1>,
        RootModuleExecutionErrorV1<O::Error>,
    > {
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            self.check_current_disabled_cognition_v1(parent, current, work, common, &live_owner())?;
            common
                .charge(
                    1,
                    std::mem::size_of::<PaidDisabledNormalCognitionSourceV1>() as u64,
                )
                .map_err(|e| RootModuleExecutionErrorV1::Body(formal(e)))?;
            let fee = common
                .memory()
                .reserve(
                    std::mem::size_of::<PaidDisabledNormalCognitionSourceV1>(),
                    MemoryClass::Ordinary,
                )
                .map_err(ValueTreeError::Memory)?;
            let marker = common
                .memory()
                .reserve(0, MemoryClass::Ordinary)
                .map_err(ValueTreeError::Memory)?
                .retain(());
            Ok(fee.retain(PaidDisabledNormalCognitionSourceV1 {
                guard: self.ctx.transaction.guard(),
                core: common.work().identity(),
                head: parent.refs().head.verified_record_id(),
                _fee: marker,
            }))
        }));
        self.finish_disabled_result(result)
    }
    pub(in crate::world) fn execute_disabled_cognition_expiry_v1<'p>(
        &mut self,
        parent: &'p ReservedValue<BoundNormalTickParentV1<'p>>,
        written: &ReservedValue<HistoryWrittenIwsCommitmentV1>,
        current: &ReservedValue<CompleteCognitionCurrentQualificationV3>,
        source: &ReservedValue<PaidDisabledNormalCognitionSourceV1>,
        continued: Option<PendingCurrentCognitionJournalV1<'p>>,
        work: &mut CurrentCognitionExpiryWorkV1,
        common: &mut FormalBoundedContextV1<'_>,
        live_owner: &mut dyn FnMut() -> ExecutionFence,
    ) -> Result<CurrentCognitionExpiryOutcomeV1<'p>, RootModuleExecutionErrorV1<O::Error>> {
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            source
                .validate(parent, current, self.ctx.transaction, common, &live_owner())
                .map_err(RootModuleExecutionErrorV1::Body)?;
            written
                .validate_evidence(self.ctx.transaction, common, &live_owner())
                .map_err(RootModuleExecutionErrorV1::Body)?;
            self.check_current_disabled_cognition_v1(parent, current, work, common, &live_owner())?;
            common
                .charge(
                    1,
                    std::mem::size_of::<CurrentCognitionExpiryOutcomeV1>() as u64,
                )
                .map_err(|e| RootModuleExecutionErrorV1::Body(formal(e)))?;
            let fee = common
                .memory()
                .reserve(
                    std::mem::size_of::<CurrentCognitionExpiryOutcomeV1>(),
                    MemoryClass::Ordinary,
                )
                .map_err(ValueTreeError::Memory)?
                .retain(());
            unchanged(continued, self.ctx.transaction, common, &live_owner(), fee)
                .map_err(RootModuleExecutionErrorV1::Body)
        }));
        self.finish_disabled_result(result)
    }
    pub(in crate::world) fn validate_disabled_normal_cognition_current_v1(
        &mut self,
        parent: &ReservedValue<BoundNormalTickParentV1<'_>>,
        current: &ReservedValue<CompleteCognitionCurrentQualificationV3>,
        source: &ReservedValue<PaidDisabledNormalCognitionSourceV1>,
        work: &mut CurrentCognitionExpiryWorkV1,
        common: &mut FormalBoundedContextV1<'_>,
        live: &ExecutionFence,
    ) -> Result<(), RootModuleExecutionErrorV1<O::Error>> {
        source
            .validate(parent, current, self.ctx.transaction, common, live)
            .map_err(RootModuleExecutionErrorV1::Body)?;
        self.check_current_disabled_cognition_v1(parent, current, work, common, live)
    }
    fn finish_disabled_result<T>(
        &mut self,
        result: std::thread::Result<Result<T, RootModuleExecutionErrorV1<O::Error>>>,
    ) -> Result<T, RootModuleExecutionErrorV1<O::Error>> {
        match result {
            Ok(Ok(value)) => Ok(value),
            Ok(Err(error)) => {
                let _: Result<(), ExecutionError> = self
                    .ctx
                    .transaction
                    .contribute(|_| Err(ExecutionError::Aborted));
                Err(error)
            }
            Err(panic) => {
                let _: Result<(), ExecutionError> = self
                    .ctx
                    .transaction
                    .contribute(|_| Err(ExecutionError::Aborted));
                std::panic::resume_unwind(panic)
            }
        }
    }
}
