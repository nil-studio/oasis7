//! Atomic Current Agent-ledger transfer preparation/application.
//!
//! This owner is deliberately narrower than the generic inventory updater:
//! the source must be the requester's Agent ledger, both Agent identities are
//! read from the real Current subtree, and the complete parent inventory
//! qualification is carried by the private Current material scope. Only the
//! destination group may be created, and only after the destination Agent is
//! proven to exist. Both balance outputs are prepared before either is applied.
use crate::{
    MaterialLedgerId,
    incremental_agent_subtree::{
        AgentSubtreeStructureQualificationV1, CurrentAgentPresenceV1,
        read_current_agent_presence_v1,
    },
    incremental_field_member::MemberRecordWriteContext,
    incremental_field_overlay::{WorldFieldOverlayContextV1, WorldFieldOverlayErrorV1},
    incremental_field_read::WorldFieldReadContextV1,
    incremental_inventory::CurrentInventoryMaterialScopeV1,
    incremental_inventory_change::{
        InventoryChangeError, InventoryChangeRefV1, InventoryOutputV1,
        prepare_resolved_inventory_change_v1,
    },
    incremental_inventory_genesis::InventoryWriteSink,
    incremental_inventory_overlay_read::resolve_point,
    incremental_inventory_read::LedgerQuantityV1,
    incremental_inventory_transaction::apply_inventory_output_v1,
    incremental_inventory_value_read::InventoryValueReader,
};
use oasis7_formal_protocol::{
    incremental_execution::{ExecutionError, ExecutionFence, PreparedGuard, RootSetV1},
    incremental_state::{MerkleNodeReader, StateWriteV1},
    incremental_value::{ValueNodeReader, ValueNodeWriter, ValueTreeError},
};
use oasis7_proto::memory_budget::{MemoryClass, ReservedValue};

/// Result is a business observation, not a permission token. `Applied` means
/// both point outputs were prepared and provisionally written to the supplied
/// transaction; the transaction owner still controls seal and publication.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AgentLedgerTransferOutcomeV1 {
    RequesterAbsent,
    RecipientAbsent,
    Insufficient { available: i64 },
    DestinationOverflow { current: i64 },
    Applied,
}

/// Caller-owned finite work. Presence reads share the same pending-entry cap;
/// point preparation consumes these counters continuously across both ledgers.
pub struct AgentLedgerTransferWorkV1 {
    pub steps_remaining: u64,
    pub bytes_remaining: u64,
    pub pending_entries_remaining: usize,
}
impl AgentLedgerTransferWorkV1 {
    fn spend(&mut self, bytes: usize) -> Result<(), ValueTreeError> {
        self.steps_remaining =
            self.steps_remaining
                .checked_sub(1)
                .ok_or(ValueTreeError::ResourceLimited(
                    "Agent ledger transfer steps",
                ))?;
        self.bytes_remaining = self
            .bytes_remaining
            .checked_sub(
                u64::try_from(bytes)
                    .map_err(|_| ValueTreeError::ResourceLimited("Agent ledger transfer bytes"))?,
            )
            .ok_or(ValueTreeError::ResourceLimited(
                "Agent ledger transfer bytes",
            ))?;
        Ok(())
    }
}

struct PointWrites {
    writes: [Option<ReservedValue<StateWriteV1>>; 2],
    len: usize,
}
impl PointWrites {
    fn new() -> Self {
        Self {
            writes: std::array::from_fn(|_| None),
            len: 0,
        }
    }
    fn iter(&self) -> impl Iterator<Item = &ReservedValue<StateWriteV1>> + Clone {
        self.writes[..self.len].iter().flatten()
    }
}
impl InventoryWriteSink for PointWrites {
    fn put_write(&mut self, write: ReservedValue<StateWriteV1>) -> Result<(), ValueTreeError> {
        let slot = self
            .writes
            .get_mut(self.len)
            .ok_or(ValueTreeError::Invalid("Agent ledger transfer write count"))?;
        *slot = Some(write);
        self.len += 1;
        Ok(())
    }
}

fn change_error(error: InventoryChangeError) -> WorldFieldOverlayErrorV1 {
    match error {
        InventoryChangeError::Precondition => {
            ValueTreeError::Invalid("Agent ledger transfer point precondition").into()
        }
        InventoryChangeError::Invalid(reason) => ValueTreeError::Invalid(reason).into(),
        InventoryChangeError::Value(error) => error.into(),
        InventoryChangeError::Record(error) => WorldFieldOverlayErrorV1::Member(error),
    }
}

