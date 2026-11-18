//! Declared original graph from the retained GEN producer. Catalog page trees
//! and raw artifact occurrences retain their independent physical obligations.
use super::*;

fn insert(
    objects: &mut Vec<ArtifactObject>,
    object: ArtifactObject,
    common: &mut FormalBoundedContextV1<'_>,
) -> Result<()> {
    let (mut lo, mut hi) = (0, objects.len());
    while lo < hi {
        common.charge(1, 128).map_err(context_error)?;
        let mid = lo + (hi - lo) / 2;
        match objects[mid].content_hash.cmp(&object.content_hash) {
            std::cmp::Ordering::Less => lo = mid + 1,
            std::cmp::Ordering::Greater => hi = mid,
            std::cmp::Ordering::Equal => {
                common
                    .charge(1, (objects[mid].bytes.len() + object.bytes.len()) as u64)
                    .map_err(context_error)?;
                common
                    .charge(
                        1,
                        ((objects[mid].references.len() + object.references.len()) * 64) as u64,
                    )
                    .map_err(context_error)?;
                if objects[mid].bytes != object.bytes
                    || objects[mid].references != object.references
                {
                    return Err(
                        PageStoreError::Corrupt("GEN duplicate original/edge mismatch").into(),
                    );
                }
                return Ok(());
            }
        }
    }
    if objects.len() == objects.capacity() {
        return Err(PageStoreError::ResourceLimited("GEN original graph slots").into());
    }
    common
        .charge(
            1,
            ((objects.len() - lo) * size_of::<ArtifactObject>()) as u64,
        )
        .map_err(context_error)?;
    objects.insert(lo, object);
    Ok(())
}
fn refs(
    ids: &[[u8; 32]],
    allocation: &mut Allocation,
    common: &mut FormalBoundedContextV1<'_>,
) -> Result<Vec<String>> {
    let mut out = allocation.vector::<String>(ids.len(), common)?;
    for id in ids {
        let value = allocation.hex(id, common)?;
        let (mut lo, mut hi) = (0, out.len());
        while lo < hi {
            common.charge(1, 128).map_err(context_error)?;
            let mid = lo + (hi - lo) / 2;
            match out[mid].cmp(&value) {
                std::cmp::Ordering::Less => lo = mid + 1,
                std::cmp::Ordering::Greater => hi = mid,
                std::cmp::Ordering::Equal => {
                    lo = mid;
                    hi = mid;
                }
            }
        }
        if out.get(lo).is_some_and(|old| old == &value) {
            continue;
        }
        common
            .charge(1, ((out.len() - lo) * size_of::<String>()) as u64)
            .map_err(context_error)?;
        out.insert(lo, value);
    }
    Ok(out)
}
fn put(
    reference: OriginalArtifactRefV1,
    wire: &ReservedValue<Vec<u8>>,
    edges: &[[u8; 32]],
    objects: &mut Vec<ArtifactObject>,
    allocation: &mut Allocation,
    common: &mut FormalBoundedContextV1<'_>,
) -> Result<()> {
    if !wire.uses_budget(common.memory())
        || wire.len() as u64 != reference.byte_len
        || digest(wire, common)? != reference.id
    {
        return Err(PageStoreError::Corrupt("GEN exact retained original").into());
    }
    let object = ArtifactObject {
        content_hash: allocation.hex(&reference.id, common)?,
        bytes: allocation.bytes(wire, common)?,
        references: refs(edges, allocation, common)?,
    };
    insert(objects, object, common)
}
fn validate(material: &ClosedRecord, common: &mut FormalBoundedContextV1<'_>) -> Result<()> {
    material
        .validate_iws_bounded(common)
        .map_err(|error| Error::Body(BodyMutationError::Original(Box::new(error))))
}

