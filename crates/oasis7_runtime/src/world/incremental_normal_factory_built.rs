//! Current reducer for one due `FactoryBuilt` system event.
//!
//! Selection DTOs are observations only. This reducer revalidates the
//! qualified pending-build identity, decodes the Current row again, applies
//! the original state effects through typed owners, and only then creates the
//! private publication receipt consumed by the counted event sink.

use super::{NormalTailWorkV1, due_economy::DecodedFactoryBuildJobV1};
use crate::world::{
    World,
    incremental_action_factory_lifecycle::{FactoryLifecycleSourcesV1, FactoryLifecycleWorkV1},
    incremental_body::{BodyMutationError, write_body_value},
    incremental_body_event::{materials::CorePolicyMaterialContextV1, read_fixed},
    incremental_step::IwsStepContext,
};
use oasis7_formal_protocol::{
    controlled_authority::formal::bounded_context::WorkIdentityV1,
    incremental_execution::{
        ExecutionError, ExecutionFence, ExecutionTransaction, PreparedGuard, RootSetV1,
    },
    incremental_state::{MerkleNodeReader, StateNamespaceV1, StatePageWriter},
    incremental_value::{ValueNodeReader, ValueNodeWriter, ValueReadContext, ValueTreeError},
};
use oasis7_proto::memory_budget::{MemoryBudget, MemoryClass, MemoryReservation, ReservedValue};
use oasis7_world_state::{
    FactoryBuildJobState, FactoryState,
    incremental_agent_body::{AGENT_BODY_VALUE_BYTES, AgentBodyKeyV1, AgentBodyValueV1},
    incremental_agent_subtree::{CurrentAgentPresenceV1, read_current_agent_presence_v1},
    incremental_factory_lifecycle_index::observe_current_factory_lifecycle_v1,
    incremental_factory_records::{
        FactoryBuildParentIdentityV1, FactoryCellReadV1, FactoryCellV1, FactoryFamilyReadContextV1,
        FactoryRecordChangeV1, FactoryRecordCreateV1, FactoryRecordKeyV1,
        FactoryRecordRefV1 as RecordRef, create_factory_record_overlay_v1,
        read_factory_cell_overlay_v1, replace_factory_record_overlay_v1,
        visit_qualified_build_parent_identities_v1,
    },
    incremental_field_member::MemberRecordWriteContext,
    incremental_field_overlay::{WorldFieldOverlayContextV1, WorldFieldOverlayErrorV1},
    incremental_field_read::WorldFieldReadContextV1,
    incremental_world_id_sets::{
        WorldIdSetChangeV1, WorldIdSetFieldV1, WorldIdSetKeyRefV1, change_world_id_set_overlay_v1,
        world_id_set_contains_overlay_v1,
    },
    incremental_world_native::{
        change_current_factory_industry_stage_v1, recover_current_factory_construction_receipt_v1,
        recover_current_factory_industry_progress_v1, recover_gameplay_policy_overlay_v1,
        write_current_factory_construction_receipt_v1,
    },
};
use std::{cell::RefCell, io::Write};

fn overlay_error(error: WorldFieldOverlayErrorV1) -> BodyMutationError {
    match error {
        WorldFieldOverlayErrorV1::Execution(error) => error.into(),
        WorldFieldOverlayErrorV1::Value(error) => error.into(),
        WorldFieldOverlayErrorV1::Member(error) => BodyMutationError::Original(Box::new(error)),
    }
}

fn invalid(reason: &'static str) -> BodyMutationError {
    BodyMutationError::Invalid(reason)
}

struct FactoryBuildOutput<'a>(&'a mut dyn Write);

impl Write for FactoryBuildOutput<'_> {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        self.0.write(bytes)
    }

    fn flush(&mut self) -> std::io::Result<()> {
        self.0.flush()
    }
}

/// A system-event publication owner, minted only after all matching Current
/// writes have completed in the exact transaction and source work pool.
pub(in crate::world) struct PreparedNormalFactoryBuiltV1 {
    event: ReservedValue<crate::WorldEvent>,
    guard: PreparedGuard,
    identity: WorkIdentityV1,
    now: u64,
    memory: MemoryBudget,
    _event_phase: MemoryReservation,
}

