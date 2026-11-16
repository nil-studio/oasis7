//! Private release publication plan. Storage writers consume exact Current
//! owners; this type deliberately has no free-form event/data constructor.
use super::current::CurrentModuleMarketAgentV1;
use super::*;
use oasis7_world_state::{
    incremental_world_native::CurrentFactoryNativeRecordV1,
    state::{ModuleReleaseManifestMappingState, ModuleReleaseRequestState},
};
pub(super) enum CurrentModuleReleaseMutationV1 {
    Request {
        outer: u64,
        before_request: ReservedValue<CurrentFactoryNativeRecordV1<ModuleReleaseRequestState>>,
        after_request: ReservedValue<ModuleReleaseRequestState>,
        before_mapping:
            ReservedValue<CurrentFactoryNativeRecordV1<ModuleReleaseManifestMappingState>>,
        /// None preserves actual mapping absence for RoleApproved/Rejected. It is
        /// never a default replacement for required shadow/attestation/apply rows.
        after_mapping: Option<ReservedValue<ModuleReleaseManifestMappingState>>,
        next: Option<(u64, u64)>,
    },
    Roles {
        before: ReservedValue<
            oasis7_world_state::incremental_world_collection_controls::CurrentModuleReleaseRolesV1,
        >,
        after: ReservedValue<std::collections::BTreeSet<String>>,
    },
}
pub(super) struct PreparedCurrentModuleReleaseV1 {
    pub(super) event: ReservedValue<crate::DomainEvent>,
    pub(super) mutation: CurrentModuleReleaseMutationV1,
    /// Original release projection touches the real operator and, only for a
    /// distinct BindRoles target, the real target. Their other fields are kept.
    pub(super) agents: [Option<CurrentModuleMarketAgentV1>; 2],
    selected: usize,
    outcome: usize,
    action_id: u64,
    guard: PreparedGuard,
    core: WorkIdentityV1,
    now: u64,
    _fees: Vec<MemoryReservation>,
    _layout: MemoryReservation,
    _parts: Box<super::release_facts::CurrentReleaseFactsPartsV1>,
}
impl PreparedCurrentModuleReleaseV1 {
    pub(super) fn action_id(&self) -> u64 {
        self.action_id
    }
    pub(super) fn validate_selection_v1(
        &self,
        selected: &SelectedPendingActionV1,
        outcome: &ReservedValue<RootRulePhaseOutcomeV1>,
    ) -> Result<(), BodyMutationError> {
        if self.selected != selected as *const _ as usize
            || self.outcome != outcome as *const _ as usize
            || self.action_id != selected.envelope().id
        {
            return Err(invalid());
        }
        Ok(())
    }
    pub(super) fn validate_current_v1<M: MerkleNodeReader>(
        &self,
        selected: &SelectedPendingActionV1,
        outcome: &ReservedValue<RootRulePhaseOutcomeV1>,
        tx: &ExecutionTransaction<'_, M>,
        tail: &crate::world::incremental_normal_tail::NormalTailWorkV1<'_, '_>,
        now: u64,
        live: &ExecutionFence,
    ) -> Result<(), BodyMutationError> {
        tx.validate_guard(&self.guard, live)?;
        self.validate_selection_v1(selected, outcome)?;
        self.validate_evidence_v1(tx, tail.memory(), tail.identity(), now, live)
    }
    pub(super) fn validate_evidence_v1<M: MerkleNodeReader>(
        &self,
        tx: &ExecutionTransaction<'_, M>,
        memory: &oasis7_proto::memory_budget::MemoryBudget,
        core: WorkIdentityV1,
        now: u64,
        live: &ExecutionFence,
    ) -> Result<(), BodyMutationError> {
        tx.validate_transaction_identity(&self.guard)?;
        tx.validate_guard(&tx.guard(), live)?;
        if self.core != core
            || self.now != now
            || !self.event.uses_budget(memory)
            || !self._layout.uses_budget(memory)
            || self._fees.iter().any(|fee| !fee.uses_budget(memory))
        {
            return Err(invalid());
        }
        validate_parts(&self._parts, tx, memory, core, now, live)?;
        for agent in self.agents.iter().flatten() {
            tx.validate_transaction_identity(&agent.guard)?;
            if !agent._fee.uses_budget(memory) {
                return Err(invalid());
            }
        }
        match &self.mutation {
            CurrentModuleReleaseMutationV1::Request {
                before_request,
                after_request,
                before_mapping,
                after_mapping,
                ..
            } => {
                if !before_request.uses_budget(memory)
                    || !after_request.uses_budget(memory)
                    || !before_mapping.uses_budget(memory)
                    || after_mapping
                        .as_ref()
                        .is_some_and(|row| !row.uses_budget(memory))
                {
                    return Err(invalid());
                }
            }
            CurrentModuleReleaseMutationV1::Roles { before, after } => {
                if !before.uses_budget(memory) || !after.uses_budget(memory) {
                    return Err(invalid());
                }
            }
        }
        Ok(())
    }
}

