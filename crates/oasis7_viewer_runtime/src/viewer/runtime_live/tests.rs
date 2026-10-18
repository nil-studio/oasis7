use super::*;
use crate::simulator::AgentDecision;
use crate::simulator::{
    DEFAULT_PROVIDER_ACTION_SCHEMA_VERSION, DEFAULT_PROVIDER_OBSERVATION_SCHEMA_VERSION,
    ProviderExecutionMode,
};
use oasis7_simulator::ResourceOwner;
use std::io::{BufRead, BufReader, BufWriter, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::atomic::Ordering;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};
#[path = "tests/agent_memory_correction_control.rs"]
mod agent_memory_correction_control;
mod auth_actions;
mod auth_actions_collect_data;
mod auth_actions_feedback;
mod auth_actions_fragment_replenishment;
#[path = "tests/auth_actions_provider_context.rs"]
mod auth_actions_provider_context;
#[path = "tests/auth_actions_provider_continuation_restart.rs"]
mod auth_actions_provider_continuation_restart;
#[path = "tests/auth_actions_provider_stale.rs"]
mod auth_actions_provider_stale;
mod authoritative;
mod background_play;
mod chain_sync;
#[path = "tests/mock_http.rs"]
mod mock_http;
mod paused_start;
#[path = "tests/periodic_connection_lifecycle.rs"]
mod periodic_connection_lifecycle;
#[path = "tests/provider_continuation_drains.rs"]
mod provider_continuation_drains;
pub(super) use chain_sync::TestChainStatusServer;
mod chain_sync_feedback;
mod claim_choice;
mod claim_runway;
mod director_capability;
mod fine_grain_translation;
mod governance_vote_quote;
mod governance_vote_quote_debug;
mod industrial_progression;
mod module_visual_runtime;
pub(super) use industrial_progression::setup_industrial_gameplay_with_completed_jobs;
#[path = "tests/canonical_goal_snapshot_privacy.rs"]
mod canonical_goal_snapshot_privacy;
mod industrial_progression_grind;
mod industrial_progression_preview;
mod industrial_progression_readiness;
#[path = "tests/power_projection.rs"]
mod power_projection;
mod power_sale_quote;
mod prompt_control;
#[path = "tests/prompt_control_enhanced.rs"]
mod prompt_control_enhanced;
#[path = "tests/prompt_control_terminal.rs"]
mod prompt_control_terminal;
mod provider_settings;
#[path = "tests/runtime_live_core.rs"]
mod runtime_live_core;
mod runtime_live_server_config;
mod schedule_recipe_quote;
mod smelter_affordability_debug;
mod snapshot_fallback;
mod snapshot_first_chat_unlock_preview;
mod snapshot_generated_world;
mod snapshot_micro_depot;
mod snapshot_progress;
mod snapshot_progress_empty_world;
mod snapshot_provider_probe;
mod snapshot_reprioritize;
mod social_quote;
mod social_quote_capability;
#[path = "tests_support.rs"]
mod tests_support;
mod transfer_material_quote;
mod wait_resolution_quote;
use tests_support::*;

fn send_runtime_live_request(writer: &mut BufWriter<TcpStream>, request: &ViewerRequest) {
    serde_json::to_writer(&mut *writer, request).expect("write request");
    writer.write_all(b"\n").expect("write newline");
    writer.flush().expect("flush request");
}
fn read_runtime_live_snapshot(reader: &mut BufReader<TcpStream>) -> WorldSnapshot {
    loop {
        let response = read_runtime_live_response(reader);
        if let ViewerResponse::Snapshot { snapshot } = response {
            return snapshot;
        }
    }
}

fn read_runtime_live_response(reader: &mut BufReader<TcpStream>) -> ViewerResponse {
    let mut line = String::new();
    reader.read_line(&mut line).expect("read response");
    serde_json::from_str(line.trim_end()).expect("decode response")
}

fn seed_agent_chat_oc(server: &mut ViewerRuntimeLiveServer, agent_id: &str) {
    server
        .world
        .set_main_token_supply(crate::runtime::MainTokenSupplyState {
            total_supply: 1_000_000,
            circulating_supply: 1_000_000,
            ..crate::runtime::MainTokenSupplyState::default()
        });
    server
        .world
        .set_main_token_account_balance(agent_id, 1, 0)
        .expect("seed agent chat OC");
}

