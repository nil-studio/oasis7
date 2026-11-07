//! Test-only ownership of the canonical service pump and HTTP executor.
use super::ViewerRuntimeLiveServer;
use std::sync::{Arc, Mutex, mpsc::Sender};
use std::thread::JoinHandle;

pub(super) struct Workers {
    pub(super) pump: JoinHandle<()>,
    pub(super) executor: JoinHandle<()>,
}

#[cfg(test)]
pub(super) struct IterationGate {
    pub(super) entered: Sender<()>,
    pub(super) release: std::sync::mpsc::Receiver<()>,
}

#[cfg(test)]
impl super::agent_service_io_diagnostics::IoDiagnostics {
    pub(super) fn hold_iteration(&self) {
        let gate = self.iteration_gate.lock().unwrap().take();
        if let Some(gate) = gate {
            gate.entered.send(()).unwrap();
            gate.release
                .recv_timeout(std::time::Duration::from_secs(2))
                .unwrap();
        }
    }
}

impl ViewerRuntimeLiveServer {
    /// Close and join all Viewer sessions before consuming the last external
    /// owner here. The caller must bound this cleanup with its existing watchdog.
    /// This joins canonical service workers, not unrelated provider actors.
    pub fn test_join_agent_service_workers(
        shared: Arc<Mutex<Self>>,
        join_started: Option<Sender<()>>,
    ) -> Result<(), &'static str> {
        let mut failed = false;
        let diagnostics = {
            let server = shared.lock().unwrap_or_else(|poison| {
                failed = true;
                poison.into_inner()
            });
            server.agent_service_io_diagnostics.clone()
        };
        // A pump may still own this iteration's temporary Arc. Its explicit
        // drop at the loop tail makes the next Weak upgrade fail. No registry
        // or executor owns a server Arc, so no new dispatch can follow its join.
        drop(shared);
        let workers = {
            let mut owned = diagnostics.workers.lock().unwrap_or_else(|poison| {
                failed = true;
                poison.into_inner()
            });
            std::mem::take(&mut *owned)
        };
        if let Some(join_started) = join_started {
            failed |= join_started.send(()).is_err();
        }
        let mut executors = Vec::with_capacity(workers.len());
        for Workers { pump, executor } in workers {
            // Never return early on a panic and detach the remaining handles.
            failed |= pump.join().is_err();
            executors.push(executor);
        }
        // Pump destruction drops the jobs sender and result receiver. An
        // executor finishes the current transport before seeing disconnection;
        // a pending result send cannot keep it alive after the receiver drops.
        for executor in executors {
            failed |= executor.join().is_err();
        }
        if failed {
            Err("canonical service worker cleanup failed")
        } else {
            Ok(())
        }
    }
}

#[cfg(test)]
#[path = "agent_service_worker_cleanup_tests.rs"]
mod tests;
