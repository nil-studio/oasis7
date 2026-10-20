//! Private, exact-instance owner for formal live runtimes.
//!
//! Owner state is only local process-management evidence. A caller must still
//! restore both authenticated formal endpoints before it can claim an active
//! authority or qualified head.
use super::super::formal_live_config::FormalLiveConfigV1;
use super::FormalLiveRuntime;
use serde::{Deserialize, Serialize};
use std::{
    fs,
    net::Shutdown,
    os::unix::{
        fs::PermissionsExt,
        net::{UnixListener, UnixStream},
    },
    path::{Path, PathBuf},
    process::{Command, Stdio},
    time::{Duration, Instant},
};

#[path = "formal_live_owner_attempt.rs"]
mod attempt;
#[path = "formal_live_owner_fixed_trust.rs"]
mod fixed_trust;
use attempt::{read_owner_record, write_owner_record};
#[path = "formal_live_runtime_owner_wire.rs"]
mod control_wire;
use control_wire::{bounded_reason, read_message, write_message};

const OWNER_API: &str = "oasis7.formal-runtime-owner/v1";
const MAX_CONTROL_BYTES: usize = 16 * 1024;
const OWNER_SOCKET_UNAVAILABLE: &str =
    "owner state was nonterminal but its exact process control socket is unavailable";
