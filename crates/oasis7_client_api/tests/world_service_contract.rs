use oasis7_client_api::world_service::*;
use serde_json::{Value, json};

fn world() -> WorldIdentity {
    WorldIdentity {
        world_id: "world-a".into(),
        genesis_digest: "genesis-a".into(),
    }
}
fn commit(position: u64) -> CommitRef {
    CommitRef {
        world: world(),
        binding: ExecutionBinding {
            provider_world_id: "provider-a".into(),
            branch_id: "branch-a".into(),
            finality_ref: "approved-contract-a".into(),
            reorg_generation: 0,
            governing_manifest_ref: "manifest-a".into(),
            authority_generation: 1,
            permission_generation: 2,
        },
        position,
        execution_block_hash: format!("block-{position}"),
        state_root_ref: format!("root-{position}"),
    }
}
fn cursor(sequence: u64) -> EventCursor {
    EventCursor {
        stream_id: "events".into(),
        scope_id: "agent-a".into(),
        era: 3,
        sequence,
        commit: commit(7),
    }
}
fn correlation() -> RequestCorrelation {
    RequestCorrelation {
        key: RequestKey {
            world: world(),
            verified_subject: "key-a".into(),
            operation_domain: "gameplay".into(),
            nonce_scope: "player-key".into(),
            request_id_or_nonce: "12".into(),
        },
        payload_digest: "digest-a".into(),
    }
}
fn read_request() -> ReadWorldViewRequest {
    ReadWorldViewRequest {
        observation_nonce: None,
        formal_anchor: None,
        contract_version: 1,
        world: world(),
        scope_id: "agent-a".into(),
        min_commit: Some(commit(7)),
        fixed_commit: None,
        deadline_unix_ms: None,
    }
}

#[test]
fn request_golden_preserves_payload_bytes_and_additive_fields() {
    // Bytes are opaque: wrapping does not reinterpret or extend their signed domain.
    let signed_bytes = b"existing signed CBOR bytes".to_vec();
    let request = SubmitIntentRequest {
        contract_version: 1,
        correlation: correlation(),
        deadline_unix_ms: None,
        signed_payload: signed_bytes.clone(),
    };
    request.validate().unwrap();
    let mut wire = serde_json::to_value(&request).unwrap();
    assert_eq!(
        wire["correlation"],
        json!({"key":{"world":{"world_id":"world-a","genesis_digest":"genesis-a"},
        "verified_subject":"key-a","operation_domain":"gameplay","nonce_scope":"player-key","request_id_or_nonce":"12"},"payload_digest":"digest-a"})
    );
    wire["future_optional"] = json!("ignored");
    let decoded: SubmitIntentRequest<Vec<u8>> = serde_json::from_value(wire).unwrap();
    assert_eq!(decoded.signed_payload, signed_bytes);
    assert_eq!(decoded, request);
}

#[test]
fn existing_transfer_signing_bytes_survive_typed_envelope_round_trip() {
    use oasis7_client_api::{MainTokenTransfer, build_main_token_transfer_signing_payload};
    let public_key = "fded5085f1e8099257b7bfb2346eb6bd4194c3351d8f97686b18cfcc5969e0a3";
    let account = format!("oc:pk:{public_key}");
    let transfer: MainTokenTransfer = serde_json::from_value(json!({
        "from_account_id": account, "to_account_id": "oc:pk:destination", "amount": 1, "nonce": 12
    }))
    .unwrap();
    let bytes = build_main_token_transfer_signing_payload(&transfer, &account, public_key).unwrap();
    let request = SubmitIntentRequest {
        contract_version: 1,
        correlation: correlation(),
        deadline_unix_ms: None,
        signed_payload: transfer,
    };
    let wire = serde_json::to_vec(&request).unwrap();
    let decoded: SubmitIntentRequest<MainTokenTransfer> = serde_json::from_slice(&wire).unwrap();
    assert_eq!(
        build_main_token_transfer_signing_payload(&decoded.signed_payload, &account, public_key)
            .unwrap(),
        bytes
    );
    // Changing outer declarations does not change what the existing signer signs.
    let mut changed = decoded;
    changed.correlation.key.world.world_id = "another-world".into();
    assert_eq!(
        build_main_token_transfer_signing_payload(&changed.signed_payload, &account, public_key)
            .unwrap(),
        bytes
    );
}

