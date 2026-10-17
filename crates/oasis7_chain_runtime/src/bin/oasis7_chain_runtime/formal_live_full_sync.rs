//! Issuer-grant-gated access to an exact, qualified full-state World cut.
//!
//! The route is separate from the public projection API. It serves bounded
//! pages of record-closed objects and root-proven retained-index objects from
//! a cached immutable cut after proving that cut beneath the current qualified
//! head. A grant is checked against the current authority epoch on every
//! request; a historical cut or provider-supplied anchor is never trust.
use super::*;
use oasis7::world_service::authority;
use oasis7_client_api::world_service_full_sync::{
    FORMAL_FULL_SYNC_MANIFEST_PATH, FORMAL_FULL_SYNC_OBJECT_PATH,
    FormalFullSyncArtifactRoleV1 as WireRole, FormalFullSyncArtifactRootV1, FormalFullSyncCutV1,
    FormalFullSyncManifestRequestV1, FormalFullSyncManifestV1, FormalFullSyncObjectChunkV1,
    FormalFullSyncObjectDescriptorV1, FormalFullSyncObjectRequestV1, FormalNodeReadGrantV1,
    MAX_FULL_SYNC_RECORD_BYTES,
};
use oasis7_distfs::controlled_authority::{
    formal::{business_index::BusinessIndexNodeKindV1, verify_record_material},
    replicated_protocol::{ArtifactRole, ClosedRecord},
};
use std::time::{Duration, Instant};

const MAX_BUSINESS_INDEX_NODE_BYTES: usize = 4 * 1024 * 1024 + 128;
const MAX_FULL_SYNC_CACHE_ENTRIES: usize = 4;
const MAX_FULL_SYNC_CACHE_BYTES: usize = 32 * 1024 * 1024;
const FULL_SYNC_CACHE_IDLE_TTL: Duration = Duration::from_secs(10 * 60);
const MAX_ANCESTRY_PAGE_COMMITS: usize = 64;
const MAX_ANCESTRY_QUALIFICATION_TIME: Duration = Duration::from_secs(30);

pub(super) fn handle_manifest(service: &FormalLiveService, body: &[u8]) -> HttpReply {
    let path = FORMAL_FULL_SYNC_MANIFEST_PATH;
    let signed: SignedReadRequest<FormalFullSyncManifestRequestV1> = match parse_strict(body) {
        Ok(request) => request,
        Err(error) => return raw_error(path, body, "invalid_request", &error, &service.signer),
    };
    let digest = authority::request_digest(path, &signed);
    let result = (|| {
        signed.request.validate()?;
        authority::verify_read_request(path, &signed)?;
        let current = authorize_cut(
            service,
            &signed.request.grant,
            &signed.request.cut,
            &signed.subject_public_key,
        )?;
        let (anchor, material) = material_for_cut(service, &current.anchor, &signed.request.cut)?;
        let manifest = manifest(&anchor, &material)?;
        reauthorize_for_response(
            service,
            &signed.request.grant,
            &signed.request.cut,
            &signed.subject_public_key,
        )?;
        Ok::<_, String>(manifest)
    })();
    match result {
        Ok(payload) => signed_reply_with_digest(200, path, digest, &payload, &service.signer),
        Err(error) => rejected(path, digest, &error, service),
    }
}

