//! Fixed-layout, receipt-authenticated archive for the original publication.
//! Live CAS retention may collect these historical objects after this bundle
//! has been written; the current head remains validated from live CAS.
use super::{Options, Receipt, bridge, files};
use oasis7::world_service::{CommitRef, WorldIdentity};
use oasis7_runtime::{LocalCasStore, blake3_hex};
use serde::{Deserialize, Serialize};
use std::path::Path;

const ARCHIVE_SCHEMA: &str = "oasis7.devlocal.preview-initial-publication/v1";
const ARCHIVE_DIR: &str = "genesis/initial-publication";
const MANIFEST_FILE: &str = "manifest.json";
const MAX_RECORD_BYTES: usize = 1024 * 1024;
const MAX_ENVELOPE_BYTES: usize = 16 * 1024 * 1024;
const MAX_SNAPSHOT_BYTES: usize = 16 * 1024 * 1024;
const MAX_JOURNAL_BYTES: usize = 16 * 1024 * 1024;
const MAX_PROOF_BYTES: usize = 4 * 1024 * 1024;
const MAX_MANIFEST_BYTES: usize = 256 * 1024;

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Manifest {
    schema: String,
    commit: CommitRef,
    record_hash: String,
    envelope_hash: String,
    snapshot_hash: String,
    journal_hash: String,
    proof_hash: String,
}

pub(super) fn publish(
    root: &Path,
    options: &Options,
    identity: &WorldIdentity,
    commit: &CommitRef,
) -> Result<String, String> {
    let directory = root.join(ARCHIVE_DIR);
    files::private_dir(&directory)?;
    let record_path = root
        .join("records")
        .join(format!("{:020}.json", commit.position));
    let record_bytes = files::read_bounded(&record_path, MAX_RECORD_BYTES)?;
    let record: serde_json::Value = serde_json::from_slice(&record_bytes)
        .map_err(|_| "initial publication execution record invalid")?;
    let snapshot_ref = string_field(&record, "snapshot_ref")?;
    let journal_ref = string_field(&record, "journal_ref")?;
    let proof_ref = string_field(&record, "world_head_proof_ref")?;
    let store = LocalCasStore::new(root.join("store"));
    let snapshot = read_cas_bounded(&store, snapshot_ref, MAX_SNAPSHOT_BYTES, "snapshot")?;
    let journal = read_cas_bounded(&store, journal_ref, MAX_JOURNAL_BYTES, "journal")?;
    let proof = read_cas_bounded(&store, proof_ref, MAX_PROOF_BYTES, "proof")?;
    let envelope_path = root
        .join("replication/replication_commit_messages")
        .join(format!("{:020}.json", commit.position));
    let envelope = files::read_bounded(&envelope_path, MAX_ENVELOPE_BYTES)?;

    write_archive_file(&directory, "record.json", &record_bytes)?;
    write_archive_file(&directory, "consensus-envelope.json", &envelope)?;
    write_archive_file(&directory, "snapshot.cbor", &snapshot)?;
    write_archive_file(&directory, "journal.cbor", &journal)?;
    write_archive_file(&directory, "world-head-proof.cbor", &proof)?;
    let manifest = Manifest {
        schema: ARCHIVE_SCHEMA.into(),
        commit: commit.clone(),
        record_hash: blake3_hex(&record_bytes),
        envelope_hash: blake3_hex(&envelope),
        snapshot_hash: blake3_hex(&snapshot),
        journal_hash: blake3_hex(&journal),
        proof_hash: blake3_hex(&proof),
    };
    let manifest_bytes = super::encode(
        serde_json::to_value(&manifest)
            .map_err(|_| "initial publication manifest serialization failed")?,
    )?;
    files::write_new(&directory.join(MANIFEST_FILE), &manifest_bytes)?;
    let manifest_hash = blake3_hex(&manifest_bytes);
    verify_components(root, options, identity, &manifest, commit)?;
    Ok(manifest_hash)
}

