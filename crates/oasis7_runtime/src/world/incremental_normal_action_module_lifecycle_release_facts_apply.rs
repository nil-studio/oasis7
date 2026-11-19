//! Current Apply observers: actual epoch provenance and all current approver
//! presences are retained without imposing the two touched-agent slot limit.
use super::super::*;
use super::super::{release_apply_rules::*, release_rules::*};
use super::current::{CurrentModuleReleaseFactsV1, CurrentReleaseFactsPartsV1};
use crate::world::{
    incremental_module_invocation::AcceptedInvocationDirectoryV1,
    incremental_normal_action_factory_schedule::{
        FactoryScheduleReadErrorV1, with_module_lifecycle_read_io_v1,
    },
};
use oasis7_formal_protocol::incremental_value::ValueNodeReader;
use oasis7_world_state::{
    incremental_field_overlay::WorldFieldOverlayContextV1,
    incremental_field_read::WorldFieldReadContextV1, state::ModuleReleaseRequestState,
};
use std::sync::Arc;
type Error = FactoryScheduleReadErrorV1;
struct CurrentApplyFactsV1<'a, 's, 't, 'v, 'm, 'p, 'b, 'mw, M, P, R, E, C, PE, PC> {
    inner: CurrentModuleReleaseFactsV1<'a, 's, 't, 'v, 'm, 'p, 'b, 'mw, M, P, R, E, C, PE, PC>,
    operator: String,
    epochs: Vec<Arc<crate::world::PaidCurrentModuleEpochSnapshotV1>>,
    _fee: MemoryReservation,
}
impl<M, P, R, E, C, PE, PC> sealed::Sealed
    for CurrentApplyFactsV1<'_, '_, '_, '_, '_, '_, '_, '_, M, P, R, E, C, PE, PC>
{
}
impl<M, P, R, E, C, PE, PC> ModuleReleaseBusinessFactsV1
    for CurrentApplyFactsV1<'_, '_, '_, '_, '_, '_, '_, '_, M, P, R, E, C, PE, PC>
where
    M: MerkleNodeReader,
    P: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    PE: FnMut() -> Option<ValueTreeError>,
    PC: FnMut() -> bool,
{
    type Error = Error;
    fn agent_exists(&mut self, id: &str) -> Result<bool, Error> {
        if id == self.operator {
            return self.inner.agent_exists(id);
        }
        let inner = &mut self.inner;
        let s = inner.sources;
        let live = inner.live;
        with_module_lifecycle_read_io_v1(
            s.families.roots,
            inner.ctx,
            inner.parent,
            inner.factory,
            inner.common,
            inner.module,
            |ctx, parent, factory, _| {
                let guard = ctx.transaction.guard();
                let value =
                    oasis7_world_state::incremental_agent_subtree::read_current_agent_presence_v1(
                        id,
                        s.factory.lifecycle.agents,
                        s.families.roots,
                        ctx,
                        parent,
                        &mut factory.lifecycle.pending_entries_remaining,
                        &guard,
                        live,
                    )
                    .map_err(overlay_error)?;
                Ok(value!=oasis7_world_state::incremental_agent_subtree::CurrentAgentPresenceV1::Absent)
            },
        )
    }
    fn artifact_exists(&mut self, hash: &str) -> Result<bool, Error> {
        self.inner.artifact_exists(hash)
    }
    fn owner(&mut self, hash: &str) -> Result<Option<String>, Error> {
        self.inner.owner(hash)
    }
    fn frozen_admission(&mut self, m: &crate::ModuleManifest) -> Result<(), Error> {
        self.inner.frozen_admission(m)
    }
    fn request(&mut self, id: u64) -> Result<Option<ModuleReleaseRequestSnapshotV1>, Error> {
        self.inner.request(id)
    }
    fn next_request_id(&mut self) -> Result<u64, Error> {
        self.inner.next_request_id()
    }
    fn node_identity_present(&mut self, id: &str) -> Result<bool, Error> {
        self.inner.node_identity_present(id)
    }
    fn role_membership(
        &mut self,
        a: &str,
        r: &str,
    ) -> Result<
        oasis7_world_state::incremental_authorization_mapsets::AuthorizationMembershipV1,
        Error,
    > {
        self.inner.role_membership(a, r)
    }
    fn profile_changes(
        &mut self,
        p: &crate::ModuleProfileChanges,
    ) -> Result<Result<(), String>, Error> {
        self.inner.profile_changes(p)
    }
    fn shadow_hash(
        &mut self,
        m: &crate::ModuleManifest,
        a: bool,
        p: &crate::ModuleProfileChanges,
    ) -> Result<Result<String, String>, Error> {
        self.inner.shadow_hash(m, a, p)
    }
}
impl<M, P, R, E, C, PE, PC> ModuleReleaseApplyFactsV1
    for CurrentApplyFactsV1<'_, '_, '_, '_, '_, '_, '_, '_, M, P, R, E, C, PE, PC>