fn contribution_error(
    error: crate::incremental_inventory_transaction::InventoryContributionError,
) -> WorldFieldOverlayErrorV1 {
    use crate::incremental_inventory_transaction::InventoryContributionError as Contribution;
    match error {
        Contribution::Execution(error) => WorldFieldOverlayErrorV1::Execution(error),
        Contribution::Invalid(reason) => ValueTreeError::Invalid(reason).into(),
        Contribution::DependencyConflict => {
            ValueTreeError::Invalid("Agent ledger transfer dependency conflict").into()
        }
        Contribution::Cancelled => ValueTreeError::Cancelled.into(),
    }
}

/// Validate the real Current agent identities and inventory scope, then stage
/// an own-Agent-ledger to existing-Agent-ledger transfer atomically.
///
/// The helper does not grant arbitrary ledger-group creation. Non-Agent source
/// or destination variants are unsupported here and must be handled by a
/// consumer with their own Current authority reader.
#[allow(clippy::too_many_arguments)]
pub fn transfer_current_agent_material_overlay_v1<M, P, R, V, E, C, PE, PC, W, CW>(
    requester: &str,
    from: &MaterialLedgerId,
    to: &MaterialLedgerId,
    material: &str,
    amount: i64,
    inventory: &ReservedValue<CurrentInventoryMaterialScopeV1>,
    agents: &ReservedValue<AgentSubtreeStructureQualificationV1>,
    roots: &RootSetV1,
    context: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    parent_read: &mut WorldFieldReadContextV1<'_, P, V, PE, PC>,
    write: &mut MemberRecordWriteContext<'_, W, CW>,
    work: &mut AgentLedgerTransferWorkV1,
    guard: &PreparedGuard,
    live: &ExecutionFence,
) -> Result<AgentLedgerTransferOutcomeV1, WorldFieldOverlayErrorV1>
where
    M: MerkleNodeReader,
    P: MerkleNodeReader,
    R: ValueNodeReader,
    V: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    PE: FnMut() -> Option<ValueTreeError>,
    PC: FnMut() -> bool,
    W: ValueNodeWriter,
    CW: FnMut() -> bool,
{
    let result = (|| {
        context.transaction.validate_guard(guard, live)?;
        let requester_matches_source =
            matches!(from, MaterialLedgerId::Agent(id) if id == requester);
        let recipient = match to {
            MaterialLedgerId::Agent(id) => id.as_str(),
            _ => {
                return Err(ValueTreeError::Invalid(
                    "Agent ledger transfer destination unsupported",
                )
                .into());
            }
        };
        if !requester_matches_source || from == to || amount <= 0 || material.trim().is_empty() {
            return Err(ValueTreeError::Invalid("Agent ledger transfer input").into());
        }
        if context.values.class != MemoryClass::Ordinary
            || parent_read.values.class != MemoryClass::Ordinary
            || !context.values.memory.same_pool(write.memory)
            || !context.values.memory.same_pool(parent_read.values.memory)
            || !context.transaction.uses_budget(context.values.memory)
            || !inventory.uses_budget(context.values.memory)
            || !agents.uses_budget(context.values.memory)
            || agents.allocation_class() != MemoryClass::Ordinary
        {
            return Err(ValueTreeError::Invalid("Agent ledger transfer pool/class").into());
        }

        // This snapshot came from the qualified Current inventory opener. Validate
        // it before doing Current agent reads; the exact transfer points below are
        // then resolved from the transaction itself.
        let inventory_guard = context.transaction.guard();
        inventory.validate_current(context, &inventory_guard, live)?;

        let requester_presence_guard = context.transaction.guard();
        if read_current_agent_presence_v1(
            requester,
            agents,
            roots,
            context,
            parent_read,
            &mut work.pending_entries_remaining,
            &requester_presence_guard,
            live,
        )? != CurrentAgentPresenceV1::Existing
        {
            return Ok(AgentLedgerTransferOutcomeV1::RequesterAbsent);
        }
        let recipient_presence_guard = context.transaction.guard();
        if read_current_agent_presence_v1(
            recipient,
            agents,
            roots,
            context,
            parent_read,
            &mut work.pending_entries_remaining,
            &recipient_presence_guard,
            live,
        )? != CurrentAgentPresenceV1::Existing
        {
            return Ok(AgentLedgerTransferOutcomeV1::RecipientAbsent);
        }

        // The fixed output owners contain only fixed-layout keys/dependency arrays;
        // StateWrite key buffers receive their own per-write reservation in the
        // original inventory writer. Hold both output owners until all outputs are
        // applied so their retained arrays are not charged only during creation.
        let _prepared = context
            .values
            .memory
            .reserve(
                2 * (std::mem::size_of::<InventoryOutputV1>() + std::mem::size_of::<PointWrites>())
                    + (16 << 10),
                MemoryClass::Ordinary,
            )
            .map_err(ValueTreeError::Memory)?;

        context.transaction.contribute(|transaction| {
            if !transaction.uses_budget(context.values.memory)
                || !context.values.memory.same_pool(write.memory)
                || context.values.class != MemoryClass::Ordinary
            {
                return Err(ValueTreeError::Invalid("Agent ledger transfer pool/class").into());
            }
            work.spend(requester.len() + recipient.len() + material.len())?;
            if (context.cancelled)() || (write.cancelled)() || (parent_read.cancelled)() {
                return Err(ValueTreeError::Cancelled.into());
            }
            let (source_point, destination_point) = {
                let mut cancelled =
                    || (context.cancelled)() || (write.cancelled)() || (parent_read.cancelled)();
                let mut records = InventoryValueReader {
                    values: &mut context.values,
                    key_work: &mut *context.key_work,
                    semantic_bytes: context.semantic_bytes,
                    record_bytes: context.record_bytes,
                    cancelled: &mut cancelled,
                };
                work.spend(requester.len() + material.len())?;
                let source = resolve_point(
                    transaction,
                    &mut records,
                    &mut context.take_state_error,
                    "material_ledgers",
                    &(from, material),
                    Some(from),
                )?;
                work.spend(recipient.len() + material.len())?;
                let destination = resolve_point(
                    transaction,
                    &mut records,
                    &mut context.take_state_error,
                    "material_ledgers",
                    &(to, material),
                    Some(to),
                )?;
                (source, destination)
            };
            let available = match source_point.quantity() {
                LedgerQuantityV1::Value(value) => value,
                LedgerQuantityV1::LedgerAbsent | LedgerQuantityV1::MaterialAbsent => 0,
            };
            if available < amount {
                return Ok(AgentLedgerTransferOutcomeV1::Insufficient { available });
            }
            let source_after = available
                .checked_sub(amount)
                .ok_or(ValueTreeError::Invalid(
                    "Agent ledger transfer source arithmetic",
                ))?;
            let destination_before = match destination_point.quantity() {
                LedgerQuantityV1::Value(value) => value,
                LedgerQuantityV1::LedgerAbsent | LedgerQuantityV1::MaterialAbsent => 0,
            };
            let Some(destination_after) = destination_before.checked_add(amount) else {
                return Ok(AgentLedgerTransferOutcomeV1::DestinationOverflow {
                    current: destination_before,
                });
            };
            let source_expected = source_point.quantity();
            let destination_expected = destination_point.quantity();
            let source_change = InventoryChangeRefV1::LedgerMaterial {
                ledger: from,
                material,
                expected: source_expected,
                after: (source_after != 0).then_some(source_after),
                create_ledger: false,
            };
            let destination_change = InventoryChangeRefV1::LedgerMaterial {
                ledger: to,
                material,
                expected: destination_expected,
                after: (destination_after != 0).then_some(destination_after),
                create_ledger: matches!(destination_expected, LedgerQuantityV1::LedgerAbsent),
            };

            let mut source_writes = PointWrites::new();
            let mut destination_writes = PointWrites::new();
            work.spend(requester.len() + material.len())?;
            let mut cancelled = || (context.cancelled)() || (parent_read.cancelled)();
            let source_output = prepare_resolved_inventory_change_v1(
                source_change,
                source_point,
                write,
                &mut source_writes,
                &mut cancelled,
            )
            .map_err(change_error)?;
            work.spend(recipient.len() + material.len())?;
            let destination_output = prepare_resolved_inventory_change_v1(
                destination_change,
                destination_point,
                write,
                &mut destination_writes,
                &mut cancelled,
            )
            .map_err(change_error)?;
            if (context.cancelled)() || (write.cancelled)() || (parent_read.cancelled)() {
                return Err(ValueTreeError::Cancelled.into());
            }

            // Neither write batch reaches the overlay until both balance outputs
            // and their exact dependencies have been prepared successfully.
            apply_inventory_output_v1(
                &source_output,
                source_writes.iter(),
                transaction,
                write.memory,
                &mut || (context.cancelled)() || (write.cancelled)() || (parent_read.cancelled)(),
            )
            .map_err(contribution_error)?;
            apply_inventory_output_v1(
                &destination_output,
                destination_writes.iter(),
                transaction,
                write.memory,
                &mut || (context.cancelled)() || (write.cancelled)() || (parent_read.cancelled)(),
            )
            .map_err(contribution_error)?;
            if (context.cancelled)() || (write.cancelled)() || (parent_read.cancelled)() {
                return Err(ValueTreeError::Cancelled.into());
            }
            Ok(AgentLedgerTransferOutcomeV1::Applied)
        })
    })();
    if result.is_err() {
        let _: Result<(), ExecutionError> = context
            .transaction
            .contribute(|_| Err(ExecutionError::Aborted));
    }
    result
}

#[cfg(test)]
#[path = "incremental_inventory_agent_transfer_tests.rs"]
mod tests;
