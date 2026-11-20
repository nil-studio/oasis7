use oasis7_wasm_abi::{
    AgentCommandResponse, CapabilityAudience, CapabilityCatalogEntry, CapabilityGrantV2,
    CapabilityIssuer, CapabilityPresenter, CapabilityScope, CapabilitySubject,
    canonical_sha256_hex, capability_grant_body_hash, capability_request_hash,
    capability_scope_hash,
};

const SCHEMA_HASH: &str = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";

fn subject() -> CapabilitySubject {
    CapabilitySubject::Agent {
        agent_id: "agent-7".to_string(),
        owner_binding: "owner-7".to_string(),
        generation: 1,
    }
}

fn presenter() -> CapabilityPresenter {
    CapabilityPresenter {
        presenter_id: "provider-1".to_string(),
        presenter_kind: "provider".to_string(),
        session_id: Some("session-1".to_string()),
        attestation_ref: None,
    }
}

fn audience() -> CapabilityAudience {
    CapabilityAudience {
        world_id: "world.test".to_string(),
        branch_id: "branch-1".to_string(),
        finality_epoch: 4,
        target_kind: "world".to_string(),
        target_id: None,
    }
}

fn scope() -> CapabilityScope {
    CapabilityScope {
        module_id: "module.weather".to_string(),
        module_version: "1.0.0".to_string(),
        namespace: "weather".to_string(),
        object_kind: "command".to_string(),
        object_name: "observe".to_string(),
        operation: "execute".to_string(),
        entity_selector: Some(vec!["station-1".to_string()]),
        resource_selector: Some(vec!["weather.read".to_string()]),
        max_payload_bytes: Some(128),
        policy_class: Some("read-only".to_string()),
    }
}

fn grant() -> CapabilityGrantV2 {
    CapabilityGrantV2 {
        grant_id: "grant-weather-1".to_string(),
        grant_version: 2,
        subject: subject(),
        audience: audience(),
        issuer: CapabilityIssuer {
            issuer_id: "governance-1".to_string(),
            issuer_kind: "governance".to_string(),
            governance_epoch: 9,
            finalized_receipt_id: "finality-9".to_string(),
            key_id: "governance-key-1".to_string(),
            issuer_key_epoch: 3,
            authority_rotation_receipt_id: None,
            signature: "ed25519:opaque".to_string(),
        },
        scope: scope(),
        issued_at_tick: 1,
        expires_at_tick: Some(100),
        grant_nonce: "grant-nonce-1".to_string(),
        parent_grant_id: None,
        delegation_depth: 0,
        revocation_epoch: 2,
        status: "verified".to_string(),
        canonical_body_hash: "body-hash".to_string(),
        issuance_signature: "ed25519:opaque".to_string(),
    }
}

#[test]
fn frozen_fixture_wire_round_trips_without_provider_authority() {
    let value = serde_json::json!({
        "grant_id": "grant-weather-1",
        "grant_version": 2,
        "subject": {"kind":"agent","agent_id":"agent-7","owner_binding":"owner-7","generation":1},
        "audience": {"world_id":"world.test","branch_id":"branch-1","finality_epoch":4,"target_kind":"world","target_id":null},
        "issuer": {"issuer_id":"governance-1","issuer_kind":"governance","governance_epoch":9,"finalized_receipt_id":"finality-9","key_id":"governance-key-1","issuer_key_epoch":3,"authority_rotation_receipt_id":null,"signature":"ed25519:opaque"},
        "scope": {"module_id":"module.weather","module_version":"1.0.0","namespace":"weather","object_kind":"command","object_name":"observe","operation":"execute","entity_selector":["station-1"],"resource_selector":["weather.read"],"max_payload_bytes":128,"policy_class":"read-only"},
        "issued_at_tick":1,"expires_at_tick":100,"grant_nonce":"grant-nonce-1","parent_grant_id":null,"delegation_depth":0,"revocation_epoch":2,"status":"verified","canonical_body_hash":"body-hash","issuance_signature":"ed25519:opaque"
    });
    let decoded: CapabilityGrantV2 = serde_json::from_value(value).expect("v2 fixture wire");
    decoded.validate().expect("structural validation");
    assert_eq!(decoded.issuer.signature, "ed25519:opaque");
    assert_ne!(
        decoded.canonical_body_hash().unwrap(),
        decoded.canonical_body_hash
    );
}

#[test]
fn scope_matching_is_conjunctive_and_subset_only() {
    let parent = scope();
    let mut child = parent.clone();
    child.max_payload_bytes = Some(64);
    assert!(child.is_subset_of(&parent));
    assert!(!parent.matches_exact(&child));
    assert!(parent.allows(&child));

    child.operation = "write".to_string();
    assert!(!child.is_subset_of(&parent));
    child.operation = "execute".to_string();
    child.entity_selector = Some(vec!["*".to_string()]);
    assert!(!child.is_subset_of(&parent));

    let mut omitted = parent.clone();
    omitted.entity_selector = None;
    assert!(!omitted.matches_exact(&parent));
}

