//! Bounded, durable owner-attempt history and exact OS process identity.
use super::{FormalRuntimeOwnerSnapshotV1, OWNER_API, owner_root};
use crate::node_lifecycle::local_io as io;
use serde::{Deserialize, Serialize};
use std::{
    fs,
    path::{Path, PathBuf},
};

const JOURNAL_API: &str = "oasis7.formal-runtime-owner-attempts/v1";
pub(super) const MAX_RETAINED_ATTEMPTS: usize = 32;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct OwnerRecord {
    pub(super) api_version: String,
    pub(super) snapshot: FormalRuntimeOwnerSnapshotV1,
    #[serde(default = "first_attempt")]
    pub(super) attempt: u64,
    #[serde(default)]
    pub(super) process_start_identity: Option<String>,
    #[serde(default)]
    pub(super) attempt_history_digest: Option<String>,
}

fn first_attempt() -> u64 {
    1
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Journal {
    api_version: String,
    attempts: Vec<OwnerAttemptReceiptV1>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct OwnerAttemptReceiptV1 {
    attempt: u64,
    snapshot: FormalRuntimeOwnerSnapshotV1,
    process_start_identity: Option<String>,
    exit_observation: String,
}

fn journal_path(owner_state_path: &Path) -> Result<PathBuf, String> {
    Ok(owner_state_path
        .parent()
        .ok_or("formal owner state parent missing")?
        .join("attempt-history.json"))
}

fn empty_journal() -> Journal {
    Journal {
        api_version: JOURNAL_API.into(),
        attempts: Vec::new(),
    }
}

pub(super) fn empty_history_digest() -> Result<String, String> {
    digest(&empty_journal())
}

fn read_journal(owner_state_path: &Path) -> Result<Journal, String> {
    let path = journal_path(owner_state_path)?;
    io::reject_symlinks(&path)?;
    if !path.exists() {
        return Ok(empty_journal());
    }
    crate::controlled_live_config::require_private_owned(&path, false)?;
    let journal: Journal = io::read(&path)?;
    if journal.api_version != JOURNAL_API
        || journal.attempts.len() > MAX_RETAINED_ATTEMPTS
        || journal.attempts.iter().enumerate().any(|(index, attempt)| {
            attempt.attempt != index as u64 + 1
                || attempt.snapshot.api_version != OWNER_API
                || attempt.snapshot.instance.api_version != OWNER_API
                || attempt.snapshot.instance.operation_id.is_empty()
                || attempt.exit_observation.is_empty()
        })
    {
        return Err("invalid or over-capacity formal owner attempt history".into());
    }
    Ok(journal)
}

fn digest(journal: &Journal) -> Result<String, String> {
    io::digest(journal)
}

fn expected_history_digest(record: &OwnerRecord) -> Option<&str> {
    record.attempt_history_digest.as_deref()
}

pub(super) fn validate_history(
    owner_state_path: &Path,
    record: &OwnerRecord,
) -> Result<Journal, String> {
    let journal = read_journal(owner_state_path)?;
    let actual = digest(&journal)?;
    match expected_history_digest(record) {
        Some(expected) if expected == actual => {}
        None if journal.attempts.is_empty() => {}
        _ => return Err("formal owner attempt history digest differs from current receipt".into()),
    }
    if record.attempt as usize != journal.attempts.len() + 1 {
        return Err("formal owner current attempt number differs from retained history".into());
    }
    if journal.attempts.iter().any(|attempt| {
        let previous = &attempt.snapshot.instance;
        let current = &record.snapshot.instance;
        previous.operation_id != current.operation_id
            || previous.request_digest != current.request_digest
            || previous.config_digest != current.config_digest
            || previous.node_id != current.node_id
            || previous.world_id != current.world_id
            || previous.chain_id != current.chain_id
            || previous.authority_id != current.authority_id
            || previous.authority_epoch != current.authority_epoch
            || previous.profile_version != current.profile_version
    }) {
        return Err("formal owner attempt history changed immutable authority identity".into());
    }
    Ok(journal)
}

pub(super) fn read_owner_record(owner_state_path: &Path) -> Result<Option<OwnerRecord>, String> {
    io::reject_symlinks(owner_state_path)?;
    if !owner_state_path.exists() {
        return Ok(None);
    }
    crate::controlled_live_config::require_private_owned(owner_state_path, false)?;
    let record: OwnerRecord = io::read(owner_state_path)?;
    if record.api_version != OWNER_API
        || record.snapshot.api_version != OWNER_API
        || record.snapshot.instance.api_version != OWNER_API
        || record.attempt == 0
    {
        return Err("invalid formal runtime owner state schema".into());
    }
    Ok(Some(record))
}

pub(super) fn write_owner_record(
    owner_state_path: &Path,
    snapshot: FormalRuntimeOwnerSnapshotV1,
) -> Result<(), String> {
    let mut record = read_owner_record(owner_state_path)?.ok_or("owner attempt receipt missing")?;
    if record.snapshot.instance != snapshot.instance {
        return Err("formal owner state update changed exact attempt identity".into());
    }
    validate_history(owner_state_path, &record)?;
    record.snapshot = snapshot;
    io::write(owner_state_path, &record)
}

pub(super) fn write_new_owner_attempt(
    owner_state_path: &Path,
    snapshot: FormalRuntimeOwnerSnapshotV1,
    attempt: u64,
    process_start_identity: String,
    attempt_history_digest: String,
) -> Result<(), String> {
    let journal = read_journal(owner_state_path)?;
    let expected_attempt = journal.attempts.len() as u64 + 1;
    if attempt != expected_attempt
        || process_start_identity.is_empty()
        || digest(&journal)? != attempt_history_digest
    {
        return Err("formal owner new attempt does not extend exact retained history".into());
    }
    if journal.attempts.iter().any(|previous| {
        let old = &previous.snapshot.instance;
        let new = &snapshot.instance;
        old.operation_id != new.operation_id
            || old.request_digest != new.request_digest
            || old.config_digest != new.config_digest
            || old.node_id != new.node_id
            || old.world_id != new.world_id
            || old.chain_id != new.chain_id
            || old.authority_id != new.authority_id
            || old.authority_epoch != new.authority_epoch
            || old.profile_version != new.profile_version
    }) {
        return Err("formal owner retry changed immutable authority identity".into());
    }
    let record = OwnerRecord {
        api_version: OWNER_API.into(),
        snapshot,
        attempt,
        process_start_identity: Some(process_start_identity),
        attempt_history_digest: Some(attempt_history_digest),
    };
    io::write(owner_state_path, &record)
}

/// Confirm that the former exact PID/start-time is gone. `Ok(None)` means the
/// exact process is still alive (or a legacy PID cannot be disambiguated).
pub(super) fn process_exit_observation(record: &OwnerRecord) -> Result<Option<String>, String> {
    let pid = record.snapshot.instance.process_id;
    if pid == 0 {
        return Err("formal owner process identity has an invalid PID".into());
    }
    let current = process_start_identity(pid)?;
    match (record.process_start_identity.as_deref(), current.as_deref()) {
        (Some(expected), Some(actual)) if expected == actual => Ok(None),
        (Some(_), None) => Ok(Some("pid_absent".into())),
        (Some(_), Some(_)) => Ok(Some("pid_reused_with_different_start_identity".into())),
        (None, None) => Ok(Some("legacy_pid_absent".into())),
        // A legacy owner record has no birth identity. A live PID might be the
        // old worker or a reused PID, so retry remains fail-closed.
        (None, Some(_)) => Ok(None),
    }
}

pub(super) fn owner_lock_available(config_path: &Path) -> Result<bool, String> {
    let root = owner_root(config_path)?;
    let lock = io::private_file(&root.join("owner.lock"))?;
    Ok(lock.try_lock().is_ok())
}

pub(super) fn record_interrupted_attempt(
    owner_state_path: &Path,
    record: &OwnerRecord,
    exit_observation: String,
) -> Result<(u64, String), String> {
    if matches!(record.snapshot.state.as_str(), "stopped") {
        return Err("a deliberately stopped owner attempt cannot be restarted".into());
    }
    let mut journal = read_journal(owner_state_path)?;
    let baseline_digest = expected_history_digest(record)
        .map(str::to_owned)
        .unwrap_or(digest(&empty_journal())?);
    let actual_digest = digest(&journal)?;
    if actual_digest != baseline_digest {
        // Recover a crash after the append-only receipt was synced but before
        // the new current-owner state was atomically published.
        let mut prefix = journal.clone();
        let pending = prefix.attempts.pop();
        let is_exact_pending = pending.as_ref().is_some_and(|entry| {
            entry.attempt == record.attempt
                && entry.snapshot == record.snapshot
                && entry.process_start_identity == record.process_start_identity
                && !entry.exit_observation.is_empty()
        });
        if !is_exact_pending || digest(&prefix)? != baseline_digest {
            return Err("formal owner retry journal has an unrelated partial append".into());
        }
    } else {
        if journal.attempts.len() >= MAX_RETAINED_ATTEMPTS {
            return Err("formal owner retry history reached its bounded retention limit".into());
        }
        journal.attempts.push(OwnerAttemptReceiptV1 {
            attempt: record.attempt,
            snapshot: record.snapshot.clone(),
            process_start_identity: record.process_start_identity.clone(),
            exit_observation,
        });
        let path = journal_path(owner_state_path)?;
        io::write(&path, &journal)?;
    }
    let next_attempt = journal.attempts.len() as u64 + 1;
    if next_attempt != record.attempt.saturating_add(1) {
        return Err("formal owner retry attempt number is not consecutive".into());
    }
    Ok((next_attempt, digest(&journal)?))
}

/// The owner lock is acquired by the caller before this check. It proves the
/// old runtime writer has relinquished exclusivity; this second check proves
/// the persisted exact PID/start-time is no longer alive.
pub(super) fn retry_exit_observation(record: &OwnerRecord) -> Result<String, String> {
    process_exit_observation(record)?.ok_or_else(|| {
        "ORIGINAL_OUTCOME_UNKNOWN: prior exact owner PID/start-time is still live or cannot be disambiguated".into()
    })
}

pub(super) fn current_process_start_identity() -> Result<String, String> {
    process_start_identity(std::process::id())?
        .ok_or_else(|| "current formal owner process has no OS start identity".into())
}

pub(super) fn interrupted_snapshot(
    config_path: &Path,
    record: &OwnerRecord,
) -> Result<Option<FormalRuntimeOwnerSnapshotV1>, String> {
    if record.snapshot.state == "stopped" {
        return Ok(None);
    }
    let Some(observation) = process_exit_observation(record)? else {
        return Ok(None);
    };
    if !owner_lock_available(config_path)? {
        return Ok(None);
    }
    let mut snapshot = record.snapshot.clone();
    snapshot.state = "interrupted".into();
    snapshot.last_error = Some(format!(
        "exact owner process exit confirmed ({observation}); writer lock is free"
    ));
    Ok(Some(snapshot))
}

pub(super) fn remove_stale_socket_after_confirmed_exit(
    path: &Path,
    had_prior_attempt: bool,
) -> Result<(), String> {
    use std::os::unix::fs::FileTypeExt;
    io::reject_symlinks(path)?;
    let metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(error.to_string()),
    };
    if !had_prior_attempt || !metadata.file_type().is_socket() {
        return Err("formal runtime owner refuses to replace an unrelated socket path".into());
    }
    fs::remove_file(path)
        .map_err(|error| format!("remove confirmed stale owner socket: {error}"))?;
    io::sync_dir(path.parent().ok_or("formal owner socket parent missing")?)
}

#[cfg(target_os = "linux")]
pub(super) fn process_start_identity(pid: u32) -> Result<Option<String>, String> {
    let pid: libc::pid_t = pid.try_into().map_err(|_| "owner PID exceeds pid_t")?;
    let stat_path = PathBuf::from(format!("/proc/{pid}/stat"));
    let stat = match fs::read_to_string(&stat_path) {
        Ok(stat) => stat,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return match linux_pid_exists(pid)? {
                false => Ok(None),
                true => Err("Linux process exists but its start identity is unreadable".into()),
            };
        }
        Err(error) => return Err(format!("read owner process start identity: {error}")),
    };
    let close = stat
        .rfind(')')
        .ok_or("Linux owner process stat has no command terminator")?;
    let fields = stat[close + 1..].split_whitespace().collect::<Vec<_>>();
    let start_ticks = fields
        .get(19)
        .ok_or("Linux owner process stat omits start ticks")?;
    if !start_ticks.bytes().all(|byte| byte.is_ascii_digit()) {
        return Err("Linux owner process start ticks are invalid".into());
    }
    let boot_id = fs::read_to_string("/proc/sys/kernel/random/boot_id")
        .map_err(|error| format!("read Linux boot identity: {error}"))?
        .trim()
        .to_owned();
    if boot_id.is_empty() {
        return Err("Linux boot identity is empty".into());
    }
    Ok(Some(format!("linux:{boot_id}:{start_ticks}")))
}

