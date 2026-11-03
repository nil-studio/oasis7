//! Permanent original WAL accounting; this local pin is not a rollback anchor.
use super::*;
pub(super) const SCOPE: &str = "formal-bft-header-auto-prepare-v1";
pub(super) fn shard(checkpoint: &str, object: &str) -> String {
    format!("checkpoint-{checkpoint}-object-{object}")
}
pub(super) fn publication_bytes(length: usize, object: &str) -> Result<usize, String> {
    #[derive(serde::Serialize)]
    struct PinReceipt<'a> {
        pins: std::collections::BTreeSet<&'a str>,
    }
    let pin = serde_json::to_vec(&PinReceipt {
        pins: std::collections::BTreeSet::from([object]),
    })
    .map_err(|_| "encode auto Prepare pin bound")?
    .len();
    length
        .checked_mul(4)
        .and_then(|n| n.checked_add(2 * pin))
        .ok_or("auto Prepare full publication bound overflow".into())
}
pub(super) fn retained_usage(
    records: &Path,
    store: &LocalCasStore,
    material: &str,
    instance: &str,
) -> Result<usize, String> {
    inventory(records, store, material, instance, None)
}
pub(super) fn publication_plan(
    records: &Path,
    store: &LocalCasStore,
    intent: &AutoPrepareIntentV1,
) -> Result<usize, String> {
    inventory(
        records,
        store,
        &intent.owner.material_identity,
        &intent.owner.private_instance,
        Some(intent),
    )
}
fn checked(
    value: &AutoPrepareIntentV1,
    material: &str,
    instance: &str,
    checkpoint: &str,
) -> Result<(), String> {
    if value.schema_version != 1
        || value.domain != DOMAIN
        || value.owner.material_identity != material
        || value.owner.private_instance != instance
        || value
            .body
            .checkpoint
            .digest()
            .map_err(|_| "auto Prepare original digest")?
            != checkpoint
    {
        return Err("original auto Prepare owner/source/codec differs".into());
    }
    value
        .body
        .validate()
        .map_err(|_| "retained auto Prepare body")?;
    value
        .manifest
        .validate_for_checkpoint(&value.body.checkpoint)
        .map_err(|_| "retained auto Prepare manifest".to_string())
}
fn inventory(
    records: &Path,
    store: &LocalCasStore,
    material: &str,
    instance: &str,
    candidate: Option<&AutoPrepareIntentV1>,
) -> Result<usize, String> {
    let candidate_bytes = candidate.map(|v| v.bytes()).transpose()?;
    let candidate_identity = candidate
        .map(|v| {
            v.body
                .checkpoint
                .digest()
                .map_err(|_| "auto Prepare candidate digest")
        })
        .transpose()?;
    let shards = store
        .list_pin_scope_shards(SCOPE)
        .map_err(|_| "inspect original auto Prepare inventory pins")?;
    if shards.len() > 64 {
        return Err("auto Prepare original pin count capacity".into());
    }
    let mut originals = std::collections::BTreeMap::new();
    let mut owner = None;
    let mut source = None;
    for name in shards {
        let (checkpoint, object) = name
            .strip_prefix("checkpoint-")
            .and_then(|n| n.split_once("-object-"))
            .ok_or("unknown auto Prepare pin identity")?;
        if checkpoint.len() != 64 || object.len() != 64 {
            return Err("auto Prepare pin encoding".into());
        }
        let bytes = common_storage::read_cas_bounded(store, object, MAX_INTENT)?;
        let value: AutoPrepareIntentV1 =
            serde_cbor::from_slice(&bytes).map_err(|_| "decode retained auto Prepare")?;
        checked(&value, material, instance, checkpoint)?;
        if value.bytes()? != bytes
            || owner.as_ref().is_some_and(|v| v != &value.owner)
            || source
                .as_ref()
                .is_some_and(|v| v != &value.body.checkpoint.source)
        {
            return Err("auto Prepare pinned original binding differs".into());
        }
        let refs = std::collections::BTreeSet::from([object.to_owned()]);
        store
            .verify_pin_scope_shard(SCOPE, &name, &refs)
            .map_err(|_| "original auto Prepare pin changed")?;
        owner = Some(value.owner);
        source = Some(value.body.checkpoint.source);
        if originals
            .insert(checkpoint.to_owned(), (object.to_owned(), bytes.len()))
            .is_some()
        {
            return Err("multiple original auto Prepare bodies at checkpoint".into());
        }
    }
    if let Some(value) = candidate {
        if owner.as_ref().is_some_and(|v| v != &value.owner)
            || source
                .as_ref()
                .is_some_and(|v| v != &value.body.checkpoint.source)
        {
            return Err("auto Prepare publication changed original source".into());
        }
        let identity = candidate_identity
            .as_ref()
            .ok_or("auto Prepare candidate identity")?;
        checked(value, material, instance, identity)?;
        if let Some((object, _)) = originals.get(identity)
            && *object
                != blake3_hex(
                    candidate_bytes
                        .as_ref()
                        .ok_or("auto Prepare candidate bytes")?,
                )
        {
            return Err("auto Prepare publication changes original body".into());
        }
    }
    let directory = records.join("successor-auto-prepares-v1");
    let entries = match std::fs::symlink_metadata(&directory) {
        Ok(meta) if meta.file_type().is_dir() => {
            Some(std::fs::read_dir(&directory).map_err(|_| "read auto Prepare inventory")?)
        }
        Err(e)
            if e.kind() == std::io::ErrorKind::NotFound
                && (originals.is_empty() || candidate.is_some()) =>
        {
            None
        }
        _ => return Err("retained original auto Prepare directory missing or replaced".into()),
    };
    if entries.is_some() {
        crate::controlled_live_config::require_private_owned(&directory, true)?;
    }
    let mut present = std::collections::BTreeSet::new();
    let mut copies = std::collections::BTreeMap::<String, usize>::new();
    if let Some(entries) = entries {
        for (count, entry) in entries.enumerate() {
            if count >= 256 {
                return Err("auto Prepare file count capacity".into());
            }
            let entry = entry.map_err(|_| "inspect auto Prepare file")?;
            if !entry
                .file_type()
                .map_err(|_| "inspect auto Prepare type")?
                .is_file()
            {
                return Err("auto Prepare inventory nonregular file".into());
            }
            files::single_link(&entry.path())?;
            let name = entry
                .file_name()
                .into_string()
                .map_err(|_| "auto Prepare file name")?;
            let target = files::stage_target(&name).unwrap_or(&name);
            let (checkpoint, reserved) =
                if let Some(id) = target.strip_suffix(".prepare-reservation.cbor") {
                    (id, true)
                } else {
                    (
                        target
                            .strip_suffix(".prepare.cbor")
                            .ok_or("unknown auto Prepare file")?,
                        false,
                    )
                };
            let bytes = common_storage::read_optional_bytes(&entry.path(), MAX_INTENT)?
                .ok_or("auto Prepare file disappeared")?;
            if !originals.contains_key(checkpoint) {
                if !reserved
                    || candidate_identity.as_deref() != Some(checkpoint)
                    || candidate_bytes.as_deref() != Some(bytes.as_slice())
                {
                    return Err("unretained auto Prepare original cannot become a new body".into());
                }
                originals.insert(checkpoint.to_owned(), (blake3_hex(&bytes), bytes.len()));
            }
            let (object, length) = originals
                .get(checkpoint)
                .ok_or("auto Prepare original absent")?;
            if bytes.len() != *length || blake3_hex(&bytes) != *object {
                return Err("original auto Prepare file changed".into());
            }
            if name == target && !reserved {
                present.insert(checkpoint.to_owned());
            }
            *copies.entry(checkpoint.to_owned()).or_insert(1) += 1; // canonical CAS itself
        }
    }
    for checkpoint in originals.keys() {
        if !present.contains(checkpoint) && candidate_identity.as_ref() != Some(checkpoint) {
            return Err("retained original auto Prepare WAL missing".into());
        }
    }
    if let (Some(identity), Some(bytes)) = (candidate_identity, candidate_bytes) {
        originals
            .entry(identity)
            .or_insert((blake3_hex(&bytes), bytes.len()));
    }
    if originals.len() > 64 {
        return Err("auto Prepare original count capacity".into());
    }
    let mut total = 0usize;
    for (checkpoint, (object, length)) in originals {
        let extra = copies
            .get(&checkpoint)
            .copied()
            .unwrap_or(1)
            .saturating_sub(4)
            .checked_mul(length)
            .ok_or("auto Prepare stage byte overflow")?;
        total = total
            .checked_add(publication_bytes(length, &object)?)
            .and_then(|n| n.checked_add(extra))
            .filter(|n| *n <= 512 * 1024 * 1024)
            .ok_or("auto Prepare cumulative material capacity")?;
    }
    Ok(total)
}
