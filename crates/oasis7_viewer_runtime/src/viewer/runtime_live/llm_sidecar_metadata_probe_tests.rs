//! Actual HTTP metadata transport, actual cache clock, and delayed capsule delivery.
use super::*;
use std::{
    io::{Read, Write},
    net::{TcpListener, TcpStream},
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
    thread,
    time::{Duration, Instant},
};

const KEYS: &[&str] = &[
    VIEWER_AGENT_DECISION_SOURCE_ENV,
    VIEWER_AGENT_PROVIDER_BACKEND_ENV,
    VIEWER_AGENT_PROVIDER_CONTRACT_ENV,
    VIEWER_AGENT_PROVIDER_TRANSPORT_ENV,
    VIEWER_AGENT_PROVIDER_URL_ENV,
    VIEWER_AGENT_PROVIDER_PROFILE_ENV,
    VIEWER_AGENT_EXECUTION_LANE_ENV,
    VIEWER_AGENT_PROVIDER_MODE_ENV,
];
struct EnvRestore(Vec<(&'static str, Option<std::ffi::OsString>)>);
impl EnvRestore {
    fn capture() -> Self {
        let previous = KEYS
            .iter()
            .map(|key| (*key, std::env::var_os(key)))
            .collect();
        // SAFETY: Every test holds the canonical provider environment lock.
        for key in KEYS {
            unsafe {
                oasis7::env_mut::remove_var(key);
            }
        }
        Self(previous)
    }
}
impl Drop for EnvRestore {
    fn drop(&mut self) {
        for (key, value) in self.0.drain(..) {
            // SAFETY: The lock outlives this restore guard.
            unsafe {
                match value {
                    Some(value) => oasis7::env_mut::set_var(key, value),
                    None => oasis7::env_mut::remove_var(key),
                }
            }
        }
    }
}
fn configure(endpoint: &str) {
    // SAFETY: The calling test holds the canonical provider environment lock.
    unsafe {
        for (key, value) in [
            (VIEWER_AGENT_DECISION_SOURCE_ENV, "provider_backed"),
            (VIEWER_AGENT_PROVIDER_MODE_ENV, "provider_backed"),
            (VIEWER_AGENT_PROVIDER_BACKEND_ENV, "provider_local_mock"),
            (VIEWER_AGENT_PROVIDER_CONTRACT_ENV, "worldsim_provider_v1"),
            (VIEWER_AGENT_PROVIDER_TRANSPORT_ENV, "loopback_http"),
            (VIEWER_AGENT_PROVIDER_PROFILE_ENV, "oasis7_p0_low_freq_npc"),
            (VIEWER_AGENT_EXECUTION_LANE_ENV, "player_parity"),
            (VIEWER_AGENT_PROVIDER_URL_ENV, endpoint),
        ] {
            oasis7::env_mut::set_var(key, value);
        }
    }
}
struct HttpMetadata {
    endpoint: String,
    address: std::net::SocketAddr,
    info: Arc<AtomicUsize>,
    health: Arc<AtomicUsize>,
    fail: Arc<AtomicBool>,
    stop: Arc<AtomicBool>,
    worker: Option<thread::JoinHandle<()>>,
}
impl HttpMetadata {
    fn start(failing: bool) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let info = Arc::new(AtomicUsize::new(0));
        let health = Arc::new(AtomicUsize::new(0));
        let fail = Arc::new(AtomicBool::new(failing));
        let stop = Arc::new(AtomicBool::new(false));
        let (i, h, f, s) = (info.clone(), health.clone(), fail.clone(), stop.clone());
        let worker = thread::spawn(move || {
            while !s.load(Ordering::SeqCst) {
                let (mut socket, _) = listener.accept().unwrap();
                socket
                    .set_read_timeout(Some(Duration::from_secs(2)))
                    .unwrap();
                let mut bytes = Vec::new();
                let mut buffer = [0; 1024];
                while !bytes.windows(4).any(|v| v == b"\r\n\r\n") {
                    match socket.read(&mut buffer) {
                        Ok(0) | Err(_) => break,
                        Ok(n) => bytes.extend_from_slice(&buffer[..n]),
                    }
                    assert!(bytes.len() < 65536);
                }
                if bytes.is_empty() {
                    continue;
                }
                let first = std::str::from_utf8(&bytes).unwrap().lines().next().unwrap();
                let (status, body) = if first.starts_with("GET /v1/provider/info ") {
                    i.fetch_add(1, Ordering::SeqCst);
                    if f.load(Ordering::SeqCst) {
                        (503, r#"{"error":"actual metadata failure"}"#)
                    } else {
                        (
                            200,
                            r#"{"provider_id":"provider_local_bridge","name":"Provider Local Bridge","version":"0.1.0","protocol_version":"world-simulator-provider-loopback-http-v1","chain_resource_manifest_schema_version":"oasis7.world_resource_manifest.v1","chain_resource_delta_schema_version":"oasis7.world_resource_delta.v1","capabilities":["decision","feedback"],"supported_action_sets":["wait","wait_ticks","move_agent","speak_to_nearby","inspect_target","simple_interact"]}"#,
                        )
                    }
                } else if first.starts_with("GET /v1/provider/health ") {
                    h.fetch_add(1, Ordering::SeqCst);
                    (
                        200,
                        r#"{"ok":true,"status":"ready","uptime_ms":42,"last_error":null,"queue_depth":0}"#,
                    )
                } else {
                    panic!("metadata probe unexpectedly invoked another route: {first}")
                };
                write!(socket,"HTTP/1.1 {status} Response\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",body.len()).unwrap();
            }
        });
        Self {
            endpoint: format!("http://{address}"),
            address,
            info,
            health,
            fail,
            stop,
            worker: Some(worker),
        }
    }
    fn counts(&self) -> (usize, usize) {
        (
            self.info.load(Ordering::SeqCst),
            self.health.load(Ordering::SeqCst),
        )
    }
}
impl Drop for HttpMetadata {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        let _ = TcpStream::connect(self.address);
        if let Some(worker) = self.worker.take() {
            worker.join().unwrap();
        }
    }
}
fn expire(sidecar: &RuntimeLlmSidecar) {
    let checked = sidecar
        .provider_check_snapshot
        .as_ref()
        .unwrap()
        .checked_at_unix_ms;
    let began = Instant::now();
    while runtime_provider_check_now_unix_ms().saturating_sub(checked)
        < RUNTIME_PROVIDER_CHECK_CACHE_MS
    {
        assert!(
            began.elapsed() < Duration::from_secs(4),
            "actual metadata clock did not reach TTL"
        );
        thread::sleep(Duration::from_millis(10));
    }
}
fn run_probe(sidecar: &mut RuntimeLlmSidecar) {
    let job = sidecar
        .prepare_provider_metadata_probe()
        .unwrap()
        .expect("actual metadata job");
    let result = job.execute();
    sidecar.apply_provider_metadata_probe(result).unwrap();
}
#[test]
fn actual_http_metadata_ready_cache_expires_and_reprobes_after_ttl() {
    let _lock = crate::viewer::runtime_live::canonical_runtime_provider_env_lock()
        .lock()
        .unwrap_or_else(|p| p.into_inner());
    let _env = EnvRestore::capture();
    let http = HttpMetadata::start(false);
    configure(&http.endpoint);
    let mut sidecar = RuntimeLlmSidecar::new(ViewerLiveDecisionMode::Llm);
    run_probe(&mut sidecar);
    assert!(matches!(
        sidecar.fresh_provider_metadata_readiness().unwrap(),
        FreshProviderMetadataReadiness::Ready { .. }
    ));
    assert_eq!(http.counts(), (1, 1));
    assert!(sidecar.prepare_provider_metadata_probe().unwrap().is_none());
    assert_eq!(http.counts(), (1, 1));
    expire(&sidecar);
    assert!(matches!(
        sidecar.fresh_provider_metadata_readiness().unwrap(),
        FreshProviderMetadataReadiness::Stale
    ));
    run_probe(&mut sidecar);
    assert_eq!(http.counts(), (2, 2));
    assert!(matches!(
        sidecar.fresh_provider_metadata_readiness().unwrap(),
        FreshProviderMetadataReadiness::Ready { .. }
    ));
}
#[test]
fn actual_http_metadata_failure_stays_failed_until_ttl_then_recovers() {
    let _lock = crate::viewer::runtime_live::canonical_runtime_provider_env_lock()
        .lock()
        .unwrap_or_else(|p| p.into_inner());
    let _env = EnvRestore::capture();
    let http = HttpMetadata::start(true);
    configure(&http.endpoint);
    let mut sidecar = RuntimeLlmSidecar::new(ViewerLiveDecisionMode::Llm);
    run_probe(&mut sidecar);
    assert_eq!(http.counts(), (1, 0));
    assert!(matches!(
        sidecar.fresh_provider_metadata_readiness().unwrap(),
        FreshProviderMetadataReadiness::Failed
    ));
    assert_eq!(
        sidecar
            .provider_check_snapshot
            .as_ref()
            .unwrap()
            .error
            .as_deref(),
        Some("provider metadata probe failed")
    );
    http.fail.store(false, Ordering::SeqCst);
    assert!(sidecar.prepare_provider_metadata_probe().unwrap().is_none());
    assert_eq!(http.counts(), (1, 0));
    expire(&sidecar);
    run_probe(&mut sidecar);
    assert_eq!(http.counts(), (2, 1));
    assert!(matches!(
        sidecar.fresh_provider_metadata_readiness().unwrap(),
        FreshProviderMetadataReadiness::Ready { .. }
    ));
}
#[test]
fn actual_http_old_config_completion_preserves_new_inflight_and_ready_cache() {
    let _lock = crate::viewer::runtime_live::canonical_runtime_provider_env_lock()
        .lock()
        .unwrap_or_else(|p| p.into_inner());
    let _env = EnvRestore::capture();
    let a = HttpMetadata::start(false);
    let b = HttpMetadata::start(false);
    configure(&a.endpoint);
    let mut sidecar = RuntimeLlmSidecar::new(ViewerLiveDecisionMode::Llm);
    let old = sidecar
        .prepare_provider_metadata_probe()
        .unwrap()
        .unwrap()
        .execute();
    // Retain the actual received response for a duplicate transport completion;
    // no successful metadata DTO is manufactured by this test.
    let replay = match &old.response {
        Ok(AgentServiceIoResponse::Metadata(info, health)) => AgentServiceIoResult {
            token: old.token.clone(),
            response: Ok(AgentServiceIoResponse::Metadata(
                info.clone(),
                health.clone(),
            )),
        },
        _ => panic!("actual old HTTP probe failed"),
    };
    configure(&b.endpoint);
    let new = sidecar.prepare_provider_metadata_probe().unwrap().unwrap();
    let token = new.token.clone();
    sidecar.apply_provider_metadata_probe(old).unwrap();
    assert_eq!(
        sidecar.provider_metadata_inflight,
        Some((token.config_digest.clone(), token.generation))
    );
    assert_eq!(
        sidecar.provider_check_snapshot.as_ref().unwrap().cache_key,
        token.config_digest
    );
    sidecar
        .apply_provider_metadata_probe(new.execute())
        .unwrap();
    let ready = sidecar.provider_check_snapshot.clone();
    sidecar.apply_provider_metadata_probe(replay).unwrap();
    assert_eq!(sidecar.provider_check_snapshot, ready);
    assert!(sidecar.provider_metadata_inflight.is_none());
    assert!(matches!(
        sidecar.fresh_provider_metadata_readiness().unwrap(),
        FreshProviderMetadataReadiness::Ready { .. }
    ));
    assert_eq!(a.counts(), (1, 1));
    assert_eq!(b.counts(), (1, 1));
}
