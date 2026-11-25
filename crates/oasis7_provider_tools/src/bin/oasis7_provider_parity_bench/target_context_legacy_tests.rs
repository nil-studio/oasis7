use super::super::target_context::build_target_context;
use super::sample_patrol_observation;
use oasis7::simulator::{
    ContinuousAgentResponseContextV1, DecisionProvider, DecisionResponse, ProviderLoopbackAdapter,
    cognition_legacy_response_digest, golden_decision_provider_fixtures,
};
use std::io::{Read, Write};
use std::net::TcpListener;

#[test]
fn target_route_rejects_legacy_full_response_digest() {
    let fixture = golden_decision_provider_fixtures()
        .into_iter()
        .next()
        .expect("golden parity fixture");
    let observation = sample_patrol_observation();
    let (turn_context, request_context) = build_target_context(
        fixture.request,
        &observation,
        fixture.fixture_id.as_str(),
        "parity-session-legacy-response-test",
        0,
    );
    let base_response = DecisionResponse::wait("parity-legacy-response-provider");
    let response_context = ContinuousAgentResponseContextV1 {
        base_decision_response: base_response.clone(),
        context_discriminator: request_context.context_discriminator.clone(),
        context_version: request_context.context_version,
        agent_session_id: request_context.agent_session_id.clone(),
        agent_turn_id: request_context.agent_turn_id.clone(),
        decision_request_id: request_context.decision_request_id.clone(),
        retry_seq: request_context.retry_seq,
        transport_attempt: request_context.transport_attempt,
        request_digest: request_context.request_digest.clone(),
        response_digest: cognition_legacy_response_digest(&base_response),
    };
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind legacy response listener");
    let bind = listener
        .local_addr()
        .expect("legacy response listener address");
    let serve = std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().expect("accept legacy response request");
        let mut request = [0_u8; 8 * 1024];
        let bytes = stream
            .read(&mut request)
            .expect("read legacy response request");
        assert!(
            bytes > 0,
            "legacy response request must reach external server"
        );
        let body = serde_json::to_string(&response_context).expect("encode legacy response");
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
            body.len(),
            body
        );
        stream
            .write_all(response.as_bytes())
            .expect("write legacy response");
    });

    let mut adapter = ProviderLoopbackAdapter::new(&format!("http://{bind}"), None, 5_000)
        .expect("create loopback adapter");
    let error = adapter
        .decide_with_continuous_request_context(
            &request_context.base_decision_request,
            &turn_context,
            &request_context,
        )
        .expect_err("external legacy full-response digest must fail closed");
    assert_eq!(error.code, "legacy_response_digest_unsupported");
    serve.join().expect("legacy response server should finish");
}