impl PreparedNormalFactoryBuiltV1 {
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
            || !transaction.uses_budget(memory)
            || !self.memory.same_pool(memory)
            || !self.event.uses_budget(memory)
            || now != self.now
            || self.event.time != now
            || self.event.caused_by.is_some()
            || !matches!(
                &self.event.body,
                crate::WorldEventBody::Domain(crate::DomainEvent::FactoryBuilt { .. })
            )
        {
            return Err(invalid("FactoryBuilt prepared owner binding"));
        }
        Ok(())
    }
}

/// Apply the old FactoryBuilt state effects against authenticated Current.
/// All supplied qualifications and DTOs are rechecked; the only output is a
/// private counted-publication owner. This does not install a World cache.
#[allow(clippy::too_many_arguments)]
pub(in crate::world) fn complete_current_factory_build_v1<
    M: MerkleNodeReader,
    P: MerkleNodeReader,
    V: ValueNodeReader,
    W: ValueNodeReader + ValueNodeWriter + StatePageWriter,
    PE: FnMut() -> Option<ValueTreeError>,
    PC: FnMut() -> bool,
    AE: FnMut() -> Option<ValueTreeError>,
    AC: FnMut() -> bool,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    ME: FnMut() -> Option<ValueTreeError>,
    D: FnMut() -> bool,