#[test]
fn canonical_hashes_are_stable_and_use_sha256() {
    assert_eq!(
        canonical_sha256_hex(b"abc").unwrap(),
        "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
    );
    let first = capability_grant_body_hash(&grant()).unwrap();
    assert_eq!(first, capability_grant_body_hash(&grant()).unwrap());
    assert_eq!(first, grant().canonical_body_hash().unwrap());
    assert_eq!(
        capability_scope_hash(&scope()).unwrap(),
        capability_scope_hash(&scope()).unwrap()
    );
}

#[test]
fn response_wire_and_request_hash_bind_exact_selection() {
    let response = AgentCommandResponse {
        response_nonce: "response-1".to_string(),
        subject: subject(),
        presenter: presenter(),
        audience: audience(),
        catalog_snapshot_id: "catalog-weather-1".to_string(),
        selected_entry: CapabilityCatalogEntry {
            module_id: "module.weather".to_string(),
            module_version: "1.0.0".to_string(),
            namespace: "weather".to_string(),
            command: "observe".to_string(),
            schema_version: 1,
            schema_hash: SCHEMA_HASH.to_string(),
            max_payload_bytes: 128,
            eligible_grant_ids: Vec::new(),
        },
        envelope: oasis7_wasm_abi::ModuleCommandEnvelope {
            namespace: "weather".to_string(),
            name: "observe".to_string(),
            schema_version: 1,
            schema_hash: SCHEMA_HASH.to_string(),
            payload: vec![123, 125],
        },
        provider_id: Some("provider-1".to_string()),
        trace_id: Some("trace-1".to_string()),
    };
    response.validate().expect("response structure");
    let first = capability_request_hash(&response).unwrap();
    assert_eq!(first, capability_request_hash(&response).unwrap());
    assert_eq!(
        first,
        oasis7_wasm_abi::canonical_request_hash(&response).unwrap()
    );
    let mut altered = response.clone();
    altered.response_nonce = "response-2".to_string();
    assert_ne!(first, capability_request_hash(&altered).unwrap());
}

fn catalog_entry() -> CapabilityCatalogEntry {
    CapabilityCatalogEntry {
        module_id: "module.weather".into(),
        module_version: "1.0.0".into(),
        namespace: "weather".into(),
        command: "observe".into(),
        schema_version: 1,
        schema_hash: SCHEMA_HASH.into(),
        max_payload_bytes: 128,
        eligible_grant_ids: Vec::new(),
    }
}

fn catalog() -> oasis7_wasm_abi::CapabilityCatalogSnapshot {
    oasis7_wasm_abi::CapabilityCatalogSnapshot {
        snapshot_id: "catalog".into(),
        world_id: "world.test".into(),
        world_head: 7,
        branch_id: "branch-1".into(),
        finality_epoch: 4,
        logical_tick: 10,
        module_registry_hash: "registry".into(),
        policy_hash: "policy".into(),
        revocation_epoch: 2,
        subject: subject(),
        presenter: presenter(),
        audience: audience(),
        entries: vec![catalog_entry()],
        valid_until_tick: 20,
    }
}

fn command_response() -> AgentCommandResponse {
    AgentCommandResponse {
        response_nonce: "response".into(),
        subject: subject(),
        presenter: presenter(),
        audience: audience(),
        catalog_snapshot_id: "catalog".into(),
        selected_entry: catalog_entry(),
        envelope: oasis7_wasm_abi::ModuleCommandEnvelope {
            namespace: "weather".into(),
            name: "observe".into(),
            schema_version: 1,
            schema_hash: SCHEMA_HASH.into(),
            payload: b"{}".to_vec(),
        },
        provider_id: Some("provider-1".into()),
        trace_id: Some("trace".into()),
    }
}

#[test]
fn all_fallible_validation_entries_preserve_the_host_error() {
    macro_rules! check {
        ($value:expr) => {{
            let value = $value;
            assert_eq!(
                value
                    .validate_with_fallible_work_v1(&mut |_, _| Ok::<_, u64>(()))
                    .unwrap(),
                value.validate()
            );
            let mut calls = 0;
            let result = value.validate_with_fallible_work_v1(&mut |_, _| {
                calls += 1;
                Err(73_u64)
            });
            assert_eq!(result, Err(73));
            assert_eq!(calls, 1);
        }};
    }
    check!(subject());
    check!(presenter());
    check!(audience());
    check!(grant().issuer);
    check!(scope());
    check!(grant());
    check!(catalog_entry());
    check!(catalog());
    check!(command_response());
}

