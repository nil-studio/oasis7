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
        resolve_current_industrial_state_owner_v1,
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
        let guard = &ctx.transaction.guard();
        let agent_exists = read_current_agent_presence_v1(
            &order.owner_id,
            sources.agents,
            sources.roots,
            ctx,
            read,
            &mut work.pending_entries_remaining,
            guard,
            live,
        )? == CurrentAgentPresenceV1::Existing;
        if !agent_exists {
            let error = validate_primary_production_facts_v1(
                PrimaryProductionFactsV1 {
                    agent_exists: false,
                    starter_claim: None,
                },
                &order.owner_id,
            )
            .err()
            .ok_or(ValueTreeError::Invalid(
                "industrial primary missing predicate",
            ))?;
            return Ok(fee.retain(CurrentIndustrialBlockerV1 {
                blocker: Some(bounded_debug(&error, cap)?),
            }));
        }
        let starter = {
            let mut claims = WorldFieldReadContextV1 {
                root: sources.roots.state_root(StateNamespaceV1::Authorization),
                state: read.state,
                values: ValueReadContext {
                    source: read.values.source,
                    memory: read.values.memory,
                    class: read.values.class,
                    io: &mut *read.values.io,
                },
                key_work: &mut *read.key_work,
                semantic_bytes: read.semantic_bytes,
                record_bytes: read.record_bytes,
                take_state_error: &mut read.take_state_error,
                cancelled: &mut read.cancelled,
            };
            let guard = &ctx.transaction.guard();
            recover_current_starter_oc_claim_v1(
                &order.owner_id,
                sources.native,
                sources.roots,
                ctx.transaction,
                &mut claims,
                &mut work.scan,
                &mut work.typed,
                guard,
                live,
            )?
        };
        let primary = PrimaryProductionFactsV1 {
            agent_exists,
            starter_claim: starter.as_ref().map(|row| row.value().value()),
        };
        if let Err(error) = validate_primary_production_facts_v1(primary, &order.owner_id) {
            return Ok(fee.retain(CurrentIndustrialBlockerV1 {
                blocker: Some(bounded_debug(&error, cap)?),
            }));
        }
        work.read.cell()?;
        let retired = world_id_set_contains_overlay_v1(
            WorldIdSetFieldV1::RetiredFactories,
            WorldIdSetKeyRefV1::Text(&order.factory_id),
            ctx,
        )?;
        // Preserve business short circuiting: do not read policy/factory after
        // an earlier retired decision, or unrelated damaged data could win.
        let policy = if retired {
            None
        } else {
            let mut policy_read = WorldFieldReadContextV1 {
                root: sources.roots.state_root(StateNamespaceV1::Business),
                state: read.state,
                values: ValueReadContext {
                    source: read.values.source,
                    memory: read.values.memory,
                    class: read.values.class,
                    io: &mut *read.values.io,
                },
                key_work: &mut *read.key_work,
                semantic_bytes: read.semantic_bytes,
                record_bytes: read.record_bytes,
                take_state_error: &mut read.take_state_error,
                cancelled: &mut read.cancelled,
            };
            Some(recover_gameplay_policy_overlay_v1(
                ctx.transaction,
                live,
                &mut policy_read,
                &mut work.scan,
                &mut work.typed,
            )?)
        };
        let mut blocked = false;
        if let Some(policy) = policy.as_ref() {
            for id in &policy.value().value().blocked_agents {
                work.read.comparison_bytes_remaining = work
                    .read
                    .comparison_bytes_remaining
                    .checked_sub(id.len() as u64)
                    .ok_or(ValueTreeError::Invalid("industrial policy comparison work"))?;
                if (ctx.cancelled)() {
                    return Err(ValueTreeError::Cancelled.into());
                }
                blocked |= id == &order.owner_id;
            }
        }
        let factory = if retired || blocked {
            None
        } else {
            let guard = &ctx.transaction.guard();
            read_current_production_factory_v1(
                &order.factory_id,
                sources.factories,
                ctx,
                &mut work.read,
                guard,
                live,
            )?
        };
        let mut facts = ProductionEligibilityFactsV1 {
            primary,
            retired,
            blocked,
            factory: factory.as_ref().map(|f| FactoryProductionFactsV1 {
                builder_agent_id: &f.builder_agent_id,
                builtin_spec_available: f.builtin_spec_available,
                spec_matches_builtin: f.spec_matches_builtin,
                recipe_slots: f.recipe_slots,
                durability_ppm: f.durability_ppm,
                input_ledger: &f.input_ledger,
                site_location_id: f.site_location_id.as_deref(),
            }),
            site: None,
            anchor_active: None,
            resolved_owner: None,
        };
        // Check the prefix before opening later authority or owner records.
        if retired
            || blocked
            || factory.as_ref().is_none_or(|f| {
                !f.builtin_spec_available
                    || f.builder_agent_id != order.owner_id
                    || !f.spec_matches_builtin
                    || f.recipe_slots < 2
                    || f.durability_ppm <= 0
            })
        {
            let error = production_eligibility_from_facts_v1(
                facts,
                &order.owner_id,
                &order.factory_id,
                &order.quote.delivery_location_id,
            )
            .err()
            .ok_or(ValueTreeError::Invalid(
                "industrial prefix predicate mismatch",
            ))?;
            return Ok(fee.retain(CurrentIndustrialBlockerV1 {
                blocker: Some(bounded_debug(&error, cap)?),
            }));
        }
        let factory = factory.as_ref().ok_or(ValueTreeError::Invalid(
            "industrial working factory vanished",
        ))?;
        let guard = &ctx.transaction.guard();
        let site = read_current_production_site_v1(
            &factory.site_id,
            &order.owner_id,
            sources.sites,
            state,
            ctx,
            &mut work.scan,
            &mut work.read,
            guard,
            live,
        )?;
        facts.site = site.as_ref().map(|s| FactorySiteProductionFactsV1 {
            active: s.active,
            chunk_ready: s.chunk_ready,
            location_id: &s.location_id,
            owner_agent_id: &s.owner_agent_id,
            owner_authorized: s.owner_authorized,
        });
        if site.is_some() {
            let guard = &ctx.transaction.guard();
            facts.anchor_active = read_current_production_anchor_v1(
                &order.quote.delivery_location_id,
                sources.anchors,
                state,
                ctx,
                &mut work.scan,
                &mut work.read,
                guard,
                live,
            )?;
        }
        if let Err(error) = production_eligibility_from_facts_v1(
            facts,
            &order.owner_id,
            &order.factory_id,
            &order.quote.delivery_location_id,
        ) {
            return Ok(fee.retain(CurrentIndustrialBlockerV1 {
                blocker: Some(bounded_debug(&error, cap)?),
            }));
        }
        if factory.input_ledger != order.quote.input_ledger {
            // Let the original shared kernel own the exact business error.
            let error = order_production_eligibility_from_facts_v1(facts, order)
                .err()
                .ok_or(ValueTreeError::Invalid(
                    "industrial ledger predicate mismatch",
                ))?;
            return Ok(fee.retain(CurrentIndustrialBlockerV1 {
                blocker: Some(bounded_debug(&error, cap)?),
            }));
        }
        let guard = &ctx.transaction.guard();
        let owner = resolve_current_industrial_state_owner_v1(
            sources,
            &order.owner_id,
            state,
            ctx,
            read,
            work,
            guard,
            live,
        )?;
        facts.resolved_owner = owner.as_ref().map(|o| ResolvedProductionOwnerFactsV1 {
            player_id: &o.player_id,
            source_id: &o.source_id,
            public_key: o.public_key.as_deref(),
        });
        let blocker = order_production_eligibility_from_facts_v1(facts, order)
            .err()
            .map(|e| bounded_debug(&e, cap))
            .transpose()?;
        if (ctx.cancelled)() {
            return Err(ValueTreeError::Cancelled.into());
        }
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
fn validate_current_order_budgeted_v1<M, R, E, C>(
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

pub(in crate::world) use incremental_industrial_eligibility_owner::safe_alternative_identity::read_current_safe_alternative_identity_v1;
pub(in crate::world) use incremental_industrial_eligibility_owner::{
    CurrentStateOwnerSourcesV1, OwnedCurrentIndustrialStateOwnerV1, resolve_current_state_owner_v1,
};
