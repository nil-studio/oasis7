use super::*;

use crate::viewer::{
    ProviderOriginalMaterialOwnerV1, ViewerRuntimeLiveServer, ViewerRuntimeLiveServerConfig,
    install_provider_original_material_owner,
};
use oasis7::simulator::{
    ContinuousAgentRequestContextV1, ContinuousAgentResponseContextV1,
    ProviderAttemptAuthorityBindingV1, ProviderDispositionEvidenceV1, ProviderMaterialAttemptV1,
    ProviderMaterialAuthority, ProviderMaterialOwnerBindingV1, ProviderOriginalMaterialRefsV1,
    ProviderPendingAttemptPhaseV1, ProviderPendingRootsCursorV1, cognition_response_digest,
};
use oasis7_distfs::incremental_page_store::PageCompression;
use oasis7_proto::memory_budget::{MemoryBudget, MemoryClass};
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::thread;
use std::time::{Duration, Instant};

#[path = "llm_sidecar_provider_original_store_interruption_tests.rs"]
mod interruption_tests;

const FIXTURE_PARENT_ROOT_SET: [u8; 32] = [0x22; 32];
const FIXTURE_AUTHORITY_SNAPSHOT: [u8; 32] = [0x33; 32];

struct FixtureProviderAuthority {
    world_id: String,
    owner_id: [u8; 32],
}

impl ProviderMaterialAuthority for FixtureProviderAuthority {
    fn verify_owner_scope(&self, owner: &ProviderMaterialOwnerBindingV1) -> Result<(), String> {
        if owner.world_id != self.world_id || owner.owner_id != self.owner_id {
            return Err("fixture authority rejected owner scope".into());
        }
        owner.validate().map_err(str::to_owned)
    }

    fn request_binding(
        &self,
        owner: &ProviderMaterialOwnerBindingV1,
        request: &ContinuousAgentRequestContextV1,
    ) -> Result<ProviderAttemptAuthorityBindingV1, String> {
        self.verify_owner_scope(owner)?;
        if request.runtime_binding.world_id != owner.world_id {
            return Err("fixture request is outside owner world".into());
        }
        Ok(ProviderAttemptAuthorityBindingV1 {
            parent_root_set: FIXTURE_PARENT_ROOT_SET,
            authority_snapshot: FIXTURE_AUTHORITY_SNAPSHOT,
        })
    }

    fn verify_commit(
        &self,
        owner: &ProviderMaterialOwnerBindingV1,
        materials: &ProviderOriginalMaterialRefsV1,
        receipt_id: &str,
        action_id: u64,
    ) -> Result<(), String> {
        self.verify_owner_scope(owner)?;
        if materials.owner != *owner
            || materials.parent_root_set != FIXTURE_PARENT_ROOT_SET
            || materials.authority_snapshot != FIXTURE_AUTHORITY_SNAPSHOT
            || receipt_id != "fixture-runtime-receipt"
            || action_id != 7
        {
            return Err("fixture authority rejected commit disposition".into());
        }
        Ok(())
    }

    fn verify_rejection(
        &self,
        owner: &ProviderMaterialOwnerBindingV1,
        attempt: &ProviderMaterialAttemptV1,
        _evidence: ProviderDispositionEvidenceV1,
    ) -> Result<(), String> {
        self.verify_owner_scope(owner)?;
        if attempt.owner != *owner {
            return Err("fixture authority rejected foreign attempt".into());
        }
        Err("fixture has no authority-issued rejection disposition".into())
    }
}

