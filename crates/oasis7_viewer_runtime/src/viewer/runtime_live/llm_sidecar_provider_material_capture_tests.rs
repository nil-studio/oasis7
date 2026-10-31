use super::*;

use crate::simulator::{
    ContinuousAgentRequestContextV1, ContinuousAgentResponseContextV1, ProviderMaterialAttemptV1,
    ProviderMaterialOwnerBindingV1, ProviderOriginalMaterialRefsV1, ProviderOriginalMaterialStore,
    ProviderSpoolReservation, cognition_response_digest,
};
use crate::viewer::{
    ProviderOriginalMaterialOwnerV1, ViewerRuntimeLiveServer, ViewerRuntimeLiveServerConfig,
    install_provider_original_material_owner,
};
use oasis7_proto::memory_budget::{MemoryBudget, MemoryClass, MemoryReservation};
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

struct TestSpoolReservation {
    _lease: MemoryReservation,
}
impl ProviderSpoolReservation for TestSpoolReservation {}

#[derive(Default)]
struct CapturedProviderMaterialState {
    events: Vec<&'static str>,
    attempt: Option<ProviderMaterialAttemptV1>,
    request_body: Option<Vec<u8>>,
    response_body: Option<Vec<u8>>,
    refs: Option<ProviderOriginalMaterialRefsV1>,
    committed: Option<(String, u64)>,
}

/// A test-only new-format owner. It exercises the install/sidecar/adapter
/// contract; it is not a production root owner or durability implementation.
struct TestProviderMaterialOwner {
    binding: ProviderMaterialOwnerBindingV1,
    state: Mutex<CapturedProviderMaterialState>,
    memory: MemoryBudget,
    cancelled: AtomicBool,
}

impl TestProviderMaterialOwner {
    fn new(world_id: &str) -> Self {
        Self {
            binding: ProviderMaterialOwnerBindingV1 {
                world_id: world_id.to_string(),
                genesis_root: [0x11; 32],
                owner_id: [0x44; 32],
            },
            state: Mutex::new(CapturedProviderMaterialState::default()),
            memory: MemoryBudget::new(16 * 1024 * 1024, 2 * 1024 * 1024)
                .expect("finite provider capture memory pool"),
            cancelled: AtomicBool::new(false),
        }
    }

    fn snapshot(&self) -> CapturedProviderMaterialState {
        let state = self.state.lock().expect("capture state lock");
        CapturedProviderMaterialState {
            events: state.events.clone(),
            attempt: state.attempt.clone(),
            request_body: state.request_body.clone(),
            response_body: state.response_body.clone(),
            refs: state.refs.clone(),
            committed: state.committed.clone(),
        }
    }
}

fn read_exact_bounded_body(
    input: &mut dyn Read,
    declared_length: u64,
    limit: u64,
) -> Result<Vec<u8>, String> {
    if declared_length > limit {
        return Err("body exceeds owner limit".to_string());
    }
    let capacity = usize::try_from(declared_length)
        .map_err(|_| "body length does not fit this process".to_string())?;
    let mut body = Vec::with_capacity(capacity);
    let mut buffer = [0u8; 8192];
    loop {
        let count = input.read(&mut buffer).map_err(|error| error.to_string())?;
        if count == 0 {
            break;
        }
        let next = body
            .len()
            .checked_add(count)
            .ok_or_else(|| "body length overflow".to_string())?;
        if next > capacity || next as u64 > limit {
            return Err("body stream exceeds declared owner bound".to_string());
        }
        body.extend_from_slice(&buffer[..count]);
    }
    if body.len() != capacity {
        return Err("body stream ended before its declared length".to_string());
    }
    Ok(body)
}

fn object_id(bytes: &[u8]) -> [u8; 32] {
    *blake3::hash(bytes).as_bytes()
}

impl ProviderOriginalMaterialStore for TestProviderMaterialOwner {
    fn owner_binding(&self) -> &ProviderMaterialOwnerBindingV1 {
        &self.binding
    }

    fn pending_protection_root(&self) -> Result<[u8; 32], String> {
        Ok([0x44; 32])
    }

