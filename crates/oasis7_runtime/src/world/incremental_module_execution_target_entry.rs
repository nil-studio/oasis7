//! Real tick invocation -> typed sealed VM -> original output kernel consumers.
use super::*;
use crate::world::incremental_module_execution_effects::heap::{
    EffectCloneWorkV1, json_clone_bound_v1,
};
use oasis7_wasm_abi::{ModuleCallCaller, ModuleCallInput, ModuleCallOrigin, ModuleContext};
use oasis7_wasm_executor::bounded_worker::{WorkerExecutableV1, WorkerLimitsV1};

#[derive(Clone, Copy)]
struct PreActionEnvelopeV1<'a> {
    original: &'a crate::ActionEnvelope,
    action: &'a crate::Action,
    retain_metadata: bool,
}
impl serde::Serialize for PreActionEnvelopeV1<'_> {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeStruct;
        let crate::ActionEnvelope {
            governance_authorization,
            committed_recipe_origin,
            id,
            action: _,
        } = self.original;
        let governance_authorization = governance_authorization
            .as_ref()
            .filter(|_| self.retain_metadata);
        let committed_recipe_origin = committed_recipe_origin
            .as_ref()
            .filter(|_| self.retain_metadata);
        let mut out = serializer.serialize_struct(
            "ActionEnvelope",
            2 + usize::from(governance_authorization.is_some())
                + usize::from(committed_recipe_origin.is_some()),
        )?;
        if let Some(value) = governance_authorization {
            out.serialize_field("governance_authorization", value)?;
        }
        if let Some(value) = committed_recipe_origin {
            out.serialize_field("committed_recipe_origin", value)?;
        }
        out.serialize_field("id", id)?;
        out.serialize_field("action", self.action)?;
        out.end()
    }
}
// Private invocation evidence and VM-only failure channel. A missing module
// and a genuine VM failure cannot be confused with a successful empty output.
struct DueProductInputV1<'a, 'job> {
    call: &'a crate::world::incremental_normal_industry_due::PreparedDueProductCallV1<'job>,
    failure: &'a RefCell<Option<ReservedValue<WorldError>>>,
}
enum EconomyInputV1<'a, 'source> {
    Due(DueProductInputV1<'a, 'source>),
    Factory {
        call: &'a crate::world::incremental_normal_action_factory_schedule::factory_module::PreparedFactoryModuleCallV1<'source>,
        failure: &'a RefCell<Option<ReservedValue<WorldError>>>,
    },
}
impl EconomyInputV1<'_, '_> {
    fn failure(&self) -> &RefCell<Option<ReservedValue<WorldError>>> {
        match self {
            Self::Due(value) => value.failure,
            Self::Factory { failure, .. } => failure,
        }
    }
}
// Keeps the original selection path distinct from the opaque due-route cut.
enum InputFirstSelectionV1 {
    Ordinary(PreparedRootModuleInvocationV1),
    Captured(crate::world::incremental_module_invocation::PreparedCapturedGameplayTickV1),
}
impl std::ops::Deref for InputFirstSelectionV1 {
    type Target = PreparedRootModuleInvocationV1;
    fn deref(&self) -> &Self::Target {
        match self {
            Self::Ordinary(value) => value,
            Self::Captured(value) => value,
        }
    }
}
impl<'job> std::ops::Deref for DueProductInputV1<'_, 'job> {
    type Target = crate::world::incremental_normal_industry_due::PreparedDueProductCallV1<'job>;
    fn deref(&self) -> &Self::Target {
        self.call
    }
}

pub struct RootModuleExecutionContextV1<
    'a,
    't,
    's,
    'v,
    M,
    R,
    E,
    C,
    W,
    F,
    O: OriginalObjectStreamReader,
