//! Actual Current service/source/owner/location/policy observations. Root
//! Observe and Active share this fixed provenance chain, without World clone.
use super::*;
use crate::world::incremental_industrial_eligibility::{
    CurrentStateOwnerSourcesV1, OwnedCurrentIndustrialStateOwnerV1, resolve_current_state_owner_v1,
};
use oasis7_world_state::{
    incremental_agent_fields::AgentFieldV1,
    incremental_agent_subtree::{
        AgentSubtreeQualifiedRequestV1, AgentSubtreeScalarRequestV1, AgentSubtreeValueV1,
        CurrentAgentPresenceV1, read_agent_subtree_scalar_overlay_v1,
        read_current_agent_presence_v1,
    },
    incremental_factory_schedule_authority::{
        recover_current_schedule_agent_location_v1, recover_current_schedule_location_anchor_v1,
    },
    incremental_world_native::recover_gameplay_policy_overlay_v1,
};
/// Both alternatives own genuine selected Current original readers; no raw
/// registration DTO, Vec or caller permission flag can substitute for them.
pub(in crate::world) enum CurrentEmergencyServiceSelectionV1 {
    Service(ReservedValue<CurrentEmergencyServiceV1>),
    MinimumRegistration(ReservedValue<CurrentEmergencyMinimumServiceV1>),
}
impl CurrentEmergencyServiceSelectionV1 {
    fn service(
        &self,
    ) -> Result<
        &oasis7_agent_api::world_action::EmergencyServiceRegistrationV1,
        IndustrialDeliveryErrorV1,
    > {
        match self {
            Self::Service(v) => v.value(),
            Self::MinimumRegistration(v) => v.value().map(|v| &v.registration),
        }
        .ok_or_else(|| unverifiable())
    }
}
fn unverifiable() -> IndustrialDeliveryErrorV1 {
    crate::world_rules::invalid("need_source_unverifiable").into()
}
pub(in crate::world) struct EmergencyServiceSourceWorkV1<'a> {
    pub authority: &'a mut FactoryScheduleAuthorityWorkV1,
    pub authority_cells: &'a mut u64,
}
pub(in crate::world) struct CurrentEmergencyServiceSourcesV1 {
    selection: CurrentEmergencyServiceSelectionV1,
    minimum: ReservedValue<CurrentEmergencyMinimumServiceV1>,
    owner: ReservedValue<Option<OwnedCurrentIndustrialStateOwnerV1>>,
    authority: ReservedValue<oasis7_world_state::AgentLocationAuthorityV1>,
    anchor: ReservedValue<oasis7_world_state::LocationAnchorV1>,
    policy: WorldNativeRecoveredRecordV1<oasis7_world_state::GameplayPolicyState>,
    guard: PreparedGuard,
    memory: MemoryBudget,
    identity: WorkIdentityV1,
    now: u64,
}
impl CurrentEmergencyServiceSourcesV1 {
    pub(in crate::world) fn service(
        &self,
    ) -> &oasis7_agent_api::world_action::EmergencyServiceRegistrationV1 {
        self.selection
            .service()
            .expect("private successful source producer")
    }
    pub(in crate::world) fn minimum_source(
        &self,
    ) -> &oasis7_world_state::world_rules::MinimumServiceContractV1 {
        self.minimum
            .value()
            .expect("private successful selected minimum")
    }
    pub(in crate::world) fn validate<M: MerkleNodeReader>(
        &self,
        tx: &ExecutionTransaction<'_, M>,
        tail: &mut NormalTailWorkV1<'_, '_>,
        now: u64,
        live: &ExecutionFence,
    ) -> Result<(), IndustrialDeliveryErrorV1> {
        tx.validate_guard(&self.guard, live)?;
        tail.charge_record()?;
        if self.now != now
            || self.identity != tail.identity()
            || !self.memory.same_pool(tail.memory())
            || !tx.uses_budget(tail.memory())
        {
            return Err(
                BodyMutationError::Invalid("Current emergency service sources scope").into(),
            );
        }
        Ok(())
    }
}
pub(in crate::world) struct CurrentEmergencyObservationServiceSourcesV1 {
    sources: ReservedValue<CurrentEmergencyServiceSourcesV1>,
    electricity: i64,
    guard: PreparedGuard,
}
impl CurrentEmergencyObservationServiceSourcesV1 {
    pub(in crate::world) fn service(
        &self,
    ) -> &oasis7_agent_api::world_action::EmergencyServiceRegistrationV1 {
        self.sources.service()
    }
    pub(in crate::world) fn minimum_source(
        &self,
    ) -> &oasis7_world_state::world_rules::MinimumServiceContractV1 {
        self.sources.minimum_source()
    }
    pub(in crate::world) fn electricity_max_zero(&self) -> i64 {
        self.electricity
    }
    pub(in crate::world) fn validate<M: MerkleNodeReader>(
        &self,
        tx: &ExecutionTransaction<'_, M>,
        tail: &mut NormalTailWorkV1<'_, '_>,
        now: u64,
        live: &ExecutionFence,
    ) -> Result<(), IndustrialDeliveryErrorV1> {
        tx.validate_guard(&self.guard, live)?;
        tail.charge_record()?;
        if self.sources.now != now
            || self.sources.identity != tail.identity()
            || !self.sources.memory.same_pool(tail.memory())
            || !tx.uses_budget(tail.memory())
        {
            return Err(
                BodyMutationError::Invalid("Current observation service source scope").into(),
            );
        }
        Ok(())
    }
}
fn clamp_authority(work: &mut EmergencyServiceSourceWorkV1<'_>, tail: &NormalTailWorkV1<'_, '_>) {
    let steps = tail.remaining_steps() / 2;
    work.authority.cells_remaining = work.authority.cells_remaining.min(steps);
    *work.authority_cells = (*work.authority_cells).min(steps);
    work.authority.bytes_remaining = work
        .authority
        .bytes_remaining
        .min(tail.remaining_read_bytes());
}
fn reconcile_authority(
    before: (u64, u64, u64),
    work: &EmergencyServiceSourceWorkV1<'_>,
    tail: &mut NormalTailWorkV1<'_, '_>,
) -> Result<(), IndustrialDeliveryErrorV1> {
    let steps = before
        .0
        .checked_sub(work.authority.cells_remaining)
        .and_then(|n| n.checked_add(before.2.checked_sub(*work.authority_cells)?))
        .ok_or(BodyMutationError::Invalid(
            "emergency authority work increased",
        ))?;
    let bytes =
        before
            .1
            .checked_sub(work.authority.bytes_remaining)
            .ok_or(BodyMutationError::Invalid(
                "emergency authority bytes increased",
            ))?;
    tail.charge(
        usize::try_from(bytes)
            .map_err(|_| BodyMutationError::Invalid("emergency authority work address"))?,
    )?;
    for _ in 0..steps {
        tail.charge_record()?;
    }
    Ok(())
}
fn prepare<M, P, V, R, E, C, PE, PC>(
    selection: CurrentEmergencyServiceSelectionV1,
    source: &ReservedValue<CurrentEmergencyLifecycleSourceV1>,
    sources: &EmergencyCloseSourcesV1<'_, '_>,
    state: &M,
    ctx: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    parent: &mut WorldFieldReadContextV1<'_, P, V, PE, PC>,
    work: &mut IndustrialDeliveryWorkV1,
    extra: &mut EmergencyServiceSourceWorkV1<'_>,
    tail: &mut NormalTailWorkV1<'_, '_>,
    now: u64,
    observation: bool,
    live: &ExecutionFence,
) -> Result<ReservedValue<CurrentEmergencyServiceSourcesV1>, IndustrialDeliveryErrorV1>
where
    M: MerkleNodeReader,
    P: MerkleNodeReader,
    V: ValueNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    PE: FnMut() -> Option<ValueTreeError>,
    PC: FnMut() -> bool,
{
    let root = sources.delivery.eligibility.roots;
    let fee = tail.reserve(
        std::mem::size_of::<CurrentEmergencyServiceSourcesV1>(),
        MemoryClass::Ordinary,
    )?;
    tail.charge_record()?;
    // Revalidate the selected original against the actual source's Current leaf,
    // not merely the fact that its Rust DTO once decoded successfully.
    let guard = ctx.transaction.guard();
    match &selection {
        CurrentEmergencyServiceSelectionV1::Service(v) => {
            validate_current_emergency_service_selection_v1(
                source,
                v,
                sources.rules.emergency(),
                root,
                ctx,
                &work.composite_read,
                &guard,
                live,
            )?
        }
        CurrentEmergencyServiceSelectionV1::MinimumRegistration(v) => {
            validate_current_emergency_minimum_selection_v1(
                source,
                v,
                sources.rules.emergency(),
                root,
                ctx,
                &work.composite_read,
                &guard,
                live,
            )?
        }
    }
    let service = selection.service()?;
    let guard = ctx.transaction.guard();
    let minimum = read_current_emergency_minimum_service_v1(
        source,
        sources.rules.emergency(),
        root,
        state,
        &service.obligation_id,
        ctx,
        &mut work.eligibility.scan,
        &mut work.composite_read,
        &guard,
        live,
    )?;
    let contract = minimum.value().ok_or_else(unverifiable)?;
    let narrow = CurrentStateOwnerSourcesV1 {
        roots: root,
        orders: sources.delivery.eligibility.orders,
        agents: sources.delivery.eligibility.agents,
        native: sources.delivery.eligibility.native,
    };
    let guard = ctx.transaction.guard();
    let owner = resolve_current_state_owner_v1(
        &narrow,
        &contract.registration.beneficiary_id,
        ctx,
        parent,
        &mut work.eligibility,
        &guard,
        live,
    )?;
    let Some(resolved) = owner.as_ref() else {
        return Err(unverifiable());
    };
    tail.charge(
        resolved
            .player_id()
            .len()
            .min(contract.registration.controller_id.len())
            .checked_add(
                resolved
                    .source_id()
                    .len()
                    .min(contract.owner_source_id.len()),
            )
            .and_then(|n| {
                n.checked_add(
                    resolved
                        .source_id()
                        .len()
                        .min(contract.registration.source_ref.len()),
                )
            })
            .and_then(|n| {
                n.checked_add(
                    resolved
                        .primary_agent_id()
                        .len()
                        .min(contract.operator_agent_id.len()),
                )
            })
            .ok_or(BodyMutationError::Invalid("minimum source comparison size"))?,
    )?;
    if !oasis7_world_state::world_rules::minimum_service_owner_fields_match_v1(
        contract,
        resolved.primary_agent_id(),
        resolved.player_id(),
        resolved.source_id(),
    ) {
        return Err(unverifiable());
    }
    clamp_authority(extra, tail);
    let before = (
        extra.authority.cells_remaining,
        extra.authority.bytes_remaining,
        *extra.authority_cells,
    );
    let guard = ctx.transaction.guard();
    let result = recover_current_schedule_agent_location_v1(
        &service.beneficiary_id,
        sources.delivery.eligibility.anchors,
        state,
        ctx,
        &mut work.eligibility.scan,
        extra.authority_cells,
        extra.authority,
        &guard,
        live,
    );
    let accounting = reconcile_authority(before, extra, tail);
    let authority = match result {
        Err(e) => return Err(e.into()),
        Ok(v) => {
            accounting?;
            v
        }
    }
    .ok_or_else(unverifiable)?;
    tail.charge(
        authority
            .agent_id
            .len()
            .min(service.beneficiary_id.len())
            .checked_add(authority.location_id.len().min(service.location_id.len()))
            .ok_or(BodyMutationError::Invalid(
                "observation location comparison",
            ))?,
    )?;
    if !authority.active
        || authority.agent_id != service.beneficiary_id
        || authority.location_id != service.location_id
        || authority.effective_at > now
    {
        return Err(unverifiable());
    }
    clamp_authority(extra, tail);
    let before = (
        extra.authority.cells_remaining,
        extra.authority.bytes_remaining,
        *extra.authority_cells,
    );
    let guard = ctx.transaction.guard();
    let result = recover_current_schedule_location_anchor_v1(
        &service.location_id,
        sources.delivery.eligibility.anchors,
        state,
        ctx,
        &mut work.eligibility.scan,
        extra.authority_cells,
        extra.authority,
        &guard,
        live,
    );
    let accounting = reconcile_authority(before, extra, tail);
    let anchor = match result {
        Err(e) => return Err(e.into()),
        Ok(v) => {
            accounting?;
            v
        }
    }
    .ok_or_else(unverifiable)?;
    tail.charge(anchor.location_id.len().min(service.location_id.len()))?;
    if !oasis7_world_state::world_rules::validate_observation_location_from_records_v1(
        Some(&authority),
        Some(&anchor),
        &service.beneficiary_id,
        &service.location_id,
        now,
    ) {
        return Err(unverifiable());
    }
    if observation
        && (authority.effective_at > service.registered_at
            || anchor.effective_at > service.registered_at)
    {
        return Err(unverifiable());
    }
    let policy = {
        let mut read = WorldFieldReadContextV1 {
            root: root.state_root(StateNamespaceV1::Business),
            state: parent.state,
            values: ValueReadContext {
                source: parent.values.source,
                memory: parent.values.memory,
                class: parent.values.class,
                io: &mut *parent.values.io,
            },
            key_work: &mut *parent.key_work,
            semantic_bytes: parent.semantic_bytes,
            record_bytes: parent.record_bytes,
            take_state_error: &mut parent.take_state_error,
            cancelled: &mut parent.cancelled,
        };
        recover_gameplay_policy_overlay_v1(
            ctx.transaction,
            live,
            &mut read,
            &mut work.eligibility.scan,
            &mut work.eligibility.typed,
        )?
    };
    for id in &policy.value().value().blocked_agents {
        tail.charge(id.len().min(service.beneficiary_id.len()))?;
        if id == &service.beneficiary_id {
            return Err(unverifiable());
        }
    }
    tail.charge_record()?;
    let guard = ctx.transaction.guard();
    ctx.transaction.validate_guard(&guard, live)?;
    Ok(fee.retain(CurrentEmergencyServiceSourcesV1 {
        selection,
        minimum,
        owner,
        authority,
        anchor,
        policy,
        guard,
        memory: tail.memory().clone(),
        identity: tail.identity(),
        now,
    }))
}
pub(in crate::world) fn read_current_emergency_active_service_sources_v1<M, P, V, R, E, C, PE, PC>(
    selection: CurrentEmergencyServiceSelectionV1,
    source: &ReservedValue<CurrentEmergencyLifecycleSourceV1>,
    sources: &EmergencyCloseSourcesV1<'_, '_>,
    state: &M,
    ctx: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    parent: &mut WorldFieldReadContextV1<'_, P, V, PE, PC>,
    work: &mut IndustrialDeliveryWorkV1,
    extra: &mut EmergencyServiceSourceWorkV1<'_>,
    tail: &mut NormalTailWorkV1<'_, '_>,
    now: u64,
    live: &ExecutionFence,
) -> Result<ReservedValue<CurrentEmergencyServiceSourcesV1>, IndustrialDeliveryErrorV1>
where
    M: MerkleNodeReader,
    P: MerkleNodeReader,
    V: ValueNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    PE: FnMut() -> Option<ValueTreeError>,
    PC: FnMut() -> bool,
{
    prepare(
        selection, source, sources, state, ctx, parent, work, extra, tail, now, false, live,
    )
}
pub(in crate::world) fn read_current_emergency_observation_service_sources_v1<
    M,
    P,
    V,
    R,
    E,
    C,
    PE,
    PC,
