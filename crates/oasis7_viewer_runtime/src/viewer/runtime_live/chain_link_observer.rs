//! Read-only legacy chain observer loading, cancellation and source epochs.
use super::*;

#[derive(Default)]
pub(in super::super) struct ObserverLoader {
    running: bool,
    cancellation: Option<Arc<std::sync::atomic::AtomicBool>>,
    pub(super) source_epoch: u64,
    source: Option<(Option<String>, Option<PathBuf>)>,
    latest_pending: Option<ViewerRuntimeLiveServerConfig>,
    ready: Option<Result<PreparedChainLinkedRuntimeUpdate, ViewerRuntimeLiveServerError>>,
}

impl Drop for ViewerRuntimeLiveServer {
    fn drop(&mut self) {
        if let Ok(mut loader) = self.chain_observer_loader.lock() {
            loader.source_epoch = loader.source_epoch.wrapping_add(1);
            loader.latest_pending = None;
            loader.ready = None;
            if let Some(cancel) = &loader.cancellation {
                cancel.store(true, std::sync::atomic::Ordering::Relaxed);
            }
        }
    }
}

pub(super) fn request_observer_update(
    loader: &Arc<Mutex<ObserverLoader>>,
    config: &ViewerRuntimeLiveServerConfig,
) -> Result<Option<PreparedChainLinkedRuntimeUpdate>, ViewerRuntimeLiveServerError> {
    let mut state = loader
        .lock()
        .map_err(|_| ViewerRuntimeLiveServerError::Init("observer loader poisoned".into()))?;
    let source = (
        config.chain_status_bind.clone(),
        config.chain_execution_world_dir.clone(),
    );
    if state.source.as_ref() != Some(&source) {
        if let Some(cancel) = &state.cancellation {
            cancel.store(true, std::sync::atomic::Ordering::Relaxed);
        }
        state.source_epoch = state.source_epoch.wrapping_add(1);
        state.source = Some(source);
        state.ready = None;
    }
    // A status hint only replaces the pending target. It never invalidates a
    // captured, still-valid generation from the same source.
    state.latest_pending = Some(config.clone());
    let ready = state.ready.take();
    if !state.running {
        let pending = state.latest_pending.take().expect("queued target");
        let epoch = state.source_epoch;
        state.running = true;
        let cancellation = Arc::new(std::sync::atomic::AtomicBool::new(false));
        state.cancellation = Some(cancellation.clone());
        let loader_for_wait = loader.clone();
        let loader = loader.clone();
        let worker = std::thread::spawn(move || {
            let mut result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                prepare_chain_linked_runtime_update(&pending, cancellation)
            }))
            .unwrap_or_else(|_| {
                Err(ViewerRuntimeLiveServerError::Init(
                    "observer worker failed".into(),
                ))
            });
            if let Ok(candidate) = &mut result {
                candidate.source_epoch = epoch;
            }
            if let Ok(mut state) = loader.lock() {
                state.running = false;
                if state.source_epoch == epoch {
                    state.ready = Some(result);
                }
            }
        });
        // The owning caller waits outside the Viewer lock in the minimized
        // entrypoints. Concurrent callers only update latest_pending.
        drop(state);
        if ready.is_some() {
            return ready.transpose();
        }
        worker
            .join()
            .map_err(|_| ViewerRuntimeLiveServerError::Init("observer worker failed".into()))?;
        let mut state = loader_for_wait
            .lock()
            .map_err(|_| ViewerRuntimeLiveServerError::Init("observer loader poisoned".into()))?;
        return ready.or_else(|| state.ready.take()).transpose();
    }
    ready.transpose()
}

fn prepare_chain_linked_runtime_update(
    config: &ViewerRuntimeLiveServerConfig,
    cancellation: Arc<std::sync::atomic::AtomicBool>,
) -> Result<PreparedChainLinkedRuntimeUpdate, ViewerRuntimeLiveServerError> {
    let bind = config.chain_status_bind.as_deref().ok_or_else(|| {
        ViewerRuntimeLiveServerError::Init("chain status is not configured".into())
    })?;
    let root = config.chain_execution_world_dir.as_deref().ok_or_else(|| {
        ViewerRuntimeLiveServerError::Init(
            "--chain-execution-world-dir is required for a chain observer".into(),
        )
    })?;
    let chain_status = fetch_chain_status_snapshot(bind)?;
    let world = RuntimeWorld::load_observer_with_control(
        root,
        crate::runtime::ObserverReadLimits::default(),
        crate::runtime::ObserverLoadControl::from_shared_cancellation(cancellation),
    )
    .map_err(|error| {
        ViewerRuntimeLiveServerError::Init(format!("observer load rejected: {error:?}"))
    })?
    .with_release_security_policy(chain_status.release_security_policy);
    let sync_watermark =
        chain_linked_runtime_sync_watermark(chain_status.consensus.committed_height, &world);
    Ok(PreparedChainLinkedRuntimeUpdate {
        committed_height: sync_watermark,
        source_epoch: 0,
        source: (
            config.chain_status_bind.clone(),
            config.chain_execution_world_dir.clone(),
        ),
        world,
        verified_view: None,
        service_events: None,
        intent_results: Vec::new(),
    })
}

fn chain_linked_runtime_sync_watermark(committed_height: u64, _world: &RuntimeWorld) -> u64 {
    committed_height
}
