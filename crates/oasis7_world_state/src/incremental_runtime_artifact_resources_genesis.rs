//! Bounded borrowed genesis for the original Runtime Artifacts fields.
use super::*;
use crate::{
    incremental_field_member::*,
    incremental_field_presence::{
        PresenceRecordLimitsV1, PresenceRecordWriteContext, write_field_group_v1,
        write_field_metadata_v1,
    },
    incremental_field_write,
    incremental_record_tree::{self, RecordTreeLabelsV1, RecordTreeWritePolicyV1},
    incremental_semantic_key::{SemanticKeyV1, hash_semantic_key_v1},
};
use oasis7_formal_protocol::{
    incremental_execution::ExecutionWriteV1,
    incremental_record_root::RecordRootRefV1,
    incremental_state::{StateNamespaceV1, StatePageWriter, StateRootV1},
    incremental_value::{ValueNodeReader, ValueNodeWriter, ValueTreeError},
};
use oasis7_proto::memory_budget::{MemoryClass, ReservedValue};

#[path = "incremental_runtime_artifact_resources_genesis_delta.rs"]
mod delta;
#[path = "incremental_runtime_artifact_resources_genesis_identity.rs"]
mod identity;

const LABELS: RecordTreeLabelsV1 = RecordTreeLabelsV1 {
    read: "runtime artifact child state read",
    pending: "runtime artifact pending node allowance",
    tree: "runtime artifact typed child state tree",
    digest: "runtime artifact state node digest",
    read_allowance: "runtime artifact child read allowance",
    admission: "runtime artifact child source admission",
    bound: "runtime artifact child source bound",
    producer_pool: "runtime artifact producer pool",
    page: "runtime artifact state page",
    write_allowance: "runtime artifact write allowance",
};

/// Caller-owned logical work budget. Memory and I/O remain charged through the
/// shared contexts; this counter limits semantic rows and record framing work.
pub struct RuntimeArtifactResourcesWorkV1 {
    pub cells_remaining: u64,
    pub max_pending_node_bytes: usize,
}
impl RuntimeArtifactResourcesWorkV1 {
    fn spend(&mut self, cancelled: &mut impl FnMut() -> bool) -> Result<(), ValueTreeError> {
        if cancelled() {
            return Err(ValueTreeError::Cancelled);
        }
        self.cells_remaining =
            self.cells_remaining
                .checked_sub(1)
                .ok_or(ValueTreeError::ResourceLimited(
                    "runtime artifact resource cells",
                ))?;
        Ok(())
    }
}

fn append<W: StatePageWriter + ValueNodeReader, C: FnMut() -> bool>(
    root: &mut StateRootV1,
    field_key: &WorldFieldKeyV1,
    value_root: [u8; 32],
    context: &mut MemberRecordWriteContext<'_, W, C>,
    work: &RuntimeArtifactResourcesWorkV1,
) -> Result<(), MemberRecordError> {
    *root = incremental_record_tree::append(
        root.clone(),
        field_key,
        value_root,
        context,
        RecordTreeWritePolicyV1 {
            max_pending_node_bytes: work.max_pending_node_bytes,
            labels: LABELS,
        },
    )?;
    Ok(())
}

fn child_root() -> Result<StateRootV1, ValueTreeError> {
    StateRootV1::empty(StateNamespaceV1::Artifacts)
        .map_err(|_| ValueTreeError::Invalid("runtime artifact child namespace"))
}

fn presence_context<'a, W, C: FnMut() -> bool>(
    context: &'a mut MemberRecordWriteContext<'_, W, C>,
) -> PresenceRecordWriteContext<'a, W, &'a mut C> {
    PresenceRecordWriteContext {
        sink: &mut *context.sink,
        memory: context.memory,
        io: &mut *context.io,
        key_work: &mut *context.key_work,
        limits: PresenceRecordLimitsV1 {
            semantic_bytes: context.limits.semantic_bytes,
            record_bytes: context.limits.record_bytes,
        },
        cancelled: &mut context.cancelled,
    }
}

