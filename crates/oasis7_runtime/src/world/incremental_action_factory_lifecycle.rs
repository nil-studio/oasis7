//! Selected MaintainFactory/RecycleFactory execution with genuine Current owners.
//! Common admission/Accepted/Rule fees/PostAction remain with the sole driver.
use crate::world::{
    World,
    incremental_action_execution::{SelectedActionAuditV1, SelectedPendingActionV1},
    incremental_body::BodyMutationError,
    incremental_body_event::{AgentEventOriginalV1, materials::CorePolicyMaterialContextV1},
    incremental_step::IwsStepContext,
};
use oasis7_formal_protocol::{
    incremental_execution::{ExecutionError, ExecutionFence, ExecutionTransaction, PreparedGuard},
    incremental_state::{MerkleNodeReader, StatePageWriter, StateScanBudget},
    incremental_value::{ValueNodeReader, ValueNodeWriter, ValueReadContext, ValueTreeError},
};
use oasis7_proto::memory_budget::{MemoryBudget, MemoryClass, MemoryReservation, ReservedValue};
use oasis7_world_state::{
    MaterialLedgerId, MaterialStack,
    incremental_agent_subtree::{
        AgentSubtreeStructureQualificationV1, CurrentAgentPresenceV1,
        read_current_agent_presence_v1,
    },
    incremental_factory_lifecycle_index::{
        FactoryLifecycleIndexV1, FactoryLifecycleIndexWorkV1, observe_current_factory_lifecycle_v1,
    },
    incremental_factory_records::*,
    incremental_field_member::{MemberRecordLimitsV1, MemberRecordWriteContext},
    incremental_field_overlay::{WorldFieldOverlayContextV1, WorldFieldOverlayErrorV1},
    incremental_field_read::WorldFieldReadContextV1,
    incremental_industry_jobs::{IndustryFamilyQualificationV1, IndustryWorkBudgetV1},
    incremental_inventory::{
        CurrentInventoryMaterialScopeV1, open_current_inventory_material_scope_v1,
    },
    incremental_inventory_change::InventoryChangeRefV1,
    incremental_inventory_read::LedgerQuantityV1,
    incremental_world_event_decode::WorldEventDecodeWorkV1,
    incremental_world_native::{WorldNativeFieldsQualificationV1, WorldNativeSelectedReadWorkV1},
    incremental_world_state_genesis::TypedRecordWriteWorkV1,
};
use std::cell::RefCell;
#[path = "incremental_action_factory_lifecycle_event.rs"]
mod event;
#[path = "incremental_action_factory_lifecycle_math.rs"]
pub(in crate::world) mod math;
#[path = "incremental_action_factory_lifecycle_read.rs"]
mod read;
/// No plain source DTO or boolean can replace these qualified parents.
pub(in crate::world) struct FactoryLifecycleSourcesV1<'a> {
    pub agents: &'a ReservedValue<AgentSubtreeStructureQualificationV1>,
    pub factories: &'a ReservedValue<FactoryFamilyQualificationV1>,
    pub industry: &'a ReservedValue<IndustryFamilyQualificationV1>,
    pub index: &'a ReservedValue<FactoryLifecycleIndexV1>,
    pub inventory: &'a ReservedValue<
        oasis7_world_state::incremental_inventory::InventoryFamilyQualificationV1,
    >,
    pub primitive: &'a ReservedValue<
        oasis7_world_state::incremental_owner_closure::WorldPrimitiveQualificationV1,
    >,
    pub native: &'a ReservedValue<WorldNativeFieldsQualificationV1>,
}
pub(in crate::world) struct FactoryLifecycleWorkV1 {
    pub steps_remaining: u64,
    pub bytes_remaining: u64,
    pub pending_entries_remaining: usize,
    pub max_scalar_bytes: usize,
    pub max_material_entries: usize,
    pub factory: FactoryWorkBudgetV1,
    pub industry: IndustryWorkBudgetV1,
    pub lifecycle: FactoryLifecycleIndexWorkV1,
    pub native_scan: StateScanBudget,
    pub native_read: WorldNativeSelectedReadWorkV1,
    pub native_write: TypedRecordWriteWorkV1,
    pub records_remaining: u64,
    pub output_group: oasis7_world_state::incremental_inventory::FactoryOutputLedgerGroupWorkV1,
}
impl FactoryLifecycleWorkV1 {
    fn spend(&mut self, n: usize) -> Result<(), BodyMutationError> {
        self.steps_remaining = self
            .steps_remaining
            .checked_sub(1)
            .ok_or(ValueTreeError::ResourceLimited("factory lifecycle steps"))?;
        self.bytes_remaining = self
            .bytes_remaining
            .checked_sub(
                u64::try_from(n)
                    .map_err(|_| ValueTreeError::ResourceLimited("factory lifecycle bytes"))?,
            )
            .ok_or(ValueTreeError::ResourceLimited("factory lifecycle bytes"))?;
        Ok(())
    }
}
fn overlay_error(error: WorldFieldOverlayErrorV1) -> BodyMutationError {
    match error {
        WorldFieldOverlayErrorV1::Execution(e) => e.into(),
        WorldFieldOverlayErrorV1::Value(e) => e.into(),
        WorldFieldOverlayErrorV1::Member(e) => BodyMutationError::Original(Box::new(e)),
    }
}

