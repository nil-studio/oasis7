pub(super) fn new_current_thread_runtime(label: &str) -> tokio::runtime::Runtime {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect(label)
}

#[cfg(test)]
pub(crate) fn run_on_libp2p_test_runtime<T>(f: impl FnOnce() -> T) -> T {
    let runtime = new_current_thread_runtime("build libp2p test tokio runtime");
    runtime.block_on(async move { f() })
}

/// Wake the runtime even when discovery/republish timers are disabled so exposed
/// health views lose expired discovery evidence within one second of idle time.
pub(super) fn discovery_expiry_tick() -> impl futures::future::FusedFuture<Output = ()> + Unpin {
    use futures::FutureExt;
    Box::pin(tokio::time::sleep(std::time::Duration::from_secs(1))).fuse()
}

pub(super) fn log_connection_error(
    errors: &std::sync::Arc<std::sync::Mutex<Vec<String>>>,
    max_messages: usize,
    message: String,
) {
    super::push_bounded_clone(errors, message, max_messages, "lock errors");
}
