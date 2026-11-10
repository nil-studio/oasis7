//! One parent-bound WorldEvent ledger counter shared by the actual step sinks.
//! Cognition Event facts do not enter this channel. Scalar telemetry is not an
//! execution/source/physical permission and cannot create an accepted head.
use super::incremental_body::BodyMutationError;
use oasis7_formal_protocol::{
    controlled_authority::formal::VerifiedFormalCommitV1,
    incremental_execution::{
        ExecutionError, ExecutionFactKindV1, ExecutionTransaction, PreparedGuard,
    },
    incremental_state::MerkleNodeReader,
};
use oasis7_proto::memory_budget::{MemoryBudget, MemoryClass, MemoryReservation, ReservedValue};
use oasis7_world_state::{
    incremental_runtime_history_manifest::RuntimeHistoryLaneV1,
    incremental_runtime_history_source::AcceptedRuntimeHistorySourceV1,
};
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
};
struct Counts {
    appended: u64,
    last_world_fact_end: usize,
    last_original: Option<[u8; 32]>,
}
struct Inner {
    memory: MemoryBudget,
    guard: PreparedGuard,
    base_hot: u64,
    counts: Mutex<Counts>,
    failed: AtomicBool,
    factory_prologue_claimed: AtomicBool,
    _charge: MemoryReservation,
}
/// Clones share the SAME sealed counter; they never start another event count.
#[derive(Clone)]
pub struct WorldEventCounterV1 {
    inner: Arc<Inner>,
}
fn invalid() -> BodyMutationError {
    BodyMutationError::Invalid("WorldEvent counter scope/order")
}
impl WorldEventCounterV1 {
    /// Must occur before Input/Event/Result facts are added to this fresh sole
    /// transaction. The real accepted Runtime selected Result authenticates the
    /// current hot window count; total/lastEventId are deliberately not used.
    pub fn begin<M: MerkleNodeReader>(
        parent: &ReservedValue<AcceptedRuntimeHistorySourceV1>,
        head: &VerifiedFormalCommitV1,
        transaction: &ExecutionTransaction<'_, M>,
        memory: &MemoryBudget,
    ) -> Result<Self, BodyMutationError> {
        let charge = memory
            .reserve(4096, MemoryClass::Ordinary)
            .map_err(ExecutionError::Memory)?;
        Self::validate_parent_source(parent, head, transaction, memory)?;
        transaction.claim_world_event_counter()?;
        let guard = transaction.guard();
        Ok(Self {
            inner: Arc::new(Inner {
                memory: memory.clone(),
                guard,
                base_hot: parent.manifest().lanes[RuntimeHistoryLaneV1::Journal.index()].count,
                counts: Mutex::new(Counts {
                    appended: 0,
                    last_world_fact_end: 0,
                    last_original: None,
                }),
                failed: AtomicBool::new(false),
                factory_prologue_claimed: AtomicBool::new(false),
                _charge: charge,
            }),
        })
    }
    /// Validate a fresh parent for directory construction without minting a
    /// counter. All actual event producers share the sole claimed counter.
    pub(super) fn validate_parent_source<M: MerkleNodeReader>(
        parent: &ReservedValue<AcceptedRuntimeHistorySourceV1>,
        head: &VerifiedFormalCommitV1,
        transaction: &ExecutionTransaction<'_, M>,
        memory: &MemoryBudget,
    ) -> Result<(), BodyMutationError> {
        let fact = parent.occurrence();
        let fence = transaction.parent_fence();
        if transaction.ordered_fact_count() != 0
            || !transaction.uses_budget(memory)
            || !parent.uses_budget(memory)
            || parent.allocation_class() != MemoryClass::Ordinary
            || fact.kind() != ExecutionFactKindV1::Result
            || fact.head_record_id() != head.verified_record_id()
            || fact.head_body_id() != fact.fact_body_id()
            || fact.head_body_id() != fence.parent_commit
            || fact.roots_id() != fence.parent_roots
            || parent.manifest().roots != fence.parent_roots
        {
            return Err(invalid());
        }
        let guard = transaction.guard();
        transaction.validate_transaction_identity(&guard)?;
        Ok(())
    }
    pub(super) fn same_counter(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.inner, &other.inner)
    }
    pub(super) fn validate<M: MerkleNodeReader>(
        &self,
        transaction: &ExecutionTransaction<'_, M>,
    ) -> Result<(), BodyMutationError> {
        if self.inner.failed.load(Ordering::Acquire) || !transaction.uses_budget(&self.inner.memory)
        {
            return Err(invalid());
        }
        transaction.validate_transaction_identity(&self.inner.guard)?;
        Ok(())
    }
    /// One prologue claim across every clone of this sole transaction counter.
    /// It is bookkeeping only: the Runtime owner must still authenticate and
    /// advance the actual clock before producing a private execution permit.
    pub(super) fn claim_factory_prologue<M: MerkleNodeReader>(
        &self,
        transaction: &mut ExecutionTransaction<'_, M>,
    ) -> Result<(), BodyMutationError> {
        transaction.contribute(|tx| {
            self.validate(tx)?;
            if tx.ordered_fact_count() != 0 {
                return Err(BodyMutationError::Invalid(
                    "factory prologue requires no contributed facts",
                ));
            }
            self.inner
                .factory_prologue_claimed
                .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
                .map_err(|_| BodyMutationError::Invalid("factory prologue already claimed"))?;
            Ok(())
        })
    }
    pub fn journal_height<M: MerkleNodeReader>(
        &self,
        transaction: &ExecutionTransaction<'_, M>,
    ) -> Result<u64, BodyMutationError> {
        self.validate(transaction)?;
        let counts = self.inner.counts.lock().map_err(|_| invalid())?;
        self.inner
            .base_hot
            .checked_add(counts.appended)
            .ok_or(ExecutionError::ResourceLimited("WorldEvent journal height").into())
    }
    /// Only typed WorldEvent success sites call this, after their canonical
    /// producer finish AND actual append_fact succeeded. The exact one-fact
    /// progress and monotonic ordinal reject duplicate notes. Any error must
    /// remain inside transaction.contribute so the entire step is poisoned.
    pub(super) fn note_success<M: MerkleNodeReader>(
        &self,
        transaction: &ExecutionTransaction<'_, M>,
        before_fact_count: usize,
    ) -> Result<(), BodyMutationError> {
        let result = (|| {
            self.validate(transaction)?;
            let after = transaction.ordered_fact_count();
            if before_fact_count.checked_add(1) != Some(after) {
                return Err(invalid());
            }
            let fact = transaction
                .ordered_fact(before_fact_count)
                .ok_or_else(invalid)?;
            if fact.kind != ExecutionFactKindV1::Event {
                return Err(invalid());
            }
            let mut counts = self.inner.counts.lock().map_err(|_| invalid())?;
            if before_fact_count < counts.last_world_fact_end {
                return Err(invalid());
            }
            let next = counts
                .appended
                .checked_add(1)
                .ok_or(ExecutionError::ResourceLimited("WorldEvent count"))?;
            self.inner
                .base_hot
                .checked_add(next)
                .ok_or(ExecutionError::ResourceLimited("WorldEvent journal height"))?;
            counts.appended = next;
            counts.last_world_fact_end = after;
            counts.last_original = Some(fact.id);
            Ok(())
        })();
        if result.is_err() {
            self.inner.failed.store(true, Ordering::Release);
        }
        result
    }

    /// Only a typed WorldEvent success site can advance this source. Unrelated
    /// Input/Result/Cognition facts appended later do not replace the tail.
    pub(super) fn latest_source<M: MerkleNodeReader>(
        &self,
        transaction: &ExecutionTransaction<'_, M>,
    ) -> Result<Option<CountedWorldEventSourceV1>, BodyMutationError> {
        self.validate(transaction)?;
        let counts = self.inner.counts.lock().map_err(|_| invalid())?;
        let Some(original) = counts.last_original else {
            if counts.appended != 0 || counts.last_world_fact_end != 0 {
                return Err(invalid());
            }
            return Ok(None);
        };
        let ordinal = counts
            .last_world_fact_end
            .checked_sub(1)
            .ok_or_else(invalid)?;
        let fact = transaction.ordered_fact(ordinal).ok_or_else(invalid)?;
        if fact.kind != ExecutionFactKindV1::Event || fact.id != original {
            return Err(invalid());
        }
        Ok(Some(CountedWorldEventSourceV1 {
            original,
            ordinal,
            identity: transaction.guard(),
        }))
    }
}

/// Source provenance from a real typed producer's counted success. This is not
/// a business execution permit, accepted occurrence or durable cache receipt.
pub(super) struct CountedWorldEventSourceV1 {
    original: [u8; 32],
    ordinal: usize,
    identity: PreparedGuard,
}
impl CountedWorldEventSourceV1 {
    pub(super) fn original(&self) -> &[u8; 32] {
        &self.original
    }
    pub(super) fn validate<M: MerkleNodeReader>(
        &self,
        transaction: &ExecutionTransaction<'_, M>,
    ) -> Result<(), BodyMutationError> {
        transaction.validate_transaction_identity(&self.identity)?;
        let fact = transaction.ordered_fact(self.ordinal).ok_or_else(invalid)?;
        if fact.kind != ExecutionFactKindV1::Event || fact.id != self.original {
            return Err(invalid());
        }
        Ok(())
    }
}