    fn protected_roots_page(
        &self,
        _cursor: Option<&oasis7::simulator::ProviderPendingRootsCursorV1>,
        _limit: usize,
    ) -> Result<oasis7::simulator::ProviderPendingRootsPageV1, String> {
        let charge = self
            .memory
            .reserve(0, MemoryClass::Ordinary)
            .map_err(|error| error.to_string())?;
        let owner_charge = self
            .memory
            .reserve(512, MemoryClass::Ordinary)
            .map_err(|error| error.to_string())?;
        let attempt_charge = self
            .memory
            .reserve(0, MemoryClass::Ordinary)
            .map_err(|error| error.to_string())?;
        let roots_charge = self
            .memory
            .reserve(512, MemoryClass::Ordinary)
            .map_err(|error| error.to_string())?;
        let empty = oasis7_formal_protocol::incremental_state::StateRootV1::empty(
            oasis7_formal_protocol::incremental_state::StateNamespaceV1::Effects,
        )
        .map_err(|error| error.to_string())?;
        Ok(oasis7::simulator::ProviderPendingRootsPageV1 {
            owner: std::sync::Arc::new(owner_charge.retain(self.binding.clone())),
            pending_root: [0x44; 32],
            object_ids: std::sync::Arc::new(charge.retain(Vec::new())),
            attempts: std::sync::Arc::new(attempt_charge.retain(Vec::new())),
            state_roots: std::sync::Arc::new(roots_charge.retain(
                oasis7::simulator::ProviderPendingStateRootsV1 {
                    events: empty.clone(),
                    pending: empty.clone(),
                    seen: empty,
                },
            )),
            next_cursor: None,
        })
    }

    fn validate_request_owner(
        &self,
        request: &ContinuousAgentRequestContextV1,
    ) -> Result<(), String> {
        request
            .validate_production_lane()
            .map_err(|error| error.to_string())?;
        if request.runtime_binding.world_id != self.binding.world_id {
            return Err("request world does not match fixture owner".to_string());
        }
        Ok(())
    }

    fn memory_budget(&self) -> MemoryBudget {
        self.memory.clone()
    }

    fn request_body_limit_bytes(&self) -> u64 {
        1024 * 1024
    }

    fn response_body_limit_bytes(&self) -> u64 {
        1024 * 1024
    }

    fn reserve_body_spools(
        &self,
        request_limit: u64,
        response_limit: u64,
    ) -> Result<Box<dyn ProviderSpoolReservation>, String> {
        let bytes = request_limit
            .checked_add(response_limit)
            .and_then(|value| usize::try_from(value).ok())
            .ok_or_else(|| "spool reservation size overflow".to_string())?;
        self.memory
            .reserve(bytes, MemoryClass::Ordinary)
            .map(|reservation| {
                Box::new(TestSpoolReservation {
                    _lease: reservation,
                }) as _
            })
            .map_err(|error| error.to_string())
    }

    fn cancelled(&self) -> bool {
        self.cancelled.load(Ordering::Acquire)
    }

    fn persist_request(
        &self,
        request: &ContinuousAgentRequestContextV1,
        expected_body_id: [u8; 32],
        original_body: &mut dyn Read,
        length: u64,
    ) -> Result<ProviderMaterialAttemptV1, String> {
        let body = read_exact_bounded_body(original_body, length, self.request_body_limit_bytes())?;
        if object_id(&body) != expected_body_id {
            return Err("request body ID differs from HTTP spool".into());
        }
        let context_bytes = serde_json::to_vec(request).map_err(|error| error.to_string())?;
        let request_object_id = object_id(&body);
        if request_object_id != expected_body_id {
            return Err("request body ID differs from HTTP spool".into());
        }
        let request_context_object_id = object_id(&context_bytes);
        let operation_id = request.provider_invocation_key().to_string();
        let attempt_id = object_id(
            &[
                self.binding.owner_id.as_slice(),
                operation_id.as_bytes(),
                &request.transport_attempt.to_be_bytes(),
                request_object_id.as_slice(),
            ]
            .concat(),
        );
        let attempt = ProviderMaterialAttemptV1 {
            owner: self.binding.clone(),
            operation_id,
            transport_attempt: request.transport_attempt,
            attempt_id,
            parent_root_set: [0x22; 32],
            authority_snapshot: [0x33; 32],
            request_object_id,
            request_context_object_id,
        };
        let mut state = self.state.lock().expect("capture state lock");
        state.events.push("persist_request");
        state.attempt = Some(attempt.clone());
        state.request_body = Some(body);
        Ok(attempt)
    }

