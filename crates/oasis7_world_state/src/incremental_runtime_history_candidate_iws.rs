//! The IWS Tick-lane data profile is admitted only inside the existing actual
//! candidate-fact join. These checks are not execution, Node-origin, finality,
//! a Current read permit, or a replacement for complete candidate verification.
use super::*;
use crate::incremental_iws_tick_consensus::IwsTickConsensusRecordV1;
use oasis7_formal_protocol::controlled_authority::formal::bounded_wire::{
    BoundedWireErrorV1, BoundedWireWorkV1, encode_bounded_wire_v1,
};
use sha2::{Digest, Sha256};
pub(super) fn check_iws_candidate_record_v1<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    OE,
>(
    value: &IwsTickConsensusRecordV1,
    index_ordinal: u64,
    request: &RuntimeHistoryCandidateReadRequestV1<'_>,
    ctx: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
    work: &mut RuntimeHistoryCandidateReadWorkV1,
) -> Result<(), RuntimeHistoryCandidateReadErrorV1<OE>> {
    stop(ctx)?;
    let frame = &value.current;
    let parent = request.core.parent_roots().ok_or_else(invalid)?;
    // All fixed-root temporary encoders and comparisons are admitted before use.
    let _scratch = ctx
        .values
        .memory
        .reserve(8192, MemoryClass::Ordinary)
        .map_err(ValueTreeError::Memory)?;
    let bytes = frame
        .context
        .world_id
        .len()
        .checked_add(request.core.roots().world_id.len())
        .and_then(|n| n.checked_add(frame.context.action_root.len()))
        .and_then(|n| n.checked_add(request.candidate.record().action_root.len()))
        .and_then(|n| n.checked_add(value.authority_source.len()))
        .and_then(|n| n.checked_add(8192))
        .ok_or_else(invalid)?;
    work.decode.structure.bytes_remaining = work
        .decode
        .structure
        .bytes_remaining
        .checked_sub(bytes as u64)
        .ok_or(ValueTreeError::ResourceLimited(
            "IWS History source comparisons",
        ))?;
    work.decode.structure.values_remaining = work
        .decode
        .structure
        .values_remaining
        .checked_sub(24)
        .ok_or(ValueTreeError::ResourceLimited("IWS History source fields"))?;
    value.validate_data().map_err(|_| invalid())?;
    if Some(frame.parent_record) != request.core.parent_record_id()
        || Some(frame.parent_body) != request.core.body().parent_commit
        || frame.parent_roots != parent.digest().map_err(|_| invalid())?
        || frame.input_original != request.core.original_input_id()
        || frame.policy_original != request.core.original_execution_policy_id()
        || frame.context.height != request.core.roots().height
        || frame.context.world_id != request.core.roots().world_id
        || frame.context.action_root != request.candidate.record().action_root
        || value.tick != request.core.roots().simulation_tick
        || value.consensus_height != index_ordinal.checked_add(1).ok_or_else(invalid)?
    {
        return Err(invalid().into());
    }
    if let Some(parent_source) = request.parent {
        let location = parent_source.occurrence().location();
        if frame.runtime_original != location.original || frame.runtime_ordinal != location.ordinal
        {
            return Err(invalid().into());
        }
    }
    // The frame digest uses the unique protocol wire encoder (declaration
    // field order), not the independently canonicalized History envelope codec.
    let mut wire_work = BoundedWireWorkV1 {
        max_wire_bytes: work.decode.max_original_bytes,
        remaining_steps: work.decode.structure.values_remaining,
        remaining_bytes: work.decode.structure.bytes_remaining,
    };
    let encoded =
        encode_bounded_wire_v1(frame, ctx.values.memory, &mut wire_work, &mut ctx.cancelled);
    work.decode.structure.values_remaining = wire_work.remaining_steps;
    work.decode.structure.bytes_remaining = wire_work.remaining_bytes;
    let encoded = encoded.map_err(|e| match e {
        BoundedWireErrorV1::Cancelled => ValueTreeError::Cancelled,
        BoundedWireErrorV1::Memory(e) => ValueTreeError::Memory(e),
        BoundedWireErrorV1::StepLimit
        | BoundedWireErrorV1::ByteLimit
        | BoundedWireErrorV1::WireLimit => ValueTreeError::ResourceLimited("IWS commitment wire"),
        _ => invalid(),
    })?;
    work.decode.structure.bytes_remaining = work
        .decode
        .structure
        .bytes_remaining
        .checked_sub(encoded.len() as u64)
        .ok_or(ValueTreeError::ResourceLimited(
            "IWS commitment SHA256 bytes",
        ))?;
    work.decode.structure.values_remaining = work
        .decode
        .structure
        .values_remaining
        .checked_sub(encoded.chunks(4096).len() as u64)
        .ok_or(ValueTreeError::ResourceLimited(
            "IWS commitment SHA256 chunks",
        ))?;
    let mut hash = Sha256::new();
    for chunk in encoded.chunks(4096) {
        stop(ctx)?;
        hash.update(chunk);
    }
    let digest: [u8; 32] = hash.finalize().into();
    if digest != value.current_digest {
        return Err(invalid().into());
    }
    stop(ctx)?;
    Ok(())
}
