use super::*;
use oasis7_client_api::world_service::participant_discovery::ParticipantOwnerViewProfileV1;
use serde_json::json;
fn request() -> ParticipantOwnerViewRequestV1 {
    let ns = json!({"service_id":"identity","environment_id":"test","instance_epoch":"1"});
    serde_json::from_value(json!({
        "owner":{"request":{"api_rev":1,"client_operation_id":"register","identity_namespace":ns,"account_id":"account","world_id":"world","genesis_digest":"genesis","owner_public_key":"ab".repeat(32),"owner_key_version":"1"},"subject_public_key":"ab".repeat(32),"signature_hex":"00".repeat(64)},
        "admission":{"request":{"api_rev":1,"business_digest":"digest","identity_namespace":ns,"account_id":"account","world_id":"world","genesis_digest":"genesis","owner_public_key":"ab".repeat(32),"branch_id":"main","reorg_epoch":"0","policy_generation":"1","valid_from_tick":"0","valid_until_tick":"100"},"subject_public_key":"cd".repeat(32),"signature_hex":"00".repeat(64)},
        "read":{"request":{"contract_version":1,"observation_nonce":"ef".repeat(32),"world":{"world_id":"world","genesis_digest":"genesis"},"scope_id":"agent:agent","min_commit":null,"fixed_commit":null,"deadline_unix_ms":null},"subject_public_key":"ab".repeat(32),"signature_hex":"00".repeat(64)}
    })).unwrap()
}

#[test]
fn guarded_actual_compact_read_rejects_before_any_world_read() {
    let root = std::env::temp_dir().join(format!(
        "oasis7-guarded-owner-no-read-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    assert!(!root.exists());
    let runtime = Arc::new(Mutex::new(NodeRuntime::new(
        oasis7_node::NodeConfig::new("node", "world", oasis7_node::NodeRole::Sequencer).unwrap(),
    )));
    let signer = FeedbackSubmitSigner {
        private_key_hex: "09".repeat(32),
        public_key_hex: "ab".repeat(32),
    };
    let guarded = crate::controlled_live_config::GuardedReadAuthority::from_verified(
        oasis7_distfs::controlled_authority::replicated_protocol::HeadAnchor {
            position: 0,
            decision_hash: "test-anchor".into(),
            qualified: false,
        },
        "test-configuration".into(),
    );
    let service = Service {
        restore_context: world_service_read::RestoreContext::default(),
        records: &root,
        storage: &root,
        identity: WorldIdentity {
            world_id: "world".into(),
            genesis_digest: "genesis".into(),
        },
        signer: &signer,
        runtime: &runtime,
        guarded: Some((&root, &guarded)),
    };
    let mut req = request();
    req.response_profile = Some(ParticipantOwnerViewProfileV1::OwnerControlV1);
    assert_eq!(
        read(&service, &req).unwrap_err(),
        "bounded owner control requires ordinary service profile"
    );
    assert!(!root.exists());
}
