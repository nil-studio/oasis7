//! Current material-transit reducer and its private event capability.

use super::common::*;
use super::current_read::{
    CurrentLogisticsPathV1, CurrentLogisticsRouteV1, read_current_path_v1, read_current_route_v1,
    read_current_transit_v1,
};
use super::settlement::{
    MaterialTransitSettlementPlanV1, apply_current_material_credit_v1,
    apply_current_transit_logistics_v1, plan_current_transit_settlement_v1,
};

fn invalid_at(reason: &'static str) -> BodyMutationError {
    BodyMutationError::Invalid(reason)
}

pub(in crate::world) struct PreparedNormalMaterialTransitV1 {
    event: ReservedValue<crate::WorldEvent>,
    guard: PreparedGuard,
    identity: WorkIdentityV1,
    now: u64,
    memory: MemoryBudget,
    _event_phase: MemoryReservation,
}

impl PreparedNormalMaterialTransitV1 {
    pub(in crate::world) fn event(&self) -> &crate::WorldEvent {
        &self.event
    }

    pub(in crate::world) fn validate<M: MerkleNodeReader>(
        &self,
        transaction: &ExecutionTransaction<'_, M>,
        memory: &MemoryBudget,
        now: u64,
        identity: WorkIdentityV1,
        live: &ExecutionFence,
    ) -> Result<(), BodyMutationError> {
        transaction.validate_guard(&self.guard, live)?;
        if identity != self.identity
            || now != self.now
            || !transaction.uses_budget(memory)
            || !self.memory.same_pool(memory)
            || !self.event.uses_budget(memory)
        {
            return Err(BodyMutationError::Invalid(
                "MaterialTransitCompleted publication binding",
            ));
        }
        let crate::WorldEventBody::Domain(crate::DomainEvent::MaterialTransitCompleted { .. }) =
            &self.event.body
        else {
            return Err(BodyMutationError::Invalid(
                "MaterialTransitCompleted publication body",
            ));
        };
        if self.event.time != now || self.event.caused_by.is_some() {
            return Err(BodyMutationError::Invalid(
                "MaterialTransitCompleted publication source",
            ));
        }
        Ok(())
    }
}

/// Reconcile one parent-selected due transit from transaction Current. The
/// parent visit only supplies `job_id`; all business values are reopened from
/// the same transaction before a private system-event owner is minted.
#[allow(clippy::too_many_arguments)]
pub(in crate::world) fn reduce_current_material_transit_v1<
    M: MerkleNodeReader,
    P: MerkleNodeReader,
    V: ValueNodeReader,
    W: ValueNodeReader + ValueNodeWriter + StatePageWriter,
    D: FnMut() -> bool,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    PE: FnMut() -> Option<ValueTreeError>,
    PC: FnMut() -> bool,
    ME: FnMut() -> Option<ValueTreeError>,
