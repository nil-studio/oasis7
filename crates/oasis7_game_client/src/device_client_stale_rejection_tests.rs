use super::*;
use oasis7_agent_api::{
    game::{GameTaskControlV1, GameTaskFenceV1},
    world_action::{Action, GeoPos},
    world_cognition::{RuntimeCognitionCommitRequestV1, RuntimeCognitionResponseArtifactV1},
};
use oasis7_client_api::world_service_wire::{AuthenticatedLookup, CognitionIntentV1};
use std::{
    io::{Read, Write},
    net::{TcpListener, TcpStream},
    thread,
};
fn digest(byte: char) -> String {
    format!("blake3:{}", byte.to_string().repeat(64))
}
fn control() -> GameTaskControlV1 {
    let public = authority::sign_read_request("key", (), &hex::encode([7; 32]))
        .unwrap()
        .subject_public_key;
    serde_json::from_value(json!({"schema_revision":1,"task_id":"task","world_id":"world","genesis_digest":"genesis","branch_id":"main","reorg_epoch":"0","agent_id":"a","owner_binding":"owner","owner_generation":"1","owner_public_key":public,"task_revision":"3","task_status":"active","latest_goal_revision":"1","active_goal_revision":"1","active_goal_reference":{"goal_revision":"1","goal_ref":"cd".repeat(32),"prepare_operation_id":"goal","publish_operation_id":"publish","activation_committed_height":"2"},"executor_epoch":"1","executor_id":"executor","executor_binding":{"executor_id":"executor","executor_public_key":public,"delegation_generation":"1","executor_epoch":"1"},"created_operation_id":"create"})).unwrap()
}
fn action(control: &GameTaskControlV1) -> WorldServicePayloadV1 {
    let request=RuntimeCognitionCommitRequestV1{agent_id:"a".into(),agent_session_id:"session".into(),agent_turn_id:"turn".into(),decision_request_id:"request".into(),retry_seq:1,transport_attempt:1,request_digest:digest('1'),observation_digest:digest('2'),context_digest:digest('3'),capability_snapshot_hash:digest('4'),authority_context_hash:digest('5'),captured_base_binding:serde_json::from_value(json!({"world_id":"world","branch_id":"main","finality_epoch":0,"finality_block_hash":null,"finality_status":"pending","base_tick":0,"base_world_hash":digest('6'),"reorg_epoch":0,"runtime_manifest_hash":digest('7')})).unwrap()};
    let mut response_artifact = RuntimeCognitionResponseArtifactV1 {
        schema_version: 1,
        context_discriminator: RuntimeCognitionResponseArtifactV1::CONTEXT_DISCRIMINATOR.into(),
        context_version: 1,
        agent_session_id: request.agent_session_id.clone(),
        agent_turn_id: request.agent_turn_id.clone(),
        decision_request_id: request.decision_request_id.clone(),
        retry_seq: 1,
        transport_attempt: 1,
        request_digest: request.request_digest.clone(),
        response_digest: digest('8'),
        artifact_digest: String::new(),
    };
    response_artifact.refresh_artifact_digest();
    WorldServicePayloadV1::Cognition(
        authority::sign_read_request(
            "cognition",
            CognitionIntentV1 {
                request,
                action: Action::MoveAgent {
                    agent_id: "a".into(),
                    to: GeoPos::new(0, 0, 0),
                },
                response_artifact,
                delegation_generation: 1,
                causal_proposal: None,
                task_fence: Some(GameTaskFenceV1::from_control(control)),
            },
            &hex::encode([7; 32]),
        )
        .unwrap(),
    )
}
fn record() -> Value {
    json!({"feedback":{"feedback_id":"stale-f1","feedback_seq":1,"agent_subject":"a","agent_session_id":"session","agent_turn_id":"turn","decision_request_id":"request","candidate_action_id":null,"runtime_receipt_id":null,"status":"rejected","request_digest":digest('1'),"reject_reason":"stale_base","provenance":"runtime_authoritative"},"original_envelope_digest":digest('9'),"delivery_state":"pending","receipt_lineage":null})
}
fn read(stream: &mut TcpStream) -> (String, Vec<u8>) {
    stream.set_nonblocking(false).unwrap();
    stream
        .set_read_timeout(Some(Duration::from_secs(2)))
        .unwrap();
    let mut head = Vec::new();
    while !head.ends_with(b"\r\n\r\n") {
        let mut b = [0];
        stream.read_exact(&mut b).unwrap();
        head.push(b[0]);
    }
    let head = String::from_utf8(head).unwrap();
    let path = head.split_whitespace().nth(1).unwrap().to_owned();
    let len: usize = head
        .lines()
        .find_map(|line| {
            let (name, v) = line.split_once(':')?;
            name.eq_ignore_ascii_case("content-length")
                .then(|| v.trim().parse().unwrap())
        })
        .unwrap();
    let mut body = vec![0; len];
    stream.read_exact(&mut body).unwrap();
    (path, body)
}
fn check(
    change: impl FnOnce(&mut Value),
    outcome: IntentOutcome<Value>,
    wrong_service: bool,
) -> (Result<Option<AuthorizedFeedbackRecord>, String>, usize) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let mut config = crate::tests::config(format!("http://{}", listener.local_addr().unwrap()));
    config.max_response_bytes = 128 * 1024;
    let root = std::env::temp_dir().canonicalize().unwrap().join(format!(
        "oasis7-stale-feedback-{}",
        hex::encode({
            let mut r = [0; 16];
            getrandom::fill(&mut r).unwrap();
            r
        })
    ));
    let public = authority::sign_read_request("key", (), &hex::encode([7; 32]))
        .unwrap()
        .subject_public_key;
    let client = Client {
        remote: crate::WorldServiceTaskClient::new(config).unwrap(),
        world: WorldIdentity {
            world_id: "world".into(),
            genesis_digest: "genesis".into(),
        },
        agent: "a".into(),
        public_key: public,
        key_hex: hex::encode([7; 32]).into(),
        #[cfg(unix)]
        gateway: None,
        state: crate::operation_store::OperationStore::open(&root, [13; 32]).unwrap(),
    };
    let control = control();
    let original = client
        .cache_intent(ClientStateKind::Action, "operation", action(&control))
        .unwrap();
    let (_, mut view) = crate::verified_task_view::tests::fixture("agent:a");
    view.view["game_task_control"] = serde_json::to_value(control).unwrap();
    view.view["feedback_history"] = json!({"agent_id":"a","records":[record()]});
    change(&mut view.view);
    let terminal = terminal_original_rejection(&outcome);
    let payload = IntentResponse {
        formal_profile: None,
        execution_evidence_scope: None,
        contract_version: 1,
        correlation: original.correlation.clone(),
        outcome,
    };
    let expected = original.clone();
    let server = thread::spawn(move || {
        let mut count = 0;
        let deadline = std::time::Instant::now() + Duration::from_secs(3);
        while count < if terminal { 2 } else { 1 } {
            let (mut stream, _) = loop {
                match listener.accept() {
                    Ok(s) => break s,
                    Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                        if std::time::Instant::now() >= deadline {
                            return count;
                        }
                        thread::sleep(Duration::from_millis(2))
                    }
                    Err(e) => panic!("{e}"),
                }
            };
            let (path, body) = read(&mut stream);
            let (digest, payload) = if count == 0 {
                assert_eq!(path, LOOKUP_PATH);
                let lookup: AuthenticatedLookup = serde_json::from_slice(&body).unwrap();
                assert_eq!(lookup.original, expected.signed_payload);
                assert_eq!(lookup.request.key, expected.correlation.key);
                (
                    authority::request_digest(LOOKUP_PATH, &lookup).unwrap(),
                    serde_json::to_value(&payload).unwrap(),
                )
            } else {
                assert_eq!(path, VIEW_JSON_PATH);
                let proof: SignedReadRequest<ReadWorldViewRequest> =
                    serde_json::from_slice(&body).unwrap();
                authority::verify_read_request(VIEW_JSON_PATH, &proof).unwrap();
                assert_eq!(proof.request.world, expected.correlation.key.world);
                (
                    authority::request_digest(VIEW_JSON_PATH, &proof).unwrap(),
                    serde_json::to_value(&view).unwrap(),
                )
            };
            let signer = if wrong_service && count == 1 {
                [10; 32]
            } else {
                [9; 32]
            };
            let signed =
                authority::sign_service_response(&path, digest, payload, &hex::encode(signer))
                    .unwrap();
            let bytes = serde_json::to_vec(&signed).unwrap();
            write!(stream,"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",bytes.len()).unwrap();
            stream.write_all(&bytes).unwrap();
            count += 1;
        }
        count
    });
    let result = client.known_stale_rejection("operation");
    let unchanged = client
        .cached_intent(ClientStateKind::Action, "operation")
        .unwrap()
        .unwrap();
    assert_eq!(
        unchanged, original,
        "feedback lookup must not replace or resign original action"
    );
    let count = server.join().unwrap();
    drop(client);
    std::fs::remove_dir_all(root).unwrap();
    (result, count)
}
fn rejected() -> IntentOutcome<Value> {
    IntentOutcome::Rejected {
        reason: WorldServiceErrorKind::Conflict,
    }
}
#[test]
fn signed_terminal_original_and_typed_scoped_stale_feedback_allow_new_decision() {
    let (result, calls) = check(|_| {}, rejected(), false);
    let record = result.unwrap().unwrap();
    assert_eq!(calls, 2);
    assert_eq!(record.feedback.feedback_id, "stale-f1");
    assert_eq!(record.original_envelope_digest, digest('9'));
    assert!(record.receipt_lineage.is_none());
}
#[test]
fn missing_history_wrong_call_identity_and_other_rejection_never_authorize_redecision() {
    for field in [
        "agent_session_id",
        "agent_turn_id",
        "decision_request_id",
        "request_digest",
        "reject_reason",
        "status",
    ] {
        let (result, _) = check(
            |view| {
                view["feedback_history"]["records"][0]["feedback"][field] = json!(match field {
                    "request_digest" => digest('a'),
                    "reject_reason" => "authority_denied".into(),
                    "status" => "failed".into(),
                    _ => "other".into(),
                });
            },
            rejected(),
            false,
        );
        assert!(result.is_err() || result.unwrap().is_none(), "{field}");
    }
    let (result, _) = check(
        |view| view["feedback_history"]["records"] = json!([]),
        rejected(),
        false,
    );
    assert!(result.unwrap().is_none());
}
#[test]
fn wrong_subject_digest_lineage_fence_and_service_fail_closed() {
    for case in 0..5 {
        let (result, _) = check(
            |view| match case {
                0 => {
                    view["feedback_history"]["records"][0]["feedback"]["agent_subject"] =
                        json!("other")
                }
                1 => {
                    view["feedback_history"]["records"][0]["original_envelope_digest"] =
                        json!("forged")
                }
                2 => {
                    view["feedback_history"]["records"][0]["receipt_lineage"] =
                        json!({"schema_version":"wrong"})
                }
                3 => view["game_task_control"]["task_revision"] = json!("4"),
                _ => {}
            },
            rejected(),
            case == 4,
        );
        assert!(result.is_err(), "case {case}");
    }
}
#[test]
fn nonterminal_and_noncanonical_outcomes_cannot_select_feedback() {
    for outcome in [
        IntentOutcome::<Value>::Unknown,
        IntentOutcome::Pending,
        IntentOutcome::Received {
            durability: AdmissionDurability::Durable,
        },
        IntentOutcome::Expired,
        IntentOutcome::HistoryUnavailable,
        IntentOutcome::Rejected {
            reason: WorldServiceErrorKind::Unauthorized,
        },
    ] {
        assert!(!terminal_original_rejection(&outcome));
    }
    let (result, calls) = check(|_| {}, IntentOutcome::HistoryUnavailable, false);
    assert!(result.unwrap().is_none());
    assert_eq!(calls, 1);
}

#[test]
fn committed_receipt_json_flags_and_ambiguous_feedback_do_not_trigger_redecision() {
    let (_, view) = crate::verified_task_view::tests::fixture("agent:a");
    let (result, calls) = check(
        |_| {},
        IntentOutcome::Committed {
            commit: Box::new(view.version.commit),
            receipt: json!({"rejected":"stale_base"}),
        },
        false,
    );
    assert!(result.unwrap().is_none());
    assert_eq!(calls, 1);
    let (result, _) = check(
        |view| {
            let mut second = record();
            second["feedback"]["feedback_id"] = json!("stale-f2");
            second["feedback"]["feedback_seq"] = json!(2);
            view["feedback_history"]["records"]
                .as_array_mut()
                .unwrap()
                .push(second);
        },
        rejected(),
        false,
    );
    assert!(result.is_err());
}