#[test]
fn capabilities_default_unavailable_and_unknown_version_fails() {
    let response = DescribeWorldResponse {
        formal_profile: None,
        execution_evidence_scope: None,
        contract_version: 1,
        world: world(),
        binding: commit(0).binding,
        capabilities: vec![],
        availability: WorldAvailability {
            readable: false,
            writable: false,
            recovering: true,
            reason: Some(WorldServiceErrorKind::Unavailable),
            retry_after_ms: None,
        },
        current_view: None,
    };
    let mut wire = serde_json::to_value(response).unwrap();
    wire.as_object_mut().unwrap().remove("capabilities");
    let mut decoded: DescribeWorldResponse = serde_json::from_value(wire).unwrap();
    assert!(decoded.capabilities.is_empty());
    decoded.validate(&world()).unwrap();
    decoded.contract_version = 2;
    assert!(decoded.validate(&world()).is_err());
}

#[test]
fn missing_terminal_material_and_unknown_status_fail_deserialization() {
    let response = IntentResponse {
        terminal_recovery_authority: None,
        formal_profile: None,
        execution_evidence_scope: None,
        contract_version: 1,
        correlation: correlation(),
        outcome: IntentOutcome::Committed {
            commit: commit(7).into(),
            receipt: json!({"result":"done"}),
        },
    };
    response.validate(&correlation()).unwrap();
    let wire = serde_json::to_value(&response).unwrap();
    for field in ["commit", "receipt"] {
        let mut broken = wire.clone();
        broken["outcome"].as_object_mut().unwrap().remove(field);
        assert!(serde_json::from_value::<IntentResponse<Value>>(broken).is_err());
    }
    let mut broken = wire;
    broken["outcome"]["status"] = json!("future_success");
    assert!(serde_json::from_value::<IntentResponse<Value>>(broken).is_err());
}

#[test]
fn correlation_digest_conflict_and_wrong_world_are_rejected() {
    let mut response = IntentResponse {
        terminal_recovery_authority: None,
        formal_profile: None,
        execution_evidence_scope: None,
        contract_version: 1,
        correlation: correlation(),
        outcome: IntentOutcome::Unknown::<Value>,
    };
    response.validate(&correlation()).unwrap();
    response.correlation.payload_digest = "different".into();
    assert_eq!(response.correlation.key, correlation().key);
    assert!(response.validate(&correlation()).is_err());
    response.correlation = correlation();
    let mut wrong = commit(7);
    wrong.world.world_id = "world-b".into();
    response.outcome = IntentOutcome::Committed {
        commit: wrong.into(),
        receipt: json!({}),
    };
    assert!(response.validate(&correlation()).is_err());
}

#[test]
fn view_axes_are_distinct_and_view_cursor_must_match() {
    let mut response = ReadWorldViewResponse {
        formal_profile: None,
        formal_proof: None,
        execution_evidence_scope: None,
        contract_version: 1,
        version: ProjectionVersion {
            commit: commit(7),
            projection_revision: "projection-v1".into(),
            visibility_scope: "agent-a".into(),
        },
        logical_tick: 900,
        continuation: cursor(1200),
        view: json!({"energy":10}),
    };
    response.validate(&read_request()).unwrap();
    let wire = serde_json::to_value(&response).unwrap();
    assert_eq!(wire["logical_tick"], 900);
    assert_eq!(wire["version"]["commit"]["position"], 7);
    assert_eq!(wire["continuation"]["sequence"], 1200);
    response.version.commit = commit(6);
    response.continuation.commit = commit(6);
    assert!(response.validate(&read_request()).is_err());
    response.version.commit = commit(7);
    assert!(response.validate(&read_request()).is_err());
}

