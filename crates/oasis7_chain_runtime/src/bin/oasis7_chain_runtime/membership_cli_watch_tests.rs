//! Real child-process CLI polling over authenticated TCP. This exercises the
//! explicitly scoped engineering assertion path, not formal deployment proof.
use super::*;
use ed25519_dalek::SigningKey;
use oasis7::world_service::{
    Capability, DescribeWorldResponse, ExecutionBinding, MembershipReadEnvelope, SignedReadRequest,
    SignedServiceResponse, WorldAvailability, WorldIdentity, membership, verified_view,
};
use oasis7_runtime::{WorldGovernanceMembershipView, WorldState};
use std::{
    io::{Read, Write},
    net::{TcpListener, TcpStream},
    process::{Command, Output},
    thread,
};
pub(super) const CHILD_ENV: &str = "OASIS7_TEST_MEMBERSHIP_WATCH_CHILD";
pub(super) const CHILD_FILTER: &str = "membership_cli::tests::watch_tcp::watch_cli_child_entry";
fn key(n: u8) -> String {
    hex::encode([n; 32])
}
fn public(n: u8) -> String {
    hex::encode(SigningKey::from_bytes(&[n; 32]).verifying_key().to_bytes())
}

#[test]
fn watch_cli_child_entry() {
    let Ok(encoded) = std::env::var(CHILD_ENV) else {
        return;
    };
    let arguments: Vec<String> = serde_json::from_str(&encoded).unwrap();
    // Enter the production CLI dispatcher inside an actual OS child process.
    // No helper prints an observation or supplies a pre-verified value.
    super::super::super::identity_receipt::dispatch(arguments.iter().map(String::as_str))
        .expect("validator CLI dispatch")
        .unwrap_or_else(|error| panic!("watch child exited: {error}"));
}
fn committed(position: u64, signer: u8) -> ReadMembershipResponse {
    let state = WorldState {
        governance_finality_signer_registry: Some(
            oasis7_runtime::GovernanceFinalitySignerRegistry {
                slot_id: "watch-slot".into(),
                threshold: 1,
                threshold_bps: 0,
                signer_bindings: BTreeMap::from([("node".into(), public(signer))]),
                validator_stakes: BTreeMap::from([("node".into(), 100)]),
            },
        ),
        ..WorldState::default()
    };
    let mut world = oasis7::runtime_host::world::new_with_state(state);
    world
        .bind_cognition_runtime("watch-world", "main", 0, None, "pending", 0)
        .unwrap();
    let binding = world.current_cognition_runtime_binding().unwrap();
    let commit = CommitRef {
        world: WorldIdentity {
            world_id: "watch-world".into(),
            genesis_digest: "watch-genesis".into(),
        },
        binding: ExecutionBinding {
            provider_world_id: "watch-world".into(),
            branch_id: "main".into(),
            finality_ref: authority::request_digest(
                "finality",
                &(
                    binding.finality_epoch,
                    &binding.finality_status,
                    &binding.finality_block_hash,
                ),
            )
            .unwrap(),
            reorg_generation: 0,
            governing_manifest_ref: binding.runtime_manifest_hash.to_string(),
            authority_generation: 0,
            permission_generation: 0,
        },
        position,
        execution_block_hash: key(position as u8),
        state_root_ref: format!("committed-cas-{position}"),
    };
    let scope = verified_view::CONTROLLED_PREREQUISITE_READ_SCOPE;
    let view = WorldGovernanceMembershipView::read(
        &world,
        &commit,
        &WorldGovernanceMembershipView::world_state_root_hash(&world).unwrap(),
        scope,
    )
    .unwrap();
    membership::public_response(view, Some(scope.into()))
}
pub(super) fn receive(stream: &mut TcpStream, path: &str) -> SignedReadRequest<Value> {
    stream
        .set_read_timeout(Some(Duration::from_secs(5)))
        .unwrap();
    let mut bytes = Vec::new();
    let (offset, length) = loop {
        let mut part = [0; 4096];
        let n = stream.read(&mut part).unwrap();
        assert!(n > 0);
        bytes.extend_from_slice(&part[..n]);
        if let Some(end) = bytes.windows(4).position(|part| part == b"\r\n\r\n") {
            let header = String::from_utf8_lossy(&bytes[..end]).to_lowercase();
            assert!(header.starts_with(&format!("post {path} ")));
            let length: usize = header
                .lines()
                .find_map(|line| line.strip_prefix("content-length:"))
                .unwrap()
                .trim()
                .parse()
                .unwrap();
            if bytes.len() >= end + 4 + length {
                break (end + 4, length);
            }
        }
        assert!(bytes.len() < 64 * 1024, "bounded diagnostic request");
    };
    let signed = serde_json::from_slice(&bytes[offset..offset + length]).unwrap();
    authority::verify_read_request(path, &signed).unwrap();
    signed
}
pub(super) fn send(stream: &mut TcpStream, response: &SignedServiceResponse<Value>) {
    let body = serde_json::to_vec(response).unwrap();
    write!(
        stream,
        "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        body.len()
    )
    .unwrap();
    stream.write_all(&body).unwrap();
}
fn describe(listener: &TcpListener, response: &ReadMembershipResponse) {
    let (mut stream, _) = listener.accept().unwrap();
    let signed = receive(&mut stream, "/v1/world/describe");
    let payload = DescribeWorldResponse {
        formal_profile: None,
        execution_evidence_scope: response.execution_evidence_scope.clone(),
        contract_version: 1,
        world: response.commit.world.clone(),
        binding: response.commit.binding.clone(),
        capabilities: vec![Capability::GovernanceMembership],
        availability: WorldAvailability {
            readable: true,
            writable: false,
            recovering: false,
            reason: None,
            retry_after_ms: None,
        },
        current_view: None,
    };
    let reply = authority::sign_service_response(
        "/v1/world/describe",
        authority::request_digest("/v1/world/describe", &signed).unwrap(),
        serde_json::to_value(payload).unwrap(),
        &key(9),
    )
    .unwrap();
    send(&mut stream, &reply);
}
fn child(endpoint: &str, cursor: Option<&CommitRef>) -> Output {
    let mut args = vec![
        "validator".to_string(),
        "watch".into(),
        "--node-id".into(),
        "node".into(),
        "--allow-controlled-prerequisite".into(),
        "--timeout-ms".into(),
        "5000".into(),
        "--interval-ms".into(),
        "100".into(),
    ];
    if let Some(cursor) = cursor {
        args.extend(["--cursor".into(), serde_json::to_string(cursor).unwrap()]);
    }
    Command::new(std::env::current_exe().unwrap())
        .args(["--exact", CHILD_FILTER, "--nocapture", "--test-threads=1"])
        .env(CHILD_ENV, serde_json::to_string(&args).unwrap())
        .env("OASIS7_WORLD_SERVICE_ENDPOINT", endpoint)
        .env("OASIS7_WORLD_SERVICE_PUBLIC_KEY", public(9))
        .env("OASIS7_WORLD_SERVICE_WORLD_ID", "watch-world")
        .env("OASIS7_WORLD_SERVICE_GENESIS_DIGEST", "watch-genesis")
        .env("OASIS7_WORLD_SERVICE_SCOPE", "public")
        .env("OASIS7_WORLD_SERVICE_READ_PRIVATE_KEY", key(7))
        .env_remove("OASIS7_WORLD_SERVICE_FORMAL_TRUST_FILE")
        .output()
        .unwrap()
}
pub(super) fn observations(output: &Output) -> Vec<Value> {
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter_map(|line| {
            let start = line.find('{')?;
            let value: Value = serde_json::from_str(&line[start..]).ok()?;
            value.get("cursor").map(|_| value.clone())
        })
        .collect()
}

