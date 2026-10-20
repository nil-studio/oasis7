use super::{
    ChainAgentClaimActionResponse, ChainAgentClaimSubmitRequest, maybe_handle_agent_claim_request,
    reset_agent_claim_api_state_for_tests,
};
use crate::cli::DEFAULT_NODE_ID;
use oasis7::geometry::GeoPos;
use oasis7_node::{
    NodeConfig, NodeExecutionCommitContext, NodeExecutionCommitResult, NodeExecutionHook, NodeRole,
    NodeRuntime,
};
use oasis7_runtime::{Action, MainTokenSupplyState, World};
use std::fs;
use std::io::Read;
use std::net::{TcpListener, TcpStream};
use std::path::PathBuf;
use std::sync::{Arc, Mutex, MutexGuard, OnceLock};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

fn agent_claim_api_test_guard() -> MutexGuard<'static, ()> {
    static GUARD: OnceLock<Mutex<()>> = OnceLock::new();
    GUARD
        .get_or_init(|| Mutex::new(()))
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn tcp_stream_pair() -> (TcpStream, TcpStream) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind loopback listener");
    let bind = listener.local_addr().expect("read local addr");
    let client = TcpStream::connect(bind).expect("connect loopback client");
    let (server, _) = listener.accept().expect("accept loopback connection");
    (server, client)
}

#[derive(Debug)]
struct NoopExecutionHook;

impl NodeExecutionHook for NoopExecutionHook {
    fn on_commit(
        &mut self,
        context: NodeExecutionCommitContext,
    ) -> Result<NodeExecutionCommitResult, String> {
        Ok(NodeExecutionCommitResult {
            execution_height: context.height,
            execution_block_hash: format!("noop-block-{}", context.height),
            execution_state_root: format!("noop-root-{}", context.height),
        })
    }
}

fn decode_http_json_response<T: serde::de::DeserializeOwned>(bytes: &[u8]) -> (u16, T) {
    let boundary = bytes
        .windows(4)
        .position(|window| window == b"\r\n\r\n")
        .expect("response must include HTTP body separator");
    let header = std::str::from_utf8(&bytes[..boundary]).expect("response header utf-8");
    let status = header
        .split_whitespace()
        .nth(1)
        .and_then(|token| token.parse::<u16>().ok())
        .expect("response status code");
    let payload =
        serde_json::from_slice::<T>(&bytes[(boundary + 4)..]).expect("response json payload");
    (status, payload)
}

fn unique_temp_dir(prefix: &str) -> PathBuf {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("duration")
        .as_nanos();
    std::env::temp_dir().join(format!("oasis7-agent-claim-api-{prefix}-{unique}"))
}

