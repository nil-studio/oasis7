//! Original action-root payload and fixed validation, charged before allocation.
use super::*;
use oasis7_formal_protocol::controlled_authority::formal::{
    FormalError, bounded_context::FormalBoundedContextV1, context_wire,
};
use oasis7_proto::memory_budget::{MemoryClass, ReservedValue};

pub fn compute_consensus_action_root_bounded(
    actions: &[NodeConsensusAction],
    ctx: &mut FormalBoundedContextV1<'_>,
) -> Result<ReservedValue<String>, FormalError> {
    let invalid = || FormalError::Invalid("consensus action root layout".into());
    ctx.consume_steps(1)?;
    let bytes = actions
        .len()
        .checked_mul(std::mem::size_of::<ActionRootEntry<'_>>())
        .and_then(|n| n.checked_add(std::mem::size_of::<ActionRootPayload<'_>>()))
        .and_then(|n| n.checked_add(4096))
        .ok_or_else(invalid)?;
    let _entries_fee = ctx
        .memory()
        .reserve(bytes, MemoryClass::Ordinary)
        .map_err(|_| invalid())?;
    let mut entries = Vec::with_capacity(actions.len());
    let mut last = 0;
    for action in actions {
        let n = action
            .payload_cbor
            .len()
            .checked_add(action.payload_hash.len())
            .and_then(|n| n.checked_add(action.submitter_player_id.len()))
            .ok_or_else(invalid)?;
        ctx.charge(1, u64::try_from(n).map_err(|_| invalid())?)?;
        // Reserve the original validator/order diagnostic and its error bridge.
        let _diagnostic = ctx
            .memory()
            .reserve(
                n.checked_mul(24)
                    .and_then(|n| n.checked_add(4096))
                    .ok_or_else(invalid)?,
                MemoryClass::Ordinary,
            )
            .map_err(|_| invalid())?;
        validate_action_order(action, last).map_err(|e| FormalError::Invalid(e.reason))?;
        last = action.action_id;
        entries.push(ActionRootEntry {
            action_id: action.action_id,
            submitter_player_id: &action.submitter_player_id,
            payload_hash: &action.payload_hash,
        });
    }
    let payload = ActionRootPayload {
        version: 2,
        actions: entries,
    };
    let wire = context_wire::encode_paid(
        &payload,
        usize::MAX,
        ctx,
        "consensus action root wire bound",
        "consensus action root encoding",
    )?;
    let fee = ctx
        .memory()
        .reserve(64 + std::mem::size_of::<String>(), MemoryClass::Ordinary)
        .map_err(|_| invalid())?;
    let mut hash = blake3::Hasher::new();
    for chunk in wire.chunks(8192) {
        ctx.charge(1, chunk.len() as u64)?;
        hash.update(chunk);
    }
    ctx.charge(1, 64)?;
    Ok(fee.retain(hash.finalize().to_hex().to_string()))
}
