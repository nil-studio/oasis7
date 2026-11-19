//! Recovery of the one retained normal output. Cold work is admitted before
//! Node execution and remains the same owned work across retries and Warm ticks.
use super::*;
use crate::iws_host_runtime::cold_recovery::{
    ColdEndpointReopenRefsV1, HostColdRecoveryContextV1, HostColdRecoveryWorkV1,
    HostColdReopenInputV1, reopen_host_bounded_v1,
};
use oasis7_distfs::controlled_authority::formal::endpoint::{
    FormalEndpointPairPublicationRefsV1, HostAcceptedParentReadViewV1, IwsColdParentHintsV1,
    NormalReadyCandidateReadViewV1,
};
use oasis7_formal_protocol::{
    controlled_authority::formal::bounded_context::{FormalBoundedContextV1, WorkIdentityV1},
    incremental_execution::ExecutionFactKindV1,
    incremental_material::FactLocationV1,
};
use oasis7_runtime::{AuthorizationRootCacheLimitsV1, WorldRuntimeMemoryLimits};
use std::{collections::BTreeMap, path::Path};

pub(crate) struct NormalColdRecoveryInputsV1<'a> {
    pub endpoints: [ColdEndpointReopenRefsV1<'a>; 2],
    pub authorization_hint_path: &'a Path,
    pub projection_limits: AuthorizationRootCacheLimitsV1,
    pub process: crate::iws_host_runtime::cold::ProcessInput<'a>,
    pub configuration: &'a WorldRuntimeMemoryLimits,
    pub worker: &'a WorkerLimitsV1,
    pub transaction: oasis7_formal_protocol::incremental_execution::TransactionLimits,
    pub mailbox_hints: &'a BTreeMap<[u8; 32], FactLocationV1>,
}

#[derive(Clone, Copy)]
struct ColdTargetV1 {
    record: [u8; 32],
    body: [u8; 32],
    roots: [u8; 32],
    namespaces: ((u64, u64), (u64, u64)),
    runtime: FactLocationV1,
    cognition: FactLocationV1,
}
impl ColdTargetV1 {
    fn parent(view: HostAcceptedParentReadViewV1<'_>) -> Self {
        Self {
            record: view.head().verified_record_id(),
            body: view.fence().parent_commit,
            roots: view.fence().parent_roots,
            namespaces: view.namespace_identities(),
            runtime: view.runtime().occurrence().location(),
            cognition: view.cognition().selected().occurrence().location(),
        }
    }
    fn child(view: &NormalReadyCandidateReadViewV1<'_>) -> Self {
        Self {
            record: view.core().candidate_record_id(),
            body: view.core().catalog().entry.commit,
            roots: view.core().body().roots,
            namespaces: view.namespace_identities(),
            runtime: view.storage().runtime().occurrence().location(),
            cognition: view
                .storage()
                .cognition()
                .selected()
                .occurrence()
                .location(),
        }
    }
    fn hints(self) -> IwsColdParentHintsV1 {
        IwsColdParentHintsV1 {
            runtime: self.runtime,
            cognition: self.cognition,
        }
    }
    fn validate_host(
        self,
        host: &IwsHostRuntimeV1,
        common: &mut FormalBoundedContextV1<'_>,
    ) -> Result<bool, IwsHostRuntimeErrorV1> {
        let Some(current) = host.current.current() else {
            return Ok(false);
        };
        current.source.validate_context(common)?;
        common.charge(1, 128)?;
        let fence = current.source.fence();
        if current.source.record_id() != self.record
            || fence.parent_commit != self.body
            || fence.parent_roots != self.roots
            || (
                current.source.page_namespace_identity(),
                current.source.native_namespace_identity(),
            ) != self.namespaces
        {
            return Err(IwsHostRuntimeErrorV1::Move(ExecutionError::ParentConflict));
        }
        current
            .resident
            .as_ref()
            .ok_or_else(|| FormalError::Invalid("cold child genuine resident missing".into()))?
            .validate_live_binding_v1(self.record, &fence, common)
            .map_err(IwsHostRuntimeErrorV1::NormalSource)?;
        Ok(true)
    }
}
struct ColdReadyBindingsV1 {
    parent: [ColdTargetV1; 2],
    child: [ColdTargetV1; 2],
    mailbox: ReservedValue<Vec<FactLocationV1>>,
}
struct ColdRecoveryStateV1 {
    endpoints: [HostColdRecoveryWorkV1; 2],
    bindings: Option<ColdReadyBindingsV1>,
    core: WorkIdentityV1,
}
pub struct NormalColdRecoveryWorkV1 {
    // The real box drops before the fee for that box. Nested source owners
    // retain their own original fees and work identities independently.
    state: Box<ColdRecoveryStateV1>,
    fee: MemoryReservation,
}
impl NormalColdRecoveryWorkV1 {
    pub fn new_bounded(
        endpoints: [HostColdRecoveryWorkV1; 2],
        common: &mut FormalBoundedContextV1<'_>,
    ) -> Result<Self, IwsHostRuntimeErrorV1> {
        for endpoint in &endpoints {
            endpoint.validate_context(common)?;
        }
        let bytes = size_of::<Self>() + size_of::<ColdRecoveryStateV1>();
        common.charge(1, bytes as u64)?;
        let fee = common
            .memory()
            .reserve(bytes, MemoryClass::Ordinary)
            .map_err(|error| IwsHostRuntimeErrorV1::Move(error.into()))?;
        Ok(Self {
            state: Box::new(ColdRecoveryStateV1 {
                endpoints,
                bindings: None,
                core: common.work().identity(),
            }),
            fee,
        })
    }
    pub(crate) fn validate_context(
        &self,
        common: &mut FormalBoundedContextV1<'_>,
    ) -> Result<(), IwsHostRuntimeErrorV1> {
        common.consume_steps(1)?;
        if self.state.core != common.work().identity() || !self.fee.uses_budget(common.memory()) {
            return Err(FormalError::Invalid("normal cold original work Core/pool".into()).into());
        }
        for endpoint in &self.state.endpoints {
            endpoint.validate_context(common)?;
        }
        Ok(())
    }
    pub(super) fn completed(&mut self) {
        self.state.bindings = None;
    }
    #[cfg(any(test, feature = "iws-host-native-test-support"))]
    pub(crate) fn remaining_for_fixture_v1(&self) -> [u64; 6] {
        let [primary, replica] = &self.state.endpoints;
        [
            primary.cold.objects_physical.reads_remaining,
            primary.cold.mailbox_occurrences,
            primary.source.history_hash_bytes,
            replica.cold.objects_physical.reads_remaining,
            replica.cold.mailbox_occurrences,
            replica.source.history_hash_bytes,
        ]
    }