#[test]
fn formal_result_page_accepts_exact_fixed_head_and_rejects_tampering() {
    let from_anchor = FormalReadAnchorRefV1 {
        position: 1,
        record_id: "a".repeat(64),
        active_authority_id: "b".repeat(64),
    };
    let mut target_commit = commit(2);
    target_commit.execution_block_hash = "c".repeat(64);
    target_commit.state_root_ref = "d".repeat(64);
    let target_anchor = FormalReadAnchorRefV1 {
        position: target_commit.position,
        record_id: target_commit.execution_block_hash.clone(),
        active_authority_id: from_anchor.active_authority_id.clone(),
    };
    let request = ReadWorldViewRequest {
        observation_nonce: Some("0".repeat(64)),
        formal_anchor: Some(from_anchor.clone()),
        contract_version: 1,
        world: world(),
        scope_id: "agent-a".into(),
        min_commit: None,
        fixed_commit: Some(target_commit.clone()),
        deadline_unix_ms: None,
    };
    let response = ReadWorldViewResponse {
        formal_profile: Some(FormalReadProfileV1::controlled()),
        formal_proof: Some(FormalReadSidecarV1 {
            schema_version: 1,
            profile: FormalReadProfileV1::controlled(),
            anchor: from_anchor.clone(),
            history_cbor_hex: String::new(),
            projection: FormalProjectionInclusionV1 {
                schema_version: 1,
                audience: FormalProjectionAudienceV1 {
                    scope_id: "agent-a".into(),
                    reader_public_key: None,
                },
                leaf_index: 0,
                leaf_count: 1,
                siblings: vec![],
            },
        }),
        execution_evidence_scope: None,
        contract_version: 1,
        version: ProjectionVersion {
            commit: target_commit.clone(),
            projection_revision: "projection-v1".into(),
            visibility_scope: "agent-a".into(),
        },
        logical_tick: 2,
        continuation: EventCursor {
            stream_id: "events".into(),
            scope_id: "agent-a".into(),
            era: 3,
            sequence: 0,
            commit: target_commit.clone(),
        },
        view: json!({}),
    };
    let page = FormalBusinessOutcomePageV1 {
        schema_version: FORMAL_BUSINESS_RESULT_SCHEMA_V1,
        requested_correlation: correlation(),
        from_anchor: from_anchor.clone(),
        target_request: request,
        target_view: response,
        result: None,
        historical_cbor_hex: None,
        continuation_anchor: Some(target_anchor),
    };

    page.validate().unwrap();

    let mut tampered = page.clone();
    let mut other_head = target_commit;
    other_head.position += 1;
    other_head.execution_block_hash = "e".repeat(64);
    other_head.state_root_ref = "f".repeat(64);
    tampered.target_view.version.commit = other_head.clone();
    tampered.target_view.continuation.commit = other_head.clone();
    tampered.continuation_anchor = Some(FormalReadAnchorRefV1 {
        position: other_head.position,
        record_id: other_head.execution_block_hash,
        active_authority_id: from_anchor.active_authority_id,
    });
    assert_eq!(
        tampered.validate().unwrap_err().to_string(),
        "fixed view mismatch"
    );

    let mut wrong_proof_anchor = page.clone();
    wrong_proof_anchor
        .target_view
        .formal_proof
        .as_mut()
        .unwrap()
        .anchor
        .position += 1;
    assert_eq!(
        wrong_proof_anchor.validate().unwrap_err().to_string(),
        "formal result page request/head binding"
    );

    let mut with_minimum = page;
    with_minimum.target_request.min_commit = Some(commit(1));
    assert_eq!(
        with_minimum.validate().unwrap_err().to_string(),
        "formal result page request/head binding"
    );
}

