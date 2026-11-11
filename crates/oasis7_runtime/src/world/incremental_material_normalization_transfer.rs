//! Selected, direct Agent-ledger TransferMaterial execution.
//!
//! This deliberately covers only the legacy immediate lane: the requester is
//! the source Agent ledger, the destination is an existing Agent ledger, and
//! the action has no route or transit distance. Other source kinds and routed
//! transfers fail closed until their own Current owners exist.
use crate::world::{
    World,
    incremental_action_execution::{SelectedActionAuditV1, SelectedPendingActionV1},
    incremental_body::BodyMutationError,
    incremental_body_event::{AgentEventOriginalV1, materials::CorePolicyMaterialContextV1},
    incremental_step::IwsStepContext,
};
use oasis7_formal_protocol::{
    incremental_execution::{
        ExecutionError, ExecutionFence, ExecutionTransaction, PreparedGuard, RootSetV1,
    },
    incremental_state::{MerkleNodeReader, StatePageWriter},
    incremental_value::{ValueNodeReader, ValueNodeWriter, ValueReadContext, ValueTreeError},
};
use oasis7_proto::memory_budget::{MemoryClass, MemoryReservation, ReservedValue};
use oasis7_wasm_abi::MaterialDefaultPriority;
use oasis7_world_state::{
    MaterialLedgerId, MaterialTransferReceiptV1, MaterialTransitPriority,
    incremental_agent_subtree::{
        AgentSubtreeStructureQualificationV1, CurrentAgentPresenceV1,
        read_current_agent_presence_v1,
    },
    incremental_field_member::MemberRecordWriteContext,
    incremental_field_overlay::{WorldFieldOverlayContextV1, WorldFieldOverlayErrorV1},
    incremental_field_read::WorldFieldReadContextV1,
    incremental_inventory::{
        AgentLedgerTransferOutcomeV1, AgentLedgerTransferWorkV1, CurrentInventoryMaterialScopeV1,
        open_current_inventory_material_scope_v1, transfer_current_agent_material_overlay_v1,
    },
    incremental_material_records::{
        MaterialFamilyQualificationV1, MaterialProfileCurrentReadWorkV1, MaterialRecordCreateV1,
        MaterialRecordKeyV1, MaterialRecordQualificationV1, MaterialRecordRefV1,
        MaterialWorkBudgetV1, create_qualified_material_record_v1,
        read_current_material_default_priority_overlay_v1,
    },
    incremental_world_event_decode::WorldEventDecodeWorkV1,
};
use std::{cell::RefCell, fmt::Write as _};

#[path = "incremental_material_normalization_transfer_event.rs"]
mod event;

/// Continuous work shared by the action validation, Current profile, real
/// Agent-ledger point owner and typed receipt producer. No budget is recreated
/// at an event or record boundary.
pub(in crate::world) struct MaterialTransferWorkV1 {
    pub steps_remaining: u64,
    pub bytes_remaining: u64,
    pub profile: MaterialProfileCurrentReadWorkV1,
    pub inventory: AgentLedgerTransferWorkV1,
    pub receipt: MaterialWorkBudgetV1,
}
impl MaterialTransferWorkV1 {
    fn spend(&mut self, bytes: usize) -> Result<(), BodyMutationError> {
        self.steps_remaining = self
            .steps_remaining
            .checked_sub(1)
            .ok_or(ValueTreeError::ResourceLimited("material transfer steps"))?;
        self.bytes_remaining = self
            .bytes_remaining
            .checked_sub(
                u64::try_from(bytes)
                    .map_err(|_| ValueTreeError::ResourceLimited("material transfer byte bound"))?,
            )
            .ok_or(ValueTreeError::ResourceLimited("material transfer bytes"))?;
        Ok(())
    }
}

