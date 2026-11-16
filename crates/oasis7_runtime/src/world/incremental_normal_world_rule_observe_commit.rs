//! Exact Current registry CAS after complete-region preparation and actual
//! original publication. The private owner survives to counted event routing.
use super::*;
use crate::world::incremental_body_event::materials::CorePolicyMaterialContextV1;
use crate::world::incremental_normal_sources::BoundNormalTickParentV1;
use oasis7_formal_protocol::{
    controlled_authority::formal::bounded_context::FormalBoundedContextV1,
    incremental_state::StatePageWriter,
    incremental_value::{ValueNodeWriter, ValueTreeError},
};
use oasis7_world_state::{
    incremental_field_member::MemberRecordWriteContext,
    incremental_world_rule_composite_overlay::WorldRuleCompositeOverlayV1,
    incremental_world_rule_requests_current::{
        CurrentObservationRegistryEntryV2, insert_current_observation_registry_v2,
    },
    world_rules::WorldRuleReceiptV1,
};

type Requests = BTreeMap<String, WorldRuleReceiptV1>;
pub(in crate::world) struct CommittedCurrentObservationV2 {
    published: PublishedCurrentObservationV2,
    receipt: ReservedValue<WorldRuleReceiptV1>,
    requests: ReservedValue<WorldRuleCompositeOverlayV1<Requests>>,
    event: crate::DomainEvent,
    result_id: String,
    guard: PreparedGuard,
    identity: WorkIdentityV1,
    now: u64,
    memory: MemoryBudget,
    _fee: MemoryReservation,
}
impl CommittedCurrentObservationV2 {
    pub(in crate::world) fn result_id(&self) -> &str {
        &self.result_id
    }
    pub(in crate::world) fn validate<M: MerkleNodeReader>(
        &self,
        header: &header::PreparedFreshLifecycleHeaderV1,
        tx: &ExecutionTransaction<'_, M>,
        tail: &mut NormalTailWorkV1<'_, '_>,
        now: u64,
        live: &ExecutionFence,
    ) -> Result<(), BodyMutationError> {
        tx.validate_guard(&self.guard, live)?;
        if self.identity != tail.identity()
            || self.now != now
            || !self.memory.same_pool(tail.memory())
            || !self.receipt.uses_budget(tail.memory())
            || !self.requests.uses_budget(tail.memory())
        {
            return Err(BodyMutationError::Invalid(
                "committed Current observation scope",
            ));
        }
        let bytes = header_bytes(&self.event)?
            .checked_add(header_bytes(header.event())?)
            .ok_or(BodyMutationError::Invalid("Observe event comparison size"))?;
        tail.charge(bytes)?;
        if self.event != *header.event() {
            return Err(BodyMutationError::Invalid("committed Observe header"));
        }
        Ok(())
    }
}
fn bad() -> BodyMutationError {
    BodyMutationError::Invalid("Current Observe registry provenance")
}
fn header_bytes(event: &crate::DomainEvent) -> Result<usize, BodyMutationError> {
    let crate::DomainEvent::WorldRuleCommitted {
        action_id: 0,
        actor_id,
        request_id,
        command:
            WorldRuleCommandV1::EmergencySupply {
                command: EmergencySupplyCommandV1::Observe { region_id },
            },
        world_binding,
        ..
    } = event
    else {
        return Err(bad());
    };
    actor_id
        .len()
        .checked_add(request_id.len())
        .and_then(|n| n.checked_add(region_id.len()))
        .and_then(|n| n.checked_add(world_binding.as_ref().map_or(0, String::len)))
        .ok_or_else(bad)
}
#[allow(clippy::too_many_arguments)]
pub(in crate::world) fn commit_current_observation_v2<M, P, V, R, W, E, C, PE, PC, CW, ME>(
    published: PublishedCurrentObservationV2,
    registry: ReservedValue<CurrentObservationRegistryEntryV2>,
    header: &header::PreparedFreshLifecycleHeaderV1,
    normal_parent: &ReservedValue<BoundNormalTickParentV1<'_>>,
    sources: &super::emergency_close::EmergencyCloseSourcesV1<'_, '_>,
    state: &M,
    ctx: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    parent: &mut WorldFieldReadContextV1<'_, P, V, PE, PC>,
    write: &mut MemberRecordWriteContext<'_, W, CW>,
    materials: &mut CorePolicyMaterialContextV1<'_, M, ME>,
    work: &mut IndustrialDeliveryWorkV1,
    common: &mut FormalBoundedContextV1<'_>,
    now: u64,
    live: &ExecutionFence,
) -> Result<CommittedCurrentObservationV2, IndustrialDeliveryErrorV1>
where
    M: MerkleNodeReader,
    P: MerkleNodeReader,
    V: ValueNodeReader,
    R: ValueNodeReader,
    W: ValueNodeReader + ValueNodeWriter + StatePageWriter,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    PE: FnMut() -> Option<ValueTreeError>,
    PC: FnMut() -> bool,
    CW: FnMut() -> bool,
    ME: FnMut() -> Option<ValueTreeError>,
{
    // This validates the physical stream receipt against its actual complete
    // coverage and cut before taking the original exclusive child adapter.
    published
        .validate(normal_parent, ctx.transaction, common, live)
        .map_err(|e| BodyMutationError::Original(Box::new(ObservationCommitSourceErrorV2(e))))?;
    super::super::delivery::paid_io::run(
        ctx,
        parent,
        write,
        materials,
        work,
        common,
        |ctx, _parent, write, _materials, work, tail| {
            if now == 0
                || !now.is_multiple_of(60)
                || published.observation().window_start != now - 60
                || registry.registered().is_some()
            {
                return Err(bad().into());
            }
            let crate::DomainEvent::WorldRuleCommitted {
                actor_id,
                request_id,
                command,
                ..
            } = header.event()
            else {
                return Err(bad().into());
            };
            let WorldRuleCommandV1::EmergencySupply {
                command: EmergencySupplyCommandV1::Observe { region_id },
            } = command
            else {
                return Err(bad().into());
            };
            tail.charge(region_id.len().min(published.region().len()))?;
            if region_id != published.region()
                || header.identity != tail.identity()
                || header.now != now
                || !header.memory.same_pool(tail.memory())
            {
                return Err(bad().into());
            }
            let payload = header_bytes(header.event())?;
            let fee = tail.reserve(
                payload
                    .checked_add(std::mem::size_of::<CommittedCurrentObservationV2>())
                    .and_then(|n| n.checked_add(1024))
                    .ok_or_else(bad)?,
                MemoryClass::Ordinary,
            )?;
            tail.charge(payload.checked_add(512).ok_or_else(bad)?)?;
            let receipt_fee = tail.reserve(
                actor_id
                    .len()
                    .checked_add(request_id.len())
                    .and_then(|n| n.checked_add(region_id.len()))
                    .and_then(|n| n.checked_add(256 + std::mem::size_of::<WorldRuleReceiptV1>()))
                    .ok_or_else(bad)?,
                MemoryClass::Ordinary,
            )?;
            let mut original = String::with_capacity(32 + 64);
            original.push_str("current-observation-original-v2:");
            original.push_str(
                blake3::Hash::from(*published.original_id())
                    .to_hex()
                    .as_str(),
            );
            let receipt = receipt_fee.retain(WorldRuleReceiptV1 {
                request_id: request_id.clone(),
                actor_id: actor_id.clone(),
                command: command.clone(),
                committed_at: now,
                result_id: original,
            });
            let result_id = oasis7_world_state::world_rules::emergency_observation_snapshot_key_v1(
                region_id,
                now - 60,
            );
            let event = header.event().clone();
            // Original Observe materializes every eligible minimum-service
            // registration in Emergency.services before storing the snapshot.
            // Rebind after each real write; no stale source spans a new body.
            for service_source in &published._services {
                let service = service_source.service();
                tail.charge(service.obligation_id.len())?;
                let guard = ctx.transaction.guard();
                let current = oasis7_world_state::incremental_world_rules_family::bind_current_emergency_lifecycle_v1(
                    sources.rules.emergency(), sources.delivery.eligibility.roots, state, ctx,
                    &mut work.fixed_scan, &mut work.composite_read, &guard, live,
                )?;
                let guard = ctx.transaction.guard();
                let selected = oasis7_world_state::incremental_world_rules_family::read_current_emergency_service_v1(
                    &current, sources.rules.emergency(), sources.delivery.eligibility.roots,
                    state, &service.obligation_id, ctx, &mut work.fixed_scan,
                    &mut work.composite_read, &guard, live,
                )?;
                let guard = ctx.transaction.guard();
                let _after = oasis7_world_state::incremental_world_rules_family::replace_current_emergency_service_v1(
                    &current, &selected, Some(service), sources.rules.emergency(),
                    sources.delivery.eligibility.roots, state, ctx, write, &mut work.fixed_scan,
                    &mut work.composite_read, &mut work.composite_write, &guard, live,
                )?;
            }

            let guard = ctx.transaction.guard();
            let requests = insert_current_observation_registry_v2(
                &registry,
                &receipt,
                sources.rules.requests(),
                sources.delivery.eligibility.roots,
                state,
                ctx,
                write,
                &mut work.fixed_scan,
                &mut work.composite_read,
                &mut work.composite_write,
                &guard,
                live,
            )?
            .ok_or_else(bad)?;
            Ok(CommittedCurrentObservationV2 {
                published,
                receipt,
                requests,
                event,
                result_id,
                guard: ctx.transaction.guard(),
                identity: tail.identity(),
                now,
                memory: tail.memory().clone(),
                _fee: fee,
            })
        },
    )
}
#[derive(Debug)]
struct ObservationCommitSourceErrorV2(
    crate::world::incremental_normal_world_rule_observation_v2::ObservationCurrentSourceErrorV2,
);
impl std::fmt::Display for ObservationCommitSourceErrorV2 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Current observation original: {:?}", self.0)
    }
}
impl std::error::Error for ObservationCommitSourceErrorV2 {}

