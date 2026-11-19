//! Bounded business DATA read from the actual accepted Host parent. A read
//! retains the original source/proof and all decoded event fees; no World or
//! unqualified snapshot is reconstructed. Work is supplied once by its caller.
use super::cold::{Loan, amounts, take};
use super::*;
use oasis7_distfs::incremental_page_store::{PageIoBudget, PageStoreError, objects::ObjectIo};
use oasis7_formal_protocol::{
    controlled_authority::formal::bounded_context::{FormalBoundedContextV1, WorkIdentityV1},
    incremental_execution::{
        ExecutionFactKindV1, ExecutionParent, ExecutionTransaction, RootSetV1, TransactionLimits,
    },
    incremental_material::OriginalObjectReader,
    incremental_state::StateRootV1,
    incremental_value::{ValueIoBudget, ValueReadContext, ValueTreeError},
};
use oasis7_proto::memory_budget::{MemoryReservation, ReservedValue};
use oasis7_world_state::{
    incremental_agent_fields::AgentFieldV1,
    incremental_agent_subtree::{
        AgentSubtreeQualifiedRequestV1, AgentSubtreeScalarRequestV1, AgentSubtreeValueV1,
        read_agent_subtree_scalar_overlay_v1,
    },
    incremental_field_overlay::{WorldFieldOverlayContextV1, WorldFieldOverlayErrorV1},
    incremental_semantic_key::SemanticKeyBudget,
    incremental_world_event_decode::{
        DecodedWorldEventV1, WorldEventDecodeWorkV1, decode_world_event_original_v1,
    },
};

fn physical(error: PageStoreError) -> IwsHostRuntimeErrorV1 {
    IwsHostRuntimeErrorV1::Physical(error)
}
fn value(error: ValueTreeError) -> IwsHostRuntimeErrorV1 {
    IwsHostRuntimeErrorV1::Value(error)
}
fn invalid() -> FormalError {
    FormalError::Invalid("accepted observation requires the exact parent/work".into())
}
fn overlay(error: WorldFieldOverlayErrorV1) -> IwsHostRuntimeErrorV1 {
    match error {
        WorldFieldOverlayErrorV1::Execution(error) => IwsHostRuntimeErrorV1::Move(error),
        WorldFieldOverlayErrorV1::Value(error) => value(error),
        WorldFieldOverlayErrorV1::Member(error) => IwsHostRuntimeErrorV1::NormalSource(
            oasis7_runtime::BodyMutationError::Original(Box::new(error)),
        ),
    }
}

/// Original finite read accounts, retained across observations and failures.
/// The local ceilings never copy Common's bank. The exclusive loan below
/// reconciles every physical, logical, key and decoder debit with that bank.
pub struct HostObservationWorkV1 {
    state_pages: PageIoBudget,
    value_pages: PageIoBudget,
    original_pages: PageIoBudget,
    logical: ValueIoBudget,
    original_values: ValueIoBudget,
    keys: SemanticKeyBudget,
    event_decode: WorldEventDecodeWorkV1,
    points_remaining: u64,
    events_remaining: u64,
    semantic_bytes: u64,
    record_bytes: u64,
    max_scalar_bytes: u64,
    core: WorkIdentityV1,
    fee: MemoryReservation,
}