/// The transfer producer alone creates this owner after the real Rule outcome,
/// Current balance updates and typed receipt have succeeded.
pub(in crate::world) struct PreparedMaterialTransferEventV1 {
    event: ReservedValue<crate::WorldEvent>,
    receipt: ReservedValue<MaterialRecordQualificationV1>,
    action_id: u64,
    now: u64,
    identity: PreparedGuard,
}
impl PreparedMaterialTransferEventV1 {
    fn check_binding(
        &self,
        memory: &oasis7_proto::memory_budget::MemoryBudget,
        now: u64,
    ) -> Result<(), BodyMutationError> {
        if self.now != now
            || !self.event.uses_budget(memory)
            || !self.receipt.uses_budget(memory)
            || self.receipt.allocation_class() != MemoryClass::Ordinary
        {
            return Err(BodyMutationError::Invalid(
                "prepared MaterialTransfer pool/time",
            ));
        }
        let crate::WorldEvent {
            id: _,
            time,
            caused_by,
            body,
        } = &*self.event;
        let crate::WorldEventBody::Domain(crate::DomainEvent::MaterialTransferred {
            transfer_id,
            requester_agent_id,
            from_ledger,
            to_ledger,
            kind,
            amount,
            distance_km,
            route_id,
            ..
        }) = body
        else {
            return Err(BodyMutationError::Invalid(
                "prepared MaterialTransfer event kind",
            ));
        };
        if *time != now
            || *caused_by != Some(crate::CausedBy::Action(self.action_id))
            || *transfer_id != Some(self.action_id)
            || *amount <= 0
            || *distance_km != 0
            || route_id.is_some()
            || !matches!(from_ledger, MaterialLedgerId::Agent(id) if id == requester_agent_id)
            || !matches!(to_ledger, MaterialLedgerId::Agent(id) if id != requester_agent_id)
            || kind.trim().is_empty()
        {
            return Err(BodyMutationError::Invalid(
                "prepared MaterialTransfer binding",
            ));
        }
        Ok(())
    }

    pub(in crate::world) fn validate<M: MerkleNodeReader>(
        &self,
        tx: &ExecutionTransaction<'_, M>,
        memory: &oasis7_proto::memory_budget::MemoryBudget,
        now: u64,
    ) -> Result<(), BodyMutationError> {
        tx.validate_transaction_identity(&self.identity)?;
        self.check_binding(memory, now)
    }

    pub(in crate::world) fn event(&self) -> &crate::WorldEvent {
        &self.event
    }
}

/// This applied witness is minted only after last-active and the requester's
/// mailbox have been staged. Its fresh guard is the seam consumed by the
/// specialized original/fact bridge.
pub(in crate::world) struct AppliedMaterialTransferEventV1<'event> {
    prepared: &'event PreparedMaterialTransferEventV1,
    identity: PreparedGuard,
}
impl AppliedMaterialTransferEventV1<'_> {
    pub(in crate::world) fn validate<M: MerkleNodeReader>(
        &self,
        tx: &ExecutionTransaction<'_, M>,
        memory: &oasis7_proto::memory_budget::MemoryBudget,
        now: u64,
    ) -> Result<(), BodyMutationError> {
        tx.validate_transaction_identity(&self.identity)?;
        self.prepared.check_binding(memory, now)
    }

    pub(in crate::world) fn event(&self) -> &crate::WorldEvent {
        self.prepared.event()
    }
}

/// Genuine result of the selected reducer and all specialized sinks. The
/// private event owner is required before PostAction can consume the result.
pub(in crate::world) struct CompletedMaterialTransferV1 {
    owner: CompletedMaterialTransferOwnerV1,
    _fee: MemoryReservation,
}
enum CompletedMaterialTransferOwnerV1 {
    Rejection(SelectedActionAuditV1),
    Transferred {
        event: ReservedValue<crate::WorldEvent>,
        identity: PreparedGuard,
    },
}
impl CompletedMaterialTransferV1 {
    pub(in crate::world) fn from_rejection<M: MerkleNodeReader>(
        owner: SelectedActionAuditV1,
        selected: &SelectedPendingActionV1,
        tx: &ExecutionTransaction<'_, M>,
        memory: &oasis7_proto::memory_budget::MemoryBudget,
    ) -> Result<Self, BodyMutationError> {
        selected.validate(tx)?;
        owner.validate(tx)?;
        if !tx.uses_budget(memory)
            || owner.event().caused_by != Some(crate::CausedBy::Action(selected.envelope().id))
            || !matches!(&owner.event().body,crate::WorldEventBody::Domain(crate::DomainEvent::ActionRejected {action_id,..}) if *action_id==selected.envelope().id)
        {
            return Err(BodyMutationError::Invalid(
                "material transfer rejection receipt binding",
            ));
        }
        let fee = memory
            .reserve(std::mem::size_of::<Self>(), MemoryClass::Ordinary)
            .map_err(ExecutionError::Memory)?;
        Ok(Self {
            owner: CompletedMaterialTransferOwnerV1::Rejection(owner),
            _fee: fee,
        })
    }
    pub(in crate::world) fn event(&self) -> &crate::WorldEvent {
        match &self.owner {
            CompletedMaterialTransferOwnerV1::Rejection(owner) => owner.event(),
            CompletedMaterialTransferOwnerV1::Transferred { event, .. } => event,
        }
    }