> {
    pub families: RootModuleExecutionFamiliesV1<'a>,
    pub now: u64,
    pub state: &'a M,
    pub ctx: &'a mut WorldFieldOverlayContextV1<'t, 's, 'v, M, R, E, C>,
    pub members: &'a mut MemberRecordWriteContext<'v, W, C>,
    pub fees: &'a mut RootModuleFeeWriteContextV1<'v, W, C>,
    pub events: &'a mut ModuleOutputEventContextV1<'v, R, W, C, F>,
    pub originals: &'a mut O,
    pub worker: &'a mut BoundedWorkerSandboxV1<'v, RootModuleWorkerAllocatorV1>,
    pub manifest_cache: &'a ReservedValue<AcceptedWorldManifestHashV1>,
    pub manifest_work: &'a mut RootWorldManifestHashWorkV1,
    pub work: &'a mut RootModuleExecutionWorkV1,
    #[cfg(any(test, feature = "test-support"))]
    pub input_observation: Option<&'a mut Option<(usize, [u8; 32])>>,
}
impl<'a, 't, 's, 'v, M, R, E, C, W, F, O>
    RootModuleExecutionContextV1<'a, 't, 's, 'v, M, R, E, C, W, F, O>
where
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    W: ValueNodeReader
        + ValueNodeWriter
        + oasis7_formal_protocol::incremental_state::StatePageWriter,
    F: FnMut(
        &mut dyn crate::world::incremental_body_event::AgentEventOriginalV1,
    ) -> Result<(), BodyMutationError>,
    O: OriginalObjectStreamReader,
{
    /// Component executor only: the final private driver establishes the real
    /// due/command authorization and Native baseline before invoking this API.
    /// No complete execution permit or accepted state is returned here.
    pub fn execute_tick_invocation_v1(
        &mut self,
        invocation_id: &str,
        live_owner: &mut dyn FnMut() -> ExecutionFence,
    ) -> Result<Option<ReservedValue<ModuleOutput>>, RootModuleExecutionErrorV1<O::Error>> {
        let result = self.execute_invocation_inner(invocation_id, None, None, live_owner);
        match result {
            Ok(output) => Ok(output),
            Err(error) => {
                // An inner owner may already have poisoned this transaction.
                // Rejection must retain its first concrete error rather than
                // replace it with contribute's subsequent Aborted sentinel.
                let _: Result<(), oasis7_formal_protocol::incremental_execution::ExecutionError> =
                    self.ctx.transaction.contribute(|_| {
                        Err(oasis7_formal_protocol::incremental_execution::ExecutionError::Aborted)
                    });
                Err(error)
            }
        }
    }
    pub(in crate::world) fn execute_captured_gameplay_tick_v1(
        &mut self,
        slot: crate::world::incremental_normal_gameplay_cycles::DueGameplayTickSlotV1,
        core: oasis7_formal_protocol::controlled_authority::formal::bounded_context::WorkIdentityV1,
        live_owner: &mut dyn FnMut() -> ExecutionFence,
    ) -> Result<Option<ReservedValue<ModuleOutput>>, RootModuleExecutionErrorV1<O::Error>> {
        // The invocation ID is retained by the opaque slot through the complete
        // call; the fixed Tick input does not accept an independent caller ID.
        let result = self.execute_captured_input_invocation_inner(
            "",
            None,
            None,
            None,
            None,
            Some((slot, core)),
            live_owner,
        );
        if result.is_err() {
            let _: Result<(), oasis7_formal_protocol::incremental_execution::ExecutionError> =
                self.ctx.transaction.contribute(|_| {
                    Err(oasis7_formal_protocol::incremental_execution::ExecutionError::Aborted)
                });
        }
        result
    }
    pub(in crate::world) fn execute_pre_action_invocation_v1(
        &mut self,
        invocation_id: &str,
        action: &crate::world::incremental_action_execution::SelectedPendingActionV1,
        live_owner: &mut dyn FnMut() -> ExecutionFence,
    ) -> Result<Option<ReservedValue<ModuleOutput>>, RootModuleExecutionErrorV1<O::Error>> {
        let result = self.ctx.transaction.contribute(|tx| {
            action
                .validate(tx)
                .map_err(RootModuleExecutionErrorV1::from)
        });
        result?;
        let result = self.execute_invocation_inner(
            invocation_id,
            Some(PreActionEnvelopeV1 {
                original: action.envelope(),
                action: &action.envelope().action,
                retain_metadata: true,
            }),
            None,
            live_owner,
        );
        if result.is_err() {
            let _: Result<(), oasis7_formal_protocol::incremental_execution::ExecutionError> =
                self.ctx.transaction.contribute(|_| {
                    Err(oasis7_formal_protocol::incremental_execution::ExecutionError::Aborted)
                });
        }
        result
    }
    /// Route one genuine persisted Event through current invocation owners.
    pub(in crate::world) fn route_persisted_event_v1(
        &mut self,
        event: &crate::world::incremental_module_execution_event::PersistedWorldEventV1<'_>,
        product_validation_event_id_era: Option<u64>,
        directory: &ReservedValue<AcceptedInvocationDirectoryV1>,
        enumeration: &mut CurrentInvocationEnumerationWorkV1,
        live_owner: &mut dyn FnMut() -> ExecutionFence,
    ) -> Result<usize, RootModuleExecutionErrorV1<O::Error>> {
        self.route_completed_invocations(
            None,
            Some(event),
            product_validation_event_id_era,
            directory,
            enumeration,
            live_owner,
        )
    }

    /// PostAction subscriptions match the selected action; its persisted result
    /// is input only and never selects an Event subscription.
    pub(in crate::world) fn route_post_action_v1(
        &mut self,
        action: &crate::world::incremental_action_execution::SelectedPendingActionV1,
        result_event: Option<
            &crate::world::incremental_module_execution_event::PersistedWorldEventV1<'_>,
        >,
        directory: &ReservedValue<AcceptedInvocationDirectoryV1>,
        enumeration: &mut CurrentInvocationEnumerationWorkV1,
        live_owner: &mut dyn FnMut() -> ExecutionFence,
    ) -> Result<usize, RootModuleExecutionErrorV1<O::Error>> {
        self.ctx.transaction.contribute(|tx| {
            action
                .validate(tx)
                .map_err(RootModuleExecutionErrorV1::from)
        })?;
        if let Some(result) = result_event {
            self.ctx.transaction.contribute(|tx| {
                result.validate(tx)?;
                if result.event().caused_by != Some(crate::CausedBy::Action(action.envelope().id)) {
                    return Err(RootModuleExecutionErrorV1::Value(ValueTreeError::Invalid(
                        "post-action result action binding",
                    )));
                }
                Ok(())
            })?;
        }
        self.route_completed_invocations(
            Some(PreActionEnvelopeV1 {
                original: action.envelope(),
                action: &action.envelope().action,
                retain_metadata: true,
            }),
            result_event,
            None,
            directory,
            enumeration,
            live_owner,
        )
    }

    fn route_completed_invocations(
        &mut self,
        action: Option<PreActionEnvelopeV1<'_>>,
        event: Option<&crate::world::incremental_module_execution_event::PersistedWorldEventV1<'_>>,
        era: Option<u64>,
        directory: &ReservedValue<AcceptedInvocationDirectoryV1>,
        enumeration: &mut CurrentInvocationEnumerationWorkV1,
        live_owner: &mut dyn FnMut() -> ExecutionFence,
    ) -> Result<usize, RootModuleExecutionErrorV1<O::Error>> {
        let result = (|| {
            if let Some(event) = event {
                event.validate(self.ctx.transaction)?;
            }
            let live = live_owner();
            let guard = self.ctx.transaction.guard();
            let ids = collect_current_module_invocation_ids_v1(
                directory,
                self.families.instances,
                self.families.config,
                self.state,
                self.ctx,
                enumeration,
                &guard,
                &live,
            )
            .map_err(|_| ValueTreeError::Invalid("completed invocation directory"))?;
            let stage = if action.is_some() {
                oasis7_wasm_abi::ModuleSubscriptionStage::PostAction
            } else {
                oasis7_wasm_abi::ModuleSubscriptionStage::PostEvent
            };
            let mut invoked = 0usize;
            for id in ids.iter() {
                if self
                    .execute_routed_invocation_inner(
                        id,
                        action,
                        None,
                        Some((stage, event, era)),
                        live_owner,
                    )?
                    .is_some()
                {
                    invoked = invoked
                        .checked_add(1)
                        .ok_or(ValueTreeError::ResourceLimited("routed invocation count"))?;
                }
            }
            Ok(invoked)
        })();
        if result.is_err() {
            let _: Result<(), oasis7_formal_protocol::incremental_execution::ExecutionError> =
                self.ctx.transaction.contribute(|_| {
                    Err(oasis7_formal_protocol::incremental_execution::ExecutionError::Aborted)
                });
        }
        result
    }

    fn select(
        &mut self,
        invocation_id: &str,
        live: &ExecutionFence,
    ) -> Result<Option<PreparedRootModuleInvocationV1>, RootModuleExecutionErrorV1<O::Error>> {
        let guard = self.ctx.transaction.guard();
        prepare_root_module_invocation_v1(
            ModuleInvocationPreparationRequestV1 {
                invocation_id,
                parent_roots: self.families.roots,
                instances: self.families.instances,
                config: self.families.config,
                content: self.families.content,
                current_active: None,
                current_record: None,
                guard: &guard,
                live,
            },
            self.state,
            self.ctx,
            &mut self.work.selection_scan,
            &mut self.work.selection,
            self.originals,
            &mut self.work.original,
            &mut self.work.selection_limits,
        )
        .map_err(RootModuleExecutionErrorV1::Invocation)
    }
    fn select_registry(
        &mut self,
        invocation_id: &str,
        live: &ExecutionFence,
    ) -> Result<Option<PreparedRootModuleInvocationV1>, RootModuleExecutionErrorV1<O::Error>> {
        let guard = self.ctx.transaction.guard();
        crate::world::incremental_module_invocation::prepare_root_module_registry_invocation_v1(
            ModuleInvocationPreparationRequestV1 {
                invocation_id,
                parent_roots: self.families.roots,
                instances: self.families.instances,
                config: self.families.config,
                content: self.families.content,
                current_active: None,
                current_record: None,
                guard: &guard,
                live,
            },
            self.state,
            self.ctx,
            &mut self.work.selection_scan,
            &mut self.work.selection,
            self.originals,
            &mut self.work.original,
            &mut self.work.selection_limits,
        )
        .map_err(RootModuleExecutionErrorV1::Invocation)
    }
    fn execute_invocation_inner(
        &mut self,
        invocation_id: &str,
        action: Option<PreActionEnvelopeV1<'_>>,
        capture: Option<&RefCell<rule_capture::RuleOutputCaptureV1>>,
        live_owner: &mut dyn FnMut() -> ExecutionFence,
    ) -> Result<Option<ReservedValue<ModuleOutput>>, RootModuleExecutionErrorV1<O::Error>> {
        self.execute_routed_invocation_inner(invocation_id, action, capture, None, live_owner)
    }
    fn execute_routed_invocation_inner(
        &mut self,
        invocation_id: &str,
        action: Option<PreActionEnvelopeV1<'_>>,
        capture: Option<&RefCell<rule_capture::RuleOutputCaptureV1>>,
        route: Option<(
            oasis7_wasm_abi::ModuleSubscriptionStage,
            Option<&crate::world::incremental_module_execution_event::PersistedWorldEventV1<'_>>,
            Option<u64>,
        )>,
        live_owner: &mut dyn FnMut() -> ExecutionFence,
    ) -> Result<Option<ReservedValue<ModuleOutput>>, RootModuleExecutionErrorV1<O::Error>> {
        self.execute_input_invocation_inner(invocation_id, action, capture, route, None, live_owner)
    }
    fn execute_input_invocation_inner(
        &mut self,
        invocation_id: &str,
        action: Option<PreActionEnvelopeV1<'_>>,
        capture: Option<&RefCell<rule_capture::RuleOutputCaptureV1>>,
        route: Option<(
            oasis7_wasm_abi::ModuleSubscriptionStage,
            Option<&crate::world::incremental_module_execution_event::PersistedWorldEventV1<'_>>,
            Option<u64>,
        )>,
        product: Option<&EconomyInputV1<'_, '_>>,
        live_owner: &mut dyn FnMut() -> ExecutionFence,
    ) -> Result<Option<ReservedValue<ModuleOutput>>, RootModuleExecutionErrorV1<O::Error>> {
        self.execute_captured_input_invocation_inner(
            invocation_id,
            action,
            capture,
            route,
            product,
            None,
            live_owner,
        )
    }
}