fn wait_for_committed_height(runtime: &Arc<Mutex<NodeRuntime>>, minimum_height: u64) {
    let start = Instant::now();
    while start.elapsed() < Duration::from_secs(2) {
        let height = runtime
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .snapshot()
            .consensus
            .committed_height;
        if height >= minimum_height {
            return;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    let height = runtime
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .snapshot()
        .consensus
        .committed_height;
    panic!("timed out waiting for committed height >= {minimum_height}, got {height}");
}

fn started_runtime(node_id: &str) -> Arc<Mutex<NodeRuntime>> {
    let config = NodeConfig::new(node_id, "world-agent-claim-api", NodeRole::Sequencer)
        .expect("node config")
        .with_tick_interval(Duration::from_millis(20))
        .expect("tick interval");
    let mut node_runtime = NodeRuntime::new(config).with_execution_hook(NoopExecutionHook);
    node_runtime.start().expect("start node runtime");
    Arc::new(Mutex::new(node_runtime))
}

fn stop_runtime(runtime: &Arc<Mutex<NodeRuntime>>) {
    runtime
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .stop()
        .expect("stop node runtime");
}

fn register_agent(world: &mut World, agent_id: &str) {
    world.submit_action(Action::RegisterAgent {
        agent_id: agent_id.to_string(),
        pos: GeoPos::new(0, 0, 0),
    });
    world.step().expect("register agent");
}

fn configure_claim_world() -> World {
    let mut world = oasis7::runtime_host::world::new();
    world
        .set_governance_execution_policy(oasis7_runtime::GovernanceExecutionPolicy {
            epoch_length_ticks: 1,
            ..oasis7_runtime::GovernanceExecutionPolicy::default()
        })
        .expect("set governance policy");
    register_agent(&mut world, "alice");
    register_agent(&mut world, "bob");
    world
        .set_agent_reputation_score("alice", 0)
        .expect("set reputation");
    world.set_main_token_supply(MainTokenSupplyState {
        total_supply: 325,
        circulating_supply: 0,
        ..MainTokenSupplyState::default()
    });
    world
}

fn save_world(world: &World, prefix: &str) -> PathBuf {
    let dir = unique_temp_dir(prefix);
    fs::create_dir_all(&dir).expect("create temp dir");
    world.save_to_dir(&dir).expect("save world");
    dir
}

fn auto_funding_claim_world_dir() -> PathBuf {
    let mut world = configure_claim_world();
    world
        .set_main_token_treasury_balance(
            oasis7_runtime::MAIN_TOKEN_TREASURY_BUCKET_RESTRICTED_STARTER_CLAIM_LIVEOPS_POOL,
            325,
        )
        .expect("seed dedicated pool");
    world.step().expect("persist auto-funding world state root");
    save_world(&world, "auto-funding")
}

fn build_http_request(path: &str, method: &str, body: &str) -> Vec<u8> {
    format!(
        "{method} {path} HTTP/1.1\r\nHost: 127.0.0.1:5121\r\nContent-Length: {}\r\n\r\n{}",
        body.len(),
        body
    )
    .into_bytes()
}

#[test]
fn agent_claim_submit_handler_rejects_missing_claimer_in_preflight() {
    let _guard = agent_claim_api_test_guard();
    reset_agent_claim_api_state_for_tests();
    let runtime = Arc::new(Mutex::new(NodeRuntime::new(
        NodeConfig::new(
            "node-agent-claim-bad-submit",
            "world-agent-claim-bad-submit",
            NodeRole::Sequencer,
        )
        .expect("node config"),
    )));
    let world_dir = auto_funding_claim_world_dir();

    let request = ChainAgentClaimSubmitRequest {
        claimer_agent_id: "unknown-agent".to_string(),
        target_agent_id: "bob".to_string(),
    };
    let body = serde_json::to_string(&request).expect("serialize request");
    let http_request = build_http_request("/v1/chain/agent-claim/submit", "POST", &body);
    let (mut server_stream, mut client_stream) = tcp_stream_pair();
    let handled = maybe_handle_agent_claim_request(
        &mut server_stream,
        &http_request,
        &runtime,
        "POST",
        "/v1/chain/agent-claim/submit",
        "/v1/chain/agent-claim/submit",
        DEFAULT_NODE_ID,
        "world-agent-claim-bad-submit",
        world_dir.as_path(),
    )
    .expect("handler should process request");
    assert!(handled);
    drop(server_stream);

    client_stream
        .set_read_timeout(Some(Duration::from_secs(2)))
        .expect("set client timeout");
    let mut response_bytes = Vec::new();
    client_stream
        .read_to_end(&mut response_bytes)
        .expect("read handler response");
    let (status, response): (u16, ChainAgentClaimActionResponse) =
        decode_http_json_response(&response_bytes);
    assert_eq!(status, 409);
    assert!(!response.ok);
    assert_eq!(response.error_code.as_deref(), Some("action_rejected"));
    assert!(
        response
            .error
            .as_deref()
            .is_some_and(|error| error.contains("agent not found"))
    );
}

#[test]
fn agent_claim_submit_handler_previews_slot_1_auto_funding() {
    let _guard = agent_claim_api_test_guard();
    reset_agent_claim_api_state_for_tests();
    let runtime = started_runtime("node-agent-claim-auto-funding");
    let world_dir = auto_funding_claim_world_dir();

    let request = ChainAgentClaimSubmitRequest {
        claimer_agent_id: "alice".to_string(),
        target_agent_id: "bob".to_string(),
    };
    let body = serde_json::to_string(&request).expect("serialize request");
    let http_request = build_http_request("/v1/chain/agent-claim/submit", "POST", &body);
    let (mut server_stream, mut client_stream) = tcp_stream_pair();
    let handled = maybe_handle_agent_claim_request(
        &mut server_stream,
        &http_request,
        &runtime,
        "POST",
        "/v1/chain/agent-claim/submit",
        "/v1/chain/agent-claim/submit",
        DEFAULT_NODE_ID,
        "world-agent-claim-auto-funding",
        world_dir.as_path(),
    )
    .expect("handler should process request");
    assert!(handled);
    drop(server_stream);

    client_stream
        .set_read_timeout(Some(Duration::from_secs(2)))
        .expect("set client timeout");
    let mut response_bytes = Vec::new();
    client_stream
        .read_to_end(&mut response_bytes)
        .expect("read handler response");
    let (status, response): (u16, ChainAgentClaimActionResponse) =
        decode_http_json_response(&response_bytes);
    assert_eq!(status, 200, "{response:?}");
    assert!(response.ok);
    assert_eq!(
        response
            .preview
            .as_ref()
            .and_then(|preview| preview.auto_issued_restricted_amount),
        Some(325)
    );
    assert_eq!(
        response
            .preview
            .as_ref()
            .and_then(|preview| preview.slot_index),
        Some(1)
    );

    wait_for_committed_height(&runtime, 1);
    stop_runtime(&runtime);
}