    pub(in crate::world) fn validate<M: MerkleNodeReader>(
        &self,
        tx: &ExecutionTransaction<'_, M>,
    ) -> Result<(), ExecutionError> {
        match &self.owner {
            CompletedMaterialTransferOwnerV1::Rejection(owner) => owner.validate(tx),
            CompletedMaterialTransferOwnerV1::Transferred { identity, .. } => {
                tx.validate_transaction_identity(identity)
            }
        }
    }
}

fn overlay_error(error: WorldFieldOverlayErrorV1) -> BodyMutationError {
    match error {
        WorldFieldOverlayErrorV1::Execution(error) => BodyMutationError::Execution(error),
        WorldFieldOverlayErrorV1::Value(error) => BodyMutationError::Value(error),
        WorldFieldOverlayErrorV1::Member(error) => BodyMutationError::Original(Box::new(error)),
    }
}

fn unsupported() -> BodyMutationError {
    BodyMutationError::Invalid("TransferMaterial route or ledger unsupported by IWS lane")
}

#[derive(Debug)]
struct MaterialTransferOverflowErrorV1 {
    cause: crate::WorldError,
    _fee: MemoryReservation,
}
impl std::fmt::Display for MaterialTransferOverflowErrorV1 {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match &self.cause {
            crate::WorldError::ResourceBalanceInvalid { reason } => formatter.write_str(reason),
            _ => write!(formatter, "{:?}", self.cause),
        }
    }
}
impl std::error::Error for MaterialTransferOverflowErrorV1 {}

fn material_transfer_overflow_error(
    kind: &str,
    current: i64,
    amount: i64,
    memory: &oasis7_proto::memory_budget::MemoryBudget,
) -> Result<BodyMutationError, BodyMutationError> {
    // The original reason includes the caller's kind and two bounded i64
    // decimal values. Keep the allocation fee with the typed failure.
    const REASON_PREFIX: &str =
        "material transfer add preflight failed: material balance overflow: kind=";
    const CURRENT_PREFIX: &str = " current=";
    const AMOUNT_PREFIX: &str = " amount=";
    const I64_DECIMAL_MAX_BYTES: usize = 20;
    let reason_capacity = REASON_PREFIX
        .len()
        .checked_add(kind.len())
        .and_then(|bytes| bytes.checked_add(CURRENT_PREFIX.len()))
        .and_then(|bytes| bytes.checked_add(I64_DECIMAL_MAX_BYTES))
        .and_then(|bytes| bytes.checked_add(AMOUNT_PREFIX.len()))
        .and_then(|bytes| bytes.checked_add(I64_DECIMAL_MAX_BYTES))
        .ok_or(ValueTreeError::ResourceLimited(
            "MaterialTransfer overflow reason bound",
        ))?;
    let fee_bytes = std::mem::size_of::<MaterialTransferOverflowErrorV1>()
        .checked_add(reason_capacity)
        .ok_or(ValueTreeError::ResourceLimited(
            "MaterialTransfer overflow error allocation",
        ))?;
    let fee = memory
        .reserve(fee_bytes, MemoryClass::Ordinary)
        .map_err(ValueTreeError::Memory)?;
    let mut reason = String::with_capacity(reason_capacity);
    write!(
        &mut reason,
        "{REASON_PREFIX}{kind}{CURRENT_PREFIX}{current}{AMOUNT_PREFIX}{amount}"
    )
    .map_err(|_| ValueTreeError::Invalid("MaterialTransfer overflow reason formatting"))?;
    if reason.len() > reason_capacity || reason.capacity() > reason_capacity {
        return Err(
            ValueTreeError::ResourceLimited("MaterialTransfer overflow reason capacity").into(),
        );
    }
    Ok(BodyMutationError::Original(Box::new(
        MaterialTransferOverflowErrorV1 {
            cause: crate::WorldError::ResourceBalanceInvalid { reason },
            _fee: fee,
        },
    )))
}