#[cfg(test)]
const TEST_WORKER_CONFIG: &str = "OASIS7_FORMAL_OWNER_TEST_CONFIG";
#[cfg(test)]
const TEST_WORKER_OPERATION: &str = "OASIS7_FORMAL_OWNER_TEST_OPERATION";
#[cfg(test)]
const TEST_WORKER_REQUEST_DIGEST: &str = "OASIS7_FORMAL_OWNER_TEST_REQUEST_DIGEST";
#[cfg(test)]
const TEST_WORKER_CONFIG_DIGEST: &str = "OASIS7_FORMAL_OWNER_TEST_CONFIG_DIGEST";

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct FormalRuntimeInstanceV1 {
    pub api_version: String,
    pub operation_id: String,
    pub request_digest: String,
    pub incarnation: String,
    pub config_digest: String,
    pub node_id: String,
    pub world_id: String,
    pub chain_id: String,
    pub authority_id: String,
    pub authority_epoch: u64,
    pub profile_version: u32,
    /// Diagnostic only. Control always uses the private socket and incarnation.
    pub process_id: u32,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct FormalRuntimeOwnerSnapshotV1 {
    pub api_version: String,
    pub instance: FormalRuntimeInstanceV1,
    pub state: String,
    pub last_error: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct CurrentOwner {
    api_version: String,
    operation_id: String,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ControlRequest {
    api_version: String,
    action: String,
    operation_id: String,
    request_digest: String,
    config_digest: String,
    incarnation: String,
    timeout_ms: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    durable_set: Option<DurableSetControlRequestV1>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) enum DurableSetControlRequestV1 {
    PrepareArm {
        operation_id: String,
        replica: Box<oasis7_distfs::controlled_authority::formal::EndpointSignerBindingV1>,
    },
    Arm {
        operation_id: String,
        transition:
            Box<oasis7_distfs::controlled_authority::formal::SignedNormalDurableSetTransitionV1>,
    },
    Query {
        operation_id: String,
        boundary_position: u64,
    },
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ControlReply {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    prepared_transition:
        Option<oasis7_distfs::controlled_authority::formal::SignedNormalDurableSetTransitionV1>,
    api_version: String,
    snapshot: FormalRuntimeOwnerSnapshotV1,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    durable_set_status:
        Option<super::super::execution_bridge::formal_durable_set_arm::DurableSetArmStatusV1>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    control_error: Option<String>,
}

#[path = "formal_live_owner_diagnostics.rs"]
mod diagnostics;
use diagnostics::{WorkerDiagnostics, append_worker_diagnostic};

fn owner_root(config_path: &Path) -> Result<PathBuf, String> {
    let validated = FormalLiveConfigV1::load(config_path)?;
    Ok(validated.config.writer_directory.join("runtime-owner"))
}

fn operation_root(root: &Path, operation_id: &str) -> Result<PathBuf, String> {
    if operation_id.is_empty()
        || operation_id.len() > 128
        || !operation_id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b"-_:".contains(&byte))
    {
        return Err("invalid formal runtime owner operation identity".into());
    }
    let digest = super::super::node_lifecycle::local_io::digest(&operation_id)?;
    Ok(root.join("operations").join(digest))
}

fn prepare_operation_root(root: &Path, operation_id: &str) -> Result<PathBuf, String> {
    super::super::node_lifecycle::local_io::private_dir(root)?;
    let operations = root.join("operations");
    super::super::node_lifecycle::local_io::private_dir(&operations)?;
    let operation = operation_root(root, operation_id)?;
    super::super::node_lifecycle::local_io::private_dir(&operation)?;
    Ok(operation)
}

fn paths(config_path: &Path, operation_id: &str) -> Result<(PathBuf, PathBuf, PathBuf), String> {
    let root = owner_root(config_path)?;
    let operation = operation_root(&root, operation_id)?;
    Ok((
        root,
        operation.join("owner.json"),
        socket_path(config_path, operation_id)?,
    ))
}

fn socket_path(config_path: &Path, operation_id: &str) -> Result<PathBuf, String> {
    let uid = unsafe { libc::geteuid() };
    let identity = super::super::node_lifecycle::local_io::digest(&(
        config_path.to_string_lossy(),
        operation_id,
    ))?;
    // Use platform-fixed canonical temp roots. macOS exposes /tmp as a symlink
    // to /private/tmp, while this owner deliberately rejects symlink parents.
    // Do not honor TMPDIR here: the socket directory is part of the local
    // process-control boundary and must not be redirected to a shared path.
    #[cfg(target_os = "linux")]
    let temp_root = PathBuf::from("/tmp");
    #[cfg(target_os = "macos")]
    let temp_root = PathBuf::from("/private/tmp");
    #[cfg(not(any(target_os = "linux", target_os = "macos")))]
    {
        return Err("formal runtime owner sockets require Linux or macOS".into());
    }
    #[cfg(any(target_os = "linux", target_os = "macos"))]
    {
        let root = temp_root.join(format!("o7-runtime-owner-{uid}"));
        Ok(root.join(format!("{}.sock", &identity[..32])))
    }
}

fn config_digest(path: &Path) -> Result<String, String> {
    super::super::node_lifecycle::local_io::sha256(path)
}

fn validate_snapshot(
    snapshot: &FormalRuntimeOwnerSnapshotV1,
    operation_id: &str,
    request_digest: &str,
    expected_config_digest: &str,
    expected_incarnation: Option<&str>,
) -> Result<(), String> {
    if snapshot.api_version != OWNER_API
        || snapshot.instance.api_version != OWNER_API
        || snapshot.instance.operation_id != operation_id
        || snapshot.instance.request_digest != request_digest
        || snapshot.instance.config_digest != expected_config_digest
        || expected_incarnation.is_some_and(|value| snapshot.instance.incarnation != value)
        || !matches!(
            snapshot.state.as_str(),
            "starting" | "ready" | "stopping" | "stopped" | "failed" | "unknown" | "interrupted"
        )
    {
        return Err("formal runtime owner response differs from exact operation/instance".into());
    }
    Ok(())
}

fn send_control(
    config_path: &Path,
    instance: &FormalRuntimeInstanceV1,
    action: &str,
    timeout: Duration,
) -> Result<FormalRuntimeOwnerSnapshotV1, String> {
    if !matches!(action, "query" | "stop") {
        return Err("unsupported formal runtime owner control action".into());
    }
    let (_, state_path, socket_path) = paths(config_path, &instance.operation_id)?;
    let stream = UnixStream::connect(&socket_path)
        .map_err(|error| format!("formal runtime owner control unavailable: {error}"))?;
    send_control_on_stream(&state_path, instance, action, timeout, stream)
}

fn send_control_on_stream(
    state_path: &Path,
    instance: &FormalRuntimeInstanceV1,
    action: &str,
    timeout: Duration,
    mut stream: UnixStream,
) -> Result<FormalRuntimeOwnerSnapshotV1, String> {
    if !matches!(action, "query" | "stop") {
        return Err("unsupported formal runtime owner control action".into());
    }
    stream
        .set_read_timeout(Some(timeout.min(Duration::from_secs(60))))
        .map_err(|error| error.to_string())?;
    stream
        .set_write_timeout(Some(timeout.min(Duration::from_secs(60))))
        .map_err(|error| error.to_string())?;
    let request = ControlRequest {
        api_version: OWNER_API.into(),
        action: action.into(),
        operation_id: instance.operation_id.clone(),
        request_digest: instance.request_digest.clone(),
        config_digest: instance.config_digest.clone(),
        incarnation: instance.incarnation.clone(),
        timeout_ms: timeout.as_millis().min(60_000) as u64,
        durable_set: None,
    };
    write_message(&mut stream, &request)?;
    let reply: ControlReply = read_message(&mut stream)?;
    if reply.api_version != OWNER_API {
        return Err("formal runtime owner response schema mismatch".into());
    }
    validate_snapshot(
        &reply.snapshot,
        &instance.operation_id,
        &instance.request_digest,
        &instance.config_digest,
        Some(&instance.incarnation),
    )?;
    if let Some(record) = read_owner_record(state_path)?
        && record.snapshot != reply.snapshot
    {
        return Err("formal runtime owner durable/live status mismatch".into());
    }
    Ok(reply.snapshot)
}

/// The private socket, persisted owner and exact incarnation all agree before
/// an old-writer-signed boundary can be armed. Local owner status grants no
/// formal qualification, seal or activation permission.
pub(crate) fn control_durable_set_transition(
    config_path: &Path,
    instance: &FormalRuntimeInstanceV1,
    request: DurableSetControlRequestV1,
    timeout: Duration,
) -> Result<
    Option<super::super::execution_bridge::formal_durable_set_arm::DurableSetArmStatusV1>,
    String,
> {
    Ok(control_durable_set_request(config_path, instance, request, timeout)?.durable_set_status)
}

pub(crate) fn control_prepare_durable_set_transition(
    config_path: &Path,
    instance: &FormalRuntimeInstanceV1,
    operation_id: &str,
    replica: oasis7_distfs::controlled_authority::formal::EndpointSignerBindingV1,
    timeout: Duration,
) -> Result<oasis7_distfs::controlled_authority::formal::SignedNormalDurableSetTransitionV1, String>
{
    let reply = control_durable_set_request(
        config_path,
        instance,
        DurableSetControlRequestV1::PrepareArm {
            operation_id: operation_id.to_owned(),
            replica: Box::new(replica.clone()),
        },
        timeout,
    )?;
    let transition = reply
        .prepared_transition
        .ok_or("original durable-set preparation outcome unknown")?;
    let status = reply
        .durable_set_status
        .ok_or("prepared durable-set status absent")?;
    let mut expected_replica = replica;
    expected_replica.instance.authority_epoch = transition.body.successor.epoch;
    if transition.body.operation_id != operation_id
        || transition.body.successor.replica != expected_replica
        || status.operation_id != operation_id
        || status.boundary_position != transition.body.boundary_position
        || status.transition_digest != transition.body.digest().map_err(|e| e.to_string())?
        || status.phase
            == super::super::execution_bridge::formal_durable_set_arm::DurableSetArmPhaseV1::Unknown
    {
        return Err("prepared durable-set reply differs from original request".into());
    }
    Ok(transition)
}

fn control_durable_set_request(
    config_path: &Path,
    instance: &FormalRuntimeInstanceV1,
    request: DurableSetControlRequestV1,
    timeout: Duration,
) -> Result<ControlReply, String> {
    if timeout.is_zero() || timeout > Duration::from_secs(60) {
        return Err("durable-set owner control timeout in (0,60s]".into());
    }
    let (_, state_path, socket_path) = paths(config_path, &instance.operation_id)?;
    let current = read_owner_record(&state_path)?.ok_or("durable-set owner receipt missing")?;
    if current.snapshot.instance != *instance || current.snapshot.state != "ready" {
        return Err("durable-set control requires exact ready owner".into());
    }
    let mut stream = UnixStream::connect(socket_path).map_err(|error| error.to_string())?;
    stream
        .set_read_timeout(Some(timeout))
        .map_err(|error| error.to_string())?;
    stream
        .set_write_timeout(Some(timeout))
        .map_err(|error| error.to_string())?;
    write_message(
        &mut stream,
        &ControlRequest {
            api_version: OWNER_API.into(),
            action: "durable_set".into(),
            operation_id: instance.operation_id.clone(),
            request_digest: instance.request_digest.clone(),
            config_digest: instance.config_digest.clone(),
            incarnation: instance.incarnation.clone(),
            timeout_ms: timeout.as_millis() as u64,
            durable_set: Some(request),
        },
    )?;
    let reply: ControlReply = read_message(&mut stream)?;
    if reply.api_version != OWNER_API
        || reply.snapshot.instance != *instance
        || read_owner_record(&state_path)?.is_none_or(|record| record.snapshot != reply.snapshot)
    {
        return Err("durable-set owner reply differs from exact persisted incarnation".into());
    }
    if let Some(error) = reply.control_error {
        return Err(error);
    }
    Ok(reply)
}

pub(crate) fn query_operation(
    config_path: &Path,
    operation_id: &str,
    request_digest: &str,
    expected_config_digest: &str,
    expected_incarnation: Option<&str>,
) -> Result<Option<FormalRuntimeOwnerSnapshotV1>, String> {
    let (_, state_path, socket_path) = paths(config_path, operation_id)?;
    let Some(record) = read_owner_record(&state_path)? else {
        return Ok(None);
    };
    validate_snapshot(
        &record.snapshot,
        operation_id,
        request_digest,
        expected_config_digest,
        expected_incarnation,
    )?;
    if record.snapshot.state == "stopped" {
        return Ok(Some(record.snapshot));
    }
    if let Some(interrupted) = attempt::interrupted_snapshot(config_path, &record)? {
        return Ok(Some(interrupted));
    }
    if record.snapshot.state == "failed" {
        return Ok(Some(record.snapshot));
    }
    match UnixStream::connect(&socket_path) {
        Ok(stream) => send_control_on_stream(
            &state_path,
            &record.snapshot.instance,
            "query",
            Duration::from_secs(2),
            stream,
        )
        .map(Some),
        Err(_) => Ok(Some(FormalRuntimeOwnerSnapshotV1 {
            state: "unknown".into(),
            last_error: Some(OWNER_SOCKET_UNAVAILABLE.into()),
            ..record.snapshot
        })),
    }
}

pub(crate) fn query_operation_with_fixed_trust(
    config_path: &Path,
    operation_id: &str,
    request_digest: &str,
    expected_config_digest: &str,
    expected_incarnation: Option<&str>,
    expected_trust: &str,
) -> Result<Option<FormalRuntimeOwnerSnapshotV1>, String> {
    let snapshot = query_operation(
        config_path,
        operation_id,
        request_digest,
        expected_config_digest,
        expected_incarnation,
    )?;
    if snapshot.is_some() {
        let root = owner_root(config_path)?;
        fixed_trust::check(
            &operation_root(&root, operation_id)?,
            request_digest,
            expected_config_digest,
            expected_trust,
        )?;
    }
    Ok(snapshot)
}

pub(crate) fn current_operation(
    config_path: &Path,
) -> Result<FormalRuntimeOwnerSnapshotV1, String> {
    let root = owner_root(config_path)?;
    let current_path = root.join("current.json");
    super::super::node_lifecycle::local_io::reject_symlinks(&current_path)?;
    let current: CurrentOwner = super::super::node_lifecycle::local_io::read(&current_path)?;
    if current.api_version != OWNER_API {
        return Err("invalid formal runtime current-owner pointer".into());
    }
    let (_, state_path, _) = paths(config_path, &current.operation_id)?;
    let record = read_owner_record(&state_path)?.ok_or("formal runtime owner state absent")?;
    if matches!(record.snapshot.state.as_str(), "stopped" | "failed") {
        return Ok(record.snapshot);
    }
    send_control(
        config_path,
        &record.snapshot.instance,
        "query",
        Duration::from_secs(2),
    )
}

pub(crate) fn stop_operation(
    config_path: &Path,
    expected: &FormalRuntimeInstanceV1,
    timeout: Duration,
) -> Result<FormalRuntimeOwnerSnapshotV1, String> {
    send_control(config_path, expected, "stop", timeout)
}

pub(crate) fn launch_operation(
    config_path: &Path,
    operation_id: &str,
    request_digest: &str,
    expected_config_digest: &str,
    timeout: Duration,
) -> Result<FormalRuntimeOwnerSnapshotV1, String> {
    launch_operation_with_fixed_trust(
        config_path,
        operation_id,
        request_digest,
        expected_config_digest,
        None,
        timeout,
    )
}

pub(crate) fn launch_operation_with_fixed_trust(
    config_path: &Path,
    operation_id: &str,
    request_digest: &str,
    expected_config_digest: &str,
    expected_read_trust_digest: Option<&str>,
    timeout: Duration,
) -> Result<FormalRuntimeOwnerSnapshotV1, String> {
    let (root, state_path, socket_path) = paths(config_path, operation_id)?;
    let prepared_root = prepare_operation_root(&root, operation_id)?;
    let launch_lock =
        super::super::node_lifecycle::local_io::private_file(&prepared_root.join("launch.lock"))?;
    launch_lock.try_lock().map_err(|_| {
        "ORIGINAL_OUTCOME_UNKNOWN: exact owner startup is already in progress; retry query-only"
    })?;
    let prior = read_owner_record(&state_path)?;
    if let Some(record) = &prior {
        validate_snapshot(
            &record.snapshot,
            operation_id,
            request_digest,
            expected_config_digest,
            None,
        )?;
        if record.snapshot.state == "stopped" {
            return Ok(record.snapshot.clone());
        }
        if attempt::process_exit_observation(record)?.is_none() {
            return query_operation(
                config_path,
                operation_id,
                request_digest,
                expected_config_digest,
                None,
            )?
            .ok_or_else(|| "formal runtime owner attempt disappeared while being queried".into());
        }
        if !attempt::owner_lock_available(config_path)? {
            return Err(
                "ORIGINAL_OUTCOME_UNKNOWN: prior owner process exited but the writer lock remains held".into(),
            );
        }
    }
    let fixed_trust_path = operation_root(&root, operation_id)?.join(fixed_trust::FILE);
    if let Some(expected) = expected_read_trust_digest {
        if fixed_trust_path.exists() {
            fixed_trust::check(
                &operation_root(&root, operation_id)?,
                request_digest,
                expected_config_digest,
                expected,
            )?;
        } else if prior.is_some() {
            return Err(
                "ORIGINAL_OUTCOME_UNKNOWN: original fixed trust admission is missing".into(),
            );
        }
    } else if fixed_trust_path.exists() {
        return Err("original fixed trust owner admission cannot be downgraded".into());
    }
    super::super::node_lifecycle::local_io::private_dir(&root)?;
    let temp_root = socket_path
        .parent()
        .ok_or("formal runtime owner socket directory missing")?;
    super::super::node_lifecycle::local_io::private_dir(temp_root)?;
    super::super::node_lifecycle::local_io::reject_symlinks(&socket_path)?;
    if socket_path.exists() {
        if prior.is_none() {
            return Err("ORIGINAL_OUTCOME_UNKNOWN: formal runtime owner socket path already exists; it will not be replaced".into());
        }
        attempt::remove_stale_socket_after_confirmed_exit(&socket_path, true)?;
    }
    let executable = std::env::current_exe().map_err(|error| error.to_string())?;
    let mut command = Command::new(&executable);
    #[cfg(test)]
    {
        // Rust's test harness binary is not the production CLI. Launch one
        // exact ignored test as a child so process-owner acceptance tests still
        // exercise the real owner loop instead of substituting an in-process
        // fake. The production binary branch below remains byte-for-byte
        // separate from this test-only dispatch.
        let test_name = owner_worker_test_name(&executable)?;
        command
            .args(["--ignored", "--exact", &test_name, "--nocapture"])
            .env(TEST_WORKER_CONFIG, config_path)
            .env(TEST_WORKER_OPERATION, operation_id)
            .env(TEST_WORKER_REQUEST_DIGEST, request_digest)
            .env(TEST_WORKER_CONFIG_DIGEST, expected_config_digest);
    }
    #[cfg(not(test))]
    command.args([
        "op",
        "formal-owner-worker",
        "--config",
        config_path
            .to_str()
            .ok_or("non-UTF8 formal runtime config path")?,
        "--operation-id",
        operation_id,
        "--request-digest",
        request_digest,
        "--config-digest",
        expected_config_digest,
    ]);
    if let Some(digest) = expected_read_trust_digest {
        #[cfg(test)]
        command.env("OASIS7_FORMAL_OWNER_TEST_READ_TRUST_DIGEST", digest);
        #[cfg(not(test))]
        command.args(["--expected-formal-read-trust-digest", digest]);
    } else {
        #[cfg(test)]
        command.env_remove("OASIS7_FORMAL_OWNER_TEST_READ_TRUST_DIGEST");
    }
    // The owner outlives this CLI. Inheriting CLI-owned output pipes would
    // leave it with broken stdout/stderr as soon as the launching CLI exits.
    let diagnostics = WorkerDiagnostics::prepare(&prepared_root)?;
    let (stdout, stderr) = diagnostics.stdio()?;
    let mut child = command
        .stdin(Stdio::null())
        .stdout(stdout)
        .stderr(stderr)
        .spawn()
        .map_err(|error| format!("formal runtime owner spawn outcome unknown: {error}"))?;
    // Retain the exact Child handle to detect worker exit during startup. It is
    // used only for wait status, never for process control; lifecycle control
    // remains through the private authenticated socket.

    let child_birth = attempt::process_start_identity(child.id())?;
    let deadline = Instant::now() + timeout.min(Duration::from_secs(60));
    loop {
        let query = query_operation(
            config_path,
            operation_id,
            request_digest,
            expected_config_digest,
            None,
        );
        // The previous attempt remains durable until the new child publishes
        // its own receipt. It cannot report this child's readiness or failure.
        let query = query.and_then(|snapshot| {
            attempt::filter_spawn_receipt(snapshot, &state_path, child.id(), child_birth.as_deref())
        });
        let worker_exit = child
            .try_wait()
            .ok()
            .flatten()
            .map(|status| status.to_string());
        match query {
            Ok(Some(snapshot)) if snapshot.state == "ready" && worker_exit.is_none() => {
                if let Some(digest) = expected_read_trust_digest {
                    fixed_trust::check(
                        &operation_root(&root, operation_id)?,
                        request_digest,
                        expected_config_digest,
                        digest,
                    )?;
                }
                return Ok(snapshot);
            }
            Ok(Some(mut snapshot)) if snapshot.state == "ready" => {
                snapshot.state = "unknown".into();
                snapshot.last_error = Some(append_worker_diagnostic(
                    &format!(
                        "owner worker exited after writing ready state (exit status {})",
                        worker_exit.as_deref().unwrap_or("unavailable")
                    ),
                    &diagnostics,
                ));
                return Ok(snapshot);
            }
            Ok(Some(mut snapshot)) if snapshot.state == "unknown" => {
                // run_owner writes the durable `starting` receipt just before
                // binding its socket. During that small window query_operation
                // reports an unavailable socket; this is not a terminal
                // worker failure. Keep polling only while the exact persisted
                // instance is still starting and its exact Child is alive.
                let startup_socket_race = snapshot.last_error.as_deref()
                    == Some(OWNER_SOCKET_UNAVAILABLE)
                    && read_owner_record(&state_path).is_ok_and(|record| {
                        record.is_some_and(|record| {
                            record.snapshot.state == "starting"
                                && record.snapshot.instance == snapshot.instance
                        })
                    });
                if startup_socket_race && worker_exit.is_none() {
                    // Continue within the caller's original bounded deadline.
                } else if startup_socket_race {
                    snapshot.last_error = Some(append_worker_diagnostic(
                        &format!(
                            "owner worker exited while its durable state remained starting and its control socket was unavailable (exit status {})",
                            worker_exit.as_deref().unwrap_or("unavailable")
                        ),
                        &diagnostics,
                    ));
                    return Ok(snapshot);
                } else {
                    snapshot.last_error = Some(append_worker_diagnostic(
                        snapshot
                            .last_error
                            .as_deref()
                            .unwrap_or("owner state is unknown"),
                        &diagnostics,
                    ));
                    return Ok(snapshot);
                }
            }
            Ok(Some(mut snapshot)) if snapshot.state == "failed" => {
                if let Some(error) = snapshot.last_error.as_deref() {
                    snapshot.last_error = Some(append_worker_diagnostic(error, &diagnostics));
                }
                return Ok(snapshot);
            }
            Err(error) if error.contains("owner state schema") => return Err(error),
            Ok(None) if worker_exit.is_some() => {
                return Err(append_worker_diagnostic(
                    &format!(
                        "ORIGINAL_OUTCOME_UNKNOWN: owner worker exited before a durable owner receipt appeared (exit status {}); resume query-only",
                        worker_exit.as_deref().unwrap_or("unavailable")
                    ),
                    &diagnostics,
                ));
            }
            Err(error) if worker_exit.is_some() => {
                return Err(append_worker_diagnostic(
                    &format!(
                        "ORIGINAL_OUTCOME_UNKNOWN: owner worker exited while owner state could not be queried (exit status {}): {error}; resume query-only",
                        worker_exit.as_deref().unwrap_or("unavailable")
                    ),
                    &diagnostics,
                ));
            }
            _ => {}
        }
        if Instant::now() >= deadline {
            let message = "ORIGINAL_OUTCOME_UNKNOWN: formal runtime owner did not reach a queryable ready state before timeout; resume query-only";
            return Err(append_worker_diagnostic(message, &diagnostics));
        }
        std::thread::sleep(Duration::from_millis(40));
    }
}

#[cfg(test)]
fn owner_worker_test_name(executable: &Path) -> Result<String, String> {
    let output = Command::new(executable)
        .arg("--list")
        .output()
        .map_err(|error| format!("list formal owner test worker: {error}"))?;
    if !output.status.success() {
        return Err("formal owner test worker discovery failed".into());
    }
    let listing = String::from_utf8(output.stdout)
        .map_err(|error| format!("formal owner test worker listing is not UTF-8: {error}"))?;
    let mut matches = listing.lines().filter_map(|line| {
        line.strip_suffix(": test")
            .filter(|name| name.ends_with("::protected_handoff_owner_worker"))
    });
    let name = matches
        .next()
        .ok_or("formal owner test worker is not registered")?;
    if matches.next().is_some() {
        return Err("multiple formal owner test workers are registered".into());
    }
    Ok(name.to_owned())
}

pub(crate) fn run_standalone(config_path: &Path) -> Result<(), String> {
    let config_digest = config_digest(config_path)?;
    let operation_id = format!(
        "runtime-{}",
        super::super::node_lifecycle::local_io::random_id()?
    );
    let request_digest = super::super::node_lifecycle::local_io::digest(&(
        "oasis7.formal-runtime.standalone.v1",
        &operation_id,
        &config_digest,
    ))?;
    run_owner(config_path, &operation_id, &request_digest, &config_digest)
}

#[cfg(test)]
pub(crate) fn run_operation_worker(
    config_path: &Path,
    operation_id: &str,
    request_digest: &str,
    expected_config_digest: &str,
) -> Result<(), String> {
    #[cfg(test)]
    let expected = std::env::var("OASIS7_FORMAL_OWNER_TEST_READ_TRUST_DIGEST").ok();
    #[cfg(not(test))]
    let expected: Option<String> = None;
    run_owner_with_fixed_trust(
        config_path,
        operation_id,
        request_digest,
        expected_config_digest,
        expected.as_deref(),
    )
}

fn run_owner(
    config_path: &Path,
    operation_id: &str,
    request_digest: &str,
    expected_config_digest: &str,
) -> Result<(), String> {
    run_owner_with_fixed_trust(
        config_path,
        operation_id,
        request_digest,
        expected_config_digest,
        None,
    )
}

pub(crate) fn run_operation_worker_with_fixed_trust(
    config_path: &Path,
    operation_id: &str,
    request_digest: &str,
    expected_config_digest: &str,
    expected_read_trust_digest: Option<&str>,
) -> Result<(), String> {
    run_owner_with_fixed_trust(
        config_path,
        operation_id,
        request_digest,
        expected_config_digest,
        expected_read_trust_digest,
    )
}

fn run_owner_with_fixed_trust(
    config_path: &Path,
    operation_id: &str,
    request_digest: &str,
    expected_config_digest: &str,
    expected_read_trust_digest: Option<&str>,
) -> Result<(), String> {
    let (root, _, socket_path) = paths(config_path, operation_id)?;
    let operation_root = prepare_operation_root(&root, operation_id)?;
    let state_path = operation_root.join("owner.json");
    let owner_lock =
        super::super::node_lifecycle::local_io::private_file(&root.join("owner.lock"))?;
    owner_lock
        .try_lock()
        .map_err(|_| "formal runtime writer already has a process owner")?;
    let prior = read_owner_record(&state_path)?;
    if let Some(record) = &prior {
        validate_snapshot(
            &record.snapshot,
            operation_id,
            request_digest,
            expected_config_digest,
            None,
        )?;
        if record.snapshot.state == "stopped" {
            return Err("a deliberately stopped formal owner operation cannot be restarted".into());
        }
    }
    let config_digest_value = config_digest(config_path)?;
    if config_digest_value != expected_config_digest {
        return Err("formal runtime config digest changed before owner start".into());
    }
    let validated = FormalLiveConfigV1::load(config_path)?;
    if let Some(expected) = expected_read_trust_digest {
        if super::super::node_lifecycle::local_io::digest(&validated.trust)? != expected {
            return Err("formal external read trust changed before owner start".into());
        }
        let fixed_trust_path = operation_root.join(fixed_trust::FILE);
        if fixed_trust_path.exists() {
            fixed_trust::check(
                &operation_root,
                request_digest,
                expected_config_digest,
                expected,
            )?;
        } else if prior.is_some() {
            return Err("formal owner retry is missing its original fixed read trust".into());
        } else {
            fixed_trust::publish(
                &operation_root,
                request_digest,
                expected_config_digest,
                expected,
            )?;
        }
    } else if operation_root
        .join(fixed_trust::FILE)
        .try_exists()
        .map_err(|e| e.to_string())?
    {
        return Err("original fixed trust owner admission cannot be downgraded".into());
    }
    if validated.anchor.authority().definition().profile_version != 2 {
        return Err("UNSUPPORTED_ROLE_OR_PROFILE: formal runtime owner requires profile 2".into());
    }
    let initial_authority =
        super::super::formal_live_history::verified_startup_authority(&validated)?;
    let node_id = validated.config.node_id.clone();
    // Restore and verify lineage before binding the immutable attempt identity.
    // An open runtime does not serve until the receipt and socket are ready.
    let restored = FormalLiveRuntime::open_from_validated_with_native_history(validated);
    let active = restored
        .as_ref()
        .map_or(&initial_authority, |runtime| runtime.active_authority());
    let instance = FormalRuntimeInstanceV1 {
        api_version: OWNER_API.into(),
        operation_id: operation_id.into(),
        request_digest: request_digest.into(),
        incarnation: super::super::node_lifecycle::local_io::random_id()?,
        config_digest: config_digest_value,
        node_id,
        world_id: active.definition().world.world_id.clone(),
        chain_id: active.definition().world.chain_id.clone(),
        authority_id: active.authority_id().map_err(|error| error.to_string())?,
        authority_epoch: active.definition().epoch,
        profile_version: active.definition().profile_version,
        process_id: std::process::id(),
    };
    let mut snapshot = FormalRuntimeOwnerSnapshotV1 {
        api_version: OWNER_API.into(),
        instance: instance.clone(),
        state: "starting".into(),
        last_error: None,
    };
    let process_start_identity = attempt::current_process_start_identity()?;
    let (attempt_number, history_digest) = if let Some(record) = &prior {
        let previous = &record.snapshot.instance;
        if previous.node_id != instance.node_id
            || previous.world_id != instance.world_id
            || previous.chain_id != instance.chain_id
            || previous.authority_id != instance.authority_id
            || previous.authority_epoch != instance.authority_epoch
            || previous.profile_version != instance.profile_version
        {
            return Err(
                "formal owner retry changed fixed config, manifest, C0, or authority identity"
                    .into(),
            );
        }
        let exit = attempt::retry_exit_observation(record)?;
        attempt::remove_stale_socket_after_confirmed_exit(&socket_path, true)?;
        attempt::record_interrupted_attempt(&state_path, record, exit)?
    } else {
        attempt::remove_stale_socket_after_confirmed_exit(&socket_path, false)?;
        (1, attempt::empty_history_digest()?)
    };
    attempt::write_new_owner_attempt(
        &state_path,
        snapshot.clone(),
        attempt_number,
        process_start_identity,
        history_digest,
    )?;
    let current_path = root.join("current.json");
    super::super::node_lifecycle::local_io::write(
        &current_path,
        &CurrentOwner {
            api_version: OWNER_API.into(),
            operation_id: operation_id.into(),
        },
    )?;

    let socket_root = socket_path
        .parent()
        .ok_or("formal runtime socket directory missing")?;
    super::super::node_lifecycle::local_io::private_dir(socket_root)?;
    super::super::node_lifecycle::local_io::reject_symlinks(&socket_path)?;
    if socket_path.exists() {
        return Err("formal runtime owner refuses to replace an existing control socket".into());
    }
    let listener = UnixListener::bind(&socket_path).map_err(|error| error.to_string())?;
    fs::set_permissions(&socket_path, fs::Permissions::from_mode(0o600))
        .map_err(|error| error.to_string())?;
    listener
        .set_nonblocking(true)
        .map_err(|error| error.to_string())?;

    let mut runtime = match restored {
        Ok(runtime) => runtime,
        Err(error) => {
            snapshot.state = "failed".into();
            snapshot.last_error = Some(bounded_reason(&error));
            write_owner_record(&state_path, snapshot)?;
            return Err(error);
        }
    };
    if let Err(error) = runtime.start() {
        snapshot.state = "failed".into();
        snapshot.last_error = Some(bounded_reason(&error));
        write_owner_record(&state_path, snapshot)?;
        return Err(error);
    }
    snapshot.state = "ready".into();
    write_owner_record(&state_path, snapshot.clone())?;

    let shutdown = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let signal_shutdown = std::sync::Arc::clone(&shutdown);
    ctrlc::set_handler(move || {
        signal_shutdown.store(true, std::sync::atomic::Ordering::Release);
    })
    .map_err(|error| error.to_string())?;
    loop {
        if shutdown.load(std::sync::atomic::Ordering::Acquire) {
            snapshot.state = "stopping".into();
            write_owner_record(&state_path, snapshot.clone())?;
            match runtime.stop(Duration::from_secs(30)) {
                Ok(()) => {
                    snapshot.state = "stopped".into();
                    write_owner_record(&state_path, snapshot)?;
                    return Ok(());
                }
                Err(error) => {
                    snapshot.state = "unknown".into();
                    snapshot.last_error = Some(bounded_reason(&error));
                    write_owner_record(&state_path, snapshot)?;
                    return Err(error);
                }
            }
        }
        match listener.accept() {
            Ok((mut stream, _)) => {
                let mut stop_started = false;
                let result: Result<bool, String> = (|| {
                    configure_control_stream(&stream)?;
                    let request: ControlRequest = read_message(&mut stream)?;
                    validate_control(&request, &instance)?;
                    let mut durable_set_status = None;
                    let mut prepared_transition = None;
                    match request.action.as_str() {
                        "query" => {}
                        "stop" => {
                            stop_started = true;
                            snapshot.state = "stopping".into();
                            write_owner_record(&state_path, snapshot.clone())?;
                            runtime.stop(Duration::from_millis(request.timeout_ms))?;
                            snapshot.state = "stopped".into();
                            write_owner_record(&state_path, snapshot.clone())?;
                        }
                        "durable_set" => {
                            if snapshot.state != "ready" {
                                return Err("durable-set owner is not ready".into());
                            }
                            durable_set_status = match request
                                .durable_set
                                .as_ref()
                                .ok_or("missing durable-set owner request")?
                            {
                                DurableSetControlRequestV1::PrepareArm {
                                    operation_id,
                                    replica,
                                } => {
                                    let transition = runtime.prepare_durable_set_transition(
                                        operation_id,
                                        replica.as_ref().clone(),
                                        Duration::from_millis(request.timeout_ms),
                                    )?;
                                    let status = runtime.query_durable_set_transition(
                                        operation_id,
                                        transition.body.boundary_position,
                                    )?;
                                    prepared_transition = Some(transition);
                                    status
                                }
                                DurableSetControlRequestV1::Arm {
                                    operation_id,
                                    transition,
                                } => Some(runtime.arm_durable_set_transition(
                                    operation_id,
                                    transition.as_ref().clone(),
                                )?),
                                DurableSetControlRequestV1::Query {
                                    operation_id,
                                    boundary_position,
                                } => runtime.query_durable_set_transition(
                                    operation_id,
                                    *boundary_position,
                                )?,
                            };
                        }
                        _ => return Err("unsupported owner command".into()),
                    }
                    write_message(
                        &mut stream,
                        &ControlReply {
                            api_version: OWNER_API.into(),
                            snapshot: snapshot.clone(),
                            durable_set_status,
                            prepared_transition,
                            control_error: None,
                        },
                    )?;
                    stream
                        .shutdown(Shutdown::Both)
                        .map_err(|error| format!("close owner control reply: {error}"))?;
                    Ok(request.action == "stop")
                })();
                match result {
                    Ok(true) => return Ok(()),
                    Ok(false) => {}
                    Err(error) => {
                        // A startup query may already have timed out while the
                        // runtime was opening. A disconnected or invalid frame
                        // has not changed this owner's runtime state.
                        if !stop_started {
                            // In particular, a bounded pending-continuation
                            // wait is retryable and must reach the caller as
                            // an explicit unknown result, with no arm write.
                            let _ = write_message(
                                &mut stream,
                                &ControlReply {
                                    api_version: OWNER_API.into(),
                                    snapshot: snapshot.clone(),
                                    durable_set_status: None,
                                    prepared_transition: None,
                                    control_error: Some(bounded_reason(&error)),
                                },
                            );
                            let _ = stream.shutdown(Shutdown::Both);
                            continue;
                        }
                        if snapshot.state != "stopped" {
                            snapshot.state = "unknown".into();
                            snapshot.last_error = Some(bounded_reason(&error));
                            let _ = write_owner_record(&state_path, snapshot.clone());
                        }
                        let _ = write_message(
                            &mut stream,
                            &ControlReply {
                                api_version: OWNER_API.into(),
                                snapshot: snapshot.clone(),
                                durable_set_status: None,
                                prepared_transition: None,
                                control_error: Some(bounded_reason(&error)),
                            },
                        );
                        if snapshot.state == "stopped" {
                            return Ok(());
                        }
                    }
                }
            }
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                std::thread::sleep(Duration::from_millis(20));
            }
            Err(error) => {
                snapshot.state = "unknown".into();
                snapshot.last_error = Some(bounded_reason(&error.to_string()));
                write_owner_record(&state_path, snapshot)?;
                return Err(error.to_string());
            }
        }
    }
}

fn validate_control(
    request: &ControlRequest,
    instance: &FormalRuntimeInstanceV1,
) -> Result<(), String> {
    if request.api_version != OWNER_API
        || (request.action == "durable_set") != request.durable_set.is_some()
        || request.operation_id != instance.operation_id
        || request.request_digest != instance.request_digest
        || request.config_digest != instance.config_digest
        || request.incarnation != instance.incarnation
        || !(1..=60_000).contains(&request.timeout_ms)
    {
        return Err("formal runtime control request differs from exact owner incarnation".into());
    }
    Ok(())
}

fn configure_control_stream(stream: &UnixStream) -> Result<(), String> {
    // BSD may inherit the listener's nonblocking flag. Control frames use
    // bounded blocking reads after accept.
    stream
        .set_nonblocking(false)
        .map_err(|error| format!("configure owner control blocking mode: {error}"))?;
    stream
        .set_read_timeout(Some(Duration::from_secs(2)))
        .map_err(|error| format!("configure owner control read timeout: {error}"))?;
    stream
        .set_write_timeout(Some(Duration::from_secs(2)))
        .map_err(|error| format!("configure owner control write timeout: {error}"))
}
