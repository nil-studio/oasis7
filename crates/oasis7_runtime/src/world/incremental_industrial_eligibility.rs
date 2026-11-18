//! Actual single-producing-order eligibility in the sole Current transaction.
#[path = "incremental_industrial_eligibility_authority.rs"]
mod incremental_industrial_eligibility_authority;
#[path = "incremental_industrial_eligibility_factory.rs"]
pub(super) mod incremental_industrial_eligibility_factory;
#[path = "incremental_industrial_eligibility_owner.rs"]
mod incremental_industrial_eligibility_owner;
use self::{
    incremental_industrial_eligibility_authority::{
        read_current_production_anchor_v1, read_current_production_site_v1,
    },
    incremental_industrial_eligibility_factory::{
        IndustrialEligibilityReadWorkV1, read_current_production_factory_v1,
    },
};
use super::incremental_factory_directory_orders::{
    CurrentFactoryOrderDirectoryV1, FactoryOrderDirectoryErrorV1,
};
use oasis7_formal_protocol::{
    incremental_execution::{ExecutionError, ExecutionFence, PreparedGuard, RootSetV1},
    incremental_state::{MerkleNodeReader, StateNamespaceV1, StateScanBudget},
    incremental_value::{ValueNodeReader, ValueReadContext, ValueTreeError},
};
use oasis7_proto::memory_budget::{MemoryClass, ReservedValue};
use oasis7_world_state::{
    incremental_agent_authority_records::AgentAuthorityFamilyQualificationV1,
    incremental_agent_subtree::{
        AgentSubtreeStructureQualificationV1, CurrentAgentPresenceV1,
        read_current_agent_presence_v1,
    },
    incremental_factory_authority_records::FactoryAuthorityFamilyQualificationV1,
    incremental_factory_records::FactoryFamilyQualificationV1,
    incremental_field_overlay::{WorldFieldOverlayContextV1, WorldFieldOverlayErrorV1},
    incremental_field_read::WorldFieldReadContextV1,
    incremental_runtime_artifact_resources::RuntimeArtifactResourcesQualificationV1,
    incremental_runtime_revocation::RevocationReadWorkV1 as TypedRecordReadWorkV1,
    incremental_runtime_revocation::{RevocationReadWorkV1, RuntimeRevocationQualificationV1},
    incremental_world_id_sets::{
        WorldIdSetFieldV1, WorldIdSetKeyRefV1, world_id_set_contains_overlay_v1,
    },
    incremental_world_native::recover_gameplay_policy_overlay_v1,
    incremental_world_native::{
        WorldNativeFieldsQualificationV1, recover_current_starter_oc_claim_v1,
    },
    incremental_world_rule_composite_family::read_current_agent_order_v1,
    world_rules::{AgentIndustrialPhaseV1, industrial_eligibility_kernel::*},
};
#[derive(Debug)]
pub(super) enum IndustrialEligibilityErrorV1 {
    Execution(ExecutionError),
    Value(ValueTreeError),
    Overlay(WorldFieldOverlayErrorV1),
    Directory(FactoryOrderDirectoryErrorV1),
}
impl From<ExecutionError> for IndustrialEligibilityErrorV1 {
    fn from(e: ExecutionError) -> Self {
        Self::Execution(e)
    }
}
impl From<ValueTreeError> for IndustrialEligibilityErrorV1 {
    fn from(e: ValueTreeError) -> Self {
        Self::Value(e)
    }
}
impl From<WorldFieldOverlayErrorV1> for IndustrialEligibilityErrorV1 {
    fn from(e: WorldFieldOverlayErrorV1) -> Self {
        Self::Overlay(e)
    }
}
impl From<FactoryOrderDirectoryErrorV1> for IndustrialEligibilityErrorV1 {
    fn from(e: FactoryOrderDirectoryErrorV1) -> Self {
        Self::Directory(e)
    }
}

