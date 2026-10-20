use std::env;
use std::fs;
use std::path::Path;

use oasis7_wasm_executor::{WasmExecutorMetricsSnapshot, snapshot_global_wasm_executor_metrics};
use oasis7_wasm_router::{WasmRouterMetricsSnapshot, snapshot_global_wasm_router_metrics};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize)]
pub(super) struct ChainWasmStatus {
    pub(super) metrics_available: bool,
    pub(super) observed_since_unix_ms: Option<i64>,
    pub(super) degraded_reason: Option<String>,
    pub(super) build: ChainWasmBuildStatus,
    pub(super) executor: WasmExecutorMetricsSnapshot,
    pub(super) router: WasmRouterMetricsSnapshot,
}

#[derive(Debug, Clone, Serialize)]
pub(super) struct ChainWasmBuildStatus {
    pub(super) metrics_available: bool,
    pub(super) observed_since_unix_ms: Option<i64>,
    pub(super) degraded_reason: Option<String>,
    pub(super) total_build_wall_ms: Option<u64>,
    pub(super) cargo_build_ms: Option<u64>,
    pub(super) canonicalize_ms: Option<u64>,
    pub(super) hash_ms: Option<u64>,
    pub(super) receipt_write_ms: Option<u64>,
    pub(super) metadata_write_ms: Option<u64>,
    pub(super) wasm_size_bytes: Option<u64>,
}

#[derive(Debug, Deserialize)]
struct BuildTimingPayload {
    total_build_wall_ms: u64,
    cargo_build_ms: u64,
    canonicalize_ms: u64,
    hash_ms: u64,
    receipt_write_ms: u64,
    metadata_write_ms: u64,
}

#[derive(Debug, Deserialize)]
struct BuildMetricsPayload {
    recorded_at_unix_ms: i64,
    wasm_size_bytes: u64,
    build_timing: BuildTimingPayload,
}

pub(super) fn build_chain_wasm_status() -> ChainWasmStatus {
    let build = build_chain_wasm_build_status();
    let executor = snapshot_global_wasm_executor_metrics();
    let router = snapshot_global_wasm_router_metrics();
    compose_chain_wasm_status(build, executor, router)
}

fn compose_chain_wasm_status(
    build: ChainWasmBuildStatus,
    executor: WasmExecutorMetricsSnapshot,
    router: WasmRouterMetricsSnapshot,
) -> ChainWasmStatus {
    let observed_since_unix_ms = [
        build
            .metrics_available
            .then_some(build.observed_since_unix_ms)
            .flatten(),
        executor
            .metrics_available
            .then_some(executor.observed_since_unix_ms),
        router
            .metrics_available
            .then_some(router.observed_since_unix_ms),
    ]
    .into_iter()
    .flatten()
    .min();
    let metrics_available =
        build.metrics_available || executor.metrics_available || router.metrics_available;
    let reasons = [
        build.degraded_reason.clone(),
        executor.degraded_reason.clone(),
        router.degraded_reason.clone(),
    ]
    .into_iter()
    .flatten()
    .collect::<Vec<_>>();
    let degraded_reason = if reasons.is_empty() {
        (!metrics_available).then(|| "wasm metrics unavailable".to_string())
    } else {
        Some(reasons.join("; "))
    };
    ChainWasmStatus {
        metrics_available,
        observed_since_unix_ms,
        degraded_reason,
        build,
        executor,
        router,
    }
}

fn build_chain_wasm_build_status() -> ChainWasmBuildStatus {
    if let Some(path) = env_path("OASIS7_WASM_BUILD_METADATA_PATH") {
        return load_build_metrics_payload(path.as_path());
    }
    if let Some(path) = env_path("OASIS7_WASM_BUILD_RECEIPT_PATH") {
        return load_build_metrics_payload(path.as_path());
    }
    ChainWasmBuildStatus {
        metrics_available: false,
        observed_since_unix_ms: None,
        degraded_reason: Some("build metrics path not configured".to_string()),
        total_build_wall_ms: None,
        cargo_build_ms: None,
        canonicalize_ms: None,
        hash_ms: None,
        receipt_write_ms: None,
        metadata_write_ms: None,
        wasm_size_bytes: None,
    }
}

