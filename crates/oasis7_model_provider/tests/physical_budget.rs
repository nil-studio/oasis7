//! Local HTTP conformance for physical-call accounting. No paid endpoint is used.
use oasis7_agent_api::{
    ModelProvider, ModelProviderError, ModelProviderRequest, ModelProviderResponse,
};
use oasis7_agent_harness::{
    NativeTaskContext, NativeTaskControl, NativeTaskHost, NativeTaskRecovery, NativeTaskRunConfig,
    NativeTaskRunner, NativeTaskStopReason, NativeTaskTool, NativeTaskToolResult,
};
use oasis7_model_provider::{
    CompletionToolProfile, LlmCompletionRequest, ModelProviderConfig, OpenAiChatCompletionClient,
};
use serde_json::{Value, json};
use std::{
    io::{Read, Write},
    net::{TcpListener, TcpStream},
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicUsize, Ordering},
        mpsc,
    },
    thread,
    time::{Duration, Instant},
};

struct HttpProbe {
    endpoint: String,
    count: Arc<AtomicUsize>,
    started: Arc<AtomicBool>,
    stop: Arc<AtomicBool>,
    release: Option<mpsc::Sender<()>>,
    thread: Option<thread::JoinHandle<()>>,
}
fn read_body(stream: &mut TcpStream) {
    // Accepted sockets inherit the listener's nonblocking mode on macOS.
    // Request reads use the explicit timeout rather than an accept/data race.
    stream.set_nonblocking(false).unwrap();
    stream
        .set_read_timeout(Some(Duration::from_secs(2)))
        .unwrap();
    let mut bytes = Vec::new();
    let mut chunk = [0; 4096];
    let end = loop {
        let n = stream.read(&mut chunk).unwrap();
        assert!(n > 0);
        bytes.extend_from_slice(&chunk[..n]);
        if let Some(i) = bytes.windows(4).position(|w| w == b"\r\n\r\n") {
            break i + 4;
        }
        assert!(bytes.len() < 65536);
    };
    let head = String::from_utf8_lossy(&bytes[..end]);
    let length: usize = head
        .lines()
        .find_map(|line| {
            let (name, value) = line.split_once(':')?;
            name.eq_ignore_ascii_case("content-length")
                .then(|| value.trim().parse().unwrap())
        })
        .unwrap();
    assert!(length < 1024 * 1024);
    while bytes.len() < end + length {
        let n = stream.read(&mut chunk).unwrap();
        assert!(n > 0);
        bytes.extend_from_slice(&chunk[..n]);
    }
}
impl HttpProbe {
    fn start(status: u16, delayed: bool) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let endpoint = format!("http://{}/v1", listener.local_addr().unwrap());
        let count = Arc::new(AtomicUsize::new(0));
        let started = Arc::new(AtomicBool::new(false));
        let stop = Arc::new(AtomicBool::new(false));
        let (c, s, done) = (count.clone(), started.clone(), stop.clone());
        let (tx, rx) = mpsc::channel();
        let task = thread::spawn(move || {
            while !done.load(Ordering::SeqCst) {
                match listener.accept() {
                    Ok((mut stream, _)) => {
                        read_body(&mut stream);
                        let index = c.fetch_add(1, Ordering::SeqCst);
                        s.store(true, Ordering::SeqCst);
                        if delayed && index == 0 {
                            let _ = rx.recv_timeout(Duration::from_secs(3));
                        }
                        let body = r#"{"error":{"message":"HTTP conformance unavailable","type":"rate_limit_error","code":"fixture","param":null}}"#;
                        let _ = write!(
                            stream,
                            "HTTP/1.1 {status} Fixture\r\nContent-Type: application/json\r\nRetry-After: 0\r\nLocation: /v1/responses\r\nConnection: close\r\nContent-Length: {}\r\n\r\n{body}",
                            body.len()
                        );
                    }
                    Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                        thread::sleep(Duration::from_millis(2))
                    }
                    Err(e) => panic!("HTTP conformance listener: {e}"),
                }
            }
        });
        Self {
            endpoint,
            count,
            started,
            stop,
            release: Some(tx),
            thread: Some(task),
        }
    }
    fn release(&mut self) {
        if let Some(tx) = self.release.take() {
            let _ = tx.send(());
        }
    }
}
impl Drop for HttpProbe {
    fn drop(&mut self) {
        self.release();
        self.stop.store(true, Ordering::SeqCst);
        if let Some(task) = self.thread.take()
            && let Err(error) = task.join()
            && !thread::panicking()
        {
            std::panic::resume_unwind(error);
        }
    }
}
fn client(endpoint: &str) -> OpenAiChatCompletionClient {
    let config = ModelProviderConfig::from_values_for_agent(
        |key| match key {
            "OASIS7_LLM_BASE_URL" => Some(endpoint.into()),
            "OASIS7_LLM_API_KEY" => Some("local-fixture-secret".into()),
            "OASIS7_LLM_TIMEOUT_MS" => Some("2000".into()),
            _ => None,
        },
        "agent",
    )
    .unwrap();
    OpenAiChatCompletionClient::from_config(&config).unwrap()
}
fn request() -> ModelProviderRequest {
    serde_json::from_value(json!({"request_digest":format!("blake3:{}","a".repeat(64)),"model":"local-fixture",
        "system_prompt":"system","user_prompt":"goal","tools":[{"name":"observe","parameters":{"type":"object","properties":{},"additionalProperties":false}}],
        "budget":{"max_latency_ms":2000,"max_repair_attempts":0,"max_model_calls":1,"max_tool_calls":1}})).unwrap()
}
#[test]
fn native_budget_one_sends_one_http_request_for_429_and_503() {
    for status in [429, 503] {
        let probe = HttpProbe::start(status, false);
        assert!(ModelProvider::complete_checked(&mut client(&probe.endpoint), &request()).is_err());
        assert_eq!(
            probe.count.load(Ordering::SeqCst),
            1,
            "HTTP {status} must not activate the SDK default retry layer"
        );
    }
}
#[test]
fn native_budget_does_not_replay_provider_post_through_redirects() {
    let probe = HttpProbe::start(307, false);
    assert!(ModelProvider::complete_checked(&mut client(&probe.endpoint), &request()).is_err());
    assert_eq!(probe.count.load(Ordering::SeqCst), 1);
}
#[test]
fn legacy_unbudgeted_transport_preserves_sdk_http_status_retry_policy() {
    let probe = HttpProbe::start(503, false);
    let result = client(&probe.endpoint).complete_with_profile(
        &LlmCompletionRequest {
            model: "local-fixture".into(),
            system_prompt: "legacy".into(),
            user_prompt: "goal".into(),
            debug_mode: false,
            max_model_calls: None,
        },
        CompletionToolProfile::LegacySimulator,
    );
    assert!(result.is_err());
    assert_eq!(probe.count.load(Ordering::SeqCst), 4);
}