pub(super) struct IndustrialEligibilitySourcesV1<'a, 'source> {
    pub roots: &'a RootSetV1,
    pub orders: &'a ReservedValue<CurrentFactoryOrderDirectoryV1<'source>>,
    pub agents: &'a ReservedValue<AgentSubtreeStructureQualificationV1>,
    pub native: &'a ReservedValue<WorldNativeFieldsQualificationV1>,
    pub factories: &'a ReservedValue<FactoryFamilyQualificationV1>,
    pub sites: &'a ReservedValue<FactoryAuthorityFamilyQualificationV1>,
    pub anchors: &'a ReservedValue<AgentAuthorityFamilyQualificationV1>,
    pub revocation: &'a ReservedValue<RuntimeRevocationQualificationV1>,
    pub artifacts: &'a ReservedValue<RuntimeArtifactResourcesQualificationV1>,
}
pub(super) struct IndustrialEligibilityWorkV1 {
    pub read: IndustrialEligibilityReadWorkV1,
    pub typed: TypedRecordReadWorkV1,
    pub scan: StateScanBudget,
    pub revocation: RevocationReadWorkV1,
    pub pending_entries_remaining: usize,
}
/// This owner is plain business output. None means the original producing-order
/// predicate has no blocker; it does not mint dispatch/commit authority.
pub(super) struct CurrentIndustrialBlockerV1 {
    pub blocker: Option<String>,
}