fn metadata<W: ValueNodeWriter + StatePageWriter + ValueNodeReader, C: FnMut() -> bool>(
    root: &mut StateRootV1,
    record: RuntimeArtifactRecordV1,
    context: &mut MemberRecordWriteContext<'_, W, C>,
    work: &mut RuntimeArtifactResourcesWorkV1,
) -> Result<(), MemberRecordError> {
    work.spend(&mut context.cancelled)?;
    let field = record.descriptor();
    let record = write_field_metadata_v1(field, &mut presence_context(context))?;
    append(root, &record.field_key, record.value_root, context, work)
}

fn group<
    W: ValueNodeWriter + StatePageWriter + ValueNodeReader,
    C: FnMut() -> bool,
    K: SemanticKeyV1 + ?Sized,
>(
    root: &mut StateRootV1,
    record: RuntimeArtifactRecordV1,
    key: &K,
    context: &mut MemberRecordWriteContext<'_, W, C>,
    work: &mut RuntimeArtifactResourcesWorkV1,
) -> Result<(), MemberRecordError> {
    work.spend(&mut context.cancelled)?;
    let field = record.descriptor();
    let record = write_field_group_v1(field, key, &mut presence_context(context))?;
    append(root, &record.field_key, record.value_root, context, work)
}

fn member<
    W: ValueNodeWriter + StatePageWriter + ValueNodeReader,
    C: FnMut() -> bool,
    K: SemanticKeyV1 + ?Sized,
>(
    root: &mut StateRootV1,
    record: RuntimeArtifactRecordV1,
    key: &K,
    payload: MemberPayloadRefV1<'_>,
    context: &mut MemberRecordWriteContext<'_, W, C>,
    work: &mut RuntimeArtifactResourcesWorkV1,
) -> Result<(), MemberRecordError> {
    work.spend(&mut context.cancelled)?;
    let cell = write_member_record_v1(
        record.descriptor(),
        WorldFieldRecordKindV1::Entry,
        key,
        payload,
        context,
    )?;
    append(root, &cell.field_key, cell.value_root, context, work)
}

fn root_reference<
    K: SemanticKeyV1 + ?Sized,
    W: ValueNodeWriter + StatePageWriter + ValueNodeReader,
    C: FnMut() -> bool,
>(
    record: RuntimeArtifactRecordV1,
    key: &K,
    child: StateRootV1,
    context: &mut MemberRecordWriteContext<'_, W, C>,
) -> Result<RecordRootRefV1, MemberRecordError> {
    let owner = hash_semantic_key_v1(
        key,
        context.memory,
        MemoryClass::Ordinary,
        context.key_work,
        &mut context.cancelled,
    )
    .map_err(MemberRecordError::Key)?;
    RecordRootRefV1::new(record.schema(), owner.digest, child)
        .map_err(|_| MemberRecordError::Invalid("runtime artifact record root"))
}

fn text<W: ValueNodeWriter + StatePageWriter + ValueNodeReader, C: FnMut() -> bool>(
    root: &mut StateRootV1,
    record: RuntimeArtifactRecordV1,
    key: &'static str,
    value: &str,
    context: &mut MemberRecordWriteContext<'_, W, C>,
    work: &mut RuntimeArtifactResourcesWorkV1,
) -> Result<(), MemberRecordError> {
    member(
        root,
        record,
        &key,
        MemberPayloadRefV1::Text(value),
        context,
        work,
    )
}

fn u64_value<W: ValueNodeWriter + StatePageWriter + ValueNodeReader, C: FnMut() -> bool>(
    root: &mut StateRootV1,
    record: RuntimeArtifactRecordV1,
    key: &'static str,
    value: u64,
    context: &mut MemberRecordWriteContext<'_, W, C>,
    work: &mut RuntimeArtifactResourcesWorkV1,
) -> Result<(), MemberRecordError> {
    member(
        root,
        record,
        &key,
        MemberPayloadRefV1::U64(value),
        context,
        work,
    )
}