pub(super) fn build(
    header: &ProvisionalGenesisHeaderV1,
    allocation: &mut Allocation,
    common: &mut FormalBoundedContextV1<'_>,
) -> Result<ClosedRecord> {
    let inputs = &header.inputs;
    let business = inputs.business();
    let nodes = &business.delta().delta().node_writes;
    let slots = nodes
        .len()
        .checked_add(23)
        .ok_or(PageStoreError::ResourceLimited("GEN object graph count"))?;
    let mut objects = allocation.vector::<ArtifactObject>(slots, common)?;
    for (reference, wire) in [
        business.config_original(),
        business.roots_original(),
        business.retention_original(),
        business.wasm_catalog_original(),
        inputs.rules_catalog_original(),
        inputs.native_original(),
        inputs.policy_original(),
    ] {
        put(reference, wire, &[], &mut objects, allocation, common)?;
    }
    put(
        inputs.manifest_original().0,
        inputs.manifest_original().1,
        &[inputs.policy_original().0.id],
        &mut objects,
        allocation,
        common,
    )?;
    put(
        header.input.reference,
        &header.input.wire,
        &[],
        &mut objects,
        allocation,
        common,
    )?;
    let mut source_ids =
        allocation.vector::<[u8; 32]>(inputs.source_history().result_count(), common)?;
    for original in inputs.source_history().originals() {
        put(
            original.reference,
            &original.wire,
            &[],
            &mut objects,
            allocation,
            common,
        )?;
        source_ids.push(original.reference.id);
    }
    put(
        header.facts.reference,
        &header.facts.wire,
        &source_ids,
        &mut objects,
        allocation,
        common,
    )?;
    for original in [
        &header.segment,
        &header.prefix,
        &header.checkpoint,
        &header.catalog_original,
    ] {
        put(
            original.reference,
            &original.wire,
            &[],
            &mut objects,
            allocation,
            common,
        )?;
    }
    put(
        business.descriptor_original().0,
        business.descriptor_original().1,
        &[],
        &mut objects,
        allocation,
        common,
    )?;
    let mut node_ids = allocation.vector::<[u8; 32]>(nodes.len(), common)?;
    for node in nodes {
        common
            .charge(1, node.digest.len() as u64)
            .map_err(context_error)?;
        let mut id = [0; 32];
        hex::decode_to_slice(&node.digest, &mut id)
            .map_err(|_| PageStoreError::Corrupt("GEN business original node digest"))?;
        if digest(&node.bytes, common)? != id {
            return Err(PageStoreError::Corrupt("GEN business original node bytes").into());
        }
        insert(
            &mut objects,
            ArtifactObject {
                content_hash: allocation.hex(&id, common)?,
                bytes: allocation.bytes(&node.bytes, common)?,
                references: Vec::new(),
            },
            common,
        )?;
        node_ids.push(id);
    }
    put(
        business.delta_original().0,
        business.delta_original().1,
        &node_ids,
        &mut objects,
        allocation,
        common,
    )?;
    put(
        header.business_locator.reference,
        &header.business_locator.wire,
        &[
            business.descriptor_original().0.id,
            business.delta_original().0.id,
        ],
        &mut objects,
        allocation,
        common,
    )?;
    put(
        header.body.reference,
        &header.body.wire,
        &[
            header.facts.reference.id,
            header.segment.reference.id,
            header.prefix.reference.id,
            header.checkpoint.reference.id,
            header.catalog_original.reference.id,
            header.business_locator.reference.id,
        ],
        &mut objects,
        allocation,
        common,
    )?;
    let rows = [
        (ArtifactRole::Input, header.input.reference.id),
        (
            ArtifactRole::IncrementalGenesisConfig,
            business.config_original().0.id,
        ),
        (
            ArtifactRole::IncrementalRootSet,
            business.roots_original().0.id,
        ),
        (
            ArtifactRole::IncrementalCommitBody,
            header.body.reference.id,
        ),
        (
            ArtifactRole::IncrementalHistoryPrefix,
            header.prefix.reference.id,
        ),
        (
            ArtifactRole::IncrementalRetentionLedger,
            business.retention_original().0.id,
        ),
        (
            ArtifactRole::ExecutionManifest,
            inputs.manifest_original().0.id,
        ),
        (ArtifactRole::Wasm, business.wasm_catalog_original().0.id),
        (ArtifactRole::Rules, inputs.rules_catalog_original().0.id),
        (
            ArtifactRole::NativeExecutionEnvironment,
            inputs.native_original().0.id,
        ),
    ];
    allocation.admit(rows.len() * 2048, common)?;
    let mut roots = BTreeMap::new();
    for (role, id) in rows {
        common.charge(1, 128).map_err(context_error)?;
        roots.insert(role, allocation.hex(&id, common)?);
    }
    let material = ClosedRecord {
        payload_digest: allocation.hex(&header.input.reference.id, common)?,
        before_state_root: allocation.hex(&business.roots_original().0.id, common)?,
        after_state_root: allocation.hex(&business.roots_original().0.id, common)?,
        roots,
        objects,
    };
    validate(&material, common)?;
    Ok(material)
}

pub(super) fn attach_projection_sidecar(
    header: &ProvisionalGenesisHeaderV1,
    record: [u8; 32],
    material: &mut ClosedRecord,
    allocation: &mut Allocation,
    common: &mut FormalBoundedContextV1<'_>,
) -> Result<super::super::history::Original> {
    let json = header
        .inputs
        .business()
        .projected()
        .projected()
        .retained_history_root_json_bounded_v1(common)
        .map_err(Error::Body)?;
    let value = oasis7_formal_protocol::incremental_projection_sidecar::ProjectionSidecarV1 {
        record,
        commit_body: header.body.reference.id,
        roots: header.inputs.business().roots_original().0.id,
        retained_history_root_json: allocation.bytes(&json, common)?,
    };
    let memory = common.memory().clone();
    let wire =
        artifact_catalog::protocol(common, |pay| value.canonical_bytes_with_work(&memory, pay))?;
    let reference = OriginalArtifactRefV1 {
        id: digest(&wire, common)?,
        byte_len: wire.len() as u64,
    };
    if reference.id == header.body.reference.id {
        return Err(PageStoreError::Corrupt("GEN projection side/body cycle").into());
    }
    put(
        reference,
        &wire,
        &[],
        &mut material.objects,
        allocation,
        common,
    )?;
    let body = allocation.hex(&header.body.reference.id, common)?;
    let side = allocation.hex(&reference.id, common)?;
    let mut found = None;
    for (index, object) in material.objects.iter().enumerate() {
        common.charge(1, 128).map_err(context_error)?;
        if object.content_hash == body {
            found = Some(index);
            break;
        }
    }
    let index = found.ok_or(PageStoreError::Corrupt("GEN body original missing"))?;
    let old = &material.objects[index].references;
    let mut edges = allocation.vector::<String>(old.len() + 1, common)?;
    let mut added = false;
    for value in old {
        common.charge(1, 128).map_err(context_error)?;
        if !added && side < *value {
            edges.push(allocation.text(&side, common)?);
            added = true;
        }
        if value == &side {
            return Err(PageStoreError::Corrupt("GEN duplicate projection side edge").into());
        }
        edges.push(allocation.text(value, common)?);
    }
    if !added {
        edges.push(side);
    }
    material.objects[index].references = edges;
    validate(material, common)?;
    Ok(super::super::history::Original { reference, wire })
}