pub(super) enum CurrentReleasePlanDecisionV1 {
    Ready(PreparedCurrentModuleReleaseV1),
    Rejected(PreparedCurrentReleaseRejectionV1),
}
pub(super) struct PreparedCurrentReleaseRejectionV1 {
    pub(super) reason: ReservedValue<crate::RejectReason>,
    parts: Box<super::release_facts::CurrentReleaseFactsPartsV1>,
    selected: usize,
    outcome: usize,
    action_id: u64,
    guard: PreparedGuard,
    core: WorkIdentityV1,
    now: u64,
    _layout: MemoryReservation,
}
impl PreparedCurrentReleaseRejectionV1 {
    pub(super) fn action_id(&self) -> u64 {
        self.action_id
    }
    pub(super) fn validate<M: MerkleNodeReader>(
        &self,
        tx: &ExecutionTransaction<'_, M>,
        memory: &oasis7_proto::memory_budget::MemoryBudget,
        core: WorkIdentityV1,
        now: u64,
        live: &ExecutionFence,
    ) -> Result<(), BodyMutationError> {
        tx.validate_transaction_identity(&self.guard)?;
        tx.validate_guard(&tx.guard(), live)?;
        if self.core != core
            || self.now != now
            || !self.reason.uses_budget(memory)
            || !self._layout.uses_budget(memory)
        {
            return Err(invalid());
        }
        validate_parts(&self.parts, tx, memory, core, now, live)
    }
    pub(super) fn validate_selection<M: MerkleNodeReader>(
        &self,
        selected: &SelectedPendingActionV1,
        outcome: &ReservedValue<RootRulePhaseOutcomeV1>,
        tx: &ExecutionTransaction<'_, M>,
        common: &FormalBoundedContextV1<'_>,
        now: u64,
        live: &ExecutionFence,
    ) -> Result<(), BodyMutationError> {
        if self.selected != selected as *const _ as usize
            || self.outcome != outcome as *const _ as usize
            || self.action_id != selected.envelope().id
        {
            return Err(invalid());
        }
        self.validate(tx, common.memory(), common.work().identity(), now, live)
    }
}
fn validate_parts<M: MerkleNodeReader>(
    parts: &super::release_facts::CurrentReleaseFactsPartsV1,
    tx: &ExecutionTransaction<'_, M>,
    memory: &oasis7_proto::memory_budget::MemoryBudget,
    core: WorkIdentityV1,
    now: u64,
    live: &ExecutionFence,
) -> Result<(), BodyMutationError> {
    if !parts.slots.uses_budget(memory)
        || parts.fees.iter().any(|fee| !fee.uses_budget(memory))
        || parts
            .output_fee
            .as_ref()
            .is_some_and(|fee| !fee.uses_budget(memory))
        || parts
            .request
            .as_ref()
            .is_some_and(|request| !request.uses_budget(memory))
    {
        return Err(invalid());
    }
    for owner in &parts.profiles {
        owner.validate_provenance(tx, memory, core, live)?;
    }
    for epoch in &parts.epochs {
        epoch.validate(tx, memory, core, now, live)?;
    }
    Ok(())
}
struct ProjectionCause {
    error: crate::WorldError,
    _parts: Box<super::release_facts::CurrentReleaseFactsPartsV1>,
    _layout: MemoryReservation,
}
impl std::fmt::Debug for ProjectionCause {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}", self.error)
    }
}
impl std::fmt::Display for ProjectionCause {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}", self.error)
    }
}
impl std::error::Error for ProjectionCause {}

