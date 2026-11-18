//! Node boundary DATA derived from the actual installed accepted IWS owner.
//! The complete stored Qualified proof is compared with that owner's original
//! proof before a GEN or cold normal boundary can be supplied to Node.
use super::*;
use oasis7_distfs::controlled_authority::formal::endpoint::HostAcceptedParentReadViewV1;
use oasis7_formal_protocol::controlled_authority::{
    formal::{
        FormalRecordKindV1,
        bounded_cbor_data::{CborDataLayoutV1, CborDataLimitsV1, decode_closed_cbor_data_v1},
        bounded_context::{FormalBoundedContextV1, WorkIdentityV1},
        incremental::is_iws_record,
    },
    replicated_protocol::{ArtifactRole, ClosedRecord},
};
use oasis7_node::{
    NodeConsensusAction, NodeExecutionBootstrap, NodeExecutionCommitContext,
    NodeExecutionGenesisBootstrap,
};
use oasis7_proto::memory_budget::MemoryReservation;
use std::borrow::Borrow;

enum Boundary {
    Genesis(NodeExecutionGenesisBootstrap),
    Normal(NodeExecutionBootstrap),
}

/// Only borrowed Node DATA is exposed. This owner cannot install a Host, mint
/// a Node-issued continuation, or substitute for an accepted resident source.
pub(crate) struct PaidIwsNodeBoundaryV1 {
    value: Boundary,
    record: [u8; 32],
    fence: ExecutionFence,
    namespaces: ((u64, u64), (u64, u64)),
    core: WorkIdentityV1,
    _fee: MemoryReservation,
}
impl PaidIwsNodeBoundaryV1 {
    pub(crate) fn boundary(&self) -> &NodeExecutionBootstrap {
        match &self.value {
            Boundary::Genesis(value) => value.boundary(),
            Boundary::Normal(value) => value,
        }
    }
    pub(crate) fn genesis(&self) -> Option<&NodeExecutionGenesisBootstrap> {
        match &self.value {
            Boundary::Genesis(value) => Some(value),
            Boundary::Normal(_) => None,
        }
    }
    pub(crate) fn validate_current_bounded(
        &self,
        host: &IwsHostRuntimeV1,
        common: &mut FormalBoundedContextV1<'_>,
    ) -> Result<(), IwsHostRuntimeErrorV1> {
        common.charge(1, 192)?;
        let current = host.current.current().ok_or_else(invalid)?;
        current.source.validate_context(common)?;
        if self.core != common.work().identity()
            || !self._fee.uses_budget(common.memory())
            || self.record != current.source.record_id()
            || self.fence != current.source.fence()
            || self.namespaces
                != (
                    current.source.page_namespace_identity(),
                    current.source.native_namespace_identity(),
                )
        {
            return Err(invalid().into());
        }
        Ok(())
    }
}

fn invalid() -> FormalError {
    FormalError::Invalid("Node boundary requires the exact accepted IWS source".into())
}

fn text(source: &str, common: &mut FormalBoundedContextV1<'_>) -> Result<String, FormalError> {
    // The containing fixed owner reserves the complete output before any copy.
    common.charge(1, source.len() as u64)?;
    let mut value = String::new();
    value
        .try_reserve_exact(source.len())
        .map_err(|_| invalid())?;
    if value.capacity() > source.len() {
        return Err(invalid());
    }
    // Strings were already validated by the canonical parent / closed decoder.
    value.push_str(source);
    Ok(value)
}

fn id_text(
    source: &[u8; 32],
    common: &mut FormalBoundedContextV1<'_>,
) -> Result<String, FormalError> {
    common.charge(1, 96)?;
    let mut buffer = [0u8; 64];
    hex::encode_to_slice(source, &mut buffer).map_err(|_| invalid())?;
    text(std::str::from_utf8(&buffer).map_err(|_| invalid())?, common)
}

fn context_layout() -> CborDataLayoutV1 {
    CborDataLayoutV1 {
        root: size_of::<NodeExecutionCommitContext>(),
        sequence_element: size_of::<NodeConsensusAction>(),
        map_pair: size_of::<(String, NodeConsensusAction)>(),
        set_element: 0,
        box_pointee: 0,
        default_heap_per_value: 0,
        symbols_bytes: 0,
        visits: 0,
        missing: None,
    }
}

