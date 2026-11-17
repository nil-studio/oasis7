//! Explicit remaining Host work borrowed by the fixed normal consumer.
//! These values carry allowances only: they cannot bind an input, family,
//! transaction, worker, accepted parent or completion permission.
use crate::world::{
    incremental_body::BodyMutationError,
    incremental_body_event::{AgentEventOriginalV1, materials::CorePolicyMaterialContextV1},
    incremental_factory_phase::{
        FactoryPhaseAcceptedCachesV1, FactoryPhaseFamiliesV1, FactoryPhaseWorkV1,
    },
    incremental_module_execution_manifest_hash::RootWorldManifestHashWorkV1,
    incremental_module_invocation::*,
    incremental_normal_action_work::NormalActionWorkSetV1,
    incremental_normal_industry_due::delivery::{
        CurrentProductValidationDeliveryIndexV1, ProductValidationDeliveryWorkV1,
    },
    incremental_normal_parent_caches::NormalParentCachesV1,
    incremental_normal_sources::BoundNormalTickParentV1,
    incremental_normal_tick_driver::{
        NormalTickCurrentDataV1, NormalTickDriverErrorV1, NormalTickTailSourcesV1,
        NormalTickTailWorkSetV1,
    },
    incremental_normal_world_rule_observation_v2::ObservationJsonOriginalSinkV2,
};
use oasis7_consensus::normal_tick_input::PaidBoundNormalInputDataV1;
use oasis7_formal_protocol::{
    controlled_authority::formal::bounded_context::FormalBoundedContextV1,
    incremental_execution::ExecutionFence,
    incremental_material::OriginalObjectStreamReader,
    incremental_schedule::SchedulePageWriter,
    incremental_state::{MerkleNodeReader, StatePageWriter, StateRootV1, StateScanBudget},
    incremental_value::{
        ValueIoBudget, ValueNodeReader, ValueNodeWriter, ValueReadContext, ValueTreeError,
    },
};
use oasis7_proto::memory_budget::{MemoryBudget, MemoryClass, ReservedValue};
use oasis7_world_state::{
    incremental_cognition_history::PendingCurrentCognitionJournalV1,
    incremental_factory_records::FactoryTransitionReadContextV1,
    incremental_field_member::{MemberRecordLimitsV1, MemberRecordWriteContext},
    incremental_field_read::WorldFieldReadContextV1,
    incremental_industry_jobs::{IndustryFamilyReadContextV1, IndustryTransitionReadContextV1},
    incremental_pending_action_family::PendingActionFamilyWorkV1,
    incremental_pending_action_overlay::PendingActionCurrentV1,
    incremental_runtime_revocation::{
        RevocationReadWorkV1, RuntimeRevocationCurrentV1, RuntimeRevocationQualificationV1,
    },
    incremental_semantic_key::SemanticKeyBudget,
    incremental_world_event_decode::WorldEventDecodeWorkV1,
};