pub(super) fn handle_object(service: &FormalLiveService, body: &[u8]) -> HttpReply {
    let path = FORMAL_FULL_SYNC_OBJECT_PATH;
    let signed: SignedReadRequest<FormalFullSyncObjectRequestV1> = match parse_strict(body) {
        Ok(request) => request,
        Err(error) => return raw_error(path, body, "invalid_request", &error, &service.signer),
    };
    let digest = authority::request_digest(path, &signed);
    let result = (|| {
        signed.request.validate()?;
        authority::verify_read_request(path, &signed)?;
        let current = authorize_cut(
            service,
            &signed.request.grant,
            &signed.request.cut,
            &signed.subject_public_key,
        )?;
        let (anchor, material) = material_for_cut(service, &current.anchor, &signed.request.cut)?;
        let retained_root = retained_history_root(&material)?;
        let bytes = if let Some(object) = material
            .objects
            .iter()
            .find(|object| object.content_hash == signed.request.content_hash)
        {
            if signed.request.business_path.is_some()
                || signed.request.retained_history_root_digest.is_some()
            {
                return Err("record-closure object cannot claim retained-tree metadata".into());
            }
            object.bytes.clone()
        } else if let Some(chunk) = material
            .native_environment()
            .map_err(|error| error.to_string())?
            .and_then(|environment| {
                environment
                    .components
                    .into_iter()
                    .flat_map(|component| component.chunks)
                    .find(|chunk| chunk.digest == signed.request.content_hash)
            })
        {
            if signed.request.business_path.is_some()
                || signed.request.retained_history_root_digest.is_some()
            {
                return Err("native chunk cannot claim a retained-tree path".into());
            }
            let bytes = service
                .history
                .full_sync_business_object(&anchor, &chunk.digest)?
                .ok_or("full-sync original native chunk unavailable")?;
            if bytes.len() != chunk.byte_len as usize {
                return Err("full-sync original native chunk length".into());
            }
            bytes
        } else {
            let retained_root = retained_root
                .as_deref()
                .ok_or("qualified snapshot has no retained tree")?;
            if signed.request.retained_history_root_digest.as_deref() != Some(retained_root) {
                return Err(
                    "formal full-sync retained root differs from qualified snapshot".into(),
                );
            }
            let path = signed
                .request
                .business_path
                .as_deref()
                .ok_or("full-sync CAS object is outside the qualified record closure")?;
            verify_business_path(
                service,
                &anchor,
                &material,
                retained_root,
                path,
                &signed.request.content_hash,
            )?
        };
        if bytes.len() > MAX_FULL_SYNC_RECORD_BYTES
            || blake3::hash(&bytes).to_hex().as_str() != signed.request.content_hash
        {
            return Err("formal full-sync object hash/size mismatch".into());
        }
        let start = usize::try_from(signed.request.offset)
            .map_err(|_| "formal full-sync offset exceeds platform bound")?;
        if start > bytes.len() {
            return Err("formal full-sync offset exceeds object".into());
        }
        let end = start
            .saturating_add(signed.request.max_bytes as usize)
            .min(bytes.len());
        let payload = FormalFullSyncObjectChunkV1 {
            schema_version: 1,
            cut: signed.request.cut.clone(),
            content_hash: signed.request.content_hash.clone(),
            total_bytes: bytes.len() as u64,
            offset: signed.request.offset,
            bytes: bytes[start..end].to_vec(),
        };
        reauthorize_for_response(
            service,
            &signed.request.grant,
            &signed.request.cut,
            &signed.subject_public_key,
        )?;
        Ok::<_, String>(payload)
    })();
    match result {
        Ok(payload) => signed_reply_with_digest(200, path, digest, &payload, &service.signer),
        Err(error) => rejected(path, digest, &error, service),
    }
}

fn authorize_cut(
    service: &FormalLiveService,
    grant: &FormalNodeReadGrantV1,
    requested: &FormalFullSyncCutV1,
    subject_public_key: &str,
) -> Result<PublishedReadCut, String> {
    if !service.runtime_status().ready {
        return Err("formal runtime is not ready for full-state sync".into());
    }
    let current = service.published.current()?;
    authorize_grant_for_current(
        service,
        grant,
        requested,
        subject_public_key,
        &current.anchor,
    )?;
    Ok(current)
}

fn authorize_grant_for_current(
    service: &FormalLiveService,
    grant: &FormalNodeReadGrantV1,
    requested: &FormalFullSyncCutV1,
    subject_public_key: &str,
    current: &VerifiedFormalReadAnchorV1,
) -> Result<(), String> {
    if !service.runtime_status().ready {
        return Err("formal runtime is not ready for full-state sync".into());
    }
    let current_cut = cut_from_anchor(current)?;
    if subject_public_key != grant.body.node_public_key {
        return Err("read grant node binding mismatch".into());
    }
    if requested.world != current_cut.world
        || requested.chain_id != current_cut.chain_id
        || requested.position > current_cut.position
    {
        return Err("full-sync request is outside the current qualified World history".into());
    }
    grant.verify(
        &service.trust.initial_trust.issuer_public_key,
        &current_cut.world,
        &current_cut.chain_id,
        subject_public_key,
        &grant.body.role,
        current_cut.authority_epoch,
        now_unix_ms(),
    )
}

