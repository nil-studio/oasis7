//! Test-only composition of the real empty-directory Rule phase; no DTO ctor.
use super::*;
pub(in crate::world) fn directory_work() -> CurrentInvocationEnumerationWorkV1 {
    CurrentInvocationEnumerationWorkV1 {
        instance_scan: genesis::scan(),
        registry_scan: genesis::scan(),
        pending_entries: 100000,
        comparison_bytes_remaining: 4_000_000_000,
        max_keys: 256,
        max_key_bytes: 1024,
        max_text_bytes: 1024,
    }
}
pub(in crate::world) struct CoreExecutionFixtureV1<'p> {
    pub limits: MaterialFixtureLimitsV1,
    pub persist: &'p mut dyn FnMut(
        &mut dyn crate::world::incremental_body_event::AgentEventOriginalV1,
    ) -> Result<(), BodyMutationError>,
}
#[allow(clippy::too_many_arguments)]
pub(in crate::world) fn outcome<'a>(
    fixture: &genesis::Fixture<'a>,
    reader: &Reader<'a>,
    transaction: &mut ExecutionTransaction<'_, Reader<'a>>,
    selected: &crate::world::incremental_action_execution::SelectedPendingActionV1,
    directory: &oasis7_proto::memory_budget::ReservedValue<AcceptedInvocationDirectoryV1>,
    counter: &WorldEventCounterV1,
    originals: &mut Originals<'a>,
    memory: &'a MemoryBudget,
    now: u64,
    live: &ExecutionFence,
) -> oasis7_proto::memory_budget::ReservedValue<RootRulePhaseOutcomeV1> {
    evaluate_with_core(
        fixture,
        reader,
        transaction,
        selected,
        directory,
        counter,
        originals,
        memory,
        now,
        live,
        None,
    )
    .0
}
pub(in crate::world) enum NonmovementFixtureWorkV1<'p> {
    LogisticsRegistration{
        work:&'p mut crate::world::incremental_normal_action_logistics::LogisticsRegistrationWorkV1,
        common:&'p oasis7_formal_protocol::controlled_authority::formal::bounded_context::FormalBoundedWorkV1,
        result:&'p mut Option<crate::world::incremental_normal_action_logistics::CompletedLogisticsRegistrationV1>,
    },

    NormalExtension {
        work: crate::world::incremental_module_invocation::NormalCoreExtensionWorkViewV1<'p>,
        result: &'p mut Option<crate::world::incremental_module_invocation::CompletedNormalExtensionV1>,
    },
    Crisis(&'p mut crate::world::incremental_action_crisis::CrisisWorkV1),
    ContractAccept(&'p mut crate::world::incremental_action_contract_accept::ContractAcceptWorkV1),
    MetaProgress(&'p mut crate::world::incremental_action_meta_progress::MetaProgressWorkV1),
    FactoryLifecycle(
        &'p mut crate::world::incremental_action_factory_lifecycle::FactoryLifecycleWorkV1,
    ),
    RouteAvailability(
        &'p mut crate::world::incremental_action_route_availability::RouteAvailabilityWorkV1,
    ),
    MaterialTransfer(
        &'p mut crate::world::incremental_material_normalization::transfer::MaterialTransferWorkV1,
    ),
    Body(&'p mut BodyActionWorkV1),
    DataAccess(&'p mut crate::world::incremental_action_data::data_access::DataAccessWorkV1),
    CollectData(&'p mut crate::world::incremental_action_data::CollectDataWorkV1),
    AuthenticatedCollectData(
        &'p mut crate::world::incremental_action_data::CollectDataWorkV1,
        &'p mut crate::world::incremental_action_data::auth_current::CollectDataAuthorizationWorkV1,
    ),
    TransferResource(
        &'p mut crate::world::incremental_action_data::transfer_resource::TransferResourceWorkV1,
    ),
}
pub(in crate::world) struct NonmovementExecutionFixtureV1<'p> {
    pub limits: MaterialFixtureLimitsV1,
    pub persist: &'p mut dyn FnMut(
        &mut dyn crate::world::incremental_body_event::AgentEventOriginalV1,
    ) -> Result<(), BodyMutationError>,
    pub work: NonmovementFixtureWorkV1<'p>,
    pub audit: &'p mut oasis7_world_state::incremental_world_event_decode::WorldEventDecodeWorkV1,
}
pub(in crate::world) type NonmovementFixtureErrorV1 =
    crate::world::incremental_module_invocation::NonmovementSelectedErrorV1<ValueTreeError>;
enum FixtureInputV1<'a> {
    PendingFallible(
        &'a mut oasis7_proto::memory_budget::ReservedValue<
            oasis7_world_state::incremental_pending_action_overlay::PendingActionCurrentV1,
        >,
        &'a mut oasis7_world_state::incremental_pending_action_family::PendingActionFamilyWorkV1,
        &'a mut Option<NonmovementFixtureErrorV1>,
        Option<&'a AtomicBool>,
    ),
    EmptyCancelled(
        &'a mut oasis7_proto::memory_budget::ReservedValue<
            oasis7_world_state::incremental_pending_action_overlay::PendingActionCurrentV1,
        >,
        &'a mut oasis7_world_state::incremental_pending_action_family::PendingActionFamilyWorkV1,
    ),
    Selected(&'a crate::world::incremental_action_execution::SelectedPendingActionV1),
    Pending(
        &'a mut oasis7_proto::memory_budget::ReservedValue<
            oasis7_world_state::incremental_pending_action_overlay::PendingActionCurrentV1,
        >,
        &'a mut oasis7_world_state::incremental_pending_action_family::PendingActionFamilyWorkV1,
    ),
}
#[allow(clippy::too_many_arguments)]
pub(in crate::world) fn evaluate_with_core<'a>(
    fixture: &genesis::Fixture<'a>,
    reader: &Reader<'a>,
    transaction: &mut ExecutionTransaction<'_, Reader<'a>>,
    selected: &crate::world::incremental_action_execution::SelectedPendingActionV1,
    directory: &oasis7_proto::memory_budget::ReservedValue<AcceptedInvocationDirectoryV1>,
    counter: &WorldEventCounterV1,
    originals: &mut Originals<'a>,
    memory: &'a MemoryBudget,
    now: u64,
    live: &ExecutionFence,
    core: Option<CoreExecutionFixtureV1<'_>>,
) -> (
    oasis7_proto::memory_budget::ReservedValue<RootRulePhaseOutcomeV1>,
    Option<crate::world::incremental_action_execution_core::CompletedCoreActionV1>,
) {
    let (outcome, completed, _, _, _, _, _, _, _, _, _, _) = evaluate(
        fixture,
        reader,
        transaction,
        FixtureInputV1::Selected(selected),
        directory,
        counter,
        originals,
        memory,
        now,
        live,
        core,
        None,
    );
    (outcome.expect("actual Rule outcome"), completed)
}
#[path = "incremental_action_body_tests_rules_consumers.rs"]
mod consumers;
pub(in crate::world) use consumers::*;
#[path = "incremental_action_body_tests_rules_factory_lifecycle.rs"]
mod factory_lifecycle;
pub(in crate::world) use factory_lifecycle::*;
#[allow(clippy::too_many_arguments)]
pub(in crate::world) fn cancelled_empty_transfer_probe<'a>(
    fixture: &genesis::Fixture<'a>,
    reader: &Reader<'a>,
    transaction: &mut ExecutionTransaction<'_, Reader<'a>>,
    pending: &mut oasis7_proto::memory_budget::ReservedValue<
        oasis7_world_state::incremental_pending_action_overlay::PendingActionCurrentV1,
    >,
    pending_work: &mut oasis7_world_state::incremental_pending_action_family::PendingActionFamilyWorkV1,
    directory: &oasis7_proto::memory_budget::ReservedValue<AcceptedInvocationDirectoryV1>,
    counter: &WorldEventCounterV1,
    originals: &mut Originals<'a>,
    memory: &'a MemoryBudget,
    live: &ExecutionFence,
    body: NonmovementExecutionFixtureV1<'_>,
) {
    let result = evaluate(
        fixture,
        reader,
        transaction,
        FixtureInputV1::EmptyCancelled(pending, pending_work),
        directory,
        counter,
        originals,
        memory,
        0,
        live,
        None,
        Some(body),
    );
    assert!(result.0.is_none());
}
#[allow(clippy::too_many_arguments)]
fn evaluate<'a>(
    fixture: &genesis::Fixture<'a>,
    reader: &Reader<'a>,
    transaction: &mut ExecutionTransaction<'_, Reader<'a>>,
    input: FixtureInputV1<'_>,
    directory: &oasis7_proto::memory_budget::ReservedValue<AcceptedInvocationDirectoryV1>,
    counter: &WorldEventCounterV1,
    originals: &mut Originals<'a>,
    memory: &'a MemoryBudget,
    now: u64,
    live: &ExecutionFence,
    mut core: Option<CoreExecutionFixtureV1<'_>>,
    body: Option<NonmovementExecutionFixtureV1<'_>>,
) -> (
    Option<oasis7_proto::memory_budget::ReservedValue<RootRulePhaseOutcomeV1>>,
    Option<crate::world::incremental_action_execution_core::CompletedCoreActionV1>,
    Option<crate::world::incremental_action_body::CompletedBodyActionV1>,
    Option<crate::world::incremental_action_data::CompletedCollectDataV1>,
    Option<crate::world::incremental_action_data::transfer_resource::CompletedTransferResourceV1>,
    Option<crate::world::incremental_action_data::data_access::CompletedDataAccessV1>,
    Option<crate::world::incremental_material_normalization::transfer::CompletedMaterialTransferV1>,
    Option<crate::world::incremental_action_route_availability::CompletedRouteAvailabilityV1>,
    Option<crate::world::incremental_action_factory_lifecycle::CompletedFactoryLifecycleV1>,
    Option<crate::world::incremental_action_meta_progress::CompletedMetaProgressV1>,
    Option<crate::world::incremental_action_crisis::CompletedCrisisV1>,
    Option<crate::world::incremental_action_contract_accept::CompletedContractAcceptV1>,
) {
    let selected = match &input {
        FixtureInputV1::Selected(selected) => Some(*selected),
        FixtureInputV1::Pending(..)
        | FixtureInputV1::PendingFallible(..)
        | FixtureInputV1::EmptyCancelled(..) => None,
    };
    let empty_cancelled = matches!(&input, FixtureInputV1::EmptyCancelled(..));
    let core_limits = core.as_ref().map(|request| request.limits);
    let (body_limits, mut body_work, mut body_audit, mut body_persist) = match body {
        Some(body) => (
            Some(body.limits),
            Some(body.work),
            Some(body.audit),
            Some(body.persist),
        ),
        None => (None, None, None, None),
    };
    let default_stop = AtomicBool::new(false);
    let stop = &default_stop;
    let mut io = genesis::io();
    let mut keys = genesis::keys();
    let mut overlay = WorldFieldOverlayContextV1 {
        transaction,
        values: ValueReadContext {
            source: reader,
            memory,
            class: MemoryClass::Ordinary,
            io: &mut io,
        },
        key_work: &mut keys,
        semantic_bytes: 4096,
        record_bytes: 16 << 20,
        take_state_error: || reader.cas.error(),
        cancelled: stopped(&stop),
    };
    let mut member_cas = fixture.cas.clone();
    let mut fee_cas = fixture.cas.clone();
    let mut event_cas = fixture.cas.clone();
    let mut member_io = genesis::io();
    let mut member_keys = genesis::keys();
    let mut members = genesis::member(
        &mut member_cas,
        memory,
        &mut member_io,
        &mut member_keys,
        stopped(&stop),
    );
    let mut fee_io = genesis::io();
    let mut fee_keys = genesis::keys();
    let mut fees = RootModuleFeeWriteContextV1 {
        values: ValueMutationContextV1 {
            sink: &mut fee_cas,
            memory,
            io: &mut fee_io,
            cancelled: stopped(&stop),
        },
        key_work: &mut fee_keys,
        member_limits: MemberRecordLimitsV1 {
            semantic_bytes: 4096,
            payload_bytes: 8 << 20,
            record_bytes: 9 << 20,
        },
    };
    let mut event_io = genesis::io();
    let mut events = ModuleOutputEventContextV1 {
        values: reader,
        world_events: counter,
        mutation: BodyMutationContext {
            sink: &mut event_cas,
            memory,
            io: &mut event_io,
            cancelled: stopped(&stop),
        },
        persist_original:
            |producer: &mut dyn crate::world::incremental_body_event::AgentEventOriginalV1| {
                if let Some(request) = core.as_mut() {
                    return (request.persist)(producer);
                }
                if let Some(persist) = body_persist.as_mut() {
                    return persist(producer);
                }
                producer.persist_to(&mut |stream:&mut dyn crate::world::incremental_body_event::AgentEventOriginalStreamV1,cancelled:&mut dyn FnMut()->bool|{
        let n=usize::try_from(stream.original_bytes()).unwrap();assert!(n<=64<<10);
        let _fee=memory.reserve(n+std::mem::size_of::<Vec<u8>>(),MemoryClass::Ordinary).map_err(ExecutionError::Memory)?;
        let mut bytes=Vec::with_capacity(n);stream.write_to(&mut bytes)?;assert!(!cancelled());assert_eq!(fixture.objects.put(&bytes),stream.original_id());Ok(())
    })
            },
        max_event_bytes: 64 << 10,
        original_work: genesis::canonical(),
    };
    let image = fixture_worker::Image::new(memory);
    let allocator = RootModuleWorkerAllocatorV1::new(memory);
    let mut worker = BoundedWorkerSandboxV1 {
        executable: WorkerExecutableV1::SealedFd(&image.file),
        allocator: &allocator,
        cancelled: &stop,
        limits: WorkerLimitsV1 {
            address_space_bytes: 256 << 20,
            max_request_bytes: 1 << 20,
            max_output_bytes: 16 << 10,
            wall_time_ms: 30000,
        },
    };
    let mut work = execution_work();
    let mut hash = genesis::hash_work();
    let mut rule = RootRulePhaseWorkV1 {
        enumeration: directory_work(),
        payload:
            oasis7_world_state::incremental_rule_decision_decode::RuleDecisionPayloadDecodeWorkV1 {
                structure: oasis7_proto::cbor_decode_preflight::CborPreflightWorkV1 {
                    bytes_remaining: 1 << 20,
                    values_remaining: 100000,
                    max_depth: 64,
                },
                max_payload_bytes: 64 << 10,
                max_heap_bytes: 128 << 20,
            },
        canonical: genesis::canonical(),
        max_captured_outputs: 8,
        max_payload_bytes: 64 << 10,
        merge_units_remaining: 1_000_000_000,
    };
    let mut target = RootModuleExecutionContextV1 {
        families: RootModuleExecutionFamiliesV1 {
            roots: &fixture.roots,
            parent: &fixture.runtime,
            head: &fixture.head,
            instances: fixture.current.raw().instances(),
            config: fixture.current.runtime().config(),
            content: fixture.current.runtime().content(),
            agents: fixture.current.raw().agents(),
            authorization: fixture.current.runtime().module_authorization(),
            effects: fixture.current.runtime().effects(),
            state: fixture.current.raw().module_states(),
            agent_authority: fixture.current.native().agent_authority(),
            marketplace: fixture.current.native().market(),
        },
        now,
        state: reader,
        ctx: &mut overlay,
        members: &mut members,
        fees: &mut fees,
        events: &mut events,
        originals,
        worker: &mut worker,
        manifest_cache: &fixture.manifest_hash,
        manifest_work: &mut hash,
        work: &mut work,
        input_observation: None,
    };
    let (
        result,
        completed,
        body_completed,
        data_completed,
        transfer_completed,
        access_completed,
        material_completed,
        route_completed,
        factory_completed,
        meta_completed,
        crisis_completed,
        contract_accept_completed,
    ) = if let Some(limits) = core_limits {
        let mut backing = MaterialFixtureV1::build(
            &fixture.roots,
            reader,
            reader,
            memory,
            limits,
            || reader.cas.error(),
            || false,
        );
        let mut materials = backing.context(&fixture.roots, reader, || reader.cas.error());
        let mut read_io = genesis::io();
        let mut read_keys = genesis::keys();
        let mut read = oasis7_world_state::incremental_field_read::WorldFieldReadContextV1 {
            root: fixture.roots.state_root(StateNamespaceV1::Business),
            state: reader,
            values: ValueReadContext {
                source: reader,
                memory,
                class: MemoryClass::Ordinary,
                io: &mut read_io,
            },
            key_work: &mut read_keys,
            semantic_bytes: 4096,
            record_bytes: 16 << 20,
            take_state_error: || reader.cas.error(),
            cancelled: || false,
        };
        let mut admission_io = genesis::io();
        let mut admission_keys = genesis::keys();
        let mut admission_read =
            oasis7_world_state::incremental_field_read::WorldFieldReadContextV1 {
                root: fixture.roots.state_root(StateNamespaceV1::Authorization),
                state: reader,
                values: ValueReadContext {
                    source: reader,
                    memory,
                    class: MemoryClass::Ordinary,
                    io: &mut admission_io,
                },
                key_work: &mut admission_keys,
                semantic_bytes: 4096,
                record_bytes: 16 << 20,
                take_state_error: || reader.cas.error(),
                cancelled: || false,
            };
        let mut scan = genesis::scan();
        let mut policy = genesis::current_work(&fixture.roots).runtime.revocation;
        let guard = target.ctx.transaction.guard();
        let current = oasis7_world_state::incremental_runtime_revocation::begin_runtime_revocation_current_v1(
            fixture.current.runtime().revocation(), target.ctx, &mut admission_read,
            &mut scan, &mut policy, target.members, &guard, live).unwrap();
        let mut tail = crate::world::incremental_world_event_tail::CurrentWorldEventTailWorkV1 {
            originals_remaining: 1,
            original_bytes_remaining: 1 << 20,
            decode: super::audit_work(),
        };
        let (outcome, completed) = target
            .execute_selected_core_v1(
                selected.expect("selected Core fixture"),
                fixture.current.runtime().revocation(),
                &current,
                &mut admission_read,
                &mut materials,
                &mut read,
                &mut scan,
                &mut policy,
                &mut super::audit_work(),
                directory,
                &mut rule,
                &mut directory_work(),
                &mut tail,
                &mut || *live,
            )
            .unwrap();
        (
            outcome,
            Some(completed),
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
        )
    } else if let Some(limits) = body_limits {
        let mut admission_io = genesis::io();
        let mut admission_keys = genesis::keys();
        let mut admission_read =
            oasis7_world_state::incremental_field_read::WorldFieldReadContextV1 {
                root: fixture.roots.state_root(StateNamespaceV1::Authorization),
                state: reader,
                values: ValueReadContext {
                    source: reader,
                    memory,
                    class: MemoryClass::Ordinary,
                    io: &mut admission_io,
                },
                key_work: &mut admission_keys,
                semantic_bytes: 4096,
                record_bytes: 16 << 20,
                take_state_error: || reader.cas.error(),
                cancelled: || false,
            };
        let mut scan = genesis::scan();
        let mut policy = genesis::current_work(&fixture.roots).runtime.revocation;
        let guard = target.ctx.transaction.guard();
        let current = oasis7_world_state::incremental_runtime_revocation::begin_runtime_revocation_current_v1(
            fixture.current.runtime().revocation(), target.ctx, &mut admission_read,
            &mut scan, &mut policy, target.members, &guard, live).unwrap();
        let mut backing = MaterialFixtureV1::build(
            &fixture.roots,
            reader,
            reader,
            memory,
            limits,
            || reader.cas.error(),
            || false,
        );
        let mut materials = backing.context(&fixture.roots, reader, || reader.cas.error());
        let mut ri = genesis::io();
        let mut rk = genesis::keys();
        let mut pi = genesis::io();
        let mut pk = genesis::keys();
        let mut read = oasis7_world_state::incremental_field_read::WorldFieldReadContextV1 {
            root: fixture.roots.state_root(StateNamespaceV1::Business),
            state: reader,
            values: ValueReadContext {
                source: reader,
                memory,
                class: MemoryClass::Ordinary,
                io: &mut ri,
            },
            key_work: &mut rk,
            semantic_bytes: 4096,
            record_bytes: 16 << 20,
            take_state_error: || reader.cas.error(),
            cancelled: || false,
        };
        let mut parent_read = oasis7_world_state::incremental_field_read::WorldFieldReadContextV1 {
            root: fixture.roots.state_root(StateNamespaceV1::Business),
            state: reader,
            values: ValueReadContext {
                source: reader,
                memory,
                class: MemoryClass::Ordinary,
                io: &mut pi,
            },
            key_work: &mut pk,
            semantic_bytes: 4096,
            record_bytes: 16 << 20,
            take_state_error: || reader.cas.error(),
            cancelled: || empty_cancelled || stop.load(std::sync::atomic::Ordering::Relaxed),
        };
        let mut tail = crate::world::incremental_world_event_tail::CurrentWorldEventTailWorkV1 {
            originals_remaining: 1,
            original_bytes_remaining: 1 << 20,
            decode: super::audit_work(),
        };
        use crate::world::incremental_module_invocation::{
            CompletedNonmovementActionV1, NonmovementSelectedWorkV1,
        };
        let work = match body_work.as_mut().unwrap() {
            NonmovementFixtureWorkV1::LogisticsRegistration { work, common, .. } => {
                NonmovementSelectedWorkV1::LogisticsRegistration(
                    &mut **work,
                    fixture.current.native().logistics(),
                    common,
                )
            }
            NonmovementFixtureWorkV1::NormalExtension { work, .. } => {
                NonmovementSelectedWorkV1::NormalExtension(work.reborrow())
            }
            NonmovementFixtureWorkV1::Crisis(work) => NonmovementSelectedWorkV1::Crisis(
                &mut **work,
                crate::world::incremental_action_crisis::CrisisSourcesV1 {
                    agents: fixture.current.raw().agents(),
                    diplomacy: fixture.current.native().diplomacy(),
                },
            ),
            NonmovementFixtureWorkV1::ContractAccept(work) => {
                NonmovementSelectedWorkV1::ContractAccept(
                    &mut **work,
                    crate::world::incremental_action_contract_accept::ContractAcceptSourcesV1 {
                        agents: fixture.current.raw().agents(),
                        diplomacy: fixture.current.native().diplomacy(),
                    },
                )
            }
            NonmovementFixtureWorkV1::MetaProgress(work) => {
                NonmovementSelectedWorkV1::MetaProgress(
                    &mut **work,
                    crate::world::incremental_action_meta_progress::MetaProgressSourcesV1 {
                        agents: fixture.current.raw().agents(),
                        native: fixture.current.native().native(),
                    },
                )
            }
            NonmovementFixtureWorkV1::FactoryLifecycle(work) => {
                NonmovementSelectedWorkV1::FactoryLifecycle(
                    &mut **work,
                    crate::world::incremental_action_factory_lifecycle::FactoryLifecycleSourcesV1 {
                        agents: fixture.current.raw().agents(),
                        factories: fixture.current.native().factory(),
                        industry: fixture.current.native().industry(),
                        index: fixture.current.native().factory_lifecycle_index(),
                        native: fixture.current.native().native(),
                        primitive: fixture.current.native().primitive(),
                        inventory: fixture.current.native().inventory(),
                    },
                )
            }
            NonmovementFixtureWorkV1::RouteAvailability(work) => {
                NonmovementSelectedWorkV1::RouteAvailability(
                    &mut **work,
                    fixture.current.native().logistics(),
                )
            }
            NonmovementFixtureWorkV1::MaterialTransfer(work) => {
                NonmovementSelectedWorkV1::MaterialTransfer(
                    &mut **work,
                    fixture.current.native().material(),
                )
            }
            NonmovementFixtureWorkV1::DataAccess(work) => NonmovementSelectedWorkV1::DataAccess(
                &mut **work,
                fixture.current.native().controls(),
            ),
            NonmovementFixtureWorkV1::Body(work) => NonmovementSelectedWorkV1::Body(&mut **work),
            NonmovementFixtureWorkV1::AuthenticatedCollectData(work, auth) => {
                NonmovementSelectedWorkV1::AuthenticatedCollectData(
                    &mut **work,
                    &mut **auth,
                    fixture.current.native().native().starter_binding_index(),
                    fixture.current.native().controls(),
                )
            }
            NonmovementFixtureWorkV1::CollectData(work) => {
                NonmovementSelectedWorkV1::CollectData(&mut **work)
            }
            NonmovementFixtureWorkV1::TransferResource(work) => {
                NonmovementSelectedWorkV1::TransferResource(&mut **work)
            }
        };
        let (outcome, completed) = match input {
            FixtureInputV1::Selected(selected) => target
                .execute_selected_nonmovement_v1(
                    selected,
                    fixture.current.runtime().revocation(),
                    &current,
                    &mut admission_read,
                    &mut scan,
                    &mut policy,
                    &mut materials,
                    &mut read,
                    &mut parent_read,
                    work,
                    body_audit.as_deref_mut().unwrap(),
                    directory,
                    &mut rule,
                    &mut directory_work(),
                    &mut tail,
                    &mut || *live,
                )
                .unwrap(),
            FixtureInputV1::Pending(pending, pending_work) => target
                .execute_next_nonmovement_for_component_v1(
                    pending,
                    pending_work,
                    fixture.current.runtime().revocation(),
                    &current,
                    &mut admission_read,
                    &mut scan,
                    &mut policy,
                    &mut materials,
                    &mut read,
                    &mut parent_read,
                    work,
                    body_audit.as_deref_mut().unwrap(),
                    directory,
                    &mut rule,
                    &mut directory_work(),
                    &mut tail,
                    &mut || *live,
                )
                .unwrap()
                .expect("Root FIFO transfer result"),
            FixtureInputV1::PendingFallible(pending, pending_work, failure, cancelled) => {
                // Arm the requested signal after genuine fixture Current setup,
                // immediately before the actual Root FIFO selection.
                if let Some(cancelled) = cancelled {
                    stop.store(
                        cancelled.load(std::sync::atomic::Ordering::Relaxed),
                        std::sync::atomic::Ordering::Relaxed,
                    );
                }
                match target.execute_next_nonmovement_for_component_v1(
                    pending,
                    pending_work,
                    fixture.current.runtime().revocation(),
                    &current,
                    &mut admission_read,
                    &mut scan,
                    &mut policy,
                    &mut materials,
                    &mut read,
                    &mut parent_read,
                    work,
                    body_audit.as_deref_mut().unwrap(),
                    directory,
                    &mut rule,
                    &mut directory_work(),
                    &mut tail,
                    &mut || *live,
                ) {
                    Ok(value) => value.expect("Root fallible FIFO selection"),
                    Err(error) => {
                        *failure = Some(error);
                        return (
                            None, None, None, None, None, None, None, None, None, None, None, None,
                        );
                    }
                }
            }
            FixtureInputV1::EmptyCancelled(pending, pending_work) => {
                let result = target.execute_next_nonmovement_for_component_v1(
                    pending,
                    pending_work,
                    fixture.current.runtime().revocation(),
                    &current,
                    &mut admission_read,
                    &mut scan,
                    &mut policy,
                    &mut materials,
                    &mut read,
                    &mut parent_read,
                    work,
                    body_audit.as_deref_mut().unwrap(),
                    directory,
                    &mut rule,
                    &mut directory_work(),
                    &mut tail,
                    &mut || *live,
                );
                let detail = match &result {
                    Err(error) => format!("Err({error:?})"),
                    Ok(_) => "Ok".into(),
                };
                assert_eq!(
                    detail, "Err(Business(Module(Body(Value(Cancelled)))))",
                    "cancelled empty FIFO must return original Cancelled"
                );
                return (
                    None, None, None, None, None, None, None, None, None, None, None, None,
                );
            }
        };
        match completed {
            CompletedNonmovementActionV1::LogisticsRegistration(value) => {
                let Some(NonmovementFixtureWorkV1::LogisticsRegistration { result, .. }) =
                    body_work.as_mut()
                else {
                    panic!("route registration requires its dedicated FIFO fixture");
                };
                assert!(result.is_none());
                **result = Some(value);
                (
                    outcome, None, None, None, None, None, None, None, None, None, None, None,
                )
            }
            CompletedNonmovementActionV1::NormalExtension(value) => {
                let Some(NonmovementFixtureWorkV1::NormalExtension { result, .. }) =
                    body_work.as_mut()
                else {
                    panic!("normal extension requires its dedicated FIFO fixture");
                };
                assert!(result.is_none(), "one genuine FIFO result per fixture call");
                **result = Some(value);
                (
                    outcome, None, None, None, None, None, None, None, None, None, None, None,
                )
            }
            CompletedNonmovementActionV1::Core(value) => (
                outcome,
                Some(value),
                None,
                None,
                None,
                None,
                None,
                None,
                None,
                None,
                None,
                None,
            ),
            CompletedNonmovementActionV1::ContractAccept(value) => (
                outcome,
                None,
                None,
                None,
                None,
                None,
                None,
                None,
                None,
                None,
                None,
                Some(value),
            ),

            CompletedNonmovementActionV1::Crisis(value) => (
                outcome,
                None,
                None,
                None,
                None,
                None,
                None,
                None,
                None,
                None,
                Some(value),
                None,
            ),
            CompletedNonmovementActionV1::MetaProgress(value) => (
                outcome,
                None,
                None,
                None,
                None,
                None,
                None,
                None,
                None,
                Some(value),
                None,
                None,
            ),
            CompletedNonmovementActionV1::FactoryLifecycle(value) => (
                outcome,
                None,
                None,
                None,
                None,
                None,
                None,
                None,
                Some(value),
                None,
                None,
                None,
            ),
            CompletedNonmovementActionV1::RouteAvailability(value) => (
                outcome,
                None,
                None,
                None,
                None,
                None,
                None,
                Some(value),
                None,
                None,
                None,
                None,
            ),
            CompletedNonmovementActionV1::MaterialTransfer(value) => (
                outcome,
                None,
                None,
                None,
                None,
                None,
                Some(value),
                None,
                None,
                None,
                None,
                None,
            ),
            CompletedNonmovementActionV1::DataAccess(value) => (
                outcome,
                None,
                None,
                None,
                None,
                Some(value),
                None,
                None,
                None,
                None,
                None,
                None,
            ),
            CompletedNonmovementActionV1::Body(value) => (
                outcome,
                None,
                Some(value),
                None,
                None,
                None,
                None,
                None,
                None,
                None,
                None,
                None,
            ),
            CompletedNonmovementActionV1::CollectData(value) => (
                outcome,
                None,
                None,
                Some(value),
                None,
                None,
                None,
                None,
                None,
                None,
                None,
                None,
            ),
            CompletedNonmovementActionV1::TransferResource(value) => (
                outcome,
                None,
                None,
                None,
                Some(value),
                None,
                None,
                None,
                None,
                None,
                None,
                None,
            ),
        }
    } else {
        (
            target
                .execute_pre_action_rule_phases_v1(
                    selected.expect("selected Rule fixture"),
                    directory,
                    &mut rule,
                    &mut || *live,
                )
                .unwrap(),
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
        )
    };
    assert_eq!(result.decision.verdict, crate::RuleVerdict::Allow);
    assert!(result.decision.override_action.is_none());
    if let Some(selected) = selected {
        assert!(
            result
                .override_action_for_selected_v1(selected, target.ctx.transaction)
                .unwrap()
                .is_none()
        );
    }
    (
        Some(result),
        completed,
        body_completed,
        data_completed,
        transfer_completed,
        access_completed,
        material_completed,
        route_completed,
        factory_completed,
        meta_completed,
        crisis_completed,
        contract_accept_completed,
    )
}