>(
    job_id: u64,
    step: &mut IwsStepContext<'_, '_, '_, '_, M, V, W, D>,
    materials: &mut crate::world::incremental_body_event::materials::CorePolicyMaterialContextV1<
        '_,
        M,
        ME,
    >,
    parent: &BoundNormalTickParentV1<'_>,
    roots: &RootSetV1,
    current_read: &mut WorldFieldReadContextV1<'_, P, V, E, C>,
    agent_parent_read: &mut WorldFieldReadContextV1<'_, P, V, PE, PC>,
    factories: &ReservedValue<
        oasis7_world_state::incremental_factory_records::FactoryFamilyQualificationV1,
    >,
    factory_index: &ReservedValue<FactoryLifecycleIndexV1>,
    industry: &ReservedValue<IndustryFamilyQualificationV1>,
    logistics: &ReservedValue<LogisticsFamilyQualificationV1>,
    agents: &ReservedValue<AgentSubtreeStructureQualificationV1>,
    native: &ReservedValue<WorldNativeFieldsQualificationV1>,
    work: &mut FactoryLifecycleWorkV1,
    logistics_work: &mut LogisticsWorkBudgetV1,
    normal: &mut NormalTailWorkV1<'_, '_>,
    guard: &PreparedGuard,
    live: &ExecutionFence,
) -> Result<PreparedNormalMaterialTransitV1, BodyMutationError> {
    let result = (|| {
        let now = step.time();
        let memory = step.mutation.memory;
        step.transaction.validate_guard(guard, live)?;
        normal.validate_parent(parent)?;
        if current_read.values.class != MemoryClass::Ordinary
            || agent_parent_read.values.class != MemoryClass::Ordinary
            || !step.transaction.uses_budget(memory)
            || !normal.memory().same_pool(memory)
            || !current_read.values.memory.same_pool(memory)
            || !agent_parent_read.values.memory.same_pool(memory)
            || !materials.parent.uses_budget(memory)
            || !factories.uses_budget(memory)
            || !factory_index.uses_budget(memory)
            || !industry.uses_budget(memory)
            || !logistics.uses_budget(memory)
            || !agents.uses_budget(memory)
            || !native.uses_budget(memory)
        {
            return Err(BodyMutationError::Invalid(
                "material transit source pool/class",
            ));
        }
        let _root_digest_fee = charge_reserve(normal, &memory, 4096, "transit root digest")?;
        roots.validate().map_err(|_| invalid())?;
        let digest = roots.digest().map_err(|_| invalid())?;
        if digest != step.transaction.parent_fence().parent_roots {
            return Err(BodyMutationError::Invalid("material transit parent roots"));
        }
        if let Some(error) = (current_read.take_state_error)() {
            return Err(error.into());
        }
        if let Some(error) = (agent_parent_read.take_state_error)() {
            return Err(error.into());
        }

        // Match the original event application: both material projections are
        // normalized before the settled/no-op decision.
        materials.contribute(step.transaction, step.values, step.mutation, guard, live)?;
        factories
            .validate_binding(&digest)
            .map_err(BodyMutationError::Value)?;
        industry
            .validate_binding(&digest)
            .map_err(BodyMutationError::Value)?;
        logistics
            .validate_binding(&digest)
            .map_err(BodyMutationError::Value)?;
        factory_index
            .validate_binding(factories, industry, roots, memory, MemoryClass::Ordinary)
            .map_err(BodyMutationError::Value)?;
        native
            .validate_binding(roots)
            .map_err(BodyMutationError::Value)?;

        // Event allocation is observed before applying the event's state
        // transition, matching World::append_event ordering.
        let sequence =
            step.read_allocator(crate::world::incremental_allocators::RuntimeAllocatorV1::Event)?;
        let (event_id, _, _) = crate::World::preview_next_event_id(sequence.next, sequence.era);

        let current = read_current_transit_v1(
            job_id,
            step,
            current_read,
            parent,
            roots,
            normal,
            work.max_scalar_bytes,
            work.max_material_entries,
            &step.transaction.guard(),
            live,
        )?
        .ok_or_else(|| {
            super::common::original_world_error(crate::WorldError::ResourceBalanceInvalid {
                reason: format!("material transit completion has no pending job: job_id={job_id}"),
            })
        })?;
        let job = &current.job;

        let event_bytes = event_heap_bound(job)?;
        normal.charge(event_bytes)?;
        let event_phase_bytes = event_bytes
            .checked_mul(2)
            .and_then(|n| n.checked_add(size_of::<MaterialTransitJobState>()))
            .and_then(|n| n.checked_add(size_of::<PreparedNormalMaterialTransitV1>()))
            .ok_or(ValueTreeError::ResourceLimited("transit event phase"))?;
        let event_phase = memory
            .reserve(event_phase_bytes, MemoryClass::Ordinary)
            .map_err(ValueTreeError::Memory)?;

        let settled = {
            let stop = RefCell::new(&mut current_read.cancelled);
            let mutation_cancel = RefCell::new(&mut step.mutation.cancelled);
            let mut overlay = WorldFieldOverlayContextV1 {
                transaction: &mut *step.transaction,
                values: ValueReadContext {
                    source: current_read.values.source,
                    memory,
                    class: MemoryClass::Ordinary,
                    io: &mut *current_read.values.io,
                },
                key_work: &mut *current_read.key_work,
                semantic_bytes: current_read.semantic_bytes,
                record_bytes: current_read.record_bytes,
                take_state_error: &mut current_read.take_state_error,
                cancelled: || (stop.borrow_mut())() || (mutation_cancel.borrow_mut())(),
            };
            let settled = world_id_set_contains_overlay_v1(
                WorldIdSetFieldV1::SettledLogisticsTransits,
                WorldIdSetKeyRefV1::Action(job_id),
                &mut overlay,
            )
            .map_err(overlay_error)?;
            let tail = overlay.transaction.guard();
            overlay.transaction.validate_guard(&tail, live)?;
            settled
        };

        let mut plan_routes = BTreeMap::new();
        let mut existing_path = None;
        let mut existing_owners = BTreeSet::new();
        let no_op = settled;
        if !no_op {
            if now < job.ready_at {
                return Err(super::common::original_world_error(
                    crate::WorldError::ResourceBalanceInvalid {
                        reason: format!(
                            "material transit completion is early: job_id={job_id} ready_at={} now={now}",
                            job.ready_at
                        ),
                    },
                ));
            }
            if job.amount < 0 {
                return Err(super::common::original_world_error(
                    crate::WorldError::ResourceBalanceInvalid {
                        reason: format!(
                            "material transit completion does not match pending commitment: job_id={job_id}"
                        ),
                    },
                ));
            }
            let loss = crate::world::logistics::material_transit_loss_amount(
                job.amount,
                job.distance_km,
                job.loss_bps,
            );
            let received = job.amount.saturating_sub(loss);
            // Preserve legacy error precedence: destination balance overflow
            // is rejected before path/route settlement validation.
            apply_current_material_credit_v1(
                step,
                materials,
                current_read,
                &job.to_ledger,
                &job.kind,
                received,
                live,
            )?;

            let route_count = job.route_ids.len();
            let route_id_bytes = job.route_ids.iter().try_fold(0usize, |sum, id| {
                sum.checked_add(id.len())
                    .ok_or(ValueTreeError::ResourceLimited("transit route id bytes"))
            })?;
            let map_layout = route_count
                .checked_mul(size_of::<(
                    String,
                    Option<ReservedValue<CurrentLogisticsRouteV1>>,
                )>())
                .and_then(|n| n.checked_add(route_count.checked_mul(96)?))
                // The route IDs are copied into both the set and map below.
                .and_then(|n| n.checked_add(route_id_bytes.checked_mul(2)?))
                .and_then(|n| {
                    n.checked_add(size_of::<
                        BTreeMap<String, Option<ReservedValue<CurrentLogisticsRouteV1>>>,
                    >())
                })
                .ok_or(ValueTreeError::ResourceLimited("transit route map layout"))?;
            let _route_map_fee = charge_reserve(normal, memory, map_layout, "transit route map")?;
            let mut route_ids = BTreeSet::new();
            let route_id_set_fee = charge_reserve(
                normal,
                memory,
                route_count
                    .checked_mul(size_of::<String>() + 64)
                    .and_then(|n| n.checked_add(size_of::<BTreeSet<String>>()))
                    .ok_or(ValueTreeError::ResourceLimited("transit route id set"))?,
                "transit route id set",
            )?;
            let _owner_set_fee = charge_reserve(
                normal,
                memory,
                route_count
                    .checked_mul(
                        work.max_scalar_bytes
                            .checked_add(size_of::<String>() + 64)
                            .ok_or(ValueTreeError::ResourceLimited("transit route owner set"))?,
                    )
                    .and_then(|n| n.checked_add(size_of::<BTreeSet<String>>()))
                    .ok_or(ValueTreeError::ResourceLimited("transit route owner set"))?,
                "transit route owner set",
            )?;
            {
                let stop = RefCell::new(&mut current_read.cancelled);
                let mutation_cancel = RefCell::new(&mut step.mutation.cancelled);
                let mut overlay = WorldFieldOverlayContextV1 {
                    transaction: &mut *step.transaction,
                    values: ValueReadContext {
                        source: current_read.values.source,
                        memory,
                        class: MemoryClass::Ordinary,
                        io: &mut *current_read.values.io,
                    },
                    key_work: &mut *current_read.key_work,
                    semantic_bytes: current_read.semantic_bytes,
                    record_bytes: current_read.record_bytes,
                    take_state_error: &mut current_read.take_state_error,
                    cancelled: || (stop.borrow_mut())() || (mutation_cancel.borrow_mut())(),
                };
                for route_id in &job.route_ids {
                    if route_ids.contains(route_id) {
                        continue;
                    }
                    normal.charge(route_id.len())?;
                    route_ids.insert(route_id.clone());
                    let route = read_current_route_v1(
                        route_id,
                        work.max_scalar_bytes,
                        memory,
                        normal,
                        &mut |cell, mut output| {
                            read_logistics_cell_overlay_v1(
                                LogisticsRecordKeyV1 {
                                    kind: LogisticsRecordKindV1::Route,
                                    id: route_id,
                                },
                                cell,
                                &mut overlay,
                                work.max_scalar_bytes as u64,
                                &mut output,
                            )
                            .map_err(overlay_error)
                        },
                    )?;
                    if let Some(route) = route {
                        existing_owners.insert(route.value.owner_agent_id.clone());
                        plan_routes.insert(route_id.clone(), Some(route));
                    } else {
                        plan_routes.insert(route_id.clone(), None);
                    }
                }
                if let Some(path_id) = job.path_id.as_deref() {
                    existing_path = read_current_path_v1(
                        path_id,
                        work.max_scalar_bytes,
                        work.max_material_entries,
                        memory,
                        normal,
                        &mut |cell, mut output| {
                            read_logistics_cell_overlay_v1(
                                LogisticsRecordKeyV1 {
                                    kind: LogisticsRecordKindV1::Path,
                                    id: path_id,
                                },
                                cell,
                                &mut overlay,
                                work.max_scalar_bytes as u64,
                                &mut output,
                            )
                            .map_err(overlay_error)
                        },
                    )?;
                }
                let tail = overlay.transaction.guard();
                overlay.transaction.validate_guard(&tail, live)?;
            }
            let owner_id_bytes =
                existing_owners
                    .iter()
                    .try_fold(job.requester_agent_id.len(), |sum, id| {
                        sum.checked_add(id.len())
                            .ok_or(ValueTreeError::ResourceLimited("transit agent id bytes"))
                    })?;
            let _agent_set_fee = charge_reserve(
                normal,
                memory,
                existing_owners
                    .len()
                    .checked_add(1)
                    .and_then(|entries| entries.checked_mul(size_of::<String>() + 64))
                    .and_then(|n| n.checked_add(owner_id_bytes.checked_mul(2)?))
                    .and_then(|n| n.checked_add(size_of::<BTreeSet<String>>() * 2))
                    .ok_or(ValueTreeError::ResourceLimited("transit agent set layout"))?,
                "transit agent set",
            )?;
            normal.charge(job.requester_agent_id.len())?;
            let mut owners_with_requester = existing_owners;
            owners_with_requester.insert(job.requester_agent_id.clone());
            let mut actual_agents = BTreeSet::new();
            {
                let stop = RefCell::new(&mut current_read.cancelled);
                let mutation_cancel = RefCell::new(&mut step.mutation.cancelled);
                let mut overlay = WorldFieldOverlayContextV1 {
                    transaction: &mut *step.transaction,
                    values: ValueReadContext {
                        source: current_read.values.source,
                        memory,
                        class: MemoryClass::Ordinary,
                        io: &mut *current_read.values.io,
                    },
                    key_work: &mut *current_read.key_work,
                    semantic_bytes: current_read.semantic_bytes,
                    record_bytes: current_read.record_bytes,
                    take_state_error: &mut current_read.take_state_error,
                    cancelled: || (stop.borrow_mut())() || (mutation_cancel.borrow_mut())(),
                };
                for agent in &owners_with_requester {
                    let fresh = overlay.transaction.guard();
                    if oasis7_world_state::incremental_agent_subtree::read_current_agent_presence_v1(
                        agent,
                        agents,
                        roots,
                        &mut overlay,
                        agent_parent_read,
                        &mut work.pending_entries_remaining,
                        &fresh,
                        live,
                    )
                    .map_err(overlay_error)?
                        == oasis7_world_state::incremental_agent_subtree::CurrentAgentPresenceV1::Existing
                    {
                        actual_agents.insert(agent.clone());
                    }
                }
                let tail = overlay.transaction.guard();
                overlay.transaction.validate_guard(&tail, live)?;
            }
            existing_owners = actual_agents;
            let plan = plan_current_transit_settlement_v1(
                job,
                &plan_routes,
                existing_path.as_deref(),
                &existing_owners,
                false,
                now,
                memory,
                normal,
            )?;

            // Before the first logistics write, validate every electricity
            // credit against the exact Current scalar that will be CASed.
            let credit_layout = plan.owner_payouts.iter().try_fold(
                size_of::<BTreeMap<String, (Option<i64>, i64)>>(),
                |sum, (id, _)| {
                    sum.checked_add(size_of::<String>() + 96)
                        .and_then(|n| n.checked_add(id.len().checked_mul(2)?))
                        .ok_or(ValueTreeError::ResourceLimited(
                            "transit resource credit plan",
                        ))
                },
            )?;
            let credit_fee =
                charge_reserve(normal, memory, credit_layout, "transit resource credits")?;
            let mut credit_changes = BTreeMap::<String, (Option<i64>, i64)>::new();
            {
                let stop = RefCell::new(&mut current_read.cancelled);
                let mutation_cancel = RefCell::new(&mut step.mutation.cancelled);
                let mut overlay = WorldFieldOverlayContextV1 {
                    transaction: &mut *step.transaction,
                    values: ValueReadContext {
                        source: current_read.values.source,
                        memory,
                        class: MemoryClass::Ordinary,
                        io: &mut *current_read.values.io,
                    },
                    key_work: &mut *current_read.key_work,
                    semantic_bytes: current_read.semantic_bytes,
                    record_bytes: current_read.record_bytes,
                    take_state_error: &mut current_read.take_state_error,
                    cancelled: || (stop.borrow_mut())() || (mutation_cancel.borrow_mut())(),
                };
                for (agent, amount) in &plan.owner_payouts {
                    let guard = overlay.transaction.guard();
                    let before = read_agent_subtree_scalar_overlay_v1(
                        AgentSubtreeQualifiedRequestV1 {
                            cell: AgentSubtreeScalarRequestV1 {
                                agent,
                                field: AgentFieldV1::ResourceAmount,
                                ordinal: 0,
                            },
                            qualification: agents,
                        },
                        &mut overlay,
                        work.max_scalar_bytes as u64,
                        &mut std::io::sink(),
                        &guard,
                        live,
                    )
                    .map_err(overlay_error)?;
                    let before = match before {
                        None => None,
                        Some(AgentSubtreeValueV1::I64(value)) => Some(value),
                        _ => return Err(invalid_at("transit electricity Current scalar")),
                    };
                    let after = oasis7_agent_api::world_resource::checked_resource_add_v1(
                        oasis7_agent_api::world_action::ResourceKind::Electricity,
                        before.unwrap_or(0),
                        *amount,
                    )
                    .map_err(|error| {
                        super::common::original_world_error(
                            crate::WorldError::ResourceBalanceInvalid {
                                reason: format!("logistics tariff credit failed: {error:?}"),
                            },
                        )
                    })?;
                    normal.charge(agent.len())?;
                    credit_changes.insert(agent.clone(), (before, after));
                }
                let tail = overlay.transaction.guard();
                overlay.transaction.validate_guard(&tail, live)?;
            }

            let stop = RefCell::new(&mut current_read.cancelled);
            let mutation_cancel = RefCell::new(&mut step.mutation.cancelled);
            let mut overlay = WorldFieldOverlayContextV1 {
                transaction: &mut *step.transaction,
                values: ValueReadContext {
                    source: current_read.values.source,
                    memory,
                    class: MemoryClass::Ordinary,
                    io: &mut *current_read.values.io,
                },
                key_work: &mut *current_read.key_work,
                semantic_bytes: current_read.semantic_bytes,
                record_bytes: current_read.record_bytes,
                take_state_error: &mut current_read.take_state_error,
                cancelled: || (stop.borrow_mut())() || (mutation_cancel.borrow_mut())(),
            };
            let mut writer =
                oasis7_world_state::incremental_field_member::MemberRecordWriteContext {
                    sink: &mut *step.mutation.sink,
                    memory,
                    io: &mut *step.mutation.io,
                    key_work: &mut *materials.write_keys,
                    limits: materials.write_limits,
                    cancelled: || (stop.borrow_mut())() || (mutation_cancel.borrow_mut())(),
                };
            apply_current_transit_logistics_v1(
                &plan,
                job,
                logistics,
                &mut overlay,
                &mut writer,
                logistics_work,
                industry,
                &mut work.industry,
                logistics_work.max_pending_node_bytes as u64,
                live,
            )?;
            // The member writer borrows the shared mutation sink and IO lane.
            // Finish its route/path writes before opening the scalar writer
            // below; both writers still use the same transaction and budget.
            drop(writer);

            for (agent, (before, after)) in &credit_changes {
                let fresh = overlay.transaction.guard();
                let mut mutation = ValueMutationContextV1 {
                    sink: &mut *step.mutation.sink,
                    memory,
                    io: &mut *step.mutation.io,
                    cancelled: || (stop.borrow_mut())() || (mutation_cancel.borrow_mut())(),
                };
                change_agent_subtree_scalar_v1(
                    oasis7_world_state::incremental_agent_subtree::AgentSubtreeCellChangeV1 {
                        query: AgentSubtreeQualifiedRequestV1 {
                            cell: AgentSubtreeScalarRequestV1 {
                                agent,
                                field: AgentFieldV1::ResourceAmount,
                                ordinal: 0,
                            },
                            qualification: agents,
                        },
                        expected: before.map(AgentScalarRefV1::I64),
                        after: (*after != 0).then_some(AgentScalarRefV1::I64(*after)),
                        max_scalar_bytes: work.max_scalar_bytes as u64,
                    },
                    &mut overlay,
                    &mut mutation,
                    &fresh,
                    live,
                )
                .map_err(overlay_error)?;
            }

            drop(overlay);
            drop(credit_fee);
            drop(route_id_set_fee);

            if existing_owners.contains(&job.requester_agent_id) {
                normal.charge(
                    job.requester_agent_id
                        .len()
                        .checked_add(AGENT_BODY_VALUE_BYTES * 2)
                        .and_then(|n| n.checked_add(size_of::<AgentBodyValueV1>()))
                        .ok_or(ValueTreeError::ResourceLimited(
                            "transit requester activity work",
                        ))?,
                )?;
                let key = AgentBodyKeyV1::new(&job.requester_agent_id)
                    .map_err(BodyMutationError::Invalid)?;
                let body = AgentBodyValueV1::from_canonical_bytes(
                    &crate::world::incremental_body_event::read_fixed::<
                        AGENT_BODY_VALUE_BYTES,
                        _,
                        _,
                        _,
                        _,
                    >(
                        step.transaction,
                        oasis7_formal_protocol::incremental_state::StateNamespaceV1::Business,
                        key.as_bytes(),
                        step.values,
                        step.mutation,
                    )?,
                )
                .map_err(BodyMutationError::Invalid)?;
                if body.last_active != now {
                    let latest = step.transaction.guard();
                    step.transaction.validate_guard(&latest, live)?;
                    let after = AgentBodyValueV1 {
                        body_view: body.body_view,
                        last_active: now,
                    };
                    let reference =
                        crate::world::incremental_body::write_body_value(&after, step.mutation)?;
                    step.transaction.write(
                        oasis7_formal_protocol::incremental_state::StateNamespaceV1::Business,
                        key.as_bytes(),
                        Some(reference),
                    )?;
                }
            }

            // The original progress transition observes global factory stage,
            // then increments this completion's counter with saturating math.
            normal.charge(32 << 10)?;
            let mut progress_read = WorldFieldReadContextV1 {
                root: roots.state_root(
                    oasis7_formal_protocol::incremental_state::StateNamespaceV1::Authorization,
                ),
                state: current_read.state,
                values: ValueReadContext {
                    source: current_read.values.source,
                    memory,
                    class: MemoryClass::Ordinary,
                    io: &mut *current_read.values.io,
                },
                key_work: &mut *current_read.key_work,
                semantic_bytes: current_read.semantic_bytes,
                record_bytes: current_read.record_bytes,
                take_state_error: &mut current_read.take_state_error,
                cancelled: &mut current_read.cancelled,
            };
            let policy = recover_gameplay_policy_overlay_v1(
                step.transaction,
                live,
                &mut progress_read,
                &mut work.native_scan,
                &mut work.native_read,
            )
            .map_err(overlay_error)?;
            let taxes_enabled = policy.value().value().electricity_tax_bps > 0
                || policy.value().value().data_tax_bps > 0;
            let before_guard = step.transaction.guard();
            let before_progress = recover_current_factory_industry_progress_v1(
                native,
                roots,
                step.transaction,
                &mut progress_read,
                &mut work.native_scan,
                &mut work.native_read,
                &before_guard,
                live,
            )
            .map_err(overlay_error)?;
            let prior_transits = before_progress
                .value()
                .ok_or(invalid_at("industry progress absent"))?
                .completed_material_transits;
            let observation_guard = step.transaction.guard();
            let observation = observe_current_factory_lifecycle_v1(
                "",
                factory_index,
                factories,
                industry,
                roots,
                step.transaction,
                &mut progress_read,
                &mut work.factory,
                &mut work.industry,
                &mut work.lifecycle,
                work.max_scalar_bytes as u64,
                &observation_guard,
                live,
            )
            .map_err(overlay_error)?;
            observation
                .validate(step.transaction, memory, live)
                .map_err(overlay_error)?;
            let stage = oasis7_world_state::state::industry_transition::industry_stage_for_progress(
                observation.has_stable_production_line(),
                observation.stage_completed_jobs_total(),
                prior_transits.saturating_add(1),
                taxes_enabled,
            );
            let latest_guard = step.transaction.guard();
            let latest_progress = recover_current_factory_industry_progress_v1(
                native,
                roots,
                step.transaction,
                &mut progress_read,
                &mut work.native_scan,
                &mut work.native_read,
                &latest_guard,
                live,
            )
            .map_err(overlay_error)?;
            observation
                .validate(step.transaction, memory, live)
                .map_err(overlay_error)?;
            if latest_progress
                .value()
                .ok_or(invalid_at("industry progress absent"))?
                .completed_material_transits
                != prior_transits
            {
                return Err(invalid_at("industry progress changed"));
            }
            let mut progress_writer =
                oasis7_world_state::incremental_field_member::MemberRecordWriteContext {
                    sink: &mut *step.mutation.sink,
                    memory,
                    io: &mut *step.mutation.io,
                    key_work: &mut *materials.write_keys,
                    limits: materials.write_limits,
                    cancelled: || (step.mutation.cancelled)(),
                };
            increment_current_material_transit_progress_v1(
                &latest_progress,
                stage,
                now,
                step.transaction,
                &mut progress_read,
                &mut progress_writer,
                &mut work.native_scan,
                &mut work.native_read,
                &mut work.native_write,
                live,
            )
            .map_err(overlay_error)?;
            drop(progress_read);
            let event = prepare_material_transit_event_v1(job, event_id, now, memory, normal)?;
            let final_guard = step.transaction.guard();
            step.transaction.validate_guard(&final_guard, live)?;
            if (current_read.cancelled)() || (step.mutation.cancelled)() {
                return Err(ValueTreeError::Cancelled.into());
            }
            Ok(PreparedNormalMaterialTransitV1 {
                event,
                guard: final_guard,
                identity: normal.identity(),
                now,
                memory: memory.clone(),
                _event_phase: event_phase,
            })
        } else {
            let empty_routes = BTreeMap::new();
            let no_op_plan = plan_current_transit_settlement_v1(
                job,
                &empty_routes,
                None,
                &existing_owners,
                true,
                now,
                memory,
                normal,
            )?;
            drop(no_op_plan);
            let event = prepare_material_transit_event_v1(job, event_id, now, memory, normal)?;
            let final_guard = step.transaction.guard();
            step.transaction.validate_guard(&final_guard, live)?;
            Ok(PreparedNormalMaterialTransitV1 {
                event,
                guard: final_guard,
                identity: normal.identity(),
                now,
                memory: memory.clone(),
                _event_phase: event_phase,
            })
        }
    })();
    if result.is_err() {
        let _: Result<(), ExecutionError> = step
            .transaction
            .contribute(|_| Err(ExecutionError::Aborted));
    }
    result
}