    pub(super) fn bind_ready_bounded(
        &mut self,
        ready: &PaidNormalReadyPairV1,
        memory: &MemoryBudget,
        archive: &FormalArchiveReadWorkV1,
        cancelled: &AtomicBool,
    ) -> Result<(), IwsHostRuntimeErrorV1> {
        self.validate_context(
            &mut archive.bounded_context(memory, &mut || cancelled.load(Ordering::Acquire)),
        )?;
        if self.state.bindings.is_some() {
            return Ok(());
        }
        let parent = [
            ready.with_primary_parent_read_view_bounded(
                memory,
                archive,
                &mut || cancelled.load(Ordering::Acquire),
                |view| Ok::<_, IwsHostRuntimeErrorV1>(ColdTargetV1::parent(view)),
            )?,
            ready.with_replica_parent_read_view_bounded(
                memory,
                archive,
                &mut || cancelled.load(Ordering::Acquire),
                |view| Ok::<_, IwsHostRuntimeErrorV1>(ColdTargetV1::parent(view)),
            )?,
        ];
        let (primary, mailbox) = ready.with_primary_candidate_read_view_bounded(
            memory,
            archive,
            &mut || cancelled.load(Ordering::Acquire),
            |view| -> Result<_, IwsHostRuntimeErrorV1> {
                let facts = &view.core().facts().facts;
                let count = facts.len();
                let bytes = count
                    .checked_mul(size_of::<FactLocationV1>())
                    .and_then(|n| n.checked_add(size_of::<Vec<FactLocationV1>>()))
                    .ok_or_else(|| FormalError::Invalid("cold candidate locator layout".into()))?;
                let comparisons = count
                    .checked_mul(count)
                    .and_then(|n| n.checked_mul(2))
                    .ok_or_else(|| FormalError::Invalid("cold candidate locator work".into()))?;
                let steps = comparisons
                    .checked_add(count)
                    .ok_or_else(|| FormalError::Invalid("cold candidate locator steps".into()))?;
                let work_bytes = comparisons
                    .checked_mul(64)
                    .and_then(|n| n.checked_add(bytes))
                    .ok_or_else(|| FormalError::Invalid("cold candidate locator bytes".into()))?;
                archive.charge(steps as u64, work_bytes as u64, &mut || {
                    cancelled.load(Ordering::Acquire)
                })?;
                let fee = memory
                    .reserve(bytes, MemoryClass::Ordinary)
                    .map_err(|error| IwsHostRuntimeErrorV1::Move(error.into()))?;
                let mut locations = Vec::new();
                locations.try_reserve_exact(count).map_err(|_| {
                    FormalError::Invalid("cold candidate locator allocation".into())
                })?;
                if locations.capacity() > count {
                    return Err(
                        FormalError::Invalid("cold candidate locator capacity".into()).into(),
                    );
                }
                let height = view.storage().runtime().occurrence().location().height;
                for (ordinal, fact) in facts.iter().enumerate() {
                    if fact.kind == ExecutionFactKindV1::Event {
                        locations.push(FactLocationV1 {
                            height,
                            ordinal: ordinal as u64,
                            original: fact.id,
                        });
                    }
                }
                locations.sort_unstable_by_key(|location| location.original);
                if view.parent_record_id() != parent[0].record {
                    return Err(IwsHostRuntimeErrorV1::Move(ExecutionError::ParentConflict));
                }
                Ok((ColdTargetV1::child(&view), fee.retain(locations)))
            },
        )?;
        let replica = ready.with_replica_candidate_read_view_bounded(
            memory,
            archive,
            &mut || cancelled.load(Ordering::Acquire),
            |view| -> Result<_, IwsHostRuntimeErrorV1> {
                if view.parent_record_id() != parent[1].record {
                    return Err(IwsHostRuntimeErrorV1::Move(ExecutionError::ParentConflict));
                }
                Ok(ColdTargetV1::child(&view))
            },
        )?;
        if primary.record != replica.record
            || primary.body != replica.body
            || primary.roots != replica.roots
            || parent[0].record != parent[1].record
            || primary.namespaces != parent[0].namespaces
            || replica.namespaces != parent[1].namespaces
        {
            return Err(IwsHostRuntimeErrorV1::Move(ExecutionError::ParentConflict));
        }
        self.state.bindings = Some(ColdReadyBindingsV1 {
            parent,
            child: [primary, replica],
            mailbox,
        });
        Ok(())
    }
}

