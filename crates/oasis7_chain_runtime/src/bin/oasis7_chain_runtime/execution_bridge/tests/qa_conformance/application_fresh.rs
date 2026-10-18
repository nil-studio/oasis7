//! Ordinary configured server admission; the discarded preflight server never starts an actor.
use super::*;
use oasis7::simulator::{
    ContinuousAgentRequestContextV1, ContinuousAgentTurnContextV1, DecisionProvider,
    ProviderLoopbackAdapter,
};
use oasis7::viewer::ViewerRuntimeLiveServer;

#[test]
fn real_tcp_hosted_fresh_admission_observes_reserves_and_invokes_provider() {
    application_harness::run_isolated_application_mode(
        false,
        false,
        false,
        true,
        false,
        false,
        "fresh-admission",
    );
}

pub(super) fn verify_fresh(client: &RemoteWorldServiceClient) {
    let root = std::path::PathBuf::from(std::env::var("PRE2_METADATA_DIR").unwrap());
    let config = application_hosted::server_config(
        client,
        true,
        Duration::from_millis(200),
        Some(root.join("fresh-private-lineage.json")),
    );
    preflight(client, &root);
    let ordinary = ViewerRuntimeLiveServer::new(config).unwrap();
    application_hosted::verify_hosted_server(ordinary);
    println!("PRE2_HOSTED_FRESH_ADMISSION_CANONICAL_RECEIPT_PASSED");
}

pub(super) fn preflight(client: &RemoteWorldServiceClient, root: &std::path::Path) {
    let mut preflight = ViewerRuntimeLiveServer::new(application_hosted::server_config(
        client,
        false,
        Duration::from_secs(60),
        Some(root.join("preflight-private-lineage.json")),
    ))
    .unwrap();
    let context = preflight
        .test_prepare_canonical_provider_response(
            "agent-a",
            oasis7_simulator::Action::MoveAgent {
                agent_id: "agent-a".into(),
                to: "runtime:2:2:0".into(),
            },
        )
        .unwrap();
    let request: ContinuousAgentRequestContextV1 =
        serde_json::from_value(context["request"]["request_context"].clone()).unwrap();
    let turn: ContinuousAgentTurnContextV1 =
        serde_json::from_value(context["request"]["turn_context"].clone()).unwrap();
    let endpoint = std::env::var("OASIS7_AGENT_PROVIDER_URL").unwrap();
    let mut provider = ProviderLoopbackAdapter::new(&endpoint, None, 1000).unwrap();
    let response = provider
        .decide_with_continuous_request_context(&request.base_decision_request, &turn, &request)
        .unwrap();
    assert_eq!(response.request_digest, request.request_digest);
    assert_eq!(
        response.response_digest,
        oasis7::simulator::cognition_response_digest(&response.base_decision_response)
    );
    drop(preflight);
    fs::write(
        root.join("fresh-provider-preflight-complete"),
        b"actual adapter validated",
    )
    .unwrap();
    let reset_deadline = Instant::now() + Duration::from_secs(1);
    while !root.join("fresh-provider-counter-reset").exists() && Instant::now() < reset_deadline {
        thread::sleep(Duration::from_millis(5));
    }
    assert!(
        root.join("fresh-provider-counter-reset").exists(),
        "real model accounting must exclude completed preflight"
    );
    println!(
        "fresh_model_protocol_preflight_valid=true canonical_reserve_prefix_start_count=0 preflight_model_count=1 target_factory_calls=0"
    );
}