fn application_admission(
    selected: &PreparedRootModuleInvocationV1,
    output: &ModuleOutput,
    trace: &str,
    work: &mut RootModuleExecutionWorkV1,
    cancelled: &mut impl FnMut() -> bool,
) -> Result<usize, ValueTreeError> {
    let mut fee = selected
        .manifest
        .reserved_bytes()
        .checked_add(selected.manifest.admitted_json_bytes_v1()?)
        .and_then(|n| n.checked_mul(16))
        .and_then(|n| n.checked_add(32 << 10))
        .ok_or(ValueTreeError::ResourceLimited(
            "root module output application",
        ))?;
    let mut add = |n: usize| -> Result<(), ValueTreeError> {
        fee = fee.checked_add(n).ok_or(ValueTreeError::ResourceLimited(
            "root module application heap",
        ))?;
        Ok(())
    };
    add(work
        .max_input_bytes
        .checked_mul(2)
        .ok_or(ValueTreeError::ResourceLimited("root module hook inputs"))?)?;
    add(work
        .max_prepared_effects
        .checked_mul(std::mem::size_of::<ReservedValue<PreparedRootModuleEffectV1>>())
        .ok_or(ValueTreeError::ResourceLimited("root module effect slots"))?)?;
    add(work
        .max_policy_selections
        .checked_mul(
            std::mem::size_of::<PreparedRootModuleInvocationV1>()
                + 2 * std::mem::size_of::<MemoryReservation>(),
        )
        .ok_or(ValueTreeError::ResourceLimited("root module hook slots"))?)?;
    add(trace
        .len()
        .checked_add(selected.module_id().len())
        .and_then(|n| n.checked_mul(32))
        .ok_or(ValueTreeError::ResourceLimited("root module trace copies"))?)?;
    if let Some(state) = &output.new_state {
        add(state
            .len()
            .checked_mul(4)
            .ok_or(ValueTreeError::ResourceLimited("root module state copies"))?)?;
    }
    let mut shape = EffectCloneWorkV1 {
        nodes: work.effects.clone_nodes_remaining,
        bytes: work.effects.clone_bytes_remaining,
        max_depth: work.effects.max_json_depth,
    };
    let result = (|| {
        for effect in &output.effects {
            add(
                json_clone_bound_v1(&effect.params, &mut shape, 0, cancelled)?
                    .checked_mul(16)
                    .ok_or(ValueTreeError::ResourceLimited("root module effect copies"))?,
            )?;
            add(effect
                .kind
                .len()
                .checked_add(effect.cap_ref.len())
                .and_then(|n| n.checked_add(effect.cap_slot.as_ref().map_or(0, String::len)))
                .and_then(|n| n.checked_mul(32))
                .ok_or(ValueTreeError::ResourceLimited(
                    "root module capability copies",
                ))?)?;
        }
        for emit in &output.emits {
            add(
                json_clone_bound_v1(&emit.payload, &mut shape, 0, cancelled)?
                    .checked_mul(16)
                    .ok_or(ValueTreeError::ResourceLimited("root module emit copies"))?,
            )?;
            add(emit
                .kind
                .len()
                .checked_mul(32)
                .ok_or(ValueTreeError::ResourceLimited("root module emit labels"))?)?;
        }
        Ok::<(), ValueTreeError>(())
    })();
    work.effects.clone_nodes_remaining = shape.nodes;
    work.effects.clone_bytes_remaining = shape.bytes;
    result?;
    if fee > work.transient_heap_bytes {
        return Err(ValueTreeError::ResourceLimited(
            "root module application admission",
        ));
    }
    Ok(fee)
}

