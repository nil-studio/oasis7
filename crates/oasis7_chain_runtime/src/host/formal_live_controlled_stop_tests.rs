//! Artificial scheduling proves stop mechanisms, never a historical CI cause.
use super::*;
use oasis7_node::{NodeExecutionCommitContext, NodeExecutionCommitResult, NodeExecutionHook};
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    mpsc,
};

struct ControlledTick {
    entered: Option<mpsc::SyncSender<()>>,
    release: mpsc::Receiver<()>,
    calls: Arc<AtomicUsize>,
}
impl NodeExecutionHook for ControlledTick {
    fn on_commit(
        &mut self,
        context: NodeExecutionCommitContext,
    ) -> Result<NodeExecutionCommitResult, String> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        if let Some(entered) = self.entered.take() {
            entered.try_send(()).map_err(|error| error.to_string())?;
            self.release
                .recv_timeout(Duration::from_secs(2))
                .map_err(|error| error.to_string())?;
        }
        Ok(NodeExecutionCommitResult {
            execution_height: context.height,
            execution_block_hash: format!("controlled-stop-block-{}", context.height),
            execution_state_root: format!("controlled-stop-state-{}", context.height),
        })
    }
}

/// Exists before start and travels with the runtime into the scoped stop thread.
/// Every error/panic releases the tick before joining stop, while fixture peers live.
struct OwnedRuntime {
    runtime: Option<FormalLiveRuntime>,
    release: Option<mpsc::Sender<()>>,
    cleanup_errors: Arc<Mutex<Vec<String>>>,
}
impl OwnedRuntime {
    fn release_tick(&mut self) {
        if let Some(release) = self.release.take() {
            let _ = release.send(());
        }
    }
}
impl Drop for OwnedRuntime {
    fn drop(&mut self) {
        self.release_tick();
        if let Some(runtime) = &mut self.runtime
            && let Err(error) = runtime.stop(Duration::from_secs(2))
        {
            self.cleanup_errors.lock().unwrap().push(error);
        }
    }
}

