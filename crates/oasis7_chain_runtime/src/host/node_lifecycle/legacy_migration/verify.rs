//! Strict read-only closure checks. Missing material never invokes a default
//! world, startup recovery or repair. Old evidence remains old evidence.
use super::io;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Closure {
    pub chain_id: String,
    pub genesis_ref: String,
    pub committed_height: u64,
    pub execution_block_hash: String,
    pub committed_record_digest: String,
    pub committed_state_root: String,
    pub actual_snapshot_root: String,
    pub journal_len: usize,
    pub journal_sha256: String,
    pub pending_actions: usize,
    pub pending_effects: usize,
    pub inflight_effects: usize,
    pub cognition_digest: String,
    pub feedback_outbox_count: usize,
    pub canonical_snapshot_digest: String,
    pub history_record_count: u64,
    pub execution_cas_blobs: u64,
    pub replication_cas_blobs: u64,
}
fn text<'a>(value: &'a Value, key: &str) -> Result<&'a str, String> {
    value[key]
        .as_str()
        .filter(|v| !v.is_empty())
        .ok_or_else(|| format!("required persisted {key} missing"))
}
fn read_json(path: &Path) -> Result<Value, String> {
    io::regular(path)?;
    io::read(path)
}
const MAX_MIGRATION_CAS_DECODED_BYTES: usize = 64 * 1024 * 1024;
fn decode_cas(root: &Path, digest: &str, output: impl std::io::Write) -> Result<(), String> {
    let path = root.join("blobs").join(format!("{digest}.blob"));
    let file = fs::File::open(io::regular(&path)?).map_err(|e| e.to_string())?;
    oasis7_distfs::decode_local_blob_into(
        oasis7_distfs::HashAlgorithm::Blake3,
        digest,
        file,
        output,
        MAX_MIGRATION_CAS_DECODED_BYTES,
    )
    .map_err(|e| format!("legacy CAS payload unavailable/corrupt: {e:?}"))?;
    Ok(())
}
fn cas(root: &Path) -> Result<u64, String> {
    let dir = root.join("blobs");
    if !dir.is_dir() {
        return Err("existing CAS blobs directory missing".into());
    }
    let mut count = 0;
    for entry in fs::read_dir(&dir).map_err(|e| e.to_string())? {
        let path = entry.map_err(|e| e.to_string())?.path();
        let name = path
            .file_name()
            .and_then(|n| n.to_str())
            .ok_or("invalid CAS filename")?;
        let digest = name
            .strip_suffix(".blob")
            .ok_or("unknown CAS file format")?;
        io::regular(&path)?;
        if digest.len() != 64 || !digest.bytes().all(|b| b.is_ascii_hexdigit()) {
            return Err("CAS filename/content digest mismatch".into());
        }
        // LocalCasStore can encode a disk blob; its identity covers decoded
        // payload bytes, not the filesystem representation being copied.
        decode_cas(root, digest, std::io::sink())?;
        count += 1;
    }
    Ok(count)
}
fn refs(value: &Value, cas: &Path) -> Result<(), String> {
    for name in [
        "latest_state_ref",
        "snapshot_ref",
        "journal_ref",
        "commit_log_ref",
        "checkpoint_ref",
        "external_effect_ref",
        "world_head_proof_ref",
        "controlled_capture_ref",
    ] {
        if let Some(reference) = value[name].as_str()
            && (reference.len() != 64
                || !reference.bytes().all(|b| b.is_ascii_hexdigit())
                || !cas
                    .join("blobs")
                    .join(format!("{reference}.blob"))
                    .is_file())
        {
            return Err(format!(
                "execution record {name} has unavailable CAS material"
            ));
        }
    }
    Ok(())
}
fn verify_record_payload(
    record: &Value,
    cas: &Path,
    world_id: &str,
    height: u64,
) -> Result<Option<oasis7::simulator::WorldKernel>, String> {
    let payload = |reference: &str| {
        let mut decoded = Vec::new();
        decode_cas(cas, reference, &mut decoded)?;
        Ok::<_, String>(decoded)
    };
    // This migration adapter requires complete per-height recovery material.
    // A surviving materialized cache never substitutes for missing history.
    let snapshot_ref = text(record, "snapshot_ref")?;
    let journal_ref = text(record, "journal_ref")?;
    let snapshot_bytes = payload(snapshot_ref)?;
    if oasis7_runtime::blake3_hex(&snapshot_bytes) != text(record, "execution_state_root")? {
        return Err("legacy record snapshot root mismatch".into());
    }
    let snapshot = serde_cbor::from_slice(&snapshot_bytes).map_err(|e| e.to_string())?;
    let journal = serde_cbor::from_slice(&payload(journal_ref)?).map_err(|e| e.to_string())?;
    let restored =
        oasis7_runtime::World::from_snapshot(snapshot, journal).map_err(|e| format!("{e:?}"))?;
    if restored.state().time != height
        || restored.chain_resource_manifest().world_id != world_id
        || record["journal_len"].as_u64() != Some(restored.journal().len() as u64)
    {
        return Err("legacy record restored world/height/journal mismatch".into());
    }
    let mirror = match record.get("simulator_mirror") {
        None | Some(Value::Null) => return Ok(None),
        Some(mirror) if mirror.is_object() => mirror,
        _ => return Err("invalid legacy simulator mirror record".into()),
    };
    refs(mirror, cas)?;
    let mirror_snapshot = payload(text(mirror, "snapshot_ref")?)?;
    if oasis7_runtime::blake3_hex(&mirror_snapshot) != text(mirror, "state_root")? {
        return Err("legacy simulator mirror snapshot root mismatch".into());
    }
    let snapshot: oasis7::simulator::WorldSnapshot = serde_cbor::from_slice(&mirror_snapshot)
        .map_err(|e| format!("legacy simulator snapshot decode: {e}"))?;
    let journal: oasis7::simulator::WorldJournal =
        serde_cbor::from_slice(&payload(text(mirror, "journal_ref")?)?)
            .map_err(|e| format!("legacy simulator journal decode: {e}"))?;
    let restored_mirror = oasis7::simulator::WorldKernel::from_snapshot(snapshot, journal)
        .map_err(|e| format!("legacy simulator mirror rebuild: {e:?}"))?;
    if mirror["journal_len"].as_u64() != Some(restored_mirror.journal().len() as u64) {
        return Err("legacy simulator mirror journal length mismatch".into());
    }
    Ok(Some(restored_mirror))
}

