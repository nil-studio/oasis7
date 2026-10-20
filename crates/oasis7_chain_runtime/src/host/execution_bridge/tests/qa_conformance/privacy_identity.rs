//! Actual server authorization and world identity negatives bypass local client prechecks.
use super::*;
fn denied(endpoint: &str, path: &str, body: &impl serde::Serialize) -> serde_json::Value {
    let response = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(2))
        .build()
        .unwrap()
        .post(format!("{endpoint}{path}"))
        .json(body)
        .send()
        .unwrap();
    assert_eq!(response.status().as_u16(), 503);
    let error: serde_json::Value = response.json().unwrap();
    assert_eq!(error["error"], "world_service_unavailable");
    for protected in ["outcome", "changes", "view", "payload"] {
        assert!(error.get(protected).is_none());
    }
    error
}
#[test]
fn real_tcp_cross_subject_lookup_and_protected_cursor_are_denied() {
    let fixture = Fixture::with_controlled_commits(true);
    let victim = fixture.delegation();
    fixture.client.submit(victim.clone()).unwrap();
    commit_request(&mut fixture.driver.lock().unwrap(), 0, Some(victim.clone()));
    fixture.committed(&victim);
    let before = fixture
        .driver
        .lock()
        .unwrap()
        .state
        .last_applied_committed_height;
    let WorldServicePayloadV1::Delegation(original) = &victim.signed_payload else {
        unreachable!()
    };
    let mut attacker_request = original.request.clone();
    attacker_request.nonce += 1;
    let attacker = WorldServicePayloadV1::Delegation(
        sign_read_request("delegation", attacker_request, &hex::encode([10u8; 32])).unwrap(),
    );
    let error = denied(
        &fixture.client.config().endpoint,
        "/v1/world/lookup",
        &wire::AuthenticatedLookup {
            request: LookupIntentRequest {
                contract_version: 1,
                key: victim.correlation.key,
            },
            original: attacker,
        },
    );
    assert_eq!(error["reason"], "service request cannot be served");
    let mut config = fixture.client.config().clone();
    config.scope_id = "agent:agent-a".into();
    let owner = RemoteWorldServiceClient::new(config.clone()).unwrap();
    let protected_view = ReadWorldViewRequest {
        observation_nonce: None,
        formal_anchor: None,
        contract_version: 1,
        world: config.expected_world.clone(),
        scope_id: config.scope_id,
        min_commit: None,
        fixed_commit: None,
        deadline_unix_ms: None,
    };
    let view = owner.read_view(protected_view.clone()).unwrap();
    denied(
        &config.endpoint,
        "/v1/world/view",
        &sign_read_request("/v1/world/view", protected_view, &hex::encode([10u8; 32])).unwrap(),
    );
    denied(
        &config.endpoint,
        "/v1/world/changes",
        &sign_read_request(
            "/v1/world/changes",
            ReadWorldChangesRequest {
                contract_version: 1,
                cursor: view.continuation().clone(),
                max_items: 32,
                max_bytes: 65536,
            },
            &hex::encode([10u8; 32]),
        )
        .unwrap(),
    );
    assert_eq!(
        fixture
            .driver
            .lock()
            .unwrap()
            .state
            .last_applied_committed_height,
        before
    );
    println!(
        "PRE2_CROSS_SUBJECT_LOOKUP_CURSOR_DENIAL_PASSED raw_server_checks=true canonical_height_unchanged=true"
    );
}
#[test]
fn real_tcp_same_world_name_wrong_genesis_is_not_same_identity() {
    let fixture = Fixture::with_controlled_commits(true);
    let before = fixture.client.describe().unwrap();
    let mut wrong = fixture.client.config().expected_world.clone();
    wrong.genesis_digest = "unrelated-genesis".into();
    let request = DescribeWorldRequest {
        contract_version: 1,
        expected_world: wrong,
        trust_config_ref: fixture.client.config().trusted_service_public_key.clone(),
    };
    let error = denied(
        &fixture.client.config().endpoint,
        "/v1/world/describe",
        &sign_read_request("/v1/world/describe", request, &fixture.owner).unwrap(),
    );
    assert_eq!(error["reason"], "service request cannot be served");
    assert_eq!(fixture.client.describe().unwrap().world, before.world);
    println!("PRE2_WORLD_IDENTITY_GENESIS_FENCE_PASSED world_name_same=true trusted_key_same=true");
}