where
    M: MerkleNodeReader,
    P: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    PE: FnMut() -> Option<ValueTreeError>,
    PC: FnMut() -> bool,
{
    fn reserve_apply_query(&mut self, operator: &str, id: u64) -> Result<(), Error> {
        self.inner.reserve(&(operator, id))
    }
    fn epoch_snapshot(&mut self) -> Result<ModuleReleaseEpochSnapshotV1, Error> {
        let inner = &mut self.inner;
        let value = crate::world::read_current_module_epoch_snapshot_v1(
            inner.sources,
            inner.ctx,
            inner.parent,
            inner.factory,
            inner.module,
            inner.common,
            inner.live,
        )
        .map_err(|error| match error {
            crate::world::CurrentModuleGovernanceFinalityErrorV1::Body(error) => Error::Body(error),
            crate::world::CurrentModuleGovernanceFinalityErrorV1::World { error, fees } => {
                Error::Body(BodyMutationError::Original(Box::new(EpochReadCause {
                    error,
                    _fees: fees,
                })))
            }
        })?;
        let bytes = std::mem::size_of::<crate::world::PaidCurrentModuleEpochSnapshotV1>()
            .checked_add(2 * std::mem::size_of::<usize>())
            .and_then(|n| n.checked_add(256))
            .ok_or_else(invalid)?;
        pay(inner.common, bytes)?;
        let mut extra = inner
            .common
            .memory()
            .reserve(bytes, MemoryClass::Ordinary)
            .map_err(ValueTreeError::Memory)?;
        self._fee
            .merge(&mut extra)
            .map_err(BodyMutationError::Invalid)?;
        if self.epochs.len() == self.epochs.capacity() {
            return Err(invalid().into());
        }
        let owner = Arc::new(value);
        self.epochs.push(owner.clone());
        Ok(ModuleReleaseEpochSnapshotV1::Current(owner))
    }
    fn reserve_attestation_evidence(
        &mut self,
        request: &ModuleReleaseRequestState,
        snapshot: &crate::GovernanceFinalityEpochSnapshot,
    ) -> Result<(), Error> {
        self.inner.reserve(&(request, snapshot))?;
        // Original set/map joins compare borrowed UTF8 strings. Fund actual rows and
        // a conservative logarithmic comparison depth before those comparisons.
        let n = request
            .attestations
            .len()
            .checked_add(snapshot.signer_node_ids.len())
            .and_then(|n| n.checked_add(snapshot.validator_stakes.len()))
            .and_then(|n| n.checked_add(1))
            .ok_or_else(invalid)?;
        let levels = usize::try_from(usize::BITS - n.leading_zeros() + 4).map_err(|_| invalid())?;
        let mut strings = 0usize;
        for id in &snapshot.signer_node_ids {
            strings = strings.checked_add(id.len()).ok_or_else(invalid)?;
        }
        for row in request.attestations.values() {
            for text in [
                &row.signer_node_id,
                &row.wasm_hash,
                &row.source_hash,
                &row.build_manifest_hash,
                &row.builder_image_digest,
                &row.container_platform,
                &row.canonicalizer_version,
            ] {
                strings = strings.checked_add(text.len()).ok_or_else(invalid)?;
            }
        }
        let visits = n
            .checked_mul(levels)
            .and_then(|n| n.checked_mul(16))
            .ok_or_else(invalid)?;
        let bytes = n
            .checked_mul(64)
            .and_then(|n| strings.checked_add(n))
            .and_then(|n| n.checked_mul(levels))
            .and_then(|n| n.checked_mul(16))
            .ok_or_else(invalid)?;
        self.inner
            .common
            .charge(
                u64::try_from(visits).map_err(|_| invalid())?,
                u64::try_from(bytes).map_err(|_| invalid())?,
            )
            .map_err(|e| BodyMutationError::Original(Box::new(e)))?;
        Ok(())
    }
}
pub(in crate::world::incremental_normal_action_module_lifecycle) enum CurrentReleaseApplyDecisionV1
{
    Ready(super::ValidatedCurrentReleaseApplyV1),
    Rejected(super::super::release_plan::PreparedCurrentReleaseRejectionV1),
}
#[allow(clippy::too_many_arguments)]
pub(in crate::world::incremental_normal_action_module_lifecycle) fn prepare_current_release_apply_v1<
    M,
    P,
    R,
    E,
    C,
    PE,
    PC,