fn load_build_metrics_payload(path: &Path) -> ChainWasmBuildStatus {
    let bytes = match fs::read(path) {
        Ok(bytes) => bytes,
        Err(err) => {
            return ChainWasmBuildStatus {
                metrics_available: false,
                observed_since_unix_ms: None,
                degraded_reason: Some(format!(
                    "read build metrics {} failed: {err}",
                    path.display()
                )),
                total_build_wall_ms: None,
                cargo_build_ms: None,
                canonicalize_ms: None,
                hash_ms: None,
                receipt_write_ms: None,
                metadata_write_ms: None,
                wasm_size_bytes: None,
            };
        }
    };
    let payload: BuildMetricsPayload = match serde_json::from_slice(&bytes) {
        Ok(payload) => payload,
        Err(err) => {
            return ChainWasmBuildStatus {
                metrics_available: false,
                observed_since_unix_ms: None,
                degraded_reason: Some(format!(
                    "parse build metrics {} failed: {err}",
                    path.display()
                )),
                total_build_wall_ms: None,
                cargo_build_ms: None,
                canonicalize_ms: None,
                hash_ms: None,
                receipt_write_ms: None,
                metadata_write_ms: None,
                wasm_size_bytes: None,
            };
        }
    };
    ChainWasmBuildStatus {
        metrics_available: true,
        observed_since_unix_ms: Some(payload.recorded_at_unix_ms),
        degraded_reason: None,
        total_build_wall_ms: Some(payload.build_timing.total_build_wall_ms),
        cargo_build_ms: Some(payload.build_timing.cargo_build_ms),
        canonicalize_ms: Some(payload.build_timing.canonicalize_ms),
        hash_ms: Some(payload.build_timing.hash_ms),
        receipt_write_ms: Some(payload.build_timing.receipt_write_ms),
        metadata_write_ms: Some(payload.build_timing.metadata_write_ms),
        wasm_size_bytes: Some(payload.wasm_size_bytes),
    }
}

fn env_path(key: &str) -> Option<std::path::PathBuf> {
    env::var_os(key)
        .filter(|value| !value.is_empty())
        .map(std::path::PathBuf::from)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::process;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn compose_chain_wasm_status_surfaces_partial_build_degradation() {
        let status = compose_chain_wasm_status(
            ChainWasmBuildStatus {
                metrics_available: false,
                observed_since_unix_ms: None,
                degraded_reason: Some("build metrics path not configured".to_string()),
                total_build_wall_ms: None,
                cargo_build_ms: None,
                canonicalize_ms: None,
                hash_ms: None,
                receipt_write_ms: None,
                metadata_write_ms: None,
                wasm_size_bytes: None,
            },
            WasmExecutorMetricsSnapshot::empty(),
            WasmRouterMetricsSnapshot::empty(),
        );

        assert!(status.metrics_available);
        assert_eq!(
            status.degraded_reason.as_deref(),
            Some("build metrics path not configured")
        );
        assert!(!status.build.metrics_available);
    }

    #[test]
    fn load_build_metrics_payload_reads_build_timing_snapshot() {
        let path = temp_metrics_path("valid");
        fs::write(
            &path,
            r#"{
  "recorded_at_unix_ms": 1700000000000,
  "wasm_size_bytes": 4096,
  "build_timing": {
    "total_build_wall_ms": 120,
    "cargo_build_ms": 80,
    "canonicalize_ms": 10,
    "hash_ms": 5,
    "receipt_write_ms": 3,
    "metadata_write_ms": 2
  }
}"#,
        )
        .expect("write valid build metrics payload");

        let status = load_build_metrics_payload(path.as_path());
        let _ = fs::remove_file(&path);

        assert!(status.metrics_available);
        assert_eq!(status.observed_since_unix_ms, Some(1_700_000_000_000));
        assert_eq!(status.total_build_wall_ms, Some(120));
        assert_eq!(status.cargo_build_ms, Some(80));
        assert_eq!(status.canonicalize_ms, Some(10));
        assert_eq!(status.hash_ms, Some(5));
        assert_eq!(status.receipt_write_ms, Some(3));
        assert_eq!(status.metadata_write_ms, Some(2));
        assert_eq!(status.wasm_size_bytes, Some(4096));
    }

    #[test]
    fn load_build_metrics_payload_reports_parse_failure() {
        let path = temp_metrics_path("invalid");
        fs::write(&path, "{not-json").expect("write invalid build metrics payload");

        let status = load_build_metrics_payload(path.as_path());
        let _ = fs::remove_file(&path);

        assert!(!status.metrics_available);
        assert!(
            status
                .degraded_reason
                .as_deref()
                .is_some_and(|reason| reason.contains("parse build metrics"))
        );
    }

    fn temp_metrics_path(label: &str) -> std::path::PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|duration| duration.as_nanos())
            .unwrap_or(0);
        std::env::temp_dir().join(format!(
            "oasis7_wasm_status_{label}_{}_{}.json",
            process::id(),
            nonce
        ))
    }
}
