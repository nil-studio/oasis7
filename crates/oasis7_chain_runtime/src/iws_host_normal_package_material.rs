//! Actual sealed originals and declared closure; no World/Snapshot conversion.
use super::*;
fn header(
    common: &mut FormalBoundedContextV1<'_>,
    encode: impl FnOnce(
        &mut dyn FnMut(u64, u64) -> Result<(), ExecutionError>,
    ) -> Result<Vec<u8>, ExecutionError>,
) -> Result<ReservedValue<Vec<u8>>, NormalPackageErrorV1> {
    let fee = common
        .memory()
        .reserve(
            4 * oasis7_formal_protocol::incremental_history::HISTORY_MAX_HEADER_BYTES + 1024,
            MemoryClass::Ordinary,
        )
        .map_err(|e| NormalPackageErrorV1::Value(ValueTreeError::Memory(e)))?;
    let mut first = None;
    let raw = encode(&mut |s, b| {
        common.charge(s, b).map_err(|e| {
            if first.is_none() {
                first = Some(e)
            }
            ExecutionError::Aborted
        })
    });
    if let Some(error) = first {
        return Err(error.into());
    }
    let raw = raw?;
    if raw.capacity() > fee.bytes() {
        return Err(invalid());
    }
    Ok(fee.retain(raw))
}
fn insert(
    objects: &mut Vec<ArtifactObject>,
    object: ArtifactObject,
    common: &mut FormalBoundedContextV1<'_>,
) -> Result<(), NormalPackageErrorV1> {
    let (mut lo, mut hi) = (0, objects.len());
    while lo < hi {
        common.charge(1, 128)?;
        let mid = lo + (hi - lo) / 2;
        match objects[mid].content_hash.cmp(&object.content_hash) {
            std::cmp::Ordering::Less => lo = mid + 1,
            std::cmp::Ordering::Greater => hi = mid,
            std::cmp::Ordering::Equal => {
                common.charge(1, object.bytes.len() as u64)?;
                if objects[mid].bytes != object.bytes {
                    return Err(invalid());
                }
                // Existing actual originals retain their authenticated declared
                // edges. Identical header bytes need no duplicate object.
                return Ok(());
            }
        }
    }
    if objects.len() == objects.capacity() {
        return Err(invalid());
    }
    common.charge(
        1,
        u64::try_from(
            (objects.len() - lo)
                .checked_mul(size_of::<ArtifactObject>())
                .ok_or_else(invalid)?,
        )
        .map_err(|_| invalid())?,
    )?;
    objects.insert(lo, object);
    Ok(())
}
pub(super) fn put(
    raw: &[u8],
    objects: &mut Vec<ArtifactObject>,
    allocation: &mut Allocation,
    common: &mut FormalBoundedContextV1<'_>,
) -> Result<String, NormalPackageErrorV1> {
    let id = allocation.hash(raw, common)?;
    let object = ArtifactObject {
        content_hash: allocation.text(&id, common)?,
        bytes: allocation.bytes(raw, common)?,
        references: Vec::new(),
    };
    insert(objects, object, common)?;
    Ok(id)
}
pub(super) fn add_ref(
    id: &str,
    refs: &mut Vec<String>,
    a: &mut Allocation,
    c: &mut FormalBoundedContextV1<'_>,
) -> Result<(), NormalPackageErrorV1> {
    let (mut lo, mut hi) = (0, refs.len());
    while lo < hi {
        c.charge(1, 128)?;
        let mid = lo + (hi - lo) / 2;
        match refs[mid].as_str().cmp(id) {
            std::cmp::Ordering::Less => lo = mid + 1,
            std::cmp::Ordering::Greater => hi = mid,
            std::cmp::Ordering::Equal => return Ok(()),
        }
    }
    if refs.len() == refs.capacity() {
        return Err(invalid());
    }
    c.charge(1, ((refs.len() - lo) * size_of::<String>()) as u64)?;
    refs.insert(lo, a.text(id, c)?);
    Ok(())
}
fn put_ref(
    raw: &[u8],
    objects: &mut Vec<ArtifactObject>,
    refs: &mut Vec<String>,
    a: &mut Allocation,
    c: &mut FormalBoundedContextV1<'_>,
) -> Result<String, NormalPackageErrorV1> {
    let id = put(raw, objects, a, c)?;
    add_ref(&id, refs, a, c)?;
    Ok(id)
}
fn reference(
    id: &str,
    len: usize,
    c: &mut FormalBoundedContextV1<'_>,
) -> Result<
    oasis7_formal_protocol::incremental_artifacts::OriginalArtifactRefV1,
    NormalPackageErrorV1,
