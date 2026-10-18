//! Fresh service Agent admission requires an explicit application-owned durable store.
use super::*;
use oasis7::viewer::ViewerRuntimeLiveServer;

#[test]
fn real_tcp_service_agent_missing_store_blocks_fresh_native_admission() {
    application_harness::run_isolated_application_mode(
        false,
        false,
        false,
        true,
        false,
        false,
        "missing-store",
    );
}

pub(super) fn verify_missing_store(client: &RemoteWorldServiceClient) {
    let config = application_hosted::server_config(client, false, Duration::from_secs(60), None);
    let mut server = ViewerRuntimeLiveServer::new(config).unwrap();
    client.describe().unwrap();
    let action = oasis7_simulator::Action::MoveAgent {
        agent_id: "agent-a".into(),
        to: "runtime:2:2:0".into(),
    };
    let result = server
        .test_prepare_canonical_provider_response("agent-a", action.clone())
        .and_then(|context| server.test_queue_canonical_provider_response(context, action));
    let summary = server.test_canonical_provider_summary();
    let blocked = result.as_ref().err().is_some_and(|error| {
        error.starts_with("fresh service Agent admission requires an explicit App-private provider lineage store (--provider-lineage-store)")
    });
    println!(
        "missing_store_admission_clear_block={blocked} readonly_describe_verified=true pending_intents={} mirrored_leases={}",
        summary["pending_intent_count"], summary["mirrored_lease_count"]
    );
    if let Err(error) = &result {
        println!("missing_store_admission_diagnostic={error}");
    }
    assert!(
        blocked,
        "fresh service Agent admission must require explicit durable provider_lineage_store"
    );
    assert_eq!(summary["pending_intent_count"], 0);
    assert_eq!(summary["mirrored_lease_count"], 0);
    println!("PRE2_SERVICE_AGENT_MISSING_STORE_BLOCKED");
}
