//! Scope-bound resident response locator and exact original ANY consumer.
//! Missing resident metadata is never an empty response history.
use super::*;
use super::{
    facts::CurrentCognitionRetryProbeV1,
    response_index::ResponseEnvelopeIndexV1,
    response_read::{CurrentCognitionResponseOriginalV1, CurrentCognitionResponseReadWorkV1},
};
use crate::world::{
    incremental_module_invocation::*, incremental_normal_sources::BoundNormalTickParentV1,
};
use oasis7_formal_protocol::{
    controlled_authority::formal::VerifiedFormalCommitV1,
    incremental_execution::SealedExecutionBindingV1,
    incremental_material::OriginalObjectStreamReader,
    incremental_state::{StatePageWriter, StateRootV1},
    incremental_value::{ValueNodeReader, ValueNodeWriter},
};
use oasis7_world_state::{
    incremental_cognition_history::{
        CheckedCognitionHistorySourceV3, CognitionHistoryFieldV1,
        PendingCurrentCognitionResponsesV1,
    },
    incremental_field_read::WorldFieldReadContextV1,
};
struct Rows {
    index: ResponseEnvelopeIndexV1,
    root: Option<StateRootV1>,
    count: u64,
}
fn layout(root: Option<&StateRootV1>, header: usize) -> Result<usize, BodyMutationError> {
    header
        .checked_add(root.map_or(0, |r| r.node_root.len()))
        .ok_or_else(|| bad("response locator root layout"))
}
fn clone_root(
    root: Option<&StateRootV1>,
    common: &mut FormalBoundedContextV1<'_>,
) -> Result<Option<StateRootV1>, BodyMutationError> {
    if let Some(root) = root {
        common
            .charge(1, root.node_root.len() as u64)
            .map_err(original)?;
    }
    Ok(root.cloned())
}
pub(in crate::world) struct AcceptedCognitionResponseLocatorV1 {
    rows: Rows,
    record: [u8; 32],
    body: [u8; 32],
    roots: [u8; 32],
    _fee: MemoryReservation,
}
pub(in crate::world) struct CurrentCognitionResponseLocatorV1 {
    rows: Rows,
    record: [u8; 32],
    body: [u8; 32],
    roots: [u8; 32],
    guard: PreparedGuard,
    core: WorkIdentityV1,
    _fee: MemoryReservation,
    _root_update_fee: Option<MemoryReservation>,
}
pub(in crate::world) struct FrozenCognitionResponseLocatorV1 {
    rows: Rows,
    guard: PreparedGuard,
    core: WorkIdentityV1,
    _fee: MemoryReservation,
}
pub(in crate::world) struct PreparedCognitionResponseLocatorV1 {
    inner: ReservedValue<AcceptedCognitionResponseLocatorV1>,
}
fn parent_rows<'a>(
    parent: &'a BoundNormalTickParentV1<'_>,
) -> Result<(Option<&'a StateRootV1>, u64), BodyMutationError> {
    let refs = parent.refs();
    match refs.cognition.indexes()[1].as_ref() {
        Some(index) => {
            if index.field() != CognitionHistoryFieldV1::Responses
                || index.head_record_id() != refs.head.verified_record_id()
                || index.roots_id() != refs.core.body().roots
            {
                return Err(bad("response locator actual accepted index"));
            }
            Ok((Some(index.root()), index.record_count()))
        }
        None => {
            if refs.cognition.disabled().is_none() {
                return Err(bad("response locator accepted source missing"));
            }
            Ok((None, 0))
        }
    }
}
impl AcceptedCognitionResponseLocatorV1 {
    pub(in crate::world) fn validate_parent(
        &self,
        parent: &BoundNormalTickParentV1<'_>,
        common: &mut FormalBoundedContextV1<'_>,
    ) -> Result<(), BodyMutationError> {
        parent.validate_context(common).map_err(original)?;
        common.charge(5, 256).map_err(original)?;
        let (root, count) = parent_rows(parent)?;
        let refs = parent.refs();
        if !self._fee.uses_budget(common.memory())
            || self.record != refs.head.verified_record_id()
            || self.body != refs.runtime.occurrence().head_body_id()
            || self.roots != refs.core.body().roots
            || self.rows.root.as_ref() != root
            || self.rows.count != count
        {
            return Err(bad("response locator accepted provenance"));
        }
        Ok(())
    }
    pub(in crate::world) fn begin_current<M: MerkleNodeReader>(
        &self,
        parent: &BoundNormalTickParentV1<'_>,
        responses: &PendingCurrentCognitionResponsesV1<'_>,
        tx: &ExecutionTransaction<'_, M>,
        common: &mut FormalBoundedContextV1<'_>,
        live: &ExecutionFence,
    ) -> Result<CurrentCognitionResponseLocatorV1, BodyMutationError> {
        self.validate_parent(parent, common)?;
        responses.validate_context(tx, common, live)?;
        if self.rows.root.as_ref() != Some(responses.root_data_v1())
            || self.rows.count != responses.count()
        {
            return Err(bad("response locator actual initial Current root"));
        }
        let n = layout(
            self.rows.root.as_ref(),
            size_of::<CurrentCognitionResponseLocatorV1>(),
        )?;
        common.charge(1, n as u64).map_err(original)?;
        let fee = common
            .memory()
            .reserve(n, MemoryClass::Ordinary)
            .map_err(ValueTreeError::Memory)?;
        Ok(CurrentCognitionResponseLocatorV1 {
            rows: Rows {
                index: self.rows.index.share(common)?,
                root: clone_root(self.rows.root.as_ref(), common)?,
                count: self.rows.count,
            },
            record: self.record,
            body: self.body,
            roots: self.roots,
            guard: tx.guard(),
            core: common.work().identity(),
            _fee: fee,
            _root_update_fee: None,
        })
    }
    pub(in crate::world) fn freeze_disabled<M: MerkleNodeReader>(
        &self,
        parent: &BoundNormalTickParentV1<'_>,
        tx: &ExecutionTransaction<'_, M>,
        common: &mut FormalBoundedContextV1<'_>,
        live: &ExecutionFence,
    ) -> Result<FrozenCognitionResponseLocatorV1, BodyMutationError> {
        self.validate_parent(parent, common)?;
        tx.validate_guard(&tx.guard(), live)?;
        if self.rows.root.is_some() || self.rows.count != 0 {
            return Err(bad("response locator actual disabled source"));
        }
        let n = size_of::<FrozenCognitionResponseLocatorV1>();
        common.charge(1, n as u64).map_err(original)?;
        let fee = common
            .memory()
            .reserve(n, MemoryClass::Ordinary)
            .map_err(ValueTreeError::Memory)?;
        Ok(FrozenCognitionResponseLocatorV1 {
            rows: Rows {
                index: self.rows.index.share(common)?,
                root: None,
                count: 0,
            },
            guard: tx.guard(),
            core: common.work().identity(),
            _fee: fee,
        })
    }
}
impl CurrentCognitionResponseLocatorV1 {
    pub(in crate::world) fn validate<M: MerkleNodeReader>(
        &self,
        parent: &BoundNormalTickParentV1<'_>,
        responses: &PendingCurrentCognitionResponsesV1<'_>,
        tx: &ExecutionTransaction<'_, M>,
        common: &mut FormalBoundedContextV1<'_>,
        live: &ExecutionFence,
    ) -> Result<(), BodyMutationError> {
        parent.validate_context(common).map_err(original)?;
        responses.validate_context(tx, common, live)?;
        tx.validate_exact_transaction_identity(&self.guard)?;
        common.charge(5, 256).map_err(original)?;
        let refs = parent.refs();
        if self.core != common.work().identity()
            || !self._fee.uses_budget(common.memory())
            || self
                ._root_update_fee
                .as_ref()
                .is_some_and(|f| !f.uses_budget(common.memory()))
            || self.record != refs.head.verified_record_id()
            || self.body != refs.runtime.occurrence().head_body_id()
            || self.roots != refs.core.body().roots
            || self.rows.root.as_ref() != Some(responses.root_data_v1())
            || self.rows.count != responses.count()
        {
            return Err(bad("response locator Current source frontier"));
        }
        Ok(())
    }
    pub(in crate::world) fn freeze<M: MerkleNodeReader>(
        &self,
        parent: &BoundNormalTickParentV1<'_>,
        responses: &PendingCurrentCognitionResponsesV1<'_>,
        tx: &ExecutionTransaction<'_, M>,
        common: &mut FormalBoundedContextV1<'_>,
        live: &ExecutionFence,
    ) -> Result<FrozenCognitionResponseLocatorV1, BodyMutationError> {
        self.validate(parent, responses, tx, common, live)?;
        let n = layout(
            self.rows.root.as_ref(),
            size_of::<FrozenCognitionResponseLocatorV1>(),
        )?;
        common.charge(1, n as u64).map_err(original)?;
        let fee = common
            .memory()
            .reserve(n, MemoryClass::Ordinary)
            .map_err(ValueTreeError::Memory)?;
        Ok(FrozenCognitionResponseLocatorV1 {
            rows: Rows {
                index: self.rows.index.share(common)?,
                root: clone_root(self.rows.root.as_ref(), common)?,
                count: self.rows.count,
            },
            guard: tx.guard(),
            core: common.work().identity(),
            _fee: fee,
        })
    }
}
impl FrozenCognitionResponseLocatorV1 {
    pub(in crate::world) fn prepare_candidate(
        self,
        sealed: &ReservedValue<SealedExecutionBindingV1>,
        roots: &oasis7_formal_protocol::incremental_execution::RootSetV1,
        source: &ReservedValue<CheckedCognitionHistorySourceV3>,
        common: &mut FormalBoundedContextV1<'_>,
    ) -> Result<PreparedCognitionResponseLocatorV1, BodyMutationError> {
        common.charge(5, 8192).map_err(original)?;
        sealed.validate_source_guard(&self.guard, common.memory())?;
        sealed.validate_after_roots(roots, common.memory())?;
        if self.core != common.work().identity()
            || !self._fee.uses_budget(common.memory())
            || !source.uses_budget(common.memory())
            || source.roots_id() != sealed.after_roots_digest()
        {
            return Err(bad("response locator checked candidate context"));
        }
        match &self.rows.root {
            None => {
                if source.disabled().is_none()
                    || source.indexes()[1].is_some()
                    || self.rows.count != 0
                {
                    return Err(bad("response locator checked disabled source"));
                }
            }
            Some(root) => {
                let index = source.indexes()[1]
                    .as_ref()
                    .ok_or_else(|| bad("response locator checked index missing"))?;
                if index.field() != CognitionHistoryFieldV1::Responses
                    || index.root() != root
                    || index.record_count() != self.rows.count
                    || index.candidate_record_id() != source.candidate_record_id()
                    || index.body_id() != source.body_id()
                    || index.roots_id() != source.roots_id()
                {
                    return Err(bad("response locator checked index frontier"));
                }
            }
        }
        let n = layout(
            self.rows.root.as_ref(),
            size_of::<AcceptedCognitionResponseLocatorV1>(),
        )?;
        common.charge(1, n as u64).map_err(original)?;
        let fee = common
            .memory()
            .reserve(n, MemoryClass::Ordinary)
            .map_err(ValueTreeError::Memory)?;
        // Move the real row graph/root; do not clone an unbounded event/response Vec.
        let outer = common
            .memory()
            .reserve(
                size_of::<AcceptedCognitionResponseLocatorV1>(),
                MemoryClass::Ordinary,
            )
            .map_err(ValueTreeError::Memory)?;
        Ok(PreparedCognitionResponseLocatorV1 {
            inner: outer.retain(AcceptedCognitionResponseLocatorV1 {
                rows: self.rows,
                record: source.candidate_record_id(),
                body: source.body_id(),
                roots: source.roots_id(),
                _fee: fee,
            }),
        })
    }
}
impl PreparedCognitionResponseLocatorV1 {
    pub(in crate::world) fn finish(
        self,
        head: &VerifiedFormalCommitV1,
    ) -> Result<ReservedValue<AcceptedCognitionResponseLocatorV1>, BodyMutationError> {
        if self.inner.record != head.verified_record_id() {
            return Err(bad("response locator genuine acceptance record"));
        }
        Ok(self.inner)
    }
}
pub(in crate::world) struct CurrentCognitionResponseMatchV1 {
    pub(super) original: CurrentCognitionResponseOriginalV1,
    root: StateRootV1,
    count: u64,
    guard: PreparedGuard,
    core: WorkIdentityV1,
    _fee: MemoryReservation,
}
impl CurrentCognitionResponseMatchV1 {
    pub(in crate::world) fn record(
        &self,
    ) -> &oasis7_world_state::cognition_recovery::CognitionResponseRecordV1 {
        &self.original.response
    }
    pub(in crate::world) fn validate<M: MerkleNodeReader>(
        &self,
        responses: &PendingCurrentCognitionResponsesV1<'_>,
        tx: &ExecutionTransaction<'_, M>,
        common: &mut FormalBoundedContextV1<'_>,
        live: &ExecutionFence,
    ) -> Result<(), BodyMutationError> {
        responses.validate_context(tx, common, live)?;
        tx.validate_exact_transaction_identity(&self.guard)?;
        common.charge(3, 256).map_err(original)?;
        if self.core != common.work().identity()
            || !self._fee.uses_budget(common.memory())
            || self.root != *responses.root_data_v1()
            || self.count != responses.count()
        {
            return Err(bad("response exact ANY source cut"));
        }
        Ok(())
    }
}
impl<M, R, E, C, W, F, O> RootModuleExecutionContextV1<'_, '_, '_, '_, M, R, E, C, W, F, O>
where
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    W: ValueNodeReader + ValueNodeWriter + StatePageWriter,
    F: FnMut(
        &mut dyn crate::world::incremental_body_event::AgentEventOriginalV1,
    ) -> Result<(), BodyMutationError>,
    O: OriginalObjectStreamReader,
{
    pub(in crate::world) fn seed_accepted_cognition_response_locator_v1<P, V, PE, PC>(
        &mut self,
        parent: &BoundNormalTickParentV1<'_>,
        read: &mut WorldFieldReadContextV1<'_, P, V, PE, PC>,
        work: &mut CurrentCognitionResponseReadWorkV1<'_>,
        common: &mut FormalBoundedContextV1<'_>,
        live: &mut dyn FnMut() -> ExecutionFence,
    ) -> Result<
        ReservedValue<AcceptedCognitionResponseLocatorV1>,
        RootModuleExecutionErrorV1<O::Error>,
    >
    where
        P: MerkleNodeReader,
        V: ValueNodeReader,
        PE: FnMut() -> Option<ValueTreeError>,
        PC: FnMut() -> bool,
    {
        parent.validate_context(common).map_err(original)?;
        self.ctx
            .transaction
            .validate_bootstrap_read_only_v1(&live())?;
        let (root, count) = parent_rows(parent)?;
        let phase_n = layout(root, size_of::<Rows>())?;
        common.charge(1, phase_n as u64).map_err(original)?;
        let _root_phase = common
            .memory()
            .reserve(phase_n, MemoryClass::Ordinary)
            .map_err(ValueTreeError::Memory)?;
        let root = clone_root(root, common)?;
        let mut index = ResponseEnvelopeIndexV1::empty();
        if root.is_some() {
            let refs = parent.refs();
            let mut responses = PendingCurrentCognitionResponsesV1::from_accepted(
                refs.cognition,
                refs.head,
                refs.roots,
                self.ctx.transaction,
                common,
                &live(),
            )?;
            if responses.count() != count || Some(responses.root_data_v1()) != root.as_ref() {
                return Err(bad("response locator cold actual frontier").into());
            }
            for ordinal in 0..count {
                common.consume_steps(1).map_err(original)?;
                let row = self.read_current_cognition_response_original_v1(
                    parent,
                    &mut responses,
                    ordinal,
                    read,
                    work,
                    common,
                    live,
                )?;
                index.replace_row(ordinal, Some(&row.response.envelope_digest), common)?;
            }
        }
        self.ctx
            .transaction
            .validate_bootstrap_read_only_v1(&live())?;
        let n = layout(
            root.as_ref(),
            size_of::<AcceptedCognitionResponseLocatorV1>(),
        )?;
        common.charge(1, n as u64).map_err(original)?;
        let fee = common
            .memory()
            .reserve(n, MemoryClass::Ordinary)
            .map_err(ValueTreeError::Memory)?;
        let outer = common
            .memory()
            .reserve(
                size_of::<AcceptedCognitionResponseLocatorV1>(),
                MemoryClass::Ordinary,
            )
            .map_err(ValueTreeError::Memory)?;
        let refs = parent.refs();
        let owner = outer.retain(AcceptedCognitionResponseLocatorV1 {
            rows: Rows { index, root, count },
            record: refs.head.verified_record_id(),
            body: refs.runtime.occurrence().head_body_id(),
            roots: refs.core.body().roots,
            _fee: fee,
        });
        owner.validate_parent(parent, common)?;
        Ok(owner)
    }
    pub(in crate::world) fn find_current_cognition_retry_response_v1<P, V, PE, PC>(
        &mut self,
        probe: &CurrentCognitionRetryProbeV1<'_, '_, '_, '_>,
        parent: &BoundNormalTickParentV1<'_>,
        locator: &CurrentCognitionResponseLocatorV1,
        responses: &mut PendingCurrentCognitionResponsesV1<'_>,
        read: &mut WorldFieldReadContextV1<'_, P, V, PE, PC>,
        work: &mut CurrentCognitionResponseReadWorkV1<'_>,
        canonical: &mut oasis7_wasm_abi::CanonicalCborWorkV1,
        common: &mut FormalBoundedContextV1<'_>,
        live: &mut dyn FnMut() -> ExecutionFence,
    ) -> Result<Option<CurrentCognitionResponseMatchV1>, RootModuleExecutionErrorV1<O::Error>>
    where
        P: MerkleNodeReader,
        V: ValueNodeReader,
        PE: FnMut() -> Option<ValueTreeError>,
        PC: FnMut() -> bool,
    {
        probe.validate(parent, self.ctx.transaction, common, &live())?;
        locator.validate(parent, responses, self.ctx.transaction, common, &live())?;
        let marker = probe
            .marker()
            .ok_or_else(|| bad("retry response requires actual selected marker"))?;
        let mut after = None;
        while let Some(ordinal) =
            locator
                .rows
                .index
                .next_ordinal(&marker.envelope_digest, after, common)?
        {
            common.consume_steps(1).map_err(original)?;
            if ordinal >= responses.count() {
                return Err(bad("response locator selected ordinal range").into());
            }
            let row = self.read_current_cognition_response_original_v1(
                parent, responses, ordinal, read, work, common, live,
            )?;
            common
                .charge(
                    1,
                    (row.response.envelope_digest.len() + marker.envelope_digest.len()) as u64,
                )
                .map_err(original)?;
            if row.response.envelope_digest != marker.envelope_digest {
                return Err(bad("response locator selected identity mismatch").into());
            }
            let matched = {
                let shared = RefCell::new(&mut *common);
                let source = work::Source::new(&shared, canonical);
                source.result(
                    crate::world::cognition_persistence::response_artifact_matches_with_work_v1(
                        &row.response,
                        &marker.envelope_digest,
                        probe.input().response_artifact_json(),
                        &source,
                    ),
                )?
            };
            if matched {
                let n = layout(
                    Some(responses.root_data_v1()),
                    size_of::<CurrentCognitionResponseMatchV1>(),
                )?;
                common.charge(1, n as u64).map_err(original)?;
                let fee = common
                    .memory()
                    .reserve(n, MemoryClass::Ordinary)
                    .map_err(ValueTreeError::Memory)?;
                let root = clone_root(Some(responses.root_data_v1()), common)?
                    .ok_or_else(|| bad("response matched actual root"))?;
                let owner = CurrentCognitionResponseMatchV1 {
                    original: row,
                    root,
                    count: responses.count(),
                    guard: self.ctx.transaction.guard(),
                    core: common.work().identity(),
                    _fee: fee,
                };
                owner.validate(responses, self.ctx.transaction, common, &live())?;
                return Ok(Some(owner));
            }
            after = Some(ordinal);
        }
        probe.validate(parent, self.ctx.transaction, common, &live())?;
        locator.validate(parent, responses, self.ctx.transaction, common, &live())?;
        Ok(None)
    }
}
#[path = "incremental_normal_cognition_commit_response_append.rs"]
mod append;
