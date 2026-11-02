use super::common::*;
use super::*;
use oasis7_agent_api::world_resource::ChainResourceManifest;
pub(super) fn record_manifest<
    W: ValueNodeWriter + StatePageWriter + ValueNodeReader,
    C: FnMut() -> bool,
>(
    source: &ChainResourceManifest,
    context: &mut MemberRecordWriteContext<'_, W, C>,
    work: &mut RuntimeArtifactResourcesWorkV1,
) -> Result<StateRootV1, MemberRecordError> {
    use oasis7_agent_api::world_resource::ChainResourceManifest as M;
    let M {
        schema_version,
        world_id,
        chain_id,
        genesis_ref,
        world_seed,
        chunk_generation_schema_version,
        world_config_hash,
        generation_algorithm_id,
        generation_algorithm_hash,
        created_at_height,
        created_at_block_hash,
        manifest_height,
        manifest_hash,
        resource_balances,
        material_ledgers,
        generated_chunks,
    } = source;
    let kind = RuntimeArtifactRecordV1::ChainResourceManifest;
    let mut root = child_root()?;
    metadata(&mut root, kind, context, work)?;
    for (field, value) in [
        ("schema_version", schema_version.as_str()),
        ("world_id", world_id.as_str()),
        ("chain_id", chain_id.as_str()),
        (
            "chunk_generation_schema_version",
            chunk_generation_schema_version.as_str(),
        ),
        ("world_config_hash", world_config_hash.as_str()),
        ("generation_algorithm_id", generation_algorithm_id.as_str()),
        (
            "generation_algorithm_hash",
            generation_algorithm_hash.as_str(),
        ),
        ("manifest_hash", manifest_hash.as_str()),
    ] {
        text(&mut root, kind, field, value, context, work)?;
    }
    u64_value(&mut root, kind, "world_seed", *world_seed, context, work)?;
    u64_value(
        &mut root,
        kind,
        "created_at_height",
        *created_at_height,
        context,
        work,
    )?;
    u64_value(
        &mut root,
        kind,
        "manifest_height",
        *manifest_height,
        context,
        work,
    )?;
    option_text(
        &mut root,
        kind,
        "genesis_ref",
        genesis_ref.as_deref(),
        context,
        work,
    )?;
    option_text(
        &mut root,
        kind,
        "created_at_block_hash",
        created_at_block_hash.as_deref(),
        context,
        work,
    )?;

    group(&mut root, kind, &("resource_balances", ()), context, work)?;
    u64_value(
        &mut root,
        kind,
        "resource_balances.count",
        resource_balances.len() as u64,
        context,
        work,
    )?;
    for (resource, balance) in resource_balances {
        member(
            &mut root,
            kind,
            &("resource_balances", resource),
            MemberPayloadRefV1::I64(*balance),
            context,
            work,
        )?;
    }

    group(&mut root, kind, &("material_ledgers", ()), context, work)?;
    u64_value(
        &mut root,
        kind,
        "material_ledgers.count",
        material_ledgers.len() as u64,
        context,
        work,
    )?;
    for (ledger_id, materials) in material_ledgers {
        let ledger_kind = RuntimeArtifactRecordV1::MaterialLedger;
        let mut ledger = child_root()?;
        metadata(&mut ledger, ledger_kind, context, work)?;
        group(&mut ledger, ledger_kind, &("materials", ()), context, work)?;
        member(
            &mut ledger,
            ledger_kind,
            &"materials.count",
            MemberPayloadRefV1::U64(materials.len() as u64),
            context,
            work,
        )?;
        for (material, amount) in materials {
            member(
                &mut ledger,
                ledger_kind,
                &("materials", material),
                MemberPayloadRefV1::I64(*amount),
                context,
                work,
            )?;
        }
        root_member(
            &mut root,
            kind,
            &("material_ledgers", ledger_id),
            ledger_kind,
            ledger,
            context,
            work,
        )?;
    }

    group(&mut root, kind, &("generated_chunks", ()), context, work)?;
    u64_value(
        &mut root,
        kind,
        "generated_chunks.count",
        generated_chunks.len() as u64,
        context,
        work,
    )?;
    for (chunk_key, chunk) in generated_chunks {
        let child = record_chunk(chunk, context, work)?;
        root_member(
            &mut root,
            kind,
            &("generated_chunks", chunk_key),
            RuntimeArtifactRecordV1::ChunkManifest,
            child,
            context,
            work,
        )?;
    }
    Ok(root)
}

pub(super) fn record_chunk<
    W: ValueNodeWriter + StatePageWriter + ValueNodeReader,
    C: FnMut() -> bool,
