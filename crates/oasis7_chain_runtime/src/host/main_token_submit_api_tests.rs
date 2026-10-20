use super::{ChainMainTokenSubmitResponse, maybe_handle_main_token_submit_request};
use ed25519_dalek::SigningKey;
use oasis7::consensus_action_payload::{
    sign_main_token_runtime_action_auth, sign_threshold_main_token_runtime_action_auth,
};
use oasis7_node::{NodeConfig, NodeRole, NodeRuntime};
use oasis7_runtime::{Action, main_token_account_id_from_node_public_key};
use std::io::Read;
use std::net::{TcpListener, TcpStream};
use std::sync::{Arc, Mutex};
use std::time::Duration;

fn tcp_stream_pair() -> (TcpStream, TcpStream) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind loopback listener");
    let bind = listener.local_addr().expect("read local addr");
    let client = TcpStream::connect(bind).expect("connect loopback client");
    let (server, _) = listener.accept().expect("accept loopback connection");
    (server, client)
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

fn test_signer(seed: u8) -> (String, String) {
    let private_key = [seed; 32];
    let signing_key = SigningKey::from_bytes(&private_key);
    (
        hex::encode(signing_key.verifying_key().to_bytes()),
        hex::encode(private_key),
    )
}

#[test]
fn claim_main_token_submit_handler_accepts_signed_request() {
    let runtime = Arc::new(Mutex::new(NodeRuntime::new(
        NodeConfig::new(
            "node-main-token-claim",
            "world-main-token-claim",
            NodeRole::Sequencer,
        )
        .expect("node config"),
    )));
    let (public_key, private_key) = test_signer(31);
    let beneficiary = main_token_account_id_from_node_public_key(public_key.as_str());
    let action = Action::ClaimMainTokenVesting {
        bucket_id: "public_testnet_faucet_genesis".to_string(),
        beneficiary: beneficiary.clone(),
        nonce: 1,
    };
    let proof = sign_main_token_runtime_action_auth(
        &action,
        beneficiary.as_str(),
        public_key.as_str(),
        private_key.as_str(),
    )
    .expect("sign claim action");
    let body = serde_json::json!({
        "action": "claim_main_token_vesting",
        "bucket_id": "public_testnet_faucet_genesis",
        "beneficiary": beneficiary,
        "nonce": 1,
        "auth": proof,
    })
    .to_string();

    let (mut server_stream, mut client_stream) = tcp_stream_pair();
    let request = format!(
        "POST /v1/chain/main-token/submit HTTP/1.1\r\nHost: 127.0.0.1:5121\r\nContent-Length: {}\r\n\r\n{}",
        body.len(),
        body
    );
    let handled = maybe_handle_main_token_submit_request(
        &mut server_stream,
        request.as_bytes(),
        &runtime,
        "POST",
        "/v1/chain/main-token/submit",
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
    let (status, response): (u16, ChainMainTokenSubmitResponse) =
        decode_http_json_response(&response_bytes);
    assert_eq!(status, 200);
    assert!(response.ok);
    assert!(response.action_id.is_some());
}

#[test]
fn initialize_main_token_genesis_submit_handler_accepts_threshold_request() {
    let (public_key_a, private_key_a) = test_signer(41);
    let (public_key_b, private_key_b) = test_signer(42);
    let faucet_account = main_token_account_id_from_node_public_key(public_key_a.as_str());
    let runtime = Arc::new(Mutex::new(NodeRuntime::new(
        NodeConfig::new(
            "node-main-token-genesis",
            "world-main-token-genesis",
            NodeRole::Sequencer,
        )
        .expect("node config")
        .with_main_token_controller_binding(
            oasis7_node::NodeMainTokenControllerBindingConfig::default()
                .with_controller_signer_policy(
                    "msig.genesis.v1",
                    2,
                    vec![public_key_a.clone(), public_key_b.clone()],
                )
                .expect("controller signer policy"),
        )
        .expect("controller binding"),
    )));
    let action = Action::InitializeMainTokenGenesis {
        allocations: vec![oasis7_runtime::MainTokenGenesisAllocationPlan {
            bucket_id: "public_testnet_faucet_genesis".to_string(),
            ratio_bps: 10_000,
            recipient: faucet_account.clone(),
            cliff_epochs: 0,
            linear_unlock_epochs: 0,
            start_epoch: 0,
        }],
    };
    let proof = sign_threshold_main_token_runtime_action_auth(
        &action,
        "msig.genesis.v1",
        2,
        &[
            (public_key_a.as_str(), private_key_a.as_str()),
            (public_key_b.as_str(), private_key_b.as_str()),
        ],
    )
    .expect("sign threshold genesis action");
    let body = serde_json::json!({
        "action": "initialize_main_token_genesis",
        "allocations": [{
            "bucket_id": "public_testnet_faucet_genesis",
            "ratio_bps": 10000,
            "recipient": faucet_account,
            "cliff_epochs": 0,
            "linear_unlock_epochs": 0,
            "start_epoch": 0
        }],
        "auth": proof,
    })
    .to_string();

    let (mut server_stream, mut client_stream) = tcp_stream_pair();
    let request = format!(
        "POST /v1/chain/main-token/submit HTTP/1.1\r\nHost: 127.0.0.1:5121\r\nContent-Length: {}\r\n\r\n{}",
        body.len(),
        body
    );
    let handled = maybe_handle_main_token_submit_request(
        &mut server_stream,
        request.as_bytes(),
        &runtime,
        "POST",
        "/v1/chain/main-token/submit",
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
    let (status, response): (u16, ChainMainTokenSubmitResponse) =
        decode_http_json_response(&response_bytes);
    assert_eq!(status, 200, "{response:?}");
    assert!(response.ok, "{response:?}");
}

#[test]
fn main_token_submit_handler_rejects_invalid_signature() {
    let runtime = Arc::new(Mutex::new(NodeRuntime::new(
        NodeConfig::new(
            "node-main-token-invalid-signature",
            "world-main-token-invalid-signature",
            NodeRole::Sequencer,
        )
        .expect("node config"),
    )));
    let (public_key, _) = test_signer(51);
    let beneficiary = main_token_account_id_from_node_public_key(public_key.as_str());
    let body = serde_json::json!({
        "action": "claim_main_token_vesting",
        "bucket_id": "public_testnet_faucet_genesis",
        "beneficiary": beneficiary,
        "nonce": 1,
        "auth": {
            "scheme": "ed25519",
            "account_id": beneficiary,
            "public_key": public_key,
            "signature": format!("occlaimauth:v1:{}", "f".repeat(128)),
        },
    })
    .to_string();

    let (mut server_stream, mut client_stream) = tcp_stream_pair();
    let request = format!(
        "POST /v1/chain/main-token/submit HTTP/1.1\r\nHost: 127.0.0.1:5121\r\nContent-Length: {}\r\n\r\n{}",
        body.len(),
        body
    );
    maybe_handle_main_token_submit_request(
        &mut server_stream,
        request.as_bytes(),
        &runtime,
        "POST",
        "/v1/chain/main-token/submit",
    )
    .expect("handler should process request");
    drop(server_stream);

    client_stream
        .set_read_timeout(Some(Duration::from_secs(2)))
        .expect("set client timeout");
    let mut response_bytes = Vec::new();
    client_stream
        .read_to_end(&mut response_bytes)
        .expect("read handler response");
    let (status, response): (u16, ChainMainTokenSubmitResponse) =
        decode_http_json_response(&response_bytes);
    assert_eq!(status, 400);
    assert!(!response.ok);
    assert_eq!(response.error_code.as_deref(), Some("invalid_signature"));
}