#[expect(
    clippy::too_many_arguments,
    reason = "Selected action, qualified Current scopes, shared budgets and original sink stay explicit."
)]
#[path = "incremental_material_normalization_transfer_execute.rs"]
mod execute;
pub(in crate::world) use execute::execute_selected_material_transfer_v1;

fn ledger_text_bytes(ledger: &MaterialLedgerId) -> usize {
    match ledger {
        MaterialLedgerId::World => 0,
        MaterialLedgerId::Agent(id)
        | MaterialLedgerId::Site(id)
        | MaterialLedgerId::Factory(id) => id.len(),
    }
}

fn urgent_kind(kind: &str) -> bool {
    let bytes = kind.as_bytes();
    super::super::logistics::MATERIAL_TRANSIT_URGENT_KEYWORDS
        .iter()
        .any(|keyword| {
            let keyword = keyword.as_bytes();
            bytes
                .windows(keyword.len())
                .any(|candidate| candidate.eq_ignore_ascii_case(keyword))
        })
}

#[allow(clippy::too_many_arguments)]
fn read_agent_presence<M, P, V, W, E, C, PE, PC, D>(
    step: &mut IwsStepContext<'_, '_, '_, '_, M, V, W, D>,
    agent: &str,
    agents: &ReservedValue<AgentSubtreeStructureQualificationV1>,
    roots: &RootSetV1,
    read: &mut WorldFieldReadContextV1<'_, P, V, E, C>,
    parent_read: &mut WorldFieldReadContextV1<'_, P, V, PE, PC>,
    pending_entries_remaining: &mut usize,
    live: &ExecutionFence,
) -> Result<CurrentAgentPresenceV1, BodyMutationError>
where
    M: MerkleNodeReader,
    P: MerkleNodeReader,
    V: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    PE: FnMut() -> Option<ValueTreeError>,
    PC: FnMut() -> bool,
    D: FnMut() -> bool,
{
    let mut overlay = WorldFieldOverlayContextV1 {
        transaction: step.transaction,
        values: ValueReadContext {
            source: read.values.source,
            memory: step.mutation.memory,
            class: read.values.class,
            io: &mut *read.values.io,
        },
        key_work: &mut *read.key_work,
        semantic_bytes: read.semantic_bytes,
        record_bytes: read.record_bytes,
        take_state_error: &mut read.take_state_error,
        cancelled: || (read.cancelled)() || (step.mutation.cancelled)(),
    };
    let guard = overlay.transaction.guard();
    read_current_agent_presence_v1(
        agent,
        agents,
        roots,
        &mut overlay,
        parent_read,
        pending_entries_remaining,
        &guard,
        live,
    )
    .map_err(overlay_error)
}

