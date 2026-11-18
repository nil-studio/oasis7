//! TransferMainToken from a genuine FIFO selection and its actual Rule result.
//! The shared WorldState reducer consumes only the three authenticated point
//! reads. No reconstructed World or DATA delta can grant publication authority.
use crate::world::{
    World,
    incremental_action_execution::{SelectedActionAuditV1, SelectedPendingActionV1},
    incremental_body::BodyMutationError,
    incremental_body_event::{AgentEventOriginalV1, materials::CorePolicyMaterialContextV1},
    incremental_module_invocation::RootRulePhaseOutcomeV1,
    incremental_step::IwsStepContext,
};
use oasis7_formal_protocol::{
    controlled_authority::formal::bounded_context::{FormalBoundedWorkV1, WorkIdentityV1},
    incremental_execution::{
        ExecutionError, ExecutionFence, ExecutionTransaction, PreparedGuard, RootSetV1,
    },
    incremental_state::MerkleNodeReader,
    incremental_value::{ValueNodeReader, ValueNodeWriter, ValueReadContext, ValueTreeError},
};
use oasis7_proto::memory_budget::{MemoryBudget, MemoryClass, MemoryReservation, ReservedValue};
use oasis7_world_state::{
    incremental_agent_subtree::{
        AgentSubtreeStructureQualificationV1, CurrentAgentPresenceV1,
        read_current_agent_presence_v1,
    },
    incremental_field_member::MemberRecordWriteContext,
    incremental_field_overlay::{WorldFieldOverlayContextV1, WorldFieldOverlayErrorV1},
    incremental_field_read::WorldFieldReadContextV1,
    incremental_owner_closure::WorldPrimitiveQualificationV1,
    incremental_world_balances::WorldBalanceReadV1,
    incremental_world_event_decode::WorldEventDecodeWorkV1,
    world::main_token_monetary_publication::{
        MainTokenTransferDeltaV1, MainTokenTransferErrorV1, prepare_main_token_transfer_delta_v1,
    },
};
use std::mem::size_of;

#[path = "incremental_action_main_token_transfer_event.rs"]
mod event;
#[path = "incremental_action_main_token_transfer_read.rs"]
mod read;

pub(in crate::world) struct MainTokenTransferWorkV1 {
    pub steps_remaining: u64,
    pub bytes_remaining: u64,
    pub pending_entries_remaining: usize,
    pub max_identity_bytes: u64,
    pub core: WorkIdentityV1,
}
impl MainTokenTransferWorkV1 {
    fn spend(
        &mut self,
        common: &FormalBoundedWorkV1,
        bytes: usize,
        stop: &mut dyn FnMut() -> bool,
    ) -> Result<(), BodyMutationError> {
        if self.core != common.identity() {
            return Err(BodyMutationError::Invalid(
                "main token transfer Common identity",
            ));
        }
        let bytes = u64::try_from(bytes).map_err(|_| limited())?;
        common
            .charge(1, bytes, stop)
            .map_err(|e| BodyMutationError::Original(Box::new(e)))?;
        self.steps_remaining = self.steps_remaining.checked_sub(1).ok_or_else(limited)?;
        self.bytes_remaining = self
            .bytes_remaining
            .checked_sub(bytes)
            .ok_or_else(limited)?;
        Ok(())
    }
}
fn limited() -> ValueTreeError {
    ValueTreeError::ResourceLimited("main token transfer work")
}
fn overlay_error(error: WorldFieldOverlayErrorV1) -> BodyMutationError {
    match error {
        WorldFieldOverlayErrorV1::Execution(e) => e.into(),
        WorldFieldOverlayErrorV1::Value(e) => e.into(),
        WorldFieldOverlayErrorV1::Member(e) => BodyMutationError::Original(Box::new(e)),
    }
}