#[cfg(target_os = "linux")]
fn linux_pid_exists(pid: libc::pid_t) -> Result<bool, String> {
    if unsafe { libc::kill(pid, 0) } == 0 {
        return Ok(true);
    }
    match std::io::Error::last_os_error().raw_os_error() {
        Some(libc::ESRCH) => Ok(false),
        Some(libc::EPERM) => Ok(true),
        _ => Err("query Linux owner process liveness failed".into()),
    }
}

#[cfg(target_os = "macos")]
pub(super) fn process_start_identity(pid: u32) -> Result<Option<String>, String> {
    let pid: libc::pid_t = pid.try_into().map_err(|_| "owner PID exceeds pid_t")?;
    let mut info: libc::proc_bsdinfo = unsafe { std::mem::zeroed() };
    let returned = unsafe {
        libc::proc_pidinfo(
            pid,
            libc::PROC_PIDTBSDINFO,
            0,
            &mut info as *mut _ as *mut libc::c_void,
            std::mem::size_of::<libc::proc_bsdinfo>() as libc::c_int,
        )
    };
    if returned == std::mem::size_of::<libc::proc_bsdinfo>() as libc::c_int {
        return Ok(Some(format!(
            "macos:{}:{}",
            info.pbi_start_tvsec, info.pbi_start_tvusec
        )));
    }
    if unsafe { libc::kill(pid, 0) } == -1 {
        match std::io::Error::last_os_error().raw_os_error() {
            Some(libc::ESRCH) => return Ok(None),
            Some(libc::EPERM) => {
                return Err("macOS owner process exists but start identity is inaccessible".into());
            }
            _ => return Err("query macOS owner process liveness failed".into()),
        }
    }
    Err("macOS owner process exists but start identity is unavailable".into())
}