#[test]
fn history_and_fixed_version_constraints_fail_closed() {
    let base = commit(7);
    for field in [
        "world",
        "branch",
        "reorg",
        "manifest",
        "authority",
        "permission",
        "provider",
        "finality",
    ] {
        let mut changed = commit(8);
        match field {
            "world" => changed.world.genesis_digest = "other".into(),
            "branch" => changed.binding.branch_id = "other".into(),
            "reorg" => changed.binding.reorg_generation += 1,
            "manifest" => changed.binding.governing_manifest_ref = "other".into(),
            "authority" => changed.binding.authority_generation += 1,
            "permission" => changed.binding.permission_generation += 1,
            "provider" => changed.binding.provider_world_id = "other".into(),
            _ => changed.binding.finality_ref = "other".into(),
        }
        assert!(changed.satisfies_minimum(&base).is_err(), "{field}");
    }
    let mut request = read_request();
    request.fixed_commit = Some(commit(6));
    assert!(request.validate().is_err());
    let mut conflicting = base.clone();
    conflicting.state_root_ref = "other".into();
    assert!(conflicting.satisfies_minimum(&base).is_err());
}

#[test]
fn changes_are_bounded_ordered_and_scope_era_bound() {
    let request = ReadWorldChangesRequest {
        contract_version: 1,
        cursor: cursor(10),
        max_items: 2,
        max_bytes: 4096,
    };
    let mut response = ReadWorldChangesResponse {
        formal_profile: None,
        execution_evidence_scope: None,
        contract_version: 1,
        changes: vec![WorldChange {
            cursor: cursor(11),
            change: json!({"effect":1}),
        }],
        next_cursor: cursor(11),
    };
    response.validate(&request).unwrap();
    response.changes[0].cursor.era = 4;
    assert!(response.validate(&request).is_err());
    response.changes[0].cursor = cursor(11);
    response.changes[0].cursor.scope_id = "other-user".into();
    assert!(response.validate(&request).is_err());
    response.changes[0].cursor = cursor(10);
    assert!(response.validate(&request).is_err());
    response.changes.clear();
    assert!(response.validate(&request).is_err());
    response.next_cursor = request.cursor.clone();
    response.validate(&request).unwrap();
    let zero = ReadWorldChangesRequest {
        max_bytes: 0,
        ..request
    };
    assert!(zero.validate().is_err());
}

#[test]
fn missing_and_empty_identity_never_become_committed_defaults() {
    assert!(serde_json::from_value::<WorldIdentity>(json!({"world_id":"world-a"})).is_err());
    let mut missing = world();
    missing.genesis_digest.clear();
    assert!(missing.validate().is_err());
    let mut binding = commit(7).binding;
    binding.governing_manifest_ref.clear();
    assert!(binding.validate().is_err());
}

fn commit_wire() -> Value {
    json!({"world":{"world_id":"world-a","genesis_digest":"genesis-a"},
        "binding":{"provider_world_id":"provider-a","branch_id":"branch-a",
        "finality_ref":"approved-contract-a","reorg_generation":0,
        "governing_manifest_ref":"manifest-a","authority_generation":1,"permission_generation":2},
        "position":7,"execution_block_hash":"block-7","state_root_ref":"root-7"})
}

fn cursor_wire(sequence: u64) -> Value {
    json!({"stream_id":"events","scope_id":"agent-a","era":3,
        "sequence":sequence,"commit":commit_wire()})
}

fn assert_wire<T>(value: &T, expected: Value)
where
    T: serde::Serialize + serde::de::DeserializeOwned + PartialEq + std::fmt::Debug,
{
    assert_eq!(serde_json::to_value(value).unwrap(), expected);
    assert_eq!(&serde_json::from_value::<T>(expected).unwrap(), value);
}

