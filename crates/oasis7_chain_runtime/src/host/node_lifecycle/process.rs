//! Exact local incarnation control. PID is diagnostic only; commands authenticate
//! the private Unix socket against the persisted incarnation/generation/digest.
pub(super) use super::types::{LocalActionRecord as ActionRecord, LocalInstance as Instance};
use super::{
    local_io as io,
    model::{CONFIG_FILE, NodeConfig},
};
use serde::{Deserialize, Serialize};
use std::fs::{self, File, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::os::fd::AsRawFd;
use std::os::unix::{
    fs::{FileTypeExt, OpenOptionsExt, PermissionsExt},
    net::{UnixListener, UnixStream},
    process::CommandExt,
};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

#[path = "formal_full_sync_runtime.rs"]
mod formal_full_sync_runtime;
#[cfg(test)]
#[path = "package_failed_start_tests.rs"]
mod package_failed_start_tests;

const INSTANCE_FILE: &str = "instance.json";
const START_INTENT: &str = "start-action.json";
const MAX_CONTROL: usize = 8192;

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ControlRequest {
    operation: String,
    incarnation: String,
    generation: u64,
    config_digest: String,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ControlResponse {
    instance: Instance,
}

/// Serialize observer status writes with stop so an in-flight poll cannot
/// publish stale status after its incarnation has entered the stop transition.
struct ObserverWorkerStop {
    flag: Arc<AtomicBool>,
    status_gate: Arc<Mutex<()>>,
}
impl ObserverWorkerStop {
    fn stop(&self) {
        let _guard = self
            .status_gate
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        self.flag.store(true, Ordering::SeqCst);
    }
}
impl Drop for ObserverWorkerStop {
    fn drop(&mut self) {
        self.stop();
    }
}

// Transient admission fence shared with immutable management bindings. The
// lifetime process lock is separate: a running receiver must still admit stop.
pub(super) fn admission_lock(data: &Path) -> Result<File, String> {
    let file = io::private_file(&data.join("action-admission.lock"))?;
    file.try_lock()
        .map_err(|_| "native action admission writer busy".to_string())?;
    Ok(file)
}
pub(super) fn lock(data: &Path) -> Result<File, String> {
    let path = data.join("process.lock");
    io::regular(&path)?;
    super::super::controlled_live_config::require_private_owned(&path, false)?;
    let file = OpenOptions::new()
        .read(true)
        .write(true)
        .open(&path)
        .map_err(|e| e.to_string())?;
    file.try_lock()
        .map_err(|_| "another process retains this exact local node ownership lock".to_string())?;
    Ok(file)
}
pub(super) fn read_instance(data: &Path) -> Result<Option<Instance>, String> {
    let path = data.join(INSTANCE_FILE);
    if !path.exists() {
        io::reject_symlinks(&path)?;
        return Ok(None);
    }
    let record: Instance = io::read(&path)?;
    if record.api_version != "oasis7.local-node-instance/v1"
        || record.generation == 0
        || record.incarnation.len() != 32
        || !record.incarnation.bytes().all(|b| b.is_ascii_hexdigit())
    {
        return Err("invalid local instance identity".into());
    }
    Ok(Some(record))
}
pub(super) fn control_dir(data: &Path) -> Result<PathBuf, String> {
    let tmp = fs::canonicalize("/tmp").map_err(|e| e.to_string())?;
    let uid = unsafe { libc::geteuid() };
    let hash = io::digest(&data.to_string_lossy())?;
    Ok(tmp.join(format!("oasis7-node-{uid}-{}", &hash[..32])))
}
fn control_path(data: &Path) -> Result<PathBuf, String> {
    Ok(control_dir(data)?.join("control.sock"))
}
fn control(data: &Path, instance: &Instance, operation: &str) -> Result<Instance, String> {
    let mut socket = UnixStream::connect(control_path(data)?)
        .map_err(|e| format!("exact instance control unavailable: {e}"))?;
    socket
        .set_read_timeout(Some(Duration::from_secs(2)))
        .map_err(|e| e.to_string())?;
    socket
        .set_write_timeout(Some(Duration::from_secs(2)))
        .map_err(|e| e.to_string())?;
    let request = ControlRequest {
        operation: operation.into(),
        incarnation: instance.incarnation.clone(),
        generation: instance.generation,
        config_digest: instance.config_digest.clone(),
    };
    write_line(&mut socket, &request)?;
    let response: ControlResponse = read_line(&mut socket)?;
    if response.instance.incarnation != instance.incarnation
        || response.instance.generation != instance.generation
        || response.instance.config_digest != instance.config_digest
        || response.instance.action_id != instance.action_id
    {
        return Err("control reply belongs to a different local instance".into());
    }
    Ok(response.instance)
}
fn write_line<T: Serialize>(stream: &mut UnixStream, value: &T) -> Result<(), String> {
    let mut bytes = serde_json::to_vec(value).map_err(|e| e.to_string())?;
    if bytes.len() > MAX_CONTROL {
        return Err("control message limit".into());
    }
    bytes.push(b'\n');
    stream.write_all(&bytes).map_err(|e| e.to_string())
}
fn read_line<T: serde::de::DeserializeOwned>(stream: &mut UnixStream) -> Result<T, String> {
    let mut bytes = Vec::new();
    std::io::Read::take(BufReader::new(stream), (MAX_CONTROL + 1) as u64)
        .read_until(b'\n', &mut bytes)
        .map_err(|e| e.to_string())?;
    if bytes.len() > MAX_CONTROL || bytes.last() != Some(&b'\n') {
        return Err("incomplete or oversized control message".into());
    }
    serde_json::from_slice(&bytes).map_err(|e| e.to_string())
}
fn write_instance(data: &Path, instance: &Instance) -> Result<(), String> {
    io::write(&data.join(INSTANCE_FILE), instance)
}
pub(super) fn action_path(data: &Path, id: &str) -> Result<PathBuf, String> {
    if id.trim().is_empty() || id.len() > 256 {
        return Err("invalid local action identity".into());
    }
    Ok(data
        .join("actions")
        .join(format!("{}.json", io::digest(&id)?)))
}
pub(super) fn save_action(data: &Path, action: &ActionRecord) -> Result<(), String> {
    io::private_dir(&data.join("actions"))?;
    let path = action_path(data, &action.action_id)?;
    if path.exists() {
        let previous: ActionRecord = io::read(&path)?;
        if previous.action_id != action.action_id
            || previous.payload_digest != action.payload_digest
            || previous.kind != action.kind
            || previous.expected_generation != action.expected_generation
            || previous.target.config_digest != action.target.config_digest
            || previous.target.action_id != action.target.action_id
            || previous.target.incarnation != action.target.incarnation
            || previous.target.generation != action.target.generation
        {
            return Err("local action identity payload/instance conflict".into());
        }
    }
    io::write(&path, action)
}
fn config(data: &Path) -> Result<NodeConfig, String> {
    NodeConfig::load(&data.join(CONFIG_FILE))
}
fn status_value(
    config: &NodeConfig,
    instance: Option<Instance>,
    blocker: Option<String>,
) -> Result<serde_json::Value, String> {
    let changed = instance
        .as_ref()
        .is_some_and(|i| io::digest(config).ok().as_ref() != Some(&i.config_digest));
    let observer_status = if config.identity.chain.formal_observer().is_some() {
        instance.as_ref().and_then(|instance| {
            io::read::<super::formal_observer::FormalObserverStatusV1>(
                &config
                    .identity
                    .data_dir
                    .join(super::formal_observer::STATUS_FILE),
            )
            .ok()
            .filter(|status| {
                status.incarnation == instance.incarnation
                    && status.generation == instance.generation
                    && status.config_digest == instance.config_digest
            })
        })
    } else {
        None
    };
    let full_sync_status = if config.identity.chain.formal_full_sync().is_some() {
        instance.as_ref().and_then(|instance| {
            io::read::<super::formal_full_sync::FormalFullSyncStatusV1>(
                &config
                    .identity
                    .data_dir
                    .join(super::formal_full_sync::STATUS_FILE),
            )
            .ok()
            .filter(|status| {
                status.incarnation == instance.incarnation
                    && status.generation == instance.generation
                    && status.config_digest == instance.config_digest
            })
        })
    } else {
        None
    };
    let active_instance = instance.as_ref().is_some_and(|i| i.state == "running");
    let full_sync_ready = blocker.is_none()
        && active_instance
        && full_sync_status.as_ref().is_some_and(|status| {
            status.ready && status.state_root_verified && status.world_loaded
        });
    let formal_readiness = blocker.is_none()
        && active_instance
        && (observer_status
            .as_ref()
            .is_some_and(|status| status.read_ready)
            || full_sync_ready);
    let full_state_sync_readiness = full_sync_status.as_ref().map(|status| {
        if full_sync_ready {
            "ready".to_owned()
        } else {
            status.state.clone()
        }
    });
    let full_sync_status = full_sync_status.map(|mut status| {
        if !full_sync_ready {
            status.ready = false;
            status.can_propose = false;
            status.world_loaded = false;
        }
        status
    });
    let configured_role = config.identity.chain.local_role();
    Ok(
        serde_json::json!({"api_version":"oasis7.local-node-status/v1","node_id":config.identity.node_id,"peer_id":config.identity.peer_id,"chain":config.identity.chain,
        "config_digest":io::digest(config)?,"local_generation":instance.as_ref().map_or(0,|i|i.generation),"instance":instance,"blocker":blocker,"configuration_changed":changed,"configured_role":configured_role,"runtime_profile":config.identity.chain.runtime_profile(),
        "verified_sync_readiness":full_state_sync_readiness,"formal_readiness":formal_readiness,"formal_public_observer":observer_status,"formal_full_state_sync":full_sync_status,"can_propose":false,"governance_membership":null,"applied_consensus_membership":null,"paths":config.paths(),"control_socket":control_path(&config.identity.data_dir)?}),
    )
}
pub(super) fn status(data: &Path) -> Result<serde_json::Value, String> {
    let config = config(data)?;
    let current = read_instance(&config.identity.data_dir)?;
    if let Ok(_lock) = lock(&config.identity.data_dir) {
        let blocker = current.as_ref().filter(|i| !matches!(i.state.as_str(), "stopped" | "failed")).map(|_| "supervisor ownership ended without a durable terminal result; original instance outcome is unknown".into());
        if current.is_none() && config.identity.data_dir.join(START_INTENT).exists() {
            let intent: ActionRecord = io::read(&config.identity.data_dir.join(START_INTENT))?;
            return status_value(
                &config,
                Some(intent.target),
                Some(
                    "start intent exists without an admitted receiver; original outcome is unknown"
                        .into(),
                ),
            );
        }
        return status_value(&config, current, blocker);
    }
    let Some(current) = current else {
        return status_value(
            &config,
            None,
            Some("ownership lock held before instance admission; outcome unknown".into()),
        );
    };
    match control(&config.identity.data_dir, &current, "query") {
        Ok(current) => status_value(&config, Some(current), None),
        Err(error) => status_value(&config, Some(current), Some(error)),
    }
}
pub(super) fn start(path: &Path) -> Result<serde_json::Value, String> {
    start_with_action(path, None, None)
}
pub(super) fn start_with_action(
    path: &Path,
    action_id: Option<&str>,
    expected_generation: Option<u64>,
) -> Result<serde_json::Value, String> {
    start_with_action_using_runtime(path, action_id, expected_generation, None)
}

pub(super) fn start_with_action_using_runtime(
    path: &Path,
    action_id: Option<&str>,
    expected_generation: Option<u64>,
    runtime_artifact: Option<&super::LocalRuntimeArtifactV1>,
) -> Result<serde_json::Value, String> {
    let config = NodeConfig::load(path)?;
    let data = &config.identity.data_dir;
    let digest = io::digest(&config)?;
    let admission = admission_lock(data)?;
    if let Some(id) = action_id {
        let path = action_path(data, id)?;
        if path.exists() {
            let original: ActionRecord = io::read(&path)?;
            if original.kind != "start"
                || original.target.config_digest != digest
                || original.runtime_artifact.as_ref() != runtime_artifact
            {
                return Err("start action identity reused with another payload".into());
            }
            let current = read_instance(data)?;
            let resumable_queued = original.target.state == "queued"
                && current.as_ref().is_none_or(|i| {
                    i.incarnation == original.target.incarnation
                        && i.generation == original.target.generation
                        && i.state == "queued"
                });
            let observation = if current.as_ref().is_some_and(|i| {
                i.incarnation == original.target.incarnation
                    && i.generation == original.target.generation
            }) {
                current.unwrap()
            } else {
                original.target
            };
            if !resumable_queued {
                return status_value(
                    &config,
                    Some(observation),
                    Some(
                        "original action result only; a later instance is never substituted".into(),
                    ),
                );
            }
        }
    }

    let ownership = match lock(data) {
        Ok(lock) => lock,
        Err(_) => {
            let current = read_instance(data)?.ok_or("ownership is unresolved; no new action")?;
            if current.config_digest != digest {
                return Err(
                    "already running with a different local configuration; explicit stop required"
                        .into(),
                );
            }
            return match control(data, &current, "query") {
                Ok(current) => status_value(&config, Some(current), None),
                Err(error) => status_value(&config, Some(current), Some(error)),
            };
        }
    };
    if let Some(expected) = expected_generation {
        let observed = read_instance(data)?;
        let replaying_same_queued = observed.as_ref().is_some_and(|i| {
            i.state == "queued"
                && action_id == Some(i.action_id.as_str())
                && i.generation == expected.saturating_add(1)
        });
        if !replaying_same_queued && observed.as_ref().map_or(0, |i| i.generation) != expected {
            return Err("start expected local generation changed before locked admission".into());
        }
    }
    let intent_path = data.join(START_INTENT);
    let mut intent = if intent_path.exists() {
        let previous: ActionRecord = io::read(&intent_path)?;
        let outcome = read_instance(data)?;
        let terminal = outcome.as_ref().is_some_and(|i| {
            i.incarnation == previous.target.incarnation
                && matches!(i.state.as_str(), "stopped" | "failed")
        });
        if !terminal {
            if previous.target.config_digest != digest
                || action_id.is_some_and(|id| id != previous.action_id)
            {
                return Err(
                    "original start action unresolved; cannot replace its payload or identity"
                        .into(),
                );
            }
            if previous.target.state == "queued"
                && outcome.as_ref().is_none_or(|i| {
                    i.incarnation == previous.target.incarnation
                        && i.generation == previous.target.generation
                        && i.state == "queued"
                })
            {
                // Replay the SAME admission identity only. Receiver rechecks the
                // current intent after acquiring its inherited ownership lock.
                previous
            } else {
                return status_value(
                    &config,
                    outcome,
                    Some("original start outcome unknown; explicit reconciliation required".into()),
                );
            }
        } else {
            new_start(data, digest.clone(), action_id, runtime_artifact.cloned())?
        }
    } else {
        new_start(data, digest.clone(), action_id, runtime_artifact.cloned())?
    };
    if config.identity.chain.formal_observer().is_some()
        || config.identity.chain.formal_full_sync().is_some()
    {
        config.verify_identity()?;
        config.identity.chain.verify()?;
    } else {
        config.runtime_args()?;
    }
    // The durable pointer precedes its indexed receipt so a crash never leaves
    // an unreferenced queued action that a plain local retry could replace.
    io::write(&intent_path, &intent)?;
    save_action(data, &intent)?;
    let logs = data.join("logs");
    io::private_dir(&logs)?;
    let output = log_file(&logs.join("runtime.log"))?;
    let executable = match runtime_artifact {
        Some(artifact) => verified_package_runtime_path(artifact)?,
        None => std::env::current_exe().map_err(|e| e.to_string())?,
    };
    let mut child_command = Command::new(executable);
    #[cfg(test)]
    if runtime_artifact.is_some() {
        child_command
            .args([
                "--exact",
                package_failed_start_tests::SUPERVISOR_ENTRY,
                "--nocapture",
            ])
            .env(
                package_failed_start_tests::SUPERVISOR_CONFIG_ENV,
                path.to_str().ok_or("non-UTF8 node config path")?,
            )
            .env(
                package_failed_start_tests::SUPERVISOR_INCARNATION_ENV,
                &intent.target.incarnation,
            )
            .env(
                package_failed_start_tests::SUPERVISOR_ACTION_ENV,
                &intent.action_id,
            );
    }
    #[cfg(test)]
    if runtime_artifact.is_none() {
        child_command.args([
            "node",
            "_run",
            "--config",
            path.to_str().ok_or("non-UTF8 node config path")?,
            "--incarnation",
            &intent.target.incarnation,
        ]);
    }
    #[cfg(not(test))]
    child_command.args([
        "node",
        "_run",
        "--config",
        path.to_str().ok_or("non-UTF8 node config path")?,
        "--incarnation",
        &intent.target.incarnation,
    ]);
    let child = child_command
        .stdin(Stdio::null())
        .stdout(Stdio::from(output.try_clone().map_err(|e| e.to_string())?))
        .stderr(Stdio::from(output))
        .spawn();
    match child {
        Ok(child) => {
            intent.target.supervisor_pid = child.id();
            save_action(data, &intent)?;
        }
        Err(e) => {
            intent.target.state = "failed".into();
            write_instance(data, &intent.target)?;
            save_action(data, &intent)?;
            return Err(e.to_string());
        }
    }
    drop(ownership);
    drop(admission);
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        if let Some(current) = read_instance(data)?
            && current.incarnation == intent.target.incarnation
        {
            if let Ok(current) = control(data, &current, "query") {
                return status_value(&config, Some(current), None);
            }
            if matches!(current.state.as_str(), "failed" | "stopped") {
                return status_value(
                    &config,
                    Some(current),
                    Some("runtime ended before local start completed".into()),
                );
            }
        }
        if Instant::now() >= deadline {
            return status(data);
        }
        std::thread::sleep(Duration::from_millis(20));
    }
}
fn new_start(
    data: &Path,
    digest: String,
    action: Option<&str>,
    runtime_artifact: Option<super::LocalRuntimeArtifactV1>,
) -> Result<ActionRecord, String> {
    let generation = read_instance(data)?.map_or(Ok(1), |i| {
        i.generation
            .checked_add(1)
            .ok_or("local generation exhausted".to_string())
    })?;
    Ok(ActionRecord {
        api_version: "oasis7.local-node-action/v1".into(),
        action_id: action.map(str::to_string).unwrap_or(io::random_id()?),
        payload_digest: match &runtime_artifact {
            Some(artifact) => {
                io::digest(&("start_package", digest.clone(), generation - 1, artifact))?
            }
            None => io::digest(&("start", digest.clone(), generation - 1))?,
        },
        kind: "start".into(),
        expected_generation: generation - 1,
        target: Instance {
            api_version: "oasis7.local-node-instance/v1".into(),
            action_id: action.map(str::to_string).unwrap_or_default(),
            generation,
            incarnation: io::random_id()?,
            config_digest: digest,
            supervisor_pid: 0,
            runtime_pid: None,
            started_unix_ms: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map_err(|e| e.to_string())?
                .as_millis(),
            state: "queued".into(),
            exit_code: None,
        },
        runtime_artifact,
    })
    .map(|mut a| {
        a.target.action_id = a.action_id.clone();
        a
    })
}

fn verified_package_runtime_path(
    artifact: &super::LocalRuntimeArtifactV1,
) -> Result<PathBuf, String> {
    if artifact.api_version != "oasis7.local-package-runtime-artifact/v1"
        || !super::package::supports_profile(&artifact.profile)
        || !artifact.runtime_path.is_absolute()
        || io::sha256(&artifact.runtime_path)? != artifact.runtime_sha256
    {
        return Err("verified package runtime artifact changed before spawn".into());
    }
    Ok(artifact.runtime_path.clone())
}
fn log_file(path: &Path) -> Result<File, String> {
    io::reject_symlinks(path)?;
    let file = OpenOptions::new()
        .create(true)
        .append(true)
        .mode(0o600)
        .open(path)
        .map_err(|e| e.to_string())?;
    super::super::controlled_live_config::require_private_owned(path, false)?;
    Ok(file)
}
fn supervise_formal_public_observer(
    config: &NodeConfig,
    intent: &mut ActionRecord,
    ownership: File,
    observer_config: &super::formal_observer::FormalObserverConfigV1,
) -> Result<(), String> {
    let data = &config.identity.data_dir;
    let socket_dir = control_dir(data)?;
    io::private_dir(&socket_dir)?;
    let socket_path = control_path(data)?;
    if let Ok(metadata) = fs::symlink_metadata(&socket_path) {
        if !metadata.file_type().is_socket() {
            return Err("existing control path is not a socket".into());
        }
        fs::remove_file(&socket_path).map_err(|e| e.to_string())?;
    }
    let listener = UnixListener::bind(&socket_path).map_err(|e| e.to_string())?;
    fs::set_permissions(&socket_path, fs::Permissions::from_mode(0o600))
        .map_err(|e| e.to_string())?;
    listener.set_nonblocking(true).map_err(|e| e.to_string())?;

    let mut session =
        match super::formal_observer::Session::open(observer_config, data, &intent.target) {
            Ok(session) => session,
            Err(error) => {
                intent.target.state = "failed".into();
                write_instance(data, &intent.target)?;
                save_action(data, intent)?;
                let _ = fs::remove_file(&socket_path);
                return Err(error);
            }
        };
    intent.target.runtime_pid = None;
    intent.target.state = "running".into();
    write_instance(data, &intent.target)?;
    save_action(data, intent)?;
    io::write(
        &data.join(super::formal_observer::STATUS_FILE),
        session.observed(),
    )?;

    let stop_polling = Arc::new(AtomicBool::new(false));
    let worker_stop = stop_polling.clone();
    let status_gate = Arc::new(Mutex::new(()));
    let worker_status_gate = status_gate.clone();
    let stop_guard = ObserverWorkerStop {
        flag: stop_polling.clone(),
        status_gate: status_gate.clone(),
    };
    let status_path = data.join(super::formal_observer::STATUS_FILE);
    let poll_interval = session.poll_interval();
    let (worker_result_tx, worker_result_rx) = std::sync::mpsc::channel();
    let worker = std::thread::Builder::new()
        .name("formal-public-observer".into())
        .spawn(move || {
            let result = (|| {
                let mut next_poll = Instant::now();
                while !worker_stop.load(Ordering::SeqCst) {
                    if Instant::now() >= next_poll {
                        session.poll();
                        let _status_guard = worker_status_gate
                            .lock()
                            .map_err(|error| error.to_string())?;
                        if worker_stop.load(Ordering::SeqCst) {
                            break;
                        }
                        io::write(&status_path, session.observed())?;
                        next_poll = Instant::now() + poll_interval;
                    }
                    std::thread::sleep(Duration::from_millis(20));
                }
                Ok(())
            })();
            let _ = worker_result_tx.send(result);
        })
        .map_err(|error| error.to_string())?;

    let signal_stop = Arc::new(AtomicBool::new(false));
    let signal_flag = signal_stop.clone();
    ctrlc::set_handler(move || {
        signal_flag.store(true, Ordering::SeqCst);
    })
    .map_err(|error| error.to_string())?;
    loop {
        if signal_stop.load(Ordering::SeqCst) {
            stop_guard.stop();
            finish_formal_observer_stop(data, intent, &socket_path)?;
            drop(ownership);
            drop(worker);
            return Ok(());
        }
        let worker_result = match worker_result_rx.try_recv() {
            Ok(result) => Some(result),
            Err(std::sync::mpsc::TryRecvError::Disconnected) => Some(Err(
                "formal observer worker exited without reporting its result".into(),
            )),
            Err(std::sync::mpsc::TryRecvError::Empty) => None,
        };
        if let Some(result) = worker_result {
            stop_guard.stop();
            intent.target.state = "failed".into();
            write_instance(data, &intent.target)?;
            save_action(data, intent)?;
            let _ = fs::remove_file(&socket_path);
            drop(ownership);
            drop(worker);
            return Err(result.err().unwrap_or_else(|| {
                "formal observer worker stopped without an instance stop request".into()
            }));
        }
        match listener.accept() {
            Ok((mut socket, _)) => {
                socket
                    .set_read_timeout(Some(Duration::from_secs(1)))
                    .map_err(|error| error.to_string())?;
                socket
                    .set_write_timeout(Some(Duration::from_secs(1)))
                    .map_err(|error| error.to_string())?;
                if let Ok(request) = read_line::<ControlRequest>(&mut socket) {
                    let exact = request.incarnation == intent.target.incarnation
                        && request.generation == intent.target.generation
                        && request.config_digest == intent.target.config_digest;
                    if exact && request.operation == "query" {
                        let _ = write_line(
                            &mut socket,
                            &ControlResponse {
                                instance: intent.target.clone(),
                            },
                        );
                    } else if exact && request.operation == "stop" {
                        stop_guard.stop();
                        finish_formal_observer_stop(data, intent, &socket_path)?;
                        let _ = write_line(
                            &mut socket,
                            &ControlResponse {
                                instance: intent.target.clone(),
                            },
                        );
                        drop(ownership);
                        drop(worker);
                        return Ok(());
                    }
                }
            }
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {}
            Err(error) => return Err(error.to_string()),
        }
        std::thread::sleep(Duration::from_millis(20));
    }
}
fn finish_formal_observer_stop(
    data: &Path,
    intent: &mut ActionRecord,
    socket_path: &Path,
) -> Result<(), String> {
    intent.target.state = "draining".into();
    write_instance(data, &intent.target)?;
    save_action(data, intent)?;
    intent.target.state = "stopped".into();
    intent.target.exit_code = Some(0);
    write_instance(data, &intent.target)?;
    save_action(data, intent)?;
    fs::remove_file(socket_path).map_err(|error| error.to_string())?;
    let status_path = data.join(super::formal_observer::STATUS_FILE);
    if let Ok(mut status) = io::read::<super::formal_observer::FormalObserverStatusV1>(&status_path)
        && status.incarnation == intent.target.incarnation
        && status.generation == intent.target.generation
        && status.config_digest == intent.target.config_digest
    {
        status.read_ready = false;
        status.read_state = "stopped".into();
        status.error = None;
        let _ = io::write(&status_path, &status);
    }
    Ok(())
}
pub(super) fn supervise(path: &Path, incarnation: &str) -> Result<(), String> {
    let config = NodeConfig::load(path)?;
    let data = &config.identity.data_dir;
    let mut intent: ActionRecord = io::read(&data.join(START_INTENT))?;
    if intent.target.incarnation != incarnation
        || intent.target.config_digest != io::digest(&config)?
    {
        return Err("supervisor action identity/configuration mismatch".into());
    }
    if let Some(artifact) = &intent.runtime_artifact {
        let executable = std::env::current_exe().map_err(|error| error.to_string())?;
        if executable
            .canonicalize()
            .map_err(|error| error.to_string())?
            != artifact.runtime_path
            || io::sha256(&artifact.runtime_path)? != artifact.runtime_sha256
        {
            return Err(
                "package supervisor executable differs from its verified runtime artifact".into(),
            );
        }
    }
    // Parent persists intent before spawn and drops its admission lock afterward.
    let deadline = Instant::now() + Duration::from_secs(10);
    let ownership = loop {
        if let Ok(file) = lock(data) {
            break file;
        }
        if Instant::now() >= deadline {
            return Err("supervisor admission ownership remained locked".into());
        }
        std::thread::sleep(Duration::from_millis(10));
    };
    let admitted: ActionRecord = io::read(&data.join(START_INTENT))?;
    if admitted.action_id != intent.action_id
        || admitted.target.incarnation != intent.target.incarnation
        || admitted.target.generation != intent.target.generation
        || admitted.payload_digest != intent.payload_digest
    {
        return Err("stale supervisor action; newer incarnation remains untouched".into());
    }
    if read_instance(data)?.is_some_and(|i| {
        i.generation > intent.target.generation
            || (i.incarnation == incarnation && i.state != "queued")
    }) {
        return Err("this incarnation already has a receiver result".into());
    }
    intent.target.supervisor_pid = std::process::id();
    write_instance(data, &intent.target)?;
    if let Some(full_sync) = config.identity.chain.formal_full_sync() {
        return formal_full_sync_runtime::supervise(&config, &mut intent, ownership, full_sync);
    }
    if let Some(observer) = config.identity.chain.formal_observer() {
        return supervise_formal_public_observer(&config, &mut intent, ownership, observer);
    }
    let arguments = match config.runtime_args() {
        Ok(args) => args,
        Err(error) => {
            intent.target.state = "failed".into();
            write_instance(data, &intent.target)?;
            save_action(data, &intent)?;
            return Err(error);
        }
    };
    #[cfg(test)]
    // Keep this injected failure before bootstrap's first durable world copy.
    // The package test stages an uninitialized node; materializing its seed
    // World and then pretending startup failed would leave an unqualified
    // World without execution history, which correctly blocks rollback.
    if intent.runtime_artifact.is_some()
        && std::env::var(package_failed_start_tests::FAIL_ACTION_ENV)
            .ok()
            .as_deref()
            == Some(intent.action_id.as_str())
    {
        intent.target.state = "failed".into();
        intent.target.exit_code = Some(package_failed_start_tests::INJECTED_EXIT_CODE);
        write_instance(data, &intent.target)?;
        save_action(data, &intent)?;
        drop(ownership);
        return Err(format!(
            "test-only exact package startup failure for action {}",
            intent.action_id
        ));
    }
    if let Err(error) = super::bootstrap::prepare(&config, incarnation) {
        intent.target.state = "failed".into();
        write_instance(data, &intent.target)?;
        save_action(data, &intent)?;
        return Err(error);
    }
    let signal_stop = Arc::new(AtomicBool::new(false));
    let flag = signal_stop.clone();
    ctrlc::set_handler(move || {
        flag.store(true, Ordering::SeqCst);
    })
    .map_err(|e| e.to_string())?;
    let mut command = Command::new(std::env::current_exe().map_err(|e| e.to_string())?);
    #[cfg(test)]
    if intent.runtime_artifact.is_some() {
        command.args([
            "--exact",
            package_failed_start_tests::RUNTIME_ENTRY,
            "--nocapture",
        ]);
    } else {
        command.args(arguments.iter().map(String::as_str));
    }
    #[cfg(not(test))]
    command.args(arguments);
    #[cfg(test)]
    if intent.runtime_artifact.is_some() {
        command.env(package_failed_start_tests::RUNTIME_CHILD_ENV, "1");
    }
    command.stdin(Stdio::null());
    supervise_command(&config, &mut intent, ownership, command, signal_stop)
}
fn supervise_command(
    config: &NodeConfig,
    intent: &mut ActionRecord,
    ownership: File,
    mut command: Command,
    signal_stop: Arc<AtomicBool>,
) -> Result<(), String> {
    let data = &config.identity.data_dir;
    let socket_dir = control_dir(data)?;
    io::private_dir(&socket_dir)?;
    let socket_path = control_path(data)?;
    if let Ok(metadata) = fs::symlink_metadata(&socket_path) {
        if !metadata.file_type().is_socket() {
            return Err("existing control path is not a socket".into());
        }
        fs::remove_file(&socket_path).map_err(|e| e.to_string())?;
    }
    let listener = UnixListener::bind(&socket_path).map_err(|e| e.to_string())?;
    fs::set_permissions(&socket_path, fs::Permissions::from_mode(0o600))
        .map_err(|e| e.to_string())?;
    listener.set_nonblocking(true).map_err(|e| e.to_string())?;
    // Retain this SAME lock description in the owned runtime child. A killed
    // supervisor leaves an unresolved live runtime holding the lock; a new
    // incarnation cannot start, and stop never guesses from a reused PID.
    let descriptor = ownership.as_raw_fd();
    unsafe {
        command.pre_exec(move || {
            let inherited = libc::fcntl(descriptor, libc::F_DUPFD, 3);
            if inherited < 0 {
                return Err(std::io::Error::last_os_error());
            }
            if libc::fcntl(inherited, libc::F_SETFD, 0) < 0 {
                return Err(std::io::Error::last_os_error());
            }
            Ok(())
        });
    }
    let mut child = match command.spawn() {
        Ok(child) => child,
        Err(e) => {
            intent.target.state = "failed".into();
            write_instance(data, &intent.target)?;
            save_action(data, intent)?;
            let _ = fs::remove_file(&socket_path);
            return Err(e.to_string());
        }
    };
    intent.target.runtime_pid = Some(child.id());
    intent.target.state = "running".into();
    write_instance(data, &intent.target)?;
    save_action(data, intent)?;
    loop {
        if let Some(exit) = child.try_wait().map_err(|e| e.to_string())? {
            // The owned child has been reaped. Persist that fact before any
            // package promotion consumes the exact terminal owner receipt.
            intent.target.runtime_pid = None;
            intent.target.state = "stopped".into();
            intent.target.exit_code = exit.code();
            write_instance(data, &intent.target)?;
            save_action(data, intent)?;
            let _ = fs::remove_file(&socket_path);
            break;
        }
        if signal_stop.load(Ordering::SeqCst) {
            begin_stop(data, &mut intent.target, &mut child)?;
        }
        match listener.accept() {
            Ok((mut socket, _)) => {
                socket
                    .set_read_timeout(Some(Duration::from_secs(1)))
                    .map_err(|e| e.to_string())?;
                socket
                    .set_write_timeout(Some(Duration::from_secs(1)))
                    .map_err(|e| e.to_string())?;
                if let Ok(request) = read_line::<ControlRequest>(&mut socket)
                    && request.incarnation == intent.target.incarnation
                    && request.generation == intent.target.generation
                    && request.config_digest == intent.target.config_digest
                {
                    if request.operation == "stop" {
                        begin_stop(data, &mut intent.target, &mut child)?;
                    }
                    if matches!(request.operation.as_str(), "query" | "stop") {
                        let _ = write_line(
                            &mut socket,
                            &ControlResponse {
                                instance: intent.target.clone(),
                            },
                        );
                    }
                }
            }
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {}
            Err(e) => return Err(e.to_string()),
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    drop(ownership);
    Ok(())
}
fn begin_stop(data: &Path, instance: &mut Instance, child: &mut Child) -> Result<(), String> {
    if instance.state == "draining" {
        return Ok(());
    }
    instance.state = "draining".into();
    write_instance(data, instance)?;
    // The Child is owned and unreaped here, so its PID cannot have been reused.
    let result = unsafe { libc::kill(child.id() as i32, libc::SIGTERM) };
    if result != 0 {
        return Err(std::io::Error::last_os_error().to_string());
    }
    Ok(())
}
pub(super) fn stop(data: &Path, timeout_ms: u64) -> Result<serde_json::Value, String> {
    let config = config(data)?;
    let Some(instance) = read_instance(&config.identity.data_dir)? else {
        return status_value(&config, None, None);
    };
    stop_instance(&config, &instance, timeout_ms)
}
pub(super) fn stop_instance(
    config: &NodeConfig,
    expected: &Instance,
    timeout_ms: u64,
) -> Result<serde_json::Value, String> {
    stop_with_action(config, expected, timeout_ms, None)
}
pub(super) fn stop_with_action(
    config: &NodeConfig,
    expected: &Instance,
    timeout_ms: u64,
    requested_action_id: Option<&str>,
) -> Result<serde_json::Value, String> {
    let data = &config.identity.data_dir;
    let admission = admission_lock(data)?;
    let current = read_instance(data)?.ok_or("target instance missing")?;
    if current.incarnation != expected.incarnation
        || current.generation != expected.generation
        || current.config_digest != expected.config_digest
    {
        return Err("stale stop target; later incarnation is untouched".into());
    }
    if matches!(current.state.as_str(), "stopped" | "failed") {
        return status_value(config, Some(current), None);
    }
    // Query the original receiver before sending or resending its fixed action.
    let current = control(data, &current, "query")?;
    let action_id = requested_action_id
        .map(str::to_string)
        .unwrap_or_else(|| format!("stop:{}:{}", current.incarnation, current.generation));
    let mut action = ActionRecord {
        api_version: "oasis7.local-node-action/v1".into(),
        action_id,
        payload_digest: io::digest(&(
            "stop",
            current.incarnation.clone(),
            current.generation,
            current.config_digest.clone(),
        ))?,
        kind: "stop".into(),
        expected_generation: current.generation,
        target: current.clone(),
        runtime_artifact: None,
    };
    save_action(data, &action)?;
    drop(admission);
    let requested = control(data, &current, "stop");
    let deadline = Instant::now() + Duration::from_millis(timeout_ms);
    loop {
        let result = read_instance(data)?.ok_or("original stopped instance result missing")?;
        if result.incarnation != expected.incarnation || result.generation != expected.generation {
            return Err("stop completion cannot be attributed to a later incarnation".into());
        }
        if matches!(result.state.as_str(), "stopped" | "failed") {
            action.target = result.clone();
            save_action(data, &action)?;
            return status_value(config, Some(result), None);
        }
        if Instant::now() >= deadline {
            // Last query is reconciliation, not a new stop or cancellation.
            let observed = control(data, expected, "query").unwrap_or(result);
            action.target = observed.clone();
            save_action(data, &action)?;
            return status_value(
                config,
                Some(observed),
                Some(requested.err().unwrap_or_else(|| {
                    "stop wait timed out; exact original instance remains pending".into()
                })),
            );
        }
        std::thread::sleep(Duration::from_millis(20));
    }
}
#[cfg(test)]
pub(super) fn run_test_receiver(
    config: &NodeConfig,
    intent: ActionRecord,
    command: Command,
    ready: std::sync::mpsc::Sender<()>,
) -> Result<(), String> {
    run_test_receiver_with_stop_signal(
        config,
        intent,
        command,
        ready,
        Arc::new(AtomicBool::new(false)),
    )
}
#[cfg(test)]
pub(super) fn run_test_receiver_with_stop_signal(
    config: &NodeConfig,
    mut intent: ActionRecord,
    command: Command,
    ready: std::sync::mpsc::Sender<()>,
    signal_stop: Arc<AtomicBool>,
) -> Result<(), String> {
    let ownership = lock(&config.identity.data_dir)?;
    write_instance(&config.identity.data_dir, &intent.target)?;
    ready.send(()).map_err(|e| e.to_string())?;
    supervise_command(config, &mut intent, ownership, command, signal_stop)
}
#[cfg(test)]
pub(super) fn test_intent(config: &NodeConfig) -> Result<ActionRecord, String> {
    new_start(&config.identity.data_dir, io::digest(config)?, None, None)
}