fn verify_physical_mirror(
    world_dir: &Path,
    latest: Option<&oasis7::simulator::WorldKernel>,
    latest_height: Option<u64>,
    world_id: &str,
) -> Result<(), String> {
    let dir = crate::execution_bridge::simulator_world_dir_from_execution_world_dir(world_dir);
    io::regular(&dir.join("snapshot.json"))?;
    io::regular(&dir.join("journal.json"))?;
    // Read existing typed files directly: the live loader's missing-file
    // fallback must never manufacture genesis material during migration.
    let snapshot: oasis7::simulator::WorldSnapshot = io::read(&dir.join("snapshot.json"))?;
    let journal: oasis7::simulator::WorldJournal = io::read(&dir.join("journal.json"))?;
    if let Some(height) = latest_height
        && (snapshot.chain_resource_manifest.manifest_height != height
            || snapshot.chain_resource_manifest.world_id != world_id)
    {
        return Err("physical simulator mirror latest recorded anchor mismatch".into());
    }
    let physical = oasis7::simulator::WorldKernel::from_snapshot(snapshot, journal)
        .map_err(|e| format!("physical simulator mirror rebuild: {e:?}"))?;
    let genesis = oasis7::simulator::WorldKernel::new();
    let expected = latest.unwrap_or(&genesis);
    // Compare explicit normalized views; ordinary snapshot() retains original
    // committed-cut metadata. Never write these comparison views back to disk.
    // Runtime/player extensions and the complete journal remain part of equality.
    let normalized = |kernel: &oasis7::simulator::WorldKernel| {
        kernel.snapshot_with_chain_resource_context(
            oasis7_agent_api::world_resource::ChainResourceDerivationContext::simulator_default(
                kernel.time(),
            ),
        )
    };
    if io::digest(&normalized(&physical))? != io::digest(&normalized(expected))?
        || io::digest(&physical.journal_snapshot())? != io::digest(&expected.journal_snapshot())?
    {
        return Err("physical simulator cache differs from latest recorded mirror or original genesis kernel".into());
    }
    Ok(())
}