fn read_priority<M, P, V, W, E, C, ME, D>(
    step: &mut IwsStepContext<'_, '_, '_, '_, M, V, W, D>,
    materials: &mut CorePolicyMaterialContextV1<'_, M, ME>,
    family: &ReservedValue<MaterialFamilyQualificationV1>,
    kind: &str,
    read: &mut WorldFieldReadContextV1<'_, P, V, E, C>,
    work: &mut MaterialProfileCurrentReadWorkV1,
    live: &ExecutionFence,
) -> Result<Option<MaterialDefaultPriority>, BodyMutationError>
where
    M: MerkleNodeReader,
    P: MerkleNodeReader,
    V: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    ME: FnMut() -> Option<ValueTreeError>,
    D: FnMut() -> bool,
{
    let mut overlay = WorldFieldOverlayContextV1 {
        transaction: step.transaction,
        values: ValueReadContext {
            source: read.values.source,
            memory: step.mutation.memory,
            class: read.values.class,
            io: &mut *materials.read_io,
        },
        key_work: &mut *materials.read_keys,
        semantic_bytes: materials.semantic_bytes,
        record_bytes: materials.record_bytes,
        take_state_error: &mut materials.take_state_error,
        cancelled: || (read.cancelled)() || (step.mutation.cancelled)(),
    };
    let guard = overlay.transaction.guard();
    read_current_material_default_priority_overlay_v1(
        kind,
        family,
        materials.pages,
        &mut overlay,
        work,
        &guard,
        live,
    )
    .map_err(overlay_error)
}

fn open_current_material_scope<M, P, V, W, E, C, ME, D>(
    step: &mut IwsStepContext<'_, '_, '_, '_, M, V, W, D>,
    materials: &mut CorePolicyMaterialContextV1<'_, M, ME>,
    read: &mut WorldFieldReadContextV1<'_, P, V, E, C>,
    live: &ExecutionFence,
) -> Result<ReservedValue<CurrentInventoryMaterialScopeV1>, BodyMutationError>
where
    M: MerkleNodeReader,
    P: MerkleNodeReader,
    V: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    ME: FnMut() -> Option<ValueTreeError>,
    D: FnMut() -> bool,
{
    let mut overlay = WorldFieldOverlayContextV1 {
        transaction: step.transaction,
        values: ValueReadContext {
            source: read.values.source,
            memory: step.mutation.memory,
            class: read.values.class,
            io: &mut *materials.read_io,
        },
        key_work: &mut *materials.read_keys,
        semantic_bytes: materials.semantic_bytes,
        record_bytes: materials.record_bytes,
        take_state_error: &mut materials.take_state_error,
        cancelled: || (read.cancelled)() || (step.mutation.cancelled)(),
    };
    let guard = overlay.transaction.guard();
    open_current_inventory_material_scope_v1(
        materials.parent,
        materials.roots,
        materials.pages,
        &mut overlay,
        materials.family_work,
        materials.scope_work,
        &guard,
        live,
    )
    .map_err(overlay_error)
}

#[allow(clippy::too_many_arguments)]
fn transfer_current_material<M, P, V, W, E, C, PE, PC, ME, D>(
    step: &mut IwsStepContext<'_, '_, '_, '_, M, V, W, D>,
    materials: &mut CorePolicyMaterialContextV1<'_, M, ME>,
    inventory: &ReservedValue<CurrentInventoryMaterialScopeV1>,
    agents: &ReservedValue<AgentSubtreeStructureQualificationV1>,
    from: &MaterialLedgerId,
    to: &MaterialLedgerId,
    requester: &str,
    kind: &str,
    amount: i64,
    read: &mut WorldFieldReadContextV1<'_, P, V, E, C>,
    parent_read: &mut WorldFieldReadContextV1<'_, P, V, PE, PC>,
    work: &mut AgentLedgerTransferWorkV1,
    live: &ExecutionFence,
) -> Result<AgentLedgerTransferOutcomeV1, BodyMutationError>
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
{
    let read_stop = RefCell::new(&mut read.cancelled);
    let mutation_stop = RefCell::new(&mut step.mutation.cancelled);
    let mut overlay = WorldFieldOverlayContextV1 {
        transaction: step.transaction,
        values: ValueReadContext {
            source: read.values.source,
            memory: step.mutation.memory,
            class: read.values.class,
            io: &mut *read.values.io,
        },
        key_work: &mut *read.key_work,
        semantic_bytes: read.semantic_bytes,
        record_bytes: read.record_bytes,
        take_state_error: &mut read.take_state_error,
        cancelled: || (read_stop.borrow_mut())() || (mutation_stop.borrow_mut())(),
    };
    let mut write = MemberRecordWriteContext {
        sink: &mut *step.mutation.sink,
        memory: step.mutation.memory,
        io: &mut *step.mutation.io,
        key_work: &mut *materials.write_keys,
        limits: materials.write_limits,
        cancelled: || (read_stop.borrow_mut())() || (mutation_stop.borrow_mut())(),
    };
    let guard = overlay.transaction.guard();
    transfer_current_agent_material_overlay_v1(
        requester,
        from,
        to,
        kind,
        amount,
        inventory,
        agents,
        materials.roots,
        &mut overlay,
        parent_read,
        &mut write,
        work,
        &guard,
        live,
    )
    .map_err(overlay_error)
}

