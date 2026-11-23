//! Local supervisor for the formal full-state read-only sync profile.
//!
//! The network poll runs separately from the authenticated control socket so
//! stop/query remain responsive while a bounded catch-up or CAS transfer is
//! in progress. Readiness is durable only for the exact local incarnation and
//! only after the verified World has been restored and retained in memory.

use super::*;
use std::sync::mpsc;

#[cfg(test)]
#[path = "formal_full_sync_worker_ownership_tests.rs"]
mod worker_ownership_tests;

/// The last live worker, not merely the control socket, owns the lifetime fence.
/// Stop may detach a bounded network poll; it must not permit a new incarnation
/// to enter while the old poll can still access the same persistent state.
pub(super) fn spawn_owned_worker<T, F>(
    ownership: Arc<ProcessOwnership>,
    work: F,
) -> std::io::Result<std::thread::JoinHandle<T>>
where
    T: Send + 'static,
    F: FnOnce() -> T + Send + 'static,
{
    std::thread::Builder::new()
        .name("formal-full-state-sync".into())
        .spawn(move || {
            let result = work();
            // Also released by unwinding if work panics. No explicit unlock:
            // the supervisor may still own this same file description.
            drop(ownership);
            result
        })
}

struct WorkerStop {
    flag: Arc<AtomicBool>,
    status_gate: Arc<Mutex<()>>,
}
impl WorkerStop {
    fn stop(&self) {
        let _guard = self
            .status_gate
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        self.flag.store(true, Ordering::SeqCst);
    }
}
impl Drop for WorkerStop {
    fn drop(&mut self) {
        self.stop();
    }
}

enum PollSession {
    Legacy {
        session: super::super::formal_full_sync::FormalFullSyncSession,
        restored_world: Option<oasis7_runtime::World>,
    },
    Budgeted(super::super::formal_full_sync::operation_worker::BudgetedDriver),
}
impl PollSession {
    fn poll_interval(&self) -> Duration {
        match self {
            Self::Legacy { session, .. } => session.poll_interval(),
            Self::Budgeted(driver) => driver.poll_interval(),
        }
    }
    fn begin_poll(&mut self) {
        if let Self::Legacy {
            session,
            restored_world,
        } = self
        {
            drop(restored_world.take());
            session.begin_poll();
        }
    }
    fn poll(&mut self) -> Result<(), String> {
        match self {
            Self::Legacy {
                session,
                restored_world,
            } => {
                *restored_world = session.sync_latest().ok().map(|restored| restored.world);
                session.set_world_loaded(restored_world.is_some());
                if session.status().ready
                    && (!session.status().state_root_verified || restored_world.is_none())
                {
                    return Err("full-sync readiness lacked a retained verified World".into());
                }
            }
            Self::Budgeted(driver) => driver.poll(),
        }
        Ok(())
    }
    fn status(&self, node: &NodeConfig) -> Result<serde_json::Value, String> {
        match self {
            Self::Legacy { session, .. } => {
                serde_json::to_value(session.status()).map_err(|e| e.to_string())
            }
            Self::Budgeted(driver) => super::super::formal_full_sync::operation_status::running(
                node,
                &*driver.observation.lock().map_err(|e| e.to_string())?,
            ),
        }
    }
}

pub(super) fn supervise(
    config: &NodeConfig,
    intent: &mut ActionRecord,
    ownership: ProcessOwnership,
    full_sync_config: &super::super::formal_full_sync::FormalFullSyncConfigV1,
) -> Result<(), String> {
    supervise_inner(config, intent, ownership, Some(full_sync_config))
}

pub(super) fn supervise_budgeted(
    config: &NodeConfig,
    intent: &mut ActionRecord,
    ownership: ProcessOwnership,
) -> Result<(), String> {
    supervise_inner(config, intent, ownership, None)
}