#[path = "incremental_module_execution_target_due.rs"]
mod due;
pub use due::RootModuleTickOutcomeV1;

#[path = "incremental_module_execution_rule_phase.rs"]
mod rule_phase;
pub use rule_phase::{RootRulePhaseOutcomeV1, RootRulePhaseWorkV1};

#[cfg(test)]
#[path = "incremental_module_execution_envelope_tests.rs"]
mod envelope_tests;

#[path = "incremental_module_execution_override.rs"]
mod action_override;
pub(in crate::world) use action_override::PreparedRuleOverrideEventV1;
pub use action_override::RootRuleOverrideAuditV1;

#[path = "incremental_module_execution_scoped.rs"]
mod scoped;

#[path = "incremental_module_execution_tail.rs"]
mod event_tail;

#[path = "incremental_module_execution_core_driver.rs"]
mod core_driver;

#[path = "incremental_module_execution_body_driver.rs"]
mod body_driver;
#[path = "incremental_module_execution_normal_extension_driver.rs"]
mod normal_extension_driver;

#[path = "incremental_module_execution_selected_accepted.rs"]
mod selected_accepted;
pub(in crate::world) use selected_accepted::SelectedAcceptedDispositionV1;

#[path = "incremental_module_execution_ordinary_settlement.rs"]
mod ordinary_settlement;