fn capture_root() -> std::path::PathBuf {
    static NEXT: AtomicU64 = AtomicU64::new(1);
    std::env::temp_dir().join(format!(
        "oasis7-provider-original-owner-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ))
}

fn read_http_request(stream: &mut TcpStream) -> (String, Vec<u8>) {
    let mut reader = BufReader::new(stream.try_clone().expect("clone provider fixture stream"));
    let mut line = String::new();
    reader.read_line(&mut line).expect("read request line");
    let path = line
        .split_whitespace()
        .nth(1)
        .expect("request target")
        .to_owned();
    let mut content_length = None;
    loop {
        line.clear();
        reader.read_line(&mut line).expect("read request header");
        if line == "\r\n" || line == "\n" {
            break;
        }
        if let Some((name, value)) = line.split_once(':')
            && name.eq_ignore_ascii_case("content-length")
        {
            content_length = Some(value.trim().parse::<usize>().expect("content length"));
        }
    }
    let mut body = vec![0; content_length.expect("request body length")];
    reader.read_exact(&mut body).expect("read request body");
    (path, body)
}

fn captured_response(
    request: &ContinuousAgentRequestContextV1,
) -> ContinuousAgentResponseContextV1 {
    let base_decision_response = crate::simulator::DecisionResponse::wait("durable-owner-fixture");
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

fn store_captured_attempt(
    owner: &ProviderOriginalMaterialOwnerV1,
    base: &ContinuousAgentRequestContextV1,
    suffix: &str,
) -> (
    ContinuousAgentRequestContextV1,
    ContinuousAgentResponseContextV1,
    ProviderOriginalMaterialRefsV1,
) {
    let mut request = base.clone();
    request.decision_request_id.push_str(suffix);
    request.request_digest = request.request_digest();
    request.validate().expect("validate synthetic page attempt");
    let request_body = serde_json::to_vec(&request).expect("encode synthetic request");
    let request_id = *blake3::hash(&request_body).as_bytes();
    let mut request_reader = std::io::Cursor::new(request_body.as_slice());
    let attempt = owner
        .persist_request(
            &request,
            request_id,
            &mut request_reader,
            request_body.len() as u64,
        )
        .expect("persist synthetic request");
    owner
        .mark_dispatched(&attempt)
        .expect("persist synthetic dispatch phase");

    let response = captured_response(&request);
    let response_body = serde_json::to_vec(&response).expect("encode synthetic response");
    let response_id = *blake3::hash(&response_body).as_bytes();
    let mut response_reader = std::io::Cursor::new(response_body.as_slice());
    owner
        .persist_response_body(
            &attempt,
            200,
            response_id,
            &mut response_reader,
            response_body.len() as u64,
        )
        .expect("persist synthetic original response");
    let refs = owner
        .finish_response(&attempt, 200, response_id, &response)
        .expect("persist synthetic response context");
    (request, response, refs)
}

#[test]
fn configured_sidecar_owner_reopens_pending_refs_and_refuses_same_identity_dispatch() {
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
                    let (path, request_body) = read_http_request(&mut stream);
                    assert_eq!(path, "/v1/world-simulator/decision-context");
                    let request: ContinuousAgentRequestContextV1 =
                        serde_json::from_slice(&request_body).expect("decode original request");
                    let response = captured_response(&request);
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

    let world_id = "provider-durable-owner-world";
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
        .unwrap_or_else(|| "durable-owner-agent".to_string());
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

    let binding = ProviderMaterialOwnerBindingV1 {
        world_id: world_id.to_string(),
        genesis_root: [0x11; 32],
        owner_id: [0x44; 32],
    };
    let authority = Arc::new(FixtureProviderAuthority {
        world_id: world_id.to_string(),
        owner_id: binding.owner_id,
    });
    let root = capture_root();
    let memory = MemoryBudget::new(128 * 1024 * 1024, 8 * 1024 * 1024).expect("owner memory pool");
    let owner = ProviderOriginalMaterialOwnerV1::create(
        &root,
        binding.clone(),
        memory,
        PageCompression::Raw,
        authority.clone(),
    )
    .expect("create independent fixture pending owner");
    install_provider_original_material_owner(&mut server, owner.clone())
        .expect("install explicit native owner through Viewer entry");
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
        "first call starts the configured provider adapter"
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
                panic!("provider sidecar returned before response capture: {error}");
            }
        }
        let runner_status = server
            .llm_sidecar
            .runner
            .as_ref()
            .and_then(RuntimeDecisionRunner::async_runner)
            .map(|runner| {
                format!(
                    "active_turns={} provider_in_flight={}",
                    runner.active_turn_count(),
                    runner.provider_is_still_in_flight(agent_id.as_str()),
                )
            })
            .unwrap_or_else(|| "runner=unavailable".into());
        assert!(
            Instant::now() < deadline,
            "provider response did not return; {}; {runner_status}",
            owner.diagnostic_head_state(),
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
        .expect("content refs survive the sidecar outcome")
        .clone();
    let request = cognition.request.request_context.clone();
    server
        .llm_sidecar
        .verify_provider_original_materials(cognition)
        .expect("sidecar verifies refs against original identity");

    let (http_request_body, http_response_body) =
        server_thread.join().expect("join provider fixture");
    let response: ContinuousAgentResponseContextV1 =
        serde_json::from_slice(&http_response_body).expect("decode captured response fixture");
    assert_eq!(refs.owner, binding);
    assert_eq!(refs.parent_root_set, FIXTURE_PARENT_ROOT_SET);
    assert_eq!(refs.authority_snapshot, FIXTURE_AUTHORITY_SNAPSHOT);
    assert_eq!(
        refs.operation_id,
        request.provider_invocation_key().to_string()
    );
    assert_eq!(refs.transport_attempt, request.transport_attempt);
    assert_eq!(
        refs.request_object_id,
        *blake3::hash(&http_request_body).as_bytes()
    );
    assert_eq!(
        refs.response_object_id,
        *blake3::hash(&http_response_body).as_bytes()
    );
    owner
        .verify_commit_materials(&refs, &request, &response)
        .expect("original body and typed contexts are read back before consumption");

    let (paged_request, paged_response, paged_refs) =
        store_captured_attempt(owner.as_ref(), &request, "-page-continuation");

    let first_page = owner
        .protected_roots_page(None, 1)
        .expect("page first unresolved identity and actual state roots");
    assert_eq!(first_page.attempts.len(), 1);
    let first_cursor: &ProviderPendingRootsCursorV1 = first_page
        .next_cursor
        .as_ref()
        .expect("a second pending identity requires a continuation")
        .as_ref();
    let second_page = owner
        .protected_roots_page(Some(first_cursor), 1)
        .expect("continue the same protected-root snapshot");
    assert_eq!(second_page.attempts.len(), 1);
    assert!(second_page.next_cursor.is_none());
    assert_eq!(first_page.pending_root, second_page.pending_root);
    let summaries = first_page
        .attempts
        .iter()
        .chain(second_page.attempts.iter())
        .collect::<Vec<_>>();
    assert_eq!(summaries.len(), 2);
    assert!(summaries.iter().all(|pending| {
        pending.phase == ProviderPendingAttemptPhaseV1::ResponseCaptured
            && pending.attempt.parent_root_set == refs.parent_root_set
            && pending.attempt.authority_snapshot == refs.authority_snapshot
    }));
    assert!(
        summaries
            .iter()
            .any(|pending| pending.attempt.operation_id == refs.operation_id)
    );
    assert!(
        summaries
            .iter()
            .any(|pending| pending.attempt.operation_id == paged_refs.operation_id)
    );
    let all_page_object_ids = first_page
        .object_ids
        .iter()
        .chain(second_page.object_ids.iter())
        .copied()
        .collect::<std::collections::BTreeSet<_>>();
    for id in [
        refs.request_object_id,
        refs.request_context_object_id,
        refs.response_object_id,
        refs.response_context_object_id,
        paged_refs.request_object_id,
        paged_refs.request_context_object_id,
        paged_refs.response_object_id,
        paged_refs.response_context_object_id,
    ] {
        assert!(all_page_object_ids.contains(&id));
    }
    assert_eq!(first_page.state_roots.events.leaf_count, 12);
    assert_eq!(first_page.state_roots.pending.leaf_count, 2);
    assert_eq!(first_page.state_roots.seen.leaf_count, 2);
    let forged_cursor = ProviderPendingRootsCursorV1::from_owner_state("not a viewer cursor");
    assert!(owner.protected_roots_page(Some(&forged_cursor), 1).is_err());
    assert!(
        owner.close_ready().is_err(),
        "captured but unconsumed material stays protected"
    );
    let pending_root_before_reopen = owner.pending_protection_root().expect("protection root");

    drop(server);
    drop(owner);
    let reopened_memory =
        MemoryBudget::new(128 * 1024 * 1024, 8 * 1024 * 1024).expect("reopen memory pool");
    let ordinary_full = reopened_memory
        .reserve(120 * 1024 * 1024, MemoryClass::Ordinary)
        .expect("fill ordinary capacity while retaining recovery reserve");
    let reopened = ProviderOriginalMaterialOwnerV1::open(
        &root,
        &binding,
        reopened_memory,
        PageCompression::Raw,
        MemoryClass::Recovery,
        authority.clone(),
    )
    .expect("reopen durable pending store from recovery reserve");
    assert_eq!(
        reopened.pending_protection_root().expect("reopened root"),
        pending_root_before_reopen
    );
    let reopened_page = reopened
        .protected_roots_page(None, 1)
        .expect("recover the first page of pending attempts");
    assert_eq!(
        reopened_page.attempts.as_slice(),
        first_page.attempts.as_slice()
    );
    assert_eq!(
        reopened_page.state_roots.events,
        first_page.state_roots.events
    );
    assert_eq!(
        reopened_page.state_roots.pending,
        first_page.state_roots.pending
    );
    assert_eq!(reopened_page.state_roots.seen, first_page.state_roots.seen);
    let reopened_second_page = reopened
        .protected_roots_page(Some(first_cursor), 1)
        .expect("resume the exact root-bound cursor after reopen");
    assert_eq!(
        reopened_second_page.attempts.as_slice(),
        second_page.attempts.as_slice()
    );
    assert!(reopened_second_page.next_cursor.is_none());
    assert_eq!(
        reopened_second_page.pending_root,
        reopened_page.pending_root
    );
    assert!(reopened.close_ready().is_err());
    drop(ordinary_full);
    drop(reopened);

    let reopened = ProviderOriginalMaterialOwnerV1::open(
        &root,
        &binding,
        MemoryBudget::new(128 * 1024 * 1024, 8 * 1024 * 1024).expect("writer reopen pool"),
        PageCompression::Raw,
        MemoryClass::Ordinary,
        authority.clone(),
    )
    .expect("reopen durable pending store for identity reconciliation");
    assert_eq!(
        reopened
            .pending_protection_root()
            .expect("writer reopen root"),
        pending_root_before_reopen
    );

    // A same-identity retry is refused by the durable seen index before this
    // transport reaches the already closed first endpoint. The unchanged root
    // is the no-dispatch oracle, not the connection error alone.
    let before_retry = reopened.pending_protection_root().expect("before retry");
    let retry_client =
        oasis7::simulator::ProviderLoopbackHttpClient::new(base_url.as_str(), None, 5_000)
            .expect("create same-identity retry client");
    let retry_error = retry_client
        .request_decision_with_context_and_material_store(&request, reopened.as_ref())
        .expect_err("same operation identity cannot dispatch again after restart");
    assert!(
        retry_error
            .to_string()
            .contains("already consumed or dispatched")
    );
    assert_eq!(
        reopened
            .pending_protection_root()
            .expect("after refused retry"),
        before_retry
    );

    let forged_rejection = ProviderDispositionEvidenceV1 {
        evidence_object_id: [0x99; 32],
        disposition_digest: [0x98; 32],
    };
    assert!(
        reopened
            .mark_rejected(&reopened_page.attempts[0].attempt, forged_rejection)
            .is_err()
    );
    assert_eq!(
        reopened
            .pending_protection_root()
            .expect("after rejected evidence"),
        before_retry
    );
    reopened
        .verify_commit_materials(&refs, &request, &response)
        .expect("reopened owner revalidates exact original materials");
    reopened
        .verify_commit_materials(&paged_refs, &paged_request, &paged_response)
        .expect("reopened owner revalidates paged original materials");
    reopened
        .mark_committed(&refs, "fixture-runtime-receipt", 7)
        .expect("fixture authority accepts matching consume disposition");
    reopened
        .mark_committed(&paged_refs, "fixture-runtime-receipt", 7)
        .expect("fixture authority accepts the second matching disposition");
    assert!(
        reopened
            .protected_roots_page(Some(first_cursor), 1)
            .is_err(),
        "a cursor cannot continue after its root snapshot changes"
    );
    reopened
        .close_ready()
        .expect("terminally consumed owner is ready to close");
    let terminal_root = reopened.pending_protection_root().expect("terminal root");
    assert_ne!(terminal_root, before_retry);
    drop(reopened);

    let terminal = ProviderOriginalMaterialOwnerV1::open(
        &root,
        &binding,
        MemoryBudget::new(128 * 1024 * 1024, 8 * 1024 * 1024).expect("terminal reopen pool"),
        PageCompression::Raw,
        MemoryClass::Ordinary,
        authority,
    )
    .expect("reopen terminal state");
    terminal
        .close_ready()
        .expect("terminal status remains durable");
    let terminal_page = terminal
        .protected_roots_page(None, 8)
        .expect("terminal owner page");
    assert!(terminal_page.attempts.is_empty());
    assert_eq!(terminal_page.state_roots.events.leaf_count, 14);
    drop(terminal);
    std::fs::remove_dir_all(root).expect("remove provider owner fixture");
}