/// Original Observe returns its snapshot key when this exact region/window
/// already exists, while still publishing the counted WorldRuleCommitted event.
/// It neither inserts an ordinary request receipt nor recomputes coverage.
pub(in crate::world) struct RetainedCurrentObservationV2 {
    registry: ReservedValue<CurrentObservationRegistryEntryV2>,
    event: crate::DomainEvent,
    result_id: String,
    guard: PreparedGuard,
    memory: MemoryBudget,
    identity: WorkIdentityV1,
    now: u64,
    _fee: MemoryReservation,
}
impl RetainedCurrentObservationV2 {
    pub(in crate::world) fn result_id(&self) -> &str {
        &self.result_id
    }
    pub(in crate::world) fn validate<M: MerkleNodeReader>(
        &self,
        header: &header::PreparedFreshLifecycleHeaderV1,
        tx: &ExecutionTransaction<'_, M>,
        tail: &mut NormalTailWorkV1<'_, '_>,
        now: u64,
        live: &ExecutionFence,
    ) -> Result<(), BodyMutationError> {
        tx.validate_guard(&self.guard, live)?;
        tail.charge(
            header_bytes(header.event())?
                .checked_add(header_bytes(&self.event)?)
                .ok_or_else(bad)?,
        )?;
        if self.now != now
            || self.identity != tail.identity()
            || !self.memory.same_pool(tail.memory())
            || !self.registry.uses_budget(tail.memory())
            || self.registry.registered().is_none()
            || self.event != *header.event()
        {
            return Err(bad());
        }
        Ok(())
    }
}
pub(in crate::world) fn retain_current_observation_v2<M: MerkleNodeReader>(
    registry: ReservedValue<CurrentObservationRegistryEntryV2>,
    header: &header::PreparedFreshLifecycleHeaderV1,
    tx: &ExecutionTransaction<'_, M>,
    tail: &mut NormalTailWorkV1<'_, '_>,
    now: u64,
    live: &ExecutionFence,
) -> Result<RetainedCurrentObservationV2, IndustrialDeliveryErrorV1> {
    if !registry.uses_budget(tail.memory()) { return Err(bad().into()); }
    registry.validate_current_scope(tx, tail.memory(), live)?;
    let crate::DomainEvent::WorldRuleCommitted {
        command:
            WorldRuleCommandV1::EmergencySupply {
                command: EmergencySupplyCommandV1::Observe { region_id },
            },
        ..
    } = header.event()
    else {
        return Err(bad().into());
    };
    tail.charge(region_id.len().min(registry.region().len()))?;
    if now == 0
        || !now.is_multiple_of(60)
        || registry.window_start() != now - 60
        || registry.region() != region_id
        || registry.registered().is_none()
        || header.now != now
        || header.identity != tail.identity()
        || !header.memory.same_pool(tail.memory())
    {
        return Err(bad().into());
    }
    let size = header_bytes(header.event())?
        .checked_add(std::mem::size_of::<RetainedCurrentObservationV2>())
        .and_then(|n| n.checked_add(512))
        .ok_or_else(bad)?;
    let fee = tail.reserve(size, MemoryClass::Ordinary)?;
    tail.charge(size)?;
    let result_id =
        oasis7_world_state::world_rules::emergency_observation_snapshot_key_v1(region_id, now - 60);
    Ok(RetainedCurrentObservationV2 {
        registry,
        event: header.event().clone(),
        result_id,
        guard: tx.guard(),
        memory: tail.memory().clone(),
        identity: tail.identity(),
        now,
        _fee: fee,
    })
}