#[path = "incremental_action_body_tests_rules_meta_progress.rs"]
mod meta_progress;
pub(in crate::world) use meta_progress::*;

#[path = "incremental_action_body_tests_rules_crisis.rs"]
mod crisis;
pub(in crate::world) use crisis::*;

#[path = "incremental_action_body_tests_rules_contract_accept.rs"]
mod contract_accept;
pub(in crate::world) use contract_accept::*;

#[allow(clippy::too_many_arguments)]
pub(in crate::world) fn evaluate_with_normal_extension<'a>(
    fixture: &genesis::Fixture<'a>,
    reader: &Reader<'a>,
    transaction: &mut ExecutionTransaction<'_, Reader<'a>>,
    pending: &mut oasis7_proto::memory_budget::ReservedValue<
        oasis7_world_state::incremental_pending_action_overlay::PendingActionCurrentV1,
    >,
    pending_work:&mut oasis7_world_state::incremental_pending_action_family::PendingActionFamilyWorkV1,
    directory: &oasis7_proto::memory_budget::ReservedValue<AcceptedInvocationDirectoryV1>,
    counter: &WorldEventCounterV1,
    originals: &mut Originals<'a>,
    memory: &'a MemoryBudget,
    now: u64,
    live: &ExecutionFence,
    body: NonmovementExecutionFixtureV1<'_>,
) -> oasis7_proto::memory_budget::ReservedValue<RootRulePhaseOutcomeV1> {
    assert!(matches!(
        &body.work,
        NonmovementFixtureWorkV1::NormalExtension { .. }
    ));
    let result = evaluate(
        fixture,
        reader,
        transaction,
        FixtureInputV1::Pending(pending, pending_work),
        directory,
        counter,
        originals,
        memory,
        now,
        live,
        None,
        Some(body),
    );
    assert!(
        result.1.is_none()
            && result.2.is_none()
            && result.3.is_none()
            && result.4.is_none()
            && result.5.is_none()
            && result.6.is_none()
            && result.7.is_none()
            && result.8.is_none()
            && result.9.is_none()
            && result.10.is_none()
            && result.11.is_none()
    );
    result.0.expect("actual selected Rule outcome")
}