/// Constructed only after the genuine owner has installed its expected-value
/// account/nonce delta in the selected transaction. This is not a public DATA
/// constructor or a substitute for the later complete tick/accepted permit.
pub(in crate::world) struct PreparedMainTokenTransferEventV1 {
    event: ReservedValue<crate::WorldEvent>,
    selected_id: u64,
    identity: PreparedGuard,
}
impl PreparedMainTokenTransferEventV1 {
    pub(in crate::world) fn event(&self) -> &crate::WorldEvent {
        &self.event
    }
    pub(in crate::world) fn validate<M: MerkleNodeReader>(
        &self,
        tx: &ExecutionTransaction<'_, M>,
        memory: &MemoryBudget,
        now: u64,
        live: &ExecutionFence,
    ) -> Result<(), BodyMutationError> {
        tx.validate_guard(&self.identity, live)?;
        if !tx.uses_budget(memory)
            || !self.event.uses_budget(memory)
            || self.event.time != now
            || self.event.caused_by != Some(crate::CausedBy::Action(self.selected_id))
            || !matches!(
                self.event.body,
                crate::WorldEventBody::Domain(crate::DomainEvent::MainTokenTransferred { .. })
            )
        {
            return Err(BodyMutationError::Invalid(
                "main token transfer event binding",
            ));
        }
        Ok(())
    }
}
pub(in crate::world) struct CompletedMainTokenTransferV1 {
    owner: CompletedOwner,
    _fee: MemoryReservation,
}
enum CompletedOwner {
    Rejection(SelectedActionAuditV1),
    Transfer {
        event: ReservedValue<crate::WorldEvent>,
        identity: PreparedGuard,
    },
}
impl CompletedMainTokenTransferV1 {
    pub(in crate::world) fn from_rejection<M: MerkleNodeReader>(
        owner: SelectedActionAuditV1,
        selected: &SelectedPendingActionV1,
        tx: &ExecutionTransaction<'_, M>,
        memory: &MemoryBudget,
    ) -> Result<Self, BodyMutationError> {
        selected.validate(tx)?;
        owner.validate(tx)?;
        if !tx.uses_budget(memory)
            || owner.event().caused_by != Some(crate::CausedBy::Action(selected.envelope().id))
            || !matches!(&owner.event().body, crate::WorldEventBody::Domain(crate::DomainEvent::ActionRejected { action_id, .. }) if *action_id == selected.envelope().id)
        {
            return Err(BodyMutationError::Invalid(
                "main token transfer rejection binding",
            ));
        }
        let fee = memory
            .reserve(size_of::<Self>(), MemoryClass::Ordinary)
            .map_err(ValueTreeError::Memory)?;
        Ok(Self {
            owner: CompletedOwner::Rejection(owner),
            _fee: fee,
        })
    }
    pub(in crate::world) fn event(&self) -> &crate::WorldEvent {
        match &self.owner {
            CompletedOwner::Rejection(v) => v.event(),
            CompletedOwner::Transfer { event, .. } => event,
        }
    }
    pub(in crate::world) fn validate<M: MerkleNodeReader>(
        &self,
        tx: &ExecutionTransaction<'_, M>,
    ) -> Result<(), ExecutionError> {
        match &self.owner {
            CompletedOwner::Rejection(v) => v.validate(tx),
            CompletedOwner::Transfer { identity, .. } => tx.validate_transaction_identity(identity),
        }
    }
}

#[allow(clippy::too_many_arguments)]
pub(in crate::world) fn execute_selected_main_token_transfer_v1<
    M,
    P,
    V,
    W,
    E,
    C,
    PE,
    PC,
    D,
    CW,
    F,