struct ControlHost {
    started: Arc<AtomicBool>,
    stop_after_start: bool,
    spent: AtomicUsize,
}
impl NativeTaskHost for ControlHost {
    fn initialize_run(&self, _: &NativeTaskRunConfig) -> Result<u32, String> {
        Ok(0)
    }
    fn reserve_model_call(&self, _: &NativeTaskRunConfig, _: &str) -> Result<u32, String> {
        let count = self.spent.fetch_add(1, Ordering::SeqCst) + 1;
        if count > 1 {
            return Err("budget exhausted".into());
        }
        Ok(count as u32)
    }
    fn recover(&self, _: &str) -> Result<NativeTaskRecovery, String> {
        Ok(NativeTaskRecovery::Vacant)
    }
    fn settle_exhausted_context(&self, _: &str) -> Result<NativeTaskRecovery, String> {
        Err("unused context settlement in model transport fixture".into())
    }
    fn prepare(&self, operation: &str) -> Result<NativeTaskContext, String> {
        Ok(NativeTaskContext {
            operation: operation.into(),
            fence: self.current_control()?.fence,
            valid_until_tick: 100,
            model_context: json!({"goal":{"goal_text":"complete goal","soft_constraints":[]},"actor_observation":{},"capability_catalog":{}}),
        })
    }
    fn current_control(&self) -> Result<NativeTaskControl, String> {
        let stopped = self.stop_after_start && self.started.load(Ordering::SeqCst);
        let fence=serde_json::from_value(json!({"world_id":"w","genesis_digest":"g","branch_id":"main","reorg_epoch":"0",
            "agent_id":"agent","owner_binding":"owner","owner_generation":"1","owner_public_key":"a".repeat(64),
            "task_id":"task","task_revision":"1","task_status":if stopped {"suspended"} else {"active"},
            "latest_goal_revision":"0","active_goal_revision":"0","active_goal_reference":null,"executor_epoch":if stopped {"2"} else {"1"},
            "executor_binding":if stopped {Value::Null} else {json!({"executor_id":"executor","executor_public_key":"b".repeat(64),"delegation_generation":"1","executor_epoch":"1"})}})).unwrap();
        Ok(NativeTaskControl {
            fence,
            logical_tick: 1,
        })
    }
    fn invoke(
        &self,
        _: &NativeTaskContext,
        _: &NativeTaskTool,
    ) -> Result<NativeTaskToolResult, String> {
        panic!("failed HTTP request must not execute a world tool")
    }
}
struct ObservedProductionProvider {
    inner: OpenAiChatCompletionClient,
    finished: Arc<AtomicBool>,
}
impl ModelProvider for ObservedProductionProvider {
    fn provider_id(&self) -> &str {
        "observed-production-http"
    }
    fn complete(
        &mut self,
        request: &ModelProviderRequest,
    ) -> Result<ModelProviderResponse, ModelProviderError> {
        let result = ModelProvider::complete(&mut self.inner, request);
        self.finished.store(true, Ordering::SeqCst);
        result
    }
}
#[test]
fn stopped_or_timed_out_native_worker_cannot_start_hidden_http_retries() {
    for (stop_after_start, status) in [(true, 429), (true, 503), (false, 503)] {
        let mut probe = HttpProbe::start(status, true);
        let host = Arc::new(ControlHost {
            started: probe.started.clone(),
            stop_after_start,
            spent: AtomicUsize::new(0),
        });
        let finished = Arc::new(AtomicBool::new(false));
        let provider = ObservedProductionProvider {
            inner: client(&probe.endpoint),
            finished: finished.clone(),
        };
        let config = NativeTaskRunConfig {
            run_id: "single-reserved-http-call".into(),
            model: "local-fixture".into(),
            system_prompt: "local unpaid HTTP conformance".into(),
            max_turns: 1,
            max_model_calls: 1,
            max_tool_calls_per_call: 1,
            model_timeout_ms: if stop_after_start { 2000 } else { 100 },
            control_poll_ms: 2,
        };
        let report = NativeTaskRunner::new(host.clone(), Box::new(provider), config)
            .unwrap()
            .run()
            .unwrap();
        if stop_after_start {
            assert_eq!(report.stop_reason, NativeTaskStopReason::ControlChanged);
        } else {
            assert!(matches!(
                report.stop_reason,
                NativeTaskStopReason::ModelTimeout | NativeTaskStopReason::ModelFailed
            ));
        }
        assert_eq!(report.model_calls_spent, 1);
        assert_eq!(host.spent.load(Ordering::SeqCst), 1);
        // Return a retryable HTTP error after the Harness has already discarded
        // this worker. Keep serving until the production provider actually ends,
        // so any hidden status retry is counted rather than refused by teardown.
        probe.release();
        let until = Instant::now() + Duration::from_secs(4);
        while !finished.load(Ordering::SeqCst) {
            assert!(Instant::now() < until, "provider worker failed to finish");
            thread::sleep(Duration::from_millis(2));
        }
        assert_eq!(
            probe.count.load(Ordering::SeqCst),
            1,
            "late HTTP {status} after Stop/deadline must not retry"
        );
    }
}
