//! Local supervisor for the formal full-state read-only sync profile.
//!
//! The network poll runs separately from the authenticated control socket so
//! stop/query remain responsive while a bounded catch-up or CAS transfer is
//! in progress. Readiness is durable only for the exact local incarnation and
//! only after the verified World has been restored and retained in memory.

use super::*;
use std::sync::mpsc;

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

pub(super) fn supervise(
    config: &NodeConfig,
    intent: &mut ActionRecord,
    ownership: File,
    full_sync_config: &super::super::formal_full_sync::FormalFullSyncConfigV1,
) -> Result<(), String> {
    let data = &config.identity.data_dir;
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

    let (keypair, _) =
        super::super::super::node_keypair_config::read_existing_node_keypair_in_secure_config_dir(
            &data.join("keys"),
        )?;
    let state_root = data.join("state/full-sync");
    io::private_dir(&data.join("state"))?;
    io::private_dir(&state_root)?;
    let mut session = match super::super::formal_full_sync::FormalFullSyncSession::new(
        full_sync_config.clone(),
        keypair.private_key_hex,
        &state_root,
        &intent.target,
    ) {
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
    let status_path = data.join(super::super::formal_full_sync::STATUS_FILE);
    io::write(&status_path, session.status())?;

    let stop_polling = Arc::new(AtomicBool::new(false));
    let status_gate = Arc::new(Mutex::new(()));
    let worker_stop = Arc::clone(&stop_polling);
    let worker_status_gate = Arc::clone(&status_gate);
    let poll_interval = session.poll_interval();
    let (worker_result_tx, worker_result_rx) = mpsc::channel();
    let worker = std::thread::Builder::new()
        .name("formal-full-state-sync".into())
        .spawn(move || {
            let result = (|| {
                let mut next_poll = Instant::now();
                let mut restored_world: Option<oasis7_runtime::World> = None;
                while !worker_stop.load(Ordering::SeqCst) {
                    if Instant::now() >= next_poll {
                        drop(restored_world.take());
                        session.begin_poll();
                        {
                            let _status_guard = worker_status_gate
                                .lock()
                                .map_err(|error| error.to_string())?;
                            if worker_stop.load(Ordering::SeqCst) {
                                break;
                            }
                            io::write(&status_path, session.status())?;
                        }
                        match session.sync_latest() {
                            Ok(restored) => restored_world = Some(restored.world),
                            Err(_) => restored_world = None,
                        }
                        session.set_world_loaded(restored_world.is_some());
                        if session.status().ready
                            && (!session.status().state_root_verified || restored_world.is_none())
                        {
                            return Err(
                                "full-sync readiness lacked a retained verified World".into()
                            );
                        }
                        let _status_guard = worker_status_gate
                            .lock()
                            .map_err(|error| error.to_string())?;
                        if worker_stop.load(Ordering::SeqCst) {
                            break;
                        }
                        io::write(&status_path, session.status())?;
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
    let stop_guard = WorkerStop {
        flag: Arc::clone(&stop_polling),
        status_gate,
    };
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
    if let Ok(mut status) =
        io::read::<super::super::formal_full_sync::FormalFullSyncStatusV1>(&status_path)
        && status.incarnation == intent.target.incarnation
        && status.generation == intent.target.generation
        && status.config_digest == intent.target.config_digest
    {
        status.ready = false;
        status.can_propose = false;
        status.world_loaded = false;
        status.state = "stopped".into();
        status.error = None;
        let _ = io::write(&status_path, &status);
    }
    Ok(())
}
