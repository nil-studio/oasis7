//! Complete GEN source Result originals from the same immutable World and P.
//! Runtime occurrences retain their actual order; nonempty Cognition history
//! requires its own original migration source and is explicitly refused here.
use super::*;
use oasis7_distfs::incremental_page_store::{PageIoBudget, ValuePageSession};
use oasis7_formal_protocol::{
    controlled_authority::formal::bounded_context::WorkIdentityV1,
    incremental_material::FactLocationV1,
    incremental_state::StatePageWriter,
    incremental_value::{ValueIoBudget, ValueNodeReader, ValueNodeWriter, ValueTreeError},
};
use oasis7_proto::memory_budget::MemoryBudget;
use oasis7_runtime::World;
use oasis7_world_state::{
    incremental_cognition_history::CognitionHistoryIndexWorkV1,
    incremental_cognition_history_hash_outboard::CognitionHeadHashWorkV1,
    incremental_cognition_proof_ledger::ProofLedgerWriteWorkV1,
    incremental_field_member::{MemberRecordError, MemberRecordLimitsV1, MemberRecordWriteContext},
    incremental_runtime_history_index::{
        RuntimeHistoryIndexRowV1, RuntimeHistoryIndexWorkV1, prepare_runtime_history_index_v1,
    },
    incremental_runtime_history_manifest::{
        RuntimeHistoryLaneV1, RuntimeHistoryManifestV1, RuntimeHistoryWindowV1,
    },
    incremental_semantic_key::SemanticKeyBudget,
};
use std::sync::Arc;

#[path = "formal_iws_genesis_source_cognition.rs"]
mod cognition;

/// One caller-owned finite allowance. It is moved into the producer once;
/// physical fanout, index writes, proof JSON and head caches share its Core loan.
pub(crate) struct GenesisSourceHistoryWorkV1 {
    pub physical: [PageIoBudget; 3],
    pub values: ValueIoBudget,
    pub keys: SemanticKeyBudget,
    pub limits: MemberRecordLimitsV1,
    pub runtime: RuntimeHistoryIndexWorkV1,
    pub cognition: CognitionHistoryIndexWorkV1,
    pub hashes: CognitionHeadHashWorkV1,
    pub proof: ProofLedgerWriteWorkV1,
}

pub(super) struct Original {
    pub(super) reference: OriginalArtifactRefV1,
    pub(super) wire: ReservedValue<Vec<u8>>,
}

pub(super) struct PaidGenesisSourceHistoryV1 {
    runtime: Original,
    cognition: Original,
    proof: Option<Original>,
    core: WorkIdentityV1,
    allocation: MemoryReservation,
}
impl PaidGenesisSourceHistoryV1 {
    pub(super) fn originals(&self) -> impl Iterator<Item = &Original> {
        [&self.runtime, &self.cognition]
            .into_iter()
            .chain(self.proof.as_ref())
    }
    pub(super) fn result_count(&self) -> usize {
        2 + usize::from(self.proof.is_some())
    }
    pub(super) fn validate_context(&self, common: &mut FormalBoundedContextV1<'_>) -> Result<()> {
        common
            .charge(1, size_of::<Self>() as u64)
            .map_err(context_error)?;
        if self.core != common.work().identity() || !self.allocation.uses_budget(common.memory()) {
            return Err(PageStoreError::Corrupt("GEN source History context").into());
        }
        for original in self.originals() {
            common.consume_steps(1).map_err(context_error)?;
            if !original.wire.uses_budget(common.memory())
                || original.reference.byte_len != original.wire.len() as u64
            {
                return Err(PageStoreError::Corrupt("GEN source History original owner").into());
            }
        }
        Ok(())
    }
}

fn member(error: MemberRecordError) -> Error {
    Error::Body(BodyMutationError::Original(Box::new(error)))
}

struct Pages<'a> {
    sessions: [ValuePageSession<'a>; 3],
}
impl ValueNodeWriter for Pages<'_> {
    fn put_node(
        &mut self,
        id: [u8; 32],
        bytes: ReservedValue<Vec<u8>>,
    ) -> std::result::Result<(), ValueTreeError> {
        // All three namespaces use the original admitted pool. Publish the
        // original paid allocation by borrow, without an unmetered copy.
        for session in &mut self.sessions {
            session.put_node_borrowed(id, &bytes)?;
        }
        Ok(())
    }
}
impl ValueNodeReader for Pages<'_> {
    fn read_node(
        &self,
        id: &[u8; 32],
        max: usize,
        memory: &MemoryBudget,
        class: MemoryClass,
    ) -> std::result::Result<Arc<ReservedValue<Vec<u8>>>, ValueTreeError> {
        self.sessions[0].read_node(id, max, memory, class)
    }
}
impl StatePageWriter for Pages<'_> {}

