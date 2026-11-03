//! Fixed fields and tags for Runtime chain-resource deltas.
use super::*;

pub(super) fn record_ordering_key<
    W: ValueNodeWriter + StatePageWriter + ValueNodeReader,
    C: FnMut() -> bool,
>(
    source: &oasis7_agent_api::world_resource::ChainResourceOrderingKey,
    context: &mut MemberRecordWriteContext<'_, W, C>,
    work: &mut RuntimeArtifactResourcesWorkV1,
) -> Result<StateRootV1, MemberRecordError> {
    let mut root = child_root()?;
    let kind = RuntimeArtifactRecordV1::OrderingKey;
    metadata(&mut root, kind, context, work)?;
    u64_value(&mut root, kind, "height", source.height, context, work)?;
    u64_value(
        &mut root,
        kind,
        "event_sequence",
        source.event_sequence,
        context,
        work,
    )?;
    u64_value(
        &mut root,
        kind,
        "action_sequence",
        source.action_sequence,
        context,
        work,
    )?;
    Ok(root)
}

pub(super) fn delta_source_tag(
    source: oasis7_agent_api::world_resource::ChainResourceDeltaSource,
) -> u64 {
    use oasis7_agent_api::world_resource::ChainResourceDeltaSource as S;
    match source {
        S::ChunkGenerated => 0,
        S::MineCompound => 1,
        S::Replenish => 2,
        S::Genesis => 3,
        S::Migration => 4,
    }
}

pub(super) fn replay_status_tag(
    status: oasis7_agent_api::world_resource::ChainResourceReplayStatus,
) -> u64 {
    use oasis7_agent_api::world_resource::ChainResourceReplayStatus as S;
    match status {
        S::Committed => 0,
        S::Rejected => 1,
        S::RolledBack => 2,
    }
}
