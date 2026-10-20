use super::{
    ChainModuleReleaseAttestationSubmitResponse,
    build_module_release_attestation_submit_action_payload,
    maybe_handle_module_release_attestation_submit_request,
    parse_module_release_attestation_submit_request,
};
use oasis7::consensus_action_payload::{
    ConsensusActionPayloadBody, decode_consensus_action_payload,
};
use oasis7_node::{
    NodeConfig, NodeExecutionCommitContext, NodeExecutionCommitResult, NodeExecutionHook, NodeRole,
    NodeRuntime,
};
use oasis7_runtime::Action;
use std::io::Read;
use std::net::{TcpListener, TcpStream};
use std::sync::atomic::Ordering;
use std::sync::{Arc, Mutex};
use std::time::Duration;

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

fn reset_module_release_attestation_submit_state_for_tests() {
    super::NEXT_MODULE_RELEASE_ATTESTATION_ACTION_ID.store(1, Ordering::Relaxed);
}

#[test]
fn parse_module_release_attestation_submit_request_normalizes_fields() {
    let request = parse_module_release_attestation_submit_request(
        br#"{
          "operator_agent_id":" operator-1 ",
          "request_id":7,
          "signer_node_id":" signer-1 ",
          "platform":" Darwin-ARM64 ",
          "build_manifest_hash":"AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA",
          "source_hash":"BBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBB",
          "wasm_hash":"CCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCC",
          "proof_cid":" sha256:proof ",
          "builder_image_digest":"SHA256:DDDDDDDDDDDDDDDDDDDDDDDDDDDDDDDDDDDDDDDDDDDDDDDDDDDDDDDDDDDDDDDD",
          "container_platform":" linux-x86_64 ",
          "canonicalizer_version":" strip-custom-sections-v1 "
        }"#,
    )
    .expect("request should parse");
    assert_eq!(request.operator_agent_id, "operator-1");
    assert_eq!(request.request_id, 7);
    assert_eq!(request.signer_node_id, "signer-1");
    assert_eq!(request.platform, "darwin-arm64");
    assert_eq!(
        request.build_manifest_hash,
        "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
    );
    assert_eq!(
        request.builder_image_digest,
        "sha256:dddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddd"
    );
    assert_eq!(request.proof_cid, "sha256:proof");
    assert_eq!(request.container_platform, "linux-x86_64");
    assert_eq!(request.canonicalizer_version, "strip-custom-sections-v1");
}

#[test]
fn build_module_release_attestation_submit_action_payload_encodes_runtime_action() {
    let request = parse_module_release_attestation_submit_request(
        br#"{
          "operator_agent_id":"operator-1",
          "request_id":9,
          "signer_node_id":"attestor-node-1",
          "platform":"linux-x86_64",
          "build_manifest_hash":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
          "source_hash":"bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
          "wasm_hash":"cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc",
          "proof_cid":"sha256:proof",
          "builder_image_digest":"sha256:dddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddd",
          "container_platform":"linux-x86_64",
          "canonicalizer_version":"strip-custom-sections-v1"
        }"#,
    )
    .expect("request should parse");
    let payload =
        build_module_release_attestation_submit_action_payload(&request).expect("payload");
    let decoded = decode_consensus_action_payload(payload.as_slice()).expect("decode payload");
    match decoded {
        ConsensusActionPayloadBody::RuntimeAction { action } => match action {
            Action::ModuleReleaseSubmitAttestation {
                operator_agent_id,
                request_id,
                signer_node_id,
                platform,
                build_manifest_hash,
                source_hash,
                wasm_hash,
                proof_cid,
                builder_image_digest,
                container_platform,
                canonicalizer_version,
            } => {
                assert_eq!(operator_agent_id, "operator-1");
                assert_eq!(request_id, 9);
                assert_eq!(signer_node_id, "attestor-node-1");
                assert_eq!(platform, "linux-x86_64");
                assert_eq!(
                    build_manifest_hash,
                    "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
                );
                assert_eq!(
                    source_hash,
                    "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"
                );
                assert_eq!(
                    wasm_hash,
                    "cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc"
                );
                assert_eq!(proof_cid, "sha256:proof");
                assert_eq!(
                    builder_image_digest,
                    "sha256:dddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddd"
                );
                assert_eq!(container_platform, "linux-x86_64");
                assert_eq!(canonicalizer_version, "strip-custom-sections-v1");
            }
            other => panic!("expected ModuleReleaseSubmitAttestation action, got {other:?}"),
        },
        other => panic!("expected runtime action payload, got {other:?}"),
    }
}