const STEP_LANES: usize = 13;
const BYTE_LANES: usize = 10;
const LANES: usize = STEP_LANES + BYTE_LANES;
fn counters(work: &GenesisSourceHistoryWorkV1) -> [u64; LANES] {
    [
        work.physical[0].reads_remaining,
        work.physical[0].writes_remaining,
        work.physical[1].reads_remaining,
        work.physical[1].writes_remaining,
        work.physical[2].reads_remaining,
        work.physical[2].writes_remaining,
        work.values.nodes_remaining,
        work.runtime.cells_remaining,
        work.cognition.cells_remaining,
        work.hashes.hash_nodes_remaining,
        work.proof.cells_remaining,
        work.proof.json.cells_remaining,
        work.proof.json.json_nodes_remaining,
        work.physical[0].read_bytes_remaining,
        work.physical[0].write_bytes_remaining,
        work.physical[1].read_bytes_remaining,
        work.physical[1].write_bytes_remaining,
        work.physical[2].read_bytes_remaining,
        work.physical[2].write_bytes_remaining,
        work.values.read_bytes_remaining,
        work.values.write_bytes_remaining,
        work.keys.bytes_remaining,
        work.hashes.input_bytes_remaining,
    ]
}
fn install(work: &mut GenesisSourceHistoryWorkV1, v: [u64; LANES]) {
    for i in 0..3 {
        work.physical[i].reads_remaining = v[2 * i];
        work.physical[i].writes_remaining = v[2 * i + 1];
        work.physical[i].read_bytes_remaining = v[13 + 2 * i];
        work.physical[i].write_bytes_remaining = v[14 + 2 * i];
    }
    work.values.nodes_remaining = v[6];
    work.runtime.cells_remaining = v[7];
    work.cognition.cells_remaining = v[8];
    work.hashes.hash_nodes_remaining = v[9];
    work.proof.cells_remaining = v[10];
    work.proof.json.cells_remaining = v[11];
    work.proof.json.json_nodes_remaining = v[12];
    work.values.read_bytes_remaining = v[19];
    work.values.write_bytes_remaining = v[20];
    work.keys.bytes_remaining = v[21];
    work.hashes.input_bytes_remaining = v[22];
}
fn metered<T>(
    common: &mut FormalBoundedContextV1<'_>,
    work: &mut GenesisSourceHistoryWorkV1,
    run: impl FnOnce(&mut GenesisSourceHistoryWorkV1) -> Result<T>,
) -> Result<T> {
    common
        .charge(4 * LANES as u64, 8192)
        .map_err(context_error)?;
    let _frame = common
        .memory()
        .reserve(8192, MemoryClass::Ordinary)
        .map_err(PageStoreError::Memory)?;
    let before = counters(work);
    let lease = common.work().begin_child().map_err(context_error)?;
    let steps = lease.remaining_steps() / STEP_LANES as u64;
    let bytes = lease.remaining_read_bytes() / BYTE_LANES as u64;
    let capped = std::array::from_fn(|i| before[i].min(if i < STEP_LANES { steps } else { bytes }));
    install(work, capped);
    let result = match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| run(work))) {
        Ok(value) => value,
        Err(payload) => {
            install(work, [0; LANES]);
            drop(lease);
            std::panic::resume_unwind(payload)
        }
    };
    let after = counters(work);
    if (0..LANES).any(|i| after[i] > capped[i]) {
        install(work, [0; LANES]);
        drop(lease);
        return Err(PageStoreError::Corrupt("GEN source History lane increased").into());
    }
    let used: [u64; LANES] = std::array::from_fn(|i| capped[i] - after[i]);
    install(work, std::array::from_fn(|i| before[i] - used[i]));
    let debit = lease
        .consume(
            used[..STEP_LANES].iter().sum(),
            used[STEP_LANES..].iter().sum(),
        )
        .map_err(context_error);
    lease.finish();
    match result {
        Err(error) => Err(error),
        Ok(value) => debit.map(|()| value),
    }
}