    fn mark_dispatched(&self, attempt: &ProviderMaterialAttemptV1) -> Result<(), String> {
        let mut state = self.state.lock().expect("capture state lock");
        if state.attempt.as_ref() != Some(attempt) {
            return Err("dispatch marker changed the original attempt".to_string());
        }
        state.events.push("mark_dispatched");
        Ok(())
    }

    fn persist_response_body(
        &self,
        attempt: &ProviderMaterialAttemptV1,
        _status: u16,
        expected_body_id: [u8; 32],
        original_body: &mut dyn Read,
        length: u64,
    ) -> Result<[u8; 32], String> {
        let body =
            read_exact_bounded_body(original_body, length, self.response_body_limit_bytes())?;
        let response_id = object_id(&body);
        if response_id != expected_body_id {
            return Err("response body ID differs from HTTP spool".into());
        }
        let mut state = self.state.lock().expect("capture state lock");
        if state.attempt.as_ref() != Some(attempt) {
            return Err("response belongs to a different attempt".to_string());
        }
        state.events.push("persist_response");
        state.response_body = Some(body);
        Ok(response_id)
    }

    fn finish_response(
        &self,
        attempt: &ProviderMaterialAttemptV1,
        status: u16,
        response_object_id: [u8; 32],
        response: &ContinuousAgentResponseContextV1,
    ) -> Result<ProviderOriginalMaterialRefsV1, String> {
        let response_context = serde_json::to_vec(response).map_err(|error| error.to_string())?;
        let refs = ProviderOriginalMaterialRefsV1 {
            owner: attempt.owner.clone(),
            operation_id: attempt.operation_id.clone(),
            transport_attempt: attempt.transport_attempt,
            attempt_id: attempt.attempt_id,
            parent_root_set: attempt.parent_root_set,
            authority_snapshot: attempt.authority_snapshot,
            request_object_id: attempt.request_object_id,
            request_context_object_id: attempt.request_context_object_id,
            response_object_id,
            response_context_object_id: object_id(&response_context),
            response_status: status,
        };
        let mut state = self.state.lock().expect("capture state lock");
        if state.attempt.as_ref() != Some(attempt) {
            return Err("typed response belongs to a different attempt".to_string());
        }
        state.events.push("finish_response");
        state.refs = Some(refs.clone());
        Ok(refs)
    }

    fn verify_commit_materials(
        &self,
        materials: &ProviderOriginalMaterialRefsV1,
        request: &ContinuousAgentRequestContextV1,
        response: &ContinuousAgentResponseContextV1,
    ) -> Result<(), String> {
        let state = self.state.lock().expect("capture state lock");
        if state.refs.as_ref() != Some(materials)
            || materials.operation_id != request.provider_invocation_key().to_string()
            || materials.transport_attempt != request.transport_attempt
            || materials.request_object_id
                != object_id(
                    state
                        .request_body
                        .as_deref()
                        .ok_or("request body missing")?,
                )
            || materials.response_object_id
                != object_id(
                    state
                        .response_body
                        .as_deref()
                        .ok_or("response body missing")?,
                )
            || response.request_digest != request.request_digest
        {
            return Err("captured material identity does not match Runtime action".to_string());
        }
        Ok(())
    }

    fn mark_committed(
        &self,
        materials: &ProviderOriginalMaterialRefsV1,
        receipt_id: &str,
        action_id: u64,
    ) -> Result<(), String> {
        let mut state = self.state.lock().expect("capture state lock");
        if state.refs.as_ref() != Some(materials) || receipt_id.trim().is_empty() {
            return Err("receipt does not close the captured attempt".to_string());
        }
        state.committed = Some((receipt_id.to_string(), action_id));
        Ok(())
    }

    fn mark_rejected(
        &self,
        _attempt: &ProviderMaterialAttemptV1,
        _evidence: oasis7::simulator::ProviderDispositionEvidenceV1,
    ) -> Result<(), String> {
        Err("test owner does not support rejection evidence".into())
    }
}

fn read_request(stream: &mut TcpStream) -> (String, Vec<u8>) {
    let mut reader = BufReader::new(stream.try_clone().expect("clone provider fixture stream"));
    let mut line = String::new();
    reader.read_line(&mut line).expect("read HTTP request line");
    let path = line
        .split_whitespace()
        .nth(1)
        .expect("HTTP request path")
        .to_string();
    let mut content_length = None;
    loop {
        line.clear();
        reader
            .read_line(&mut line)
            .expect("read HTTP request header");
        if line == "\r\n" || line == "\n" {
            break;
        }
        if let Some((name, value)) = line.split_once(':')
            && name.eq_ignore_ascii_case("content-length")
        {
            content_length = Some(value.trim().parse::<usize>().expect("content length"));
        }
    }
    let length = content_length.expect("provider request content length");
    let mut body = vec![0u8; length];
    reader
        .read_exact(&mut body)
        .expect("read provider request body");
    (path, body)
}