pub(super) fn verify(
    root: &Path,
    options: &Options,
    receipt: &Receipt,
    commit: &CommitRef,
    owner: u32,
) -> Result<(), String> {
    let directory = root.join(ARCHIVE_DIR);
    files::verify_private(&directory, true, owner)
        .map_err(|_| "initial publication archive unavailable; restore from a verified backup")?;
    let manifest_bytes = files::read_bounded(&directory.join(MANIFEST_FILE), MAX_MANIFEST_BYTES)
        .map_err(|_| "initial publication archive unavailable; restore from a verified backup")?;
    let manifest_hash = blake3_hex(&manifest_bytes);
    if receipt.initial_publication_proof_hash.as_deref() != Some(manifest_hash.as_str()) {
        return Err(
            "initial publication archive hash mismatch; restore from a verified backup".into(),
        );
    }
    let manifest: Manifest = serde_json::from_slice(&manifest_bytes)
        .map_err(|_| "initial publication archive manifest invalid")?;
    if manifest_bytes
        != super::encode(
            serde_json::to_value(&manifest)
                .map_err(|_| "initial publication manifest serialization failed")?,
        )?
        || manifest.schema != ARCHIVE_SCHEMA
        || &manifest.commit != commit
    {
        return Err("initial publication archive manifest differs from signed receipt".into());
    }
    verify_components(root, options, &receipt.world, &manifest, commit)
}

fn verify_components(
    root: &Path,
    options: &Options,
    identity: &WorldIdentity,
    manifest: &Manifest,
    expected_commit: &CommitRef,
) -> Result<(), String> {
    if manifest.schema != ARCHIVE_SCHEMA || &manifest.commit != expected_commit {
        return Err("initial publication archive commit mismatch".into());
    }
    let directory = root.join(ARCHIVE_DIR);
    let record_bytes = files::read_bounded(&directory.join("record.json"), MAX_RECORD_BYTES)?;
    let envelope = files::read_bounded(
        &directory.join("consensus-envelope.json"),
        MAX_ENVELOPE_BYTES,
    )?;
    let snapshot = files::read_bounded(&directory.join("snapshot.cbor"), MAX_SNAPSHOT_BYTES)?;
    let journal = files::read_bounded(&directory.join("journal.cbor"), MAX_JOURNAL_BYTES)?;
    let proof = files::read_bounded(&directory.join("world-head-proof.cbor"), MAX_PROOF_BYTES)?;
    for (bytes, expected, name) in [
        (&record_bytes, &manifest.record_hash, "record"),
        (&envelope, &manifest.envelope_hash, "signed envelope"),
        (&snapshot, &manifest.snapshot_hash, "snapshot"),
        (&journal, &manifest.journal_hash, "journal"),
        (&proof, &manifest.proof_hash, "world head proof"),
    ] {
        if blake3_hex(bytes) != *expected {
            return Err(format!("initial publication archived {name} hash mismatch"));
        }
    }
    let record: serde_json::Value = serde_json::from_slice(&record_bytes)
        .map_err(|_| "initial publication execution record invalid")?;
    let height = record["height"]
        .as_u64()
        .ok_or("initial publication execution height unavailable")?;
    let snapshot_ref = string_field(&record, "snapshot_ref")?;
    let journal_ref = string_field(&record, "journal_ref")?;
    let proof_ref = string_field(&record, "world_head_proof_ref")?;
    if record["world_id"] != options.world_id
        || height != expected_commit.position
        || height == 0
        || record["latest_state_ref"].as_str() != Some(snapshot_ref)
        || record["execution_state_root"] != blake3_hex(&snapshot)
        || snapshot_ref != manifest.snapshot_hash
        || journal_ref != manifest.journal_hash
        || proof_ref != manifest.proof_hash
        || expected_commit.world != *identity
        || expected_commit.state_root_ref != snapshot_ref
        || record["execution_block_hash"] != expected_commit.execution_block_hash
    {
        return Err(
            "initial publication archive refs differ from the signed original commit".into(),
        );
    }
    super::verify_signed_initial_commit(options, height, &record_bytes, &envelope)?;
    bridge::world_service_read::verify_archived_initial_publication(
        &record_bytes,
        &snapshot,
        &journal,
        &proof,
        identity,
        expected_commit,
    )
}

fn string_field<'a>(value: &'a serde_json::Value, key: &str) -> Result<&'a str, String> {
    value[key]
        .as_str()
        .ok_or_else(|| format!("initial publication {key} unavailable"))
}

fn write_archive_file(directory: &Path, name: &str, bytes: &[u8]) -> Result<(), String> {
    files::write_new(&directory.join(name), bytes)
}

fn read_cas_bounded(
    store: &LocalCasStore,
    content_hash: &str,
    max: usize,
    kind: &str,
) -> Result<Vec<u8>, String> {
    let (bytes, range_complete) = store
        .get_range(content_hash, 0, max.saturating_add(1))
        .map_err(|_| format!("initial publication {kind} CAS unavailable"))?;
    if !range_complete
        || bytes.len() > max
        || bytes.is_empty()
        || blake3_hex(&bytes) != content_hash
    {
        return Err(format!(
            "initial publication {kind} CAS exceeds bounds or failed verification"
        ));
    }
    Ok(bytes)
}