/// Explicit independent read-only ceilings. These values are DATA, not a
/// replacement for Common; all reads reconcile with the same original bank.
pub struct HostObservationPartsV1 {
    pub state_pages: PageIoBudget,
    pub value_pages: PageIoBudget,
    pub original_pages: PageIoBudget,
    pub logical: ValueIoBudget,
    pub original_values: ValueIoBudget,
    pub keys: SemanticKeyBudget,
    pub event_decode: WorldEventDecodeWorkV1,
    pub points_remaining: u64,
    pub events_remaining: u64,
    pub semantic_bytes: u64,
    pub record_bytes: u64,
    pub max_scalar_bytes: u64,
}
impl HostObservationWorkV1 {
    pub fn from_parts_bounded(
        parts: HostObservationPartsV1,
        common: &mut FormalBoundedContextV1<'_>,
    ) -> Result<Self, IwsHostRuntimeErrorV1> {
        for lane in [
            &parts.state_pages,
            &parts.value_pages,
            &parts.original_pages,
        ] {
            if lane.writes_remaining != 0 || lane.write_bytes_remaining != 0 {
                return Err(invalid().into());
            }
        }
        common.charge(1, size_of::<Self>() as u64)?;
        let fee = common
            .memory()
            .reserve(size_of::<Self>(), MemoryClass::Ordinary)
            .map_err(ValueTreeError::Memory)
            .map_err(value)?;
        Ok(Self {
            state_pages: parts.state_pages,
            value_pages: parts.value_pages,
            original_pages: parts.original_pages,
            logical: parts.logical,
            original_values: parts.original_values,
            keys: parts.keys,
            event_decode: parts.event_decode,
            points_remaining: parts.points_remaining,
            events_remaining: parts.events_remaining,
            semantic_bytes: parts.semantic_bytes,
            record_bytes: parts.record_bytes,
            max_scalar_bytes: parts.max_scalar_bytes,
            core: common.work().identity(),
            fee,
        })
    }
    fn validate_context(
        &self,
        common: &mut FormalBoundedContextV1<'_>,
    ) -> Result<(), IwsHostRuntimeErrorV1> {
        common.consume_steps(1)?;
        if self.core != common.work().identity() || !self.fee.uses_budget(common.memory()) {
            return Err(invalid().into());
        }
        Ok(())
    }
    /// `true` denotes bytes; every other mutable counter is a step lane.
    fn counters(&mut self) -> [(&mut u64, bool); 25] {
        [
            (&mut self.state_pages.reads_remaining, false),
            (&mut self.state_pages.read_bytes_remaining, true),
            (&mut self.state_pages.writes_remaining, false),
            (&mut self.state_pages.write_bytes_remaining, true),
            (&mut self.value_pages.reads_remaining, false),
            (&mut self.value_pages.read_bytes_remaining, true),
            (&mut self.value_pages.writes_remaining, false),
            (&mut self.value_pages.write_bytes_remaining, true),
            (&mut self.original_pages.reads_remaining, false),
            (&mut self.original_pages.read_bytes_remaining, true),
            (&mut self.original_pages.writes_remaining, false),
            (&mut self.original_pages.write_bytes_remaining, true),
            (&mut self.logical.nodes_remaining, false),
            (&mut self.logical.read_bytes_remaining, true),
            (&mut self.logical.write_bytes_remaining, true),
            (&mut self.original_values.nodes_remaining, false),
            (&mut self.original_values.read_bytes_remaining, true),
            (&mut self.original_values.write_bytes_remaining, true),
            (&mut self.keys.bytes_remaining, true),
            (&mut self.event_decode.structure.values_remaining, false),
            (&mut self.event_decode.structure.bytes_remaining, true),
            (&mut self.event_decode.canonical.visits_remaining, false),
            (&mut self.event_decode.canonical.bytes_remaining, true),
            (&mut self.points_remaining, false),
            (&mut self.events_remaining, false),
        ]
    }
}

