use super::*;
use crate::{
    incremental_field_member::*,
    incremental_field_overlay::WorldFieldOverlayErrorV1,
    incremental_field_read::WorldFieldReadContextV1,
    incremental_inventory_value_read::InventoryValueReader,
    incremental_json_records::qualify_json_subtree_records_v1,
    incremental_record_diff::{self as diff, RecordDiffLabelsV1},
};
use oasis7_formal_protocol::{
    incremental_execution::{TransitionRecordV1, TransitionVerifier},
    incremental_state::{
        MerkleNodeReader, StateEntryV1, StateNamespaceV1, StateRootV1, StateScanBudget,
    },
    incremental_value::{ValueNodeReader, ValueReadContext, ValueTreeError},
};
use oasis7_proto::memory_budget::ReservedValue;

fn changed<R: ValueNodeReader, C: FnMut() -> bool>(
    kind: ArtifactConfigBodyKindV1,
    old: &StateEntryV1,
    new: &StateEntryV1,
    records: &mut InventoryValueReader<'_, '_, R, C>,
    max: u64,
    json: &mut crate::incremental_json_records::JsonReadWorkV1,
    parents: &mut Option<&mut ArtifactConfigJsonParentsV1<'_>>,
) -> Result<bool, ValueTreeError> {
    let binding = WorldFieldKeyV1::from_bytes(&new.key).map_err(ValueTreeError::Invalid)?;
    if binding.kind != WorldFieldRecordKindV1::Entry {
        return Ok(false);
    }
    let marker = records.key(
        genesis::body_descriptor(),
        WorldFieldRecordKindV1::Entry,
        &(),
    )?;
    if new.key == marker.0.as_bytes() {
        return Ok(false);
    }
    let key = WorldFieldKeyV1::new(
        genesis::body_descriptor(),
        WorldFieldRecordKindV1::Entry,
        binding.semantic_key,
    )
    .map_err(ValueTreeError::Invalid)?;
    if new.namespace != StateNamespaceV1::Artifacts || new.key != key.as_bytes() {
        return Err(read::invalid());
    }
    let limits = MemberRecordLimitsV1 {
        semantic_bytes: records.semantic_bytes,
        payload_bytes: max,
        record_bytes: records.record_bytes,
    };
    let header = read_member_record_header_v1(
        &new.value,
        &key,
        records.values,
        limits,
        &mut *records.cancelled,
    )?;
    let mut sink = std::io::sink();
    let mut probe = key_read::Probe::new(&mut sink);
    let result = read_member_record_profile_v1(
        &new.value,
        &key,
        records.values,
        MemberReadProfileV1 {
            codec: header.codec,
            semantic_bytes: header.semantic_bytes,
            limits,
        },
        &mut probe,
        &mut std::io::sink(),
        &mut *records.cancelled,
    );
    if matches!(&result, Err(ValueTreeError::Cancelled)) {
        return Err(ValueTreeError::Cancelled);
    }
    if probe.is_invalid() {
        return Err(read::invalid());
    }
    result?;
    let address = probe.finish()?;
    let spec = schema::spec(kind, address).map_err(|_| read::invalid())?;
    if !schema::point_supported(kind, address) && spec.codec != MemberPayloadCodecV1::RecordRoot {
        return Ok(false);
    }
    let key = (key, header.semantic_bytes);
    let after = read::value(
        kind,
        address,
        &new.value,
        &key,
        max,
        records,
        &mut std::io::sink(),
    )?;
    if let ArtifactConfigValueV1::JsonRoot(after) = after {
        let before = read::value(
            kind,
            address,
            &old.value,
            &key,
            max,
            records,
            &mut std::io::sink(),
        )?;
        let ArtifactConfigValueV1::JsonRoot(before) = before else {
            return Err(read::invalid());
        };
        if before != after {
            let advanced = if let Some(parents) = parents.as_deref_mut() {
                parents.advance(&before, &after, records)?
            } else {
                false
            };
            if !advanced {
                qualify_json_subtree_records_v1(genesis::json_profile(), &after, records, json)?;
            }
        }
    }
    if (records.cancelled)() {
        return Err(ValueTreeError::Cancelled);
    }
    Ok(true)
}
fn child<M, R, E, C>(
    kind: ArtifactConfigBodyKindV1,
    before: &StateRootV1,
    after: &StateRootV1,
    ctx: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
    scan: &mut StateScanBudget,
    work: &mut ArtifactConfigReadWorkV1,
    parents: &mut Option<&mut ArtifactConfigJsonParentsV1<'_>>,
) -> Result<(), ValueTreeError>
where
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
{
    let mut full = false;
    let max = work.max_payload_bytes;
    let json = &mut work.json;
    diff::visit(
        before,
        after,
        &mut ctx.records(),
        &mut work.cells_remaining,
        RecordDiffLabelsV1 {
            invalid: "artifact config changed paths",
            cells: "artifact config changed cells",
            stack: "artifact config changed stack",
            tree: genesis::LABELS,
        },
        |old, new, records| {
            if let (Some(old), Some(new)) = (old, new) {
                if !changed(kind, old, new, records, max, json, parents)? {
                    full = true;
                }
            } else {
                full = true;
            }
            Ok(())
        },
    )?;
    if full {
        full_body(kind, after, ctx, scan, work)?;
    }
    Ok(())
}
fn full_body<M, R, E, C>(
    kind: ArtifactConfigBodyKindV1,
    root: &StateRootV1,
    ctx: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
    scan: &mut StateScanBudget,
    work: &mut ArtifactConfigReadWorkV1,
) -> Result<(), ValueTreeError>
where
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
{
    let mut body = WorldFieldReadContextV1 {
        root,
        state: ctx.state,
        values: ValueReadContext {
            source: ctx.values.source,
            memory: ctx.values.memory,
            class: ctx.values.class,
            io: &mut *ctx.values.io,
        },
        key_work: &mut *ctx.key_work,
        semantic_bytes: ctx.semantic_bytes,
        record_bytes: ctx.record_bytes,
        take_state_error: &mut ctx.take_state_error,
        cancelled: &mut ctx.cancelled,
    };
    qualify_artifact_config_body_v1(kind, &mut body, scan, work)?;
    Ok(())
}

