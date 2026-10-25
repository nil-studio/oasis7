use super::*;
#[test]
fn canonical_codec_and_digest_match_shared_contract() {
    let value =
        serde_json::json!({"negative":-7,"fraction":1.5,"large":18_446_744_073_709_551_615_u64});
    let json = value.to_string();
    assert_eq!(
        signing("view", &json).unwrap(),
        ws::signing_bytes("view", &value).unwrap()
    );
    assert_eq!(
        digest("view", &json).unwrap(),
        ws::request_digest("view", &value).unwrap()
    );
}
#[test]
fn signed_response_rejects_wrong_request_domain_key_and_tampering() {
    let secret = "07".repeat(32);
    let request = serde_json::json!({"world":"test"});
    let signed_request = ws::sign_read_request("view", request.clone(), &secret).unwrap();
    let response = ws::sign_service_response(
        "view",
        ws::request_digest("view", &request).unwrap(),
        serde_json::json!({"ok":true}),
        &secret,
    )
    .unwrap();
    let json = serde_json::to_string(&response).unwrap();
    assert_eq!(
        verify(
            "view",
            &request.to_string(),
            &json,
            &signed_request.subject_public_key
        )
        .unwrap(),
        "{\"ok\":true}"
    );
    assert!(verify("view", "{}", &json, &signed_request.subject_public_key).is_err());
    assert!(
        verify(
            "other",
            &request.to_string(),
            &json,
            &signed_request.subject_public_key
        )
        .is_err()
    );
    assert!(verify("view", &request.to_string(), &json, &"00".repeat(32)).is_err());
    let altered = json.replace("true", "false");
    assert!(
        verify(
            "view",
            &request.to_string(),
            &altered,
            &signed_request.subject_public_key
        )
        .is_err()
    );
}
#[test]
fn bounded_and_unsupported_input_fails_closed() {
    assert_eq!(
        parse::<Value>(&" ".repeat(MAX_JSON_BYTES + 1)),
        Err("input_limit")
    );
    assert_eq!(validate("unknown", "{}", "{}"), Err("unsupported_contract"));
    assert_eq!(validate("view", "{}", "{}"), Err("invalid_json"));
    assert_eq!(signing("", "{}"), Err("invalid_domain"));
}

#[test]
fn typed_describe_binds_the_trusted_world() {
    let payload = serde_json::json!({
        "contract_version":1,"world":{"world_id":"w","genesis_digest":"g"},
        "binding":{"provider_world_id":"w","branch_id":"b","finality_ref":"f",
            "reorg_generation":0,"governing_manifest_ref":"m","authority_generation":1,"permission_generation":1},
        "capabilities":[],"availability":{"readable":true,"writable":true,"recovering":false,"reason":null,"retry_after_ms":null},
        "current_view":null
    }).to_string();
    assert!(
        validate(
            "describe",
            r#"{"world_id":"w","genesis_digest":"g"}"#,
            &payload
        )
        .is_ok()
    );
    assert_eq!(
        validate(
            "describe",
            r#"{"world_id":"other","genesis_digest":"g"}"#,
            &payload
        ),
        Err("contract_mismatch")
    );
}

#[test]
fn typed_builder_verifies_signed_delegation_before_correlation() {
    use oasis7_client_api::world_service_wire::{
        AgentSignerDelegationChangeV1, WorldServicePayloadV1,
    };
    let request = AgentSignerDelegationChangeV1 {
        world_id: "w".into(),
        branch_id: "b".into(),
        agent_id: "a".into(),
        owner_binding: "o".into(),
        agent_identity_generation: 1,
        generation: 1,
        delegate_public_key: "08".repeat(32),
        revoked: false,
        nonce: 1,
    };
    let signed = ws::sign_read_request("delegation", request, &"07".repeat(32)).unwrap();
    let payload: WorldServicePayloadV1 = WorldServicePayloadV1::Delegation(signed);
    let input =
        serde_json::json!({"world":{"world_id":"w","genesis_digest":"g"},"payload":payload});
    let correlation = build::build("correlation", &input.to_string()).unwrap();
    let parsed: ws::RequestCorrelation = serde_json::from_str(&correlation).unwrap();
    assert_eq!(parsed.key.world.world_id, "w");
    let mut altered = input;
    altered["payload"]["payload"]["signature_hex"] = serde_json::json!("00".repeat(64));
    assert_eq!(
        build::build("correlation", &altered.to_string()),
        Err("contract_mismatch")
    );
    assert_eq!(build::build("prefix", "{}"), Err("invalid_json"));
    assert_eq!(build::build("arbitrary", "{}"), Err("unsupported_contract"));
}