/// Charge the maximum finite locator search before entering the exclusive
/// Cold scalar child; the callback then spends only this prepaid search lane.
fn admit_hint_search(
    occurrences: u64,
    candidate_len: usize,
    old_len: usize,
    common: &mut FormalBoundedContextV1<'_>,
) -> Result<PaidHintSearchV1, IwsHostRuntimeErrorV1> {
    let depth = |len: usize| usize::BITS as u64 - len.leading_zeros() as u64 + 1;
    let steps = occurrences
        .checked_mul(depth(candidate_len) + depth(old_len) * 12)
        .ok_or_else(|| FormalError::Invalid("cold hint work overflow".into()))?;
    let bytes = steps
        .checked_mul(64)
        .ok_or_else(|| FormalError::Invalid("cold hint byte work overflow".into()))?;
    common.charge(steps, bytes)?;
    let fee = common
        .memory()
        .reserve(512, MemoryClass::Ordinary)
        .map_err(|error| IwsHostRuntimeErrorV1::Move(error.into()))?;
    Ok(PaidHintSearchV1 {
        remaining: occurrences,
        _fee: fee,
    })
}
struct PaidHintSearchV1 {
    remaining: u64,
    _fee: MemoryReservation,
}
fn lookup_hint(
    id: &[u8; 32],
    candidate: &[FactLocationV1],
    old: &BTreeMap<[u8; 32], FactLocationV1>,
    remaining: &mut u64,
) -> Result<FactLocationV1, String> {
    *remaining = remaining
        .checked_sub(1)
        .ok_or_else(|| "cold original hint search exhausted".to_owned())?;
    candidate
        .binary_search_by_key(id, |location| location.original)
        .ok()
        .map(|index| candidate[index])
        .or_else(|| old.get(id).copied())
        .ok_or_else(|| "cold original mailbox hint unavailable".to_owned())
}