fn response_for_request(
    request: &ContinuousAgentRequestContextV1,
) -> ContinuousAgentResponseContextV1 {
    let base_decision_response = crate::simulator::DecisionResponse::wait("capture-fixture");
    ContinuousAgentResponseContextV1 {
        response_digest: cognition_response_digest(&base_decision_response),
        base_decision_response,
        context_discriminator: crate::simulator::CONTINUOUS_AGENT_CONTEXT_DISCRIMINATOR.into(),
        context_version: crate::simulator::CONTINUOUS_AGENT_CONTEXT_VERSION,
        agent_session_id: request.agent_session_id.clone(),
        agent_turn_id: request.agent_turn_id.clone(),
        decision_request_id: request.decision_request_id.clone(),
        retry_seq: request.retry_seq,
        transport_attempt: request.transport_attempt,
        request_digest: request.request_digest.clone(),
    }
}

#[test]
fn live_server_owner_configuration_reaches_provider_sidecar_and_adapter() {
    let _env_guard = crate::viewer::runtime_live::canonical_runtime_provider_env_lock()
        .lock()
        .expect("provider environment lock");
    let _provider_env_snapshot = ProviderEnvSnapshot::capture(&[
        VIEWER_AGENT_DECISION_SOURCE_ENV,
        VIEWER_AGENT_PROVIDER_BACKEND_ENV,
        VIEWER_AGENT_PROVIDER_CONTRACT_ENV,
        VIEWER_AGENT_PROVIDER_TRANSPORT_ENV,
        VIEWER_AGENT_PROVIDER_URL_ENV,
        VIEWER_AGENT_PROVIDER_AUTH_TOKEN_ENV,
        VIEWER_AGENT_PROVIDER_CONNECT_TIMEOUT_MS_ENV,
        VIEWER_AGENT_PROVIDER_DECISION_TIMEOUT_MS_ENV,
        VIEWER_AGENT_PROVIDER_PROFILE_ENV,
        VIEWER_AGENT_EXECUTION_LANE_ENV,
        VIEWER_AGENT_PROVIDER_MODE_ENV,
    ]);

    let listener = TcpListener::bind("127.0.0.1:0").expect("bind provider fixture");
    listener
        .set_nonblocking(true)
        .expect("make provider fixture nonblocking");
    let base_url = format!(
        "http://{}/v1",
        listener.local_addr().expect("fixture address")
    );
    // SAFETY: This test owns the canonical provider environment lock and
    // restores the previous process environment when it exits.
    unsafe {
        oasis7::env_mut::set_var(VIEWER_AGENT_PROVIDER_MODE_ENV, "provider_loopback_http");
        oasis7::env_mut::set_var(VIEWER_AGENT_PROVIDER_URL_ENV, base_url.as_str());
        oasis7::env_mut::set_var(VIEWER_AGENT_PROVIDER_PROFILE_ENV, "oasis7_p0_low_freq_npc");
        oasis7::env_mut::set_var(VIEWER_AGENT_EXECUTION_LANE_ENV, "player_parity");
    }

    let server_thread = thread::spawn(move || {
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            match listener.accept() {
                Ok((mut stream, _)) => {
                    stream
                        .set_read_timeout(Some(Duration::from_secs(5)))
                        .expect("set provider fixture read timeout");
                    stream
                        .set_write_timeout(Some(Duration::from_secs(5)))
                        .expect("set provider fixture write timeout");
                    let (path, request_body) = read_request(&mut stream);
                    assert_eq!(path, "/v1/world-simulator/decision-context");
                    let request: ContinuousAgentRequestContextV1 =
                        serde_json::from_slice(&request_body).expect("decode original request");
                    let response = response_for_request(&request);
                    let response_body =
                        serde_json::to_vec(&response).expect("encode provider response");
                    write!(
                        stream,
                        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                        response_body.len()
                    )
                    .expect("write provider response headers");
                    stream
                        .write_all(&response_body)
                        .expect("write provider response body");
                    stream.flush().expect("flush provider response");
                    return (request_body, response_body);
                }
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                    assert!(
                        Instant::now() < deadline,
                        "provider adapter made no request"
                    );
                    thread::sleep(Duration::from_millis(2));
                }
                Err(error) => panic!("provider fixture accept failed: {error}"),
            }
        }
    });

    let world_id = "provider-capture-config-world";
    let config = ViewerRuntimeLiveServerConfig::new(oasis7_simulator::WorldScenario::Minimal)
        .with_world_id(world_id)
        .with_decision_mode(ViewerLiveDecisionMode::Llm);
    let mut server = ViewerRuntimeLiveServer::new(config).expect("create configured live server");
    let agent_id = server
        .world
        .state()
        .agents
        .keys()
        .next()
        .cloned()
        .unwrap_or_else(|| "capture-agent".to_string());
    if server.world.state().agents.is_empty() {
        server.world.submit_action(RuntimeAction::RegisterAgent {
            agent_id: agent_id.clone(),
            pos: GeoPos::new(0, 0, 0),
        });
        server.world.step().expect("seed provider fixture agent");
    }
    server
        .world
        .install_test_provider_capability_fixture(agent_id.as_str())
        .expect("install provider capability fixture");

    let owner = Arc::new(TestProviderMaterialOwner::new(world_id));
    server
        .install_provider_original_material_store(owner.clone())
        .expect("install explicit owner through Viewer config entry");
    server
        .llm_sidecar
        .sync_shadow_kernel(&server.world, &server.snapshot_config)
        .expect("sync sidecar shadow kernel");
    server
        .llm_sidecar
        .ensure_runner_initialized()
        .expect("initialize the configured ProviderBacked runner");
    crate::viewer::runtime_live::llm_sidecar_feedback_recovery::restore_agent_feedback_history(
        &server.world,
        server
            .llm_sidecar
            .runner
            .as_mut()
            .and_then(RuntimeDecisionRunner::async_runner_mut)
            .expect("native async provider runner"),
    )
    .expect("restore provider feedback history");
    let mut kernel = server
        .llm_sidecar
        .shadow_kernel
        .take()
        .expect("provider sidecar shadow kernel");
    assert!(
        server
            .llm_sidecar
            .next_async_provider_decision(&mut server.world, &mut kernel, world_id)
            .is_none(),
        "first call starts the actual async ProviderBacked adapter"
    );

    let deadline = Instant::now() + Duration::from_secs(10);
    let decision = loop {
        if let Some(decision) = server.llm_sidecar.next_async_provider_decision(
            &mut server.world,
            &mut kernel,
            world_id,
        ) {
            if decision.cognition.is_some() {
                break decision;
            }
            if let Some(error) = decision
                .decision_trace
                .as_ref()
                .and_then(|trace| trace.llm_error.as_deref())
            {
                panic!("provider sidecar returned before the captured response: {error}");
            }
        }
        assert!(
            Instant::now() < deadline,
            "provider response did not return to Runtime"
        );
        thread::sleep(Duration::from_millis(3));
    };
    let cognition = decision
        .cognition
        .as_ref()
        .expect("provider cognition result");
    let refs = cognition
        .provider_original_material_refs
        .as_ref()
        .expect("captured content references survive the sidecar outcome");
    server
        .llm_sidecar
        .verify_provider_original_materials(cognition)
        .expect("sidecar verifies the returned refs against the original identity");

    let (http_request_body, http_response_body) = server_thread
        .join()
        .expect("join one-request provider fixture");
    let captured = owner.snapshot();
    assert_eq!(
        captured.request_body.as_deref(),
        Some(http_request_body.as_slice())
    );
    assert_eq!(
        captured.response_body.as_deref(),
        Some(http_response_body.as_slice())
    );
    assert_eq!(
        captured.events,
        [
            "persist_request",
            "mark_dispatched",
            "persist_response",
            "finish_response"
        ]
    );
    assert_eq!(refs.owner, *owner.owner_binding());
    assert_eq!(
        refs.operation_id,
        cognition
            .request
            .request_context
            .provider_invocation_key()
            .to_string()
    );
    assert_eq!(
        refs.transport_attempt,
        cognition.request.request_context.transport_attempt
    );
    assert_eq!(refs.request_object_id, object_id(&http_request_body));
    assert_eq!(refs.response_object_id, object_id(&http_response_body));
    assert!(
        captured.committed.is_none(),
        "the test stops before a Runtime receipt closes the pending attempt"
    );
}