#[path = "incremental_module_execution_collect_data_driver.rs"]
mod collect_data_driver;

#[path = "incremental_module_execution_transfer_resource_driver.rs"]
mod transfer_resource_driver;

#[path = "incremental_module_execution_selected_rule.rs"]
mod selected_rule;

#[path = "incremental_module_execution_nonmovement_driver.rs"]
mod nonmovement_driver;
pub(in crate::world) use nonmovement_driver::NonmovementSelectedErrorV1;
pub(in crate::world) use nonmovement_driver::{
    CompletedNonmovementActionV1, NonmovementSelectedWorkV1,
};

#[path = "incremental_module_execution_data_access_driver.rs"]
mod data_access_driver;

#[path = "incremental_module_execution_authenticated_data_driver.rs"]
mod authenticated_data_driver;

#[path = "incremental_module_execution_factory_lifecycle_driver.rs"]
mod factory_lifecycle_driver;
#[path = "incremental_module_execution_material_transfer_driver.rs"]
mod material_transfer_driver;

#[path = "incremental_module_execution_route_availability_driver.rs"]
mod route_availability_driver;

#[path = "incremental_module_execution_meta_progress_driver.rs"]
mod meta_progress_driver;

#[path = "incremental_module_execution_crisis_driver.rs"]
mod crisis_driver;