/// Independent caller-provided logical IO lanes are not copies of one balance.
/// Each borrowed context returns to the very same counters on Err and unwind.
pub struct NormalHostReadWorkV1 {
    pub io: ValueIoBudget,
    pub keys: SemanticKeyBudget,
    pub semantic_bytes: u64,
    pub record_bytes: u64,
    pub max_payload_bytes: u64,
}
impl NormalHostReadWorkV1 {
    pub(in crate::world) fn field<'a, M, R, E, C>(
        &'a mut self,
        root: &'a StateRootV1,
        state: &'a M,
        values: &'a R,
        memory: &'a MemoryBudget,
        take_state_error: E,
        cancelled: C,
    ) -> WorldFieldReadContextV1<'a, M, R, E, C> {
        WorldFieldReadContextV1 {
            root,
            state,
            values: ValueReadContext {
                source: values,
                memory,
                class: MemoryClass::Ordinary,
                io: &mut self.io,
            },
            key_work: &mut self.keys,
            semantic_bytes: self.semantic_bytes,
            record_bytes: self.record_bytes,
            take_state_error,
            cancelled,
        }
    }
    pub(in crate::world) fn factory<'a, R, C>(
        &'a mut self,
        values: &'a R,
        memory: &'a MemoryBudget,
        cancelled: C,
    ) -> FactoryTransitionReadContextV1<'a, R, C> {
        FactoryTransitionReadContextV1 {
            values: ValueReadContext {
                source: values,
                memory,
                class: MemoryClass::Ordinary,
                io: &mut self.io,
            },
            key_work: &mut self.keys,
            semantic_bytes: self.semantic_bytes,
            record_bytes: self.record_bytes,
            max_payload_bytes: self.max_payload_bytes,
            cancelled,
        }
    }
    pub(in crate::world) fn recipe<'a, R, C>(
        &'a mut self,
        values: &'a R,
        memory: &'a MemoryBudget,
        cancelled: C,
    ) -> IndustryTransitionReadContextV1<'a, R, C> {
        IndustryTransitionReadContextV1 {
            values: ValueReadContext {
                source: values,
                memory,
                class: MemoryClass::Ordinary,
                io: &mut self.io,
            },
            key_work: &mut self.keys,
            semantic_bytes: self.semantic_bytes,
            record_bytes: self.record_bytes,
            max_payload_bytes: self.max_payload_bytes,
            cancelled,
        }
    }
}
pub struct NormalHostWriteWorkV1 {
    pub io: ValueIoBudget,
    pub keys: SemanticKeyBudget,
    pub limits: MemberRecordLimitsV1,
}
impl NormalHostWriteWorkV1 {
    pub(in crate::world) fn members<'a, W, C>(
        &'a mut self,
        sink: &'a mut W,
        memory: &'a MemoryBudget,
        cancelled: C,
    ) -> MemberRecordWriteContext<'a, W, C> {
        MemberRecordWriteContext {
            sink,
            memory,
            io: &mut self.io,
            key_work: &mut self.keys,
            limits: self.limits,
            cancelled,
        }
    }
}
/// Every context needed simultaneously by the fixed driver has its own actual
/// caller-owned lane. Physical page allowances stay in the Chain IO owner.
pub struct NormalHostIoWorkV1 {
    pub overlay: NormalHostReadWorkV1,
    pub current: NormalHostReadWorkV1,
    pub parent: NormalHostReadWorkV1,
    pub admission: NormalHostReadWorkV1,
    pub factory: NormalHostReadWorkV1,
    pub recipe: NormalHostReadWorkV1,
    pub industry: NormalHostReadWorkV1,
    pub industry_scan: StateScanBudget,
    pub members: NormalHostWriteWorkV1,
    pub fees: NormalHostWriteWorkV1,
    pub events: NormalHostWriteWorkV1,
}
/// Kept separate from the driver loan: RootContext borrows these for its whole
/// lifetime while the driver sequentially borrows its action/tail work.
pub struct NormalHostInvocationWorkV1 {
    pub invocation: RootModuleExecutionWorkV1,
    pub manifest: RootWorldManifestHashWorkV1,
    pub event_original: oasis7_wasm_abi::CanonicalCborWorkV1,
    pub max_event_bytes: usize,
}
/// The original bootstrap writes borrow these remaining lanes; no implicit
/// defaults or fresh allowance is created when a reserved input is present.
pub struct NormalHostBootstrapWorkV1 {
    pub finality: super::incremental_normal_bootstrap_finality::CurrentBootstrapFinalityWorkV1,
    pub revocation_write: oasis7_world_state::incremental_runtime_revocation::RevocationWriteWorkV1,
    pub authorization_write:
        oasis7_world_state::incremental_runtime_authorization_genesis::AuthorizationWriteWorkV1,
    pub groups: oasis7_world_state::incremental_cognition_groups::CognitionGroupWriteWorkV1,
    pub journal: ProductValidationDeliveryWorkV1,
}
pub struct NormalHostDriverWorkV1 {
    pub bootstrap: NormalHostBootstrapWorkV1,
    pub factory: FactoryPhaseWorkV1,
    pub pending: PendingActionFamilyWorkV1,
    // The same family owner and write allowance feed every committed enqueue;
    // draining later reborrows `pending`, never a freshly begun parent queue.
    pub pending_write:
        oasis7_world_state::incremental_runtime_pending_action_record::PendingActionWriteWorkV1,
    pub policy_scan: StateScanBudget,
    pub policy: RevocationReadWorkV1,
    pub actions: NormalActionWorkSetV1,
    pub tail: NormalTickTailWorkSetV1,
    pub product: ProductValidationDeliveryWorkV1,
    pub audit: WorldEventDecodeWorkV1,
    pub rules: RootRulePhaseWorkV1,
    pub enumeration: CurrentInvocationEnumerationWorkV1,
    pub event_tail: crate::world::incremental_world_event_tail::CurrentWorldEventTailWorkV1,
    pub finish_cut: super::incremental_normal_host_finish::NormalHostFinishCutWorkV1,
    pub final_sources: super::incremental_normal_final_sources::NormalFinalSourcesWorkV1,
}
/// Closing directories reuses the exact action/tail quotas after execution.
/// In particular grant recovery keeps the claim-stage fixed/eligibility lanes.
pub(in crate::world) struct NormalHostDirectoryWorkLoanV1<'a>{
    pub finish_cut: &'a mut super::incremental_normal_host_finish::NormalHostFinishCutWorkV1,
    pub factory:&'a mut FactoryPhaseWorkV1,
    pub inventory:&'a mut oasis7_world_state::incremental_inventory::InventoryIdentityDirectoryWorkV1,
    pub claims:&'a mut oasis7_world_state::incremental_world_native::IndustrialClaimDirectoryWorkV1,
    pub grant_cells:&'a mut u64,pub grant_pending:&'a mut usize,
    pub gameplay:&'a mut oasis7_world_state::incremental_normal_gameplay_cycles_directory::GameplayDirectoryWorkV1,
    pub threats:&'a mut oasis7_world_state::incremental_runtime_business::ThreatKeyDirectoryWorkV1,
    pub builds:&'a mut oasis7_world_state::incremental_factory_schedule_index::FactoryScheduleIndexWorkV1,
    pub receipts:&'a mut oasis7_world_state::incremental_factory_schedule_receipts::FactoryScheduleReceiptWorkV1,
}
/// A fixed, exclusive reborrow. No field is cloned or initialized here.
pub(in crate::world) struct NormalHostDriverWorkLoanV1<'a> {
    factory: &'a mut FactoryPhaseWorkV1,
    finish_cut: &'a mut super::incremental_normal_host_finish::NormalHostFinishCutWorkV1,
    final_sources: &'a mut super::incremental_normal_final_sources::NormalFinalSourcesWorkV1,
    pending: &'a mut PendingActionFamilyWorkV1,
    pending_write: &'a mut oasis7_world_state::incremental_runtime_pending_action_record::PendingActionWriteWorkV1,
    policy_scan: &'a mut StateScanBudget,
    policy: &'a mut RevocationReadWorkV1,
    actions: &'a mut NormalActionWorkSetV1,
    tail: &'a mut NormalTickTailWorkSetV1,
    product: &'a mut ProductValidationDeliveryWorkV1,
    audit: &'a mut WorldEventDecodeWorkV1,
    rules: &'a mut RootRulePhaseWorkV1,
    enumeration: &'a mut CurrentInvocationEnumerationWorkV1,
    event_tail: &'a mut crate::world::incremental_world_event_tail::CurrentWorldEventTailWorkV1,
}
impl NormalHostDriverWorkV1 {
    pub(in crate::world) fn bootstrap_loan<'a>(
        &'a mut self,
        data_limits: &'a oasis7_formal_protocol::controlled_authority::formal::bounded_cbor_data::CborDataLimitsV1,
    ) -> super::normal_bootstrap_current::CurrentBootstrapWorkV1<'a, 'a> {
        super::normal_bootstrap_current::CurrentBootstrapWorkV1 {
            module: self.actions.module_lifecycle.view(),
            factory: &mut self.actions.factory_schedule,
            cognition: &mut self.tail.cognition,
            eligibility: &mut self.tail.world_delivery.eligibility,
            orders: &mut self.tail.orders,
            revocation_write: &mut self.bootstrap.revocation_write,
            authorization_write: &mut self.bootstrap.authorization_write,
            groups: &mut self.bootstrap.groups,
            journal: &mut self.bootstrap.journal,
            data_limits,
        }
    }

    pub(in crate::world) fn committed_input_queue_loan(
        &mut self,
    ) -> crate::world::incremental_normal_committed_input::CommittedInputQueueWorkV1<'_> {
        crate::world::incremental_normal_committed_input::CommittedInputQueueWorkV1 {
            pending: &mut self.pending,
            write: &mut self.pending_write,
        }
    }

    pub(in crate::world) fn directory_loan(&mut self) -> NormalHostDirectoryWorkLoanV1<'_> {
        NormalHostDirectoryWorkLoanV1 {
            finish_cut: &mut self.finish_cut,
            factory: &mut self.factory,
            inventory: &mut self.actions.factory_schedule.inventory_directory,
            claims: &mut self.tail.world_delivery.claims,
            grant_cells: &mut self.tail.world_delivery.fixed_cells_remaining,
            grant_pending: &mut self
                .tail
                .world_delivery
                .eligibility
                .pending_entries_remaining,
            gameplay: &mut self.tail.gameplay.directory,
            threats: &mut self.tail.threat_keys,
            builds: &mut self.actions.factory_schedule.builds,
            receipts: &mut self.actions.factory_schedule.receipts,
        }
    }
    pub(in crate::world) fn final_preparation_loans(
        &mut self,
    ) -> (
        &mut super::incremental_normal_final_sources::NormalFinalSourcesWorkV1,
        NormalHostDirectoryWorkLoanV1<'_>,
    ) {
        (
            &mut self.final_sources,
            NormalHostDirectoryWorkLoanV1 {
                finish_cut: &mut self.finish_cut,
                factory: &mut self.factory,
                inventory: &mut self.actions.factory_schedule.inventory_directory,
                claims: &mut self.tail.world_delivery.claims,
                grant_cells: &mut self.tail.world_delivery.fixed_cells_remaining,
                grant_pending: &mut self
                    .tail
                    .world_delivery
                    .eligibility
                    .pending_entries_remaining,
                gameplay: &mut self.tail.gameplay.directory,
                threats: &mut self.tail.threat_keys,
                builds: &mut self.actions.factory_schedule.builds,
                receipts: &mut self.actions.factory_schedule.receipts,
            },
        )
    }
    pub(in crate::world) fn loan(&mut self) -> NormalHostDriverWorkLoanV1<'_> {
        NormalHostDriverWorkLoanV1 {
            factory: &mut self.factory,
            finish_cut: &mut self.finish_cut,
            final_sources: &mut self.final_sources,
            pending: &mut self.pending,
            pending_write: &mut self.pending_write,
            policy_scan: &mut self.policy_scan,
            policy: &mut self.policy,
            actions: &mut self.actions,
            tail: &mut self.tail,
            product: &mut self.product,
            audit: &mut self.audit,
            rules: &mut self.rules,
            enumeration: &mut self.enumeration,
            event_tail: &mut self.event_tail,
        }
    }
}
/// Genuine material parent is supplied separately; only its original remaining
/// normalization, scope, family and group allowances are borrowed here.
pub struct NormalHostMaterialWorkV1 {
    pub read: NormalHostReadWorkV1,
    pub write_keys: SemanticKeyBudget,
    pub write_limits: MemberRecordLimitsV1,
    pub family: oasis7_world_state::incremental_inventory::InventoryFamilyWorkV1,
    pub scope: oasis7_world_state::incremental_inventory::InventoryMaterialScopeWorkV1,
    pub group: oasis7_world_state::incremental_inventory::InventoryWorldGroupWorkV1,
    pub normalization_max_union_entries: usize,
    pub normalization_cells_remaining: u64,
    pub normalization_comparison_bytes_remaining: u64,
}
impl NormalHostMaterialWorkV1 {
    pub(in crate::world) fn context<'a, M, E>(
        &'a mut self,
        parent: &'a ReservedValue<
            oasis7_world_state::incremental_inventory::ParentInventoryMaterialScopeV1,
        >,
        roots: &'a oasis7_formal_protocol::incremental_execution::RootSetV1,
        pages: &'a M,
        take_state_error: E,
    ) -> CorePolicyMaterialContextV1<'a, M, E> {
        CorePolicyMaterialContextV1 {
            parent,
            roots,
            pages,
            read_io: &mut self.read.io,
            read_keys: &mut self.read.keys,
            write_keys: &mut self.write_keys,
            semantic_bytes: self.read.semantic_bytes,
            record_bytes: self.read.record_bytes,
            write_limits: self.write_limits,
            take_state_error,
            family_work: &mut self.family,
            scope_work: &mut self.scope,
            group_work: &mut self.group,
            normalization_max_union_entries: self.normalization_max_union_entries,
            normalization_cells_remaining: &mut self.normalization_cells_remaining,
            normalization_comparison_bytes_remaining: &mut self
                .normalization_comparison_bytes_remaining,
        }
    }
}
pub struct NormalHostWorkV1 {
    pub invocation: NormalHostInvocationWorkV1,
    pub io: NormalHostIoWorkV1,
    pub materials: NormalHostMaterialWorkV1,
    pub driver: NormalHostDriverWorkV1,
}