fn reserve_completed_fee(
    memory: &oasis7_proto::memory_budget::MemoryBudget,
    text_bytes: usize,
) -> Result<MemoryReservation, BodyMutationError> {
    let bytes = std::mem::size_of::<CompletedMaterialTransferV1>()
        .checked_add(std::mem::size_of::<PreparedMaterialTransferEventV1>())
        .and_then(|n| n.checked_add(text_bytes))
        .and_then(|n| n.checked_add(256))
        .ok_or(ValueTreeError::ResourceLimited(
            "MaterialTransfer completed owner allocation",
        ))?;
    memory
        .reserve(bytes, MemoryClass::Ordinary)
        .map_err(ValueTreeError::Memory)
        .map_err(Into::into)
}

fn reject<M, V, W, D, ME, F>(
    step: &mut IwsStepContext<'_, '_, '_, '_, M, V, W, D>,
    materials: &mut CorePolicyMaterialContextV1<'_, M, ME>,
    live: &ExecutionFence,
    selected: &SelectedPendingActionV1,
    reason: crate::RejectReason,
    max_event_bytes: usize,
    audit_work: &mut WorldEventDecodeWorkV1,
    persist_original: &mut F,
) -> Result<CompletedMaterialTransferV1, BodyMutationError>
where
    M: MerkleNodeReader,
    V: ValueNodeReader,
    W: ValueNodeWriter,
    D: FnMut() -> bool,
    ME: FnMut() -> Option<ValueTreeError>,
    F: FnMut(&mut dyn AgentEventOriginalV1) -> Result<(), BodyMutationError>,
{
    let fee = reserve_completed_fee(step.mutation.memory, 0)?;
    let audit = step.append_selected_rejection_owned(
        materials,
        live,
        selected,
        reason,
        max_event_bytes,
        audit_work,
        persist_original,
    )?;
    Ok(CompletedMaterialTransferV1 {
        owner: CompletedMaterialTransferOwnerV1::Rejection(audit),
        _fee: fee,
    })
}

#[cfg(test)]
mod overflow_error_tests {
    use super::*;

    #[test]
    fn destination_overflow_keeps_the_legacy_resource_balance_reason() {
        let memory = oasis7_proto::memory_budget::MemoryBudget::new(1 << 20, 1 << 20).unwrap();
        let kind = "k".repeat(1024);
        let expected_reason = format!(
            "material transfer add preflight failed: material balance overflow: kind={kind} current={} amount=1",
            i64::MAX
        );
        let reason_capacity =
            "material transfer add preflight failed: material balance overflow: kind=".len()
                + kind.len()
                + " current=".len()
                + 20
                + " amount=".len()
                + 20;
        let error = material_transfer_overflow_error(&kind, i64::MAX, 1, &memory).unwrap();
        let BodyMutationError::Original(error) = error else {
            panic!("destination overflow must retain its typed original error");
        };
        assert_eq!(error.to_string(), expected_reason);
        let error = error
            .downcast_ref::<MaterialTransferOverflowErrorV1>()
            .expect("destination overflow wrapper");
        let crate::WorldError::ResourceBalanceInvalid { reason } = &error.cause else {
            panic!("overflow must preserve the typed WorldError variant");
        };
        assert_eq!(reason, &expected_reason);
        assert!(reason.len() <= reason_capacity);
        assert!(reason.capacity() <= reason_capacity);
    }
}