>(
    source: &oasis7_agent_api::world_resource::ChainChunkResourceManifestEntry,
    context: &mut MemberRecordWriteContext<'_, W, C>,
    work: &mut RuntimeArtifactResourcesWorkV1,
) -> Result<StateRootV1, MemberRecordError> {
    let mut root = child_root()?;
    let kind = RuntimeArtifactRecordV1::ChunkManifest;
    metadata(&mut root, kind, context, work)?;
    for (field, value) in [
        ("schema_version", source.schema_version.as_str()),
        ("world_id", source.world_id.as_str()),
        ("chain_id", source.chain_id.as_str()),
        (
            "chunk_generation_schema_version",
            source.chunk_generation_schema_version.as_str(),
        ),
        (
            "chunk_budget_total_hash",
            source.chunk_budget_total_hash.as_str(),
        ),
        (
            "chunk_budget_remaining_hash",
            source.chunk_budget_remaining_hash.as_str(),
        ),
        ("manifest_hash", source.manifest_hash.as_str()),
    ] {
        text(&mut root, kind, field, value, context, work)?;
    }
    u64_value(
        &mut root,
        kind,
        "world_seed",
        source.world_seed,
        context,
        work,
    )?;
    u64_value(&mut root, kind, "seed", source.seed, context, work)?;
    u64_value(
        &mut root,
        kind,
        "fragment_count",
        u64::from(source.fragment_count),
        context,
        work,
    )?;
    u64_value(
        &mut root,
        kind,
        "block_count",
        u64::from(source.block_count),
        context,
        work,
    )?;
    i64_value(
        &mut root,
        kind,
        "coord.x",
        i64::from(source.coord.x),
        context,
        work,
    )?;
    i64_value(
        &mut root,
        kind,
        "coord.y",
        i64::from(source.coord.y),
        context,
        work,
    )?;
    i64_value(
        &mut root,
        kind,
        "coord.z",
        i64::from(source.coord.z),
        context,
        work,
    )?;
    u64_value(
        &mut root,
        kind,
        "chunk_status",
        match source.chunk_status {
            oasis7_agent_api::world_resource::ChainChunkResourceStatus::Committed => 0,
            oasis7_agent_api::world_resource::ChainChunkResourceStatus::ChainPending => 1,
            oasis7_agent_api::world_resource::ChainChunkResourceStatus::Provisional => 2,
            oasis7_agent_api::world_resource::ChainChunkResourceStatus::Exhausted => 3,
        },
        context,
        work,
    )?;
    group(&mut root, kind, &("fragment_refs", ()), context, work)?;
    u64_value(
        &mut root,
        kind,
        "fragment_refs.count",
        source.fragment_refs.len() as u64,
        context,
        work,
    )?;
    for (ordinal, fragment) in source.fragment_refs.iter().enumerate() {
        let ordinal =
            u64::try_from(ordinal).map_err(|_| MemberRecordError::Invalid("fragment ordinal"))?;
        let child = record_fragment_ref(fragment, context, work)?;
        root_member(
            &mut root,
            kind,
            &("fragment_refs", ordinal),
            RuntimeArtifactRecordV1::FragmentRef,
            child,
            context,
            work,
        )?;
    }
    write_element_map(
        &mut root,
        kind,
        "total_by_element_g",
        &source.total_by_element_g,
        context,
        work,
    )?;
    write_element_map(
        &mut root,
        kind,
        "remaining_by_element_g",
        &source.remaining_by_element_g,
        context,
        work,
    )?;
    let commit = record_commit_ref(&source.commit_ref, context, work)?;
    root_member(
        &mut root,
        kind,
        &"commit_ref",
        RuntimeArtifactRecordV1::CommitRef,
        commit,
        context,
        work,
    )?;
    Ok(root)
}

pub(super) fn write_element_map<
    W: ValueNodeWriter + StatePageWriter + ValueNodeReader,
    C: FnMut() -> bool,
>(
    root: &mut StateRootV1,
    record: RuntimeArtifactRecordV1,
    name: &'static str,
    map: &std::collections::BTreeMap<oasis7_agent_api::world_resource::FragmentElementKind, i64>,
    context: &mut MemberRecordWriteContext<'_, W, C>,
    work: &mut RuntimeArtifactResourcesWorkV1,
) -> Result<(), MemberRecordError> {
    group(root, record, &(name, ()), context, work)?;
    member(
        root,
        record,
        &(name, "count"),
        MemberPayloadRefV1::U64(map.len() as u64),
        context,
        work,
    )?;
    for (element, value) in map {
        member(
            root,
            record,
            &(name, element.wire_label()),
            MemberPayloadRefV1::I64(*value),
            context,
            work,
        )?;
    }
    Ok(())
}

pub(super) fn record_fragment_ref<
    W: ValueNodeWriter + StatePageWriter + ValueNodeReader,
    C: FnMut() -> bool,
>(
    source: &oasis7_agent_api::world_resource::ChainFragmentResourceRef,
    context: &mut MemberRecordWriteContext<'_, W, C>,
    work: &mut RuntimeArtifactResourcesWorkV1,
) -> Result<StateRootV1, MemberRecordError> {
    let mut root = child_root()?;
    let kind = RuntimeArtifactRecordV1::FragmentRef;
    metadata(&mut root, kind, context, work)?;
    for (field, value) in [
        ("fragment_id", source.fragment_id.as_str()),
        ("location_id", source.location_id.as_str()),
        ("profile_hash", source.profile_hash.as_str()),
        ("budget_total_hash", source.budget_total_hash.as_str()),
        (
            "budget_remaining_hash",
            source.budget_remaining_hash.as_str(),
        ),
    ] {
        text(&mut root, kind, field, value, context, work)?;
    }
    Ok(root)
}

pub(super) fn record_commit_ref<
    W: ValueNodeWriter + StatePageWriter + ValueNodeReader,
    C: FnMut() -> bool,
>(
    source: &oasis7_agent_api::world_resource::ChainResourceCommitRef,
    context: &mut MemberRecordWriteContext<'_, W, C>,
    work: &mut RuntimeArtifactResourcesWorkV1,
) -> Result<StateRootV1, MemberRecordError> {
    let mut root = child_root()?;
    let kind = RuntimeArtifactRecordV1::CommitRef;
    metadata(&mut root, kind, context, work)?;
    u64_value(&mut root, kind, "height", source.height, context, work)?;
    option_text(
        &mut root,
        kind,
        "block_hash",
        source.block_hash.as_deref(),
        context,
        work,
    )?;
    option_u64(&mut root, kind, "event_id", source.event_id, context, work)?;
    option_u64(
        &mut root,
        kind,
        "action_id",
        source.action_id,
        context,
        work,
    )?;
    Ok(root)
}