fn event_heap_bound(job: &MaterialTransitJobState) -> Result<usize, BodyMutationError> {
    let mut bytes = size_of::<crate::WorldEvent>()
        .checked_add(size_of::<crate::DomainEvent>())
        .and_then(|n| n.checked_add(2 * size_of::<MaterialLedgerId>()))
        .and_then(|n| n.checked_add(size_of::<String>() * 5))
        .and_then(|n| n.checked_add(size_of::<Option<String>>() * 2))
        .and_then(|n| n.checked_add(job.route_ids.len().checked_mul(size_of::<String>())?))
        .and_then(|n| n.checked_add(job.requester_agent_id.len()))
        .and_then(|n| n.checked_add(job.kind.len()))
        .and_then(|n| n.checked_add(job.route_id.as_ref().map_or(0, String::len)))
        .and_then(|n| n.checked_add(job.path_id.as_ref().map_or(0, String::len)))
        .and_then(|n| {
            n.checked_add(match &job.from_ledger {
                MaterialLedgerId::World => 0,
                MaterialLedgerId::Agent(id)
                | MaterialLedgerId::Site(id)
                | MaterialLedgerId::Factory(id) => id.len(),
            })
        })
        .and_then(|n| {
            n.checked_add(match &job.to_ledger {
                MaterialLedgerId::World => 0,
                MaterialLedgerId::Agent(id)
                | MaterialLedgerId::Site(id)
                | MaterialLedgerId::Factory(id) => id.len(),
            })
        })
        .ok_or(ValueTreeError::ResourceLimited("transit event layout"))?;
    for id in &job.route_ids {
        bytes = bytes
            .checked_add(id.len())
            .ok_or(ValueTreeError::ResourceLimited("transit event route ids"))?;
    }
    Ok(bytes)
}