fn i64_value<W: ValueNodeWriter + StatePageWriter + ValueNodeReader, C: FnMut() -> bool>(
    root: &mut StateRootV1,
    record: RuntimeArtifactRecordV1,
    key: &'static str,
    value: i64,
    context: &mut MemberRecordWriteContext<'_, W, C>,
    work: &mut RuntimeArtifactResourcesWorkV1,
) -> Result<(), MemberRecordError> {
    member(
        root,
        record,
        &key,
        MemberPayloadRefV1::I64(value),
        context,
        work,
    )
}

fn option_text<W: ValueNodeWriter + StatePageWriter + ValueNodeReader, C: FnMut() -> bool>(
    root: &mut StateRootV1,
    record: RuntimeArtifactRecordV1,
    key: &'static str,
    value: Option<&str>,
    context: &mut MemberRecordWriteContext<'_, W, C>,
    work: &mut RuntimeArtifactResourcesWorkV1,
) -> Result<(), MemberRecordError> {
    member(
        root,
        record,
        &(key, "present"),
        MemberPayloadRefV1::Bool(value.is_some()),
        context,
        work,
    )?;
    if let Some(value) = value {
        member(
            root,
            record,
            &(key, "value"),
            MemberPayloadRefV1::Text(value),
            context,
            work,
        )?;
    }
    Ok(())
}

fn option_u64<W: ValueNodeWriter + StatePageWriter + ValueNodeReader, C: FnMut() -> bool>(
    root: &mut StateRootV1,
    record: RuntimeArtifactRecordV1,
    key: &'static str,
    value: Option<u64>,
    context: &mut MemberRecordWriteContext<'_, W, C>,
    work: &mut RuntimeArtifactResourcesWorkV1,
) -> Result<(), MemberRecordError> {
    member(
        root,
        record,
        &(key, "present"),
        MemberPayloadRefV1::Bool(value.is_some()),
        context,
        work,
    )?;
    if let Some(value) = value {
        member(
            root,
            record,
            &(key, "value"),
            MemberPayloadRefV1::U64(value),
            context,
            work,
        )?;
    }
    Ok(())
}

fn root_member<
    K: SemanticKeyV1 + ?Sized,
    W: ValueNodeWriter + StatePageWriter + ValueNodeReader,
    C: FnMut() -> bool,
>(
    root: &mut StateRootV1,
    parent: RuntimeArtifactRecordV1,
    key: &K,
    child_kind: RuntimeArtifactRecordV1,
    child: StateRootV1,
    context: &mut MemberRecordWriteContext<'_, W, C>,
    work: &mut RuntimeArtifactResourcesWorkV1,
) -> Result<(), MemberRecordError> {
    let reference = root_reference(child_kind, key, child, context)?;
    member(
        root,
        parent,
        key,
        MemberPayloadRefV1::RecordRoot(&reference),
        context,
        work,
    )
}