impl NormalColdRecoveryWorkV1 {
    fn restore_ready_parent_bounded(
        &mut self,
        ready: &mut PaidNormalReadyPairV1,
        inputs: &NormalColdRecoveryInputsV1<'_>,
        context: HostColdRecoveryContextV1<'_>,
    ) -> Result<(), IwsHostRuntimeErrorV1> {
        let HostColdRecoveryContextV1 {
            memory,
            archive,
            cancelled,
            compiler: _,
        } = context;
        self.validate_context(
            &mut archive.bounded_context(memory, &mut || cancelled.load(Ordering::Acquire)),
        )?;
        let bindings =
            self.state.bindings.as_ref().ok_or_else(|| {
                FormalError::Invalid("cold Ready original bindings missing".into())
            })?;
        let [primary, replica] = &mut self.state.endpoints;
        let occurrences = primary
            .cold
            .mailbox_occurrences
            .checked_add(replica.cold.mailbox_occurrences)
            .ok_or_else(|| FormalError::Invalid("cold parent mailbox work overflow".into()))?;
        let mut search = admit_hint_search(
            occurrences,
            bindings.mailbox.len(),
            inputs.mailbox_hints.len(),
            &mut archive.bounded_context(memory, &mut || cancelled.load(Ordering::Acquire)),
        )?;
        ready
            .recover_parent_sources_bounded_v1(
                [
                    (&mut primary.cold, &mut primary.source),
                    (&mut replica.cold, &mut replica.source),
                ],
                [bindings.parent[0].hints(), bindings.parent[1].hints()],
                FormalEndpointPairPublicationRefsV1 {
                    primary_provider: inputs.endpoints[0].provider,
                    replica_provider: inputs.endpoints[1].provider,
                    primary_policy: inputs.endpoints[0].policy,
                    replica_policy: inputs.endpoints[1].policy,
                    memory,
                    work: archive,
                },
                &mut |id| {
                    lookup_hint(
                        id,
                        &bindings.mailbox,
                        inputs.mailbox_hints,
                        &mut search.remaining,
                    )
                },
                &mut || cancelled.load(Ordering::Acquire),
            )
            .map_err(IwsHostRuntimeErrorV1::ColdSource)
    }

    fn restore_child_hosts_bounded(
        &mut self,
        hosts: [&mut IwsHostRuntimeV1; 2],
        inputs: &NormalColdRecoveryInputsV1<'_>,
        context: HostColdRecoveryContextV1<'_>,
    ) -> Result<(), IwsHostRuntimeErrorV1> {
        let HostColdRecoveryContextV1 {
            memory,
            archive,
            cancelled,
            compiler: _,
        } = context;
        self.validate_context(
            &mut archive.bounded_context(memory, &mut || cancelled.load(Ordering::Acquire)),
        )?;
        let bindings =
            self.state.bindings.as_ref().ok_or_else(|| {
                FormalError::Invalid("cold child original bindings missing".into())
            })?;
        for (index, (host, work)) in hosts.into_iter().zip(&mut self.state.endpoints).enumerate() {
            let target = bindings.child[index];
            if target.validate_host(
                host,
                &mut archive.bounded_context(memory, &mut || cancelled.load(Ordering::Acquire)),
            )? {
                continue;
            }
            let previous = host.continuation.as_ref().ok_or_else(|| {
                FormalError::Invalid("cold child original continuation missing".into())
            })?;
            if previous.record != bindings.parent[index].record
                || previous.identity != self.state.core
                || !previous.fee.uses_budget(memory)
            {
                return Err(IwsHostRuntimeErrorV1::Move(ExecutionError::ParentConflict));
            }
            let mut search = admit_hint_search(
                work.cold.mailbox_occurrences,
                bindings.mailbox.len(),
                inputs.mailbox_hints.len(),
                &mut archive.bounded_context(memory, &mut || cancelled.load(Ordering::Acquire)),
            )?;
            let restored = reopen_host_bounded_v1(
                HostColdReopenInputV1 {
                    endpoint: inputs.endpoints[index],
                    authorization_hint_path: inputs.authorization_hint_path,
                    projection_limits: inputs.projection_limits,
                    process: inputs.process,
                    configuration: inputs.configuration,
                    worker: inputs.worker,
                    transaction: inputs.transaction,
                    facts: target.hints(),
                },
                work,
                &mut |id| {
                    lookup_hint(
                        id,
                        &bindings.mailbox,
                        inputs.mailbox_hints,
                        &mut search.remaining,
                    )
                },
                context,
            )?;
            if !target.validate_host(
                &restored,
                &mut archive.bounded_context(memory, &mut || cancelled.load(Ordering::Acquire)),
            )? {
                return Err(
                    FormalError::Invalid("cold restored child was not installed".into()).into(),
                );
            }
            // Both qualification and resident binding precede this no-fail move.
            // A successful first side survives a later second-side refusal.
            *host = restored;
        }
        Ok(())
    }
}