>(
    step: &mut IwsStepContext<'_, '_, '_, '_, M, V, W, D>,
    materials: &mut CorePolicyMaterialContextV1<'_, M, impl FnMut() -> Option<ValueTreeError>>,
    selected: &SelectedPendingActionV1,
    outcome: &ReservedValue<RootRulePhaseOutcomeV1>,
    primitive: &ReservedValue<WorldPrimitiveQualificationV1>,
    agents: &ReservedValue<AgentSubtreeStructureQualificationV1>,
    roots: &RootSetV1,
    current: &mut WorldFieldReadContextV1<'_, P, V, E, C>,
    parent_read: &mut WorldFieldReadContextV1<'_, P, V, PE, PC>,
    members: &mut MemberRecordWriteContext<'_, W, CW>,
    work: &mut MainTokenTransferWorkV1,
    common: &FormalBoundedWorkV1,
    audit: &mut WorldEventDecodeWorkV1,
    max_event_bytes: usize,
    live: &ExecutionFence,
    persist: &mut F,
) -> Result<CompletedMainTokenTransferV1, BodyMutationError>
where
    M: MerkleNodeReader,
    P: MerkleNodeReader,
    V: ValueNodeReader,
    W: ValueNodeWriter,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    D: FnMut() -> bool,
    PE: FnMut() -> Option<ValueTreeError>,
    PC: FnMut() -> bool,
    CW: FnMut() -> bool,
    F: FnMut(&mut dyn AgentEventOriginalV1) -> Result<(), BodyMutationError>,
{
    let attempt = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let guard = step.transaction.guard();
        step.transaction.validate_guard(&guard, live)?;
        selected.validate(step.transaction)?;
        primitive
            .validate_binding(roots)
            .map_err(|e| BodyMutationError::Original(Box::new(e)))?;
        if !primitive.uses_budget(step.mutation.memory)
            || primitive.allocation_class() != MemoryClass::Ordinary
            || !agents.uses_budget(step.mutation.memory)
            || agents.allocation_class() != MemoryClass::Ordinary
            || !outcome.uses_budget(step.mutation.memory)
            || !current.values.memory.same_pool(step.mutation.memory)
            || current.values.class != MemoryClass::Ordinary
            || !members.memory.same_pool(step.mutation.memory)
            || outcome.decision.verdict == crate::RuleVerdict::Deny
        {
            return Err(BodyMutationError::Invalid(
                "main token transfer sources/pool/class",
            ));
        }
        agents.validate_binding(roots)?;
        if parent_read.values.class != MemoryClass::Ordinary
            || !parent_read.values.memory.same_pool(step.mutation.memory)
        {
            return Err(BodyMutationError::Invalid(
                "main token transfer parent pool/class",
            ));
        }
        current.check()?;
        parent_read.check()?;
        let effective = outcome
            .override_action_for_selected_v1(selected, step.transaction)?
            .unwrap_or(&selected.envelope().action);
        let crate::Action::TransferMainToken {
            from_account_id,
            to_account_id,
            amount,
            nonce,
            asset_id,
            memo,
            ..
        } = effective
        else {
            return Err(BodyMutationError::Invalid(
                "main token transfer effective action",
            ));
        };
        let from = from_account_id.trim();
        let to = to_account_id.trim();
        let asset = asset_id.as_deref().map(str::trim).filter(|v| !v.is_empty());
        let memo = memo.as_deref().map(str::trim).filter(|v| !v.is_empty());
        let text = from
            .len()
            .checked_add(to.len())
            .and_then(|n| n.checked_add(asset.map_or(0, str::len)))
            .and_then(|n| n.checked_add(memo.map_or(0, str::len)))
            .ok_or_else(limited)?;
        let max_identity = usize::try_from(work.max_identity_bytes).map_err(|_| limited())?;
        if from.len() > max_identity || to.len() > max_identity || text > max_event_bytes {
            return Err(limited().into());
        }
        // Two bounded account buffers, their retained deltas, the original event,
        // and a closed error rendering all remain charged before any allocation.
        let bytes = max_identity
            .checked_mul(6)
            .and_then(|n| n.checked_add(text.checked_mul(3)?))
            .and_then(|n| {
                n.checked_add(
                    size_of::<MainTokenTransferDeltaV1>() + size_of::<SelfLayout>() + 2048,
                )
            })
            .ok_or_else(limited)?;
        work.spend(common, bytes, &mut || {
            (current.cancelled)() || (step.mutation.cancelled)()
        })?;
        let _data = step
            .mutation
            .memory
            .reserve(bytes, MemoryClass::Ordinary)
            .map_err(ValueTreeError::Memory)?;
        let owner_fee = step
            .mutation
            .memory
            .reserve(
                size_of::<CompletedMainTokenTransferV1>(),
                MemoryClass::Ordinary,
            )
            .map_err(ValueTreeError::Memory)?;
        if let Some(reason) = World::main_token_transfer_input_reason_v1(from, to, *amount, *nonce)
        {
            return reject(
                step,
                materials,
                selected,
                crate::RejectReason::RuleDenied {
                    notes: vec![reason.to_owned()],
                },
                audit,
                max_event_bytes,
                live,
                persist,
            );
        }
        let (plan, before, before_nonce) = {
            let mut overlay = WorldFieldOverlayContextV1 {
                transaction: step.transaction,
                values: ValueReadContext {
                    source: current.values.source,
                    memory: current.values.memory,
                    class: current.values.class,
                    io: &mut *current.values.io,
                },
                key_work: &mut *current.key_work,
                semantic_bytes: current.semantic_bytes,
                record_bytes: current.record_bytes,
                take_state_error: &mut current.take_state_error,
                cancelled: || (current.cancelled)() || (step.mutation.cancelled)(),
            };
            let mut reader = read::CurrentTransferRead::new(&mut overlay, work, common);
            let plan = prepare_main_token_transfer_delta_v1(from, to, *amount, *nonce, &mut reader);
            (plan, reader.before, reader.before_nonce)
        };
        let delta = match plan {
            Ok(v) => v,
            Err(MainTokenTransferErrorV1::Read(e)) => return Err(e),
            Err(MainTokenTransferErrorV1::Business(e)) => {
                return reject(
                    step,
                    materials,
                    selected,
                    crate::RejectReason::RuleDenied {
                        notes: vec![format!("main token transfer rejected: {e:?}")],
                    },
                    audit,
                    max_event_bytes,
                    live,
                    persist,
                );
            }
        };
        let sender_present = {
            let mut overlay = WorldFieldOverlayContextV1 {
                transaction: step.transaction,
                values: ValueReadContext {
                    source: current.values.source,
                    memory: current.values.memory,
                    class: current.values.class,
                    io: &mut *current.values.io,
                },
                key_work: &mut *current.key_work,
                semantic_bytes: current.semantic_bytes,
                record_bytes: current.record_bytes,
                take_state_error: &mut current.take_state_error,
                cancelled: || (current.cancelled)() || (step.mutation.cancelled)(),
            };
            let guard = overlay.transaction.guard();
            read_current_agent_presence_v1(
                from,
                agents,
                roots,
                &mut overlay,
                parent_read,
                &mut work.pending_entries_remaining,
                &guard,
                live,
            )
            .map_err(overlay_error)?
                == CurrentAgentPresenceV1::Existing
        };
        event::install_delta(
            step,
            materials,
            current,
            members,
            &delta,
            before,
            before_nonce,
            work,
            common,
            live,
        )?;
        let sequence = World::read_iws_allocator(
            crate::world::incremental_allocators::RuntimeAllocatorV1::Event,
            step.transaction,
            step.values,
            step.mutation,
        )?;
        let (id, _, _) = World::preview_next_event_id(sequence.next, sequence.era);
        let event_fee = step
            .mutation
            .memory
            .reserve(
                size_of::<crate::WorldEvent>()
                    .checked_add(text)
                    .ok_or_else(limited)?,
                MemoryClass::Ordinary,
            )
            .map_err(ValueTreeError::Memory)?;
        let event = event_fee.retain(crate::WorldEvent {
            id,
            time: step.time(),
            caused_by: Some(crate::CausedBy::Action(selected.envelope().id)),
            body: crate::WorldEventBody::Domain(crate::DomainEvent::MainTokenTransferred {
                from_account_id: from.to_owned(),
                to_account_id: to.to_owned(),
                amount: *amount,
                nonce: *nonce,
                asset_id: asset.map(ToOwned::to_owned),
                memo: memo.map(ToOwned::to_owned),
            }),
        });
        let prepared = PreparedMainTokenTransferEventV1 {
            event,
            selected_id: selected.envelope().id,
            identity: step.transaction.guard(),
        };
        let count_before = step.begin_world_event()?;
        event::publish(
            step,
            &prepared,
            sender_present,
            work,
            common,
            max_event_bytes,
            live,
            persist,
        )?;
        step.finish_world_event(count_before)?;
        if (current.cancelled)() || (step.mutation.cancelled)() || (members.cancelled)() {
            return Err(ValueTreeError::Cancelled.into());
        }
        Ok(CompletedMainTokenTransferV1 {
            owner: CompletedOwner::Transfer {
                event: prepared.event,
                identity: step.transaction.guard(),
            },
            _fee: owner_fee,
        })
    }));
    match attempt {
        Ok(result) => {
            if result.is_err() {
                let _: Result<(), ExecutionError> = step
                    .transaction
                    .contribute(|_| Err(ExecutionError::Aborted));
            }
            result
        }
        Err(payload) => {
            let _: Result<(), ExecutionError> = step
                .transaction
                .contribute(|_| Err(ExecutionError::Aborted));
            std::panic::resume_unwind(payload)
        }
    }
}
// Fixed phase layout covered before the bounded DATA reader is constructed.
type SelfLayout = (
    PreparedMainTokenTransferEventV1,
    [Option<WorldBalanceReadV1>; 2],
    MainTokenTransferDeltaV1,
);

#[allow(clippy::too_many_arguments)]
fn reject<M, V, W, D, F>(
    step: &mut IwsStepContext<'_, '_, '_, '_, M, V, W, D>,
    materials: &mut CorePolicyMaterialContextV1<'_, M, impl FnMut() -> Option<ValueTreeError>>,
    selected: &SelectedPendingActionV1,
    reason: crate::RejectReason,
    audit: &mut WorldEventDecodeWorkV1,
    max_event_bytes: usize,
    live: &ExecutionFence,
    persist: &mut F,
) -> Result<CompletedMainTokenTransferV1, BodyMutationError>
where
    M: MerkleNodeReader,
    V: ValueNodeReader,
    W: ValueNodeWriter,
    D: FnMut() -> bool,
    F: FnMut(&mut dyn AgentEventOriginalV1) -> Result<(), BodyMutationError>,
{
    let audit = step.append_selected_rejection_owned(
        materials,
        live,
        selected,
        reason,
        max_event_bytes,
        audit,
        persist,
    )?;
    CompletedMainTokenTransferV1::from_rejection(
        audit,
        selected,
        step.transaction,
        step.mutation.memory,
    )
}