fn test_writer_pair() -> (BufWriter<TcpStream>, TcpStream) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind test listener");
    let addr = listener.local_addr().expect("listener local addr");
    let client = TcpStream::connect(addr).expect("connect test client");
    let (server, _) = listener.accept().expect("accept test peer");
    (BufWriter::new(server), client)
}

fn read_response_line(peer: &TcpStream, timeout: Duration) -> Option<String> {
    let stream = peer.try_clone().expect("clone test peer");
    stream
        .set_read_timeout(Some(timeout))
        .expect("set read timeout");
    let mut reader = BufReader::new(stream);
    let mut line = String::new();
    match reader.read_line(&mut line) {
        Ok(0) => None,
        Ok(_) => Some(line),
        Err(err) => {
            if matches!(
                err.kind(),
                std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
            ) {
                None
            } else {
                panic!("read response line failed: {err}");
            }
        }
    }
}

fn read_available_runtime_live_responses(
    peer: &TcpStream,
    timeout: Duration,
) -> Vec<ViewerResponse> {
    let stream = peer.try_clone().expect("clone test peer");
    stream
        .set_read_timeout(Some(timeout))
        .expect("set read timeout");
    let mut reader = BufReader::new(stream);
    let mut responses = Vec::new();
    loop {
        let mut line = String::new();
        match reader.read_line(&mut line) {
            Ok(0) => break,
            Ok(_) => {
                let trimmed = line.trim_end();
                if !trimmed.is_empty() {
                    responses.push(serde_json::from_str(trimmed).expect("decode response"));
                }
            }
            Err(err) => {
                if matches!(
                    err.kind(),
                    std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                ) {
                    break;
                }
                panic!("read response line failed: {err}");
            }
        }
    }
    responses
}

fn read_control_completion_ack(
    peer: &TcpStream,
    timeout: Duration,
) -> Option<crate::viewer::ControlCompletionAck> {
    let stream = peer.try_clone().expect("clone test peer");
    stream
        .set_read_timeout(Some(Duration::from_millis(100)))
        .expect("set read timeout");
    let mut reader = BufReader::new(stream);
    let start = Instant::now();
    let mut line = String::new();
    while start.elapsed() < timeout {
        line.clear();
        match reader.read_line(&mut line) {
            Ok(0) => continue,
            Ok(_) => {}
            Err(err) => {
                if matches!(
                    err.kind(),
                    std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                ) {
                    continue;
                }
                panic!("read response line failed: {err}");
            }
        }
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let Ok(response) = serde_json::from_str::<crate::viewer::ViewerResponse>(trimmed) else {
            continue;
        };
        if let crate::viewer::ViewerResponse::ControlCompletionAck { ack } = response {
            return Some(ack);
        }
    }
    None
}

fn read_test_http_request(stream: &mut TcpStream) -> Vec<u8> {
    let mut request = Vec::new();
    let mut buffer = [0_u8; 4096];
    let mut expected_len = None;
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        let bytes = match stream.read(&mut buffer) {
            Ok(bytes) => bytes,
            Err(err)
                if matches!(
                    err.kind(),
                    std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                ) =>
            {
                if Instant::now() >= deadline {
                    panic!("timed out reading test http request chunk: {err}");
                }
                thread::sleep(Duration::from_millis(5));
                continue;
            }
            Err(err) => panic!("read test http request chunk: {err}"),
        };
        if bytes == 0 {
            break;
        }
        request.extend_from_slice(&buffer[..bytes]);
        if expected_len.is_none()
            && let Some(boundary) = request.windows(4).position(|window| window == b"\r\n\r\n")
        {
            let content_length = parse_test_http_content_length(&request[..boundary]);
            expected_len = Some(boundary + 4 + content_length);
        }
        if let Some(expected_len) = expected_len
            && request.len() >= expected_len
        {
            break;
        }
    }
    request
}

fn parse_test_http_content_length(header_bytes: &[u8]) -> usize {
    let header = std::str::from_utf8(header_bytes).expect("test request header utf-8");
    header
        .lines()
        .find_map(|line| {
            let (name, value) = line.split_once(':')?;
            if name.trim().eq_ignore_ascii_case("content-length") {
                Some(
                    value
                        .trim()
                        .parse::<usize>()
                        .expect("test request content-length"),
                )
            } else {
                None
            }
        })
        .unwrap_or(0)
}

