#[cfg(not(target_arch = "wasm32"))]
use std::sync::{Once, OnceLock};
#[cfg(not(target_arch = "wasm32"))]
use std::time::{SystemTime, UNIX_EPOCH};

#[cfg(not(target_arch = "wasm32"))]
use tracing::Level;
#[cfg(not(target_arch = "wasm32"))]
use tracing_subscriber::EnvFilter;

#[cfg(not(target_arch = "wasm32"))]
static TRACING_INIT: Once = Once::new();
#[cfg(not(target_arch = "wasm32"))]
static TRACE_SESSION_ID: OnceLock<String> = OnceLock::new();

#[cfg(not(target_arch = "wasm32"))]
pub const TRACE_SESSION_ID_ENV: &str = "OASIS7_TRACE_SESSION_ID";

#[cfg(not(target_arch = "wasm32"))]
pub fn init_tracing(service_name: &str) {
    TRACING_INIT.call_once(|| {
        if tracing::dispatcher::has_been_set() {
            return;
        }
        let filter = EnvFilter::try_from_default_env()
            .unwrap_or_else(|_| EnvFilter::new(format!("{service_name}=info,oasis7=info,warn")));
        let _ = tracing_subscriber::fmt()
            .with_env_filter(filter)
            .with_writer(std::io::stderr)
            .with_target(true)
            .with_thread_names(true)
            .compact()
            .try_init();
    });
}

#[cfg(not(target_arch = "wasm32"))]
pub fn emit_stderr_or_event(level: Level, stderr_message: &str, event_message: &str) {
    if tracing::dispatcher::has_been_set() {
        match level {
            Level::ERROR => tracing::error!(message = %stderr_message, "{event_message}"),
            Level::WARN => tracing::warn!(message = %stderr_message, "{event_message}"),
            Level::INFO => tracing::info!(message = %stderr_message, "{event_message}"),
            Level::DEBUG => tracing::debug!(message = %stderr_message, "{event_message}"),
            Level::TRACE => tracing::trace!(message = %stderr_message, "{event_message}"),
        }
    } else {
        eprintln!("{stderr_message}");
    }
}

#[cfg(not(target_arch = "wasm32"))]
pub fn resolve_trace_session_id(process_label: &str) -> String {
    TRACE_SESSION_ID
        .get_or_init(|| {
            std::env::var(TRACE_SESSION_ID_ENV)
                .ok()
                .map(|value| value.trim().to_string())
                .filter(|value| !value.is_empty())
                .unwrap_or_else(|| {
                    let now_ms = SystemTime::now()
                        .duration_since(UNIX_EPOCH)
                        .unwrap_or_default()
                        .as_millis();
                    format!("{process_label}-{}-{now_ms}", std::process::id())
                })
        })
        .clone()
}

#[cfg(test)]
mod tests {
    use super::{emit_stderr_or_event, init_tracing, resolve_trace_session_id};
    use tracing::Level;

    #[test]
    fn init_tracing_is_idempotent() {
        init_tracing("oasis7_test");
        init_tracing("oasis7_test");
    }

    #[test]
    fn emit_stderr_or_event_allows_tracing_path_after_init() {
        init_tracing("oasis7_test");
        emit_stderr_or_event(Level::WARN, "stderr warning", "structured warning");
    }

    #[test]
    fn resolve_trace_session_id_is_stable_within_process() {
        let first = resolve_trace_session_id("oasis7_test");
        let second = resolve_trace_session_id("oasis7_test");
        assert_eq!(first, second);
        assert!(!first.trim().is_empty());
    }
}