>(
    step: &mut IwsStepContext<'_, '_, '_, '_, M, V, W, D>,
    materials: &mut CorePolicyMaterialContextV1<'_, M, ME>,
    sources: &FactoryLifecycleSourcesV1<'_>,
    current_read: &mut WorldFieldReadContextV1<'_, P, V, E, C>,
    factory_parent_read: &mut FactoryFamilyReadContextV1<'_, P, V, PE, PC>,
    agent_parent_read: &mut WorldFieldReadContextV1<'_, P, V, AE, AC>,
    roots: &RootSetV1,
    identity: &ReservedValue<FactoryBuildParentIdentityV1>,
    selected: &ReservedValue<DecodedFactoryBuildJobV1>,
    normal_work: &mut NormalTailWorkV1<'_, '_>,
    work: &mut FactoryLifecycleWorkV1,
    guard: &PreparedGuard,
    live: &ExecutionFence,
) -> Result<PreparedNormalFactoryBuiltV1, BodyMutationError> {
    let result = (|| {
        let now = step.time();
        let transaction = &mut *step.transaction;
        transaction.validate_guard(guard, live)?;
        if !transaction.uses_budget(step.mutation.memory)
            || current_read.values.class != MemoryClass::Ordinary
            || factory_parent_read.values.class != MemoryClass::Ordinary
            || agent_parent_read.values.class != MemoryClass::Ordinary
            || !materials.parent.uses_budget(step.mutation.memory)
            || !sources.factories.uses_budget(step.mutation.memory)
            || !sources.agents.uses_budget(step.mutation.memory)
            || sources.factories.allocation_class() != MemoryClass::Ordinary
            || sources.agents.allocation_class() != MemoryClass::Ordinary
            || !identity.uses_budget(step.mutation.memory)
            || !selected.uses_budget(step.mutation.memory)
            || !current_read.values.memory.same_pool(step.mutation.memory)
            || !factory_parent_read
                .values
                .memory
                .same_pool(step.mutation.memory)
            || !agent_parent_read
                .values
                .memory
                .same_pool(step.mutation.memory)
            || !normal_work.memory().same_pool(step.mutation.memory)
        {
            return Err(invalid("FactoryBuilt transaction pool/class"));
        }
        normal_work.charge(4096)?;
        roots
            .validate()
            .map_err(|_| invalid("FactoryBuilt roots"))?;
        let digest = roots
            .digest()
            .map_err(|_| invalid("FactoryBuilt root digest"))?;
        if digest != transaction.parent_fence().parent_roots {
            return Err(invalid("FactoryBuilt parent roots"));
        }
        sources
            .factories
            .validate_binding(&digest)
            .map_err(BodyMutationError::Value)?;
        if identity.id != selected.job.job_id || selected.job.ready_at > now {
            return Err(invalid("FactoryBuilt selected job binding"));
        }

        // Legacy preparation normalizes the two material representations
        // before applying FactoryBuilt, including the exact retry path.
        normal_work.charge(32 << 10)?;
        materials.contribute(transaction, step.values, step.mutation, guard, live)?;

        // Authenticate both roots of the parent-selected job identity.
        let mut parent_match = false;
        let mut parent_mismatch = false;
        visit_qualified_build_parent_identities_v1(
            roots,
            sources.factories,
            factory_parent_read,
            &mut work.factory,
            |candidate| {
                if candidate.id == identity.id {
                    parent_match = candidate.value_root == identity.value_root
                        && candidate.record_root == identity.record_root;
                    parent_mismatch = !parent_match;
                }
                Ok(())
            },
        )
        .map_err(BodyMutationError::Value)?;
        if parent_mismatch {
            return Err(invalid("FactoryBuilt parent identity changed"));
        }
        if !parent_match {
            return Err(invalid("FactoryBuilt parent identity absent"));
        }

        let max_scalar_bytes = work.max_scalar_bytes;
        let max_items = work.pending_entries_remaining;
        let event_sequence =
            step.read_allocator(super::super::incremental_allocators::RuntimeAllocatorV1::Event)?;
        let mut decoded = None;
        let event_phase;
        let event_bytes;
        {
            let stop = RefCell::new(&mut current_read.cancelled);
            let mutation_cancel = RefCell::new(&mut step.mutation.cancelled);
            let mut overlay = WorldFieldOverlayContextV1 {
                transaction: &mut *step.transaction,
                values: ValueReadContext {
                    source: current_read.values.source,
                    memory: step.mutation.memory,
                    class: MemoryClass::Ordinary,
                    io: &mut *current_read.values.io,
                },
                key_work: &mut *current_read.key_work,
                semantic_bytes: current_read.semantic_bytes,
                record_bytes: current_read.record_bytes,
                take_state_error: &mut current_read.take_state_error,
                cancelled: || (stop.borrow_mut())() || (mutation_cancel.borrow_mut())(),
            };
            let mut writer = MemberRecordWriteContext {
                sink: &mut *step.mutation.sink,
                memory: step.mutation.memory,
                io: &mut *step.mutation.io,
                key_work: &mut *materials.write_keys,
                limits: materials.write_limits,
                cancelled: || (stop.borrow_mut())() || (mutation_cancel.borrow_mut())(),
            };
            decoded = Some(super::due_economy::decode_current_factory_build_job_v1(
                identity.id,
                max_scalar_bytes,
                max_items,
                step.mutation.memory,
                normal_work,
                &mut |cell, output| {
                    let mut output = FactoryBuildOutput(output);
                    read_factory_cell_overlay_v1(
                        FactoryRecordKeyV1::Build(identity.id),
                        cell,
                        &mut overlay,
                        max_scalar_bytes as u64,
                        &mut output,
                    )
                    .map_err(overlay_error)
                },
            )?);
            if decoded.as_ref().unwrap().job != selected.job {
                return Err(invalid("FactoryBuilt Current differs from selected"));
            }
            let job = &decoded.as_ref().unwrap().job;

            // Admit all retained clones before constructing FactoryState or
            // the event. The record writer separately accounts for its own
            // typed tree and output buffers.
            event_bytes = event_heap_bound(job)?;
            normal_work.charge(event_bytes)?;
            let phase_bytes = event_bytes
                .checked_mul(2)
                .and_then(|n| n.checked_add(std::mem::size_of::<FactoryState>()))
                .and_then(|n| n.checked_add(std::mem::size_of::<FactoryBuildJobState>()))
                .ok_or(ValueTreeError::ResourceLimited(
                    "FactoryBuilt retained owner bound",
                ))?;
            event_phase = step
                .mutation
                .memory
                .reserve(phase_bytes, MemoryClass::Ordinary)
                .map_err(ValueTreeError::Memory)?;

            let settled = world_id_set_contains_overlay_v1(
                WorldIdSetFieldV1::SettledFactoryBuilds,
                WorldIdSetKeyRefV1::Action(job.job_id),
                &mut overlay,
            )
            .map_err(overlay_error)?;
            let retired = world_id_set_contains_overlay_v1(
                WorldIdSetFieldV1::RetiredFactories,
                WorldIdSetKeyRefV1::Text(&job.spec.factory_id),
                &mut overlay,
            )
            .map_err(overlay_error)?;

            // The due builder only consumes a still-pending, unretired job.
            // Replays belong to the durable event receipt path, not this FIFO.
            if settled {
                return Err(invalid("FactoryBuilt job already settled"));
            }
            if retired {
                return Err(invalid("FactoryBuilt factory identity retired"));
            }
            match read_factory_cell_overlay_v1(
                FactoryRecordKeyV1::Factory(&job.spec.factory_id),
                FactoryCellV1::Record,
                &mut overlay,
                max_scalar_bytes as u64,
                &mut std::io::sink(),
            )
            .map_err(overlay_error)?
            {
                FactoryCellReadV1::RecordAbsent => {}
                _ => return Err(invalid("FactoryBuilt would replace active factory")),
            }

            let job = &decoded.as_ref().unwrap().job;
            let site_ledger = oasis7_world_state::MaterialLedgerId::Site(job.site_id.clone());
            let factory = FactoryState {
                factory_id: job.spec.factory_id.clone(),
                site_id: job.site_id.clone(),
                builder_agent_id: job.builder_agent_id.clone(),
                spec: job.spec.clone(),
                input_ledger: site_ledger.clone(),
                output_ledger: site_ledger,
                durability_ppm: 1_000_000,
                production: oasis7_world_state::FactoryProductionState::default(),
                site_authority_revision: job.site_authority_revision,
                site_location_id: job.site_location_id.clone(),
                location_anchor_revision: job.location_anchor_revision,
                construction_power_profile_key: job
                    .construction_power_obligation
                    .as_ref()
                    .map(|value| value.profile_key.clone()),
                construction_power_profile_revision: job
                    .construction_power_obligation
                    .as_ref()
                    .map(|value| value.profile_revision),
                built_at: now,
            };
            let create_guard = overlay.transaction.guard();
            create_factory_record_overlay_v1(
                FactoryRecordCreateV1 {
                    key: FactoryRecordKeyV1::Factory(&job.spec.factory_id),
                    after: RecordRef::Factory(&factory),
                    qualification: sources.factories,
                },
                &mut overlay,
                &mut writer,
                &mut work.factory,
                &create_guard,
                live,
            )
            .map_err(overlay_error)?;
            let remove_guard = overlay.transaction.guard();
            replace_factory_record_overlay_v1(
                FactoryRecordChangeV1 {
                    key: FactoryRecordKeyV1::Build(job.job_id),
                    expected: Some(RecordRef::Build(job)),
                    after: None,
                    max_current_payload_bytes: max_scalar_bytes as u64,
                },
                &mut overlay,
                &mut writer,
                &mut work.factory,
                &remove_guard,
                live,
            )
            .map_err(overlay_error)?;
            let settled_guard = overlay.transaction.guard();
            change_world_id_set_overlay_v1(
                WorldIdSetChangeV1 {
                    field: WorldIdSetFieldV1::SettledFactoryBuilds,
                    key: WorldIdSetKeyRefV1::Action(job.job_id),
                    expected: false,
                    after: true,
                },
                &mut overlay,
                &mut writer,
                &settled_guard,
                live,
            )
            .map_err(overlay_error)?;
        }
        let decoded = decoded.ok_or(invalid("FactoryBuilt decoded Current absent"))?;
        let job = &decoded.job;

        // The original reducer updates last_active only when the builder's
        // actual Current agent exists; a deleted builder does not block a
        // previously committed construction.
        {
            let stop = RefCell::new(&mut current_read.cancelled);
            let mutation_cancel = RefCell::new(&mut step.mutation.cancelled);
            let mut overlay = WorldFieldOverlayContextV1 {
                transaction: &mut *step.transaction,
                values: ValueReadContext {
                    source: current_read.values.source,
                    memory: step.mutation.memory,
                    class: MemoryClass::Ordinary,
                    io: &mut *current_read.values.io,
                },
                key_work: &mut *current_read.key_work,
                semantic_bytes: current_read.semantic_bytes,
                record_bytes: current_read.record_bytes,
                take_state_error: &mut current_read.take_state_error,
                cancelled: || (stop.borrow_mut())() || (mutation_cancel.borrow_mut())(),
            };
            let presence_guard = overlay.transaction.guard();
            let presence = read_current_agent_presence_v1(
                &job.builder_agent_id,
                sources.agents,
                roots,
                &mut overlay,
                agent_parent_read,
                &mut work.pending_entries_remaining,
                &presence_guard,
                live,
            )
            .map_err(overlay_error)?;
            drop(overlay);
            if presence == CurrentAgentPresenceV1::Existing {
                normal_work.charge(
                    job.builder_agent_id
                        .len()
                        .checked_add(AGENT_BODY_VALUE_BYTES * 2)
                        .and_then(|n| n.checked_add(std::mem::size_of::<AgentBodyValueV1>()))
                        .ok_or(ValueTreeError::ResourceLimited(
                            "FactoryBuilt builder activity work",
                        ))?,
                )?;
                let key = AgentBodyKeyV1::new(&job.builder_agent_id)
                    .map_err(BodyMutationError::Invalid)?;
                let body = AgentBodyValueV1::from_canonical_bytes(&read_fixed::<
                    AGENT_BODY_VALUE_BYTES,
                    _,
                    _,
                    _,
                    _,
                >(
                    step.transaction,
                    StateNamespaceV1::Business,
                    key.as_bytes(),
                    step.values,
                    step.mutation,
                )?)
                .map_err(BodyMutationError::Invalid)?;
                if body.last_active != now {
                    let tail = step.transaction.guard();
                    step.transaction.validate_guard(&tail, live)?;
                    let after = AgentBodyValueV1 {
                        body_view: body.body_view,
                        last_active: now,
                    };
                    let reference = write_body_value(&after, step.mutation)?;
                    step.transaction.write(
                        StateNamespaceV1::Business,
                        key.as_bytes(),
                        Some(reference),
                    )?;
                }
            }
        }

        // Preserve the optional obligation receipt in its original native
        // authorization field. Existing values are replaced only through the
        // same exact-key Current owner and writer codec.
        if let Some(obligation) = job.construction_power_obligation.as_ref() {
            normal_work.charge(
                job.spec
                    .factory_id
                    .len()
                    .checked_add(std::mem::size_of_val(obligation))
                    .ok_or(ValueTreeError::ResourceLimited(
                        "FactoryBuilt construction receipt work",
                    ))?,
            )?;
            let mut receipt_read = WorldFieldReadContextV1 {
                root: roots.state_root(StateNamespaceV1::Authorization),
                state: current_read.state,
                values: oasis7_formal_protocol::incremental_value::ValueReadContext {
                    source: current_read.values.source,
                    memory: current_read.values.memory,
                    class: current_read.values.class,
                    io: &mut *current_read.values.io,
                },
                key_work: &mut *current_read.key_work,
                semantic_bytes: current_read.semantic_bytes,
                record_bytes: current_read.record_bytes,
                take_state_error: &mut current_read.take_state_error,
                cancelled: &mut current_read.cancelled,
            };
            let receipt_guard = step.transaction.guard();
            let expected = recover_current_factory_construction_receipt_v1(
                &job.spec.factory_id,
                sources.native,
                roots,
                step.transaction,
                &mut receipt_read,
                &mut work.native_scan,
                &mut work.native_read,
                &receipt_guard,
                live,
            )
            .map_err(overlay_error)?;
            let mut writer = MemberRecordWriteContext {
                sink: &mut *step.mutation.sink,
                memory: step.mutation.memory,
                io: &mut *step.mutation.io,
                key_work: &mut *materials.write_keys,
                limits: materials.write_limits,
                cancelled: || (step.mutation.cancelled)(),
            };
            write_current_factory_construction_receipt_v1(
                &expected,
                &job.spec.factory_id,
                obligation,
                step.transaction,
                &mut receipt_read,
                &mut writer,
                &mut work.records_remaining,
                &mut work.native_write,
                live,
            )
            .map_err(overlay_error)?;
        }

        // Reproduce refresh_progress_stage after the factory is installed.
        // The lifecycle observer excludes this exact factory id, preserving
        // the old pre-insertion factory count while accounting for the actual
        // Current pending/factory overlays.
        normal_work.charge(
            job.spec
                .factory_id
                .len()
                .checked_add(32 << 10)
                .and_then(|n| n.checked_add(std::mem::size_of::<FactoryLifecycleWorkV1>()))
                .ok_or(ValueTreeError::ResourceLimited(
                    "FactoryBuilt industry progress work",
                ))?,
        )?;
        let mut progress_read = WorldFieldReadContextV1 {
            root: roots.state_root(StateNamespaceV1::Authorization),
            state: current_read.state,
            values: oasis7_formal_protocol::incremental_value::ValueReadContext {
                source: current_read.values.source,
                memory: current_read.values.memory,
                class: current_read.values.class,
                io: &mut *current_read.values.io,
            },
            key_work: &mut *current_read.key_work,
            semantic_bytes: current_read.semantic_bytes,
            record_bytes: current_read.record_bytes,
            take_state_error: &mut current_read.take_state_error,
            cancelled: &mut current_read.cancelled,
        };
        let current_read = &mut progress_read;
        let policy = recover_gameplay_policy_overlay_v1(
            step.transaction,
            live,
            current_read,
            &mut work.native_scan,
            &mut work.native_read,
        )
        .map_err(overlay_error)?;
        let taxes_enabled = policy.value().value().electricity_tax_bps > 0
            || policy.value().value().data_tax_bps > 0;
        let before_progress_guard = step.transaction.guard();
        let before_progress = recover_current_factory_industry_progress_v1(
            sources.native,
            roots,
            step.transaction,
            current_read,
            &mut work.native_scan,
            &mut work.native_read,
            &before_progress_guard,
            live,
        )
        .map_err(overlay_error)?;
        let completed_transits = before_progress
            .value()
            .ok_or(invalid("FactoryBuilt industry progress absent"))?
            .completed_material_transits;
        let observation_guard = step.transaction.guard();
        let observation = observe_current_factory_lifecycle_v1(
            &job.spec.factory_id,
            sources.index,
            sources.factories,
            sources.industry,
            roots,
            step.transaction,
            current_read,
            &mut work.factory,
            &mut work.industry,
            &mut work.lifecycle,
            work.max_scalar_bytes as u64,
            &observation_guard,
            live,
        )
        .map_err(overlay_error)?;
        observation
            .validate(step.transaction, step.mutation.memory, live)
            .map_err(overlay_error)?;
        let stage = oasis7_world_state::state::industry_transition::industry_stage_for_progress(
            observation.stable_without_factory(),
            observation.stage_completed_jobs_without_factory(),
            completed_transits,
            taxes_enabled,
        );
        let latest_progress_guard = step.transaction.guard();
        let latest_progress = recover_current_factory_industry_progress_v1(
            sources.native,
            roots,
            step.transaction,
            current_read,
            &mut work.native_scan,
            &mut work.native_read,
            &latest_progress_guard,
            live,
        )
        .map_err(overlay_error)?;
        observation
            .validate(step.transaction, step.mutation.memory, live)
            .map_err(overlay_error)?;
        if latest_progress
            .value()
            .ok_or(invalid("FactoryBuilt industry progress absent"))?
            .completed_material_transits
            != completed_transits
        {
            return Err(invalid("FactoryBuilt industry progress changed"));
        }
        let mut writer = MemberRecordWriteContext {
            sink: &mut *step.mutation.sink,
            memory: step.mutation.memory,
            io: &mut *step.mutation.io,
            key_work: &mut *materials.write_keys,
            limits: materials.write_limits,
            cancelled: || (step.mutation.cancelled)(),
        };
        change_current_factory_industry_stage_v1(
            &latest_progress,
            stage,
            now,
            step.transaction,
            current_read,
            &mut writer,
            &mut work.native_scan,
            &mut work.native_read,
            &mut work.native_write,
            live,
        )
        .map_err(overlay_error)?;

        // The event remains private until the shared counted sink validates
        // and consumes it. Retain both the event bytes and simultaneous
        // factory/receipt work before cloning owned fields.
        let (event_id, _, _) =
            World::preview_next_event_id(event_sequence.next, event_sequence.era);
        let event = crate::WorldEvent {
            id: event_id,
            time: now,
            caused_by: None,
            body: crate::WorldEventBody::Domain(crate::DomainEvent::FactoryBuilt {
                job_id: job.job_id,
                builder_agent_id: job.builder_agent_id.clone(),
                site_id: job.site_id.clone(),
                spec: job.spec.clone(),
            }),
        };
        let event_fee = step
            .mutation
            .memory
            .reserve(event_bytes, MemoryClass::Ordinary)
            .map_err(ValueTreeError::Memory)?;
        let event = event_fee.retain(event);
        let final_guard = step.transaction.guard();
        step.transaction.validate_guard(&final_guard, live)?;
        Ok(PreparedNormalFactoryBuiltV1 {
            event,
            guard: final_guard,
            identity: normal_work.identity(),
            now,
            memory: step.mutation.memory.clone(),
            _event_phase: event_phase,
        })
    })();
    if result.is_err() {
        let _ = step
            .transaction
            .contribute(|_| Err::<(), _>(ExecutionError::Aborted));
    }
    result
}