#[allow(clippy::too_many_arguments)]
pub(in crate::world) fn evaluate_with_logistics_registration<'a>(
    fixture: &genesis::Fixture<'a>,
    reader: &Reader<'a>,
    transaction: &mut ExecutionTransaction<'_, Reader<'a>>,
    pending: &mut oasis7_proto::memory_budget::ReservedValue<
        oasis7_world_state::incremental_pending_action_overlay::PendingActionCurrentV1,
    >,
    pending_work:&mut oasis7_world_state::incremental_pending_action_family::PendingActionFamilyWorkV1,
    directory: &oasis7_proto::memory_budget::ReservedValue<AcceptedInvocationDirectoryV1>,
    counter: &WorldEventCounterV1,
    originals: &mut Originals<'a>,
    memory: &'a MemoryBudget,
    now: u64,
    live: &ExecutionFence,
    body: NonmovementExecutionFixtureV1<'_>,
) -> oasis7_proto::memory_budget::ReservedValue<RootRulePhaseOutcomeV1> {
    assert!(matches!(
        &body.work,
        NonmovementFixtureWorkV1::LogisticsRegistration { .. }
    ));
    let result = evaluate(
        fixture,
        reader,
        transaction,
        FixtureInputV1::Pending(pending, pending_work),
        directory,
        counter,
        originals,
        memory,
        now,
        live,
        None,
        Some(body),
    );
    assert!(
        result.1.is_none()
            && result.2.is_none()
            && result.3.is_none()
            && result.4.is_none()
            && result.5.is_none()
            && result.6.is_none()
            && result.7.is_none()
            && result.8.is_none()
            && result.9.is_none()
            && result.10.is_none()
            && result.11.is_none()
    );
    result.0.expect("actual selected Rule outcome")
}