fn cut_from_anchor(anchor: &VerifiedFormalReadAnchorV1) -> Result<FormalFullSyncCutV1, String> {
    let record = anchor.head().record();
    Ok(FormalFullSyncCutV1 {
        world: WorldIdentity {
            world_id: record.world.world_id.clone(),
            genesis_digest: record.world.genesis_digest.clone(),
        },
        chain_id: record.world.chain_id.clone(),
        position: record.position,
        record_id: record.record_id().map_err(|error| error.to_string())?,
        after_state_root: record.after_state_root.clone(),
        active_authority_id: anchor
            .authority()
            .authority_id()
            .map_err(|error| error.to_string())?,
        finalizing_authority_id: anchor.head().finalizing_authority_id().to_owned(),
        authority_epoch: anchor.authority().definition().epoch,
    })
}

fn material_for_cut(
    service: &FormalLiveService,
    current: &VerifiedFormalReadAnchorV1,
    cut: &FormalFullSyncCutV1,
) -> Result<(VerifiedFormalReadAnchorV1, ClosedRecord), String> {
    if let Some(entry) = cached_entry(service, cut)? {
        qualify_cached_entry(service, cut, current, &entry.qualified_head)?;
        let material = decode_cached_material(&entry.serialized_material)?;
        verify_record_material(entry.anchor.head().record(), &material)
            .map_err(|error| error.to_string())?;
        return Ok((entry.anchor, material));
    }

    let origin = service.history.origin_publication(&FormalReadAnchorRefV1 {
        position: cut.position,
        record_id: cut.record_id.clone(),
        active_authority_id: cut.active_authority_id.clone(),
    })?;
    let anchor = origin.anchor().clone();
    if cut_from_anchor(&anchor)? != *cut {
        return Err("requested full-sync cut differs from externally qualified history".into());
    }
    verify_descendant(service, &anchor, current)?;
    let material = service.history.full_sync_material(&anchor)?;
    verify_record_material(anchor.head().record(), &material).map_err(|error| error.to_string())?;
    let serialized = serde_cbor::to_vec(&material)
        .map_err(|error| format!("serialize full-sync closure for cache: {error}"))?;
    if serialized.len() > MAX_FULL_SYNC_CACHE_BYTES {
        return Err("full-sync serialized closure exceeds bounded cache capacity".into());
    }
    cache_material(
        service,
        cut.clone(),
        anchor.clone(),
        current.clone(),
        serialized,
    )?;
    Ok((anchor, material))
}

fn cached_entry(
    service: &FormalLiveService,
    cut: &FormalFullSyncCutV1,
) -> Result<Option<FullSyncCacheSnapshot>, String> {
    let now = Instant::now();
    let mut cache = service
        .full_sync_material_cache
        .lock()
        .map_err(|_| "formal full-sync material cache poisoned")?;
    prune_cache(&mut cache, now);
    let Some(index) = cache.entries.iter().position(|entry| &entry.cut == cut) else {
        return Ok(None);
    };
    let mut entry = cache.entries.remove(index).expect("located cache entry");
    entry.last_used = now;
    let snapshot = FullSyncCacheSnapshot {
        anchor: entry.anchor.clone(),
        qualified_head: entry.qualified_head.clone(),
        serialized_material: Arc::clone(&entry.serialized_material),
    };
    cache.entries.push_back(entry);
    Ok(Some(snapshot))
}

struct FullSyncCacheSnapshot {
    anchor: VerifiedFormalReadAnchorV1,
    qualified_head: VerifiedFormalReadAnchorV1,
    serialized_material: std::sync::Arc<[u8]>,
}

