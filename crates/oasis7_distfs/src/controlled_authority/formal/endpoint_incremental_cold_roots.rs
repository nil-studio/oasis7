//! Decode only the exact RootSet original committed by a genuine accepted head.
use super::*;
use oasis7_formal_protocol::{
    controlled_authority::replicated_protocol::ArtifactRole,
    incremental_execution::RootSetV1,
    incremental_material::{CoreBudget, OriginalObjectReader},
};
use oasis7_proto::memory_budget::{MemoryBudget, MemoryClass, ReservedValue};
pub(super) fn recover_roots<O: OriginalObjectReader>(
    head: &VerifiedFormalCommitV1,
    objects: &mut O,
    memory: &MemoryBudget,
    budget: &mut CoreBudget,
    mut cancelled: impl FnMut() -> bool,
) -> Result<ReservedValue<RootSetV1>, FormalError>
where
    O::Error: std::fmt::Debug,
{
    if cancelled() {
        return Err(invalid("IWS cold roots cancelled"));
    }
    if objects.allocation_class() != MemoryClass::Recovery
        || !objects.allocation_budget().same_pool(memory)
    {
        return Err(invalid("IWS cold roots reader phase/pool"));
    }
    let mut id = [0u8; 32];
    hex::decode_to_slice(
        head.record()
            .artifact_roots
            .get(&ArtifactRole::IncrementalRootSet)
            .ok_or_else(|| invalid("IWS accepted RootSet role missing"))?,
        &mut id,
    )
    .map_err(|_| invalid("IWS accepted RootSet role ID"))?;
    budget.objects_remaining = budget
        .objects_remaining
        .checked_sub(1)
        .ok_or_else(|| invalid("IWS cold root object quota"))?;
    let raw = objects
        .read_original(
            &id,
            usize::try_from(budget.original_bytes_remaining.min(1024))
                .map_err(|_| invalid("IWS cold root read bound"))?,
            memory,
        )
        .map_err(|e| invalid(format!("IWS accepted RootSet original: {e:?}")))?;
    budget.original_bytes_remaining = budget
        .original_bytes_remaining
        .checked_sub(u64::try_from(raw.len()).map_err(|_| invalid("IWS root length overflow"))?)
        .ok_or_else(|| invalid("IWS cold root byte quota"))?;
    if !raw.uses_budget(memory) || raw.len() > 1024 || blake3::hash(&raw).as_bytes() != &id {
        return Err(invalid("IWS accepted RootSet bytes"));
    }
    // The protocol decoder accepts at most 1024 input bytes, six root Strings,
    // and a six-element temporary Vec before moving to its fixed array.
    let admission = 2048usize
        .checked_add(std::mem::size_of::<RootSetV1>())
        .and_then(|n| {
            n.checked_add(
                6 * (std::mem::size_of::<oasis7_formal_protocol::incremental_state::StateRootV1>()
                    + 128),
            )
        })
        .ok_or_else(|| invalid("IWS cold root accounting"))?;
    let charge = memory
        .reserve(admission, MemoryClass::Recovery)
        .map_err(|_| invalid("IWS cold root admission"))?;
    let roots = RootSetV1::from_canonical_bytes(&raw)
        .map_err(|e| invalid(format!("IWS accepted RootSet framing: {e}")))?;
    let mut after = [0u8; 32];
    hex::decode_to_slice(&head.record().after_state_root, &mut after)
        .map_err(|_| invalid("IWS accepted after-root ID"))?;
    let mut genesis = [0u8; 32];
    hex::decode_to_slice(&head.record().world.genesis_digest, &mut genesis)
        .map_err(|_| invalid("IWS accepted genesis ID"))?;
    if id != after
        || roots.world_id != head.record().world.world_id
        || roots.genesis_config != genesis
        || roots.height.checked_add(1) != Some(head.record().position)
    {
        return Err(invalid("IWS accepted RootSet head binding"));
    }
    if cancelled() {
        return Err(invalid("IWS cold roots cancelled"));
    }
    Ok(charge.retain(roots))
}