/// Independent generic replay plus changed owner relationships. Unchanged
/// qualified JSON/state subtrees are inherited; explicit new JSON is fully read.
fn accept<M, S, R, E, C>(
    parent: &ReservedValue<ArtifactConfigFamilyQualificationV1>,
    verifier: &TransitionVerifier<'_, S>,
    record: &TransitionRecordV1,
    ctx: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
    inner_scan: &mut StateScanBudget,
    work: &mut ArtifactConfigReadWorkV1,
    parents: Option<&mut ArtifactConfigJsonParentsV1<'_>>,
) -> Result<ReservedValue<ArtifactConfigFamilyQualificationV1>, WorldFieldOverlayErrorV1>
where
    M: MerkleNodeReader,
    S: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
{
    ctx.check()?;
    if ctx.values.class != oasis7_proto::memory_budget::MemoryClass::Ordinary
        || ctx.root != record.after.state_root(StateNamespaceV1::Artifacts)
    {
        return Err(read::invalid().into());
    }
    {
        ctx.check()?;
        if !parent.uses_budget(ctx.values.memory)
            || parent.allocation_class() != ctx.values.class
            || parent.reserved_bytes() < std::mem::size_of::<ArtifactConfigFamilyQualificationV1>()
            || !verifier.memory.same_pool(ctx.values.memory)
            || verifier.class != ctx.values.class
        {
            return Err(read::invalid().into());
        }
        parent.validate_binding(&record.before.digest().map_err(|_| read::invalid())?)?;
    }
    let witness = crate::incremental_verified_world_transition::verify_world_transition_v1(
        verifier, record, ctx,
    )?;
    accept_artifact_config_verified_v1(parent, &witness, ctx, inner_scan, work, parents)
}