fn original(
    bytes: &[u8],
    objects: &[IncrementalObjectStore<'_>; 3],
    transfer: &mut IwsGenesisTransferWorkV1,
    common: &mut FormalBoundedContextV1<'_>,
    stop: &AtomicBool,
) -> Result<Original> {
    common
        .charge(1, bytes.len() as u64)
        .map_err(context_error)?;
    let fee = common
        .memory()
        .reserve(bytes.len(), MemoryClass::Ordinary)
        .map_err(PageStoreError::Memory)?;
    let mut raw = Vec::new();
    raw.try_reserve_exact(bytes.len())
        .map_err(|_| PageStoreError::ResourceLimited("GEN source History wire allocation"))?;
    if raw.capacity() != bytes.len() {
        return Err(PageStoreError::Corrupt("GEN source History wire capacity").into());
    }
    raw.extend_from_slice(bytes);
    let wire = fee.retain(raw);
    let reference = write_original(objects, &wire, None, transfer, common, stop)?;
    Ok(Original { reference, wire })
}

pub(super) fn prepare(
    world: &World,
    business: &ProvisionalProjectedGenesisBusinessV1,
    stores: [&IncrementalPageStore; 3],
    mut work: GenesisSourceHistoryWorkV1,
    transfer: &mut IwsGenesisTransferWorkV1,
    common: &mut FormalBoundedContextV1<'_>,
    stop: &AtomicBool,
) -> Result<PaidGenesisSourceHistoryV1> {
    business.validate_context(common)?;
    common.charge(64 << 10, 64 << 10).map_err(context_error)?;
    let _phase = common
        .memory()
        .reserve(64 << 10, MemoryClass::Ordinary)
        .map_err(PageStoreError::Memory)?;
    let allocation = common
        .memory()
        .reserve(
            size_of::<PaidGenesisSourceHistoryV1>(),
            MemoryClass::Ordinary,
        )
        .map_err(PageStoreError::Memory)?;
    if stores
        .iter()
        .any(|store| !store.memory_budget().same_pool(common.memory()))
    {
        return Err(PageStoreError::Corrupt("GEN source History fanout pool").into());
    }
    let projected = business.projected();
    let snapshots = projected
        .snapshot_originals_bounded_v1(common)
        .map_err(Error::Body)?;
    let journal = projected
        .journal_originals_bounded_v1(common)
        .map_err(Error::Body)?;
    let (ticks, archived) = projected
        .hot_tick_originals_bounded_v1(common)
        .map_err(Error::Body)?;
    let audits = projected
        .retained_audit_originals_bounded_v1(common)
        .map_err(Error::Body)?;
    if archived != 0 {
        return Err(PageStoreError::Corrupt(
            "GEN archived Runtime history requires its actual original source",
        )
        .into());
    }
    let enabled = world
        .borrow_iws_cognition_history_source_v1()
        .map_err(ValueTreeError::Invalid)?
        .enabled();
    // Input, Runtime manifest, Cognition view, then the enabled ProofLedger.
    // Runtime original occurrences follow in exactly the FactSet order below.
    let prefix = 3usize + usize::from(enabled);
    let tick_start = prefix
        .checked_add(snapshots.len())
        .ok_or(PageStoreError::ResourceLimited("GEN Runtime fact offset"))?;
    let audit_start = tick_start
        .checked_add(ticks.len())
        .ok_or(PageStoreError::ResourceLimited("GEN Runtime fact offset"))?;
    let journal_start = audit_start
        .checked_add(audits.len())
        .ok_or(PageStoreError::ResourceLimited("GEN Runtime fact offset"))?;
    let rows = [
        snapshots.as_slice(),
        journal.as_slice(),
        ticks.as_slice(),
        audits.as_slice(),
    ];
    let offsets = [prefix, journal_start, tick_start, audit_start];
    let roots_id = business.roots_original().0.id;
    let memory_owner = common.memory().clone();
    let memory = &memory_owner;
    let (runtime, cognition, proof) = metered(common, &mut work, |work| {
        let mut pages = Pages {
            sessions: std::array::from_fn(|i| {
                stores[i].value_session_with_cancelled(work.physical[i].clone(), stop)
            }),
        };
        let produced = (|| {
            let mut windows = Vec::new();
            // The phase reservation above admits this fixed four-element heap
            // carrier before allocation. Each row is streamed directly from P.
            windows
                .try_reserve_exact(4)
                .map_err(|_| PageStoreError::ResourceLimited("GEN Runtime lane allocation"))?;
            if windows.capacity() != 4 {
                return Err(PageStoreError::Corrupt("GEN Runtime lane capacity").into());
            }
            for (slot, lane) in RuntimeHistoryLaneV1::ALL.into_iter().enumerate() {
                let count = u64::try_from(rows[slot].len())
                    .map_err(|_| PageStoreError::ResourceLimited("GEN Runtime lane count"))?;
                let root = prepare_runtime_history_index_v1(
                    lane,
                    count,
                    rows[slot].iter().enumerate().map(|(index, original)| {
                        let ordinal = offsets[slot]
                            .checked_add(index)
                            .and_then(|n| u64::try_from(n).ok())
                            .ok_or(ValueTreeError::Invalid("GEN Runtime fact ordinal"))?;
                        Ok(RuntimeHistoryIndexRowV1 {
                            location: FactLocationV1 {
                                height: 0,
                                ordinal,
                                original: original.id,
                            },
                            encoded_bytes: original.byte_len,
                        })
                    }),
                    &mut MemberRecordWriteContext {
                        sink: &mut pages,
                        memory,
                        io: &mut work.values,
                        key_work: &mut work.keys,
                        limits: work.limits,
                        cancelled: || stop.load(Ordering::Acquire),
                    },
                    &mut work.runtime,
                )
                .map_err(member)?;
                windows.push(RuntimeHistoryWindowV1 {
                    root,
                    total: count,
                    start: 0,
                    count,
                });
            }
            let runtime = RuntimeHistoryManifestV1 {
                roots: roots_id,
                previous_view: None,
                lanes: windows
                    .try_into()
                    .map_err(|_| PageStoreError::Corrupt("GEN Runtime lane arity"))?,
                max_snapshots: world.snapshot_catalog().retention.max_snapshots as u64,
                archived_tick_count: archived,
            };
            let cognition = cognition::prepare(world, roots_id, &mut pages, memory, work, stop)?;
            let proof = world
                .prepare_iws_cognition_proof_ledger_genesis_v1(
                    projected.roots(),
                    &mut MemberRecordWriteContext {
                        sink: &mut pages,
                        memory,
                        io: &mut work.values,
                        key_work: &mut work.keys,
                        limits: work.limits,
                        cancelled: || stop.load(Ordering::Acquire),
                    },
                    &mut work.proof,
                )
                .map_err(member)?;
            if proof.is_some() != enabled {
                return Err(PageStoreError::Corrupt("GEN actual Cognition ledger presence").into());
            }
            Ok((runtime, cognition, proof))
        })();
        // Copy only actual remaining physical counters back, on every return.
        // A poisoned session makes reconciliation fail closed for this Core.
        for (i, session) in pages.sessions.iter().enumerate() {
            work.physical[i] = match session.remaining_io() {
                Ok(io) => io,
                Err(error) => {
                    install(work, [0; LANES]);
                    return Err(error.into());
                }
            };
        }
        produced
    })?;
    // Fixed canonical codecs have bounded shape; pay their full metadata/hash
    // work before use, separately from the actual three-way object persistence.
    common.charge(64 << 10, 64 << 10).map_err(context_error)?;
    let objects = [
        open_store(stores[0], 0, transfer, common, stop)?,
        open_store(stores[1], 1, transfer, common, stop)?,
        open_store(stores[2], 2, transfer, common, stop)?,
    ];
    let runtime = original(
        &runtime.canonical_bytes()?,
        &objects,
        transfer,
        common,
        stop,
    )?;
    let cognition = original(cognition.as_slice(), &objects, transfer, common, stop)?;
    let proof = proof
        .map(|proof| {
            let wire = proof.canonical_bytes()?;
            original(&wire, &objects, transfer, common, stop)
        })
        .transpose()?;
    let output = PaidGenesisSourceHistoryV1 {
        runtime,
        cognition,
        proof,
        core: common.work().identity(),
        allocation,
    };
    output.validate_context(common)?;
    Ok(output)
}