fn cache_material(
    service: &FormalLiveService,
    cut: FormalFullSyncCutV1,
    anchor: VerifiedFormalReadAnchorV1,
    qualified_head: VerifiedFormalReadAnchorV1,
    serialized: Vec<u8>,
) -> Result<(), String> {
    if serialized.len() > MAX_FULL_SYNC_CACHE_BYTES {
        return Err("full-sync serialized closure exceeds bounded cache capacity".into());
    }
    let now = Instant::now();
    let mut cache = service
        .full_sync_material_cache
        .lock()
        .map_err(|_| "formal full-sync material cache poisoned")?;
    prune_cache(&mut cache, now);
    if let Some(index) = cache.entries.iter().position(|entry| entry.cut == cut) {
        let old = cache.entries.remove(index).expect("located cache entry");
        cache.serialized_bytes = cache
            .serialized_bytes
            .saturating_sub(old.serialized_material.len());
    }
    while cache.entries.len() >= MAX_FULL_SYNC_CACHE_ENTRIES
        || cache.serialized_bytes.saturating_add(serialized.len()) > MAX_FULL_SYNC_CACHE_BYTES
    {
        let Some(evicted) = cache.entries.pop_front() else {
            return Err("formal full-sync cache accounting invariant failed".into());
        };
        cache.serialized_bytes = cache
            .serialized_bytes
            .saturating_sub(evicted.serialized_material.len());
    }
    cache.serialized_bytes = cache.serialized_bytes.saturating_add(serialized.len());
    cache.entries.push_back(super::FullSyncMaterialCacheEntry {
        cut,
        anchor,
        qualified_head,
        serialized_material: std::sync::Arc::from(serialized),
        last_used: now,
    });
    Ok(())
}

fn update_cache_qualified_head(
    service: &FormalLiveService,
    cut: &FormalFullSyncCutV1,
    current: &VerifiedFormalReadAnchorV1,
) -> Result<(), String> {
    let now = Instant::now();
    let mut cache = service
        .full_sync_material_cache
        .lock()
        .map_err(|_| "formal full-sync material cache poisoned")?;
    prune_cache(&mut cache, now);
    let index = cache
        .entries
        .iter()
        .position(|entry| &entry.cut == cut)
        .ok_or("formal full-sync material was evicted during request")?;
    let mut entry = cache.entries.remove(index).expect("located cache entry");
    entry.qualified_head = current.clone();
    entry.last_used = now;
    cache.entries.push_back(entry);
    Ok(())
}

fn prune_cache(cache: &mut super::FullSyncMaterialCache, now: Instant) {
    cache
        .entries
        .retain(|entry| now.saturating_duration_since(entry.last_used) < FULL_SYNC_CACHE_IDLE_TTL);
    cache.serialized_bytes = cache
        .entries
        .iter()
        .map(|entry| entry.serialized_material.len())
        .sum();
}

fn decode_cached_material(bytes: &[u8]) -> Result<ClosedRecord, String> {
    let material: ClosedRecord = serde_cbor::from_slice(bytes)
        .map_err(|error| format!("decode cached full-sync closure: {error}"))?;
    if serde_cbor::to_vec(&material)
        .map_err(|error| format!("re-encode cached full-sync closure: {error}"))?
        != bytes
    {
        return Err("cached full-sync closure is not canonical CBOR".into());
    }
    Ok(material)
}

fn qualify_cached_entry(
    service: &FormalLiveService,
    cut: &FormalFullSyncCutV1,
    current: &VerifiedFormalReadAnchorV1,
    previously_qualified: &VerifiedFormalReadAnchorV1,
) -> Result<(), String> {
    if cut_from_anchor(previously_qualified)? != cut_from_anchor(current)? {
        verify_descendant(service, previously_qualified, current)?;
        update_cache_qualified_head(service, cut, current)?;
    }
    Ok(())
}