/// Build the exact legacy completion event only after the selected Current
/// row is reopened. Its bytes are prepaid; counted publication remains with
/// the private system-event sink.
pub(in crate::world) fn prepare_material_transit_event_v1(
    job: &MaterialTransitJobState,
    event_id: crate::WorldEventId,
    now: u64,
    memory: &MemoryBudget,
    normal: &mut NormalTailWorkV1<'_, '_>,
) -> Result<ReservedValue<crate::WorldEvent>, BodyMutationError> {
    let bytes = event_heap_bound(job)?;
    let fee = charge_reserve(normal, memory, bytes, "transit event allocation")?;
    let loss_amount = crate::world::logistics::material_transit_loss_amount(
        job.amount,
        job.distance_km,
        job.loss_bps,
    );
    let received_amount = job.amount.saturating_sub(loss_amount);
    Ok(fee.retain(crate::WorldEvent {
        id: event_id,
        time: now,
        caused_by: None,
        body: crate::WorldEventBody::Domain(crate::DomainEvent::MaterialTransitCompleted {
            job_id: job.job_id,
            requester_agent_id: job.requester_agent_id.clone(),
            from_ledger: job.from_ledger.clone(),
            to_ledger: job.to_ledger.clone(),
            kind: job.kind.clone(),
            sent_amount: job.amount,
            received_amount,
            loss_amount,
            distance_km: job.distance_km,
            priority: job.priority,
            route_id: job.route_id.clone(),
            path_id: job.path_id.clone(),
            route_ids: job.route_ids.clone(),
            tariff_electricity_total: job.tariff_electricity_total,
            reroute_count: job.reroute_count,
        }),
    }))
}