fn event_heap_bound(job: &FactoryBuildJobState) -> Result<usize, BodyMutationError> {
    let mut bytes = std::mem::size_of::<crate::WorldEvent>()
        .checked_add(std::mem::size_of::<crate::DomainEvent>())
        .and_then(|n| n.checked_add(std::mem::size_of::<oasis7_wasm_abi::FactoryModuleSpec>()))
        .and_then(|n| n.checked_add(job.builder_agent_id.len()))
        .and_then(|n| n.checked_add(job.site_id.len()))
        .and_then(|n| n.checked_add(job.spec.factory_id.len()))
        .and_then(|n| n.checked_add(job.spec.display_name.len()))
        .and_then(|n| {
            n.checked_add(job.site_id.len().checked_mul(2)?)
                .and_then(|n| n.checked_add(job.site_location_id.as_ref().map_or(0, String::len)))
                .and_then(|n| {
                    n.checked_add(
                        job.construction_power_obligation
                            .as_ref()
                            .map_or(0, |obligation| obligation.profile_key.len()),
                    )
                })
        })
        .and_then(|n| {
            n.checked_add(
                job.spec
                    .tags
                    .len()
                    .checked_mul(std::mem::size_of::<String>())?,
            )
        })
        .and_then(|n| {
            n.checked_add(
                job.spec
                    .build_cost
                    .len()
                    .checked_mul(std::mem::size_of::<oasis7_wasm_abi::MaterialStack>())?,
            )
        })
        .ok_or(ValueTreeError::ResourceLimited(
            "FactoryBuilt event heap bound",
        ))?;
    for tag in &job.spec.tags {
        bytes = bytes
            .checked_add(tag.len())
            .ok_or(ValueTreeError::ResourceLimited(
                "FactoryBuilt tag heap bound",
            ))?;
    }
    for cost in &job.spec.build_cost {
        bytes = bytes
            .checked_add(cost.kind.len())
            .ok_or(ValueTreeError::ResourceLimited(
                "FactoryBuilt cost heap bound",
            ))?;
    }
    Ok(bytes)
}