fn supervise_inner(
    config: &NodeConfig,
    intent: &mut ActionRecord,
    ownership: ProcessOwnership,
    full_sync_config: Option<&super::super::formal_full_sync::FormalFullSyncConfigV1>,
) -> Result<(), String> {
    let data = &config.identity.data_dir;
    ownership.require_directory(data)?;
    let socket_dir = control_dir(data)?;
    io::private_dir(&socket_dir)?;
    let socket_path = control_path(data)?;
    if let Ok(metadata) = fs::symlink_metadata(&socket_path) {
        if !metadata.file_type().is_socket() {
            return Err("existing control path is not a socket".into());
        }
        fs::remove_file(&socket_path).map_err(|error| error.to_string())?;
    }
    let listener = UnixListener::bind(&socket_path).map_err(|error| error.to_string())?;
    fs::set_permissions(&socket_path, fs::Permissions::from_mode(0o600))
        .map_err(|error| error.to_string())?;
    listener
        .set_nonblocking(true)
        .map_err(|error| error.to_string())?;

    let stop_polling = Arc::new(AtomicBool::new(false));
    let status_gate = Arc::new(Mutex::new(()));
    let ownership = Arc::new(ownership);
    let initialized = if let Some(full_sync_config) = full_sync_config {
        let (keypair, _) = super::super::super::node_keypair_config::read_existing_node_keypair_in_secure_config_dir(
            &data.join("keys"),
        )?;
        let state_root = data.join("state/full-sync");
        io::private_dir(&data.join("state"))?;
        io::private_dir(&state_root)?;
        super::super::formal_full_sync::FormalFullSyncSession::new(
            full_sync_config.clone(),
            keypair.private_key_hex,
            &state_root,
            &intent.target,
        )
        .map(|session| PollSession::Legacy {
            session,
            restored_world: None,
        })
    } else {
        super::super::formal_full_sync::operation_worker::BudgetedDriver::new(
            config.clone(),
            intent.target.clone(),
            Arc::clone(&ownership),
            Arc::clone(&stop_polling),
        )
        .map(PollSession::Budgeted)
    };
    let mut session = match initialized {
        Ok(session) => session,
        Err(error) => {
            intent.target.state = "failed".into();
            write_instance(data, &intent.target)?;
            save_action(data, intent)?;
            let _ = fs::remove_file(&socket_path);
            return Err(error);
        }
    };
    let live_observation = match &session {
        PollSession::Budgeted(driver) => Some(Arc::clone(&driver.observation)),
        PollSession::Legacy { .. } => None,
    };
    intent.target.runtime_pid = None;
    intent.target.state = "running".into();
    write_instance(data, &intent.target)?;
    save_action(data, intent)?;
    let status_path = data.join(super::super::formal_full_sync::STATUS_FILE);
    io::write(&status_path, &session.status(config)?)?;

    let worker_stop = Arc::clone(&stop_polling);
    let worker_status_gate = Arc::clone(&status_gate);
    let poll_interval = session.poll_interval();
    // Establish cleanup before spawning: any later initialization failure must
    // stop the worker as well as retain its ownership until it actually exits.
    let stop_guard = WorkerStop {
        flag: Arc::clone(&stop_polling),
        status_gate,
    };
    let (worker_result_tx, worker_result_rx) = mpsc::channel();
    let worker_config = config.clone();
    let worker_status_path = status_path.clone();
    let worker = spawn_owned_worker(Arc::clone(&ownership), move || {
        let result = (|| {
            let mut next_poll = Instant::now();
            while !worker_stop.load(Ordering::SeqCst) {
                if Instant::now() >= next_poll {
                    session.begin_poll();
                    {
                        let _status_guard = worker_status_gate
                            .lock()
                            .map_err(|error| error.to_string())?;
                        if worker_stop.load(Ordering::SeqCst) {
                            break;
                        }
                        io::write(&worker_status_path, &session.status(&worker_config)?)?;
                    }
                    session.poll()?;
                    let _status_guard = worker_status_gate
                        .lock()
                        .map_err(|error| error.to_string())?;
                    if worker_stop.load(Ordering::SeqCst) {
                        break;
                    }
                    io::write(&worker_status_path, &session.status(&worker_config)?)?;
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
    let signal_flag = Arc::clone(&signal_stop);
    ctrlc::set_handler(move || {
        signal_flag.store(true, Ordering::SeqCst);
    })
    .map_err(|error| error.to_string())?;
    loop {
        if signal_stop.load(Ordering::SeqCst) {
            stop_guard.stop();
            finish_stop(data, intent, &socket_path)?;
            drop(ownership);
            drop(worker);
            return Ok(());
        }
        let worker_result = match worker_result_rx.try_recv() {
            Ok(result) => Some(result),
            Err(mpsc::TryRecvError::Disconnected) => Some(Err(
                "formal full-state sync worker exited without reporting its result".into(),
            )),
            Err(mpsc::TryRecvError::Empty) => None,
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
                "formal full-state sync worker exited without a stop request".into()
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
                        if let Some(live) = &live_observation {
                            let status = super::super::formal_full_sync::operation_status::running(
                                config,
                                &*live.lock().map_err(|e| e.to_string())?,
                            )?;
                            io::write(&status_path, &status)?;
                        }
                        let _ = write_line(
                            &mut socket,
                            &ControlResponse {
                                instance: intent.target.clone(),
                            },
                        );
                    } else if exact && request.operation == "stop" {
                        stop_guard.stop();
                        finish_stop(data, intent, &socket_path)?;
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

fn finish_stop(data: &Path, intent: &mut ActionRecord, socket_path: &Path) -> Result<(), String> {
    intent.target.state = "draining".into();
    write_instance(data, &intent.target)?;
    save_action(data, intent)?;
    intent.target.state = "stopped".into();
    intent.target.exit_code = Some(0);
    write_instance(data, &intent.target)?;
    save_action(data, intent)?;
    fs::remove_file(socket_path).map_err(|error| error.to_string())?;
    let status_path = data.join(super::super::formal_full_sync::STATUS_FILE);
    if let Ok(mut status) = io::read::<serde_json::Value>(&status_path)
        && status["incarnation"] == intent.target.incarnation
        && status["generation"] == intent.target.generation
        && status["config_digest"] == intent.target.config_digest
    {
        status["ready"] = false.into();
        status["can_propose"] = false.into();
        status["world_loaded"] = false.into();
        status["state"] = "stopped".into();
        status["error"] = serde_json::Value::Null;
        if let Some(operation) = status
            .get_mut("operation")
            .filter(|value| value.is_object())
        {
            operation["elapsed_ms"] = serde_json::Value::Null;
            operation["remaining_ms"] = serde_json::Value::Null;
            operation["attempt_state"] = "interrupted".into();
        }
        let _ = io::write(&status_path, &status);
    }
    Ok(())
}