/// One exclusive Common lease; the callback cannot borrow its parent. Session
/// loans settle inside the callback before its counters are reconciled here.
fn metered<T>(
    work: &mut HostObservationWorkV1,
    common: &mut FormalBoundedContextV1<'_>,
    run: impl FnOnce(&mut HostObservationWorkV1) -> Result<T, IwsHostRuntimeErrorV1>,
) -> Result<T, IwsHostRuntimeErrorV1> {
    let layout = 3 * size_of::<[u64; 25]>() + size_of::<[bool; 25]>();
    common.charge(1, layout as u64)?;
    let _fee = common
        .memory()
        .reserve(layout, MemoryClass::Ordinary)
        .map_err(ValueTreeError::Memory)
        .map_err(value)?;
    let original = work.counters().map(|(counter, _)| *counter);
    let units = work.counters().map(|(_, bytes)| bytes);
    let byte_lanes = original
        .iter()
        .zip(units)
        .filter(|(remaining, bytes)| **remaining != 0 && *bytes)
        .count() as u64;
    let step_lanes = original
        .iter()
        .zip(units)
        .filter(|(remaining, bytes)| **remaining != 0 && !*bytes)
        .count() as u64;
    let (_, bank, _) = common.parts();
    let child = bank.begin_child()?;
    let steps = child.remaining_steps() / step_lanes.max(1);
    let bytes = child.remaining_read_bytes() / byte_lanes.max(1);
    for (counter, is_bytes) in work.counters() {
        *counter = (*counter).min(if is_bytes { bytes } else { steps });
    }
    let admitted = work.counters().map(|(counter, _)| *counter);
    let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| run(work)));
    let accounting = (|| -> Result<(), FormalError> {
        let mut steps = 0u64;
        let mut bytes = 0u64;
        for (index, (counter, is_bytes)) in work.counters().into_iter().enumerate() {
            let spent = admitted[index].checked_sub(*counter).ok_or_else(invalid)?;
            let total = if is_bytes { &mut bytes } else { &mut steps };
            *total = total.checked_add(spent).ok_or_else(invalid)?;
            // Restore only the unadmitted portion, never attempted reads.
            *counter = original[index].checked_sub(spent).ok_or_else(invalid)?;
        }
        child.consume(steps, bytes)
    })();
    match outcome {
        Err(cause) => {
            // Preserve the original panic, and burn the unrecoverable Common.
            drop(child);
            std::panic::resume_unwind(cause)
        }
        Ok(result) => {
            if accounting.is_ok() {
                child.finish();
            } else {
                drop(child);
            }
            let output = result?;
            accounting?;
            common.consume_steps(0)?;
            Ok(output)
        }
    }
}

/// Optional resource cells stay optional; absence is not silently made zero.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ObservedAgentResourcesV1 {
    pub electricity: Option<i64>,
    pub data: Option<i64>,
}

pub struct ObservedEventOriginalV1 {
    /// Original FactSet ordinal, including intervening Input/Result facts.
    pub ordinal: usize,
    pub original: [u8; 32],
    pub decoded: ReservedValue<DecodedWorldEventV1>,
}

/// DATA observations only. The returned events retain their real decoding and
/// container reservations for as long as a consumer holds them.
pub struct HostObservationV1 {
    pub record: [u8; 32],
    pub body: [u8; 32],
    pub height: u64,
    pub sender: ObservedAgentResourcesV1,
    pub receiver: ObservedAgentResourcesV1,
    pub events: ReservedValue<Vec<ObservedEventOriginalV1>>,
    proof: oasis7_distfs::controlled_authority::formal::endpoint::PaidHostQualifiedParentProofV1,
}

impl HostObservationV1 {
    /// The original qualified proof read and matched against this accepted
    /// source. Its complete paid request owner remains alive with these DATA.
    pub fn qualified_proof(
        &self,
    ) -> &oasis7_formal_protocol::controlled_authority::formal::FormalQualifiedCommitProofV1 {
        self.proof.proof()
    }
}

fn roots_clone_fee(
    roots: &RootSetV1,
    common: &mut FormalBoundedContextV1<'_>,
) -> Result<MemoryReservation, IwsHostRuntimeErrorV1> {
    common.consume_steps(roots.state.len() as u64 + 1)?;
    let layout = std::mem::size_of_val(roots)
        .checked_add(roots.world_id.len())
        .and_then(|n| {
            roots
                .state
                .len()
                .checked_mul(size_of::<StateRootV1>())
                .and_then(|v| n.checked_add(v))
        })
        .and_then(|n| {
            roots
                .state
                .iter()
                .try_fold(n, |n, root| n.checked_add(root.node_root.len()))
        })
        .and_then(|n| n.checked_mul(4))
        .and_then(|n| n.checked_add(8192))
        .ok_or_else(invalid)?;
    common.charge(layout as u64, layout as u64)?;
    common
        .memory()
        .reserve(layout, MemoryClass::Ordinary)
        .map_err(ValueTreeError::Memory)
        .map_err(value)
}