/// Execute the original blocker predicates for one actual Producing order.
/// The complete directory selects the ID; the order is decoded from its real
/// Current source, and every dependent observation uses this same transaction.
#[allow(clippy::too_many_arguments)]
pub(super) fn current_producing_order_blocker_v1<M, R, E, C, P, V, ER, CR>(
    sources: &IndustrialEligibilitySourcesV1<'_, '_>,
    order_id: &str,
    now: crate::WorldTime,
    state: &M,
    ctx: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    read: &mut WorldFieldReadContextV1<'_, P, V, ER, CR>,
    work: &mut IndustrialEligibilityWorkV1,
    guard: &PreparedGuard,
    live: &ExecutionFence,
) -> Result<ReservedValue<CurrentIndustrialBlockerV1>, IndustrialEligibilityErrorV1>
where
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    P: MerkleNodeReader,
    V: ValueNodeReader,
    ER: FnMut() -> Option<ValueTreeError>,
    CR: FnMut() -> bool,
{
    use self::incremental_industrial_eligibility_owner::{
        read_current_industrial_identity_owner_v1, read_current_industrial_world_binding_v1,
    };
    let result = (|| {
        ctx.transaction.validate_guard(guard, live)?;
        sources.orders.validate_current(ctx, guard, live)?;
        if !sources.orders.uses_budget(ctx.values.memory)
            || ctx.values.class != MemoryClass::Ordinary
            || !ctx.transaction.uses_budget(ctx.values.memory)
        {
            return Err(ValueTreeError::Invalid("industrial blocker pool/class").into());
        }
        let cap = usize::try_from(work.read.max_text_bytes)
            .map_err(|_| ValueTreeError::Invalid("industrial blocker text limit"))?;
        work.read.comparison_bytes_remaining = work
            .read
            .comparison_bytes_remaining
            .checked_sub(work.read.max_text_bytes)
            .ok_or(ValueTreeError::Invalid(
                "industrial blocker formatting work",
            ))?;
        // Original validation and the shared pure kernel allocate one of
        // these closed reason Strings before the bounded Debug projection.
        // Their transient allocation does not depend on the output text limit.
        let reason_bytes = [
            "manufacturing_recovery_incomplete",
            "manufacturing_refund_recovery_incomplete",
            "primary_bound_agent_missing",
            "primary_bound_agent_required",
            "primary_owner_source_unavailable",
            "factory_retired",
            "production_authority_blocked",
            "factory_missing",
            "assembler_policy_unavailable",
            "owned_working_assembler_required",
            "factory_site_authority_unavailable",
            "factory_location_unavailable",
            "factory_authority_changed",
            "manufacturing_input_source_changed",
            "manufacturing_root_owner_changed",
        ]
        .iter()
        .map(|reason| reason.len())
        .max()
        .unwrap_or(0);
        let _business_error_phase = ctx
            .values
            .memory
            .reserve(
                reason_bytes + std::mem::size_of::<oasis7_world_state::WorldError>(),
                MemoryClass::Ordinary,
            )
            .map_err(ValueTreeError::Memory)?;
        let bytes = cap
            .checked_mul(2)
            .and_then(|n| n.checked_add(std::mem::size_of::<CurrentIndustrialBlockerV1>()))
            .ok_or(ValueTreeError::Invalid("industrial blocker output bound"))?;
        let fee = ctx
            .values
            .memory
            .reserve(bytes, MemoryClass::Ordinary)
            .map_err(ValueTreeError::Memory)?;
        // Establish membership using the genuine complete, ordered selection.
        let mut selected = false;
        for id in sources.orders.iter_order_ids() {
            work.read.comparison_bytes_remaining = work
                .read
                .comparison_bytes_remaining
                .checked_sub(id.len() as u64)
                .ok_or(ValueTreeError::Invalid("industrial order selection work"))?;
            if (ctx.cancelled)() {
                return Err(ValueTreeError::Cancelled.into());
            }
            selected |= id == order_id;
        }
        if !selected {
            return Err(
                ValueTreeError::Invalid("industrial order outside Current directory").into(),
            );
        }
        // First reads advance the transaction read-set revision. The caller
        // guard was checked at entry; each exclusive child query gets its own
        // exact current guard after preceding authenticated reads complete.
        let guard = &ctx.transaction.guard();
        let owned = read_current_agent_order_v1(
            sources.orders.source(),
            order_id,
            state,
            ctx,
            &mut work.scan,
            &mut work.typed,
            guard,
            live,
        )?
        .ok_or(ValueTreeError::Invalid(
            "selected Current industrial order absent",
        ))?;
        let order = owned.value();
        if order.phase != AgentIndustrialPhaseV1::Producing {
            return Err(ValueTreeError::Invalid(
                "industrial depreciation requires Producing order",
            )
            .into());
        }
        if let Err(error) = validate_current_order_budgeted_v1(order, &owned, ctx, work)? {
            return Ok(fee.retain(CurrentIndustrialBlockerV1 {
                blocker: Some(bounded_debug(&error, cap)?),
            }));
        }
        let guard = &ctx.transaction.guard();
        let binding =
            read_current_industrial_world_binding_v1(sources, state, ctx, read, work, guard, live)?;
        if order.quote.world_binding.as_deref() != binding.as_deref() {
            return Ok(fee.retain(CurrentIndustrialBlockerV1 {
                blocker: Some(bounded_literal(
                    "manufacturing_world_binding_mismatch",
                    cap,
                )?),
            }));
        }
        let guard = &ctx.transaction.guard();
        let identity = read_current_industrial_identity_owner_v1(
            sources,
            &order.owner_id,
            state,
            ctx,
            read,
            work,
            guard,
            live,
        )?;
        if identity
            .as_deref()
            .is_some_and(|owner| owner != order.quote.owner_player_id)
        {
            return Ok(fee.retain(CurrentIndustrialBlockerV1 {
                blocker: Some(bounded_literal(
                    "manufacturing_owner_identity_changed",
                    cap,
                )?),
            }));
        }
        if order
            .production_deadline()
            .is_some_and(|deadline| now >= deadline)
        {
            return Ok(fee.retain(CurrentIndustrialBlockerV1 {
                blocker: Some(bounded_literal("manufacturing_production_expired", cap)?),
            }));
        }
        let observed = delivery_facts::from_order(sources,order_id,order,state,ctx,read,work,live)?;
        let blocker=order_production_eligibility_from_facts_v1(observed.facts(),order)
            .err().map(|e|bounded_debug(&e,cap)).transpose()?;
        Ok(fee.retain(CurrentIndustrialBlockerV1 { blocker }))
    })();
    let result = result.and_then(|output| {
        if (ctx.cancelled)() || (read.cancelled)() {
            Err(ValueTreeError::Cancelled.into())
        } else {
            Ok(output)
        }
    });
    if result.is_err() {
        let _: Result<(), ExecutionError> =
            ctx.transaction.contribute(|_| Err(ExecutionError::Aborted));
    }
    result
}