pub(crate) fn accept_artifact_config_verified_v1<M, R, E, C>(
    parent: &ReservedValue<ArtifactConfigFamilyQualificationV1>,
    witness: &crate::incremental_verified_world_transition::VerifiedWorldTransitionV1,
    ctx: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
    inner_scan: &mut StateScanBudget,
    work: &mut ArtifactConfigReadWorkV1,
    mut parents: Option<&mut ArtifactConfigJsonParentsV1<'_>>,
) -> Result<ReservedValue<ArtifactConfigFamilyQualificationV1>, WorldFieldOverlayErrorV1>
where
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
{
    ctx.check()?;
    witness.check_context(ctx.values.memory, ctx.values.class, ctx.root)?;
    if ctx.values.class != oasis7_proto::memory_budget::MemoryClass::Ordinary {
        return Err(read::invalid().into());
    }
    let record = witness.record();

    ctx.check()?;
    if !parent.uses_budget(ctx.values.memory)
        || parent.allocation_class() != ctx.values.class
        || parent.reserved_bytes() < std::mem::size_of::<ArtifactConfigFamilyQualificationV1>()
    {
        return Err(read::invalid().into());
    }
    parent.validate_binding(&record.before.digest().map_err(|_| read::invalid())?)?;
    let verified = witness.prepared();
    if verified
        .record
        .after
        .state_root(StateNamespaceV1::Artifacts)
        != ctx.root
    {
        return Err(read::invalid().into());
    }
    let _scratch = ctx
        .values
        .memory
        .reserve(32 << 10, ctx.values.class)
        .map_err(ValueTreeError::Memory)?;
    let mut registry_records = parent.registry_records();
    for write in &verified.record.writes {
        work.cell()?;
        let Some((field, key)) = family::field_key(write.namespace, &write.mutation.key)? else {
            continue;
        };
        let binding =
            WorldFieldKeyV1::from_bytes(key.as_bytes()).map_err(ValueTreeError::Invalid)?;
        if binding.kind == WorldFieldRecordKindV1::Metadata {
            let (Some(old), Some(new)) = (write.mutation.before, write.mutation.after) else {
                return Err(read::invalid().into());
            };
            ctx.records().presence(&old, &key, 0)?;
            ctx.records().presence(&new, &key, 0)?;
            continue;
        }
        let old = write
            .mutation
            .before
            .map(|value| family::outer(field, &key, &value, &mut ctx.records()))
            .transpose()?;
        let new = write
            .mutation
            .after
            .map(|value| family::outer(field, &key, &value, &mut ctx.records()))
            .transpose()?;
        if field.global() && (old.is_none() || new.is_none()) {
            return Err(read::invalid().into());
        }
        if field.body_kind() == ArtifactConfigBodyKindV1::Module {
            for sibling in family::RECORD_FIELDS {
                let other_key = WorldFieldKeyV1::new(
                    sibling.descriptor(),
                    WorldFieldRecordKindV1::Entry,
                    binding.semantic_key,
                )
                .map_err(ValueTreeError::Invalid)?;
                let other = ctx
                    .lookup(&other_key)?
                    .map(|value| family::outer(sibling, &other_key, &value, &mut ctx.records()))
                    .transpose()?;
                if other.as_deref() != new.as_deref() {
                    return Err(read::invalid().into());
                }
            }
            // A changed common body necessarily changes the primary outer
            // reference too; the generic replay and cross-column checks above
            // authenticate that write. Verify its body only once.
            if field != ArtifactConfigFieldV1::RegistryManifest {
                continue;
            }
        }
        if field == ArtifactConfigFieldV1::RegistryManifest {
            registry_records = match (old.is_some(), new.is_some()) {
                (false, true) => registry_records.checked_add(1),
                (true, false) => registry_records.checked_sub(1),
                _ => Some(registry_records),
            }
            .ok_or_else(read::invalid)?;
        }
        match (old, new) {
            (Some(old), Some(new)) => child(
                field.body_kind(),
                &old,
                &new,
                ctx,
                inner_scan,
                work,
                &mut parents,
            )?,
            (None, Some(new)) => full_body(field.body_kind(), &new, ctx, inner_scan, work)?,
            (Some(_), None) => {}
            (None, None) => return Err(read::invalid().into()),
        }
        ctx.check()?;
    }
    let result = family::mint(
        verified
            .record
            .after
            .digest()
            .map_err(|_| read::invalid())?,
        registry_records,
        ctx.values.memory,
        ctx.values.class,
    )?;
    ctx.check()?;
    Ok(result)
}

pub fn accept_artifact_config_transition_v1<M, S, R, E, C>(
    parent: &ReservedValue<ArtifactConfigFamilyQualificationV1>,
    verifier: &TransitionVerifier<'_, S>,
    record: &TransitionRecordV1,
    ctx: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
    inner_scan: &mut StateScanBudget,
    work: &mut ArtifactConfigReadWorkV1,
) -> Result<ReservedValue<ArtifactConfigFamilyQualificationV1>, WorldFieldOverlayErrorV1>
where
    M: MerkleNodeReader,
    S: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
{
    accept(parent, verifier, record, ctx, inner_scan, work, None)
}
/// Optional matching private parent graph scopes enable incremental JSON
/// receiving. Missing scopes retain complete-new-graph validation, never a guess.
pub fn accept_artifact_config_transition_with_json_v1<M, S, R, E, C>(
    parent: &ReservedValue<ArtifactConfigFamilyQualificationV1>,
    verifier: &TransitionVerifier<'_, S>,
    record: &TransitionRecordV1,
    ctx: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
    inner_scan: &mut StateScanBudget,
    work: &mut ArtifactConfigReadWorkV1,
    parents: &mut ArtifactConfigJsonParentsV1<'_>,
) -> Result<ReservedValue<ArtifactConfigFamilyQualificationV1>, WorldFieldOverlayErrorV1>
where
    M: MerkleNodeReader,
    S: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
{
    accept(
        parent,
        verifier,
        record,
        ctx,
        inner_scan,
        work,
        Some(parents),
    )
}
