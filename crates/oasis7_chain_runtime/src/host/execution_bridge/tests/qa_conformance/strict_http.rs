//! Real HTTP strict-field failures and supported default/opaque payload compatibility.
use super::*;
#[test]
fn real_tcp_strict_guarantee_fields_defaults_and_opaque_gameplay_bytes() {
    use oasis7_viewer_runtime::viewer::{
        CollectDataCommand, CollectDataRequest, sign_collect_data_auth_proof,
    };
    let fixture = Fixture::with_controlled_commits(true);
    let endpoint = &fixture.client.config().endpoint;
    let http = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(2))
        .build()
        .unwrap();
    let describe = sign_read_request(
        "/v1/world/describe",
        DescribeWorldRequest {
            contract_version: 1,
            expected_world: fixture.client.config().expected_world.clone(),
            trust_config_ref: fixture.client.config().trusted_service_public_key.clone(),
        },
        &fixture.owner,
    )
    .unwrap();
    let original = serde_json::to_value(&describe).unwrap();
    for location in ["wrapper", "request", "world"] {
        let mut body = original.clone();
        match location {
            "wrapper" => body["required_guarantees"] = serde_json::json!(["validator_quorum"]),
            "request" => {
                body["request"]["required_guarantees"] = serde_json::json!(["validator_quorum"])
            }
            _ => {
                body["request"]["expected_world"]["topology_identity"] =
                    serde_json::json!("node-local")
            }
        }
        let response = http
            .post(format!("{endpoint}/v1/world/describe"))
            .json(&body)
            .send()
            .unwrap();
        assert_eq!(response.status().as_u16(), 400);
        assert!(response.headers().get("retry-after").is_none());
        let error: serde_json::Value = response.json().unwrap();
        assert_eq!(error["error"], "unsupported_request_field");
        assert!(error.get("signature_hex").is_none() && error.get("payload").is_none());
    }
    let signed_view =
        sign_read_request("/v1/world/view", fixture.view(None), &fixture.owner).unwrap();
    let mut default_view = serde_json::to_value(&signed_view).unwrap();
    for optional in ["min_commit", "fixed_commit", "deadline_unix_ms"] {
        assert_eq!(default_view["request"][optional], serde_json::Value::Null);
        default_view["request"]
            .as_object_mut()
            .unwrap()
            .remove(optional);
    }
    let response = http
        .post(format!("{endpoint}/v1/world/view"))
        .json(&default_view)
        .send()
        .unwrap();
    assert_eq!(response.status().as_u16(), 200);
    let signed: SignedServiceResponse<
        ReadWorldViewResponse<oasis7_world_projection::WorldServiceProjection>,
    > = response.json().unwrap();
    oasis7::world_service::authority::verify_service_response(
        "/v1/world/view",
        &oasis7::world_service::authority::request_digest("/v1/world/view", &signed_view).unwrap(),
        &signed,
        &fixture.client.config().trusted_service_public_key,
    )
    .unwrap();
    let public = sign_read_request("owner", (), &fixture.owner)
        .unwrap()
        .subject_public_key;
    let mut command = CollectDataCommand::Submit {
        request: CollectDataRequest {
            electricity_cost: 7,
            data_amount: 11,
            player_id: "owner-a".into(),
            public_key: Some(public.clone()),
            auth: None,
        },
    };
    let proof = sign_collect_data_auth_proof(&command, 88, &public, &fixture.owner).unwrap();
    let CollectDataCommand::Submit { request } = &mut command else {
        unreachable!()
    };
    request.auth = Some(proof);
    let mut opaque = serde_json::to_value(&command).unwrap();
    opaque["future_opaque_metadata"] =
        serde_json::json!({"required_guarantees":["opaque_application_note"]});
    let bytes = serde_json::to_vec(&opaque).unwrap();
    let payload = WorldServicePayloadV1::GameplayJson(bytes.clone());
    let original = SubmitIntentRequest {
        contract_version: 1,
        correlation: derive_correlation(fixture.client.config().expected_world.clone(), &payload)
            .unwrap(),
        deadline_unix_ms: None,
        signed_payload: payload,
    };
    fixture.client.submit(original.clone()).unwrap();
    commit_request(
        &mut fixture.driver.lock().unwrap(),
        0,
        Some(original.clone()),
    );
    fixture.committed(&original);
    let key = correlation::key_digest(&original.correlation.key).unwrap();
    let world = fixture.driver.lock().unwrap().execution_world.clone();
    let result: wire::CanonicalIntentResultV1 = serde_json::from_value(
        world.capability_revocation_state().world_service_results[&key].clone(),
    )
    .unwrap();
    assert_eq!(
        result.request.signed_payload,
        WorldServicePayloadV1::GameplayJson(bytes)
    );
    assert_eq!(
        world.state().authenticated_collect_data_last_nonces["owner-a"][&public],
        88
    );
    println!(
        "PRE2_STRICT_HTTP_GUARANTEE_DEFAULT_OPAQUE_PASSED unsupported400=true retry_after=false default_optionals=true original_gameplay_bytes_preserved=true"
    );
}
