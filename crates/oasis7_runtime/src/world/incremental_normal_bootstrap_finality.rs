//! Actual accepted Tick-lane finality for the fixed Bootstrap consumer.
//! Original bytes, their decoded graph and their admission stay owned here.
use super::{
    incremental_body::BodyMutationError,
    incremental_factory_phase::io::{FactoryPhaseMeterLaneV1, FactoryPhaseMeterLanesV1},
    incremental_module_invocation::*,
    incremental_normal_sources::BoundNormalTickParentV1,
};
use oasis7_consensus::{
    node_consensus_action::NodeConsensusAction, normal_tick_input::NodeExecutionCommitContext,
};
use oasis7_formal_protocol::{
    controlled_authority::formal::{
        bounded_cbor_data::{self as cbor, CborDataLayoutV1, CborDataLimitsV1, PaidCborDataV1},
        bounded_context::{FormalBoundedContextV1, WorkIdentityV1},
        context_wire,
    },
    incremental_execution::{ExecutionFactKindV1, ExecutionFence},
    incremental_material::{FactBudget, OriginalObjectStreamReader},
    incremental_state::{MerkleNodeReader, StateNamespaceV1, StatePageWriter},
    incremental_value::{ValueNodeReader, ValueNodeWriter, ValueReadContext, ValueTreeError},
};
use oasis7_proto::memory_budget::{MemoryClass, MemoryReservation, ReservedValue};
use oasis7_world_state::{
    incremental_field_read::WorldFieldReadContextV1,
    incremental_runtime_history_decode::{
        DecodedRuntimeHistoryOriginalV1, RuntimeHistoryDecodeKindV1, RuntimeHistoryDecodeWorkV1,
        decode_runtime_history_original_v1,
    },
    incremental_runtime_history_index::{
        RuntimeHistoryIndexWorkV1, read_runtime_history_index_point_v1,
    },
    incremental_runtime_history_manifest::RuntimeHistoryLaneV1,
};
use sha2::{Digest, Sha256};
use std::{borrow::Borrow, sync::Arc};
pub struct CurrentBootstrapFinalityWorkV1 {
    pub index: RuntimeHistoryIndexWorkV1,
    pub decode: RuntimeHistoryDecodeWorkV1,
    pub originals: FactBudget,
    pub input_limits: CborDataLimitsV1,
}
impl FactoryPhaseMeterLanesV1 for CurrentBootstrapFinalityWorkV1 {
    fn visit_lanes(&mut self, v: &mut dyn for<'a> FnMut(FactoryPhaseMeterLaneV1<'a>)) {
        use FactoryPhaseMeterLaneV1::{Bytes as B, Steps as S};
        v(S(&mut self.index.cells_remaining));
        v(S(&mut self.decode.structure.values_remaining));
        v(B(&mut self.decode.structure.bytes_remaining));
        v(S(&mut self.decode.canonical.visits_remaining));
        v(B(&mut self.decode.canonical.bytes_remaining));
        v(S(&mut self.originals.facts_remaining));
        v(B(&mut self.originals.original_bytes_remaining));
    }
}
pub(in crate::world) struct CurrentBootstrapLiveFinalityV1 {
    parent: [u8; 32],
    fence: ExecutionFence,
    guard: oasis7_formal_protocol::incremental_execution::PreparedGuard,
    core: WorkIdentityV1,
    epoch: Option<u64>,
    legacy_invalid: bool,
    _raw: Option<Arc<ReservedValue<Vec<u8>>>>,
    _tick: Option<ReservedValue<DecodedRuntimeHistoryOriginalV1>>,
    _input_raw: Option<Arc<ReservedValue<Vec<u8>>>>,
    _input: Option<PaidCborDataV1<NodeExecutionCommitContext>>,
    fee: MemoryReservation,
}
fn bad() -> BodyMutationError {
    BodyMutationError::Invalid("Bootstrap actual accepted Tick finality")
}
fn formal<E: std::error::Error + Send + Sync + 'static>(e: E) -> BodyMutationError {
    BodyMutationError::Original(Box::new(e))
}
#[derive(Debug)]
pub(in crate::world) enum CurrentBootstrapFinalityErrorV1 {
    Source(BodyMutationError),
    World(ReservedValue<crate::WorldError>),
}
impl From<BodyMutationError> for CurrentBootstrapFinalityErrorV1 {
    fn from(error: BodyMutationError) -> Self {
        Self::Source(error)
    }
}
fn deny(reason: &str, common: &mut FormalBoundedContextV1<'_>) -> CurrentBootstrapFinalityErrorV1 {
    let bytes = std::mem::size_of::<crate::WorldError>()
        + reason.len()
        + std::mem::size_of::<CurrentBootstrapFinalityErrorV1>();
    if let Err(e) = common.charge(1, bytes as u64) {
        return CurrentBootstrapFinalityErrorV1::Source(formal(e));
    }
    match common.memory().reserve(bytes, MemoryClass::Ordinary) {
        Err(e) => CurrentBootstrapFinalityErrorV1::Source(ValueTreeError::Memory(e).into()),
        Ok(fee) => CurrentBootstrapFinalityErrorV1::World(fee.retain(
            crate::WorldError::CapabilityAuthorizationDenied {
                reason: reason.to_owned(),
            },
        )),
    }
}
impl CurrentBootstrapLiveFinalityV1 {
    pub(in crate::world) fn validate_audience_v1(
        &self,
        parent: &BoundNormalTickParentV1<'_>,
        epoch: u64,
        common: &mut FormalBoundedContextV1<'_>,
        live: &ExecutionFence,
    ) -> Result<(), CurrentBootstrapFinalityErrorV1> {
        parent.validate_context(common).map_err(formal)?;
        common.charge(1, 256).map_err(formal)?;
        if self.parent != parent.refs().head.verified_record_id()
            || self.core != common.work().identity()
            || !self.fee.uses_budget(common.memory())
            || *live != parent.fence()
        {
            return Err(bad().into());
        }
        self.validate_epoch_v1(epoch, common)
    }
    /// Same accepted History proof used by the original Current Bootstrap
    /// consumer. A provisional tick/cut cannot construct this owner.
    pub(in crate::world) fn validate_projection_context_v1<M: MerkleNodeReader>(
        &self,
        tx: &oasis7_formal_protocol::incremental_execution::ExecutionTransaction<'_, M>,
        common: &mut FormalBoundedContextV1<'_>,
        live: &ExecutionFence,
    ) -> Result<(), BodyMutationError> {
        tx.validate_read_source_guard(&self.guard, live)?;
        common.charge(2, 256).map_err(formal)?;
        if tx.parent_fence() != self.fence
            || *live != self.fence
            || self.core != common.work().identity()
            || !self.fee.uses_budget(common.memory())
        {
            return Err(bad());
        }
        Ok(())
    }
    pub(in crate::world) fn validate_projection_audience_v1<M: MerkleNodeReader>(
        &self,
        tx: &oasis7_formal_protocol::incremental_execution::ExecutionTransaction<'_, M>,
        epoch: u64,
        common: &mut FormalBoundedContextV1<'_>,
        live: &ExecutionFence,
    ) -> Result<(), CurrentBootstrapFinalityErrorV1> {
        self.validate_projection_context_v1(tx, common, live)?;
        self.validate_epoch_v1(epoch, common)
    }
    fn validate_epoch_v1(
        &self,
        epoch: u64,
        common: &mut FormalBoundedContextV1<'_>,
    ) -> Result<(), CurrentBootstrapFinalityErrorV1> {
        if self.legacy_invalid {
            return Err(deny("capability live finality record is invalid", common));
        }
        if self.epoch.is_some_and(|chain| chain < epoch) {
            return Err(deny(
                "capability finality epoch is ahead of live chain",
                common,
            ));
        }
        Ok(())
    }
}
/// fmt emits borrowed strings and stack-formatted scalars. Each actual
/// payload chunk is paid before SHA sees it; the first typed work error wins.
pub(in crate::world) fn legacy_tick_digest(
    block: &oasis7_world_state::TickBlock,
    common: &mut FormalBoundedContextV1<'_>,
) -> Result<[u8; 32], BodyMutationError> {
    struct Sink<'a, 'b> {
        common: &'a mut FormalBoundedContextV1<'b>,
        hash: Sha256,
        error: Option<oasis7_formal_protocol::controlled_authority::formal::FormalError>,
    }
    impl std::fmt::Write for Sink<'_, '_> {
        fn write_str(&mut self, value: &str) -> std::fmt::Result {
            for chunk in value.as_bytes().chunks(4096) {
                if let Err(error) = self.common.charge(1, chunk.len() as u64) {
                    self.error = Some(error);
                    return Err(std::fmt::Error);
                }
                self.hash.update(chunk);
            }
            Ok(())
        }
    }
    common
        .charge(1, std::mem::size_of::<Sink<'_, '_>>() as u64)
        .map_err(formal)?;
    let mut sink = Sink {
        common,
        hash: Sha256::new(),
        error: None,
    };
    if block.write_hash_payload(&mut sink).is_err() {
        return Err(sink.error.map(formal).unwrap_or_else(bad));
    }
    sink.common.charge(1, 64).map_err(formal)?;
    Ok(sink.hash.finalize().into())
}