#[path = "incremental_action_factory_lifecycle_prepare.rs"]
mod prepare;
#[path = "incremental_action_factory_lifecycle_settle.rs"]
mod settle;
enum LifecycleDecisionV1<'a> {
    Rejected(crate::RejectReason),
    Effect(LifecycleIntentV1<'a>),
}
struct LifecycleIntentV1<'a> {
    operator: &'a str,
    factory: &'a str,
    effect: LifecycleEffectV1,
    identity: PreparedGuard,
    time: u64,
}
enum LifecycleEffectV1 {
    Maintained {
        ledger: ReservedValue<MaterialLedgerId>,
        consumed: i64,
        before_durability: i64,
        durability: i64,
    },
    Recycled {
        ledger: ReservedValue<MaterialLedgerId>,
        recovered: Vec<MaterialStack>,
        durability: i64,
        _fee: MemoryReservation,
    },
}
/// Constructed only after this module's actual Current publication succeeds.
pub(in crate::world) struct PreparedFactoryLifecycleEventV1 {
    event: ReservedValue<crate::WorldEvent>,
    action_id: u64,
    time: u64,
    touch_operator: bool,
    identity: PreparedGuard,
}
impl PreparedFactoryLifecycleEventV1 {
    pub(in crate::world) fn validate<M: MerkleNodeReader>(
        &self,
        tx: &ExecutionTransaction<'_, M>,
        memory: &MemoryBudget,
        now: u64,
    ) -> Result<(), BodyMutationError> {
        tx.validate_transaction_identity(&self.identity)?;
        if !self.event.uses_budget(memory)
            || !tx.uses_budget(memory)
            || self.time != now
            || self.event.time != now
            || self.event.caused_by != Some(crate::CausedBy::Action(self.action_id))
        {
            return Err(BodyMutationError::Invalid(
                "factory lifecycle prepared identity",
            ));
        }
        if !matches!(
            self.event.body,
            crate::WorldEventBody::Domain(
                crate::DomainEvent::FactoryMaintained { .. }
                    | crate::DomainEvent::FactoryRecycled { .. }
            )
        ) {
            return Err(BodyMutationError::Invalid(
                "factory lifecycle prepared event kind",
            ));
        }
        Ok(())
    }
    pub(in crate::world) fn event(&self) -> &crate::WorldEvent {
        &self.event
    }
}
pub(in crate::world) struct FactoryLifecycleRejectionReservationV1 {
    memory: MemoryBudget,
    fee: MemoryReservation,
}
pub(in crate::world) struct CompletedFactoryLifecycleV1 {
    owner: CompletedOwner,
    _fee: MemoryReservation,
}
enum CompletedOwner {
    Published {
        event: ReservedValue<crate::WorldEvent>,
        identity: PreparedGuard,
    },
    Rejected(SelectedActionAuditV1),
}
impl CompletedFactoryLifecycleV1 {
    pub(in crate::world) fn reserve_rejection(
        memory: &MemoryBudget,
    ) -> Result<FactoryLifecycleRejectionReservationV1, BodyMutationError> {
        Ok(FactoryLifecycleRejectionReservationV1 {
            memory: memory.clone(),
            fee: memory
                .reserve(std::mem::size_of::<Self>(), MemoryClass::Ordinary)
                .map_err(ValueTreeError::Memory)?,
        })
    }
    pub(in crate::world) fn from_rejection<M: MerkleNodeReader>(
        owner: SelectedActionAuditV1,
        selected: &SelectedPendingActionV1,
        tx: &ExecutionTransaction<'_, M>,
        memory: &MemoryBudget,
        prepaid: FactoryLifecycleRejectionReservationV1,
    ) -> Result<Self, BodyMutationError> {
        selected.validate(tx)?;
        owner.validate(tx)?;
        if !tx.uses_budget(memory)
            || !prepaid.memory.same_pool(memory)
            || owner.event().caused_by != Some(crate::CausedBy::Action(selected.envelope().id))
            || !matches!(&owner.event().body,crate::WorldEventBody::Domain(crate::DomainEvent::ActionRejected{action_id,..})if *action_id==selected.envelope().id)
        {
            return Err(BodyMutationError::Invalid(
                "factory rejection receipt binding",
            ));
        }
        Ok(Self {
            owner: CompletedOwner::Rejected(owner),
            _fee: prepaid.fee,
        })
    }