>(
    selected: &SelectedPendingActionV1,
    outcome: &ReservedValue<RootRulePhaseOutcomeV1>,
    accepted: &crate::world::incremental_module_invocation::SelectedAcceptedDispositionV1,
    sources: &ModuleLifecycleSourcesV1<'_>,
    ctx: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    parent: &mut WorldFieldReadContextV1<'_, P, R, PE, PC>,
    factory: &mut FactoryScheduleWorkV1,
    module: &mut ModuleLifecycleWorkV1<'_>,
    common: &mut FormalBoundedContextV1<'_>,
    directory: &ReservedValue<AcceptedInvocationDirectoryV1>,
    now: u64,
    live: &ExecutionFence,
) -> Result<CurrentReleaseApplyDecisionV1, BodyMutationError>
where
    M: MerkleNodeReader,
    P: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    PE: FnMut() -> Option<ValueTreeError>,
    PC: FnMut() -> bool,
{
    let run = (|| -> Result<CurrentReleaseApplyDecisionV1, Error> {
        accepted.validate(selected, ctx.transaction, common.memory())?;
        let action = outcome
            .override_action_for_selected_v1(selected, ctx.transaction)?
            .unwrap_or(&selected.envelope().action);
        let (operator, id) = match action {
            crate::Action::ModuleReleaseApply {
                operator_agent_id,
                request_id,
            }
            | crate::Action::ModuleReleaseApplyWithFinality {
                operator_agent_id,
                request_id,
                ..
            } => (operator_agent_id.as_str(), *request_id),
            _ => return Err(invalid().into()),
        };
        let bytes = operator
            .len()
            .checked_add(std::mem::size_of::<super::ValidatedCurrentReleaseApplyV1>())
            .and_then(|n| n.checked_add(std::mem::size_of::<CurrentReleaseFactsPartsV1>()))
            .and_then(|n| n.checked_add(4096))
            .ok_or_else(invalid)?;
        pay(common, bytes)?;
        let fee = common
            .memory()
            .reserve(bytes, MemoryClass::Ordinary)
            .map_err(ValueTreeError::Memory)?;
        let inner = CurrentModuleReleaseFactsV1::new(
            sources, ctx, parent, factory, module, common, directory, live,
        )?;
        let mut facts = CurrentApplyFactsV1 {
            inner,
            operator: operator.to_owned(),
            epochs: Vec::with_capacity(1),
            _fee: fee,
        };
        facts.inner.reserve(action)?;
        let reviewed = prepare_module_release_apply_v1(&mut facts, operator, id)?;
        let CurrentApplyFactsV1 {
            inner,
            epochs,
            _fee,
            ..
        } = facts;
        let mut parts = inner.into_parts();
        parts.epochs = epochs;
        match reviewed {
            ModuleReleaseApplyDecisionV1::Rejected(reason) => {
                let fee = parts.output_fee.take().ok_or_else(invalid)?;
                Ok(CurrentReleaseApplyDecisionV1::Rejected(
                    super::super::release_plan::bind_current_release_apply_rejection_v1(
                        CurrentReleaseApplyRejectionV1 {
                            reason: fee.retain(reason),
                            parts: Box::new(parts),
                            layout: _fee,
                            selected: selected as *const _ as usize,
                            outcome: outcome as *const _ as usize,
                            guard: ctx.transaction.guard(),
                            core: common.work().identity(),
                            now,
                        },
                        selected,
                        outcome,
                        ctx.transaction,
                        common,
                        now,
                        live,
                    )?,
                ))
            }
            ModuleReleaseApplyDecisionV1::Ready(snapshot) => {
                let request = match snapshot {
                    ModuleReleaseRequestSnapshotV1::Current(value) => value,
                    ModuleReleaseRequestSnapshotV1::Legacy(_) => return Err(invalid().into()),
                };
                let before = parts.request.take().ok_or_else(invalid)?;
                let operator = parts.agents[0].take().ok_or_else(invalid)?;
                Ok(CurrentReleaseApplyDecisionV1::Ready(
                    super::ValidatedCurrentReleaseApplyV1 {
                        before,
                        request,
                        operator,
                        outer: id,
                        selected: selected as *const _ as usize,
                        outcome: outcome as *const _ as usize,
                        action_id: selected.envelope().id,
                        guard: ctx.transaction.guard(),
                        core: common.work().identity(),
                        now,
                        _facts: Box::new(parts),
                        _fees: Vec::new(),
                        _layout: _fee,
                    },
                ))
            }
        }
    })();
    match run {
        Ok(value) => Ok(value),
        Err(Error::Body(error)) => Err(error),
        Err(Error::World(error)) => Err(BodyMutationError::Original(Box::new(ApplySourceCauseV1(
            error,
        )))),
    }
}
#[derive(Debug)]
struct ApplySourceCauseV1(crate::WorldError);
impl std::fmt::Display for ApplySourceCauseV1 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}", self.0)
    }
}
impl std::error::Error for ApplySourceCauseV1 {}