#[test]
fn describe_and_lookup_full_wire_samples_are_pinned() {
    let describe_request = DescribeWorldRequest {
        contract_version: 1,
        expected_world: world(),
        trust_config_ref: "trusted-config-a".into(),
    };
    describe_request.validate().unwrap();
    assert_wire(
        &describe_request,
        json!({"contract_version":1,
        "expected_world":{"world_id":"world-a","genesis_digest":"genesis-a"},
        "trust_config_ref":"trusted-config-a"}),
    );
    let projection = ProjectionVersion {
        commit: commit(7),
        projection_revision: "projection-v1".into(),
        visibility_scope: "agent-a".into(),
    };
    let describe = DescribeWorldResponse {
        formal_profile: None,
        execution_evidence_scope: None,
        contract_version: 1,
        world: world(),
        binding: commit(7).binding,
        capabilities: vec![
            Capability::StableLookup,
            Capability::CommittedView,
            Capability::BoundedChanges,
        ],
        availability: WorldAvailability {
            readable: true,
            writable: false,
            recovering: true,
            reason: Some(WorldServiceErrorKind::Unavailable),
            retry_after_ms: Some(250),
        },
        current_view: Some(projection),
    };
    describe.validate(&world()).unwrap();
    assert_wire(
        &describe,
        json!({"contract_version":1,"world":commit_wire()["world"],
        "binding":commit_wire()["binding"],"capabilities":["stable_lookup","committed_view","bounded_changes"],
        "availability":{"readable":true,"writable":false,"recovering":true,"reason":"unavailable","retry_after_ms":250},
        "current_view":{"commit":commit_wire(),"projection_revision":"projection-v1","visibility_scope":"agent-a"}}),
    );
    let lookup = LookupIntentRequest {
        contract_version: 1,
        key: correlation().key,
    };
    lookup.validate().unwrap();
    let key = json!({"world":{"world_id":"world-a","genesis_digest":"genesis-a"},
        "verified_subject":"key-a","operation_domain":"gameplay","nonce_scope":"player-key","request_id_or_nonce":"12"});
    assert_wire(&lookup, json!({"contract_version":1,"key":key}));
    let response = IntentResponse {
        terminal_recovery_authority: None,
        formal_profile: None,
        execution_evidence_scope: None,
        contract_version: 1,
        correlation: correlation(),
        outcome: IntentOutcome::Committed {
            commit: commit(7).into(),
            receipt: json!({"receipt_id":"receipt-a"}),
        },
    };
    response.validate(&correlation()).unwrap();
    assert_wire(
        &response,
        json!({"contract_version":1,"correlation":{"key":key,"payload_digest":"digest-a"},
        "outcome":{"status":"committed","commit":commit_wire(),"receipt":{"receipt_id":"receipt-a"}}}),
    );
}

#[test]
fn view_and_changes_full_wire_samples_are_pinned() {
    assert_wire(
        &read_request(),
        json!({"contract_version":1,"world":commit_wire()["world"],
        "scope_id":"agent-a","min_commit":commit_wire(),"fixed_commit":null,"deadline_unix_ms":null}),
    );
    let view = ReadWorldViewResponse {
        formal_profile: None,
        formal_proof: None,
        execution_evidence_scope: None,
        contract_version: 1,
        version: ProjectionVersion {
            commit: commit(7),
            projection_revision: "projection-v1".into(),
            visibility_scope: "agent-a".into(),
        },
        logical_tick: 900,
        continuation: cursor(1200),
        view: json!({"energy":10}),
    };
    view.validate(&read_request()).unwrap();
    assert_wire(
        &view,
        json!({"contract_version":1,"version":{"commit":commit_wire(),
        "projection_revision":"projection-v1","visibility_scope":"agent-a"},"logical_tick":900,
        "continuation":cursor_wire(1200),"view":{"energy":10}}),
    );
    let request = ReadWorldChangesRequest {
        contract_version: 1,
        cursor: cursor(10),
        max_items: 1,
        max_bytes: 4096,
    };
    assert_wire(
        &request,
        json!({"contract_version":1,"cursor":cursor_wire(10),"max_items":1,"max_bytes":4096}),
    );
    let response = ReadWorldChangesResponse {
        formal_profile: None,
        execution_evidence_scope: None,
        contract_version: 1,
        changes: vec![WorldChange {
            cursor: cursor(11),
            change: json!({"effect":1}),
        }],
        next_cursor: cursor(11),
    };
    response.validate(&request).unwrap();
    assert_wire(
        &response,
        json!({"contract_version":1,"changes":[{"cursor":cursor_wire(11),"change":{"effect":1}}],"next_cursor":cursor_wire(11)}),
    );
}

