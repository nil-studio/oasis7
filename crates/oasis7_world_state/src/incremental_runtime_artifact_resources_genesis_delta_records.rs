use super::common::*;
use super::delta;
use super::*;
use oasis7_agent_api::world_resource::ChainResourceDelta;
pub(super) fn record_delta<
    W: ValueNodeWriter + StatePageWriter + ValueNodeReader,
    C: FnMut() -> bool,
>(
    source: &ChainResourceDelta,
    context: &mut MemberRecordWriteContext<'_, W, C>,
    work: &mut RuntimeArtifactResourcesWorkV1,
) -> Result<StateRootV1, MemberRecordError> {
    let mut root = child_root()?;
    let kind = RuntimeArtifactRecordV1::ChainResourceDelta;
    metadata(&mut root, kind, context, work)?;
    for (field, value) in [
        ("schema_version", source.schema_version.as_str()),
        ("world_id", source.world_id.as_str()),
        ("chain_id", source.chain_id.as_str()),
        ("delta_id", source.delta_id.as_str()),
        ("base_manifest_hash", source.base_manifest_hash.as_str()),
        (
            "resulting_manifest_hash",
            source.resulting_manifest_hash.as_str(),
        ),
    ] {
        text(&mut root, kind, field, value, context, work)?;
    }
    option_u64(
        &mut root,
        kind,
        "action_id",
        source.action_id,
        context,
        work,
    )?;
    option_u64(&mut root, kind, "event_id", source.event_id, context, work)?;
    u64_value(
        &mut root,
        kind,
        "block_height",
        source.block_height,
        context,
        work,
    )?;
    option_text(
        &mut root,
        kind,
        "commit_block_hash",
        source.commit_block_hash.as_deref(),
        context,
        work,
    )?;
    u64_value(&mut root, kind, "tick", source.tick, context, work)?;
    u64_value(
        &mut root,
        kind,
        "source",
        delta::delta_source_tag(source.source),
        context,
        work,
    )?;
    u64_value(
        &mut root,
        kind,
        "replay_status",
        delta::replay_status_tag(source.replay_status),
        context,
        work,
    )?;
    let ordering = delta::record_ordering_key(&source.ordering_key, context, work)?;
    root_member(
        &mut root,
        kind,
        &"ordering_key",
        RuntimeArtifactRecordV1::OrderingKey,
        ordering,
        context,
        work,
    )?;
    group(&mut root, kind, &("entries", ()), context, work)?;
    u64_value(
        &mut root,
        kind,
        "entries.count",
        source.entries.len() as u64,
        context,
        work,
    )?;
    for (ordinal, entry) in source.entries.iter().enumerate() {
        let ordinal = u64::try_from(ordinal)
            .map_err(|_| MemberRecordError::Invalid("delta entry ordinal"))?;
        let (entry_kind, child) = record_delta_entry(entry, context, work)?;
        root_member(
            &mut root,
            kind,
            &("entries", ordinal),
            entry_kind,
            child,
            context,
            work,
        )?;
    }
    Ok(root)
}

pub(super) fn record_delta_entry<
    W: ValueNodeWriter + StatePageWriter + ValueNodeReader,
    C: FnMut() -> bool,
>(
    source: &oasis7_agent_api::world_resource::ChainResourceDeltaEntry,
    context: &mut MemberRecordWriteContext<'_, W, C>,
    work: &mut RuntimeArtifactResourcesWorkV1,
) -> Result<(RuntimeArtifactRecordV1, StateRootV1), MemberRecordError> {
    use oasis7_agent_api::world_resource::ChainResourceDeltaEntry as E;
    let mut root = child_root()?;
    match source {
        E::RuntimeResource {
            kind: resource,
            delta,
            resulting_balance,
        } => {
            let record = RuntimeArtifactRecordV1::DeltaRuntimeResource;
            metadata(&mut root, record, context, work)?;
            u64_value(&mut root, record, "variant", 0, context, work)?;
            member(
                &mut root,
                record,
                &"kind",
                MemberPayloadRefV1::U64(match resource {
                    oasis7_agent_api::world_action::ResourceKind::Electricity => 0,
                    oasis7_agent_api::world_action::ResourceKind::Data => 1,
                }),
                context,
                work,
            )?;
            i64_value(&mut root, record, "delta", *delta, context, work)?;
            i64_value(
                &mut root,
                record,
                "resulting_balance",
                *resulting_balance,
                context,
                work,
            )?;
            Ok((record, root))
        }
        E::MaterialLedger {
            ledger_id,
            material_kind,
            delta,
            resulting_balance,
        } => {
            let record = RuntimeArtifactRecordV1::DeltaMaterialLedger;
            metadata(&mut root, record, context, work)?;
            u64_value(&mut root, record, "variant", 1, context, work)?;
            member(
                &mut root,
                record,
                &"ledger_id",
                MemberPayloadRefV1::Ledger(ledger_id),
                context,
                work,
            )?;
            text(
                &mut root,
                record,
                "material_kind",
                material_kind,
                context,
                work,
            )?;
            i64_value(&mut root, record, "delta", *delta, context, work)?;
            i64_value(
                &mut root,
                record,
                "resulting_balance",
                *resulting_balance,
                context,
                work,
            )?;
            Ok((record, root))
        }
        E::ChunkResource {
            coord,
            element,
            total_delta_g,
            remaining_delta_g,
            resulting_remaining_g,
            remaining_after_hash,
            chunk_remaining_after_hash,
        } => {
            let record = RuntimeArtifactRecordV1::DeltaChunkResource;
            metadata(&mut root, record, context, work)?;
            u64_value(&mut root, record, "variant", 2, context, work)?;
            i64_value(
                &mut root,
                record,
                "coord.x",
                i64::from(coord.x),
                context,
                work,
            )?;
            i64_value(
                &mut root,
                record,
                "coord.y",
                i64::from(coord.y),
                context,
                work,
            )?;
            i64_value(
                &mut root,
                record,
                "coord.z",
                i64::from(coord.z),
                context,
                work,
            )?;
            text(
                &mut root,
                record,
                "element",
                element.wire_label(),
                context,
                work,
            )?;
            i64_value(
                &mut root,
                record,
                "total_delta_g",
                *total_delta_g,
                context,
                work,
            )?;
            i64_value(
                &mut root,
                record,
                "remaining_delta_g",
                *remaining_delta_g,
                context,
                work,
            )?;
            i64_value(
                &mut root,
                record,
                "resulting_remaining_g",
                *resulting_remaining_g,
                context,
                work,
            )?;
            text(
                &mut root,
                record,
                "remaining_after_hash",
                remaining_after_hash,
                context,
                work,
            )?;
            text(
                &mut root,
                record,
                "chunk_remaining_after_hash",
                chunk_remaining_after_hash,
                context,
                work,
            )?;
            Ok((record, root))
        }
    }
}