fn record_manifest<W: ValueNodeWriter + StatePageWriter + ValueNodeReader, C: FnMut() -> bool>(
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
        group(
            &mut root,
            kind,
            &("material_ledgers", ledger_id),
            context,
            work,
        )?;
        member(
            &mut root,
            kind,
            &("material_ledgers.count", ledger_id),
            MemberPayloadRefV1::U64(materials.len() as u64),
            context,
            work,
        )?;
        for (material, amount) in materials {
            member(
                &mut root,
                kind,
                &("material_ledgers", (ledger_id, material)),
                MemberPayloadRefV1::I64(*amount),
                context,
                work,
            )?;
        }
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

fn record_chunk<W: ValueNodeWriter + StatePageWriter + ValueNodeReader, C: FnMut() -> bool>(
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

fn write_element_map<W: ValueNodeWriter + StatePageWriter + ValueNodeReader, C: FnMut() -> bool>(
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

fn record_fragment_ref<
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

fn record_commit_ref<W: ValueNodeWriter + StatePageWriter + ValueNodeReader, C: FnMut() -> bool>(
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

fn record_delta<W: ValueNodeWriter + StatePageWriter + ValueNodeReader, C: FnMut() -> bool>(
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

fn record_delta_entry<
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

fn record_builtin<
    W: ValueNodeWriter + StatePageWriter + ValueNodeReader,
    C: FnMut() -> bool,
    B: BuiltinReleaseManifestSourceV1,
>(
    source: &B,
    context: &mut MemberRecordWriteContext<'_, W, C>,
    work: &mut RuntimeArtifactResourcesWorkV1,
) -> Result<StateRootV1, MemberRecordError> {
    let mut root = child_root()?;
    let kind = RuntimeArtifactRecordV1::BuiltinReleaseManifest;
    metadata(&mut root, kind, context, work)?;
    group(&mut root, kind, &("module_sets", ()), context, work)?;
    let expected_sets = source.len();
    u64_value(
        &mut root,
        kind,
        "module_sets.count",
        expected_sets as u64,
        context,
        work,
    )?;
    let mut visited_sets = 0usize;
    let mut previous_set: Option<&str> = None;
    source.for_each_module_set(|set_id, set| {
        if previous_set.is_some_and(|previous| previous >= set_id) {
            return Err(MemberRecordError::Invalid("builtin module set key order"));
        }
        previous_set = Some(set_id);
        visited_sets = visited_sets
            .checked_add(1)
            .ok_or(MemberRecordError::Invalid("builtin module set count"))?;
        let mut child = child_root()?;
        let child_kind = RuntimeArtifactRecordV1::BuiltinModuleSet;
        metadata(&mut child, child_kind, context, work)?;
        group(&mut child, child_kind, &("modules", ()), context, work)?;
        let expected_modules = set.len();
        u64_value(
            &mut child,
            child_kind,
            "modules.count",
            expected_modules as u64,
            context,
            work,
        )?;
        let mut visited_modules = 0usize;
        let mut previous_module: Option<&str> = None;
        set.for_each_module(|module_id, entry| {
            if previous_module.is_some_and(|previous| previous >= module_id) {
                return Err(MemberRecordError::Invalid("builtin module key order"));
            }
            previous_module = Some(module_id);
            visited_modules = visited_modules
                .checked_add(1)
                .ok_or(MemberRecordError::Invalid("builtin module count"))?;
            let body = record_builtin_entry(&entry, context, work)?;
            root_member(
                &mut child,
                child_kind,
                &("modules", module_id),
                RuntimeArtifactRecordV1::BuiltinReleaseEntry,
                body,
                context,
                work,
            )
        })?;
        if visited_modules != expected_modules {
            return Err(MemberRecordError::Invalid("builtin module count mismatch"));
        }
        root_member(
            &mut root,
            kind,
            &("module_sets", set_id),
            child_kind,
            child,
            context,
            work,
        )
    })?;
    if visited_sets != expected_sets {
        return Err(MemberRecordError::Invalid(
            "builtin module set count mismatch",
        ));
    }
    Ok(root)
}

fn record_builtin_entry<
    W: ValueNodeWriter + StatePageWriter + ValueNodeReader,
    C: FnMut() -> bool,
>(
    source: &BuiltinReleaseEntryRefV1<'_>,
    context: &mut MemberRecordWriteContext<'_, W, C>,
    work: &mut RuntimeArtifactResourcesWorkV1,
) -> Result<StateRootV1, MemberRecordError> {
    let mut root = child_root()?;
    let kind = RuntimeArtifactRecordV1::BuiltinReleaseEntry;
    metadata(&mut root, kind, context, work)?;
    group(&mut root, kind, &("hash_tokens", ()), context, work)?;
    u64_value(
        &mut root,
        kind,
        "hash_tokens.count",
        source.hash_tokens.len() as u64,
        context,
        work,
    )?;
    for (ordinal, token) in source.hash_tokens.iter().enumerate() {
        let ordinal = u64::try_from(ordinal)
            .map_err(|_| MemberRecordError::Invalid("builtin hash ordinal"))?;
        member(
            &mut root,
            kind,
            &("hash_tokens", ordinal),
            MemberPayloadRefV1::Text(token),
            context,
            work,
        )?;
    }
    group(&mut root, kind, &("artifact_identities", ()), context, work)?;
    u64_value(
        &mut root,
        kind,
        "artifact_identities.count",
        source.artifact_identities.len() as u64,
        context,
        work,
    )?;
    for (identity_key, identity) in source.artifact_identities {
        let identity_root = identity::record_module_identity(identity, context, work)?;
        root_member(
            &mut root,
            kind,
            &("artifact_identities", identity_key),
            RuntimeArtifactRecordV1::ModuleArtifactIdentity,
            identity_root,
            context,
            work,
        )?;
    }
    Ok(root)
}

fn emit_top<
    W: ValueNodeWriter + StatePageWriter + ValueNodeReader,
    C: FnMut() -> bool,
    S: FnMut(ReservedValue<ExecutionWriteV1>) -> Result<(), ValueTreeError>,
>(
    field: RuntimeArtifactFieldV1,
    child: Option<StateRootV1>,
    context: &mut MemberRecordWriteContext<'_, W, C>,
    sink: &mut S,
    work: &mut RuntimeArtifactResourcesWorkV1,
) -> Result<(), MemberRecordError> {
    work.spend(&mut context.cancelled)?;
    let descriptor = field.descriptor();
    let meta = write_field_metadata_v1(descriptor, &mut presence_context(context))?;
    incremental_field_write::emit(meta.field_key, meta.value_root, context, sink)?;
    work.spend(&mut context.cancelled)?;
    let reference = match child {
        None => None,
        Some(child) => {
            let owner = hash_semantic_key_v1(
                &(),
                context.memory,
                MemoryClass::Ordinary,
                context.key_work,
                &mut context.cancelled,
            )
            .map_err(MemberRecordError::Key)?;
            Some(
                RecordRootRefV1::new(field.schema(), owner.digest, child)
                    .map_err(|_| MemberRecordError::Invalid("runtime artifact outer root"))?,
            )
        }
    };
    let payload = match reference.as_ref() {
        None => MemberPayloadRefV1::Unit,
        Some(reference) => MemberPayloadRefV1::RecordRoot(reference),
    };
    let outer = write_member_record_v1(
        descriptor,
        WorldFieldRecordKindV1::Entry,
        &(),
        payload,
        context,
    )?;
    incremental_field_write::emit(outer.field_key, outer.value_root, context, sink)
}

/// Emit the three original Runtime Artifacts fields as typed provisional
/// records. Every write remains uncommitted until its owning transaction seals.
pub fn prepare_runtime_artifact_resources_genesis_v1<
    W: StatePageWriter + ValueNodeReader,
    C: FnMut() -> bool,
    S: FnMut(ReservedValue<ExecutionWriteV1>) -> Result<(), ValueTreeError>,
    B: BuiltinReleaseManifestSourceV1,
>(
    source: &RuntimeArtifactResourcesRefV1<'_, B>,
    context: &mut MemberRecordWriteContext<'_, W, C>,
    work: &mut RuntimeArtifactResourcesWorkV1,
    sink: &mut S,
) -> Result<u64, MemberRecordError> {
    let starting_cells = work.cells_remaining;
    let manifest = record_manifest(source.chain_resource_manifest, context, work)?;
    emit_top(
        RuntimeArtifactFieldV1::ChainResourceManifest,
        Some(manifest),
        context,
        sink,
        work,
    )?;
    let delta = match source.latest_chain_resource_delta {
        Some(value) => Some(record_delta(value, context, work)?),
        None => None,
    };
    emit_top(
        RuntimeArtifactFieldV1::LatestChainResourceDelta,
        delta,
        context,
        sink,
        work,
    )?;
    let builtin = record_builtin(source.builtin_release_manifest, context, work)?;
    emit_top(
        RuntimeArtifactFieldV1::BuiltinReleaseManifest,
        Some(builtin),
        context,
        sink,
        work,
    )?;
    if (context.cancelled)() {
        return Err(ValueTreeError::Cancelled.into());
    }
    Ok(starting_cells - work.cells_remaining)
}
