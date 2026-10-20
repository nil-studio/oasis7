//! Actual process faults around a healthy durable-set handoff. P/R endpoint
//! providers are test simulations; the runtime owner and writer lease are real
//! local OS processes and locks.
use super::durable_set_tests::HealthySetCase;
use super::healthy_operation_tests::HealthyOwnerFaultMode;
use super::{ScenarioDeadline, run_protected_runtime_case_with_deadline};
use crate::node_lifecycle::local_io as io;
use oasis7_distfs::controlled_authority::formal::{
    FormalCompletionBranchV1, VerifiedAuthorityV1, VerifiedFormalCommitV1,
    verify_durable_set_activation, verify_formal_qualified, verify_normal_durable_set_transition,
};
use serde_json::Value;
use std::{
    fs::File,
    os::fd::AsRawFd,
    os::unix::fs::OpenOptionsExt,
    path::{Path, PathBuf},
    thread,
    time::{Duration, Instant},
};

#[test]
#[ignore = "actual healthy Node and protected endpoint processes; requires shipping runtime and endpoint binaries"]
fn healthy_durable_set_owner_retries_same_operation_after_real_successor_writer_lock_failure() {
    run_fixture("healthy_fault_writer_lock");
}

#[test]
#[ignore = "actual healthy Node and protected endpoint processes; requires shipping runtime and endpoint binaries"]
fn healthy_durable_set_owner_resumes_same_operation_after_old_owner_sigkill() {
    run_fixture("healthy_fault_old_owner_sigkill");
}

pub(super) fn exercise_writer_lock(case: HealthySetCase) -> Result<(), String> {
    super::healthy_operation_tests::exercise_with_fault(
        case,
        Some(HealthyOwnerFaultMode::SuccessorWriterLock),
    )
}

pub(super) fn exercise_old_owner_sigkill(case: HealthySetCase) -> Result<(), String> {
    super::healthy_operation_tests::exercise_with_fault(
        case,
        Some(HealthyOwnerFaultMode::OldOwnerSigkillAfterH),
    )
}

pub(super) fn hold_formal_writer_lock(writer_directory: &Path) -> Result<File, String> {
    let path = writer_directory.join("writer.lock");
    crate::controlled_live_config::require_private_owned(&path, false)?;
    let file = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
        .open(&path)
        .map_err(|error| format!("open actual formal writer lock: {error}"))?;
    if !file
        .metadata()
        .map_err(|error| error.to_string())?
        .is_file()
    {
        return Err("actual formal writer lock is not a regular file".into());
    }
    if unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) } != 0 {
        return Err(format!(
            "test could not acquire exact successor writer.lock: {}",
            std::io::Error::last_os_error()
        ));
    }
    Ok(file)
}

pub(super) fn owner_operation_directory(
    writer_directory: &Path,
    operation: &str,
) -> Result<PathBuf, String> {
    let operation_digest = io::digest(&operation.to_owned())?;
    Ok(writer_directory
        .join("runtime-owner")
        .join("operations")
        .join(operation_digest))
}

pub(super) fn assert_no_published_h_plus_one(
    writer_directory: &Path,
    h: u64,
) -> Result<(), String> {
    let h_plus_one = h.checked_add(1).ok_or("healthy H position overflow")?;
    if crate::execution_bridge::formal_writer::read_published_candidate_at(
        writer_directory,
        h_plus_one,
    )?
    .is_some()
    {
        return Err(format!(
            "old authority published H+1 before successor owner recovery (H={h})"
        ));
    }
    Ok(())
}

pub(super) fn verify_successor_h_plus_one(
    record: &Value,
    writer_directory: &Path,
    h: u64,
    old_authority: &VerifiedAuthorityV1,
    predecessor: &VerifiedFormalCommitV1,
    boundary: &VerifiedFormalCommitV1,
) -> Result<(), String> {
    let progress = &record["healthy_durable_set_progress"];
    let activation_step: oasis7::world_service::formal_read::FormalNormalDurableSetReadStepV1 =
        serde_json::from_value(progress["activation_step"].clone())
            .map_err(|error| format!("decode fixed-C0 activation evidence: {error}"))?;
    let verified_transition = verify_normal_durable_set_transition(
        &activation_step.transition,
        old_authority,
        predecessor,
        boundary,
    )
    .map_err(|error| format!("verify fixed-C0 healthy transition: {error}"))?;
    let activation = verify_durable_set_activation(
        &verified_transition,
        [&activation_step.old_seals[0], &activation_step.old_seals[1]],
        [
            &activation_step.new_installs[0],
            &activation_step.new_installs[1],
        ],
    )
    .map_err(|error| format!("verify successor activation: {error}"))?;
    if activation
        .predecessor()
        .context_digest()
        .map_err(|error| error.to_string())?
        != old_authority
            .context_digest()
            .map_err(|error| error.to_string())?
    {
        return Err("successor activation no longer binds original C0 authority".into());
    }
    let h_plus_one = h.checked_add(1).ok_or("healthy H position overflow")?;
    let (_, proof) = crate::execution_bridge::formal_writer::read_published_candidate_at(
        writer_directory,
        h_plus_one,
    )?
    .ok_or("first successor H+1 Qualified proof is missing")?;
    let commit = verify_formal_qualified(&proof, activation.successor(), Some(boundary), None)
        .map_err(|error| format!("verify successor authority H+1: {error}"))?;
    if commit.head().position != h_plus_one
        || proof.decision.branch != FormalCompletionBranchV1::Normal
        || progress["normal_readback"]["authority_id"]
            != activation
                .successor()
                .authority_id()
                .map_err(|error| error.to_string())?
    {
        return Err(
            "H+1 is not the first Normal Qualified commit under the successor authority".into(),
        );
    }
    Ok(())
}

fn run_fixture(mode: &'static str) {
    let started_at = Instant::now();
    let budget = Duration::from_secs(8 * 60);
    let deadline = ScenarioDeadline {
        started_at,
        expires_at: started_at + budget,
    };
    thread::Builder::new()
        .name(format!("{mode}-fixture"))
        .stack_size(32 * 1024 * 1024)
        .spawn(move || {
            run_protected_runtime_case_with_deadline(false, Some(mode), false, Some(deadline))
        })
        .expect("spawn healthy fault fixture thread")
        .join()
        .expect("healthy fault fixture thread panicked")
        .unwrap();
}