#[test]
fn module_release_attestation_submit_handler_rejects_bad_payload() {
    reset_module_release_attestation_submit_state_for_tests();
    let runtime = Arc::new(Mutex::new(NodeRuntime::new(
        NodeConfig::new(
            "node-module-release-attestation-bad",
            "world-module-release-attestation-bad",
            NodeRole::Sequencer,
        )
        .expect("node config"),
    )));

    let (mut server_stream, mut client_stream) = tcp_stream_pair();
    let body = r#"{"operator_agent_id":"","request_id":0}"#;
    let request = format!(
        "POST /v1/chain/module-release/attestation/submit HTTP/1.1\r\nHost: 127.0.0.1:5121\r\nContent-Length: {}\r\n\r\n{}",
        body.len(),
        body
    );
    let handled = maybe_handle_module_release_attestation_submit_request(
        &mut server_stream,
        request.as_bytes(),
        &runtime,
        "POST",
        "/v1/chain/module-release/attestation/submit",
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
    let (status, response): (u16, ChainModuleReleaseAttestationSubmitResponse) =
        decode_http_json_response(&response_bytes);
    assert_eq!(status, 400);
    assert!(!response.ok);
    assert_eq!(response.error_code.as_deref(), Some("invalid_request"));
}

#[test]
fn module_release_attestation_submit_handler_accepts_valid_payload() {
    reset_module_release_attestation_submit_state_for_tests();
    let config = NodeConfig::new(
        "node-module-release-attestation-ok",
        "world-module-release-attestation-ok",
        NodeRole::Sequencer,
    )
    .expect("node config");
    let mut node_runtime = NodeRuntime::new(config).with_execution_hook(NoopExecutionHook);
    node_runtime.start().expect("start node runtime");
    let runtime = Arc::new(Mutex::new(node_runtime));

    let (mut server_stream, mut client_stream) = tcp_stream_pair();
    let body = r#"{
      "operator_agent_id":"operator-1",
      "request_id":17,
      "signer_node_id":"attestor-node-1",
      "platform":"linux-x86_64",
      "build_manifest_hash":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
      "source_hash":"bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
      "wasm_hash":"cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc",
      "proof_cid":"sha256:proof",
      "builder_image_digest":"sha256:dddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddd",
      "container_platform":"linux-x86_64",
      "canonicalizer_version":"strip-custom-sections-v1"
    }"#;
    let request = format!(
        "POST /v1/chain/module-release/attestation/submit HTTP/1.1\r\nHost: 127.0.0.1:5121\r\nContent-Length: {}\r\n\r\n{}",
        body.len(),
        body
    );
    let handled = maybe_handle_module_release_attestation_submit_request(
        &mut server_stream,
        request.as_bytes(),
        &runtime,
        "POST",
        "/v1/chain/module-release/attestation/submit",
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
    let (status, response): (u16, ChainModuleReleaseAttestationSubmitResponse) =
        decode_http_json_response(&response_bytes);
    assert_eq!(status, 200);
    assert!(response.ok);
    assert_eq!(response.action_id, Some(1));
    assert!(response.submitted_at_unix_ms.is_some());

    runtime
        .lock()
        .expect("lock runtime for stop")
        .stop()
        .expect("stop node runtime");
}