/// Combined original-input queue writes and the genuine fixed tick result.
/// Kept together until the later freeze/commit consumer; neither is a durable
/// acceptance or permission to omit deferred service/bootstrap inputs.
pub(in crate::world) struct NormalHostTickDataV1<'p> {
    pub input: super::incremental_normal_committed_input::BoundRuntimeQueueDataV1,
    pub tick: NormalTickCurrentDataV1<'p>,
    pub final_sources: std::sync::Arc<
        ReservedValue<super::incremental_normal_final_sources::CapturedNormalFinalSourcesV1>,
    >,
}
#[derive(Debug)]
pub(in crate::world) enum NormalHostTickErrorV1<OE> {
    Input(super::incremental_normal_committed_input::BoundRuntimeInputQueueErrorV1<OE>),
    Tick(NormalTickDriverErrorV1<OE>),
    Capture(RootModuleExecutionErrorV1<OE>),
    Finish(super::incremental_normal_host_finish::NormalHostFinishErrorV1<OE>),
}
impl<'a, 't, 's, 'v, M, R, E, C, W, F, O>
    RootModuleExecutionContextV1<'a, 't, 's, 'v, M, R, E, C, W, F, O>
where
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    W: ValueNodeReader + ValueNodeWriter + StatePageWriter + SchedulePageWriter,
    F: FnMut(&mut dyn AgentEventOriginalV1) -> Result<(), BodyMutationError>,
    O: OriginalObjectStreamReader
        + crate::world::PaidProjectionRetainedOriginalReaderV1<RetainedError = BodyMutationError>,
{
    #[allow(clippy::too_many_arguments)]
    pub(in crate::world) fn execute_normal_tick_with_host_work_v1<
        'p,
        P,
        CE,
        CC,
        PE,
        PC,
        AP,
        AE,
        AC,
        FR,
        FC,
        RR,
        RC,
        IE,
        IC,
        ME,
        S,
    >(
        &mut self,
        work: NormalHostDriverWorkLoanV1<'_>,
        parent_source: &'p ReservedValue<BoundNormalTickParentV1<'p>>,
        input: &PaidBoundNormalInputDataV1<'_>,
        bootstrap: Option<
            &super::normal_bootstrap_current::CommittedCurrentBootstrapBatchV1<'_, '_>,
        >,
        input_limits: &oasis7_formal_protocol::controlled_authority::formal::bounded_cbor_data::CborDataLimitsV1,
        max_input_action_bytes: usize,
        max_grant_payload_bytes: u64,
        factory_caches: &FactoryPhaseAcceptedCachesV1<'_>,
        factory_families: &FactoryPhaseFamiliesV1<'_>,
        factory_read: &mut FactoryTransitionReadContextV1<'_, FR, FC>,
        recipe_read: &mut IndustryTransitionReadContextV1<'_, RR, RC>,
        pending: &mut ReservedValue<PendingActionCurrentV1>,
        qualification: &ReservedValue<RuntimeRevocationQualificationV1>,
        revocation: &RuntimeRevocationCurrentV1,
        admission_read: &mut WorldFieldReadContextV1<'_, AP, R, AE, AC>,
        caches: &ReservedValue<
            crate::world::incremental_normal_parent_caches::NormalParentCachesV1,
        >,
        authorization: &crate::world::incremental_authorization_root_current::CurrentAuthorizationProjectionV1<'_>,
        business_parent: &ReservedValue<crate::world::incremental_normal_business_commitment::NormalBusinessMetadataParentSourceV1>,
        projection_parent: &ReservedValue<
            crate::world::incremental_normal_projection_commitment::AcceptedNormalProjectionCacheV1,
        >,
        compiler: &crate::module_source_compiler::RuntimeSourceCompiler,
        process: &crate::world::incremental_normal_process_source::NormalProcessSourceV1,
        tail_sources: &NormalTickTailSourcesV1<'_>,
        product_delivery: &mut CurrentProductValidationDeliveryIndexV1<'_>,
        industry_read: &mut IndustryFamilyReadContextV1<'_, M, R, IE, IC>,
        sink: &mut S,
        continued_cognition: Option<PendingCurrentCognitionJournalV1<'p>>,
        materials: &mut CorePolicyMaterialContextV1<'_, M, ME>,
        current: &mut WorldFieldReadContextV1<'_, P, R, CE, CC>,
        parent: &mut WorldFieldReadContextV1<'_, P, R, PE, PC>,
        directory: &ReservedValue<AcceptedInvocationDirectoryV1>,
        common: &mut FormalBoundedContextV1<'_>,
        live_owner: &mut dyn FnMut() -> ExecutionFence,
    ) -> Result<
        super::incremental_normal_host_finish::FinishedNormalTickDirectoriesV1<'p>,
        NormalHostTickErrorV1<O::Error>,
    >
    where
        P: MerkleNodeReader,
        CE: FnMut() -> Option<ValueTreeError>,
        CC: FnMut() -> bool,
        PE: FnMut() -> Option<ValueTreeError>,
        PC: FnMut() -> bool,
        AP: MerkleNodeReader,
        AE: FnMut() -> Option<ValueTreeError>,
        AC: FnMut() -> bool,
        FR: ValueNodeReader,
        FC: FnMut() -> bool,
        RR: ValueNodeReader,
        RC: FnMut() -> bool,
        IE: FnMut() -> Option<ValueTreeError>,
        IC: FnMut() -> bool,
        ME: FnMut() -> Option<ValueTreeError>,
        S: ObservationJsonOriginalSinkV2,
        S::Error: std::fmt::Debug + Send + Sync + 'static,
        O::Error: std::error::Error + Send + Sync + 'static,
    {
        // The same ongoing FIFO and the same remaining family work feed
        // Original publication, every committed enqueue, then the full tick.
        // No caller-provided "inputs applied" flag can skip this sequence.
        let retention = super::incremental_normal_retention::NormalRetentionParticipantV1::begin(
            parent_source,
            self.ctx.transaction,
            common,
            &live_owner(),
        )
        .map_err(|e| NormalHostTickErrorV1::Capture(e.into()))?;
        let applied = self
            .enqueue_bound_runtime_block_data_v1(
                parent_source,
                input,
                bootstrap,
                pending,
                &mut super::incremental_normal_committed_input::CommittedInputQueueWorkV1 {
                    pending: &mut *work.pending,
                    write: &mut *work.pending_write,
                },
                input_limits,
                max_input_action_bytes,
                common,
                live_owner,
            )
            .map_err(NormalHostTickErrorV1::Input)?;
        let mut tick = self
            .execute_normal_current_tick_with_cached_parent_v1(
                parent_source,
                input,
                factory_caches,
                factory_families,
                factory_read,
                recipe_read,
                work.factory,
                pending,
                work.pending,
                qualification,
                revocation,
                admission_read,
                work.policy_scan,
                work.policy,
                caches,
                authorization,
                compiler,
                work.actions,
                tail_sources,
                work.tail,
                product_delivery,
                work.product,
                industry_read,
                sink,
                continued_cognition,
                materials,
                current,
                parent,
                work.audit,
                directory,
                work.rules,
                work.enumeration,
                work.event_tail,
                common,
                live_owner,
            )
            .map_err(NormalHostTickErrorV1::Tick)?;
        let final_sources = self
            .capture_normal_final_sources_v1(
                &mut tick,
                parent_source,
                input,
                tail_sources,
                retention,
                bootstrap.map(|b| b.frontier_source()),
                authorization,
                revocation,
                business_parent,
                projection_parent,
                parent,
                product_delivery,
                work.final_sources,
                work.finish_cut,
                caches,
                compiler,
                process,
                work.actions,
                common,
                live_owner,
            )
            .map_err(NormalHostTickErrorV1::Capture)?;
        // Close the same actual Current graph before giving a handoff to
        // the seal consumer. These are the remaining action/tail allowances,
        // not a second set of directory quotas initialized after execution.
        self.finish_normal_tick_directories_v1(
            NormalHostTickDataV1 {
                input: applied,
                tick,
                final_sources,
            },
            parent_source,
            input,
            caches,
            tail_sources,
            materials.parent,
            factory_read,
            recipe_read,
            NormalHostDirectoryWorkLoanV1 {
                finish_cut: work.finish_cut,
                factory: work.factory,
                inventory: &mut work.actions.factory_schedule.inventory_directory,
                claims: &mut work.tail.world_delivery.claims,
                grant_cells: &mut work.tail.world_delivery.fixed_cells_remaining,
                grant_pending: &mut work
                    .tail
                    .world_delivery
                    .eligibility
                    .pending_entries_remaining,
                gameplay: &mut work.tail.gameplay.directory,
                threats: &mut work.tail.threat_keys,
                builds: &mut work.actions.factory_schedule.builds,
                receipts: &mut work.actions.factory_schedule.receipts,
            },
            max_grant_payload_bytes,
            common,
            live_owner,
        )
        .map_err(NormalHostTickErrorV1::Finish)
    }
}