> {
    c.charge(1, 96)?;
    let mut digest = [0u8; 32];
    hex::decode_to_slice(id, &mut digest).map_err(|_| invalid())?;
    Ok(
        oasis7_formal_protocol::incremental_artifacts::OriginalArtifactRefV1 {
            id: digest,
            byte_len: u64::try_from(len).map_err(|_| invalid())?,
        },
    )
}
fn find(
    objects: &[ArtifactObject],
    id: &str,
    c: &mut FormalBoundedContextV1<'_>,
) -> Result<Option<usize>, NormalPackageErrorV1> {
    let (mut lo, mut hi) = (0, objects.len());
    while lo < hi {
        c.charge(1, 128)?;
        let mid = lo + (hi - lo) / 2;
        match objects[mid].content_hash.as_str().cmp(id) {
            std::cmp::Ordering::Less => lo = mid + 1,
            std::cmp::Ordering::Greater => hi = mid,
            std::cmp::Ordering::Equal => return Ok(Some(mid)),
        }
    }
    Ok(None)
}
pub(super) fn locate(
    objects: &[ArtifactObject],
    id: &str,
    c: &mut FormalBoundedContextV1<'_>,
) -> Result<usize, NormalPackageErrorV1> {
    find(objects, id, c)?.ok_or_else(invalid)
}
pub(super) fn select_closure(
    objects: Vec<ArtifactObject>,
    roots: &BTreeMap<ArtifactRole, String>,
    a: &mut Allocation,
    c: &mut FormalBoundedContextV1<'_>,
) -> Result<Vec<ArtifactObject>, NormalPackageErrorV1> {
    let mut seen = a.vector::<u8>(objects.len(), c)?;
    c.charge(1, objects.len() as u64)?;
    seen.resize(objects.len(), 0);
    let mut queue = a.vector::<usize>(objects.len(), c)?;
    for id in roots.values() {
        let index = locate(&objects, id, c)?;
        if seen[index] == 0 {
            seen[index] = 1;
            queue.push(index);
        }
    }
    while let Some(index) = queue.pop() {
        c.consume_steps(1)?;
        for id in &objects[index].references {
            let index = locate(&objects, id, c)?;
            if seen[index] == 0 {
                seen[index] = 1;
                queue.push(index);
            }
        }
    }
    let mut selected = a.vector(objects.len().checked_add(1).ok_or_else(invalid)?, c)?;
    for (index, object) in objects.into_iter().enumerate() {
        c.charge(1, size_of::<ArtifactObject>() as u64)?;
        if seen[index] != 0 {
            selected.push(object);
        }
    }
    Ok(selected)
}
/// Every original copied here was either already authenticated by the actual
/// Frozen holder or produced by this exact Runtime seal. Page trees stay in the
/// dual page store and keep their original endpoint verification obligation.
pub(super) fn build(
    prepared: &PreparedNormalHostRuntimeV1,
    input: &PaidVerifiedNormalTickInputV1<'_>,
    business_descriptor: &ReservedValue<Vec<u8>>,
    business_delta: &ReservedValue<Vec<u8>>,
    business_nodes:&[oasis7_formal_protocol::controlled_authority::formal::business_index::BusinessIndexNodeWriteV1],
    allocation: &mut Allocation,
    common: &mut FormalBoundedContextV1<'_>,
) -> Result<ClosedRecord, NormalPackageErrorV1> {
    if !business_descriptor.uses_budget(common.memory())
        || !business_delta.uses_budget(common.memory())
    {
        return Err(invalid());
    }
    let frozen = input.data().checked().material();
    let mut objects = allocation.vector(
        frozen
            .objects
            .len()
            .checked_add(13)
            .and_then(|n| n.checked_add(business_nodes.len()))
            .ok_or_else(invalid)?,
        common,
    )?;
    for object in &frozen.objects {
        common.consume_steps(1)?;
        objects.push(allocation.object(object, common)?);
    }
    let mut references = allocation.vector::<String>(9, common)?;
    // These are the real newly emitted originals, not all old ancestor headers.
    // Reused roots keep their exact existing declared dependency edges below.
    let descriptor_id = put(business_descriptor, &mut objects, allocation, common)?;
    let mut node_refs = allocation.vector::<String>(business_nodes.len(), common)?;
    for node in business_nodes {
        common.charge(1, node.digest.len() as u64)?;
        let id = put_ref(
            &node.bytes,
            &mut objects,
            &mut node_refs,
            allocation,
            common,
        )?;
        common.charge(1, 128)?;
        if id != node.digest {
            return Err(invalid());
        }
    }
    let delta_id = allocation.hash(business_delta, common)?;
    // Existing raw equality must not silently discard this newly required
    // exact delta->node-write edge set when an original is already present.
    if let Some(index) = find(&objects, &delta_id, common)? {
        let old = &objects[index].references;
        common.charge(1, 16)?;
        if old.len() != node_refs.len() {
            return Err(invalid());
        }
        for (left, right) in old.iter().zip(&node_refs) {
            common.charge(1, 128)?;
            if left != right {
                return Err(invalid());
            }
        }
    }
    insert(
        &mut objects,
        ArtifactObject {
            content_hash: allocation.text(&delta_id, common)?,
            bytes: allocation.bytes(business_delta, common)?,
            references: node_refs,
        },
        common,
    )?;
    let candidate = prepared.candidate();
    let history = candidate.history();
    let roots_id = put_ref(
        candidate.root_set_bytes(),
        &mut objects,
        &mut references,
        allocation,
        common,
    )?;
    put_ref(
        candidate.catalog_bytes(),
        &mut objects,
        &mut references,
        allocation,
        common,
    )?;
    if let Some(raw) = history.transition_bytes.as_ref() {
        put_ref(raw, &mut objects, &mut references, allocation, common)?;
    }
    let memory = common.memory().clone();
    let mut first = None;
    let facts = history
        .facts
        .canonical_bytes_with_work(&memory, &mut |s, b| {
            common.charge(s, b).map_err(|e| {
                if first.is_none() {
                    first = Some(e)
                }
                ExecutionError::Aborted
            })
        });
    if let Some(error) = first {
        return Err(error.into());
    }
    put_ref(&facts?, &mut objects, &mut references, allocation, common)?;
    let segment = header(common, |work| {
        history.segment.canonical_bytes_with_work(work)
    })?;
    put_ref(&segment, &mut objects, &mut references, allocation, common)?;
    let prefix = header(common, |work| {
        history.prefix.canonical_bytes_with_work(work)
    })?;
    let prefix_id = put_ref(&prefix, &mut objects, &mut references, allocation, common)?;
    if let Some(checkpoint) = history.checkpoint.as_ref() {
        let raw = header(common, |work| checkpoint.canonical_bytes_with_work(work))?;
        put_ref(&raw, &mut objects, &mut references, allocation, common)?;
    }
    let raw = header(common, |work| history.body.canonical_bytes_with_work(work))?;
    let body_id = allocation.hash(&raw, common)?;
    let locator = oasis7_formal_protocol::incremental_business_output::BusinessOutputLocatorV1 {
        commit_body: reference(&body_id, raw.len(), common)?.id,
        descriptor: reference(&descriptor_id, business_descriptor.len(), common)?,
        delta: reference(&delta_id, business_delta.len(), common)?,
    };
    let memory = common.memory().clone();
    let mut first = None;
    let locator_raw = locator.canonical_bytes_with_work(&memory, &mut |s, b| {
        common.charge(s, b).map_err(|e| {
            if first.is_none() {
                first = Some(e)
            }
            ExecutionError::Aborted
        })
    });
    if let Some(error) = first {
        return Err(error.into());
    }
    let locator_raw = locator_raw?;
    let locator_id = allocation.hash(&locator_raw, common)?;
    let mut locator_refs = allocation.vector::<String>(2, common)?;
    add_ref(&descriptor_id, &mut locator_refs, allocation, common)?;
    add_ref(&delta_id, &mut locator_refs, allocation, common)?;
    insert(
        &mut objects,
        ArtifactObject {
            content_hash: allocation.text(&locator_id, common)?,
            bytes: allocation.bytes(&locator_raw, common)?,
            references: locator_refs,
        },
        common,
    )?;
    add_ref(&locator_id, &mut references, allocation, common)?;
    for id in &references {
        common.charge(1, 128)?;
        if id == &body_id {
            return Err(invalid());
        }
    }
    insert(
        &mut objects,
        ArtifactObject {
            content_hash: allocation.text(&body_id, common)?,
            bytes: allocation.bytes(&raw, common)?,
            references,
        },
        common,
    )?;
    // This genuine Frozen participant is rebound to this exact after RootSet
    // and accepted predecessor on every tick, even when its payload is shared.
    // The descriptor itself never names its containing current record/body.
    let retention = prepared.retention_manifest_bounded_v1(common)?;
    if !retention.uses_budget(common.memory()) {
        return Err(invalid());
    }
    let memory = common.memory().clone();
    let mut first = None;
    let binding = retention.validate_binding_v1(
        history.body.roots,
        Some(input.data().checked().parent().verified_record_id()),
        &mut |s, b| {
            common.charge(s, b).map_err(|e| {
                if first.is_none() {
                    first = Some(e)
                }
                ExecutionError::Aborted
            })
        },
    );
    if let Some(error) = first {
        return Err(error.into());
    }
    binding?;
    let mut first = None;
    let raw = retention.canonical_bytes_with_work(&memory, &mut |s, b| {
        common.charge(s, b).map_err(|e| {
            if first.is_none() {
                first = Some(e)
            }
            ExecutionError::Aborted
        })
    });
    if let Some(error) = first {
        return Err(error.into());
    }
    let raw = raw?;
    let retention_id = put(&raw, &mut objects, allocation, common)?;
    let mut roots = allocation.roots(&frozen.roots, common)?;
    roots.insert(ArtifactRole::IncrementalRootSet, roots_id);
    roots.insert(ArtifactRole::IncrementalCommitBody, body_id);
    roots.insert(ArtifactRole::IncrementalHistoryPrefix, prefix_id);
    allocation.admit(
        11 * (size_of::<(ArtifactRole, String)>() + size_of::<usize>()) + 128,
        common,
    )?;
    roots.insert(ArtifactRole::IncrementalRetentionLedger, retention_id);
    let objects = select_closure(objects, &roots, allocation, common)?;
    let result = ClosedRecord {
        payload_digest: allocation.text(&frozen.payload_digest, common)?,
        before_state_root: allocation.text(&frozen.before_state_root, common)?,
        after_state_root: allocation.text(&roots[&ArtifactRole::IncrementalRootSet], common)?,
        roots,
        objects,
    };
    result
        .validate_iws_bounded(common)
        .map_err(NormalPackageErrorV1::Material)?;
    Ok(result)
}

