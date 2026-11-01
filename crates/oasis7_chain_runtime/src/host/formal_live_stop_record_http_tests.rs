//! Actual TCP/parser/fixed-route rejection only; not a successful paid query.
use super::*;
use oasis7_client_api::world_service::task_stop_record_control_v2::TASK_STOP_RECORD_CONTROL_V2;
use std::io::{Read, Write};
#[test]
fn formal_http_stop_record_control_v2_rejects_invalid_and_unready_without_business_history() {
    let (world, trust, anchor) = fixture_trust();
    let before = world.snapshot();
    let node = live_node(&anchor.head().record().world.world_id);
    let resolver = Arc::new(CurrentOnlyResolver::new(world.clone(), anchor.clone()));
    let (service, state, signer) = live_service(
        world.clone(),
        anchor.clone(),
        trust,
        resolver.clone(),
        node.clone(),
        published(world, anchor),
    );
    let body = b"{}";
    for ready in [true, false] {
        state.ready.store(ready, Ordering::Release);
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let mut client = TcpStream::connect(listener.local_addr().unwrap()).unwrap();
        client
            .set_read_timeout(Some(Duration::from_secs(5)))
            .unwrap();
        let (mut accepted, _) = listener.accept().unwrap();
        write!(client, "POST {} HTTP/1.1\r\nHost: localhost\r\nContent-Length: {}\r\nConnection: close\r\n\r\n", TASK_STOP_RECORD_CONTROL_V2, body.len()).unwrap();
        client.write_all(body).unwrap();
        http::serve_connection(&service, &mut accepted).unwrap();
        drop(accepted);
        let mut raw = Vec::new();
        client.read_to_end(&mut raw).unwrap();
        assert!(raw.starts_with(b"HTTP/1.1 400"));
        let start = raw.windows(4).position(|v| v == b"\r\n\r\n").unwrap() + 4;
        let response: SignedServiceResponse<FormalServiceErrorV1> =
            serde_json::from_slice(&raw[start..]).unwrap();
        verify_service_response(
            TASK_STOP_RECORD_CONTROL_V2,
            &authority::request_digest(TASK_STOP_RECORD_CONTROL_V2, &body.as_slice()).unwrap(),
            &response,
            &hex::encode(signer.verifying_key().to_bytes()),
        )
        .unwrap();
        assert_eq!(response.payload.code, "stop_observation_unavailable");
        assert_ne!(response.payload.code, "not_found");
        assert_eq!(resolver.business_reads.load(Ordering::SeqCst), 0);
        assert_eq!(resolver.world.lock().unwrap().snapshot(), before);
    }
    node.lock().unwrap().stop().unwrap();
}

use crate::formal_live_service::task_cleanup_fixture as actual_task_fixture;

fn http_roundtrip(service: &FormalLiveService, body: &[u8]) -> Vec<u8> {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let mut client = TcpStream::connect(listener.local_addr().unwrap()).unwrap();
    client
        .set_read_timeout(Some(Duration::from_secs(5)))
        .unwrap();
    let (mut accepted, _) = listener.accept().unwrap();
    write!(
        client,
        "POST {} HTTP/1.1\r\nHost: localhost\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        TASK_STOP_RECORD_CONTROL_V2,
        body.len()
    )
    .unwrap();
    client.write_all(body).unwrap();
    http::serve_connection(service, &mut accepted).unwrap();
    drop(accepted);
    let mut raw = Vec::new();
    client.read_to_end(&mut raw).unwrap();
    raw
}