#[test]
fn gateway_and_raw_fields_preserve_float_and_u64_signed_values() {
    let request = r#"{"amount":18446744073709551615,"ratio":1.0}"#;
    let key = ws::sign_read_request("view", (), &"07".repeat(32))
        .unwrap()
        .subject_public_key;
    let payload: Value = parse(request).unwrap();
    let response = ws::sign_service_response(
        "view",
        digest("view", request).unwrap(),
        payload,
        &"07".repeat(32),
    )
    .unwrap();
    let gateway =
        serde_json::json!({"ok":true,"api_version":2,"world_response":response}).to_string();
    let verified = verify_gateway("view", request, &gateway, &key).unwrap();
    assert!(verified.contains("18446744073709551615"));
    assert!(verified.contains("1.0"));
    let field = build::build(
        "field",
        &serde_json::json!({"json":verified,"path":["amount"]}).to_string(),
    )
    .unwrap();
    assert_eq!(field, "18446744073709551615");
    let failure = serde_json::json!({"ok":true,"world_response":response,"upstream_read_error":{}})
        .to_string();
    assert_eq!(
        verify_gateway("view", request, &failure, &key),
        Err("gateway_rejected")
    );
}

#[test]
fn participant_admission_pins_issuer_and_namespace() {
    use oasis7_agent_api::game::*;
    let namespace = GameIdentityNamespaceV1 {
        service_id: "s".into(),
        environment_id: "e".into(),
        instance_epoch: DecimalU64(1),
    };
    let key = ws::sign_read_request("unused", (), &"07".repeat(32))
        .unwrap()
        .subject_public_key;
    let registration = GameParticipantRegisterV1 {
        api_rev: 1,
        client_operation_id: "op".into(),
        identity_namespace: namespace.clone(),
        account_id: "acct".into(),
        world_id: "w".into(),
        genesis_digest: "g".into(),
        owner_public_key: key.clone(),
        owner_key_version: DecimalU64(1),
    };
    let owner = ws::sign_read_request(
        GAME_PARTICIPANT_REGISTER_DOMAIN,
        registration.clone(),
        &"07".repeat(32),
    )
    .unwrap();
    let admission = GameParticipantAdmissionV1 {
        api_rev: 1,
        business_digest: oasis7_client_api::world_service_correlation::participant_business_digest(
            &registration,
        )
        .unwrap(),
        identity_namespace: namespace.clone(),
        account_id: "acct".into(),
        world_id: "w".into(),
        genesis_digest: "g".into(),
        owner_public_key: key,
        branch_id: "b".into(),
        reorg_epoch: DecimalU64(0),
        policy_generation: DecimalU64(1),
        valid_from_tick: DecimalU64(1),
        valid_until_tick: DecimalU64(20),
    };
    let admission = ws::sign_read_request(
        GAME_PARTICIPANT_ADMISSION_DOMAIN,
        admission,
        &"08".repeat(32),
    )
    .unwrap();
    let mut request = serde_json::json!({"owner":owner,"namespace":namespace,"issuerPublicKey":admission.subject_public_key});
    let payload = serde_json::to_string(&admission).unwrap();
    assert!(validate("participant_admission", &request.to_string(), &payload).is_ok());
    request["issuerPublicKey"] = serde_json::json!("00".repeat(32));
    assert_eq!(
        validate("participant_admission", &request.to_string(), &payload),
        Err("contract_mismatch")
    );
}

#[test]
fn raw_replace_preserves_unrelated_protocol_numbers() {
    let value = build::build("replace", &serde_json::json!({
        "json":r#"{"current_public_context":{"tick":1},"proof":{"n":18446744073709551615,"f":1.0}}"#,
        "path":["current_public_context"],"value_json":r#"{"tick":2}"#
    }).to_string()).unwrap();
    assert!(value.contains("18446744073709551615"));
    assert!(value.contains("1.0"));
    assert!(value.contains("\"tick\":2"));
}