#[allow(clippy::too_many_arguments)]
pub(super) fn prepare_current_release_action_v1<M, P, R, E, C, PE, PC>(
    selected: &SelectedPendingActionV1,
    outcome: &ReservedValue<RootRulePhaseOutcomeV1>,
    accepted: &crate::world::incremental_module_invocation::SelectedAcceptedDispositionV1,
    sources: &ModuleLifecycleSourcesV1<'_>,
    ctx: &mut oasis7_world_state::incremental_field_overlay::WorldFieldOverlayContextV1<
        '_,
        '_,
        '_,
        M,
        R,
        E,
        C,
    >,
    parent: &mut oasis7_world_state::incremental_field_read::WorldFieldReadContextV1<
        '_,
        P,
        R,
        PE,
        PC,
    >,
    factory: &mut FactoryScheduleWorkV1,
    module: &mut ModuleLifecycleWorkV1<'_>,
    common: &mut FormalBoundedContextV1<'_>,
    directory: &ReservedValue<
        crate::world::incremental_module_invocation::AcceptedInvocationDirectoryV1,
    >,
    now: u64,
    live: &ExecutionFence,
) -> Result<CurrentReleasePlanDecisionV1, BodyMutationError>
where
    M: MerkleNodeReader,
    P: MerkleNodeReader,
    R: oasis7_formal_protocol::incremental_value::ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    PE: FnMut() -> Option<ValueTreeError>,
    PC: FnMut() -> bool,
{
    use super::release_rules::*;
    accepted.validate(selected, ctx.transaction, common.memory())?;
    let action = outcome
        .override_action_for_selected_v1(selected, ctx.transaction)?
        .unwrap_or(&selected.envelope().action);
    let bytes = std::mem::size_of::<PreparedCurrentModuleReleaseV1>()
        .checked_add(std::mem::size_of::<PreparedCurrentReleaseRejectionV1>())
        .and_then(|n| {
            n.checked_add(std::mem::size_of::<
                super::release_facts::CurrentReleaseFactsPartsV1,
            >())
        })
        .and_then(|n| n.checked_add(8192))
        .ok_or_else(invalid)?;
    pay(common, bytes)?;
    let mut layout = common
        .memory()
        .reserve(bytes, MemoryClass::Ordinary)
        .map_err(ValueTreeError::Memory)?;
    let mut source = super::release_facts::CurrentModuleReleaseFactsV1::new(
        sources, ctx, parent, factory, module, common, directory, live,
    )
    .map_err(read_error)?;
    source.reserve(action).map_err(read_error)?;
    let evaluated = match action {
        crate::Action::ModuleReleaseSubmit {
            requester_agent_id,
            manifest,
            activate,
            install_target,
            required_roles,
            profile_changes,
        } => prepare_module_release_submit_v1(
            &mut source,
            selected.envelope().id,
            requester_agent_id,
            manifest,
            *activate,
            install_target,
            required_roles,
            profile_changes,
        ),
        crate::Action::ModuleReleaseShadow {
            operator_agent_id,
            request_id,
        } => prepare_module_release_shadow_v1(
            &mut source,
            selected.envelope().id,
            operator_agent_id,
            *request_id,
        ),
        crate::Action::ModuleReleaseSubmitAttestation {
            operator_agent_id,
            request_id,
            signer_node_id,
            platform,
            build_manifest_hash,
            source_hash,
            wasm_hash,
            proof_cid,
            builder_image_digest,
            container_platform,
            canonicalizer_version,
        } => prepare_module_release_submit_attestation_v1(
            &mut source,
            selected.envelope().id,
            operator_agent_id,
            *request_id,
            signer_node_id,
            platform,
            build_manifest_hash,
            source_hash,
            wasm_hash,
            proof_cid,
            builder_image_digest,
            container_platform,
            canonicalizer_version,
        ),
        crate::Action::ModuleReleaseApproveRole {
            approver_agent_id,
            request_id,
            role,
        } => prepare_module_release_approve_role_v1(
            &mut source,
            selected.envelope().id,
            approver_agent_id,
            *request_id,
            role,
        ),
        crate::Action::ModuleReleaseBindRoles {
            operator_agent_id,
            target_agent_id,
            roles,
        } => prepare_module_release_bind_roles_v1(
            &mut source,
            selected.envelope().id,
            operator_agent_id,
            target_agent_id,
            roles,
        ),
        crate::Action::ModuleReleaseReject {
            rejector_agent_id,
            request_id,
            reason,
        } => prepare_module_release_reject_v1(
            &mut source,
            selected.envelope().id,
            rejector_agent_id,
            *request_id,
            reason,
        ),
        _ => return Err(invalid()),
    };
    let mut parts = source.into_parts();
    let evaluated=match evaluated{Ok(value)=>value,Err(crate::world::incremental_normal_action_factory_schedule::FactoryScheduleReadErrorV1::Body(error))=>return Err(error),Err(crate::world::incremental_normal_action_factory_schedule::FactoryScheduleReadErrorV1::World(error))=>return Err(BodyMutationError::Original(Box::new(ProjectionCause{error,_parts:Box::new(parts),_layout:layout})))};
    let output = parts.output_fee.take().ok_or_else(invalid)?;
    let domain = match evaluated {
        ModuleReleaseBusinessDecisionV1::Rejected(reason) => {
            return Ok(CurrentReleasePlanDecisionV1::Rejected(
                PreparedCurrentReleaseRejectionV1 {
                    reason: output.retain(reason),
                    parts: Box::new(parts),
                    selected: selected as *const _ as usize,
                    outcome: outcome as *const _ as usize,
                    action_id: selected.envelope().id,
                    guard: ctx.transaction.guard(),
                    core: common.work().identity(),
                    now,
                    _layout: layout,
                },
            ));
        }
        ModuleReleaseBusinessDecisionV1::Emit(domain) => output.retain(domain),
    };
    let mutation = if let crate::DomainEvent::ModuleReleaseRolesBound {
        target_agent_id,
        roles,
        ..
    } = &*domain
    {
        let before = super::release_roles_current::read_current_module_release_roles_v1(
            target_agent_id,
            sources,
            ctx,
            parent,
            factory,
            module,
            common,
            live,
        )
        .map_err(read_error)?;
        let measured =
            super::governance_json::module_json_value_paid_v1(roles, common).map_err(json_error)?;
        let bytes = measured
            .fee
            .bytes()
            .checked_mul(4)
            .and_then(|n| n.checked_add(8192))
            .ok_or_else(invalid)?;
        pay(common, bytes)?;
        let fee = common
            .memory()
            .reserve(bytes, MemoryClass::Ordinary)
            .map_err(ValueTreeError::Memory)?;
        let after = fee.retain(
            roles
                .iter()
                .map(|role| role.trim().to_ascii_lowercase())
                .filter(|role| !role.is_empty())
                .collect(),
        );
        CurrentModuleReleaseMutationV1::Roles { before, after }
    } else {
        let outer = match &*domain {
            crate::DomainEvent::ModuleReleaseRequested { request_id, .. }
            | crate::DomainEvent::ModuleReleaseShadowed { request_id, .. }
            | crate::DomainEvent::ModuleReleaseAttested { request_id, .. }
            | crate::DomainEvent::ModuleReleaseRoleApproved { request_id, .. }
            | crate::DomainEvent::ModuleReleaseRejected { request_id, .. } => *request_id,
            _ => return Err(invalid()),
        };
        let before_request = match parts.request.take() {
            Some(before) => before,
            None => super::release_current::read_current_module_release_request_v1(
                outer, sources, ctx, parent, factory, module, common, live,
            )
            .map_err(read_error)?,
        };
        let before_mapping =
            super::release_current::read_current_module_release_manifest_mapping_v1(
                outer, sources, ctx, parent, factory, module, common, live,
            )
            .map_err(read_error)?;
        let next_raw = parts.next_raw.unwrap_or(0);
        let measured = super::governance_json::module_json_value_paid_v1(
            &(&*domain, before_request.value(), before_mapping.value()),
            common,
        )
        .map_err(json_error)?;
        let bytes = measured
            .fee
            .bytes()
            .checked_mul(8)
            .and_then(|n| n.checked_add(16384))
            .ok_or_else(invalid)?;
        pay(common, bytes)?;
        let mut fee = common
            .memory()
            .reserve(bytes, MemoryClass::Ordinary)
            .map_err(ValueTreeError::Memory)?;
        let projection = match oasis7_world_state::prepare_module_release_records_v1(
            &domain,
            before_request.value(),
            before_mapping.value(),
            now,
            next_raw,
        ) {
            Ok(value) => value,
            Err(error) => {
                return Err(BodyMutationError::Original(Box::new(ProjectionCause {
                    error,
                    _parts: Box::new(parts),
                    _layout: layout,
                })));
            }
        };
        // Two separately typed values share this already-admitted whole projection.
        let request_heap = fee.bytes();
        let extra = common
            .memory()
            .reserve(request_heap, MemoryClass::Ordinary)
            .map_err(ValueTreeError::Memory)?;
        let after_request = extra.retain(projection.request);
        let after_mapping = match projection.mapping {
            Some(value) => Some(fee.retain(value)),
            None => {
                layout.merge(&mut fee).map_err(BodyMutationError::Invalid)?;
                None
            }
        };
        CurrentModuleReleaseMutationV1::Request {
            outer,
            before_request,
            after_request,
            before_mapping,
            after_mapping,
            next: projection.next_request_id.map(|after| (next_raw, after)),
        }
    };
    let agents = std::array::from_fn(|index| parts.agents[index].take());
    Ok(CurrentReleasePlanDecisionV1::Ready(
        PreparedCurrentModuleReleaseV1 {
            event: domain,
            mutation,
            agents,
            selected: selected as *const _ as usize,
            outcome: outcome as *const _ as usize,
            action_id: selected.envelope().id,
            guard: ctx.transaction.guard(),
            core: common.work().identity(),
            now,
            _fees: Vec::new(),
            _layout: layout,
            _parts: Box::new(parts),
        },
    ))
}
fn read_error(
    error: crate::world::incremental_normal_action_factory_schedule::FactoryScheduleReadErrorV1,
) -> BodyMutationError {
    match error{crate::world::incremental_normal_action_factory_schedule::FactoryScheduleReadErrorV1::Body(error)=>error,crate::world::incremental_normal_action_factory_schedule::FactoryScheduleReadErrorV1::World(error)=>BodyMutationError::Original(Box::new(SimpleWorldCause(error)))}
}
#[derive(Debug)]
struct SimpleWorldCause(crate::WorldError);
impl std::fmt::Display for SimpleWorldCause {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}", self.0)
    }
}
impl std::error::Error for SimpleWorldCause {}
struct JsonCause {
    error: serde_json::Error,
    _fee: MemoryReservation,
}
impl std::fmt::Debug for JsonCause {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        std::fmt::Debug::fmt(&self.error, f)
    }
}
impl std::fmt::Display for JsonCause {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        std::fmt::Display::fmt(&self.error, f)
    }
}
impl std::error::Error for JsonCause {}
fn json_error(error: super::governance_json::ModuleGovernanceJsonErrorV1) -> BodyMutationError {
    match error {
        super::governance_json::ModuleGovernanceJsonErrorV1::Body(error) => error,
        super::governance_json::ModuleGovernanceJsonErrorV1::Json { error, _fee } => {
            BodyMutationError::Original(Box::new(JsonCause { error, _fee }))
        }
    }
}