/// Shared only by the two fixed consumers which already hold the full genuine
/// parent proof: Node bootstrap and the original Node -> Frozen producer.
pub(super) fn read_from_parent_bounded(
    parent: &HostAcceptedParentReadViewV1<'_>,
    material: &ClosedRecord,
    limits: &CborDataLimitsV1,
    common: &mut FormalBoundedContextV1<'_>,
) -> Result<PaidIwsNodeBoundaryV1, IwsHostRuntimeErrorV1> {
    common.charge(1, 320)?;
    let record = parent.head().record();
    let core = parent.core();
    let fence = parent.fence();
    if !is_iws_record(record.schema_version)
        || core.candidate_record_id() != parent.head().verified_record_id()
        || core.body().height.checked_add(1) != Some(record.position)
        || core.body().roots != fence.parent_roots
        || core.roots().height != core.body().height
    {
        return Err(invalid().into());
    }
    let mut root = [0u8; 32];
    hex::decode_to_slice(&record.after_state_root, &mut root).map_err(|_| invalid())?;
    if root != fence.parent_roots {
        return Err(invalid().into());
    }
    let initial = record.kind == FormalRecordKindV1::InitialActivation;
    let original = if initial {
        if record.position != 1
            || core.body().height != 0
            || core.body().parent_commit.is_some()
            || core.body().transition.is_some()
            || core.history().count != 1
            || core.history().previous.is_some()
            || core.parent_record_id().is_some()
        {
            return Err(invalid().into());
        }
        None
    } else {
        if record.kind != FormalRecordKindV1::Execution || core.body().height == 0 {
            return Err(invalid().into());
        }
        let input_id = material
            .roots
            .get(&ArtifactRole::Input)
            .ok_or_else(invalid)?;
        common.charge(1, input_id.len() as u64)?;
        let mut digest = [0u8; 32];
        hex::decode_to_slice(input_id, &mut digest).map_err(|_| invalid())?;
        if digest != core.original_input_id() {
            return Err(invalid().into());
        }
        let mut found = None;
        for object in &material.objects {
            common.charge(1, object.content_hash.len() as u64 + input_id.len() as u64)?;
            if object.content_hash == *input_id {
                if found.replace(object).is_some() {
                    return Err(invalid().into());
                }
            }
        }
        let bytes = &found.ok_or_else(invalid)?.bytes;
        let mut hash = blake3::Hasher::new();
        for chunk in bytes.chunks(8192) {
            common.charge(1, chunk.len() as u64)?;
            hash.update(chunk);
        }
        if hash.finalize().as_bytes() != &digest {
            return Err(invalid().into());
        }
        let decoded = decode_closed_cbor_data_v1::<NodeExecutionCommitContext>(
            bytes,
            context_layout(),
            limits,
            common,
        )
        .map_err(IwsHostRuntimeErrorV1::InputDecode)?
        .map_err(|_| invalid())?;
        let value: &NodeExecutionCommitContext = decoded.borrow();
        common.charge(
            1,
            (value.world_id.len()
                + record.world.world_id.len()
                + value.node_id.len()
                + value.proposer_id.len()
                + value.node_block_hash.len()) as u64,
        )?;
        if value.world_id != record.world.world_id
            || value.height != core.body().height
            || value.node_id != value.proposer_id
            || value.node_block_hash.len() != 64
            || hex::decode_to_slice(&value.node_block_hash, &mut digest).is_err()
        {
            return Err(invalid().into());
        }
        Some(decoded)
    };
    let extra = original
        .as_ref()
        .map(|value| {
            let value: &NodeExecutionCommitContext = value.borrow();
            value.node_block_hash.len()
        })
        .unwrap_or(64);
    let bytes = size_of::<PaidIwsNodeBoundaryV1>()
        .checked_add(record.world.world_id.len())
        .and_then(|n| n.checked_add(64 * 3 + extra))
        .ok_or_else(invalid)?;
    let fee = common
        .memory()
        .reserve(bytes, MemoryClass::Ordinary)
        .map_err(|_| invalid())?;
    let boundary = NodeExecutionBootstrap {
        height: core.body().height,
        consensus_block_hash: match original.as_ref() {
            Some(original) => {
                let value: &NodeExecutionCommitContext = original.borrow();
                text(&value.node_block_hash, common)?
            }
            None => id_text(&parent.head().verified_record_id(), common)?,
        },
        execution_block_hash: id_text(&fence.parent_commit, common)?,
        execution_state_root: text(&record.after_state_root, common)?,
    };
    let value = if initial {
        Boundary::Genesis(
            NodeExecutionGenesisBootstrap::from_qualified_initial_activation(
                text(&record.world.world_id, common)?,
                id_text(&parent.head().verified_record_id(), common)?,
                boundary,
            )
            .map_err(|_| invalid())?,
        )
    } else {
        Boundary::Normal(boundary)
    };
    Ok(PaidIwsNodeBoundaryV1 {
        value,
        record: parent.head().verified_record_id(),
        fence,
        namespaces: parent.namespace_identities(),
        core: common.work().identity(),
        _fee: fee,
    })
}

impl IwsHostRuntimeV1 {
    pub(crate) fn read_node_execution_boundary_bounded(
        &self,
        memory: &MemoryBudget,
        archive: &FormalArchiveReadWorkV1,
        limits: &CborDataLimitsV1,
        cancelled: &AtomicBool,
    ) -> Result<PaidIwsNodeBoundaryV1, IwsHostRuntimeErrorV1> {
        let current = self.current.current().ok_or_else(invalid)?;
        current.source.with_parent_read_view_bounded(
            memory,
            archive,
            &mut || cancelled.load(Ordering::Acquire),
            |parent| {
                let proof =
                    parent.read_qualified_parent_proof_bounded(memory, archive, &mut || {
                        cancelled.load(Ordering::Acquire)
                    })?;
                let mut stop = || cancelled.load(Ordering::Acquire);
                let mut common = archive.bounded_context(memory, &mut stop);
                proof.validate_parent_bounded(&parent, &mut common)?;
                current
                    .resident
                    .as_ref()
                    .ok_or_else(invalid)?
                    .validate_live_binding_v1(
                        current.source.record_id(),
                        &parent.fence(),
                        &mut common,
                    )
                    .map_err(IwsHostRuntimeErrorV1::NormalSource)?;
                read_from_parent_bounded(&parent, proof.material(), limits, &mut common)
            },
        )
    }
}