#[test]
fn every_operation_rejects_unsupported_request_and_response_versions() {
    for version in [0, 2, u32::MAX] {
        assert!(
            DescribeWorldRequest {
                contract_version: version,
                expected_world: world(),
                trust_config_ref: "trusted".into()
            }
            .validate()
            .is_err()
        );
        assert!(
            SubmitIntentRequest {
                contract_version: version,
                correlation: correlation(),
                deadline_unix_ms: None,
                signed_payload: vec![1_u8]
            }
            .validate()
            .is_err()
        );
        assert!(
            LookupIntentRequest {
                contract_version: version,
                key: correlation().key
            }
            .validate()
            .is_err()
        );
        assert!(
            ReadWorldViewRequest {
                formal_anchor: None,
                contract_version: version,
                ..read_request()
            }
            .validate()
            .is_err()
        );
        let changes_request = ReadWorldChangesRequest {
            contract_version: 1,
            cursor: cursor(10),
            max_items: 1,
            max_bytes: 4096,
        };
        assert!(
            ReadWorldChangesRequest {
                contract_version: version,
                ..changes_request.clone()
            }
            .validate()
            .is_err()
        );
        assert!(
            DescribeWorldResponse {
                formal_profile: None,
                execution_evidence_scope: None,
                contract_version: version,
                world: world(),
                binding: commit(7).binding,
                capabilities: vec![],
                availability: WorldAvailability {
                    readable: true,
                    writable: true,
                    recovering: false,
                    reason: None,
                    retry_after_ms: None
                },
                current_view: None
            }
            .validate(&world())
            .is_err()
        );
        // Submit and Lookup share the same response contract.
        assert!(
            IntentResponse {
                terminal_recovery_authority: None,
                formal_profile: None,
                execution_evidence_scope: None,
                contract_version: version,
                correlation: correlation(),
                outcome: IntentOutcome::<Value>::Pending
            }
            .validate(&correlation())
            .is_err()
        );
        assert!(
            ReadWorldViewResponse {
                formal_profile: None,
                formal_proof: None,
                execution_evidence_scope: None,
                contract_version: version,
                version: ProjectionVersion {
                    commit: commit(7),
                    projection_revision: "projection-v1".into(),
                    visibility_scope: "agent-a".into()
                },
                logical_tick: 900,
                continuation: cursor(11),
                view: json!({})
            }
            .validate(&read_request())
            .is_err()
        );
        assert!(
            ReadWorldChangesResponse::<Value> {
                formal_profile: None,
                execution_evidence_scope: None,
                contract_version: version,
                changes: vec![],
                next_cursor: cursor(10)
            }
            .validate(&changes_request)
            .is_err()
        );
    }
}

#[test]
fn changes_reject_over_max_items_with_otherwise_valid_continuation() {
    let request = ReadWorldChangesRequest {
        contract_version: 1,
        cursor: cursor(10),
        max_items: 2,
        max_bytes: 4096,
    };
    let mut response = ReadWorldChangesResponse {
        formal_profile: None,
        execution_evidence_scope: None,
        contract_version: 1,
        changes: (11..=13)
            .map(|sequence| WorldChange {
                cursor: cursor(sequence),
                change: json!({"effect":sequence}),
            })
            .collect(),
        next_cursor: cursor(13),
    };
    assert_eq!(
        response.validate(&request),
        Err(ContractError("changes item limit exceeded"))
    );
    response.changes.pop();
    response.next_cursor = cursor(12);
    response.validate(&request).unwrap();
}

fn terminal_authority_fixture() -> Value {
    let digest = format!("blake3:{}", "11".repeat(32));
    json!({
        "schema_version":1,"role":"terminal_only","world":world(),"agent_id":"agent-a","owner_binding":"owner-a","identity_generation":1,
        "agent_session_id":"session-a","agent_turn_id":"turn-a","decision_request_id":"request-a","request_digest":digest,
        "retry_seq":1,"transport_attempt":1,"grant_id":"grant-a","grant_issued_at_tick":2,"grant_expires_at_tick":302,
        "lease":{"lease_id":"lease-a","idempotency_key":"key-a","account_id":"owner-a","quote_digest":digest,"reserved_amount":1,"reserved_at_tick":224},
        "prefix_correlation":correlation(),"prefix_result_digest":digest,"prefix_action_id":1,"prefix_deadline_unix_ms":null,"prefix_committed_height":227,
        "prefix_commit":commit(227),"admitted_at_tick":224,"writer_public_key":"key-a","delegation_generation":1,
        "current_runtime_binding":{"world_id":"world-a","branch_id":"branch-a","finality_epoch":0,"finality_block_hash":digest,
            "finality_status":"verified","base_tick":2052,"base_world_hash":digest,"reorg_epoch":0,"runtime_manifest_hash":digest},
        "current_task_fence":null,"read_commit":commit(2052)
    })
}