/// Extend only the real newly emitted body graph. The sidecar has no Artifact
/// role and does not replace any signed IWS role or output commitment.
pub(super) fn attach_projection_sidecar(
    prepared: &PreparedNormalHostRuntimeV1,
    record: [u8; 32],
    material: &mut ClosedRecord,
    allocation: &mut Allocation,
    common: &mut FormalBoundedContextV1<'_>,
) -> Result<(), NormalPackageErrorV1> {
    use oasis7_formal_protocol::incremental_projection_sidecar::ProjectionSidecarV1;
    let body_hex = material
        .roots
        .get(&ArtifactRole::IncrementalCommitBody)
        .ok_or_else(invalid)?;
    let roots_hex = material
        .roots
        .get(&ArtifactRole::IncrementalRootSet)
        .ok_or_else(invalid)?;
    common.charge(2, 128)?;
    let mut body = [0; 32];
    let mut roots = [0; 32];
    hex::decode_to_slice(body_hex, &mut body).map_err(|_| invalid())?;
    hex::decode_to_slice(roots_hex, &mut roots).map_err(|_| invalid())?;
    let side = prepared.retained_history_root_json_bounded_v1(common)?;
    let value = ProjectionSidecarV1 {
        record,
        commit_body: body,
        roots,
        retained_history_root_json: allocation.bytes(&side, common)?,
    };
    let memory = common.memory().clone();
    let mut first = None;
    let raw = value.canonical_bytes_with_work(&memory, &mut |steps, bytes| {
        common.charge(steps, bytes).map_err(|e| {
            if first.is_none() {
                first = Some(e)
            }
            ExecutionError::Aborted
        })
    });
    if let Some(error) = first {
        return Err(error.into());
    }
    let raw = raw?;
    // Locate before insertion may shift the sorted vector. Never keep an index
    // across insertion or silently discard the required edge on deduplication.
    let body_id = allocation.text(body_hex, common)?;
    let id = put(&raw, &mut material.objects, allocation, common)?;
    if id == body_id {
        return Err(invalid());
    }
    let index = locate(&material.objects, &body_id, common)?;
    let old = &material.objects[index].references;
    let mut refs =
        allocation.vector::<String>(old.len().checked_add(1).ok_or_else(invalid)?, common)?;
    for reference in old {
        refs.push(allocation.text(reference, common)?)
    }
    add_ref(&id, &mut refs, allocation, common)?;
    material.objects[index].references = refs;
    material
        .validate_iws_bounded(common)
        .map_err(NormalPackageErrorV1::Material)
}