#[test]
fn metered_validation_preserves_business_error_order_and_utf8_byte_debits() {
    use oasis7_wasm_abi::CapabilityAuthorizationValidationError as Error;
    let invalid = CapabilitySubject::Agent {
        agent_id: " \n".into(),
        owner_binding: String::new(),
        generation: 0,
    };
    assert_eq!(
        invalid
            .validate_with_fallible_work_v1(&mut |_, _| Ok::<_, u64>(()))
            .unwrap(),
        Err(Error::EmptyField("subject.agent_id"))
    );
    let value = CapabilitySubject::Agent {
        agent_id: "界".into(),
        owner_binding: "αβ".into(),
        generation: 1,
    };
    let mut charged = Vec::new();
    value
        .validate_with_fallible_work_v1(&mut |steps, bytes| {
            charged.push((steps, bytes));
            Ok::<_, u64>(())
        })
        .unwrap()
        .unwrap();
    assert_eq!(charged, [(1, 0), (1, 3), (1, 4)]);
    let mut invalid_grant = grant();
    invalid_grant.grant_version = 3;
    invalid_grant.grant_id.clear();
    assert_eq!(
        invalid_grant
            .validate_with_fallible_work_v1(&mut |_, _| Ok::<_, u64>(()))
            .unwrap(),
        Err(Error::UnsupportedVersion(3))
    );
}

#[test]
fn metered_subset_refusal_never_becomes_a_business_mismatch() {
    let mut parent = scope();
    parent.entity_selector = Some(vec!["first".into(), "station-1".into()]);
    let child = scope();
    let mut total_calls = 0;
    let accepted = parent
        .contains_subset_with_fallible_work_v1(&child, &mut |_, _| {
            total_calls += 1;
            Ok::<_, u64>(())
        })
        .unwrap();
    assert!(accepted);
    assert!(total_calls > 2);
    for stop in [0, total_calls - 1] {
        let mut calls = 0;
        let result = parent.contains_subset_with_fallible_work_v1(&child, &mut |_, _| {
            calls += 1;
            if calls > stop { Err(97_u64) } else { Ok(()) }
        });
        assert_eq!(result, Err(97));
        assert_eq!(calls, stop + 1);
    }
    let mut missing = child;
    missing.entity_selector = Some(vec!["absent".into()]);
    assert!(
        !parent
            .contains_subset_with_fallible_work_v1(&missing, &mut |_, _| Ok::<_, u64>(()))
            .unwrap()
    );
}

#[test]
fn borrowed_hash_preimages_keep_the_original_field_exclusions() {
    use oasis7_wasm_abi::{canonical_hash, encode_canonical_cbor};
    for optional_fields in [false, true] {
        let mut value = grant();
        value.issuer.issuer_id = "issuer/治理\0".into();
        value.issuer.authority_rotation_receipt_id = optional_fields.then(|| "rotation/α".into());
        value.parent_grant_id = optional_fields.then(|| "parent".into());
        value.expires_at_tick = optional_fields.then_some(100);
        // The original body excludes the grant identity and both signatures.
        let mut expected = serde_json::to_value(&value).unwrap();
        let fields = expected.as_object_mut().unwrap();
        for field in ["grant_id", "canonical_body_hash", "issuance_signature"] {
            fields.remove(field);
        }
        let issuer = fields.get_mut("issuer").unwrap().as_object_mut().unwrap();
        issuer.remove("signature");
        if !optional_fields {
            issuer.remove("authority_rotation_receipt_id");
        }
        let borrowed = value.canonical_body_input_v1();
        assert_eq!(serde_json::to_value(&borrowed).unwrap(), expected);
        let expected_bytes = encode_canonical_cbor(&expected).unwrap();
        assert_eq!(value.canonical_body_bytes().unwrap(), expected_bytes);
        assert_eq!(encode_canonical_cbor(&borrowed).unwrap(), expected_bytes);
        assert_eq!(
            value.canonical_body_hash().unwrap(),
            canonical_hash(&expected).unwrap()
        );
    }
    let value = catalog();
    let mut expected = serde_json::to_value(&value).unwrap();
    expected.as_object_mut().unwrap().remove("snapshot_id");
    let borrowed = value.canonical_hash_input_v1();
    assert_eq!(serde_json::to_value(&borrowed).unwrap(), expected);
    assert_eq!(
        encode_canonical_cbor(&borrowed).unwrap(),
        encode_canonical_cbor(&expected).unwrap()
    );
    assert_eq!(
        value.canonical_hash().unwrap(),
        canonical_hash(&expected).unwrap()
    );
}