>(
    selection: CurrentEmergencyServiceSelectionV1,
    source: &ReservedValue<CurrentEmergencyLifecycleSourceV1>,
    sources: &EmergencyCloseSourcesV1<'_, '_>,
    state: &M,
    ctx: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    parent: &mut WorldFieldReadContextV1<'_, P, V, PE, PC>,
    work: &mut IndustrialDeliveryWorkV1,
    extra: &mut EmergencyServiceSourceWorkV1<'_>,
    tail: &mut NormalTailWorkV1<'_, '_>,
    now: u64,
    live: &ExecutionFence,
) -> Result<ReservedValue<CurrentEmergencyObservationServiceSourcesV1>, IndustrialDeliveryErrorV1>
where
    M: MerkleNodeReader,
    P: MerkleNodeReader,
    V: ValueNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    PE: FnMut() -> Option<ValueTreeError>,
    PC: FnMut() -> bool,
{
    let sources_owner = prepare(
        selection, source, sources, state, ctx, parent, work, extra, tail, now, true, live,
    )?;
    let service = sources_owner.service();
    let guard = ctx.transaction.guard();
    let exists = read_current_agent_presence_v1(
        &service.beneficiary_id,
        sources.delivery.eligibility.agents,
        sources.delivery.eligibility.roots,
        ctx,
        parent,
        &mut work.eligibility.pending_entries_remaining,
        &guard,
        live,
    )? == CurrentAgentPresenceV1::Existing;
    let amount = if exists {
        let guard = ctx.transaction.guard();
        match read_agent_subtree_scalar_overlay_v1(
            AgentSubtreeQualifiedRequestV1 {
                cell: AgentSubtreeScalarRequestV1 {
                    agent: &service.beneficiary_id,
                    field: AgentFieldV1::ResourceAmount,
                    ordinal: 0,
                },
                qualification: sources.delivery.eligibility.agents,
            },
            ctx,
            8,
            &mut std::io::sink(),
            &guard,
            live,
        )? {
            None => 0,
            Some(AgentSubtreeValueV1::I64(n)) => n,
            _ => {
                return Err(
                    BodyMutationError::Invalid("observation selected electricity shape").into(),
                );
            }
        }
    } else {
        0
    };
    let fee = tail.reserve(
        std::mem::size_of::<CurrentEmergencyObservationServiceSourcesV1>(),
        MemoryClass::Ordinary,
    )?;
    tail.charge_record()?;
    let guard = ctx.transaction.guard();
    ctx.transaction.validate_guard(&guard, live)?;
    Ok(fee.retain(CurrentEmergencyObservationServiceSourcesV1 {
        sources: sources_owner,
        electricity: amount.max(0),
        guard,
    }))
}