#[test]
fn terminal_authority_rejects_nested_unknown_fields_without_changing_legacy_readers() {
    let digest = format!("blake3:{}", "11".repeat(32));
    let value = terminal_authority_fixture();
    let proof: TerminalRecoveryAuthorityV1 = serde_json::from_value(value.clone()).unwrap();
    proof.validate(&correlation()).unwrap();
    // The former inline field is an independent serialization oracle. Boxing
    // must preserve both old None bytes and the complete signed Some payload.
    #[derive(serde::Serialize)]
    struct InlineTerminalResponse<'a> {
        #[serde(skip_serializing_if = "Option::is_none")]
        terminal_recovery_authority: Option<&'a TerminalRecoveryAuthorityV1>,
        #[serde(skip_serializing_if = "Option::is_none")]
        formal_profile: Option<FormalReadProfileV1>,
        #[serde(skip_serializing_if = "Option::is_none")]
        execution_evidence_scope: Option<String>,
        contract_version: u32,
        correlation: RequestCorrelation,
        outcome: IntentOutcome<Value>,
    }
    for terminal in [None, Some(&proof)] {
        let outcome = IntentOutcome::Committed {
            commit: Box::new(commit(227)),
            receipt: json!({"dispatched":true}),
        };
        let inline = InlineTerminalResponse {
            terminal_recovery_authority: terminal,
            formal_profile: None,
            execution_evidence_scope: None,
            contract_version: 1,
            correlation: correlation(),
            outcome: outcome.clone(),
        };
        let boxed = IntentResponse {
            terminal_recovery_authority: terminal.cloned().map(Box::new),
            formal_profile: None,
            execution_evidence_scope: None,
            contract_version: 1,
            correlation: correlation(),
            outcome,
        };
        let old_bytes = serde_json::to_vec(&inline).unwrap();
        assert_eq!(old_bytes, serde_json::to_vec(&boxed).unwrap());
        assert_eq!(
            serde_json::from_slice::<IntentResponse<Value>>(&old_bytes).unwrap(),
            boxed
        );
        assert_eq!(
            signing_bytes(LOOKUP_PATH, &(&digest, &inline)).unwrap(),
            signing_bytes(LOOKUP_PATH, &(&digest, &boxed)).unwrap(),
            "boxing must preserve the entire service-signature canonical payload",
        );
    }
    for pointer in [
        "",
        "/lease",
        "/world",
        "/prefix_correlation",
        "/prefix_correlation/key",
        "/prefix_correlation/key/world",
        "/prefix_commit",
        "/prefix_commit/world",
        "/prefix_commit/binding",
        "/read_commit",
        "/read_commit/binding",
        "/current_runtime_binding",
    ] {
        let mut altered = value.clone();
        altered
            .pointer_mut(pointer)
            .unwrap()
            .as_object_mut()
            .unwrap()
            .insert("unknown_authority".into(), json!(true));
        assert!(
            serde_json::from_value::<TerminalRecoveryAuthorityV1>(altered).is_err(),
            "unknown nested field at {pointer}"
        );
    }
    let mut legacy = serde_json::to_value(commit(227)).unwrap();
    legacy["future_optional"] = json!(true);
    assert!(
        serde_json::from_value::<CommitRef>(legacy).is_ok(),
        "old DTO compatibility remains independent"
    );
}

#[path = "support/world_service_terminal_contract.rs"]
mod terminal_current_contract;
