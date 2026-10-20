//! Test-executable-only synchronization with the external SIGKILL parent.
//! The parent observes an fsynced phase marker and kills this entire process.
//! No production build includes this module or reads these environment names.
use std::{fs::OpenOptions, io::Write, os::unix::fs::OpenOptionsExt, path::PathBuf};

static ACTIVE_CRASH_BLOCK: std::sync::Mutex<Option<String>> = std::sync::Mutex::new(None);
/// Recovery child rejects even a transient recreation of this exact released job.
pub(crate) fn forbid_release_reverification(digest: &str) -> Result<(), String> {
    let Ok(expected) = std::env::var("OASIS7_FORMAL_TEST_FORBID_REVERIFY_DIGEST") else {
        return Ok(());
    };
    if expected != digest {
        return Ok(());
    }
    let marker = PathBuf::from(
        std::env::var_os("OASIS7_FORMAL_TEST_FORBID_REVERIFY_MARKER")
            .ok_or("reverify observation path missing")?,
    );
    super::durable_transaction::write_private_file_durable(&marker, digest.as_bytes())?;
    Err("test forbids recreating the exact ReleasePending closure verification job".into())
}
pub(crate) fn release_target_matches(digest: &str) -> Result<bool, String> {
    let Some(marker) = std::env::var_os("OASIS7_FORMAL_TEST_CRASH_MARKER") else {
        return Ok(false);
    };
    let target = PathBuf::from(marker).with_extension("release-target.cbor");
    if !target.exists() {
        return Ok(false);
    }
    let bytes = crate::controlled_history_cli::read_bounded(&target, 1024)?;
    let (_, expected): (u64, String) = serde_cbor::from_slice(&bytes).map_err(|e| e.to_string())?;
    Ok(expected == digest)
}
pub(crate) fn release_transport_failure(digest: &str, phase: &str) -> Result<(), String> {
    let marker = PathBuf::from(
        std::env::var_os("OASIS7_FORMAL_TEST_RELEASE_FAILURE_MARKER")
            .ok_or("release failure observation path missing")?,
    );
    if !marker.exists() {
        let bytes = serde_cbor::to_vec(&(digest, phase)).map_err(|e| e.to_string())?;
        super::durable_transaction::write_private_file_durable(&marker, &bytes)?;
    }
    Ok(())
}
pub(crate) fn attempt_release_boundary(
    phase: &str,
    candidate: &oasis7_distfs::controlled_authority::formal::endpoint::FormalCandidateMaterialV1,
) -> Result<(), String> {
    let selected = std::env::var("OASIS7_FORMAL_TEST_CRASH_PHASE").unwrap_or_default();
    if !selected.starts_with("release_") {
        return Ok(());
    }
    let material = candidate
        .frozen_material
        .as_ref()
        .ok_or("release boundary Frozen material")?;
    let manifest: super::formal_package::FormalExecutionManifestV1 =
        super::controlled_capture::decode_role(material,
            oasis7_distfs::controlled_authority::replicated_protocol::ArtifactRole::ExecutionManifest)?;
    select_original_input(&manifest.continuation)?;
    if ACTIVE_CRASH_BLOCK
        .lock()
        .map_err(|_| "release target lock poisoned")?
        .is_none()
    {
        return Ok(());
    }
    let marker = PathBuf::from(
        std::env::var_os("OASIS7_FORMAL_TEST_CRASH_MARKER").ok_or("release marker path")?,
    );
    let target = marker.with_extension("release-target.cbor");
    let digest = super::formal_attempt_lifecycle::candidate_digest(candidate)?;
    let bytes = serde_cbor::to_vec(&(candidate.candidate.record.position, digest))
        .map_err(|e| e.to_string())?;
    if !target.exists() {
        super::durable_transaction::write_private_file_durable(&target, &bytes)?;
    } else if std::fs::read(&target).map_err(|e| e.to_string())? != bytes {
        return Err("release target changed".into());
    }
    stop_after(phase)
}
/// Only complete original input from this actual committed continuation can
/// select a crash block. Empty heartbeat work never satisfies this predicate.
pub(crate) fn select_original_input(
    original: &oasis7_node::NodeLocalExecutionContinuation,
) -> Result<(), String> {
    let mut active = ACTIVE_CRASH_BLOCK
        .lock()
        .map_err(|_| "crash target lock poisoned")?;
    *active = None;
    if std::env::var_os("OASIS7_FORMAL_TEST_CRASH_PHASE").is_none() {
        return Ok(());
    }
    let path = std::env::var_os("OASIS7_FORMAL_TEST_CRASH_REQUEST")
        .ok_or("crash target original request missing")?;
    let bytes =
        crate::controlled_history_cli::read_bounded(std::path::Path::new(&path), 1024 * 1024)?;
    let expected: oasis7::world_service::SubmitIntentRequest<
        oasis7::world_service::WorldServicePayloadV1,
    > = serde_cbor::from_slice(&bytes).map_err(|e| e.to_string())?;
    expected.validate().map_err(|e| e.to_string())?;
    let (_, _, _, inputs) =
        super::driver_replicated_input::decode_committed_actions(&original.context)?;
    let expected_bytes = serde_cbor::to_vec(&expected).map_err(|e| e.to_string())?;
    for (_, input) in inputs {
        if serde_cbor::to_vec(&input).map_err(|e| e.to_string())? == expected_bytes {
            if original.context.node_block_hash.trim().is_empty() {
                return Err("crash target empty original block hash".into());
            }
            *active = Some(original.context.node_block_hash.clone());
            break;
        }
    }
    Ok(())
}
/// The real retired attempt may precede the writer's current heartbeat.
/// This test receipt identifies that exact attempt; it is never delete authority.
pub(crate) fn stop_after_retirement(position: u64, candidate_digest: &str) -> Result<(), String> {
    let block = ACTIVE_CRASH_BLOCK
        .lock()
        .map_err(|_| "crash target lock poisoned")?
        .clone();
    if let Some(block) = block {
        let marker = PathBuf::from(
            std::env::var_os("OASIS7_FORMAL_TEST_CRASH_MARKER")
                .ok_or("crash phase marker path missing")?,
        );
        let target = marker.with_extension("retirement-target.cbor");
        let bytes = serde_cbor::to_vec(&(1u32, position, block, candidate_digest))
            .map_err(|error| error.to_string())?;
        let mut file = OpenOptions::new()
            .create_new(true)
            .write(true)
            .mode(0o600)
            .custom_flags(libc::O_NOFOLLOW)
            .open(&target)
            .map_err(|error| error.to_string())?;
        file.write_all(&bytes)
            .and_then(|_| file.sync_all())
            .map_err(|error| error.to_string())?;
        std::fs::File::open(target.parent().ok_or("crash target parent missing")?)
            .and_then(|directory| directory.sync_all())
            .map_err(|error| error.to_string())?;
    }
    stop_after("attempt_output_unlinked")
}
pub(crate) fn stop_after(phase: &str) -> Result<(), String> {
    if std::env::var("OASIS7_FORMAL_TEST_CRASH_PHASE")
        .ok()
        .as_deref()
        != Some(phase)
    {
        return Ok(());
    }
    if ACTIVE_CRASH_BLOCK
        .lock()
        .map_err(|_| "crash target lock poisoned")?
        .is_none()
    {
        return Ok(());
    }
    let marker = PathBuf::from(
        std::env::var_os("OASIS7_FORMAL_TEST_CRASH_MARKER")
            .ok_or("crash phase marker path missing")?,
    );
    let mut file = OpenOptions::new()
        .create_new(true)
        .write(true)
        .mode(0o600)
        .custom_flags(libc::O_NOFOLLOW)
        .open(&marker)
        .map_err(|e| e.to_string())?;
    file.write_all(phase.as_bytes())
        .and_then(|_| file.sync_all())
        .map_err(|e| e.to_string())?;
    let parent = marker.parent().ok_or("crash marker parent missing")?;
    std::fs::File::open(parent)
        .and_then(|directory| directory.sync_all())
        .map_err(|e| e.to_string())?;
    loop {
        std::thread::park();
    }
}

static EXECUTION_ATTEMPTS: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
static EXECUTION_TARGET: std::sync::Mutex<Option<String>> = std::sync::Mutex::new(None);
pub(crate) fn mark_execution_started(block_hash: &str) {
    if EXECUTION_TARGET
        .lock()
        .unwrap()
        .as_ref()
        .is_none_or(|target| target == block_hash)
    {
        EXECUTION_ATTEMPTS.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    }
}
pub(crate) fn reset_execution_attempts(block_hash: &str) {
    *EXECUTION_TARGET.lock().unwrap() = Some(block_hash.to_owned());
    EXECUTION_ATTEMPTS.store(0, std::sync::atomic::Ordering::SeqCst);
}
pub(crate) fn execution_attempts() -> usize {
    EXECUTION_ATTEMPTS.load(std::sync::atomic::Ordering::SeqCst)
}
