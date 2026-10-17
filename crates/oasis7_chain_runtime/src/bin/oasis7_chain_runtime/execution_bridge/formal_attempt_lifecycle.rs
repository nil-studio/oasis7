//! Private attempt outputs are pinned until exact publication and independent
//! P/R readback. The writer lease is the retire fence: no API exposes attempts.
use super::{durable_transaction, to_cbor};
use oasis7_distfs::controlled_authority::formal::{
    coordinator::ExecutableFormalFrozenV1, endpoint::FormalCandidateMaterialV1,
};
use serde::{Deserialize, Serialize};
use std::path::Path;
const MANIFEST: &str = "attempt-lifecycle-v1.cbor";
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(super) enum Phase {
    Active,
    AwaitingDecision,
    QualifiedVerified,
    PublishedClosureVerified,
    Reclaimable,
    Reclaimed,
    Unknown,
    RecoveryPinned,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Attempt {
    schema_version: u32,
    attempt_id: String,
    world: oasis7_distfs::controlled_authority::formal::FormalWorldIdentityV1,
    authority_id: String,
    reservation_id: String,
    request_digest: String,
    position: u64,
    candidate_digest: Option<String>,
    phase: Phase,
    /// Only these exact immutable CAS objects may be removed. Unknown files,
    /// driver records and any external delivery/recovery evidence stay pinned.
    duplicate_outputs: Vec<String>,
    publication_digest: Option<String>,
    independent_closure_digests: Vec<String>,
    protected_obligations: Vec<String>,
}
fn save(path: &Path, value: &Attempt) -> Result<(), String> {
    durable_transaction::write_private_file_durable(&path.join(MANIFEST), &to_cbor(value)?)
}
pub(super) struct ExecutionPin(std::path::PathBuf);
impl Drop for ExecutionPin {
    fn drop(&mut self) {
        if let Ok(mut attempt) = load(&self.0)
            && attempt.phase == Phase::Active
        {
            attempt.phase = Phase::Unknown;
            attempt.protected_obligations = vec![
                "execution did not durably bind an exact candidate; original evidence retained"
                    .into(),
            ];
            let _ = save(&self.0, &attempt);
        }
    }
}
pub(super) fn active(
    path: &Path,
    frozen: &ExecutableFormalFrozenV1,
) -> Result<ExecutionPin, String> {
    let body = &frozen.input().body;
    let attempt = Attempt {
        schema_version: 1,
        attempt_id: path
            .file_name()
            .and_then(|n| n.to_str())
            .ok_or("attempt name")?
            .into(),
        world: body.world.clone(),
        authority_id: body.authority_id.clone(),
        reservation_id: body.reservation_id().map_err(|e| e.to_string())?,
        request_digest: body.request_digest.clone(),
        position: body.next_position,
        candidate_digest: None,
        phase: Phase::Active,
        duplicate_outputs: vec![],
        publication_digest: None,
        independent_closure_digests: vec![],
        protected_obligations: vec![
            "original Frozen reservation; execution outcome unknown".into(),
        ],
    };
    save(path, &attempt)?;
    Ok(ExecutionPin(path.into()))
}
fn load(path: &Path) -> Result<Attempt, String> {
    crate::controlled_live_config::require_private_owned(path, true)?;
    crate::controlled_live_config::require_private_owned(&path.join(MANIFEST), false)?;
    let bytes = crate::controlled_history_cli::read_bounded(&path.join(MANIFEST), 64 * 1024)?;
    let attempt: Attempt = serde_cbor::from_slice(&bytes).map_err(|e| e.to_string())?;
    if attempt.schema_version != 1
        || Some(attempt.attempt_id.as_str()) != path.file_name().and_then(|n| n.to_str())
        || attempt.duplicate_outputs.len() > 256
    {
        return Err("attempt lifecycle identity/budget".into());
    }
    Ok(attempt)
}
pub(super) fn candidate_digest(candidate: &FormalCandidateMaterialV1) -> Result<String, String> {
    Ok(blake3::hash(&to_cbor(&candidate.candidate)?)
        .to_hex()
        .to_string())
}
pub(super) fn awaiting(
    path: &Path,
    candidate: &FormalCandidateMaterialV1,
    outputs: &[String],
) -> Result<(), String> {
    let mut attempt = load(path)?;
    bind(&attempt, candidate)?;
    let store = oasis7::runtime::LocalCasStore::new(path.join("storage"));
    let mut verified = std::collections::BTreeSet::new();
    for id in outputs {
        let bytes = store.get_verified(id).map_err(|e| format!("{e:?}"))?;
        if !redundant(candidate, id, &bytes)? {
            continue; // Unique output remains recovery pinned; it is never a delete candidate.
        }
        verified.insert(id.clone());
    }
    attempt.candidate_digest = Some(candidate_digest(candidate)?);
    attempt.duplicate_outputs = verified.into_iter().collect();
    attempt.phase = Phase::AwaitingDecision;
    attempt.protected_obligations = vec![
        "pending.cbor exact candidate and Frozen".into(),
        "attempt records/world/unknown files and external delivery evidence".into(),
    ];
    save(path, &attempt)
}
fn redundant(
    candidate: &FormalCandidateMaterialV1,
    id: &str,
    bytes: &[u8],
) -> Result<bool, String> {
    if candidate
        .material
        .objects
        .iter()
        .any(|o| o.content_hash == id && o.bytes == bytes)
    {
        return Ok(true);
    }
    // An exact recipe is also a complete alternate representation of the raw
    // snapshot. Both endpoint readbacks traversed every declared base object.
    let snapshot = candidate
        .material
        .snapshot_bytes()
        .map_err(|e| e.to_string())?;
    Ok(blake3::hash(&snapshot).to_hex().as_str() == id && snapshot == bytes)
}
fn bind(attempt: &Attempt, candidate: &FormalCandidateMaterialV1) -> Result<(), String> {
    let frozen = &candidate
        .frozen
        .as_ref()
        .ok_or("attempt candidate Frozen missing")?
        .reservation
        .body;
    if attempt.world != candidate.candidate.record.world
        || attempt.authority_id != frozen.authority_id
        || attempt.reservation_id != frozen.reservation_id().map_err(|e| e.to_string())?
        || attempt.request_digest != frozen.request_digest
        || attempt.position != candidate.candidate.record.position
    {
        return Err("attempt original identity mismatch".into());
    }
    Ok(())
}
pub(super) fn pending_position(path: &Path) -> Result<Option<u64>, String> {
    if !path
        .join(MANIFEST)
        .try_exists()
        .map_err(|e| e.to_string())?
    {
        return Ok(None);
    }
    let attempt = load(path)?;
    Ok(
        (attempt.candidate_digest.is_some() && attempt.phase != Phase::Reclaimed)
            .then_some(attempt.position),
    )
}
pub(super) fn mark(path: &Path, digest: &str, phase: Phase, reason: &str) -> Result<(), String> {
    let mut attempt = load(path)?;
    if attempt.candidate_digest.as_deref() != Some(digest) || attempt.phase == Phase::Reclaimed {
        return Err("attempt mark exact candidate identity/phase".into());
    }
    if attempt.phase == phase
        && attempt
            .protected_obligations
            .last()
            .is_some_and(|last| last == reason)
    {
        return Ok(());
    }
    attempt.phase = phase;
    attempt.protected_obligations.push(reason.into());
    if attempt.protected_obligations.len() > 8 {
        attempt.protected_obligations.remove(0);
    }
    save(path, &attempt)
}
/// The caller has reread exact published bytes and independently traversed both
/// registered destinations. This only unpins the explicit duplicate output list.
pub(super) fn reclaim(
    path: &Path,
    candidate: &FormalCandidateMaterialV1,
    publication_digest: &str,
    independent_closure_digests: &[String],
) -> Result<bool, String> {
    if independent_closure_digests.len() != 2 {
        return Err("attempt lacks both closure readbacks".into());
    }
    let digest = candidate_digest(candidate)?;
    let mut attempt = load(path)?;
    if attempt.candidate_digest.as_ref() != Some(&digest) {
        return Err("attempt reclaim exact candidate identity".into());
    }
    bind(&attempt, candidate)?;
    if attempt.phase == Phase::Reclaimed {
        return Ok(true);
    }
    attempt.phase = Phase::QualifiedVerified;
    save(path, &attempt)?;
    attempt.publication_digest = Some(publication_digest.into());
    attempt.independent_closure_digests = independent_closure_digests.to_vec();
    attempt.phase = Phase::PublishedClosureVerified;
    save(path, &attempt)?;
    // Persist the retire fence before the first unlink. A crash leaves an
    // exact resumable delete list, and retry rechecks destination closure.
    attempt.phase = Phase::Reclaimable;
    save(path, &attempt)?;
    for directory in [path.join("storage"), path.join("storage/blobs")] {
        if !std::fs::symlink_metadata(&directory)
            .map_err(|e| e.to_string())?
            .is_dir()
        {
            return Err("attempt output directory symlink/type refused".into());
        }
    }
    let mut removed = 0usize;
    let mut read_bytes = 0u64;
    for id in &attempt.duplicate_outputs {
        if id.len() != 64
            || !id
                .bytes()
                .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
        {
            return Err("attempt invalid delete identity".into());
        }
        let blob = path.join("storage/blobs").join(format!("{id}.blob"));
        match std::fs::symlink_metadata(&blob) {
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => continue,
            Err(e) => return Err(e.to_string()),
            Ok(meta) if !meta.is_file() || meta.file_type().is_symlink() => {
                return Err("attempt delete path not regular".into());
            }
            Ok(meta) => {
                if meta.len() > 64 * 1024 * 1024 {
                    return Err("attempt duplicate output exceeds object budget".into());
                }
                if removed > 0 && read_bytes.saturating_add(meta.len()) > 8 * 1024 * 1024 {
                    return Ok(false);
                }
                read_bytes += meta.len();
            }
        }
        let store = oasis7::runtime::LocalCasStore::new(path.join("storage"));
        let bytes = store.get_verified(id).map_err(|e| format!("{e:?}"))?;
        if !redundant(candidate, id, &bytes)? {
            return Err("attempt duplicate changed before deletion".into());
        }
        std::fs::remove_file(&blob).map_err(|e| e.to_string())?;
        removed += 1;
        std::fs::File::open(blob.parent().ok_or("attempt blob parent")?)
            .and_then(|d| d.sync_all())
            .map_err(|e| e.to_string())?;
        #[cfg(all(test, unix))]
        if std::env::var("OASIS7_FORMAL_TEST_CRASH_PHASE")
            .ok()
            .as_deref()
            == Some("attempt_output_unlinked")
        {
            // Idle maintenance may reclaim an older publication after the Node
            // has selected another continuation. Bind the real unlink boundary
            // to this attempt's original Frozen input, never the current tick.
            let material = candidate
                .frozen_material
                .as_ref()
                .ok_or("attempt crash boundary original Frozen material missing")?;
            let manifest: super::formal_package::FormalExecutionManifestV1 =
                super::controlled_capture::decode_role(material,
                    oasis7_distfs::controlled_authority::replicated_protocol::ArtifactRole::ExecutionManifest)?;
            super::formal_crash_test_boundary::select_original_input(&manifest.continuation)?;
            super::formal_crash_test_boundary::stop_after_retirement(
                candidate.candidate.record.position,
                &digest,
            )?;
        }
        if removed >= 8 {
            return Ok(false);
        }
    }
    attempt.phase = Phase::Reclaimed;
    attempt.protected_obligations = vec![
        "published exact original and independently readable P/R closure".into(),
        "pending ack/outbox identities remain in pending.cbor and published material".into(),
        "attempt records/world/unknown evidence retained; no time-based sweep".into(),
    ];
    save(path, &attempt)?;
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn failed_execution_pin_records_unknown_and_preserves_unique_evidence() {
        let root = std::env::temp_dir()
            .canonicalize()
            .unwrap()
            .join(format!("oasis7-attempt-unknown-{}", std::process::id()));
        assert!(!root.exists());
        super::super::controlled_live::private_directory(&root).unwrap();
        let state = Attempt {
            schema_version: 1,
            attempt_id: root.file_name().unwrap().to_str().unwrap().into(),
            world: oasis7_distfs::controlled_authority::formal::FormalWorldIdentityV1 {
                world_id: "unknown-world".into(),
                chain_id: "unknown-chain".into(),
                genesis_digest: "0".repeat(64),
            },
            authority_id: "1".repeat(64),
            reservation_id: "2".repeat(64),
            request_digest: "3".repeat(64),
            position: 2,
            candidate_digest: None,
            phase: Phase::Active,
            duplicate_outputs: vec![],
            publication_digest: None,
            independent_closure_digests: vec![],
            protected_obligations: vec!["original unresolved input".into()],
        };
        save(&root, &state).unwrap();
        let payload = root.join("unique-original-effect");
        std::fs::write(&payload, b"original response cannot be recreated").unwrap();
        drop(ExecutionPin(root.clone()));
        assert_eq!(load(&root).unwrap().phase, Phase::Unknown);
        assert!(
            load(&root).unwrap().candidate_digest.is_none(),
            "unknown has no qualified reclaim identity"
        );
        assert_eq!(
            std::fs::read(payload).unwrap(),
            b"original response cannot be recreated"
        );
        std::fs::remove_dir_all(root).unwrap();
    }
}
