//! Side-effect-free local record closure decoding before installation.
//! This is not installation authority, pin repair, or a network fallback.
use super::*;
use std::time::Instant;

pub(super) fn read_record(
    store: &LocalCasStore,
    anchor: &VerifiedFormalReadAnchorV1,
    manifest: &FormalFullSyncManifestV1,
    material_limit: u64,
    deadline: Instant,
) -> Result<ClosedRecord, String> {
    let check_deadline = || {
        if Instant::now() >= deadline {
            Err("local record recovery deadline expired".to_string())
        } else {
            Ok(())
        }
    };
    check_deadline()?;
    manifest.validate()?;
    let record = anchor.head().record();
    if manifest.cut != cut_from_anchor(anchor)?
        || manifest.artifact_roots.len() != record.artifact_roots.len()
        || manifest.artifact_roots.iter().any(|root| {
            record.artifact_roots.get(&native_role(root.role)) != Some(&root.content_hash)
        })
    {
        return Err("local record manifest differs from authenticated target".into());
    }
    let total = manifest.objects.iter().try_fold(0u64, |total, object| {
        total
            .checked_add(object.size_bytes)
            .ok_or("local record size overflow")
    })?;
    if total > material_limit {
        return Err("local record closure exceeds original material limit".into());
    }
    super::super::local_io::reject_symlinks(store.root())?;
    super::super::local_io::reject_symlinks(store.blobs_dir())?;
    let mut objects = Vec::with_capacity(manifest.objects.len());
    for descriptor in &manifest.objects {
        check_deadline()?;
        let bytes = read_blob(
            store,
            &descriptor.content_hash,
            descriptor.size_bytes,
            deadline,
        )?;
        objects.push(ArtifactObject {
            content_hash: descriptor.content_hash.clone(),
            bytes,
            references: descriptor.references.clone(),
        });
    }
    let material = ClosedRecord {
        payload_digest: record.payload_digest.clone(),
        before_state_root: record.before_state_root.clone(),
        after_state_root: record.after_state_root.clone(),
        roots: record.artifact_roots.clone(),
        objects,
    };
    verify_record_material(record, &material).map_err(|error| error.to_string())?;
    verify_native(store, &material, material_limit, deadline)?;
    check_deadline()?;
    Ok(material)
}

fn read_blob(
    store: &LocalCasStore,
    digest: &str,
    expected_size: u64,
    deadline: Instant,
) -> Result<Vec<u8>, String> {
    let check_deadline = || {
        if Instant::now() >= deadline {
            Err("local closure recovery deadline expired".to_string())
        } else {
            Ok(())
        }
    };
    check_deadline()?;
    if !valid_hex(digest, 32) || expected_size > MAX_FULL_SYNC_RECORD_BYTES as u64 {
        return Err("local closure object exceeds supported bound".into());
    }
    super::super::local_io::reject_symlinks(store.root())?;
    super::super::local_io::reject_symlinks(store.blobs_dir())?;
    let path = store.blobs_dir().join(format!("{digest}.blob"));
    super::super::local_io::reject_symlinks(&path)?;
    let metadata = std::fs::symlink_metadata(&path).map_err(|e| e.to_string())?;
    if !metadata.file_type().is_file() {
        return Err("local record object is not a regular file".into());
    }
    let mut options = std::fs::OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK);
    }
    let input = options.open(&path).map_err(|e| e.to_string())?;
    if !input.metadata().map_err(|e| e.to_string())?.is_file() {
        return Err("opened local record object is not a regular file".into());
    }
    // Bypass range caches: recovery requires this disk object's current
    // complete decoded bytes, including actual EOF and the declared length.
    let limit =
        usize::try_from(expected_size).map_err(|_| "local record size does not fit memory")?;
    let mut bytes = Vec::new();
    let receipt = oasis7_runtime::decode_local_blob_into(
        store.hash_algorithm(),
        digest,
        input,
        &mut bytes,
        limit,
    )
    .map_err(|error| format!("decode original local record object: {error:?}"))?;
    check_deadline()?;
    if receipt.decoded_bytes as u64 != expected_size {
        return Err("original local record object size mismatch".into());
    }
    Ok(bytes)
}

/// Validate external native chunks from the same local CAS with no network,
/// persistent accounting writes, or cache substitution. This is not execution.
pub(super) fn verify_native(
    store: &LocalCasStore,
    material: &ClosedRecord,
    material_limit: u64,
    deadline: Instant,
) -> Result<(), String> {
    let Some(environment) = material.native_environment().map_err(|e| e.to_string())? else {
        return Ok(());
    };
    let mut budget = CasBudget::new(material_limit);
    for object in &material.objects {
        budget.add(&object.content_hash, object.bytes.len() as u64)?;
    }
    environment.verify(|digest| {
        let declared = environment
            .components
            .iter()
            .flat_map(|component| &component.chunks)
            .find(|chunk| chunk.digest == digest)
            .ok_or("native chunk declaration missing")?;
        let bytes = read_blob(store, digest, declared.byte_len as u64, deadline)?;
        budget.add(digest, bytes.len() as u64)?;
        Ok(bytes)
    })
}