fn bounded_literal(text: &str, cap: usize) -> Result<String, ValueTreeError> {
    if text.len() > cap {
        return Err(ValueTreeError::Invalid("industrial blocker output limit"));
    }
    Ok(text.to_owned())
}
fn bounded_debug(value: &impl std::fmt::Debug, cap: usize) -> Result<String, ValueTreeError> {
    use std::fmt::Write;
    struct Output {
        value: String,
        cap: usize,
    }
    impl std::fmt::Write for Output {
        fn write_str(&mut self, text: &str) -> std::fmt::Result {
            if text.len() > self.cap.saturating_sub(self.value.len()) {
                return Err(std::fmt::Error);
            }
            self.value.push_str(text);
            Ok(())
        }
    }
    let mut output = Output {
        value: String::with_capacity(cap),
        cap,
    };
    write!(&mut output, "{value:?}")
        .map_err(|_| ValueTreeError::Invalid("industrial blocker debug output limit"))?;
    Ok(output.value)
}

/// The actual typed owner admits every heap allocation in the closed order.
/// Cloning its quote clones only a subset of that owned graph: Strings clone
/// their lengths, the sole String/i64 BTreeMap retains the same entry graph,
/// MaterialLedgerId owns at most one String, and AgentClaimCostQuote is scalar.
/// Keep both the original owner and this additional clone admission alive.
pub(in crate::world) fn validate_current_order_budgeted_v1<M, R, E, C>(
    order: &oasis7_world_state::world_rules::AgentIndustrialOrderV1,
    owned: &oasis7_world_state::incremental_runtime_revocation::RevocationRecoveredV1<
        oasis7_world_state::world_rules::AgentIndustrialOrderV1,
    >,
    ctx: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    work: &mut IndustrialEligibilityWorkV1,
) -> Result<Result<(), oasis7_world_state::WorldError>, IndustrialEligibilityErrorV1>
where
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
{
    use std::io::Write;
    // Lock the clone-subgraph argument to the currently audited closed DTO.
    let oasis7_world_state::world_rules::AgentManufacturingQuoteV1 {
        policy_version: _,
        world_binding: _,
        recipe_id: _,
        owner_id: _,
        owner_player_id: _,
        owner_source_id: _,
        factory_id: _,
        quoted_at: _,
        expires_at: _,
        quote_digest: _,
        dependency_digest: _,
        delivery_location_id: _,
        inputs: _,
        input_ledger: _,
        electricity: _,
        claim_cost,
        upfront_liquid: _,
        liquid_after_reservation: _,
        electricity_after_reservation: _,
        maintenance_per_tick: _,
    } = &order.quote;
    let oasis7_world_state::AgentClaimCostQuote {
        reputation_tier: _,
        claim_cap: _,
        slot_index: _,
        activation_fee_amount: _,
        activation_fee_burn_amount: _,
        activation_fee_treasury_amount: _,
        claim_bond_amount: _,
        upkeep_per_epoch: _,
        release_cooldown_epochs: _,
        grace_epochs: _,
        idle_warning_epochs: _,
        forced_idle_reclaim_epochs: _,
        forced_reclaim_penalty_bps: _,
    } = claim_cost;
    // No owned JSON buffer is constructed during this first pass. Serializer
    // output consumes the caller's continuous byte work and polls cancellation.
    struct Count<'a, C> {
        bytes: usize,
        remaining: &'a mut u64,
        cancelled: &'a mut C,
        error: Option<ValueTreeError>,
    }
    impl<C: FnMut() -> bool> Write for Count<'_, C> {
        fn write(&mut self, b: &[u8]) -> std::io::Result<usize> {
            let error = if (self.cancelled)() {
                Some(ValueTreeError::Cancelled)
            } else if let Some(next) = self.remaining.checked_sub(b.len() as u64) {
                *self.remaining = next;
                None
            } else {
                Some(ValueTreeError::Invalid(
                    "industrial validate JSON byte work",
                ))
            };
            if let Some(error) = error {
                self.error = Some(error);
                return Err(std::io::Error::other("industrial validate bounded JSON"));
            }
            self.bytes = match self.bytes.checked_add(b.len()) {
                Some(n) => n,
                None => {
                    self.error = Some(ValueTreeError::Invalid("industrial validate JSON length"));
                    return Err(std::io::Error::other("industrial validate JSON length"));
                }
            };
            Ok(b.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    let cloned_bytes =
        owned.retained_allocation_bytes_v1(&mut work.read.cells_remaining, &mut ctx.cancelled)?;
    let mut count = Count {
        bytes: 0,
        remaining: &mut work.read.comparison_bytes_remaining,
        cancelled: &mut ctx.cancelled,
        error: None,
    };
    let encoded = serde_json::to_writer(&mut count, &order.quote)
        .and_then(|_| serde_json::to_writer(&mut count, &order.order_id));
    if let Some(error) = count.error.take() {
        return Err(error.into());
    }
    encoded
        .map_err(|_| ValueTreeError::Invalid("industrial validate closed JSON serialization"))?;
    // serde_json::to_vec starts at 128 and grows geometrically. The full quote
    // includes its current digest, whereas validate hashes an empty digest, so
    // counting the unmodified quote is a conservative bound without cloning it.
    let json_bytes = count
        .bytes
        .checked_mul(2)
        .and_then(|n| n.checked_add(2 * 128))
        .ok_or(ValueTreeError::Invalid("industrial validate JSON capacity"))?;
    // Replaying both serializers and SHA byte scans is actual business work.
    let replay = u64::try_from(count.bytes)
        .map_err(|_| ValueTreeError::Invalid("industrial validate work conversion"))?
        .checked_mul(2)
        .ok_or(ValueTreeError::Invalid("industrial validate work overflow"))?;
    *count.remaining = count
        .remaining
        .checked_sub(replay)
        .ok_or(ValueTreeError::Invalid("industrial validate replay work"))?;
    let bytes = cloned_bytes
        .checked_add(json_bytes)
        .and_then(|n| {
            n.checked_add(std::mem::size_of::<
                oasis7_world_state::world_rules::AgentManufacturingQuoteV1,
            >())
        })
        .and_then(|n| n.checked_add(3 * 64 + "industrial:".len() + 64))
        .ok_or(ValueTreeError::Invalid("industrial validate phase bytes"))?;
    let _phase = ctx
        .values
        .memory
        .reserve(bytes, MemoryClass::Ordinary)
        .map_err(ValueTreeError::Memory)?;
    if (ctx.cancelled)() {
        return Err(ValueTreeError::Cancelled.into());
    }
    let result = order.validate();
    if (ctx.cancelled)() {
        return Err(ValueTreeError::Cancelled.into());
    }
    Ok(result)
}

pub(in crate::world) use incremental_industrial_eligibility_owner::{
    CurrentStateOwnerSourcesV1, OwnedCurrentIndustrialStateOwnerV1, resolve_current_state_owner_v1,
};

pub(in crate::world) use incremental_industrial_eligibility_owner::safe_alternative_current::{
    SafeAlternativeCurrentSourcesV1, check_current_safe_alternative_scope_v1,
};

// Fixed consumers reuse the original typed Current manifest binding algorithm.
pub(in crate::world) use incremental_industrial_eligibility_owner::read_current_world_binding_v1;

#[path="incremental_normal_world_rule_delivery_eligibility.rs"] mod delivery_facts;
pub(in crate::world) use delivery_facts::{
    CurrentOrderRefundSourceV1, read_current_order_refund_source_v1,
    read_current_world_rule_order_production_facts_v1,
    read_current_world_rule_order_refund_source_v1,
};

#[path="incremental_normal_world_rule_delivery_blocker.rs"] mod lifecycle_blocker;
pub(in crate::world) use lifecycle_blocker::current_lifecycle_order_delivery_blocker_v1;

#[path="incremental_normal_world_rule_delivery_view.rs"] mod world_rule_view;
pub(in crate::world) use world_rule_view::{CurrentWorldRuleOrderViewV1,read_current_world_rule_order_view_v1};

pub(in crate::world) use incremental_industrial_eligibility_owner::safe_alternative_owner;
