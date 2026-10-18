//! A killed writer must release its original maintenance pins on live P/R.
//! Faults occur only in the test adapter; endpoint processes are never restarted.
use super::*;
use std::{
    fs::{File, OpenOptions},
    os::unix::process::ExitStatusExt,
    path::PathBuf,
    process::{Child, Command},
};

pub(super) struct Case {
    pub runtime: FormalLiveRuntime,
    pub runtime_config: PathBuf,
    pub writer_directory: PathBuf,
    pub root: PathBuf,
    pub primary_child: oracle_support::EndpointChild,
    pub replica_child: oracle_support::EndpointChild,
    pub owner_config: WorldServiceClientConfig,
    pub identity: WorldIdentity,
    pub original: SubmitIntentRequest<WorldServicePayloadV1>,
}
const CASE: &str = "OASIS7_ATTEMPT_RELEASE_CASE";
const NAME: &str = "formal_live_runtime::tests::protected_process_tests::attempt_release_tests::formal_attempt_release_crash_and_transport_failure_recover_live_remote_pins";
const CHILD: &str = "formal_live_runtime::tests::protected_process_tests::attempt_release_tests::attempt_release_runtime_child";
#[test]
#[ignore = "manual real P/R processes; writer SIGKILL and release transport faults"]
fn formal_attempt_release_crash_and_transport_failure_recover_live_remote_pins() {
    if std::env::var_os(CASE).is_none() {
        for phase in [
            "release_before_primary",
            "release_after_primary",
            "release_replica_reply_lost",
            "release_replica_temporarily_unavailable",
        ] {
            let status = Command::new(std::env::current_exe().unwrap())
                .args([
                    "--exact",
                    NAME,
                    "--ignored",
                    "--nocapture",
                    "--test-threads=1",
                ])
                .env(CASE, phase)
                .status()
                .unwrap();
            assert!(status.success(), "attempt release phase {phase} failed");
        }
        return;
    }
    thread::Builder::new()
        .name("formal-attempt-release".into())
        .stack_size(32 * 1024 * 1024)
        .spawn(|| {
            run_protected_runtime_fixture(
                false,
                None,
                false,
                None,
                false,
                None,
                FixtureCase::AttemptRelease,
            )
        })
        .unwrap()
        .join()
        .unwrap()
        .unwrap();
}
struct OwnedChild(Child);
impl Drop for OwnedChild {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}
#[test]
#[ignore = "internal attempt release writer child; parent supplies exact config"]
fn attempt_release_runtime_child() {
    thread::Builder::new()
        .name("attempt-release-child".into())
        .stack_size(32 * 1024 * 1024)
        .spawn(|| -> Result<(), String> {
            let config =
                std::env::var_os("OASIS7_FORMAL_TEST_CHILD_CONFIG").ok_or("child config")?;
            let ready = std::env::var_os("OASIS7_FORMAL_TEST_CHILD_READY").ok_or("child ready")?;
            let mut runtime = FormalLiveRuntime::open_with_native_history(Path::new(&config))?;
            if std::env::var_os("OASIS7_FORMAL_TEST_RELEASE_RECOVERY").is_some() {
                drain(&mut runtime)?;
                return Ok(());
            }
            runtime.start()?;
            oracle_support::write_private(Path::new(&ready), b"ready")?;
            loop {
                thread::sleep(Duration::from_millis(20));
            }
        })
        .unwrap()
        .join()
        .unwrap()
        .unwrap();
}
fn wait_file(child: &mut OwnedChild, path: &Path, deadline: Instant) -> Result<(), String> {
    loop {
        if path.is_file() {
            return Ok(());
        }
        if let Some(status) = child.0.try_wait().map_err(|e| e.to_string())? {
            return Err(format!(
                "attempt release writer exited before fault: {status}"
            ));
        }
        if Instant::now() >= deadline {
            return Err("attempt release phase deadline".into());
        }
        thread::sleep(Duration::from_millis(20));
    }
}
fn drain(runtime: &mut FormalLiveRuntime) -> Result<(), String> {
    let deadline = Instant::now() + Duration::from_secs(180);
    loop {
        if runtime.advance_storage_maintenance_for_test()? {
            return Ok(());
        }
        if Instant::now() >= deadline {
            return Err("attempt maintenance did not converge within 180s".into());
        }
        thread::sleep(Duration::from_millis(10));
    }
}
fn pin_busy(root: &Path) -> Result<Option<bool>, String> {
    let path = root.join("formal-objects-v1/.physical-guard-v1");
    let file = OpenOptions::new()
        .read(true)
        .write(true)
        .open(path)
        .map_err(|e| e.to_string())?;
    match file.try_lock() {
        Ok(()) => {
            File::unlock(&file).map_err(|e| e.to_string())?;
            Ok(Some(false))
        }
        Err(std::fs::TryLockError::WouldBlock) => match file.try_lock_shared() {
            Ok(()) => {
                File::unlock(&file).map_err(|e| e.to_string())?;
                Ok(Some(true)) // Another shared reader, not a generation writer.
            }
            Err(std::fs::TryLockError::WouldBlock) => Ok(None),
            Err(error) => Err(format!("physical shared pin probe failed: {error}")),
        },
        Err(error) => Err(format!(
            "actual physical maintenance pin probe failed: {error}"
        )),
    }
}
fn assert_pins(primary: &Path, replica: &Path, expected: (bool, bool)) -> Result<(), String> {
    // An endpoint may briefly take the same fence for its own maintenance.
    // Allow that operation to finish; a retained closure pin never unlocks.
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        let actual = (pin_busy(primary)?, pin_busy(replica)?);
        if actual == (Some(expected.0), Some(expected.1)) {
            return Ok(());
        }
        if Instant::now() >= deadline {
            return Err(format!(
                "remote maintenance pins {actual:?} != {expected:?}"
            ));
        }
        thread::sleep(Duration::from_millis(20));
    }
}
fn endpoints_alive(case: &Case) -> Result<(), String> {
    for pid in [case.primary_child.pid(), case.replica_child.pid()] {
        // SAFETY: signal zero only probes the original owned, unreaped endpoint child.
        if unsafe { libc::kill(pid as libc::pid_t, 0) } != 0 {
            return Err("protected endpoint exited during writer-only fault".into());
        }
    }
    Ok(())
}
pub(super) fn exercise(mut case: Case) -> Result<(), String> {
    let phase = std::env::var(CASE).map_err(|e| e.to_string())?;
    stop_owned_protected_runtime(
        &mut case.runtime,
        Duration::from_secs(120),
        "attempt_release_initial",
    )?;
    drain(&mut case.runtime)?;
    endpoints_alive(&case)?;
    let primary = case.root.join("primary-data");
    let replica = case.root.join("replica-data");
    assert_pins(&primary, &replica, (false, false))?;
    drop(case.runtime);
    let request_path = case.root.join("release-original.cbor");
    oracle_support::write_private(
        &request_path,
        &serde_cbor::to_vec(&case.original).map_err(|e| e.to_string())?,
    )?;
    let marker = case.root.join("release-phase");
    let ready = case.root.join("release-ready");
    let failure = case.root.join("release-transport-failed");
    let mut child = OwnedChild(
        Command::new(std::env::current_exe().map_err(|e| e.to_string())?)
            .args([
                "--exact",
                CHILD,
                "--ignored",
                "--nocapture",
                "--test-threads=1",
            ])
            .env("OASIS7_FORMAL_TEST_CHILD_CONFIG", &case.runtime_config)
            .env("OASIS7_FORMAL_TEST_CHILD_READY", &ready)
            .env("OASIS7_FORMAL_TEST_CRASH_REQUEST", &request_path)
            .env("OASIS7_FORMAL_TEST_CRASH_MARKER", &marker)
            .env("OASIS7_FORMAL_TEST_CRASH_PHASE", &phase)
            .env("OASIS7_FORMAL_TEST_RELEASE_FAILURE_MARKER", &failure)
            .spawn()
            .map_err(|e| e.to_string())?,
    );
    let deadline = Instant::now() + Duration::from_secs(240);
    wait_file(&mut child, &ready, deadline)?;
    let owner =
        RemoteWorldServiceClient::new(case.owner_config.clone()).map_err(|e| e.to_string())?;
    assert_volatile_admission(
        owner
            .submit(case.original.clone())
            .map_err(|e| e.to_string())?,
        &case.original.correlation,
    )?;
    let observed = if matches!(
        phase.as_str(),
        "release_replica_temporarily_unavailable" | "release_replica_reply_lost"
    ) {
        &failure
    } else {
        &marker
    };
    wait_file(&mut child, observed, deadline)?;
    let (position, digest): (u64, String) = serde_cbor::from_slice(
        &std::fs::read(marker.with_extension("release-target.cbor")).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    if observed == &failure {
        let (failed_digest, failed_phase): (String, String) =
            serde_cbor::from_slice(&std::fs::read(&failure).map_err(|e| e.to_string())?)
                .map_err(|e| e.to_string())?;
        if failed_digest != digest || failed_phase != phase {
            return Err("transport fault did not bind exact original candidate/phase".into());
        }
    }
    let expected = match phase.as_str() {
        "release_before_primary" => (true, true),
        "release_after_primary" | "release_replica_temporarily_unavailable" => (false, true),
        "release_replica_reply_lost" => (false, false),
        _ => return Err("unknown release phase".into()),
    };
    assert_pins(&primary, &replica, expected)?;
    child.0.kill().map_err(|e| e.to_string())?;
    let status = child.0.wait().map_err(|e| e.to_string())?;
    if status.signal() != Some(9) {
        return Err("writer-only fault was not SIGKILL".into());
    }
    drop(child);
    // Only this writer was killed. Endpoint children and their kernel locks survive.
    for pid in [case.primary_child.pid(), case.replica_child.pid()] {
        if unsafe { libc::kill(pid as libc::pid_t, 0) } != 0 {
            return Err("endpoint lost across writer kill".into());
        }
    }
    let publication_path = case
        .writer_directory
        .join("published")
        .join(format!("{position:020}.cbor"));
    let publication = std::fs::read(&publication_path).map_err(|e| e.to_string())?;
    let mut matches = Vec::new();
    for entry in
        std::fs::read_dir(case.writer_directory.join("attempts")).map_err(|e| e.to_string())?
    {
        let path = entry.map_err(|e| e.to_string())?.path();
        let bytes =
            std::fs::read(path.join("attempt-lifecycle-v1.cbor")).map_err(|e| e.to_string())?;
        let manifest: Value = serde_cbor::from_slice(&bytes).map_err(|e| e.to_string())?;
        if manifest["candidate_digest"] == digest {
            matches.push((path, manifest));
        }
    }
    if matches.len() != 1 || matches[0].1["phase"] != "ReleasePending" {
        return Err("killed writer did not durably retain exact release obligation".into());
    }
    let attempt = &matches[0].0;
    let unique = attempt.join("unique-evidence-must-remain");
    oracle_support::write_private(&unique, b"unique unresolved evidence")?;
    // Run recovery in a separately configured child: forbid observations and
    // transport fault removal never mutate the multithreaded parent's env.
    let reverified = case.root.join("release-recovery-reverified");
    let mut recovery = OwnedChild(
        Command::new(std::env::current_exe().map_err(|e| e.to_string())?)
            .args([
                "--exact",
                CHILD,
                "--ignored",
                "--nocapture",
                "--test-threads=1",
            ])
            .env("OASIS7_FORMAL_TEST_CHILD_CONFIG", &case.runtime_config)
            .env(
                "OASIS7_FORMAL_TEST_CHILD_READY",
                case.root.join("unused-recovery-ready"),
            )
            .env("OASIS7_FORMAL_TEST_RELEASE_RECOVERY", "1")
            .env("OASIS7_FORMAL_TEST_FORBID_REVERIFY_DIGEST", &digest)
            .env("OASIS7_FORMAL_TEST_FORBID_REVERIFY_MARKER", &reverified)
            .env_remove("OASIS7_FORMAL_TEST_CRASH_PHASE")
            .env_remove("OASIS7_FORMAL_TEST_CRASH_REQUEST")
            .env_remove("OASIS7_FORMAL_TEST_CRASH_MARKER")
            .env_remove("OASIS7_FORMAL_TEST_RELEASE_FAILURE_MARKER")
            .spawn()
            .map_err(|e| e.to_string())?,
    );
    let recovery_deadline = Instant::now() + Duration::from_secs(240);
    loop {
        if let Some(status) = recovery.0.try_wait().map_err(|e| e.to_string())? {
            if !status.success() {
                return Err(format!("release recovery child failed: {status}"));
            }
            break;
        }
        if Instant::now() >= recovery_deadline {
            return Err("release recovery child deadline".into());
        }
        thread::sleep(Duration::from_millis(20));
    }
    drop(recovery);
    if reverified.exists() {
        return Err("release recovery recreated an already verified closure job".into());
    }
    assert_pins(&primary, &replica, (false, false))?;
    if std::fs::read(&publication_path).map_err(|e| e.to_string())? != publication
        || std::fs::read(&unique).map_err(|e| e.to_string())? != b"unique unresolved evidence"
    {
        return Err("release recovery changed original publication or unique evidence".into());
    }
    let manifest: Value = serde_cbor::from_slice(
        &std::fs::read(attempt.join("attempt-lifecycle-v1.cbor")).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    if manifest["phase"] != "Reclaimed" {
        return Err("release obligation did not reach durable terminal state".into());
    }
    let mut runtime = FormalLiveRuntime::open_with_native_history(&case.runtime_config)?;
    runtime.start()?;
    let (cut, _) = wait_for_canonical_request_result(&runtime, &case.original)?;
    let commit = lookup_original_commit(&owner, &case.original)?;
    if commit.position != position {
        return Err("release recovery changed original committed position".into());
    }
    let task = game_task::current_control(&cut.world, "agent")
        .map_err(|e| e.to_string())?
        .ok_or("original effect missing")?;
    assert_volatile_admission(
        owner
            .submit(case.original.clone())
            .map_err(|e| e.to_string())?,
        &case.original.correlation,
    )?;
    let next = runtime
        .published
        .position()?
        .checked_add(1)
        .ok_or("release retry position overflow")?;
    wait_for_published_position(&runtime, next)?;
    if lookup_original_commit(&owner, &case.original)? != commit
        || game_task::current_control(&runtime.published.current()?.world, "agent")
            .map_err(|e| e.to_string())?
            != Some(task)
    {
        return Err("release recovery duplicated original effect".into());
    }
    stop_owned_protected_runtime(
        &mut runtime,
        Duration::from_secs(120),
        "attempt_release_final",
    )?;
    let _ = &case.identity;
    eprintln!(
        "ATTEMPT_RELEASE_RESULT=passed phase={phase} position={position} candidate={digest} endpoints_restarted=false actual_remote_pins_released=true recovery_reverify_attempts=0 fault_model={}",
        match phase.as_str() {
            "release_replica_reply_lost" => "remote_release_success_then_client_io",
            "release_replica_temporarily_unavailable" => "client_io_before_remote_release",
            _ => "writer_sigkill_at_durable_release_boundary",
        }
    );
    Ok(())
}