pub(in crate::world::incremental_normal_action_module_lifecycle) struct CurrentReleaseApplyRejectionV1
{
    reason: ReservedValue<crate::RejectReason>,
    parts: Box<CurrentReleaseFactsPartsV1>,
    layout: MemoryReservation,
    selected: usize,
    outcome: usize,
    guard: PreparedGuard,
    core: WorkIdentityV1,
    now: u64,
}
impl CurrentReleaseApplyRejectionV1 {
    pub(in crate::world::incremental_normal_action_module_lifecycle) fn validate_provenance<
        M: MerkleNodeReader,
    >(
        &self,
        selected: &SelectedPendingActionV1,
        outcome: &ReservedValue<RootRulePhaseOutcomeV1>,
        tx: &ExecutionTransaction<'_, M>,
        memory: &oasis7_proto::memory_budget::MemoryBudget,
        core: WorkIdentityV1,
        now: u64,
        live: &ExecutionFence,
    ) -> Result<(), BodyMutationError> {
        selected.validate(tx)?;
        tx.validate_transaction_identity(&self.guard)?;
        tx.validate_guard(&tx.guard(), live)?;
        if self.selected != selected as *const _ as usize
            || self.outcome != outcome as *const _ as usize
            || self.core != core
            || self.now != now
            || !self.reason.uses_budget(memory)
            || !self.layout.uses_budget(memory)
            || !self.parts.slots.uses_budget(memory)
            || self.parts.fees.iter().any(|f| !f.uses_budget(memory))
        {
            return Err(invalid());
        }
        for owner in &self.parts.epochs {
            owner.validate(tx, memory, core, now, live)?;
        }
        for owner in &self.parts.profiles {
            owner.validate_provenance(tx, memory, core, live)?;
        }
        Ok(())
    }
    pub(in crate::world::incremental_normal_action_module_lifecycle) fn into_parts(
        self,
    ) -> (
        ReservedValue<crate::RejectReason>,
        Box<CurrentReleaseFactsPartsV1>,
        MemoryReservation,
    ) {
        (self.reason, self.parts, self.layout)
    }
}

struct EpochReadCause {
    error: crate::WorldError,
    _fees: crate::world::ModuleCapabilityAdmissionFeesV1,
}
impl std::fmt::Display for EpochReadCause {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}", self.error)
    }
}
impl std::error::Error for EpochReadCause {}

impl std::fmt::Debug for EpochReadCause {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}", self.error)
    }
}