fn verify_descendant(
    service: &FormalLiveService,
    ancestor: &VerifiedFormalReadAnchorV1,
    descendant: &VerifiedFormalReadAnchorV1,
) -> Result<(), String> {
    let deadline = Instant::now()
        .checked_add(MAX_ANCESTRY_QUALIFICATION_TIME)
        .ok_or("HistoryUnavailable: full-sync ancestry deadline overflow")?;
    let ancestor_cut = cut_from_anchor(ancestor)?;
    let descendant_cut = cut_from_anchor(descendant)?;
    if ancestor_cut.world != descendant_cut.world
        || ancestor_cut.chain_id != descendant_cut.chain_id
        || ancestor_cut.position > descendant_cut.position
    {
        return Err(
            "full-sync requested cut is not an ancestor of the current qualified head".into(),
        );
    }
    let mut cursor = ancestor.clone();
    while cursor.head().record().position < descendant.head().record().position {
        check_ancestry_deadline(deadline)?;
        let next =
            service
                .history
                .forward_publication(&cursor, descendant, MAX_ANCESTRY_PAGE_COMMITS)?;
        check_ancestry_deadline(deadline)?;
        let next_anchor = next.anchor().clone();
        let cursor_position = cursor.head().record().position;
        let next_position = next_anchor.head().record().position;
        if next_position <= cursor_position
            || next_position > descendant.head().record().position
            || next_position - cursor_position > MAX_ANCESTRY_PAGE_COMMITS as u64
        {
            return Err(
                "full-sync ancestry resolver returned an invalid bounded page target".into(),
            );
        }
        let page = service.history.history_page(
            &cursor,
            &next_anchor,
            oasis7::world_service::formal_read::MAX_FORMAL_HISTORY_STEPS,
        )?;
        check_ancestry_deadline(deadline)?;
        if page.schema_version != 1
            || page.steps.len() > oasis7::world_service::formal_read::MAX_FORMAL_HISTORY_STEPS
            || !page.steps.iter().any(|step| {
                matches!(
                    step,
                    oasis7::world_service::formal_read::FormalReadStepV1::Commit(_)
                )
            })
        {
            return Err("full-sync ancestry page is empty or exceeds its proof bound".into());
        }
        page.encode_hex()?;
        cursor = next_anchor;
    }
    if cut_from_anchor(&cursor)? != descendant_cut {
        check_ancestry_deadline(deadline)?;
        let page = service.history.history_page(
            &cursor,
            descendant,
            oasis7::world_service::formal_read::MAX_FORMAL_HISTORY_STEPS,
        )?;
        check_ancestry_deadline(deadline)?;
        if page.schema_version != 1
            || page.steps.len() > oasis7::world_service::formal_read::MAX_FORMAL_HISTORY_STEPS
            || page.steps.is_empty()
        {
            return Err("full-sync authority transition exceeds its proof bound".into());
        }
        page.encode_hex()?;
        cursor = descendant.clone();
    }
    if cut_from_anchor(&cursor)? != descendant_cut {
        return Err("full-sync ancestry did not reach the exact current qualified cut".into());
    }
    Ok(())
}

fn check_ancestry_deadline(deadline: Instant) -> Result<(), String> {
    if Instant::now() >= deadline {
        Err("HistoryUnavailable: full-sync ancestry qualification deadline exceeded".into())
    } else {
        Ok(())
    }
}

fn manifest(
    anchor: &VerifiedFormalReadAnchorV1,
    material: &ClosedRecord,
) -> Result<FormalFullSyncManifestV1, String> {
    verify_record_material(anchor.head().record(), material).map_err(|error| error.to_string())?;
    let retained_history_root_digest = retained_history_root(material)?;
    let cut = cut_from_anchor(anchor)?;
    let artifact_roots = material
        .roots
        .iter()
        .map(|(role, content_hash)| FormalFullSyncArtifactRootV1 {
            role: wire_role(*role),
            content_hash: content_hash.clone(),
        })
        .collect();
    let objects = material
        .objects
        .iter()
        .map(|object| FormalFullSyncObjectDescriptorV1 {
            content_hash: object.content_hash.clone(),
            size_bytes: object.bytes.len() as u64,
            references: object.references.clone(),
        })
        .collect();
    let manifest = FormalFullSyncManifestV1 {
        schema_version: if material
            .roots
            .contains_key(&ArtifactRole::NativeExecutionEnvironment)
        {
            2
        } else {
            1
        },
        cut,
        artifact_roots,
        objects,
        retained_history_root_digest,
    };
    manifest.validate()?;
    Ok(manifest)
}

fn wire_role(role: ArtifactRole) -> WireRole {
    match role {
        ArtifactRole::Input => WireRole::Input,
        ArtifactRole::Result => WireRole::Result,
        ArtifactRole::Snapshot => WireRole::Snapshot,
        ArtifactRole::Journal => WireRole::Journal,
        ArtifactRole::NonceIndex => WireRole::NonceIndex,
        ArtifactRole::EffectOutbox => WireRole::EffectOutbox,
        ArtifactRole::ExecutionManifest => WireRole::ExecutionManifest,
        ArtifactRole::Wasm => WireRole::Wasm,
        ArtifactRole::Rules => WireRole::Rules,
        ArtifactRole::NativeExecutionEnvironment => WireRole::NativeExecutionEnvironment,
    }
}