impl NormalCommitFailureV1 {
    pub(crate) fn recover_cold_bounded_v1(
        mut self,
        primary: &mut IwsHostRuntimeV1,
        replica: &mut IwsHostRuntimeV1,
        inputs: &NormalColdRecoveryInputsV1<'_>,
        context: HostColdRecoveryContextV1<'_>,
    ) -> Result<NormalCommitWorkV1, Self> {
        if !matches!(
            self.phase,
            Some(
                Phase::ColdBeforePublication { .. }
                    | Phase::ColdPublication { .. }
                    | Phase::ColdAccepted { .. }
                    | Phase::ColdAfterPublication
            )
        ) {
            return Err(self);
        }
        let HostColdRecoveryContextV1 {
            memory,
            archive,
            cancelled,
            compiler: _,
        } = context;
        if let Err(error) = self.node.validate_context(
            &mut archive.bounded_context(memory, &mut || cancelled.load(Ordering::Acquire)),
        ) {
            self.cause = Some(error.into());
            return Err(self);
        }
        loop {
            self.cause = None;
            let phase = self.phase.take().expect("private normal Cold phase");
            match phase {
                Phase::ColdBeforePublication { mut ready } => {
                    if let Err(error) = self
                        .work
                        .cold
                        .restore_ready_parent_bounded(&mut ready, inputs, context)
                    {
                        self.cause = Some(error);
                        self.phase = Some(Phase::ColdBeforePublication { ready });
                        return Err(self);
                    }
                    match self.publish_ready_bounded(
                        ready,
                        FormalEndpointPairPublicationRefsV1 {
                            primary_provider: inputs.endpoints[0].provider,
                            replica_provider: inputs.endpoints[1].provider,
                            primary_policy: inputs.endpoints[0].policy,
                            replica_policy: inputs.endpoints[1].policy,
                            memory,
                            work: archive,
                        },
                        cancelled,
                    ) {
                        Ok(accepted) => {
                            self.phase = Some(Phase::ColdAccepted { accepted });
                        }
                        Err(failure) => {
                            self.phase = Some(Phase::ColdPublication { failure });
                            return Err(self);
                        }
                    }
                }
                Phase::ColdPublication { mut failure } => {
                    if failure.requires_authenticated_cold_reopen_v1() {
                        if let Err(error) = failure.return_observable_source_work_bounded(
                            &mut self.work.primary_source,
                            memory,
                            archive,
                            &mut || cancelled.load(Ordering::Acquire),
                        ) {
                            self.cause = Some(error.into());
                            self.phase = Some(Phase::ColdPublication { failure });
                            return Err(self);
                        }
                        drop(failure);
                        self.phase = Some(Phase::ColdAfterPublication);
                        continue;
                    }
                    match failure.recover_bounded(
                        inputs.endpoints[0].provider,
                        inputs.endpoints[1].provider,
                        inputs.endpoints[0].policy,
                        inputs.endpoints[1].policy,
                        memory,
                        archive,
                        &mut || cancelled.load(Ordering::Acquire),
                    ) {
                        Ok(accepted) => {
                            self.phase = Some(Phase::ColdAccepted { accepted });
                        }
                        Err(failure) => {
                            self.phase = Some(Phase::ColdPublication { failure });
                            return Err(self);
                        }
                    }
                }
                Phase::ColdAccepted { mut accepted } => {
                    if let Err(error) = accepted.return_source_work_bounded(
                        &mut self.work.primary_source,
                        &mut self.work.replica_source,
                        memory,
                        archive,
                        &mut || cancelled.load(Ordering::Acquire),
                    ) {
                        self.cause = Some(error.into());
                        self.phase = Some(Phase::ColdAccepted { accepted });
                        return Err(self);
                    }
                    // Both original accounts have returned before the real
                    // accepted handles close for independent child Cold.
                    drop(accepted);
                    self.phase = Some(Phase::ColdAfterPublication);
                }
                Phase::ColdAfterPublication => {
                    match self.work.cold.restore_child_hosts_bounded(
                        [primary, replica],
                        inputs,
                        context,
                    ) {
                        Ok(()) => {
                            self.work.cold.completed();
                            return Ok(self.work);
                        }
                        Err(error) => {
                            self.cause = Some(error);
                            self.phase = Some(Phase::ColdAfterPublication);
                            return Err(self);
                        }
                    }
                }
                phase => {
                    self.phase = Some(phase);
                    return Err(self);
                }
            }
        }
    }
}
