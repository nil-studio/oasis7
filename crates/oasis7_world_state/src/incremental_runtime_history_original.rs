//! Borrowed Runtime History originals. This produces provisional bytes only;
//! candidate fact/type validation and accepted manifest binding remain required.
use crate::{
    WorldEvent,
    consensus::{TickConsensusRecord, TickConsensusRejectionAuditEvent},
    snapshot::SnapshotRecord,
};
use crate::{
    incremental_agent_mutation::BodyMutationError,
    incremental_original_producer::{self as original, AgentEventOriginalV1},
};
use oasis7_proto::memory_budget::MemoryBudget;
use oasis7_wasm_abi::CanonicalCborWorkV1;

/// Result envelopes separate otherwise identical DTO encodings by original type.
#[derive(serde::Serialize)]
struct ResultEnvelope<'a, T: serde::Serialize + ?Sized> {
    domain: &'static str,
    schema: &'static str,
    value: &'a T,
}
const DOMAIN: &str = "oasis7.runtime.history.original.v1";
/// Each variant retains the actual source DTO without a JSON or owned copy.
/// Event originals retain the existing canonical WorldEvent encoding.
pub enum RuntimeHistoryOriginalRefV1<'a> {
    Snapshot(&'a SnapshotRecord),
    Event(&'a WorldEvent),
    Tick(&'a TickConsensusRecord),
    RejectionAudit(&'a TickConsensusRejectionAuditEvent),
}
/// No locator or acceptance authority is minted by this streaming callback.
/// The shared producer rejects skipped/partial/retried persistence attempts.
pub fn with_runtime_history_original_v1<C: FnMut() -> bool, T>(
    value: RuntimeHistoryOriginalRefV1<'_>,
    memory: &MemoryBudget,
    work: &mut CanonicalCborWorkV1,
    cancelled: &mut C,
    max_bytes: usize,
    consume: impl FnOnce(&mut dyn AgentEventOriginalV1) -> Result<T, BodyMutationError>,
) -> Result<T, BodyMutationError> {
    fn run<S: serde::Serialize + ?Sized, C: FnMut() -> bool, T>(
        value: &S,
        memory: &MemoryBudget,
        work: &mut CanonicalCborWorkV1,
        cancelled: &mut C,
        max_bytes: usize,
        consume: impl FnOnce(&mut dyn AgentEventOriginalV1) -> Result<T, BodyMutationError>,
    ) -> Result<T, BodyMutationError> {
        let measured = original::measure_serializable(value, memory, work, cancelled, max_bytes)?;
        let mut producer = original::Producer::new(measured, memory, work, cancelled);
        let result = consume(&mut producer)?;
        producer.check_complete()?;
        Ok(result)
    }
    match value {
        RuntimeHistoryOriginalRefV1::Snapshot(v) => run(
            &ResultEnvelope {
                domain: DOMAIN,
                schema: "snapshot-record.v1",
                value: v,
            },
            memory,
            work,
            cancelled,
            max_bytes,
            consume,
        ),
        RuntimeHistoryOriginalRefV1::Event(v) => {
            run(v, memory, work, cancelled, max_bytes, consume)
        }
        RuntimeHistoryOriginalRefV1::Tick(v) => run(
            &ResultEnvelope {
                domain: DOMAIN,
                schema: "tick-consensus-record.v1",
                value: v,
            },
            memory,
            work,
            cancelled,
            max_bytes,
            consume,
        ),
        RuntimeHistoryOriginalRefV1::RejectionAudit(v) => run(
            &ResultEnvelope {
                domain: DOMAIN,
                schema: "tick-consensus-rejection-audit.v1",
                value: v,
            },
            memory,
            work,
            cancelled,
            max_bytes,
            consume,
        ),
    }
}

/// Canonical type evidence only. A candidate fact join is still required;
/// this type does not authorize execution or certify a tick certificate.
pub struct RuntimeHistoryOriginalBindingV1 {
    lane: crate::incremental_runtime_history_manifest::RuntimeHistoryLaneV1,
    id: [u8; 32],
    bytes: u64,
    sequence: u64,
}
impl RuntimeHistoryOriginalBindingV1 {
    pub fn lane(&self) -> crate::incremental_runtime_history_manifest::RuntimeHistoryLaneV1 {
        self.lane
    }
    pub fn original_id(&self) -> [u8; 32] {
        self.id
    }
    pub fn original_bytes(&self) -> u64 {
        self.bytes
    }
    /// Event ID, tick consensus height, audit consensus height, or snapshot journal length.
    pub fn sequence(&self) -> u64 {
        self.sequence
    }
}
pub fn measure_runtime_history_original_v1<C: FnMut() -> bool>(
    value: RuntimeHistoryOriginalRefV1<'_>,
    memory: &MemoryBudget,
    work: &mut CanonicalCborWorkV1,
    cancelled: &mut C,
    max_bytes: usize,
) -> Result<
    oasis7_proto::memory_budget::ReservedValue<RuntimeHistoryOriginalBindingV1>,
    BodyMutationError,
> {
    use crate::incremental_runtime_history_manifest::RuntimeHistoryLaneV1 as Lane;
    use oasis7_proto::memory_budget::MemoryClass;
    let charge = memory
        .reserve(
            std::mem::size_of::<RuntimeHistoryOriginalBindingV1>() + 512,
            MemoryClass::Ordinary,
        )
        .map_err(oasis7_formal_protocol::incremental_execution::ExecutionError::Memory)?;
    fn measured<S: serde::Serialize + ?Sized, C: FnMut() -> bool>(
        value: &S,
        lane: Lane,
        sequence: u64,
        memory: &MemoryBudget,
        work: &mut CanonicalCborWorkV1,
        cancelled: &mut C,
        max: usize,
    ) -> Result<RuntimeHistoryOriginalBindingV1, BodyMutationError> {
        let measured = original::measure_serializable(value, memory, work, cancelled, max)?;
        let producer = original::Producer::new(measured, memory, work, cancelled);
        Ok(RuntimeHistoryOriginalBindingV1 {
            lane,
            id: producer.original_id(),
            bytes: producer.original_bytes(),
            sequence,
        })
    }
    let result = match value {
        RuntimeHistoryOriginalRefV1::Event(v) => {
            measured(v, Lane::Journal, v.id, memory, work, cancelled, max_bytes)?
        }
        RuntimeHistoryOriginalRefV1::Snapshot(v) => measured(
            &ResultEnvelope {
                domain: DOMAIN,
                schema: "snapshot-record.v1",
                value: v,
            },
            Lane::Snapshot,
            u64::try_from(v.journal_len)
                .map_err(|_| BodyMutationError::Invalid("snapshot journal length"))?,
            memory,
            work,
            cancelled,
            max_bytes,
        )?,
        RuntimeHistoryOriginalRefV1::Tick(v) => measured(
            &ResultEnvelope {
                domain: DOMAIN,
                schema: "tick-consensus-record.v1",
                value: v,
            },
            Lane::Tick,
            v.certificate.consensus_height,
            memory,
            work,
            cancelled,
            max_bytes,
        )?,
        RuntimeHistoryOriginalRefV1::RejectionAudit(v) => measured(
            &ResultEnvelope {
                domain: DOMAIN,
                schema: "tick-consensus-rejection-audit.v1",
                value: v,
            },
            Lane::Audit,
            v.consensus_height,
            memory,
            work,
            cancelled,
            max_bytes,
        )?,
    };
    if cancelled() {
        return Err(oasis7_formal_protocol::incremental_value::ValueTreeError::Cancelled.into());
    }
    Ok(charge.retain(result))
}