fn retained_history_root(material: &ClosedRecord) -> Result<Option<String>, String> {
    let bytes = material.snapshot_bytes().map_err(|e| e.to_string())?;
    let snapshot: oasis7::runtime::Snapshot =
        serde_cbor::from_slice(&bytes).map_err(|error| error.to_string())?;
    Ok(snapshot
        .state
        .retained_history_root
        .as_ref()
        .map(|root| root.root_digest.clone()))
}

fn verify_business_path(
    service: &FormalLiveService,
    anchor: &VerifiedFormalReadAnchorV1,
    material: &ClosedRecord,
    root_digest: &str,
    path: &[String],
    target: &str,
) -> Result<Vec<u8>, String> {
    let bytes = material.snapshot_bytes().map_err(|e| e.to_string())?;
    let snapshot: oasis7::runtime::Snapshot =
        serde_cbor::from_slice(&bytes).map_err(|error| error.to_string())?;
    let retained = snapshot
        .state
        .retained_history_root
        .as_ref()
        .ok_or("full-sync Snapshot has no retained history")?;
    if retained.root_digest != root_digest
        || path.len() > 257
        || path.first() != Some(&retained.index_root.node_root)
        || path.last().map(String::as_str) != Some(target)
    {
        return Err("full-sync retained path does not start at current root".into());
    }
    for (depth, pair) in path.windows(2).enumerate() {
        let parent_bytes = service
            .history
            .full_sync_business_object(anchor, &pair[0])?
            .ok_or("full-sync retained parent object missing")?;
        if blake3::hash(&parent_bytes).to_hex().as_str() != pair[0] {
            return Err("full-sync retained parent hash mismatch".into());
        }
        let node = oasis7_distfs::controlled_authority::formal::business_index::BusinessIndexNodeV1::decode(&parent_bytes)?;
        match node.kind {
            BusinessIndexNodeKindV1::Branch {
                depth: node_depth,
                left_hash,
                right_hash,
            } if node_depth as usize == depth
                && (pair[1] == left_hash || pair[1] == right_hash) => {}
            _ => return Err("full-sync retained path is not a root branch chain".into()),
        }
    }
    let bytes = service
        .history
        .full_sync_business_object(anchor, target)?
        .ok_or("full-sync retained target object missing")?;
    if bytes.len() > MAX_BUSINESS_INDEX_NODE_BYTES
        || blake3::hash(&bytes).to_hex().as_str() != target
    {
        return Err("full-sync retained target hash/size mismatch".into());
    }
    let node =
        oasis7_distfs::controlled_authority::formal::business_index::BusinessIndexNodeV1::decode(
            &bytes,
        )?;
    match node.kind {
        BusinessIndexNodeKindV1::Branch { depth, .. } if depth as usize + 1 == path.len() => {}
        BusinessIndexNodeKindV1::Leaf(_) if path.len() == 257 => {}
        _ => return Err("full-sync retained target depth/shape mismatch".into()),
    }
    Ok(bytes)
}

fn reauthorize_for_response(
    service: &FormalLiveService,
    grant: &FormalNodeReadGrantV1,
    requested: &FormalFullSyncCutV1,
    subject_public_key: &str,
) -> Result<(), String> {
    let current = service.published.current()?;
    authorize_grant_for_current(
        service,
        grant,
        requested,
        subject_public_key,
        &current.anchor,
    )?;
    let entry = cached_entry(service, requested)?
        .ok_or("formal full-sync material was evicted before response")?;
    qualify_cached_entry(service, requested, &current.anchor, &entry.qualified_head)
}

fn rejected(
    path: &str,
    digest: Result<String, String>,
    error: &str,
    service: &FormalLiveService,
) -> HttpReply {
    let (status, code) = if error.contains("HistoryUnavailable") || error.contains("not available")
    {
        (410, "history_unavailable")
    } else if error.contains("not ready") {
        (503, "not_ready")
    } else if error.contains("grant") || error.contains("authorized") || error.contains("node") {
        (403, "read_grant_rejected")
    } else {
        (400, "full_sync_rejected")
    };
    signed_error_with_digest(status, path, digest, code, error, &service.signer)
}