pub(super) fn closure(
    paths: &BTreeMap<String, PathBuf>,
    world_id: &str,
    node_id: &str,
) -> Result<Closure, String> {
    let world_dir = &paths["--execution-world-dir"];
    io::regular(&world_dir.join("snapshot.json"))?;
    io::regular(&world_dir.join("journal.json"))?;
    let world = crate::runtime_authority::load_persisted_world_exact(world_dir)?;
    if world.chain_resource_manifest().world_id != world_id {
        return Err("legacy persisted world identity differs from launch source".into());
    }
    let snapshot = world.snapshot();
    if snapshot.tick_consensus_archived_record_count > 0 {
        return Err("formal retained history requires its own migration/fencing adapter".into());
    }
    let manifest = world.chain_resource_manifest();
    let genesis = manifest
        .genesis_ref
        .as_ref()
        .filter(|g| !g.is_empty())
        .ok_or("legacy genesis binding missing")?
        .clone();
    if manifest.chain_id.is_empty() {
        return Err("legacy chain binding missing".into());
    }
    let snapshot_root =
        oasis7_runtime::blake3_hex(&serde_cbor::to_vec(&snapshot).map_err(|e| e.to_string())?);
    let state = read_json(&paths["--execution-bridge-state"])?;
    let height = state["last_applied_committed_height"]
        .as_u64()
        .filter(|h| *h > 0 && *h <= 10_000)
        .ok_or("complete bounded nonempty legacy committed history required")?;
    let block = text(&state, "last_execution_block_hash")?.to_string();
    let state_root = text(&state, "last_execution_state_root")?.to_string();
    let records = &paths["--execution-records-dir"];
    let mut previous: Option<String> = None;
    let mut previous_execution = "genesis".to_string();
    let mut latest_mirror = None;
    let mut latest_mirror_height = None;
    // This adapter does not pretend an opaque cold pack is verified history.
    // Every original height must be available through its legacy hot record.
    for h in 1..=height {
        let record = read_json(&records.join(format!("{h:020}.json")))?;
        if !matches!(record["schema_version"].as_u64().unwrap_or(1), 1..=3)
            || record["world_id"].as_str() != Some(world_id)
            || record["height"].as_u64() != Some(h)
        {
            return Err("legacy execution history schema/world/height mismatch".into());
        }
        if h > 1 && record["prev_node_block_hash"].as_str() != previous.as_deref() {
            return Err("legacy execution history predecessor mismatch".into());
        }
        let journal_len = record["journal_len"]
            .as_u64()
            .ok_or("execution history journal length missing")?;
        let journal_len = usize::try_from(journal_len).map_err(|_| "journal length overflow")?;
        let computed = crate::execution_bridge::legacy_migration_execution_hash(
            world_id,
            h,
            &previous_execution,
            text(&record, "execution_state_root")?,
            journal_len,
        )?;
        if computed != text(&record, "execution_block_hash")? {
            return Err("execution history block hash mismatch".into());
        }
        previous_execution = computed;
        refs(&record, &paths["--storage-root"])?;
        if let Some(mirror) = verify_record_payload(&record, &paths["--storage-root"], world_id, h)?
        {
            latest_mirror = Some(mirror);
            latest_mirror_height = Some(h);
        }
        previous = Some(text(&record, "node_block_hash")?.to_string());
        if h == height
            && (text(&record, "execution_block_hash")? != block
                || text(&record, "execution_state_root")? != state_root
                || record["journal_len"]
                    .as_u64()
                    .is_none_or(|n| n > world.journal().len() as u64))
        {
            return Err("persisted bridge/head/world closure mismatch".into());
        }
    }
    verify_physical_mirror(
        world_dir,
        latest_mirror.as_ref(),
        latest_mirror_height,
        world_id,
    )?;
    if snapshot_root != state_root {
        let pending = read_json(&records.join("product-validation-intent.json"))?;
        if pending["world_id"].as_str() != Some(world_id)
            || pending["height"].as_u64() != height.checked_add(1)
            || pending["pre_step_execution_state_root"].as_str() != Some(&state_root)
            || pending["staged_execution_state_root"].as_str() != Some(&snapshot_root)
            || pending["journal_len"].as_u64() != Some(world.journal().len() as u64)
            || world.state().time != height + 1
        {
            return Err(
                "pending staged world is not bound to original committed predecessor".into(),
            );
        }
    }
    let replication = &paths["--replication-root"];
    let guard: oasis7_distfs::SingleWriterReplicationGuard =
        io::read(&replication.join("replication_guard.json"))?;
    let remote: BTreeMap<String, oasis7_distfs::SingleWriterReplicationGuard> =
        io::read(&replication.join("replication_remote_guards.json"))?;
    if guard.writer_epoch == 0 || remote.values().any(|g| g.writer_epoch == 0) {
        return Err("replication epoch cannot be zero".into());
    }
    let writer = read_json(&replication.join(format!("replication_writer_state_{node_id}.json")))?;
    if writer["writer_epoch"].as_u64().is_none_or(|v| v == 0)
        || writer["last_sequence"].as_u64().is_none()
        || writer["last_replicated_height"].as_u64().is_none()
    {
        return Err("replication writer cursor schema invalid".into());
    }
    Ok(Closure {
        chain_id: manifest.chain_id.clone(),
        genesis_ref: genesis,
        committed_height: height,
        execution_block_hash: block,
        committed_record_digest: io::digest(&read_json(
            &records.join(format!("{height:020}.json")),
        )?)?,
        committed_state_root: state_root,
        actual_snapshot_root: snapshot_root.clone(),
        journal_len: world.journal().len(),
        journal_sha256: super::tree::hash(&world_dir.join("journal.json"))?.0,
        pending_actions: snapshot.pending_actions.len(),
        pending_effects: snapshot.pending_effects.len(),
        inflight_effects: snapshot.inflight_effects.len(),
        cognition_digest: io::digest(&snapshot.cognition)?,
        feedback_outbox_count: world
            .runtime_feedback_outbox()
            .map_err(|e| format!("{e:?}"))?
            .len(),
        canonical_snapshot_digest: snapshot_root,
        history_record_count: height,
        execution_cas_blobs: cas(&paths["--storage-root"])?,
        replication_cas_blobs: cas(&replication.join("store"))?,
    })
}