#[test]
fn formal_http_stop_record_control_v2_genuine_current_then_actual_delivery_expiry() {
    use oasis7_agent_api::game::*;
    use oasis7_client_api::world_service::{task_stop_record_control_v2::*, task_stop_v2::*};
    let (world, close, owner) = actual_task_fixture::fixture();
    let (mut world, _, _, _) =
        fixtures::actual_world_initial_fixture_from_world(world, fixtures::h(50));
    let now = now_unix_ms();
    let p = close.request;
    let mut stop = GameTaskStopV2 {
        api_revision: 2,
        client_operation_id: "http-observed-stop".into(),
        custody_account_id: p.custody_account_id,
        termination_plan_digest: p.termination_plan_digest,
        initial_fence: p.permitted_branches.exact_initial_active,
        target_executor_public_key: p.signer_public_key,
        target_executor_generation: p.signer_generation,
        target_executor_nonce: p.signer_nonce,
        execute_until_ms: DecimalU64(now + 10_000),
        observation_grant: None,
    };
    let current_until = now + 2_000;
    stop.observation_grant = Some(
        StopObservationGrantV2::known_control_for_exact_target(
            &stop,
            now + 20_000,
            now + 15_000,
            current_until,
        )
        .unwrap(),
    );
    let signed = sign_read_request(GAME_TASK_STOP_DOMAIN_V2, stop, &owner).unwrap();
    let original = SignedGameTaskStopV2 {
        request: signed.request,
        subject_public_key: signed.subject_public_key,
        signature_hex: signed.signature_hex,
    };
    let context = actual_task_fixture::begin(&mut world, i64::try_from(now).unwrap());
    let record = world.apply_guarded_task_stop_v2(&original).unwrap();
    world.finish_formal_administrative_tick(&context).unwrap();
    let (world, initial_trust, anchor, _) =
        fixtures::actual_world_initial_fixture_from_world(world, fixtures::h(50));
    let checkpoint = anchor.checkpoint();
    let trust = oasis7::world_service::client::FormalReadTrustConfigV1 {
        initial_trust,
        checkpoint_digest: checkpoint.digest().unwrap(),
        checkpoint,
    };
    let q = StopRecordControlRequestV2 {
        api_revision: 2,
        original: TaskStopSubmitV2 {
            api_revision: 2,
            expected_world: identity(&anchor),
            original,
            observation_nonce: "11".repeat(32),
        },
        source: StopRecordSourceV2 {
            approved_input_digest: "22".repeat(32),
            session_id: "session".into(),
            invocation_id: "owned-invocation".into(),
            source_operation_id: "source".into(),
            source_nonce: "33".repeat(32),
            world_attempt_id: "world-attempt".into(),
        },
    };
    let body = serde_json::to_vec(&q).unwrap();
    let node = live_node(&anchor.head().record().world.world_id);
    let resolver = Arc::new(CurrentOnlyResolver::new(world.clone(), anchor.clone()));
    let publication = published(world.clone(), anchor.clone());
    let (service, _, signer) = live_service(
        world.clone(),
        anchor,
        trust,
        resolver.clone(),
        node.clone(),
        publication.clone(),
    );
    let service = Arc::new(service);
    let decode = |raw: Vec<u8>| {
        assert!(raw.starts_with(b"HTTP/1.1 200"));
        let offset = raw.windows(4).position(|v| v == b"\r\n\r\n").unwrap() + 4;
        serde_json::from_slice::<StopRecordControlResponseV2>(&raw[offset..]).unwrap()
    };
    let first = decode(http_roundtrip(&service, &body));
    let node_key = hex::encode(signer.verifying_key().to_bytes());
    verify_stop_record_control_v2(&q, &first, &node_key).unwrap();
    let StopRecordFactV2::Known(fact) = &first.fact.payload.fact else {
        panic!("known actual Stop required")
    };
    assert_eq!(fact.record, record);
    assert!(matches!(
        first.current.payload.current,
        StopControlDeliveryV2::Exact(_)
    ));
    let control = publication.control_boundaries().unwrap();
    let deadline = Instant::now() + Duration::from_secs(5);
    control
        .arm(
            test_control::Point::StopRecordPrepared,
            None,
            None,
            deadline,
        )
        .unwrap();
    let child_service = service.clone();
    let child = std::thread::spawn(move || http_roundtrip(&child_service, &body));
    control.wait(deadline).unwrap();
    assert!(
        now_unix_ms() < current_until,
        "must first prepare genuine live Current"
    );
    std::thread::sleep(Duration::from_millis(
        current_until.saturating_sub(now_unix_ms()) + 1,
    ));
    control.release().unwrap();
    let late = decode(child.join().unwrap());
    verify_stop_record_control_v2(&q, &late, &node_key).unwrap();
    assert!(matches!(
        late.current.payload.current,
        StopControlDeliveryV2::Drift
    ));
    assert_eq!(first.fact.payload.fact, late.fact.payload.fact);
    assert_eq!(resolver.business_reads.load(Ordering::SeqCst), 0);
    assert_eq!(resolver.world.lock().unwrap().snapshot(), world.snapshot());
    node.lock().unwrap().stop().unwrap();
}

#[path = "formal_live_task_mutation_http_tests.rs"]
mod mutation_http_tests;