    pub(in crate::world) fn event(&self) -> &crate::WorldEvent {
        match &self.owner {
            CompletedOwner::Published { event, .. } => event,
            CompletedOwner::Rejected(owner) => owner.event(),
        }
    }
    pub(in crate::world) fn validate<M: MerkleNodeReader>(
        &self,
        tx: &ExecutionTransaction<'_, M>,
    ) -> Result<(), ExecutionError> {
        match &self.owner {
            CompletedOwner::Published { identity, .. } => {
                tx.validate_transaction_identity(identity)
            }
            CompletedOwner::Rejected(owner) => owner.validate(tx),
        }
    }
}

/// Execute only an actual FIFO selection and the same transaction's genuine
/// Rule outcome. No Accepted event is introduced for either lifecycle action.
#[allow(clippy::too_many_arguments)]
pub(in crate::world) fn execute_selected_factory_lifecycle_v1<M, P, V, W, E, C, PE, PC, ME, D, F>(
    step: &mut IwsStepContext<'_, '_, '_, '_, M, V, W, D>,
    materials: &mut CorePolicyMaterialContextV1<'_, M, ME>,
    selected: &SelectedPendingActionV1,
    outcome: &ReservedValue<crate::world::incremental_module_invocation::RootRulePhaseOutcomeV1>,
    sources: &FactoryLifecycleSourcesV1<'_>,
    read: &mut WorldFieldReadContextV1<'_, P, V, E, C>,
    parent_read: &mut WorldFieldReadContextV1<'_, P, V, PE, PC>,
    work: &mut FactoryLifecycleWorkV1,
    audit_work: &mut WorldEventDecodeWorkV1,
    max_event_bytes: usize,
    guard: &PreparedGuard,
    live: &ExecutionFence,
    mut persist: F,
) -> Result<CompletedFactoryLifecycleV1, BodyMutationError>
where
    M: MerkleNodeReader,
    P: MerkleNodeReader,
    V: ValueNodeReader,
    W: ValueNodeWriter + ValueNodeReader + StatePageWriter,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    PE: FnMut() -> Option<ValueTreeError>,
    PC: FnMut() -> bool,
    ME: FnMut() -> Option<ValueTreeError>,
    D: FnMut() -> bool,
    F: FnMut(&mut dyn AgentEventOriginalV1) -> Result<(), BodyMutationError>,
{
    let result = (|| {
        step.transaction.validate_guard(guard, live)?;
        selected.validate(step.transaction)?;
        let memory = step.mutation.memory;
        if !step.transaction.uses_budget(memory)
            || !outcome.uses_budget(memory)
            || read.values.class != MemoryClass::Ordinary
            || parent_read.values.class != MemoryClass::Ordinary
            || !read.values.memory.same_pool(memory)
            || !parent_read.values.memory.same_pool(memory)
            || !sources.agents.uses_budget(memory)
            || sources.agents.allocation_class() != MemoryClass::Ordinary
            || !sources.factories.uses_budget(memory)
            || sources.factories.allocation_class() != MemoryClass::Ordinary
            || !sources.industry.uses_budget(memory)
            || sources.industry.allocation_class() != MemoryClass::Ordinary
            || !sources.index.uses_budget(memory)
            || sources.index.allocation_class() != MemoryClass::Ordinary
            || !sources.inventory.uses_budget(memory)
            || sources.inventory.allocation_class() != MemoryClass::Ordinary
            || !sources.primitive.uses_budget(memory)
            || sources.primitive.allocation_class() != MemoryClass::Ordinary
            || !sources.native.uses_budget(memory)
        {
            return Err(BodyMutationError::Invalid(
                "factory lifecycle source pool/class",
            ));
        }
        read.check()?;
        parent_read.check()?;
        let _binding_fee = memory
            .reserve(32 << 10, MemoryClass::Ordinary)
            .map_err(ValueTreeError::Memory)?;
        let digest = materials.roots.digest()?;
        if digest != step.transaction.parent_fence().parent_roots {
            return Err(BodyMutationError::Invalid("factory lifecycle roots"));
        }
        sources.factories.validate_binding(&digest)?;
        sources.industry.validate_binding(&digest)?;
        sources.inventory.validate_binding(materials.roots)?;
        sources.native.validate_binding(materials.roots)?;
        sources
            .primitive
            .validate_binding(materials.roots)
            .map_err(|e| BodyMutationError::Original(Box::new(e)))?;
        let replacement = outcome.override_action_for_selected_v1(selected, step.transaction)?;
        if outcome.decision.verdict == crate::RuleVerdict::Deny {
            return Err(BodyMutationError::Invalid(
                "factory lifecycle denied Rule outcome",
            ));
        }
        let action = replacement.unwrap_or(&selected.envelope().action);
        let text = match action {
            crate::Action::MaintainFactory {
                operator_agent_id,
                factory_id,
                ..
            }
            | crate::Action::RecycleFactory {
                operator_agent_id,
                factory_id,
            } => operator_agent_id
                .len()
                .checked_add(factory_id.len())
                .ok_or(ValueTreeError::ResourceLimited(
                    "factory action identifiers",
                ))?,
            _ => {
                return Err(BodyMutationError::Invalid(
                    "factory lifecycle effective variant",
                ));
            }
        };
        work.spend(text)?;
        let phase = text
            .checked_add(work.max_scalar_bytes)
            .and_then(|n| n.checked_mul(8))
            .and_then(|n| n.checked_add(32 << 10))
            .ok_or(ValueTreeError::ResourceLimited("factory action workspace"))?;
        let _phase = memory
            .reserve(phase, MemoryClass::Ordinary)
            .map_err(ValueTreeError::Memory)?;
        let decision = prepare::prepare(
            action,
            step,
            sources,
            materials.roots,
            read,
            parent_read,
            work,
            live,
        )?;
        let intent = match decision {
            LifecycleDecisionV1::Rejected(reason) => {
                let prepaid = CompletedFactoryLifecycleV1::reserve_rejection(memory)?;
                let audit = step.append_selected_rejection_owned(
                    materials,
                    live,
                    selected,
                    reason,
                    max_event_bytes,
                    audit_work,
                    &mut persist,
                )?;
                return CompletedFactoryLifecycleV1::from_rejection(
                    audit,
                    selected,
                    step.transaction,
                    memory,
                    prepaid,
                );
            }
            LifecycleDecisionV1::Effect(intent) => intent,
        };
        // The completion carrier is paid before state/original publication.
        let completed_fee = memory
            .reserve(
                std::mem::size_of::<CompletedFactoryLifecycleV1>(),
                MemoryClass::Ordinary,
            )
            .map_err(ValueTreeError::Memory)?;
        let touch = settle::settle(&intent, step, materials, sources, read, work, live)?;
        let _event_phase = memory
            .reserve(settle::event_clone_bytes(&intent)?, MemoryClass::Ordinary)
            .map_err(ValueTreeError::Memory)?;
        let sequence =
            step.read_allocator(crate::world::incremental_allocators::RuntimeAllocatorV1::Event)?;
        let (id, _, _) = World::preview_next_event_id(sequence.next, sequence.era);
        let body = match &intent.effect {
            LifecycleEffectV1::Maintained {
                ledger,
                consumed,
                durability,
                ..
            } => crate::DomainEvent::FactoryMaintained {
                operator_agent_id: intent.operator.to_owned(),
                factory_id: intent.factory.to_owned(),
                consume_ledger: (**ledger).clone(),
                consumed_parts: *consumed,
                durability_ppm: *durability,
            },
            LifecycleEffectV1::Recycled {
                ledger,
                recovered,
                durability,
                ..
            } => crate::DomainEvent::FactoryRecycled {
                operator_agent_id: intent.operator.to_owned(),
                factory_id: intent.factory.to_owned(),
                recycle_ledger: (**ledger).clone(),
                recovered: recovered.clone(),
                durability_ppm: *durability,
            },
        };
        let event = crate::WorldEvent {
            id,
            time: step.time(),
            caused_by: Some(crate::CausedBy::Action(selected.envelope().id)),
            body: crate::WorldEventBody::Domain(body),
        };
        let event = oasis7_world_state::incremental_world_event_owned::retain_world_event_owned_v1(
            event,
            memory,
            MemoryClass::Ordinary,
            audit_work,
            &mut step.mutation.cancelled,
        )?;
        let prepared = PreparedFactoryLifecycleEventV1 {
            event,
            action_id: selected.envelope().id,
            time: step.time(),
            touch_operator: touch,
            identity: step.transaction.guard(),
        };
        prepared.validate(step.transaction, memory, step.time())?;
        let before = step.begin_world_event()?;
        event::sink(step, &prepared, work, max_event_bytes, live, &mut persist)?;
        step.finish_world_event(before)?;
        if (step.mutation.cancelled)() || (read.cancelled)() || (parent_read.cancelled)() {
            return Err(ValueTreeError::Cancelled.into());
        }
        Ok(CompletedFactoryLifecycleV1 {
            owner: CompletedOwner::Published {
                event: prepared.event,
                identity: step.transaction.guard(),
            },
            _fee: completed_fee,
        })
    })();
    if result.is_err() {
        let _: Result<(), ExecutionError> = step
            .transaction
            .contribute(|_| Err(ExecutionError::Aborted));
    }
    result
}