#[test]
fn engineering_validator_watch_disconnect_cursor_restart_rejects_replay_and_lower_commit() {
    let first = committed(7, 4);
    let next = committed(8, 5);
    let below = committed(6, 4);
    let cursor = first.commit.clone();
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let endpoint = format!("http://{}", listener.local_addr().unwrap());
    let expected = cursor.clone();
    let server = thread::spawn(move || {
        let mut replay = None;
        let mut nonces = Vec::new();
        for round in 0..4 {
            let response = if round == 2 {
                &below
            } else if round == 3 {
                &next
            } else {
                &first
            };
            describe(&listener, response);
            let (mut stream, _) = listener.accept().unwrap();
            let signed = receive(&mut stream, "/v1/world/governance-membership");
            let query: MembershipReadEnvelope =
                serde_json::from_value(signed.request.clone()).unwrap();
            query.validate().unwrap();
            assert_eq!(
                query.query.min_commit,
                (round > 0).then(|| expected.clone())
            );
            assert!(!nonces.contains(&query.observation_nonce));
            nonces.push(query.observation_nonce);
            let reply = authority::sign_service_response(
                "/v1/world/governance-membership",
                authority::request_digest("/v1/world/governance-membership", &signed).unwrap(),
                serde_json::to_value(response).unwrap(),
                &key(9),
            )
            .unwrap();
            if round == 0 {
                replay = Some(reply.clone());
            }
            send(
                &mut stream,
                if round == 1 {
                    replay.as_ref().unwrap()
                } else {
                    &reply
                },
            );
            drop(stream);
            if round == 0 || round == 3 {
                // Next poll fails at a real socket disconnect, after exactly one
                // authenticated committed observation was emitted.
                let (mut stream, _) = listener.accept().unwrap();
                receive(&mut stream, "/v1/world/describe");
                drop(stream);
            }
        }
    });
    let stopped = child(&endpoint, None);
    assert!(!stopped.status.success());
    let first_output = observations(&stopped);
    assert_eq!(
        first_output.len(),
        1,
        "{}",
        String::from_utf8_lossy(&stopped.stdout)
    );
    let persisted: CommitRef = serde_json::from_value(first_output[0]["cursor"].clone()).unwrap();
    assert_eq!(persisted, cursor);
    let replay = child(&endpoint, Some(&persisted));
    assert!(!replay.status.success());
    assert!(observations(&replay).is_empty());
    assert!(String::from_utf8_lossy(&replay.stderr).contains("response request mismatch"));
    let lower = child(&endpoint, Some(&persisted));
    assert!(!lower.status.success());
    assert!(observations(&lower).is_empty());
    assert!(String::from_utf8_lossy(&lower.stderr).contains("precedes minimum"));
    let resumed = child(&endpoint, Some(&persisted));
    assert!(!resumed.status.success());
    let resumed_output = observations(&resumed);
    assert_eq!(resumed_output.len(), 1);
    assert_eq!(resumed_output[0]["cursor"]["position"], 8);
    assert_eq!(
        resumed_output[0]["first_observed_in_this_run"]["position"],
        8
    );
    assert!(resumed_output[0]["actual_first_effective_commit"].is_null());
    assert!(resumed_output[0]["applied_consensus_membership"].is_null());
    assert_eq!(
        resumed_output[0]["assurance"],
        "authenticated_service_assertion_not_independent_consensus_proof"
    );
    server.join().unwrap();
}