fn scenario(
    mut runtime: FormalLiveRuntime,
    hold_admission: bool,
    cleanup_errors: Arc<Mutex<Vec<String>>>,
) -> Result<FormalLiveRuntime, String> {
    let (entered_send, entered_receive) = mpsc::sync_channel(1);
    let (release_send, release_receive) = mpsc::channel();
    let (entry_send, entry_receive) = mpsc::sync_channel(1);
    let calls = Arc::new(AtomicUsize::new(0));
    let config = local_single_authority_node_config("controlled-stop", "controlled-world")?
        .with_tick_interval(Duration::from_millis(5))
        .map_err(|error| format!("controlled config: {error:?}"))?;
    runtime.runtime = Arc::new(Mutex::new(NodeRuntime::new(config).with_execution_hook(
        ControlledTick {
            entered: Some(entered_send),
            release: release_receive,
            calls: calls.clone(),
        },
    )));
    let node = runtime.runtime.clone();
    let admission = runtime.admission_gate.clone();
    let state = runtime.state.clone();
    let mut owned = OwnedRuntime {
        runtime: Some(runtime),
        release: Some(release_send),
        cleanup_errors,
    };
    owned.runtime.as_mut().unwrap().start()?;
    entered_receive
        .recv_timeout(Duration::from_secs(1))
        .map_err(|error| error.to_string())?;
    let runtime = owned.runtime.as_mut().unwrap();
    if runtime
        .stop(Duration::ZERO)
        .as_ref()
        .map_err(String::as_str)
        != Err("formal Node stop is still pending; runtime remains owned")
        || runtime.stop_worker.is_some()
        || !Arc::ptr_eq(&node, &runtime.runtime)
    {
        return Err("controlled active stop(0) did not retain its pre-worker owner".into());
    }
    runtime.stop_entry_signal = Some(entry_send);
    let timeout = Duration::from_millis(20);
    let (mut owned, result, deadline, returned, returned_while_held) =
        std::thread::scope(|scope| {
            let (start_send, start_receive) = mpsc::sync_channel(1);
            let (done_send, done_receive) = mpsc::sync_channel(1);
            // Spawn before acquiring the gate, so OS thread-creation failure cannot
            // run OwnedRuntime cleanup while this same thread holds admission.
            let stop = scope.spawn(move || {
                let result = start_receive
                    .recv_timeout(Duration::from_secs(1))
                    .map_err(|error| error.to_string())
                    .and_then(|()| owned.runtime.as_mut().unwrap().stop(timeout));
                let returned = Instant::now();
                let _ = done_send.try_send(returned);
                (owned, result, returned)
            });
            // The guard is inside the scope closure: unwind drops it before scope joins.
            let guard = hold_admission.then(|| admission.lock().unwrap());
            start_send.try_send(()).map_err(|error| error.to_string())?;
            let deadline = entry_receive.recv_timeout(Duration::from_secs(1));
            let returned_while_held = !hold_admission
                || done_receive
                    .recv_timeout(Duration::from_millis(200))
                    .is_ok();
            drop(guard);
            let (owned, result, returned) =
                stop.join().map_err(|_| "controlled stop thread panicked")?;
            Ok::<_, String>((
                owned,
                result,
                deadline.map_err(|error| error.to_string())?,
                returned,
                returned_while_held,
            ))
        })?;
    let runtime = owned.runtime.as_mut().unwrap();
    let kind = if hold_admission { "admission" } else { "join" };
    if !returned_while_held {
        return Err("controlled stop failed to return while admission remained held".into());
    }
    eprintln!(
        "formal_controlled_stop kind={kind} timeout_ms=20 entry_to_return_us={} deadline_overrun_us={}",
        returned.duration_since(deadline - timeout).as_micros(),
        returned.saturating_duration_since(deadline).as_micros()
    );
    if result.as_ref().map_err(String::as_str)
        != Err("formal Node stop is still pending; runtime remains owned")
    {
        return Err(format!(
            "controlled {kind} expected retained Pending, got {result:?}"
        ));
    }
    if returned < deadline {
        return Err("controlled deadline consumption was not reached".into());
    }
    if !Arc::ptr_eq(&node, &runtime.runtime)
        || !runtime.started
        || state.ready.load(Ordering::Acquire)
        || !state.node_running.load(Ordering::Acquire)
        || !state.stopping.load(Ordering::Acquire)
    {
        return Err("controlled Pending lost owner identity/state".into());
    }
    if hold_admission && runtime.stop_worker.is_some() {
        return Err("expired admission spawned a late stop worker".into());
    }
    let first_worker = runtime
        .stop_worker
        .as_ref()
        .map(|pending| pending.worker.thread().id());
    let second = runtime.stop(Duration::from_millis(5));
    if second.as_ref().map_err(String::as_str)
        != Err("formal Node stop is still pending; runtime remains owned")
    {
        return Err("controlled second wait changed worker or error semantics".into());
    }
    let worker = runtime
        .stop_worker
        .as_ref()
        .ok_or("controlled retry did not create worker")?
        .worker
        .thread()
        .id();
    if first_worker.is_some_and(|first| first != worker) {
        return Err("controlled retry replaced the original worker".into());
    }
    if runtime
        .stop(Duration::from_millis(5))
        .as_ref()
        .map_err(String::as_str)
        != Err("formal Node stop is still pending; runtime remains owned")
        || runtime
            .stop_worker
            .as_ref()
            .ok_or("controlled retry lost worker")?
            .worker
            .thread()
            .id()
            != worker
    {
        return Err("controlled repeated wait replaced worker".into());
    }
    owned.release_tick();
    let runtime = owned.runtime.as_mut().unwrap();
    runtime.stop(Duration::from_secs(1))?;
    if runtime.started
        || runtime.stop_worker.is_some()
        || state.node_running.load(Ordering::Acquire)
        || state.stopping.load(Ordering::Acquire)
        || calls.load(Ordering::SeqCst) != 1
    {
        return Err("controlled completion did not consume the same worker exactly once".into());
    }
    runtime.stop(Duration::ZERO)?;
    if runtime.state.stopping.load(Ordering::Acquire) {
        return Err("already stopped stop(0) left stopping set".into());
    }
    eprintln!(
        "formal_controlled_stop kind={kind} same_owner=true same_worker=true completed=true hook_calls=1"
    );
    Ok(owned.runtime.take().unwrap())
}

pub(super) fn exercise(mut runtime: FormalLiveRuntime) -> Result<(), String> {
    runtime.stop(Duration::ZERO)?;
    if runtime.started || runtime.state.stopping.load(Ordering::Acquire) {
        return Err("unstarted stop(0) changed owner lifecycle".into());
    }
    let errors = Arc::new(Mutex::new(Vec::new()));
    let result = scenario(runtime, true, errors.clone())
        .and_then(|runtime| scenario(runtime, false, errors.clone()));
    // Normal scenarios have already stopped. On error the RAII owner has stopped
    // before this function returns and before protected fixture endpoints drop.
    if !errors.lock().unwrap().is_empty() {
        return Err(format!(
            "controlled cleanup failed: {:?}",
            errors.lock().unwrap()
        ));
    }
    result.map(drop)
}