struct RawTick {
    raw: Arc<ReservedValue<Vec<u8>>>,
    tick: ReservedValue<DecodedRuntimeHistoryOriginalV1>,
    location: oasis7_formal_protocol::incremental_material::FactLocationV1,
}
impl<M, R, E, C, W, F, O> RootModuleExecutionContextV1<'_, '_, '_, '_, M, R, E, C, W, F, O>
where
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    W: ValueNodeReader + ValueNodeWriter + StatePageWriter,
    F: FnMut(
        &mut dyn crate::world::incremental_body_event::AgentEventOriginalV1,
    ) -> Result<(), BodyMutationError>,
    O: OriginalObjectStreamReader,
{
    pub(in crate::world) fn prepare_current_bootstrap_finality_v1(
        &mut self,
        parent: &BoundNormalTickParentV1<'_>,
        work: &mut CurrentBootstrapFinalityWorkV1,
        common: &mut FormalBoundedContextV1<'_>,
        live: &mut dyn FnMut() -> ExecutionFence,
    ) -> Result<CurrentBootstrapLiveFinalityV1, RootModuleExecutionErrorV1<O::Error>> {
        parent.validate_context(common).map_err(formal)?;
        common.charge(1, 4096).map_err(formal)?;
        let fee = common
            .memory()
            .reserve(
                std::mem::size_of::<CurrentBootstrapLiveFinalityV1>() + 4096,
                MemoryClass::Ordinary,
            )
            .map_err(ValueTreeError::Memory)?;
        let history = parent.refs().runtime;
        let window = &history.manifest().lanes[RuntimeHistoryLaneV1::Tick.index()];
        if history.history_index(RuntimeHistoryLaneV1::Tick).window() != window
            || self.ctx.transaction.parent_fence() != parent.fence()
        {
            return Err(bad().into());
        }
        if window.total == 0 {
            if parent.refs().core.transition().is_some() {
                return Err(bad().into());
            }
            return Ok(CurrentBootstrapLiveFinalityV1 {
                parent: parent.refs().head.verified_record_id(),
                fence: parent.fence(),
                guard: self.ctx.transaction.guard(),
                core: common.work().identity(),
                epoch: None,
                legacy_invalid: false,
                _raw: None,
                _tick: None,
                _input_raw: None,
                _input: None,
                fee,
            });
        }
        let raw = self.with_factory_prologue_external_meter_v1(
            common,
            live,
            work,
            |root, live, work| {
                let mut read = WorldFieldReadContextV1 {
                    root: parent.refs().roots.state_root(StateNamespaceV1::Artifacts),
                    state: root.state,
                    values: ValueReadContext {
                        source: root.ctx.values.source,
                        memory: root.ctx.values.memory,
                        class: root.ctx.values.class,
                        io: &mut *root.ctx.values.io,
                    },
                    key_work: &mut *root.ctx.key_work,
                    semantic_bytes: root.ctx.semantic_bytes,
                    record_bytes: root.ctx.record_bytes,
                    take_state_error: &mut root.ctx.take_state_error,
                    cancelled: &mut root.ctx.cancelled,
                };
                let row = read_runtime_history_index_point_v1(
                    RuntimeHistoryLaneV1::Tick,
                    window,
                    window.total - 1,
                    &mut read,
                    &mut work.index,
                )?;
                work.originals.facts_remaining =
                    work.originals.facts_remaining.checked_sub(1).ok_or(
                        ValueTreeError::ResourceLimited("Bootstrap Tick original count"),
                    )?;
                if row.encoded_bytes > work.originals.max_fact_bytes
                    || row.encoded_bytes > work.originals.original_bytes_remaining
                    || row.encoded_bytes > work.decode.max_original_bytes as u64
                {
                    return Err(
                        ValueTreeError::ResourceLimited("Bootstrap Tick original bytes").into(),
                    );
                }
                let max = usize::try_from(row.encoded_bytes).map_err(|_| bad())?;
                let raw = root
                    .originals
                    .read_original(&row.location.original, max, root.ctx.values.memory)
                    .map_err(RootModuleExecutionErrorV1::ArtifactOriginalStorage)?;
                if !raw.uses_budget(root.ctx.values.memory) || raw.len() != max {
                    return Err(bad().into());
                }
                work.originals.original_bytes_remaining -= row.encoded_bytes;
                work.decode.structure.bytes_remaining = work
                    .decode
                    .structure
                    .bytes_remaining
                    .checked_sub(row.encoded_bytes)
                    .ok_or(ValueTreeError::ResourceLimited("Bootstrap Tick hash bytes"))?;
                let mut hash = blake3::Hasher::new();
                for chunk in raw.chunks(4096) {
                    if (root.ctx.cancelled)() {
                        return Err(ValueTreeError::Cancelled.into());
                    }
                    work.decode.structure.values_remaining = work
                        .decode
                        .structure
                        .values_remaining
                        .checked_sub(1)
                        .ok_or(ValueTreeError::ResourceLimited(
                            "Bootstrap Tick hash chunks",
                        ))?;
                    hash.update(chunk);
                }
                if hash.finalize().as_bytes() != &row.location.original {
                    return Err(bad().into());
                }
                let tick = decode_runtime_history_original_v1(
                    RuntimeHistoryDecodeKindV1::TickFamily,
                    &raw,
                    root.ctx.values.memory,
                    MemoryClass::Ordinary,
                    &mut work.decode,
                    &mut root.ctx.cancelled,
                )?;
                root.ctx
                    .transaction
                    .validate_guard(&root.ctx.transaction.guard(), &live())?;
                Ok(RawTick {
                    raw,
                    tick,
                    location: row.location,
                })
            },
        )?;
        let mut input_raw = None;
        let mut input_graph = None;
        let mut legacy_invalid = false;
        let epoch = match &*raw.tick {
            DecodedRuntimeHistoryOriginalV1::Tick(tick) => {
                // Stream the unique legacy preimage through the same Common work
                // instead of allocating and hashing an uninterruptible format! buffer.
                legacy_invalid = if tick.certificate.threshold == 0 {
                    true
                } else {
                    let digest = legacy_tick_digest(&tick.block, common)?;
                    let mut expected = [0u8; 64];
                    hex::encode_to_slice(digest, &mut expected).map_err(|_| bad())?;
                    common.charge(1, 64).map_err(formal)?;
                    tick.certificate.block_hash.as_bytes() != expected
                };
                tick.block.header.chain_epoch
            }
            DecodedRuntimeHistoryOriginalV1::IwsTick(tick) => {
                common
                    .charge(32, raw.raw.len() as u64 + 8192)
                    .map_err(formal)?;
                let _phase = common
                    .memory()
                    .reserve(8192, MemoryClass::Ordinary)
                    .map_err(ValueTreeError::Memory)?;
                tick.validate_data()?;
                let core = parent.refs().core;
                let frame = &tick.current;
                let ordinal = usize::try_from(raw.location.ordinal).map_err(|_| bad())?;
                if raw.location.height != core.roots().height
                    || !core.facts().facts.get(ordinal).is_some_and(|f| {
                        f.kind == ExecutionFactKindV1::Result && f.id == raw.location.original
                    })
                    || frame.input_original != core.original_input_id()
                    || frame.policy_original != core.original_execution_policy_id()
                    || Some(frame.parent_record) != core.parent_record_id()
                    || Some(frame.parent_body) != core.body().parent_commit
                    || frame.parent_roots != core.parent_roots().ok_or_else(bad)?.digest()?
                    || frame.context.world_id != core.roots().world_id
                    || frame.context.height != core.roots().height
                    || frame.context.action_root != parent.refs().head.record().action_root
                    || tick.tick != core.roots().simulation_tick
                    || tick.consensus_height != window.total
                {
                    return Err(bad().into());
                }
                let encoded = context_wire::encode_paid(
                    frame,
                    work.decode.max_original_bytes,
                    common,
                    "Bootstrap frame limit",
                    "Bootstrap actual IWS frame",
                )
                .map_err(formal)?;
                let mut digest = Sha256::new();
                for chunk in encoded.chunks(4096) {
                    common.charge(1, chunk.len() as u64).map_err(formal)?;
                    digest.update(chunk);
                }
                let digest: [u8; 32] = digest.finalize().into();
                if digest != tick.current_digest {
                    return Err(bad().into());
                }
                // Epoch comes from the original committed input as well as the Tick frame.
                // The authority signature epoch is a separate domain and is never used.
                let bytes = self.with_factory_prologue_external_meter_v1(
                    common,
                    live,
                    work,
                    |root, live, work| {
                        work.originals.facts_remaining =
                            work.originals.facts_remaining.checked_sub(1).ok_or(
                                ValueTreeError::ResourceLimited("Bootstrap input original count"),
                            )?;
                        let n = frame.input_bytes;
                        if n > work.originals.max_fact_bytes
                            || n > work.originals.original_bytes_remaining
                            || n > work.input_limits.max_wire_bytes as u64
                        {
                            return Err(ValueTreeError::ResourceLimited(
                                "Bootstrap input original bytes",
                            )
                            .into());
                        }
                        // Debit the complete known original length before starting its IO. The
                        // metered Root adapter also keeps every physical attempt in this child.
                        work.originals.original_bytes_remaining -= n;
                        let max = usize::try_from(n).map_err(|_| bad())?;
                        let bytes = root
                            .originals
                            .read_original(&frame.input_original, max, root.ctx.values.memory)
                            .map_err(RootModuleExecutionErrorV1::ArtifactOriginalStorage)?;
                        if !bytes.uses_budget(root.ctx.values.memory) || bytes.len() != max {
                            return Err(bad().into());
                        }
                        root.ctx
                            .transaction
                            .validate_guard(&root.ctx.transaction.guard(), &live())?;
                        Ok(bytes)
                    },
                )?;
                let mut hash = blake3::Hasher::new();
                for chunk in bytes.chunks(4096) {
                    common.charge(1, chunk.len() as u64).map_err(formal)?;
                    hash.update(chunk);
                }
                if hash.finalize().as_bytes() != &frame.input_original {
                    return Err(bad().into());
                }
                let layout = CborDataLayoutV1 {
                    root: std::mem::size_of::<NodeExecutionCommitContext>(),
                    sequence_element: std::mem::size_of::<NodeConsensusAction>(),
                    map_pair: 0,
                    set_element: 0,
                    box_pointee: 0,
                    default_heap_per_value: 0,
                    symbols_bytes: 512,
                    visits: 1,
                    missing: None,
                };
                let input = match cbor::decode_closed_cbor_data_v1::<NodeExecutionCommitContext>(
                    &bytes,
                    layout,
                    &work.input_limits,
                    common,
                )
                .map_err(formal)?
                {
                    Ok(v) => v,
                    Err(_) => return Err(bad().into()),
                };
                let value: &NodeExecutionCommitContext = input.borrow();
                let c = &frame.context;
                common
                    .charge(16, (bytes.len() + 8192) as u64)
                    .map_err(formal)?;
                if value.world_id != c.world_id
                    || value.node_id != c.node_id
                    || value.proposer_id != c.proposer_id
                    || value.height != c.height
                    || value.slot != c.slot
                    || value.epoch != c.epoch
                    || value.node_block_hash != c.node_block_hash
                    || value.action_root != c.action_root
                    || value.committed_at_unix_ms != c.committed_at_unix_ms
                {
                    return Err(bad().into());
                }
                let epoch = Some(value.epoch);
                input_raw = Some(bytes);
                input_graph = Some(input);
                epoch
            }
            _ => return Err(bad().into()),
        };
        self.ctx
            .transaction
            .validate_guard(&self.ctx.transaction.guard(), &live())?;
        Ok(CurrentBootstrapLiveFinalityV1 {
            parent: parent.refs().head.verified_record_id(),
            fence: parent.fence(),
            guard: self.ctx.transaction.guard(),
            core: common.work().identity(),
            epoch,
            legacy_invalid,
            _raw: Some(raw.raw),
            _tick: Some(raw.tick),
            _input_raw: input_raw,
            _input: input_graph,
            fee,
        })
    }
}

use oasis7_formal_protocol::incremental_material::OriginalObjectReader;