/// Completion is an actual committed installation owner, not an Applied DTO.
/// Read the operator again here, after fees and profile publications.
#[allow(clippy::too_many_arguments)]
pub(super) fn prepare_current_applied_release_v1<M, P, R, E, C, PE, PC>(
    completion: &super::release_completion::PaidCurrentReleaseCompletionV1<'_>,
    selected: &SelectedPendingActionV1,
    outcome: &ReservedValue<RootRulePhaseOutcomeV1>,
    sources: &ModuleLifecycleSourcesV1<'_>,
    ctx: &mut oasis7_world_state::incremental_field_overlay::WorldFieldOverlayContextV1<
        '_,
        '_,
        '_,
        M,
        R,
        E,
        C,
    >,
    parent: &mut oasis7_world_state::incremental_field_read::WorldFieldReadContextV1<
        '_,
        P,
        R,
        PE,
        PC,
    >,
    factory: &mut FactoryScheduleWorkV1,
    module: &mut ModuleLifecycleWorkV1<'_>,
    common: &mut FormalBoundedContextV1<'_>,
    directory: &ReservedValue<
        crate::world::incremental_module_invocation::AcceptedInvocationDirectoryV1,
    >,
    now: u64,
    live: &ExecutionFence,
) -> Result<PreparedCurrentModuleReleaseV1, BodyMutationError>
where
    M: MerkleNodeReader,
    P: MerkleNodeReader,
    R: oasis7_formal_protocol::incremental_value::ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    PE: FnMut() -> Option<ValueTreeError>,
    PC: FnMut() -> bool,
{
    use super::release_rules::ModuleReleaseBusinessFactsV1;
    completion.validate_selection(
        selected,
        outcome,
        ctx.transaction,
        common.memory(),
        common.work().identity(),
        now,
        live,
    )?;
    let actual = completion.final_event()?;
    let crate::DomainEvent::ModuleReleaseApplied {
        request_id: outer,
        operator_agent_id,
        ..
    } = actual
    else {
        return Err(invalid());
    };
    let bytes = std::mem::size_of::<PreparedCurrentModuleReleaseV1>()
        .checked_add(std::mem::size_of::<
            super::release_facts::CurrentReleaseFactsPartsV1,
        >())
        .and_then(|n| n.checked_add(8192))
        .ok_or_else(invalid)?;
    pay(common, bytes)?;
    let mut layout = common
        .memory()
        .reserve(bytes, MemoryClass::Ordinary)
        .map_err(ValueTreeError::Memory)?;
    let mut source = super::release_facts::CurrentModuleReleaseFactsV1::new(
        sources, ctx, parent, factory, module, common, directory, live,
    )
    .map_err(read_error)?;
    source.reserve(actual).map_err(read_error)?;
    if !source.agent_exists(operator_agent_id).map_err(read_error)? {
        return Err(invalid());
    }
    // The original row reducer below determines missing/status errors; this read
    // does not replace its first refusal with a new eligibility condition.
    let _snapshot = source.request(*outer).map_err(read_error)?;
    let mut parts = source.into_parts();
    let event = parts
        .output_fee
        .take()
        .ok_or_else(invalid)?
        .retain(actual.clone());
    let before_request = parts.request.take().ok_or_else(invalid)?;
    let before_mapping = super::release_current::read_current_module_release_manifest_mapping_v1(
        *outer, sources, ctx, parent, factory, module, common, live,
    )
    .map_err(read_error)?;
    let measured = super::governance_json::module_json_value_paid_v1(
        &(&*event, before_request.value(), before_mapping.value()),
        common,
    )
    .map_err(json_error)?;
    let bytes = measured
        .fee
        .bytes()
        .checked_mul(8)
        .and_then(|n| n.checked_add(16384))
        .ok_or_else(invalid)?;
    pay(common, bytes)?;
    let mut fee = common
        .memory()
        .reserve(bytes, MemoryClass::Ordinary)
        .map_err(ValueTreeError::Memory)?;
    let projection = match oasis7_world_state::prepare_module_release_records_v1(
        &event,
        before_request.value(),
        before_mapping.value(),
        now,
        0,
    ) {
        Ok(value) => value,
        Err(error) => {
            return Err(BodyMutationError::Original(Box::new(ProjectionCause {
                error,
                _parts: Box::new(parts),
                _layout: layout,
            })));
        }
    };
    let request_fee = common
        .memory()
        .reserve(fee.bytes(), MemoryClass::Ordinary)
        .map_err(ValueTreeError::Memory)?;
    let after_request = request_fee.retain(projection.request);
    let after_mapping = match projection.mapping {
        Some(value) => Some(fee.retain(value)),
        None => {
            layout.merge(&mut fee).map_err(BodyMutationError::Invalid)?;
            None
        }
    };
    if projection.next_request_id.is_some() {
        return Err(invalid());
    }
    let mutation = CurrentModuleReleaseMutationV1::Request {
        outer: *outer,
        before_request,
        after_request,
        before_mapping,
        after_mapping,
        next: None,
    };
    let agents = std::array::from_fn(|index| parts.agents[index].take());
    Ok(PreparedCurrentModuleReleaseV1 {
        event,
        mutation,
        agents,
        selected: selected as *const _ as usize,
        outcome: outcome as *const _ as usize,
        action_id: selected.envelope().id,
        guard: ctx.transaction.guard(),
        core: common.work().identity(),
        now,
        _fees: Vec::new(),
        _layout: layout,
        _parts: Box::new(parts),
    })
}

/// Only the sealed Apply observer can mint this input. Moving its real output
/// keeps every review/epoch reservation; no raw verdict constructor is exposed.
pub(super) fn bind_current_release_apply_rejection_v1<M: MerkleNodeReader>(
    source: super::release_facts::CurrentReleaseApplyRejectionV1,
    selected: &SelectedPendingActionV1,
    outcome: &ReservedValue<RootRulePhaseOutcomeV1>,
    tx: &ExecutionTransaction<'_, M>,
    common: &FormalBoundedContextV1<'_>,
    now: u64,
    live: &ExecutionFence,
) -> Result<PreparedCurrentReleaseRejectionV1, BodyMutationError> {
    source.validate_provenance(
        selected,
        outcome,
        tx,
        common.memory(),
        common.work().identity(),
        now,
        live,
    )?;
    let (reason, parts, layout) = source.into_parts();
    Ok(PreparedCurrentReleaseRejectionV1 {
        reason,
        parts,
        selected: selected as *const _ as usize,
        outcome: outcome as *const _ as usize,
        action_id: selected.envelope().id,
        guard: tx.guard(),
        core: common.work().identity(),
        now,
        _layout: layout,
    })
}