#[path = "incremental_module_execution_contract_accept_driver.rs"]
mod contract_accept_driver;

pub(in crate::world) use core_driver::CoreDriverErrorV1;

pub(in crate::world) use nonmovement_driver::{EffectiveActionFamilyV1, EffectiveActionWorkBankV1};

pub(in crate::world) use normal_extension_driver::{
    CompletedNormalExtensionV1, NormalCoreExtensionWorkV1, NormalCoreExtensionWorkViewV1,
};

#[path = "incremental_module_execution_logistics_registration_driver.rs"]
mod logistics_registration_driver;

#[path = "incremental_module_execution_alliance_driver.rs"]
mod alliance_driver;

#[path = "incremental_safe_alternative_driver.rs"]
mod matching_driver;

// Private result handoff for the fixed normal publisher and genuine fixtures.
pub(in crate::world) use matching_driver::MatchingBeforePublicationV1;

#[path = "incremental_module_execution_contract_lifecycle_driver.rs"]
mod contract_lifecycle_driver;

#[path = "incremental_normal_transits_due.rs"]
mod normal_transits_due;

#[path = "incremental_module_execution_factory_schedule_driver.rs"]
mod factory_schedule_driver;

#[path = "incremental_module_execution_due_product.rs"]
mod due_product;

pub(in crate::world) use due_product::PaidDueProductVmFailureV1;

pub(in crate::world) use due_product::with_current_root_module_meter_v1;

pub(in crate::world) use due_product::with_current_root_history_index_meter_v1;

#[path = "incremental_module_execution_factory_module_worker.rs"]
mod factory_module_worker;
pub(in crate::world) use factory_module_worker::PaidFactoryModuleExecutionV1;

#[path = "incremental_module_execution_profile_driver.rs"]
mod profile_driver;

#[path = "incremental_module_execution_factory_module_driver.rs"]
mod factory_module_driver;

pub(in crate::world) use factory_module_driver::{
    FactoryModuleBeforeRulesOutcomeV1, FailedFactoryModuleActionV1,
};

#[path = "incremental_module_execution_factory_module_rejection.rs"]
mod factory_module_rejection;
#[path = "incremental_module_execution_selected_factory_rule.rs"]
mod selected_factory_rule;

#[path = "incremental_module_execution_selected_with_modules.rs"]
mod selected_with_modules;
pub(in crate::world) use selected_with_modules::CompletedSelectedNonmovementV2;

#[path = "incremental_module_execution_input_invocation.rs"]
mod input_invocation;
#[path = "incremental_module_execution_staged_branch.rs"]
mod staged_branch;
pub(in crate::world) use staged_branch::StagedRootModuleDispositionV1;

pub(in crate::world) use factory_module_rejection::{
    CompletedFactoryModuleRejectionV1, FactoryModuleInitialAdmissionV1,
    PreparedFactoryModuleFollowupV1, prepare_factory_module_initial_admission_v1,
};

pub(in crate::world) use due_product::with_current_root_route_meter_v1;

#[path = "incremental_module_execution_module_lifecycle_driver.rs"]
mod module_lifecycle_driver;