#[allow(clippy::too_many_arguments)]
pub fn observe_current_bounded(
    host: &IwsHostRuntimeV1,
    sender: &str,
    receiver: &str,
    transaction: TransactionLimits,
    work: &mut HostObservationWorkV1,
    memory: &MemoryBudget,
    archive: &FormalArchiveReadWorkV1,
    cancelled: &AtomicBool,
) -> Result<HostObservationV1, IwsHostRuntimeErrorV1> {
    archive.consume_steps(1, &mut || cancelled.load(Ordering::Acquire))?;
    if host.continuation.is_some() {
        return Err(invalid().into());
    }
    let current = host.current.current().ok_or_else(invalid)?;
    let resident = current.resident.as_ref().ok_or_else(invalid)?;
    current.source.with_parent_read_view_bounded(
        memory,
        archive,
        &mut || cancelled.load(Ordering::Acquire),
        |parent| -> Result<_, IwsHostRuntimeErrorV1> {
            let mut stop = || cancelled.load(Ordering::Acquire);
            let mut common = archive.bounded_context(memory, &mut stop);
            current.source.validate_context(&mut common)?;
            work.validate_context(&mut common)?;
            resident
                .validate_live_binding_v1(current.source.record_id(), &parent.fence(), &mut common)
                .map_err(IwsHostRuntimeErrorV1::NormalSource)?;
            let proof = parent.read_qualified_parent_proof_bounded(memory, archive, &mut || {
                cancelled.load(Ordering::Acquire)
            })?;
            proof.validate_parent_bounded(&parent, &mut common)?;
            let fence = parent.fence();
            let _roots_fee = roots_clone_fee(parent.roots(), &mut common)?;
            let facts = &parent.core().facts().facts;
            let fact_bytes = facts
                .len()
                .checked_mul(size_of::<
                    oasis7_formal_protocol::incremental_execution::ExecutionFactRefV1,
                >())
                .ok_or_else(invalid)?;
            // Pay both the count and the later ordered traversal before either.
            common.charge(facts.len() as u64 * 2, fact_bytes as u64 * 2)?;
            let count = facts
                .iter()
                .filter(|fact| fact.kind == ExecutionFactKindV1::Event)
                .count();
            let layout = count
                .checked_mul(size_of::<ObservedEventOriginalV1>())
                .and_then(|n| n.checked_add(size_of::<HostObservationV1>()))
                .ok_or_else(invalid)?;
            common.charge(count as u64 + 1, layout as u64)?;
            let events_fee = memory
                .reserve(layout, MemoryClass::Ordinary)
                .map_err(ValueTreeError::Memory)
                .map_err(value)?;
            let mut events = Vec::new();
            events.try_reserve_exact(count).map_err(|_| invalid())?;
            if events.capacity() > count {
                return Err(invalid().into());
            }
            let objects = super::normal::io::open_originals_metered(
                parent.pages(),
                &mut work.original_pages,
                cancelled,
                &mut common,
            )?;
            let (sender, receiver) = metered(work, &mut common, |work| {
                let budget = take(&mut work.state_pages);
                let ceiling = amounts(&budget);
                let state = Loan {
                    value: parent
                        .pages()
                        .state_session_with_cancelled(budget, MemoryClass::Ordinary, cancelled)
                        .map_err(physical)?,
                    slot: &mut work.state_pages,
                    ceiling,
                };
                let budget = take(&mut work.value_pages);
                let ceiling = amounts(&budget);
                let values = Loan {
                    value: parent
                        .pages()
                        .value_session_with_cancelled(budget, cancelled),
                    slot: &mut work.value_pages,
                    ceiling,
                };
                let mut tx = ExecutionTransaction::begin(
                    ExecutionParent {
                        commit: fence.parent_commit,
                        roots: (**parent.roots()).clone(),
                        authority: fence.authority,
                    },
                    parent.head().verified_record_id(),
                    &state.value,
                    memory,
                    transaction,
                )
                .map_err(IwsHostRuntimeErrorV1::Move)?;
                let mut amounts = [None; 4];
                for (index, (agent, ordinal)) in
                    [(sender, 0), (sender, 1), (receiver, 0), (receiver, 1)]
                        .into_iter()
                        .enumerate()
                {
                    work.points_remaining =
                        work.points_remaining.checked_sub(1).ok_or_else(|| {
                            value(ValueTreeError::ResourceLimited("observation points"))
                        })?;
                    let guard = tx.guard();
                    let mut context = WorldFieldOverlayContextV1 {
                        transaction: &mut tx,
                        values: ValueReadContext {
                            source: &values.value,
                            memory,
                            class: MemoryClass::Ordinary,
                            io: &mut work.logical,
                        },
                        key_work: &mut work.keys,
                        semantic_bytes: work.semantic_bytes,
                        record_bytes: work.record_bytes,
                        take_state_error: || {
                            state
                                .value
                                .take_read_error()
                                .map(|error| ValueTreeError::Io(std::io::Error::other(error)))
                        },
                        cancelled: || cancelled.load(Ordering::Acquire),
                    };
                    amounts[index] = match read_agent_subtree_scalar_overlay_v1(
                        AgentSubtreeQualifiedRequestV1 {
                            cell: AgentSubtreeScalarRequestV1 {
                                agent,
                                field: AgentFieldV1::ResourceAmount,
                                ordinal,
                            },
                            qualification: parent.storage().current().raw().agents(),
                        },
                        &mut context,
                        work.max_scalar_bytes,
                        &mut std::io::sink(),
                        &guard,
                        &fence,
                    )
                    .map_err(overlay)?
                    {
                        None => None,
                        Some(AgentSubtreeValueV1::I64(amount)) => Some(amount),
                        Some(_) => return Err(invalid().into()),
                    };
                }
                tx.validate_bootstrap_read_only_v1(&fence)
                    .map_err(IwsHostRuntimeErrorV1::Move)?;
                let originals = super::normal::originals::HostNormalOriginalsV1::new(
                    &objects,
                    ObjectIo {
                        physical: &mut work.original_pages,
                        logical: &mut work.original_values,
                        class: MemoryClass::Ordinary,
                        cancelled,
                    },
                    memory,
                )
                .map_err(physical)?;
                let mut reader = originals.reader();
                for (ordinal, fact) in facts.iter().enumerate() {
                    if cancelled.load(Ordering::Acquire) {
                        return Err(value(ValueTreeError::Cancelled));
                    }
                    if fact.kind != ExecutionFactKindV1::Event {
                        continue;
                    }
                    work.events_remaining =
                        work.events_remaining.checked_sub(1).ok_or_else(|| {
                            value(ValueTreeError::ResourceLimited("observation events"))
                        })?;
                    let raw = reader
                        .read_original(&fact.id, work.event_decode.max_original_bytes, memory)
                        .map_err(physical)?;
                    let decoded = decode_world_event_original_v1(
                        raw.as_slice(),
                        memory,
                        MemoryClass::Ordinary,
                        &mut work.event_decode,
                        || cancelled.load(Ordering::Acquire),
                    )
                    .map_err(value)?;
                    events.push(ObservedEventOriginalV1 {
                        ordinal,
                        original: fact.id,
                        decoded,
                    });
                }
                Ok((
                    ObservedAgentResourcesV1 {
                        electricity: amounts[0],
                        data: amounts[1],
                    },
                    ObservedAgentResourcesV1 {
                        electricity: amounts[2],
                        data: amounts[3],
                    },
                ))
            })?;
            Ok(HostObservationV1 {
                record: parent.head().verified_record_id(),
                body: fence.parent_commit,
                height: parent.core().body().height,
                sender,
                receiver,
                events: events_fee.retain(events),
                proof,
            })
        },
    )
}