#[cfg(not(any(target_os = "linux", target_os = "macos")))]
pub(super) fn process_start_identity(_: u32) -> Result<Option<String>, String> {
    Err("formal owner retry requires Linux or macOS process start identity".into())
}

fn receipt_matches_spawn(
    receipt_pid: u32,
    receipt_birth: Option<&str>,
    child_pid: u32,
    child_birth: Option<&str>,
) -> bool {
    receipt_pid == child_pid && receipt_birth.is_some() && receipt_birth == child_birth
}

pub(super) fn filter_spawn_receipt(
    snapshot: Option<FormalRuntimeOwnerSnapshotV1>,
    state_path: &Path,
    child_pid: u32,
    child_birth: Option<&str>,
) -> Result<Option<FormalRuntimeOwnerSnapshotV1>, String> {
    let Some(snapshot) = snapshot else {
        return Ok(None);
    };
    let record = read_owner_record(state_path)?;
    let birth = process_start_identity(child_pid)?;
    Ok(record
        .filter(|record| {
            record.snapshot.instance == snapshot.instance
                && birth.as_deref() == child_birth
                && receipt_matches_spawn(
                    record.snapshot.instance.process_id,
                    record.process_start_identity.as_deref(),
                    child_pid,
                    birth.as_deref(),
                )
        })
        .map(|_| snapshot))
}

#[cfg(test)]
#[path = "formal_live_owner_attempt_tests.rs"]
mod tests;