fn wait_for_runtime_live_server(addr: &str) {
    for _ in 0..50 {
        if TcpStream::connect(addr).is_ok() {
            return;
        }
        thread::sleep(Duration::from_millis(20));
    }
    panic!("runtime live server did not start listening at {addr}");
}

fn runtime_live_temp_dir(label: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "oasis7_runtime_live_chain_status_{label}_{}_{}",
        std::process::id(),
        test_now_unix_ms()
    ));
    std::fs::create_dir_all(&dir).expect("create runtime live temp dir");
    dir
}

fn signed_prompt_control_apply_request(
    mut request: crate::viewer::PromptControlApplyRequest,
    intent: crate::viewer::PromptControlAuthIntent,
    nonce: u64,
    public_key_hex: &str,
    private_key_hex: &str,
) -> crate::viewer::PromptControlApplyRequest {
    request.public_key = Some(public_key_hex.to_string());
    let proof = crate::viewer::sign_prompt_control_apply_auth_proof(
        intent,
        &request,
        nonce,
        public_key_hex,
        private_key_hex,
    )
    .expect("sign prompt auth");
    request.auth = Some(proof);
    request
}

fn signed_agent_chat_request(
    mut request: crate::viewer::AgentChatRequest,
    nonce: u64,
    public_key_hex: &str,
    private_key_hex: &str,
) -> crate::viewer::AgentChatRequest {
    request.public_key = Some(public_key_hex.to_string());
    if request.intent_seq.is_none() {
        request.intent_seq = Some(nonce);
    }
    if request.world_id.is_none() {
        request.world_id = Some("live-runtime-minimal".to_string());
    }
    if request.reorg_epoch.is_none() {
        request.reorg_epoch = Some(0);
    }
    if request.authority_scope.is_none() {
        request.authority_scope = Some("player_agent_chat".to_string());
    }
    let proof =
        crate::viewer::sign_agent_chat_auth_proof(&request, nonce, public_key_hex, private_key_hex)
            .expect("sign agent chat auth");
    request.auth = Some(proof);
    request
}

fn signed_gameplay_action_request(
    mut request: crate::viewer::GameplayActionRequest,
    nonce: u64,
    public_key_hex: &str,
    private_key_hex: &str,
) -> crate::viewer::GameplayActionRequest {
    request.public_key = Some(public_key_hex.to_string());
    let proof = crate::viewer::sign_gameplay_action_auth_proof(
        &request,
        nonce,
        public_key_hex,
        private_key_hex,
    )
    .expect("sign gameplay action auth");
    request.auth = Some(proof);
    request
}

fn signed_session_register_request(
    mut request: crate::viewer::AuthoritativeSessionRegisterRequest,
    nonce: u64,
    public_key_hex: &str,
    private_key_hex: &str,
) -> crate::viewer::AuthoritativeSessionRegisterRequest {
    request.public_key = Some(public_key_hex.to_string());
    let proof = crate::viewer::sign_session_register_auth_proof(
        &request,
        nonce,
        public_key_hex,
        private_key_hex,
    )
    .expect("sign session register auth");
    request.auth = Some(proof);
    request
}

fn register_runtime_session(
    server: &mut ViewerRuntimeLiveServer,
    player_id: &str,
    agent_id: Option<&str>,
    nonce: u64,
    public_key_hex: &str,
    private_key_hex: &str,
) -> AuthoritativeRecoveryAck<u64> {
    register_runtime_session_with_options(
        server,
        player_id,
        agent_id,
        false,
        nonce,
        public_key_hex,
        private_key_hex,
    )
}

fn register_runtime_session_with_options(
    server: &mut ViewerRuntimeLiveServer,
    player_id: &str,
    agent_id: Option<&str>,
    force_rebind: bool,
    nonce: u64,
    public_key_hex: &str,
    private_key_hex: &str,
) -> AuthoritativeRecoveryAck<u64> {
    let request = signed_session_register_request(
        crate::viewer::AuthoritativeSessionRegisterRequest {
            player_id: player_id.to_string(),
            public_key: None,
            registration_grant: None,
            auth: None,
            requested_agent_id: agent_id.map(ToOwned::to_owned),
            force_rebind,
        },
        nonce,
        public_key_hex,
        private_key_hex,
    );
    let (ack, emit_snapshot_after_ack) = server
        .handle_authoritative_recovery(AuthoritativeRecoveryCommand::RegisterSession { request })
        .expect("register session");
    assert!(!emit_snapshot_after_ack);
    ack
}
